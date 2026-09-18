import contextlib
import io
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch
import zipfile

import lottie_compressor as lc


def animation(**extra):
    return dict(v="5.7.4", w=200, h=200, fr=30, ip=0, op=60, layers=[], **extra)


def pretty(value):
    return json.dumps(value, indent=4, ensure_ascii=False).encode()


def unzip(data):
    with zipfile.ZipFile(io.BytesIO(data)) as archive:
        return {name: archive.read(name) for name in archive.namelist()}


class CompressorTests(unittest.TestCase):
    def test_minifier_preserves_number_tokens_strings_and_unicode(self):
        source = ' { "n": 0.12345678901234567890123456789, "i": 9007199254740993, "e": 1e-400, "z": -0, "s": "niño \\\" \\n \\\\" } '.encode()
        result = lc.minify_json(source)
        self.assertEqual(lc.decode_json(result), lc.decode_json(source))
        for token in (b"0.12345678901234567890123456789", b"9007199254740993", b"1e-400", b"-0"):
            self.assertIn(token, result)
        self.assertLess(len(result), len(source))
        self.assertEqual(lc.minify_json(result), result)

    def test_invalid_json_is_rejected(self):
        for source in (b'{"x":NaN}', b'{"x":Infinity}', b'{"x":1,"x":2}', b'{broken', b'\xff'):
            with self.subTest(source=source), self.assertRaises(lc.CompressorError):
                lc.minify_json(source)

    def test_pack_is_deterministic_and_preserves_animation(self):
        data = pretty(animation(nm="Animación", markers=[dict(cm="start", tm=0, dr=1)]))
        packed = lc.pack_json(data, Path.cwd(), "demo")
        self.assertEqual(packed, lc.pack_json(data, Path.cwd(), "demo"))
        entries = unzip(packed)
        self.assertEqual(set(entries), {"manifest.json", "a/demo.json"})
        self.assertEqual(json.loads(entries["a/demo.json"]), json.loads(data))
        self.assertEqual(json.loads(entries["manifest.json"])["version"], "2")
        with zipfile.ZipFile(io.BytesIO(packed)) as archive:
            self.assertIsNone(archive.testzip())
            self.assertTrue(all(i.compress_type == zipfile.ZIP_DEFLATED for i in archive.infolist()))

    def test_local_images_are_bundled_deduplicated_and_precision_preserved(self):
        with tempfile.TemporaryDirectory() as directory:
            base = Path(directory)
            (base / "images").mkdir()
            (base / "images" / "pixel.png").write_bytes(b"image bytes")
            data = pretty(animation(assets=[
                dict(id="one", u="images/", p="pixel.png"),
                dict(id="two", u="images/", p="pixel.png"),
            ]))
            data = data.replace(b'"fr": 30', b'"fr": 30.1234567890123456789012345')
            entries = unzip(lc.pack_json(data, base))
            images = [name for name in entries if name.startswith("i/")]
            self.assertEqual(len(images), 1)
            self.assertEqual(entries[images[0]], b"image bytes")
            result = entries["a/animation.json"]
            self.assertIn(b"30.1234567890123456789012345", result)
            assets = json.loads(result)["assets"]
            self.assertEqual(assets[0]["p"], assets[1]["p"])
            self.assertEqual(assets[0]["u"], "../i/")
            self.assertEqual(assets[0]["e"], 0)

    def test_data_and_remote_uris_remain_intact(self):
        data = pretty(animation(assets=[dict(p="data:image/png;base64,AA==", e=1),
                                        dict(u="https://example.org/", p="pixel.png")]))
        entries = unzip(lc.pack_json(data, Path.cwd()))
        self.assertEqual(json.loads(entries["a/animation.json"]), json.loads(data))

    def test_local_paths_cannot_escape_source_folder(self):
        for reference in ("../secret.png", "%2e%2e/secret.png", "C:\\secret.png"):
            with self.subTest(reference=reference), self.assertRaises(lc.CompressorError):
                lc.pack_json(pretty(animation(assets=[dict(p=reference)])), Path.cwd())

    def test_missing_image_and_local_font_report_errors(self):
        with tempfile.TemporaryDirectory() as directory:
            with self.assertRaises(FileNotFoundError):
                lc.pack_json(pretty(animation(assets=[dict(p="missing.png")])), Path(directory))
            with self.assertRaisesRegex(lc.CompressorError, "Fuentes locales"):
                lc.pack_json(pretty(animation(fonts={"list": [dict(fPath="font.ttf")]})), Path(directory))

    def test_v1_v2_minification_preserves_every_resource(self):
        for version, folder in (("1.0", "animations"), ("2", "a")):
            with self.subTest(version=version):
                entries = {
                    "manifest.json": pretty(dict(version=version, animations=[dict(id="first"), dict(id="second")])),
                    f"{folder}/first.json": pretty(animation()),
                    f"{folder}/second.json": pretty(animation(nm="second")),
                    "t/theme.json": pretty(dict(rules=[dict(id="color", value=[1, 0, 0])])),
                    "s/machine.json": pretty(dict(initial="idle", states=[])),
                    "i/image.png": bytes(range(256)),
                    "f/font.woff2": b"font data", "custom/resource.bin": b"keep me",
                }
                source = lc.make_archive(entries, level=0)
                result = lc.minify_archive(source)
                self.assertLess(len(result), len(source))
                output = unzip(result)
                self.assertEqual(output.keys(), entries.keys())
                for name, data in entries.items():
                    if name.endswith(".json"):
                        self.assertEqual(json.loads(output[name]), json.loads(data))
                    else:
                        self.assertEqual(output[name], data)
                self.assertEqual(result, lc.minify_archive(result))

    def test_archive_validation(self):
        valid = {"manifest.json": pretty(dict(version="2", animations=[dict(id="x")])),
                 "a/x.json": pretty(animation())}
        invalid = [
            {"a/x.json": pretty(animation())},
            {"manifest.json": valid["manifest.json"]},
            {**valid, "../secret": b"bad"},
            {**valid, "manifest.json": pretty(dict(version="3", animations=[dict(id="x")]))},
            {**valid, "a/x.json": b"{}"},
        ]
        for entries in invalid:
            with self.subTest(entries=list(entries)), self.assertRaises(lc.CompressorError):
                lc.minify_archive(lc.make_archive(entries))
        source = lc.make_archive(valid)
        with patch.object(lc, "MAX_BYTES", 10), self.assertRaises(lc.CompressorError):
            lc.minify_archive(source)

    def test_duplicate_zip_entries_are_rejected(self):
        buffer = io.BytesIO()
        with zipfile.ZipFile(buffer, "w") as archive:
            archive.writestr("same", b"one")
            with self.assertWarns(UserWarning):
                archive.writestr("same", b"two")
        with self.assertRaisesRegex(lc.CompressorError, "duplicada"):
            lc.minify_archive(buffer.getvalue())

    def test_cli_conversion_minification_and_overwrite_guards(self):
        with tempfile.TemporaryDirectory() as directory, contextlib.redirect_stdout(io.StringIO()), contextlib.redirect_stderr(io.StringIO()):
            base = Path(directory)
            source = base / "demo.json"
            original = pretty(animation(nm="demo"))
            source.write_bytes(original)
            self.assertEqual(lc.main(["pack", str(source)]), 0)
            packed = base / "demo.lottie"
            self.assertTrue(packed.exists())
            packed.write_bytes(b"user content")
            self.assertEqual(lc.main(["pack", str(source)]), 1)
            self.assertEqual(packed.read_bytes(), b"user content")
            self.assertEqual(lc.main(["pack", str(source), "--force"]), 0)
            self.assertEqual(lc.main(["minify", str(packed)]), 0)
            self.assertEqual(lc.main(["minify", str(source)]), 0)
            self.assertEqual(json.loads((base / "demo.min.json").read_bytes()), json.loads(original))
            self.assertEqual(lc.main(["minify", str(source), "-o", str(source), "--force"]), 1)
            self.assertEqual(source.read_bytes(), original)
            self.assertFalse(list(base.glob(".lottie-*")))


if __name__ == "__main__":
    unittest.main()
