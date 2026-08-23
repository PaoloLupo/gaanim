"""Validate that the public Gaanim wheel contains no native runtime.

The wheel is deliberately an authoring companion for the Gaanim executable.
Besides checking the archive shape, this script imports the built wheel in an
isolated interpreter and verifies that it refuses to run without the embedded
``gaanim.gaanim_core`` module supplied by the executable.
"""

from __future__ import annotations

import sys
from pathlib import Path
import subprocess
from zipfile import ZipFile


RUNTIME_REQUIRED = (
    "The `gaanim` wheel only provides authoring helpers and type information."
)


def validate_archive(wheel: Path) -> list[str]:
    """Return objective packaging errors found in ``wheel``."""
    errors: list[str] = []
    with ZipFile(wheel) as archive:
        names = archive.namelist()
        wheel_metadata = [name for name in names if name.endswith(".dist-info/WHEEL")]
        metadata = (
            archive.read(wheel_metadata[0]).decode("utf-8", errors="replace")
            if len(wheel_metadata) == 1
            else ""
        )

    forbidden = [
        name
        for name in names
        if "__pycache__" in name
        or name.endswith((".pyc", ".so", ".pyd", ".dylib"))
        or name in {"gaanim/gaanim_core.py", "gaanim/gaanim_core/__init__.py"}
    ]
    required = {
        "gaanim/__init__.py",
        "gaanim/composition.py",
        "gaanim/composition.pyi",
        "gaanim/gaanim_core.pyi",
        "gaanim/matrix.py",
        "gaanim/matrix.pyi",
        "gaanim/py.typed",
    }
    missing = sorted(required.difference(names))

    if forbidden:
        errors.append(f"Native, cached, or runtime-shadowing files found: {forbidden}")
    if missing:
        errors.append(f"Required authoring files missing: {missing}")
    if not wheel.name.endswith("-py3-none-any.whl"):
        errors.append(f"Wheel is not universal: {wheel.name}")
    if len(wheel_metadata) != 1:
        errors.append(f"Expected one WHEEL metadata file, found {len(wheel_metadata)}")
    else:
        if "Root-Is-Purelib: true" not in metadata:
            errors.append("WHEEL metadata does not declare Root-Is-Purelib: true")
        if "Tag: py3-none-any" not in metadata:
            errors.append("WHEEL metadata does not declare Tag: py3-none-any")
    return errors


def validate_runtime_boundary(wheel: Path) -> list[str]:
    """Verify that plain Python cannot use the authoring wheel as a runtime."""
    code = (
        "import sys; "
        "sys.path.insert(0, sys.argv[1]); "
        "import gaanim"
    )
    result = subprocess.run(
        [sys.executable, "-I", "-c", code, str(wheel.resolve())],
        capture_output=True,
        text=True,
        check=False,
    )
    output = result.stdout + result.stderr
    if result.returncode == 0:
        return ["The authoring wheel imported without the Gaanim executable runtime"]
    if RUNTIME_REQUIRED not in output:
        return [
            "The isolated import failed for an unexpected reason; "
            f"expected the executable-runtime diagnostic, got:\n{output.strip()}"
        ]
    return []


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
    errors = validate_archive(wheel) + validate_runtime_boundary(wheel)
    if errors:
        print("\n".join(f"- {error}" for error in errors), file=sys.stderr)
        return 1
    print(f"Authoring-only wheel contract passed: {wheel}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
