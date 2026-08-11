#!/usr/bin/env python3
"""Audit objective Gaanim repository contracts and report heuristic drift."""

from __future__ import annotations

import argparse
import ast
from dataclasses import dataclass, asdict
import json
from pathlib import Path
import re
import sys
import tomllib

import impact as impact_tool


@dataclass(frozen=True)
class Finding:
    level: str
    code: str
    message: str


REQUIRED_RECIPES = {
    "build",
    "check",
    "clippy",
    "docs",
    "doctor",
    "python-develop",
    "validate-python-api",
}

REQUIRED_DOCS = {
    "animations.typ",
    "assets.typ",
    "audio.typ",
    "index.typ",
    "layout.typ",
    "mobjects.typ",
    "scene.typ",
    "themes.typ",
}


def _error(code: str, message: str) -> Finding:
    return Finding("error", code, message)


def _warning(code: str, message: str) -> Finding:
    return Finding("warning", code, message)


def _just_recipes(text: str) -> set[str]:
    return set(re.findall(r"(?m)^([a-zA-Z0-9_-]+)(?:\s+[^:]*)?:", text))


def _stub_names(path: Path) -> set[str]:
    tree = ast.parse(path.read_text(encoding="utf-8"), filename=str(path))
    names: set[str] = set()
    for node in tree.body:
        if isinstance(node, (ast.ClassDef, ast.FunctionDef, ast.AsyncFunctionDef)):
            names.add(node.name)
        elif isinstance(node, ast.AnnAssign) and isinstance(node.target, ast.Name):
            names.add(node.target.id)
        elif isinstance(node, ast.Assign):
            names.update(target.id for target in node.targets if isinstance(target, ast.Name))
    return names


def _python_exports(path: Path) -> tuple[set[str], set[str]]:
    tree = ast.parse(path.read_text(encoding="utf-8"), filename=str(path))
    available: set[str] = set()
    exported: set[str] = set()
    for node in tree.body:
        if isinstance(node, ast.ImportFrom) and node.level == 1:
            available.update(alias.asname or alias.name for alias in node.names)
        elif isinstance(node, (ast.FunctionDef, ast.AsyncFunctionDef, ast.ClassDef)):
            available.add(node.name)
        elif isinstance(node, ast.Assign):
            for target in node.targets:
                if isinstance(target, ast.Name):
                    available.add(target.id)
                    if target.id == "__all__" and isinstance(node.value, (ast.List, ast.Tuple)):
                        exported.update(
                            item.value
                            for item in node.value.elts
                            if isinstance(item, ast.Constant) and isinstance(item.value, str)
                        )
    return available, exported


def agent_guidance_drift(
    agents: str, justfile: str, member_count: int, readme_exists: bool
) -> list[Finding]:
    findings: list[Finding] = []
    count_match = re.search(r"defines\s+([0-9]+)\s+(?:workspace members|crates)", agents)
    if count_match and int(count_match.group(1)) != member_count:
        findings.append(
            _warning(
                "agents-crate-count",
                f"AGENTS.md says {count_match.group(1)} members; Cargo.toml has {member_count}",
            )
        )
    if "No README exists" in agents and readme_exists:
        findings.append(_warning("agents-readme", "AGENTS.md says README.md does not exist"))
    if "`just build` → runs `maturin develop`" in agents and "build:\n    cargo build" in justfile:
        findings.append(
            _warning(
                "agents-build",
                "AGENTS.md describes just build as maturin develop, but justfile builds application binaries",
            )
        )
    if "imports extension" in agents and "doctor:" in justfile and "validate-python-api" not in justfile.split("doctor:", 1)[1]:
        findings.append(
            _warning(
                "agents-doctor",
                "AGENTS.md says doctor imports the extension, but the recipe only checks application binaries",
            )
        )
    return findings


def change_contract_findings(change: impact_tool.Impact) -> list[Finding]:
    findings: list[Finding] = []
    paths = change.changed_files
    categories = set(change.categories)
    public_api = bool({"rust-api", "python-api"} & categories)
    changed_docs = {
        path for path in paths if path.startswith("docs/") and path.endswith(".typ")
    }
    binding_changed = any(path.startswith("crates/gaanim_python/src/") for path in paths)
    stub_changed = any(path.endswith("gaanim_core.pyi") for path in paths)
    test_or_example_changed = any(
        path.startswith("tests/")
        or path.startswith("examples/")
        or "/tests/" in path
        for path in paths
    )

    if public_api and not changed_docs:
        findings.append(
            _warning(
                "api-doc-sync",
                "Public API paths changed without a Typst documentation change; confirm whether the surface is user-visible",
            )
        )
    if "rust-api" in categories and not binding_changed:
        findings.append(
            _warning(
                "api-binding-sync",
                "gaanim_api changed without a PyO3 binding change; confirm the public Python facade is intentionally unaffected",
            )
        )
    if binding_changed and not stub_changed:
        findings.append(
            _warning(
                "binding-stub-sync",
                "PyO3 sources changed without a matching stub change; confirm the runtime surface did not change",
            )
        )
    if public_api and not test_or_example_changed:
        findings.append(
            _warning(
                "api-test-coverage",
                "Public API paths changed without a separate test or example path; confirm focused regression coverage, including inline tests",
            )
        )
    return findings


def collect_findings(repo: Path, base: str | None = None) -> list[Finding]:
    findings: list[Finding] = []

    root_manifest = repo / "Cargo.toml"
    try:
        workspace = tomllib.loads(root_manifest.read_text(encoding="utf-8"))["workspace"]
        members = workspace["members"]
    except (OSError, KeyError, tomllib.TOMLDecodeError) as error:
        return [_error("workspace-manifest", f"Cannot read workspace members: {error}")]

    for member in members:
        if not (repo / member / "Cargo.toml").is_file():
            findings.append(_error("workspace-member", f"Missing manifest for workspace member {member}"))

    justfile = (repo / "justfile").read_text(encoding="utf-8")
    missing_recipes = sorted(REQUIRED_RECIPES - _just_recipes(justfile))
    if missing_recipes:
        findings.append(_error("just-recipes", f"Missing required just recipes: {', '.join(missing_recipes)}"))

    docs_dir = repo / "docs" / "content" / "api"
    missing_docs = sorted(name for name in REQUIRED_DOCS if not (docs_dir / name).is_file())
    if missing_docs:
        findings.append(_error("api-doc-pages", f"Missing API documentation pages: {', '.join(missing_docs)}"))
    docs_index = (repo / "docs" / "content" / "index.typ").read_text(encoding="utf-8")
    unlisted_docs = sorted(name for name in REQUIRED_DOCS - {"index.typ"} if f'api/{name}' not in docs_index)
    if unlisted_docs:
        findings.append(_error("api-doc-index", f"API pages not included from docs/content/index.typ: {', '.join(unlisted_docs)}"))

    stub = repo / "crates" / "gaanim_python" / "gaanim" / "gaanim_core.pyi"
    package = repo / "crates" / "gaanim_python" / "gaanim" / "__init__.py"
    if not stub.is_file() or not package.is_file():
        findings.append(_error("python-contract-files", "Python stub or package __init__.py is missing"))
    else:
        stub_names = _stub_names(stub)
        templates_stub = package.with_name("templates.pyi")
        if templates_stub.is_file():
            stub_names.update(_stub_names(templates_stub))
        available, exported = _python_exports(package)
        undefined_exports = sorted(exported - available)
        if undefined_exports:
            findings.append(_error("python-all", f"__all__ contains undefined names: {', '.join(undefined_exports)}"))
        imported_native = available - {"Canvas", "_norm_range", "_patched_axes", "_axes_plot", "_axes_plot_parametric"}
        missing_stub_names = sorted(name for name in imported_native if not name.startswith("_") and name not in stub_names)
        if missing_stub_names:
            findings.append(_error("python-stub", f"Top-level native imports missing from stub: {', '.join(missing_stub_names)}"))

    agents_path = repo / "AGENTS.md"
    if agents_path.is_file():
        agents = agents_path.read_text(encoding="utf-8")
        findings.extend(
            agent_guidance_drift(
                agents,
                justfile,
                len(members),
                (repo / "README.md").is_file(),
            )
        )

    try:
        paths = impact_tool.changed_files(repo, base)
        change = impact_tool.analyze_paths(repo, paths)
    except RuntimeError as error:
        findings.append(_warning("git-impact", str(error)))
    else:
        findings.extend(change_contract_findings(change))

    return findings


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repo", type=Path, default=Path.cwd())
    parser.add_argument("--base")
    parser.add_argument("--format", choices=("text", "json"), default="text")
    parser.add_argument("--strict", action="store_true", help="Treat heuristic warnings as failures")
    args = parser.parse_args(argv)

    try:
        repo = impact_tool.find_repo_root(args.repo)
        findings = collect_findings(repo, args.base)
    except (FileNotFoundError, OSError, SyntaxError) as error:
        print(f"audit: {error}", file=sys.stderr)
        return 2

    if args.format == "json":
        print(json.dumps([asdict(finding) for finding in findings], indent=2))
    elif not findings:
        print("Repository audit passed with no findings.")
    else:
        for finding in findings:
            print(f"{finding.level.upper()} [{finding.code}] {finding.message}")

    errors = any(finding.level == "error" for finding in findings)
    warnings = any(finding.level == "warning" for finding in findings)
    return 1 if errors or (args.strict and warnings) else 0


if __name__ == "__main__":
    raise SystemExit(main())
