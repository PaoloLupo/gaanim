#!/usr/bin/env python3
"""Build the Gaanim web playground into a static folder.

The playground runs scripts in Pyodide and plays their scene in the web
player, so it needs no server beyond static files:

- `gaanim_pyodide` compiled for wasm32-unknown-emscripten with the Emscripten
  version of the Pyodide release the page loads, packaged with the `gaanim`
  package's Python files as a Pyodide wheel;
- `stubs.zip`, the package's type stubs, for the editor's completions;
- `examples.json`, the documentation's example scenes;
- the page itself (`crates/gaanim_pyodide/web`).

The page embeds the web player from `--player` (a URL relative to the
playground). `--with-player` also builds the player into `<out>/reproductor`
so the folder works on its own: serve it with any static server, for example
`python -m http.server -d dist/playground`.

Emscripten comes from `$EMSDK` (or `~/emsdk`), activated at the version below.

Usage: python scripts/build_playground.py [--out DIR] [--with-player]
       [--player URL] [--profile PROFILE] [--skip-extension]
"""

from __future__ import annotations

import argparse
import base64
import hashlib
import json
import os
import re
import shutil
import subprocess
import sys
import zipfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
TARGET = "wasm32-unknown-emscripten"
# Pyodide 314.x: Python 3.14 on Emscripten 5.0.3 (platform pyemscripten_2026_0).
PYODIDE_VERSION = "314.0.7"
EMSCRIPTEN_VERSION = "5.0.3"
WHEEL_TAG = "cp314-abi3-pyemscripten_2026_0_wasm32"
PACKAGE = ROOT / "crates" / "gaanim_python" / "gaanim"
WEB = ROOT / "crates" / "gaanim_pyodide" / "web"
PAGE_FILES = (
    "index.html",
    "playground.css",
    "main.js",
    "language.js",
    "runner.js",
    "language.py",
    "runner.py",
)
ICONS = ("favicon.ico", "gaanim-icon-512.png")
EXAMPLES = ROOT / "docs" / "content" / "ejemplos"


def crate_version() -> str:
    manifest = (ROOT / "crates" / "gaanim_pyodide" / "Cargo.toml").read_text(encoding="utf-8")
    return re.search(r'^version = "([^"]+)"', manifest, re.MULTILINE).group(1)


def emscripten_env() -> dict[str, str]:
    """The environment that puts Pyodide's Emscripten on PATH for Cargo."""
    emsdk = Path(os.environ.get("EMSDK") or Path.home() / "emsdk")
    emscripten = emsdk / "upstream" / "emscripten"
    if not emscripten.is_dir():
        raise SystemExit(
            f"Emscripten not found in {emsdk}: install it with "
            f"`emsdk install {EMSCRIPTEN_VERSION} && emsdk activate {EMSCRIPTEN_VERSION}` "
            "and set EMSDK"
        )
    env = dict(os.environ)
    paths = [str(emscripten)]
    paths += [str(node / "bin") for node in sorted((emsdk / "node").glob("*"))]
    env["PATH"] = os.pathsep.join(paths + [env.get("PATH", "")])
    env["EMSDK"] = str(emsdk)
    config = emsdk / ".emscripten"
    if config.exists():
        env["EM_CONFIG"] = str(config)
    emcc = "emcc.bat" if os.name == "nt" else "emcc"
    try:
        version = subprocess.run(
            [emcc, "--version"], env=env, check=True, capture_output=True, text=True, shell=os.name == "nt"
        ).stdout
    except (OSError, subprocess.CalledProcessError) as error:
        raise SystemExit(f"could not run emcc: {error}")
    if f" {EMSCRIPTEN_VERSION} " not in version.splitlines()[0]:
        raise SystemExit(
            f"Pyodide {PYODIDE_VERSION} needs Emscripten {EMSCRIPTEN_VERSION}, found: "
            f"{version.splitlines()[0]}"
        )
    # PyO3 builds an extension module against Python 3.14's stable ABI; the
    # interpreter's symbols resolve when Pyodide loads it.
    env["PYO3_BUILD_EXTENSION_MODULE"] = "1"
    env["PYO3_CROSS_PYTHON_VERSION"] = "3.14"
    return env


def build_extension(profile: str) -> Path:
    subprocess.run(
        ["cargo", "build", "-p", "gaanim_pyodide", "--target", TARGET, "--profile", profile],
        cwd=ROOT,
        env=emscripten_env(),
        check=True,
    )
    return extension_path(profile)


def extension_path(profile: str) -> Path:
    profile_dir = "debug" if profile == "dev" else profile
    module = ROOT / "target" / TARGET / profile_dir / "gaanim_pyodide.wasm"
    if not module.exists():
        raise SystemExit(f"{module} is missing; build without --skip-extension")
    return module


def package_files(*suffixes: str) -> list[Path]:
    return sorted(
        path for path in PACKAGE.iterdir() if path.is_file() and path.suffix in suffixes
    )


def content_hash(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()[:12]


def write_wheel(out: Path, module: Path, version: str) -> str:
    """A Pyodide wheel of the `gaanim` package with its native module.

    It goes in a folder named after its content, so a new build is never
    served from a cache; returns its path relative to `out`.
    """
    name = f"gaanim-{version}-{WHEEL_TAG}.whl"
    dist_info = f"gaanim-{version}.dist-info"
    entries: list[tuple[str, bytes]] = [
        (f"gaanim/{path.name}", path.read_bytes()) for path in package_files(".py")
    ]
    entries.append(("gaanim/gaanim_core.abi3.so", module.read_bytes()))
    entries.append(
        (
            f"{dist_info}/METADATA",
            f"Metadata-Version: 2.1\nName: gaanim\nVersion: {version}\n"
            "Summary: Gaanim for the web playground (Pyodide)\n".encode(),
        )
    )
    entries.append(
        (
            f"{dist_info}/WHEEL",
            "Wheel-Version: 1.0\nGenerator: gaanim build_playground.py\n"
            f"Root-Is-Purelib: false\nTag: {WHEEL_TAG}\n".encode(),
        )
    )
    record = []
    for path, data in entries:
        digest = base64.urlsafe_b64encode(hashlib.sha256(data).digest()).rstrip(b"=").decode()
        record.append(f"{path},sha256={digest},{len(data)}")
    record.append(f"{dist_info}/RECORD,,")
    entries.append((f"{dist_info}/RECORD", ("\n".join(record) + "\n").encode()))
    built = out / name
    with zipfile.ZipFile(built, "w", zipfile.ZIP_DEFLATED, compresslevel=9) as wheel:
        for path, data in entries:
            wheel.writestr(path, data)
    folder = out / "pkg" / content_hash(built)
    folder.mkdir(parents=True)
    built.rename(folder / name)
    return (folder / name).relative_to(out).as_posix()


def write_stubs(out: Path) -> None:
    """The package's stubs and helpers, which Jedi reads for the editor."""
    with zipfile.ZipFile(out / "stubs.zip", "w", zipfile.ZIP_DEFLATED, compresslevel=9) as stubs:
        for path in package_files(".py", ".pyi", ".typed"):
            stubs.write(path, f"gaanim/{path.name}")


def examples() -> list[dict[str, str]]:
    """The documentation's example scenes, titled as its catalog titles them."""
    catalog = (EXAMPLES / "escenas.typ").read_text(encoding="utf-8")
    titles = dict(re.findall(r'cell: "(\w+)".*?title: "([^"]+)"', catalog, re.DOTALL))
    found = [
        {
            "name": "quickstart",
            "title": "Hola, Gaanim",
            "code": (ROOT / "examples" / "quickstart.py").read_text(encoding="utf-8"),
        }
    ]
    for source in ("basicos.py", "avanzados.py"):
        text = (EXAMPLES / source).read_text(encoding="utf-8")
        for chunk in re.split(r"(?m)^# %% ", text)[1:]:
            cell, _, code = chunk.partition("\n")
            cell = cell.strip()
            found.append(
                {
                    "name": cell,
                    "title": titles.get(cell, cell.replace("_", " ").capitalize()),
                    "code": code.strip() + "\n",
                }
            )
    return found


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--out", type=Path, default=ROOT / "dist" / "playground")
    parser.add_argument("--profile", default="playground")
    parser.add_argument("--player", help="web player URL, relative to the playground")
    parser.add_argument(
        "--with-player", action="store_true", help="also build the web player into <out>/reproductor"
    )
    parser.add_argument(
        "--skip-extension", action="store_true", help="reuse the last extension build"
    )
    args = parser.parse_args()

    version = crate_version()
    module = extension_path(args.profile) if args.skip_extension else build_extension(args.profile)

    out = args.out
    player_dir = out / "reproductor"
    keep_player = args.with_player is False and player_dir.is_dir()
    for entry in out.glob("*") if out.is_dir() else []:
        if entry == player_dir and keep_player:
            continue
        shutil.rmtree(entry) if entry.is_dir() else entry.unlink()
    out.mkdir(parents=True, exist_ok=True)

    wheel = write_wheel(out, module, version)
    write_stubs(out)
    (out / "examples.json").write_text(
        json.dumps(examples(), ensure_ascii=False, indent=1), encoding="utf-8"
    )
    for name in PAGE_FILES:
        shutil.copy(WEB / name, out / name)
    for name in ICONS:
        shutil.copy(ROOT / "docs" / "assets" / "brand" / name, out / name)

    if args.with_player:
        subprocess.run(
            [sys.executable, str(ROOT / "scripts" / "build_web.py"), "--out", str(player_dir)],
            check=True,
        )
    player = args.player or ("reproductor/" if player_dir.is_dir() else "../reproductor/")
    config = {
        "version": version,
        "pyodide": f"https://cdn.jsdelivr.net/pyodide/v{PYODIDE_VERSION}/full/",
        "wheel": wheel,
        "stubs": f"stubs.zip?v={content_hash(out / 'stubs.zip')}",
        "examples": "examples.json",
        "player": player,
    }
    (out / "config.json").write_text(json.dumps(config, indent=1), encoding="utf-8")
    size = (out / wheel).stat().st_size
    print(
        f"playground: {out} (extension {module.stat().st_size / 1e6:.1f} MB, "
        f"wheel {size / 1e6:.1f} MB; player at {player})"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
