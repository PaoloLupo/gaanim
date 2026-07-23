"""Check that the shipped Python stub matches the native extension surface."""

from __future__ import annotations

import ast
import importlib
from pathlib import Path
import sys


ROOT = Path(__file__).resolve().parents[1]
STUB = ROOT / "crates" / "gaanim_python" / "gaanim" / "gaanim_core.pyi"
TYPE_CHECKING_ONLY = {"CurvePoint", "CurveControl", "CurveCommand"}


def declared_members(node: ast.ClassDef) -> set[str]:
    members = {
        child.name
        for child in node.body
        if isinstance(child, (ast.FunctionDef, ast.AsyncFunctionDef))
    }
    members.update(
        child.target.id
        for child in node.body
        if isinstance(child, ast.AnnAssign) and isinstance(child.target, ast.Name)
    )
    return members


def main() -> int:
    tree = ast.parse(STUB.read_text(encoding="utf-8"), filename=str(STUB))
    module = importlib.import_module("gaanim.gaanim_core")
    missing: list[str] = []

    for node in tree.body:
        if isinstance(node, ast.ClassDef):
            native_class = getattr(module, node.name, None)
            if native_class is None:
                missing.append(node.name)
                continue
            for member in declared_members(node):
                if not hasattr(native_class, member):
                    missing.append(f"{node.name}.{member}")
        elif (
            isinstance(node, ast.AnnAssign)
            and isinstance(node.target, ast.Name)
            and node.target.id not in TYPE_CHECKING_ONLY
            and not hasattr(module, node.target.id)
        ):
            missing.append(node.target.id)

    if missing:
        print("Stub declarations missing from gaanim.gaanim_core:", file=sys.stderr)
        print("\n".join(f"- {name}" for name in missing), file=sys.stderr)
        return 1

    print("Python stub matches the native extension surface.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
