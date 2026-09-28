#!/usr/bin/env python3
"""Build the Gaanim web player into a static folder.

Compiles `gaanim_web` to WebAssembly, generates its JavaScript bindings with
`wasm-bindgen` (the CLI version must match the `wasm-bindgen` crate), and
copies the page and icons next to them. Serve the folder with any static
server, for example `python -m http.server -d dist/web`.

Usage: python scripts/build_web.py [--out DIR] [--profile PROFILE]
"""

from __future__ import annotations

import argparse
import os
import shutil
import subprocess
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
TARGET = "wasm32-unknown-unknown"
# getrandom 0.3 needs its web backend selected explicitly.
RUSTFLAGS = '--cfg getrandom_backend="wasm_js"'
PAGE_FILES = ("index.html", "main.js")
ICONS = ("favicon.ico", "gaanim-icon-512.png")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--out", type=Path, default=ROOT / "dist" / "web")
    parser.add_argument("--profile", default="web")
    args = parser.parse_args()

    env = dict(os.environ, CARGO_TARGET_WASM32_UNKNOWN_UNKNOWN_RUSTFLAGS=RUSTFLAGS)
    subprocess.run(
        ["cargo", "build", "-p", "gaanim_web", "--target", TARGET, "--profile", args.profile],
        cwd=ROOT,
        env=env,
        check=True,
    )
    profile_dir = "debug" if args.profile == "dev" else args.profile
    wasm = ROOT / "target" / TARGET / profile_dir / "gaanim_web.wasm"

    out = args.out
    shutil.rmtree(out, ignore_errors=True)
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
        shutil.copy(ROOT / "crates" / "gaanim_web" / "web" / name, out / name)
    for name in ICONS:
        shutil.copy(ROOT / "docs" / "assets" / "brand" / name, out / name)

    size = (out / "pkg" / "gaanim_web_bg.wasm").stat().st_size
    print(f"web player: {out} (wasm {size / 1e6:.1f} MB)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
