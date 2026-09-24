from __future__ import annotations

import importlib.util
import json
import os
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import patch


PLUGIN = Path(__file__).resolve().parents[1]
SCRIPT = PLUGIN / "skills" / "gaanim-docs" / "scripts" / "gaanim_docs.py"
spec = importlib.util.spec_from_file_location("gaanim_docs", SCRIPT)
assert spec and spec.loader
gaanim_docs = importlib.util.module_from_spec(spec)
sys.modules["gaanim_docs"] = gaanim_docs
spec.loader.exec_module(gaanim_docs)

INDEX = """#book-part("I", "Aprender", description: "x")
#include "manual/intro.typ"
#book-part("II", "Referencia", description: "y")
#include "api/scene.typ"
"""


def write_docs(root: Path) -> None:
    (root / "manual").mkdir(parents=True)
    (root / "api").mkdir()
    (root / "examples").mkdir()
    (root / "index.typ").write_text(INDEX, encoding="utf-8")
    (root / "manual" / "intro.typ").write_text(
        '#docs-chapter(\n  title: "Introducción",\n  description: "Qué es Gaanim",\n)\n',
        encoding="utf-8",
    )
    (root / "api" / "scene.typ").write_text(
        '#show: docs-chapter.with(title: "API de Scene")\n'
        '#api-entry(name: "Scene.segment", signature: "segment(name)")\n',
        encoding="utf-8",
    )
    (root / "examples" / "basic.py").write_text("scene.render()\n", encoding="utf-8")


def write_site_packages(site: Path, *, bundled: bool) -> Path:
    package = site / "gaanim"
    package.mkdir(parents=True)
    (package / "__init__.py").write_text("", encoding="utf-8")
    (package / "gaanim_core.pyi").write_text("", encoding="utf-8")
    dist = site / "gaanim-0.9.1.dist-info"
    dist.mkdir()
    (dist / "METADATA").write_text("Name: gaanim\nVersion: 0.9.1\n", encoding="utf-8")
    if bundled:
        write_docs(package / "_docs")
    return package


class LocateTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.base = Path(self.temp.name)
        self.enterContext(patch.dict(os.environ))
        os.environ.pop("GAANIM_DOCS", None)

    def test_bundled_docs_from_project_venv(self):
        project = self.base / "proj"
        package = write_site_packages(project / ".venv" / "Lib" / "site-packages", bundled=True)
        (project / "scenes").mkdir()

        found = gaanim_docs.locate(project / "scenes")

        self.assertIsNotNone(found)
        self.assertEqual((package / "_docs").resolve(), found.root.resolve())
        self.assertEqual("0.9.1", found.version)
        self.assertEqual(package.resolve(), found.stubs.resolve())

    def test_posix_venv_layout(self):
        project = self.base / "proj"
        site = project / ".venv" / "lib" / "python3.14" / "site-packages"
        package = write_site_packages(site, bundled=True)

        found = gaanim_docs.locate(project)

        self.assertEqual((package / "_docs").resolve(), found.root.resolve())

    def test_editable_install_resolves_checkout_docs(self):
        repo = self.base / "repo"
        source = repo / "crates" / "gaanim_python"
        (source / "gaanim").mkdir(parents=True)
        (source / "gaanim" / "__init__.py").write_text("", encoding="utf-8")
        write_docs(repo / "docs" / "content")
        project = self.base / "proj"
        site = project / ".venv" / "Lib" / "site-packages"
        site.mkdir(parents=True)
        (site / "_gaanim.pth").write_text(f"{source}\n", encoding="utf-8")

        found = gaanim_docs.locate(project)

        self.assertEqual((repo / "docs" / "content").resolve(), found.root.resolve())
        self.assertIn("editable", found.source)

    def test_explicit_override_wins(self):
        docs = self.base / "docs"
        write_docs(docs)
        write_site_packages(self.base / "proj" / ".venv" / "Lib" / "site-packages", bundled=True)

        found = gaanim_docs.locate(self.base / "proj", str(docs))

        self.assertEqual(docs.resolve(), found.root)
        self.assertEqual("explicit override", found.source)


class ContentTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        write_docs(self.root)

    def test_index_follows_book_order_and_lists_extras(self):
        lines = [line.strip() for line in gaanim_docs.index(self.root) if line.strip()]

        self.assertEqual(
            [
                "## Parte I: Aprender",
                "manual/intro.typ  Introducción — Qué es Gaanim",
                "## Parte II: Referencia",
                "api/scene.typ  API de Scene",
                "## Otros archivos",
                "examples/basic.py",
            ],
            lines,
        )

    def test_search_reports_relative_path_and_line(self):
        matches = gaanim_docs.search(self.root, r"scene\.segment", limit=10)

        self.assertEqual(
            ['api/scene.typ:2: #api-entry(name: "Scene.segment", signature: "segment(name)")'],
            matches,
        )


class ManifestTests(unittest.TestCase):
    def test_manifests_and_marketplaces_register_the_plugin(self):
        repo = PLUGIN.parents[1]
        for manifest in ("plugin.json", ".claude-plugin/plugin.json", ".codex-plugin/plugin.json"):
            data = json.loads((PLUGIN / manifest).read_text(encoding="utf-8"))
            self.assertEqual("gaanim", data["name"])
        claude = json.loads((repo / ".claude-plugin" / "marketplace.json").read_text(encoding="utf-8"))
        self.assertIn(
            "./plugins/gaanim",
            [item["source"] for item in claude["plugins"] if item["name"] == "gaanim"],
        )
        codex = json.loads((repo / ".agents" / "plugins" / "marketplace.json").read_text(encoding="utf-8"))
        entry = next(item for item in codex["plugins"] if item["name"] == "gaanim")
        self.assertEqual("./plugins/gaanim", entry["source"]["path"])
        self.assertEqual(
            ["gaanim-docs"],
            [path.parent.name for path in sorted((PLUGIN / "skills").glob("*/SKILL.md"))],
        )


if __name__ == "__main__":
    unittest.main()
