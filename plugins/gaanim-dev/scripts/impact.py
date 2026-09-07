#!/usr/bin/env python3
"""Report the verification and documentation impact of a Gaanim change."""

from __future__ import annotations

import argparse
from dataclasses import dataclass, asdict
import json
from pathlib import Path
import subprocess
import sys
from typing import Iterable


VISUAL_CRATES = {
    "gaanim_animation",
    "gaanim_api",
    "gaanim_layout",
    "gaanim_math",
    "gaanim_objects",
    "gaanim_python",
    "gaanim_renderer",
    "gaanim_scene",
    "gaanim_text",
    "gaanim_timeline",
}

VISUAL_CANDIDATES = {
    "gaanim_animation": ["transform_demo"],
    "gaanim_api": ["transform_demo"],
    "gaanim_layout": ["layout_verification", "layout_fit_demo"],
    "gaanim_math": ["camera_demo", "transform_demo"],
    "gaanim_objects": ["svg_demo", "image_demo"],
    "gaanim_python": ["transform_demo"],
    "gaanim_renderer": ["visual_effects_demo", "transform_demo"],
    "gaanim_scene": ["camera_demo", "transform_demo"],
    "gaanim_text": ["typst_layout_demo", "math_animation"],
    "gaanim_timeline": ["transform_demo"],
}

PERFORMANCE_CRATES = {
    "gaanim_editor",
    "gaanim_export",
    "gaanim_media",
    "gaanim_renderer",
    "gaanim_timeline",
}

PERFORMANCE_PATHS = {
    "examples/performance_benchmark.py",
    "tests/benchmark_runtime.py",
    "tests/performance/budgets.json",
    "tests/test_benchmark_runtime.py",
}


@dataclass(frozen=True)
class Impact:
    changed_files: list[str]
    crates: list[str]
    categories: list[str]
    documentation: list[str]
    visual_examples: list[str]
    commands: list[str]

    def to_dict(self) -> dict[str, object]:
        return asdict(self)


def find_repo_root(start: Path) -> Path:
    """Find a Gaanim checkout without assuming the plugin install location."""
    current = start.resolve()
    for candidate in (current, *current.parents):
        if (
            (candidate / "Cargo.toml").is_file()
            and (candidate / "justfile").is_file()
            and (candidate / "crates").is_dir()
        ):
            return candidate
    raise FileNotFoundError(
        "Could not find a Gaanim repository (Cargo.toml, justfile, crates/)"
    )


def _git_lines(repo: Path, args: list[str]) -> set[str]:
    result = subprocess.run(
        ["git", *args],
        cwd=repo,
        check=False,
        capture_output=True,
        text=True,
    )
    if result.returncode != 0:
        detail = result.stderr.strip() or result.stdout.strip()
        raise RuntimeError(f"git {' '.join(args)} failed: {detail}")
    return {line.strip().replace("\\", "/") for line in result.stdout.splitlines() if line.strip()}


def changed_files(repo: Path, base: str | None) -> list[str]:
    """Include committed range, staged, unstaged, and untracked paths."""
    paths: set[str] = set()
    if base:
        paths |= _git_lines(repo, ["diff", "--name-only", "--diff-filter=ACMR", f"{base}...HEAD"])
    paths |= _git_lines(repo, ["diff", "--name-only", "--diff-filter=ACMR"])
    paths |= _git_lines(repo, ["diff", "--cached", "--name-only", "--diff-filter=ACMR"])
    paths |= _git_lines(repo, ["ls-files", "--others", "--exclude-standard"])
    return sorted(paths)


def _crate_for(path: str) -> str | None:
    parts = Path(path).parts
    if len(parts) >= 3 and parts[0] == "crates":
        return parts[1]
    return None


def _docs_for(paths: Iterable[str]) -> set[str]:
    docs: set[str] = set()
    for path in paths:
        lower = path.lower()
        if "gaanim_animation" in lower or any(
            token in lower for token in ("anim.rs", "transition.rs", "updater.rs")
        ):
            docs.add("docs/content/api/animations.typ")
        if "gaanim_layout" in lower or "/layout" in lower:
            docs.add("docs/content/api/layout.typ")
        if any(token in lower for token in ("theme", "color", "brush", "effect")):
            docs.add("docs/content/api/themes.typ")
        if any(token in lower for token in ("asset", "svg")):
            docs.add("docs/content/api/assets.typ")
        if "audio" in lower:
            docs.add("docs/content/api/audio.typ")
        if any(token in lower for token in ("pydrawable", "drawable", "objects", "primitive", "text")):
            docs.add("docs/content/api/mobjects.typ")
        if any(token in lower for token in ("pycanvas", "canvas", "scene", "runtime", "camera", "timeline")):
            docs.add("docs/content/api/scene.typ")
    return docs


def analyze_paths(repo: Path, paths: Iterable[str]) -> Impact:
    normalized = sorted({path.replace("\\", "/") for path in paths})
    crates = sorted({crate for path in normalized if (crate := _crate_for(path))})
    rust_files = [path for path in normalized if path.endswith(".rs")]
    docs_files = [path for path in normalized if path.startswith("docs/")]
    python_binding = any(
        path.startswith("crates/gaanim_python/src/")
        or path.endswith("gaanim_core.pyi")
        or path.endswith("gaanim/__init__.py")
        for path in normalized
    )
    rust_api = any(path.startswith("crates/gaanim_api/") for path in normalized)
    public_api = rust_api or python_binding
    visual = bool(set(crates) & VISUAL_CRATES)
    performance = bool(set(crates) & PERFORMANCE_CRATES) or any(
        path in PERFORMANCE_PATHS for path in normalized
    )
    plugin = any(path.startswith("plugins/gaanim-dev/") for path in normalized)
    repo_config = any(
        path in {"Cargo.toml", "justfile", "AGENTS.md", "README.md"}
        or path.startswith(".github/")
        for path in normalized
    )

    categories: list[str] = []
    for present, label in (
        (bool(rust_files), "rust"),
        (rust_api, "rust-api"),
        (python_binding, "python-api"),
        (bool(docs_files), "documentation"),
        (visual, "visual"),
        (performance, "performance"),
        (plugin, "plugin"),
        (repo_config, "repository-config"),
    ):
        if present:
            categories.append(label)

    documentation = sorted(_docs_for(normalized)) if public_api else []

    candidates: set[str] = set()
    for crate in crates:
        candidates.update(VISUAL_CANDIDATES.get(crate, []))
    visual_examples = sorted(
        example
        for example in candidates
        if (repo / "examples" / f"{example}.py").is_file()
        and (repo / "tests" / "visual" / example / "baseline").is_dir()
    )

    commands: list[str] = []
    docs_only = bool(normalized) and all(
        path.startswith("docs/") or path.startswith("plugins/gaanim-dev/")
        for path in normalized
    ) and bool(docs_files)
    if docs_only:
        commands.append("just docs")
    elif rust_files:
        commands.append("cargo fmt --all -- --check")
        rust_crates = sorted({crate for path in rust_files if (crate := _crate_for(path))})
        if rust_crates:
            commands.append("just dev test " + " ".join(f"-p {crate}" for crate in rust_crates))
    if rust_api:
        commands.append("just clippy")
    if python_binding:
        commands.extend(["just python-develop", "just validate-python-api"])
    if public_api:
        commands.append("just docs")
    if plugin:
        commands.append("python -m unittest discover -s plugins/gaanim-dev/tests")
    if performance:
        commands.append("just benchmark smoke")
    for example in visual_examples:
        runner = "target/debug/gaanim.exe" if sys.platform == "win32" else "target/debug/gaanim"
        commands.append(
            f"just dev-exec {runner} --diff --example examples/{example}.py --no-gui"
        )

    return Impact(
        changed_files=normalized,
        crates=crates,
        categories=categories,
        documentation=documentation,
        visual_examples=visual_examples,
        commands=list(dict.fromkeys(commands)),
    )


def _render_text(impact: Impact) -> str:
    def section(title: str, values: list[str]) -> list[str]:
        return [f"{title}:", *(f"  - {value}" for value in values or ["none"])]

    lines: list[str] = []
    lines += section("Changed files", impact.changed_files)
    lines += section("Crates", impact.crates)
    lines += section("Categories", impact.categories)
    lines += section("Documentation candidates", impact.documentation)
    lines += section("Visual examples", impact.visual_examples)
    lines += section("Recommended commands", impact.commands)
    return "\n".join(lines)


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repo", type=Path, default=Path.cwd())
    parser.add_argument("--base", help="Git ref used for <base>...HEAD impact")
    parser.add_argument("--format", choices=("text", "json"), default="text")
    args = parser.parse_args(argv)

    try:
        repo = find_repo_root(args.repo)
        impact = analyze_paths(repo, changed_files(repo, args.base))
    except (FileNotFoundError, RuntimeError) as error:
        print(f"impact: {error}", file=sys.stderr)
        return 2

    if args.format == "json":
        print(json.dumps(impact.to_dict(), indent=2))
    else:
        print(_render_text(impact))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
