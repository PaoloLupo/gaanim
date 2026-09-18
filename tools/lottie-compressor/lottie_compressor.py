"""Standalone, lossless Lottie minifier and dotLottie packer (Python 3.10+)."""

from __future__ import annotations

import argparse
import hashlib
import io
import json
import os
from pathlib import Path, PurePosixPath
import re
import sys
import tempfile
from urllib.parse import unquote, urlsplit
import zipfile
import zlib

MAX_BYTES = 128 * 1024 * 1024
MAX_ENTRIES = 4096
ID_PATTERN = re.compile(r"[a-zA-Z0-9._ \-]+")


class CompressorError(ValueError):
    """An input cannot be processed safely or is not supported."""


class Number(str):
    """Keep the original JSON number token, including arbitrary precision."""


def _reject_constant(value):
    raise CompressorError(f"Número JSON inválido: {value}")


def _unique_object(pairs):
    result = {}
    for key, value in pairs:
        if key in result:
            raise CompressorError(f"Clave JSON duplicada: {key}")
        result[key] = value
    return result


def decode_json(data: bytes):
    try:
        return json.loads(
            data.decode("utf-8-sig"), parse_int=Number, parse_float=Number,
            parse_constant=_reject_constant, object_pairs_hook=_unique_object,
        )
    except (UnicodeError, json.JSONDecodeError) as exc:
        raise CompressorError(f"JSON UTF-8 inválido: {exc}") from exc


def encode_json(value) -> bytes:
    def encode(item):
        if isinstance(item, Number):
            return str(item)
        if isinstance(item, dict):
            return "{" + ",".join(encode(k) + ":" + encode(v) for k, v in item.items()) + "}"
        if isinstance(item, list):
            return "[" + ",".join(map(encode, item)) + "]"
        # ASCII escaping also preserves unpaired surrogate escapes accepted by JSON.
        return json.dumps(item, ensure_ascii=True, allow_nan=False, separators=(",", ":"))

    return encode(value).encode("utf-8")


def minify_json(data: bytes) -> bytes:
    decode_json(data)
    # Strip only insignificant whitespace: strings, escapes and numbers stay exact.
    source = data.decode("utf-8-sig")
    result = []
    quoted = escaped = False
    for char in source:
        if quoted:
            result.append(char)
            if escaped:
                escaped = False
            elif char == "\\":
                escaped = True
            elif char == '"':
                quoted = False
        elif char == '"':
            quoted = True
            result.append(char)
        elif char not in " \t\r\n":
            result.append(char)
    return "".join(result).encode("utf-8")


def validate_animation(value):
    if not isinstance(value, dict) or not isinstance(value.get("layers"), list):
        raise CompressorError("Se esperaba una animación Lottie con una lista 'layers'.")
    for field in ("w", "h", "fr", "ip", "op"):
        if not isinstance(value.get(field), Number):
            raise CompressorError(f"La animación necesita el campo numérico '{field}'.")


def read_file(path: Path) -> bytes:
    with path.open("rb") as stream:
        data = stream.read(MAX_BYTES + 1)
    if len(data) > MAX_BYTES:
        raise CompressorError("El archivo supera el límite de 128 MiB.")
    return data


def make_archive(entries: dict[str, bytes], level: int = 9) -> bytes:
    if not 0 <= level <= 9:
        raise CompressorError("El nivel de compresión debe estar entre 0 y 9.")
    if len(entries) > MAX_ENTRIES or sum(map(len, entries.values())) > MAX_BYTES:
        raise CompressorError("El paquete supera 4096 entradas o 128 MiB descomprimidos.")
    buffer = io.BytesIO()
    with zipfile.ZipFile(buffer, "w", compression=zipfile.ZIP_DEFLATED) as archive:
        for name in sorted(entries, key=lambda n: (n != "manifest.json", n)):
            info = zipfile.ZipInfo(name, date_time=(1980, 1, 1, 0, 0, 0))
            info.create_system = 3
            info.external_attr = 0o100644 << 16
            archive.writestr(info, entries[name], compress_type=zipfile.ZIP_DEFLATED,
                             compresslevel=level)
    return buffer.getvalue()


def _local_resource(reference: str, base: Path) -> Path | None:
    parsed = urlsplit(reference)
    if parsed.scheme in ("data", "http", "https") or reference.startswith("//"):
        return None
    if parsed.scheme or parsed.query or parsed.fragment or "\\" in reference:
        raise CompressorError(f"Ruta de recurso no admitida: {reference}")
    path = (base / unquote(parsed.path)).resolve()
    if not path.is_relative_to(base.resolve()):
        raise CompressorError(f"El recurso está fuera de la carpeta de entrada: {reference}")
    return path


def pack_json(data: bytes, base: Path, animation_id="animation", level=9) -> bytes:
    if not isinstance(animation_id, str) or not ID_PATTERN.fullmatch(animation_id):
        raise CompressorError("El ID solo admite letras ASCII, números, espacios, '.', '_' y '-'.")
    animation = decode_json(data)
    validate_animation(animation)
    entries = {}
    assets = animation.get("assets", [])
    if not isinstance(assets, list):
        raise CompressorError("'assets' debe ser una lista.")
    changed = False
    for asset in assets:
        if not isinstance(asset, dict):
            raise CompressorError("Cada recurso debe ser un objeto JSON.")
        if "p" not in asset:
            continue
        prefix, name = asset.get("u", ""), asset["p"]
        if not isinstance(prefix, str) or not isinstance(name, str):
            raise CompressorError("Las rutas 'u' y 'p' deben ser cadenas.")
        # A complete URI in p is independent of its optional u prefix.
        reference = name if urlsplit(name).scheme else prefix + name
        path = _local_resource(reference, base)
        if path is None:
            continue
        content = read_file(path)
        suffix = path.suffix.lower()
        if not re.fullmatch(r"\.[a-z0-9]+", suffix):
            raise CompressorError(f"El recurso necesita una extensión válida: {path.name}")
        packaged_name = hashlib.sha256(content).hexdigest() + suffix
        entries["i/" + packaged_name] = content
        if sum(map(len, entries.values())) > MAX_BYTES:
            raise CompressorError("Los recursos superan 128 MiB.")
        asset.update(u="../i/", p=packaged_name, e=0)
        changed = True
    fonts = animation.get("fonts", {})
    if not isinstance(fonts, dict) or not isinstance(fonts.get("list", []), list):
        raise CompressorError("'fonts.list' debe ser una lista.")
    for font in fonts.get("list", []):
        if not isinstance(font, dict):
            raise CompressorError("Cada fuente debe ser un objeto JSON.")
        reference = font.get("fPath")
        if reference:
            if not isinstance(reference, str):
                raise CompressorError("'fPath' debe ser una cadena.")
            if _local_resource(reference, base) is not None:
                raise CompressorError("Fuentes locales no admitidas: incrusta glifos o usa una URL en fPath.")
    entries[f"a/{animation_id}.json"] = encode_json(animation) if changed else minify_json(data)
    entries["manifest.json"] = encode_json({
        "version": "2", "animations": [{"id": animation_id}],
        "initial": {"animation": animation_id},
    })
    return make_archive(entries, level)


def minify_archive(data: bytes, level=9) -> bytes:
    entries = {}
    with zipfile.ZipFile(io.BytesIO(data)) as archive:
        infos = archive.infolist()
        if len(infos) > MAX_ENTRIES or sum(i.file_size for i in infos) > MAX_BYTES:
            raise CompressorError("El ZIP supera 4096 entradas o 128 MiB descomprimidos.")
        seen = set()
        for info in infos:
            name = info.filename
            path = PurePosixPath(name)
            if (not name or path.is_absolute() or ".." in path.parts
                    or "\\" in name or ":" in name or "\x00" in name
                    or name.rstrip("/") != path.as_posix()):
                raise CompressorError(f"Ruta ZIP no admitida: {name}")
            if name in seen:
                raise CompressorError(f"Entrada ZIP duplicada: {name}")
            seen.add(name)
            if info.flag_bits & 1:
                raise CompressorError("No se admiten archivos ZIP cifrados.")
            if info.compress_type not in (zipfile.ZIP_STORED, zipfile.ZIP_DEFLATED):
                raise CompressorError("Solo se admiten entradas ZIP Store o Deflate.")
            if info.is_dir():
                continue
            with archive.open(info) as stream:
                content = stream.read(MAX_BYTES + 1)
            if len(content) > MAX_BYTES:
                raise CompressorError("Entrada ZIP demasiado grande.")
            entries[name] = minify_json(content) if name.lower().endswith(".json") else content
    if "manifest.json" not in entries:
        raise CompressorError("El archivo .lottie no contiene manifest.json.")
    manifest = decode_json(entries["manifest.json"])
    if not isinstance(manifest, dict):
        raise CompressorError("El manifiesto debe ser un objeto JSON.")
    version = manifest.get("version")
    if type(version) is not str or version not in ("1.0", "2"):
        raise CompressorError("Solo se admiten manifiestos dotLottie 1.0 y 2.")
    animations = manifest.get("animations")
    if not isinstance(animations, list) or not animations:
        raise CompressorError("El manifiesto debe declarar al menos una animación.")
    ids = set()
    for animation in animations:
        identifier = animation.get("id") if isinstance(animation, dict) else None
        if type(identifier) is not str or not ID_PATTERN.fullmatch(identifier) or identifier in ids:
            raise CompressorError("El manifiesto contiene un ID inválido o duplicado.")
        ids.add(identifier)
        name = f"{'a' if version == '2' else 'animations'}/{identifier}.json"
        if name not in entries:
            raise CompressorError(f"Falta la animación declarada: {name}")
        validate_animation(decode_json(entries[name]))
    result = make_archive(entries, level)
    # Already optimized archives must not grow. Store entries get converted to Deflate.
    all_deflate = all(i.is_dir() or i.compress_type == zipfile.ZIP_DEFLATED for i in infos)
    return data if all_deflate and len(data) <= len(result) else result


def write_output(path: Path, data: bytes, force: bool):
    # Construct the complete result before publishing it; do not clobber by default.
    with tempfile.NamedTemporaryFile(dir=path.parent, prefix=".lottie-", delete=False) as stream:
        temporary = Path(stream.name)
        try:
            stream.write(data)
        except BaseException:
            stream.close()
            temporary.unlink(missing_ok=True)
            raise
    try:
        if force:
            os.replace(temporary, path)
        else:
            os.link(temporary, path)
    finally:
        temporary.unlink(missing_ok=True)


def main(argv=None) -> int:
    parser = argparse.ArgumentParser(description="Comprime y minifica Lottie sin pérdida de datos.")
    commands = parser.add_subparsers(dest="command", required=True)
    for command, help_text in (("pack", "Convierte JSON a dotLottie v2"),
                               ("minify", "Minifica JSON o recomprime dotLottie v1/v2")):
        sub = commands.add_parser(command, help=help_text)
        sub.add_argument("input", type=Path)
        sub.add_argument("-o", "--output", type=Path)
        sub.add_argument("--level", type=int, choices=range(10), default=9)
        sub.add_argument("--force", action="store_true", help="Reemplaza una salida existente")
        if command == "pack":
            sub.add_argument("--id", default="animation", help="ID interno de la animación")
    args = parser.parse_args(argv)
    try:
        source = args.input.resolve()
        extension = source.suffix.lower()
        if extension not in (".json", ".lottie") or (args.command == "pack" and extension != ".json"):
            raise CompressorError("pack necesita .json; minify admite .json o .lottie.")
        default = source.with_suffix(".lottie") if args.command == "pack" else source.with_name(source.stem + ".min" + extension)
        output = (args.output or default).resolve()
        if output == source or (output.exists() and os.path.samefile(source, output)):
            raise CompressorError("La salida debe ser distinta de la entrada.")
        expected = ".lottie" if args.command == "pack" else extension
        if output.suffix.lower() != expected:
            raise CompressorError(f"La salida necesita la extensión {expected}.")
        if output.exists() and not args.force:
            raise CompressorError("La salida ya existe; usa --force para reemplazarla.")
        data = read_file(source)
        if args.command == "pack":
            result = pack_json(data, source.parent, args.id, args.level)
        elif extension == ".lottie":
            result = minify_archive(data, args.level)
        else:
            validate_animation(decode_json(data))
            result = minify_json(data)
        write_output(output, result, args.force)
        reduction = (1 - len(result) / len(data)) * 100
        print(f"{output}\n{len(data):,} -> {len(result):,} bytes | reducción: {reduction:.2f}%")
        if reduction < 0:
            print("El contenedor y sus recursos ocupan más que el JSON de entrada.")
        return 0
    except (OSError, ValueError, zipfile.BadZipFile, zlib.error, RuntimeError, NotImplementedError,
            RecursionError) as exc:
        print(f"Error: {exc}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
