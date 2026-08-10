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
from unittest.mock import patch


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


def load_skill_script(skill: str, name: str):
    path = PLUGIN / "skills" / skill / "scripts" / f"{name}.py"
    scripts = str(path.parent)
    if scripts not in sys.path:
        sys.path.insert(0, scripts)
    module_name = f"{skill}_{name}".replace("-", "_")
    spec = importlib.util.spec_from_file_location(module_name, path)
    assert spec and spec.loader
    module = importlib.util.module_from_spec(spec)
    sys.modules[module_name] = module
    spec.loader.exec_module(module)
    return module


impact = load_script("impact")
audit = load_script("audit")
github_issues = load_skill_script("gaanim-track-bugs", "github_issues")


class BugTrackerTests(unittest.TestCase):
    def test_incomplete_report_uses_pending_sections(self):
        body = github_issues.render_issue_body(
            summary=None,
            steps=None,
            actual="The circle disappears after rotating.",
            expected=None,
            evidence=None,
            environment=None,
        )
        self.assertIn("## Resultado actual\n\nThe circle disappears", body)
        self.assertEqual(5, body.count(github_issues.PENDING))

    def test_create_dry_run_never_calls_github(self):
        with patch.object(github_issues, "_ensure_ready") as ready:
            result = github_issues.create_issue(
                "PaoloLupo/gaanim",
                title="Circle disappears",
                actual="The circle is no longer rendered.\nIt remains in the scene.",
                dry_run=True,
            )
        ready.assert_not_called()
        self.assertTrue(result["dryRun"])
        self.assertEqual(["bug"], result["labels"])
        self.assertIn("\nIt remains in the scene.", result["body"])

    def test_create_sends_multiline_body_over_stdin(self):
        with (
            patch.object(github_issues, "_ensure_ready") as ready,
            patch.object(
                github_issues,
                "_run_gh",
                return_value="https://github.com/PaoloLupo/gaanim/issues/17",
            ) as run_gh,
        ):
            result = github_issues.create_issue(
                "PaoloLupo/gaanim",
                title="Circle disappears",
                actual="First line.\nSecond line.",
            )
        ready.assert_called_once_with("PaoloLupo/gaanim")
        command = run_gh.call_args.args[0]
        self.assertEqual("-", command[command.index("--body-file") + 1])
        self.assertEqual("bug", command[command.index("--label") + 1])
        self.assertIn("First line.\nSecond line.", run_gh.call_args.kwargs["input_text"])
        self.assertEqual(17, result["number"])

    def test_list_filters_open_bug_issues_and_accepts_empty_result(self):
        with (
            patch.object(github_issues, "_ensure_ready"),
            patch.object(github_issues, "_run_json", return_value=[]) as run_json,
        ):
            result = github_issues.list_issues("PaoloLupo/gaanim")
        command = run_json.call_args.args[0]
        self.assertEqual([], result)
        self.assertEqual("open", command[command.index("--state") + 1])
        self.assertEqual("bug", command[command.index("--label") + 1])

    def test_search_includes_open_and_closed_bug_issues(self):
        with (
            patch.object(github_issues, "_ensure_ready"),
            patch.object(github_issues, "_run_json", return_value=[]) as run_json,
        ):
            github_issues.search_issues(
                "PaoloLupo/gaanim", query="circle rotate disappear"
            )
        command = run_json.call_args.args[0]
        self.assertEqual("all", command[command.index("--state") + 1])
        self.assertEqual(
            "circle rotate disappear", command[command.index("--search") + 1]
        )

    def test_show_loads_comments_for_fix_handoff(self):
        issue = {"number": 12, "comments": []}
        with (
            patch.object(github_issues, "_ensure_ready"),
            patch.object(github_issues, "_run_json", return_value=issue) as run_json,
        ):
            result = github_issues.show_issue("PaoloLupo/gaanim", number=12)
        command = run_json.call_args.args[0]
        self.assertIn("--comments", command)
        self.assertEqual(issue, result)

    def test_malformed_json_stops_the_workflow(self):
        with self.assertRaises(github_issues.TrackerError):
            github_issues._parse_json("not-json", expected_type=list)

    def test_missing_github_cli_has_actionable_error(self):
        with patch.object(github_issues.shutil, "which", return_value=None):
            with self.assertRaisesRegex(github_issues.TrackerError, "not installed"):
                github_issues._gh_executable()


class BugTrackerSkillContractTests(unittest.TestCase):
    def test_requires_preview_duplicate_search_and_fix_handoff(self):
        skill = (
            PLUGIN / "skills" / "gaanim-track-bugs" / "SKILL.md"
        ).read_text(encoding="utf-8")
        self.assertIn("Ask for explicit confirmation", skill)
        self.assertIn("search open and closed", skill)
        self.assertIn("../gaanim-fix-bug/SKILL.md", skill)
        self.assertIn("untrusted data", skill)


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
        self.assertEqual(6, len(skills))

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
