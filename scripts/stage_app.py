"""Copy the Gaanim application files of one build into a directory.

`gaanim` links the engine library and Rust's `std` dynamically and loads the
Python plugin at runtime, so an installation holds these files side by side:

    gaanim[.exe]                      the executable
    gaanim_engine (.dll/.so/.dylib)   the engine, one copy
    gaanim_python_plugin (...)        Python support, loaded on demand
    std-<hash> (...)                  Rust's standard library, from the toolchain

Usage: python scripts/stage_app.py --profile dist --dest <DIR>
"""

from __future__ import annotations

import argparse
from pathlib import Path
import shutil
import subprocess
import sys


ROOT = Path(__file__).resolve().parents[1]


def library(stem: str) -> str:
    if sys.platform == "win32":
        return f"{stem}.dll"
    if sys.platform == "darwin":
        return f"lib{stem}.dylib"
    return f"lib{stem}.so"


def std_library() -> Path:
    """The toolchain's `std` shared library, which the build linked."""
    libdir = Path(subprocess.check_output(["rustc", "--print", "target-libdir"], text=True).strip())
    sysroot = Path(subprocess.check_output(["rustc", "--print", "sysroot"], text=True).strip())
    pattern = library("std-*")
    for directory in (libdir, sysroot / "bin"):
        found = sorted(directory.glob(pattern))
        if found:
            return found[0]
    raise SystemExit(f"no {pattern} in {libdir} or {sysroot / 'bin'}")


def application_files(build: Path) -> list[Path]:
    exe = "gaanim.exe" if sys.platform == "win32" else "gaanim"
    files = [build / exe, build / library("gaanim_engine"), build / library("gaanim_python_plugin")]
    missing = [str(path) for path in files if not path.is_file()]
    if missing:
        raise SystemExit("missing build outputs (run `just build-dist`): " + ", ".join(missing))
    return [*files, std_library()]


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--profile", default="dist", help="Cargo profile directory under target/")
    parser.add_argument("--dest", type=Path, required=True, help="directory to copy into")
    args = parser.parse_args()
    build = ROOT / "target" / ("debug" if args.profile == "dev" else args.profile)
    args.dest.mkdir(parents=True, exist_ok=True)
    for source in application_files(build):
        target = args.dest / source.name
        shutil.copy2(source, target)
        print(f"{source} -> {target}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
