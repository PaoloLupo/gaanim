"""Bundle the Typst documentation sources and the license texts into the
authoring wheel.

Agents working in external Gaanim projects read ``gaanim/_docs`` from the
installed package, so the documentation always matches the installed version.
Editable installs skip the copy; tools resolve the checkout's ``docs/content``.

The license texts live at the repository root, outside this project, where
``license-files`` cannot reach them; they go to ``.dist-info/extra_metadata``.
"""

from __future__ import annotations

from pathlib import Path

from hatchling.builders.hooks.plugin.interface import BuildHookInterface

DOC_SUFFIXES = {".typ", ".py"}
LICENSE_FILES = ("LICENSE-MIT", "LICENSE-APACHE")


class CustomBuildHook(BuildHookInterface):
    def initialize(self, version: str, build_data: dict) -> None:
        if self.target_name != "wheel" or version == "editable":
            return
        repository = Path(self.root).resolve().parents[1]
        for name in LICENSE_FILES:
            license_file = repository / name
            if license_file.is_file():
                build_data["extra_metadata"][str(license_file)] = name
        content = repository / "docs" / "content"
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
