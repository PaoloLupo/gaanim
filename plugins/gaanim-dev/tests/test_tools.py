from __future__ import annotations

from contextlib import redirect_stderr
import importlib.util
import io
import json
from pathlib import Path
import re
import sys
import tempfile
import unittest


PLUGIN = Path(__file__).resolve().parents[1]


def load_script(name: str):
    path = PLUGIN / "scripts" / f"{name}.py"
    scripts = str(path.parent)
    if scripts not in sys.path:
        sys.path.insert(0, scripts)
    spec = importlib.util.spec_from_file_location(name, path)
    assert spec and spec.loader
    module = importlib.util.module_from_spec(spec)
    sys.modules[name] = module
    spec.loader.exec_module(module)
    return module


impact = load_script("impact")
audit = load_script("audit")


class ImpactTests(unittest.TestCase):
    def test_scene_binding_requires_python_docs_and_visual(self):
        with tempfile.TemporaryDirectory() as temp:
            repo = Path(temp)
            (repo / "examples").mkdir()
            (repo / "tests" / "visual" / "transform_demo" / "baseline").mkdir(parents=True)
            (repo / "examples" / "transform_demo.py").touch()
            result = impact.analyze_paths(
                repo,
                ["crates/gaanim_python/src/pycanvas.rs"],
            )
        self.assertIn("python-api", result.categories)
        self.assertIn("docs/content/api/scene.typ", result.documentation)
        self.assertIn("just validate-python-api", result.commands)
        self.assertIn("transform_demo", result.visual_examples)

    def test_renderer_change_does_not_require_public_docs(self):
        with tempfile.TemporaryDirectory() as temp:
            result = impact.analyze_paths(
                Path(temp),
                ["crates/gaanim_renderer/src/pipeline.rs"],
            )
        self.assertIn("visual", result.categories)
        self.assertEqual([], result.documentation)

    def test_docs_only_selects_docs_build(self):
        with tempfile.TemporaryDirectory() as temp:
            result = impact.analyze_paths(
                Path(temp),
                ["docs/content/api/scene.typ"],
            )
        self.assertEqual(["just docs"], result.commands)


class VerifySafetyTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.verify = load_script("verify")

    def test_normalizes_example(self):
        self.assertEqual("camera_demo", self.verify._normalize_example("examples/camera_demo.py"))

    def test_rejects_parent_traversal(self):
        with self.assertRaises(ValueError):
            self.verify._normalize_example("../camera_demo.py")

    def test_bless_requires_explicit_interlock(self):
        with redirect_stderr(io.StringIO()):
            with self.assertRaises(SystemExit) as raised:
                self.verify.main(["visual", "--bless"])
        self.assertEqual(2, raised.exception.code)


class AuditTests(unittest.TestCase):
    def test_detects_stale_agent_guidance(self):
        findings = audit.agent_guidance_drift(
            "Cargo.toml defines 12 crates. No README exists. "
            "`just build` → runs `maturin develop`; doctor imports extension.",
            "build:\n    cargo build -p gaanim_editor\ndoctor: check\n    cargo build",
            member_count=16,
            readme_exists=True,
        )
        codes = {finding.code for finding in findings}
        self.assertEqual(
            {"agents-build", "agents-crate-count", "agents-doctor", "agents-readme"},
            codes,
        )

    def test_scene_api_change_prompts_binding_docs_and_tests(self):
        with tempfile.TemporaryDirectory() as temp:
            change = impact.analyze_paths(
                Path(temp),
                ["crates/gaanim_api/src/canvas/canvas_impl.rs"],
            )
        codes = {finding.code for finding in audit.change_contract_findings(change)}
        self.assertEqual(
            {"api-binding-sync", "api-doc-sync", "api-test-coverage"},
            codes,
        )


class ManifestTests(unittest.TestCase):
    def test_portable_manifest_and_skill_layout(self):
        manifest = json.loads((PLUGIN / "plugin.json").read_text(encoding="utf-8"))
        allowed = {
            "$schema",
            "name",
            "version",
            "description",
            "author",
            "homepage",
            "repository",
            "license",
            "keywords",
            "extensions",
        }
        self.assertFalse(set(manifest) - allowed)
        self.assertEqual("gaanim-dev", manifest["name"])
        self.assertRegex(
            manifest["name"],
            re.compile(r"^(?!.*(?:--|\.\.))[a-z0-9](?:[a-z0-9.-]*[a-z0-9])?$"),
        )
        self.assertEqual(
            "https://agent-plugins.org/schemas/1.0.0/plugin.schema.json",
            manifest["$schema"],
        )
        skills = sorted((PLUGIN / "skills").glob("*/SKILL.md"))
        self.assertEqual(5, len(skills))

    def test_codex_manifest_points_to_skills(self):
        manifest = json.loads(
            (PLUGIN / ".codex-plugin" / "plugin.json").read_text(encoding="utf-8")
        )
        self.assertEqual("gaanim-dev", manifest["name"])
        self.assertEqual("./skills/", manifest["skills"])

    def test_repo_marketplace_entry_is_complete(self):
        repo = PLUGIN.parents[1]
        marketplace = json.loads(
            (repo / ".agents" / "plugins" / "marketplace.json").read_text(
                encoding="utf-8"
            )
        )
        entry = next(item for item in marketplace["plugins"] if item["name"] == "gaanim-dev")
        self.assertEqual("./plugins/gaanim-dev", entry["source"]["path"])
        self.assertEqual("AVAILABLE", entry["policy"]["installation"])
        self.assertEqual("ON_INSTALL", entry["policy"]["authentication"])
        self.assertEqual("Developer Tools", entry["category"])


if __name__ == "__main__":
    unittest.main()
