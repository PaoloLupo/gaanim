#!/usr/bin/env python3
"""Build the Gaanim web player into a static folder.

Compiles `gaanim_web` to WebAssembly, generates its JavaScript bindings with
`wasm-bindgen` (the CLI version must match the `wasm-bindgen` crate), and
copies the page and icons next to them. The module ships gzip-compressed
(`gaanim_web_bg.wasm.gz`) and the page decompresses it, so it downloads at
its compressed size from any static host, GitHub Pages included. Serve the
folder with any static server, for example `python -m http.server -d dist/web`.

The embed player (`gaanim_embed`), which other pages put in an <iframe>, is
built into `<out>/insertar/`, in a Cargo run of its own: built with the web
player, Bevy's features for it would add about a tenth to the module.

Usage: python scripts/build_web.py [--out DIR] [--profile PROFILE]
"""

from __future__ import annotations

import argparse
import gzip
import hashlib
import os
import re
import shutil
import subprocess
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
TARGET = "wasm32-unknown-unknown"
# getrandom 0.3 needs its web backend selected explicitly.
RUSTFLAGS = '--cfg getrandom_backend="wasm_js"'
PAGE_FILES = ("index.html", "main.js")
# The embed player's page, in its folder of the web player.
EMBED_DIR = "insertar"
ICONS = ("favicon.ico", "gaanim-icon-512.png")


def check_wasm_bindgen() -> None:
    """Stop early when the CLI does not match the pinned crate version."""
    manifest = (ROOT / "crates" / "gaanim_web" / "Cargo.toml").read_text(encoding="utf-8")
    pinned = re.search(r'^wasm-bindgen = "=([^"]+)"', manifest, re.MULTILINE)
    try:
        output = subprocess.run(
            ["wasm-bindgen", "--version"], check=True, capture_output=True, text=True
        ).stdout
    except (OSError, subprocess.CalledProcessError):
        raise SystemExit(
            "wasm-bindgen is not installed: cargo install wasm-bindgen-cli --version "
            f"{pinned.group(1) if pinned else '<version>'} --locked"
        )
    installed = output.split()[-1]
    if pinned and installed != pinned.group(1):
        raise SystemExit(
            f"wasm-bindgen {installed} is installed but gaanim_web pins {pinned.group(1)}: "
            f"cargo install wasm-bindgen-cli --version {pinned.group(1)} --locked"
        )


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--out", type=Path, default=ROOT / "dist" / "web")
    parser.add_argument("--profile", default="web")
    args = parser.parse_args()

    check_wasm_bindgen()
    env = dict(os.environ, CARGO_TARGET_WASM32_UNKNOWN_UNKNOWN_RUSTFLAGS=RUSTFLAGS)
    for package_name in ("gaanim_web", "gaanim_embed"):
        subprocess.run(
            [
                "cargo",
                "build",
                "-p",
                package_name,
                "--target",
                TARGET,
                "--profile",
                args.profile,
            ],
            cwd=ROOT,
            env=env,
            check=True,
        )
    profile_dir = "debug" if args.profile == "dev" else args.profile
    built = ROOT / "target" / TARGET / profile_dir

    out = args.out
    shutil.rmtree(out, ignore_errors=True)
    player = package(built / "gaanim_web.wasm", out, ROOT / "crates" / "gaanim_web" / "web")
    for name in ICONS:
        shutil.copy(ROOT / "docs" / "assets" / "brand" / name, out / name)
    embed = package(
        built / "gaanim_embed.wasm", out / EMBED_DIR, ROOT / "crates" / "gaanim_embed" / "web"
    )
    print(f"web player: {out} ({player}); embed player: {out / EMBED_DIR} ({embed})")
    return 0


def package(wasm: Path, out: Path, page: Path) -> str:
    """The bindings of `wasm` in `out/pkg`, its module gzip-compressed, and
    the files of `page` next to them; returns the module's sizes."""
    (out / "pkg").mkdir(parents=True)
    subprocess.run(
        [
            "wasm-bindgen",
            "--target",
            "web",
            "--no-typescript",
            "--out-dir",
            str(out / "pkg"),
            str(wasm),
        ],
        check=True,
    )
    for name in PAGE_FILES:
        shutil.copy(page / name, out / name)

    module = out / "pkg" / f"{wasm.stem}_bg.wasm"
    size = module.stat().st_size
    compressed = module.with_suffix(".wasm.gz")
    with module.open("rb") as source, gzip.open(compressed, "wb", compresslevel=9) as target:
        shutil.copyfileobj(source, target)
    module.unlink()
    version(out, wasm.stem, hashlib.sha256(compressed.read_bytes()).hexdigest()[:12])
    return f"wasm {size / 1e6:.1f} MB, {compressed.stat().st_size / 1e6:.1f} MB compressed"


def version(out: Path, stem: str, digest: str) -> None:
    """Ask for the page's script, the bindings and the module by their build
    (`?v=digest`): cached apart, a browser could otherwise run new bindings
    with an old module ("__wbindgen_… is not a function") after a deploy.
    `pkg/version.txt` names the build for other pages that load the module
    (the playground's video export)."""
    rewrites = {
        "main.js": [
            (f"pkg/{stem}.js\"", f"pkg/{stem}.js?v={digest}\""),
            (f"pkg/{stem}_bg.wasm.gz\"", f"pkg/{stem}_bg.wasm.gz?v={digest}\""),
        ],
        "index.html": [('src="main.js"', f'src="main.js?v={digest}"')],
    }
    for name, pairs in rewrites.items():
        path = out / name
        text = path.read_text(encoding="utf-8")
        for old, new in pairs:
            if old not in text:
                raise SystemExit(f"{path}: {old} not found; update scripts/build_web.py")
            text = text.replace(old, new)
        path.write_text(text, encoding="utf-8", newline="\n")
    (out / "pkg" / "version.txt").write_text(digest, encoding="utf-8")


if __name__ == "__main__":
    raise SystemExit(main())
