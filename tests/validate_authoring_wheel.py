"""Validate that the public Gaanim wheel contains no native runtime."""

from __future__ import annotations

import sys
from pathlib import Path
from zipfile import ZipFile


def main() -> int:
    candidate = Path(sys.argv[1])
    wheels = (
        sorted(candidate.glob("gaanim-*-py3-none-any.whl"))
        if candidate.is_dir()
        else [candidate]
    )
    if len(wheels) != 1:
        print(
            f"Expected one universal Gaanim wheel in {candidate}, found {len(wheels)}",
            file=sys.stderr,
        )
        return 1
    wheel = wheels[0]
    with ZipFile(wheel) as archive:
        names = archive.namelist()
    forbidden = [
        name
        for name in names
        if "__pycache__" in name
        or name.endswith((".pyc", ".so", ".pyd", ".dylib"))
    ]
    required = {"gaanim/__init__.py", "gaanim/gaanim_core.pyi", "gaanim/py.typed"}
    missing = sorted(required.difference(names))
    if wheel.name.endswith("-py3-none-any.whl") and not forbidden and not missing:
        print(f"Authoring wheel is pure Python: {wheel}")
        return 0
    if forbidden:
        print(f"Native or cached files found: {forbidden}", file=sys.stderr)
    if missing:
        print(f"Required authoring files missing: {missing}", file=sys.stderr)
    if not wheel.name.endswith("-py3-none-any.whl"):
        print(f"Wheel is not universal: {wheel.name}", file=sys.stderr)
    return 1


if __name__ == "__main__":
    raise SystemExit(main())
