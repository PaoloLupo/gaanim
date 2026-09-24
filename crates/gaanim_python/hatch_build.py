"""Bundle the Typst documentation sources into the authoring wheel.

Agents working in external Gaanim projects read ``gaanim/_docs`` from the
installed package, so the documentation always matches the installed version.
Editable installs skip the copy; tools resolve the checkout's ``docs/content``.
"""

from __future__ import annotations

from pathlib import Path

from hatchling.builders.hooks.plugin.interface import BuildHookInterface

DOC_SUFFIXES = {".typ", ".py"}


class CustomBuildHook(BuildHookInterface):
    def initialize(self, version: str, build_data: dict) -> None:
        if self.target_name != "wheel" or version == "editable":
            return
        content = Path(self.root).resolve().parents[1] / "docs" / "content"
        if not (content / "index.typ").is_file():
            return
        for source in sorted(content.rglob("*")):
            if (
                source.is_file()
                and source.suffix in DOC_SUFFIXES
                and "__pycache__" not in source.parts
            ):
                relative = source.relative_to(content).as_posix()
                build_data["force_include"][str(source)] = f"gaanim/_docs/{relative}"
