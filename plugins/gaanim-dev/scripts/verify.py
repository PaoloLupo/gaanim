#!/usr/bin/env python3
"""Run change-aware Gaanim verification profiles without hiding skipped checks."""

from __future__ import annotations

import argparse
import os
from pathlib import Path
import shutil
import subprocess
import sys

import impact as impact_tool


def _normalize_example(value: str) -> str:
    value = value.replace("\\", "/")
    if value.startswith("examples/"):
        value = value[len("examples/") :]
    if value.endswith(".py"):
        value = value[:-3]
    if not value or value.startswith("/") or ".." in Path(value).parts:
        raise ValueError(f"Invalid example path: {value!r}")
    return value


def _run(repo: Path, command: list[str], dry_run: bool) -> int:
    rendered = subprocess.list2cmdline(command) if os.name == "nt" else " ".join(command)
    print(f"+ {rendered}", flush=True)
    if dry_run:
        return 0
    result = subprocess.run(command, cwd=repo, check=False)
    return result.returncode


def _append_unique(commands: list[list[str]], command: list[str]) -> None:
    if command not in commands:
        commands.append(command)


def _fast_commands(repo: Path, change: impact_tool.Impact) -> list[list[str]]:
    commands: list[list[str]] = []
    docs_changed = any(path.startswith("docs/") for path in change.changed_files)
    rust_changed = "rust" in change.categories
    plugin_changed = "plugin" in change.categories
    non_docs_changes = [
        path
        for path in change.changed_files
        if not path.startswith("docs/") and not path.startswith("plugins/gaanim-dev/")
    ]
    if docs_changed and not non_docs_changes:
        _append_unique(commands, ["just", "docs"])
    if rust_changed:
        _append_unique(commands, ["cargo", "fmt", "--all", "--", "--check"])
        for crate in change.crates:
            _append_unique(commands, ["cargo", "test", "-p", crate])
        _append_unique(commands, ["just", "check"])
    if plugin_changed:
        _append_unique(
            commands,
            [sys.executable, "-m", "unittest", "discover", "-s", "plugins/gaanim-dev/tests"],
        )
        _append_unique(commands, [sys.executable, "plugins/gaanim-dev/scripts/audit.py"])
    return commands


def _profile_commands(
    repo: Path, profile: str, change: impact_tool.Impact
) -> list[list[str]]:
    if profile == "fast":
        return _fast_commands(repo, change)
    if profile == "api":
        commands = _fast_commands(repo, change)
        for command in (
            ["just", "clippy"],
            ["just", "python-develop"],
            ["just", "validate-python-api"],
            ["just", "docs"],
            [sys.executable, "plugins/gaanim-dev/scripts/audit.py"],
        ):
            _append_unique(commands, command)
        return commands
    if profile == "full":
        return [
            ["cargo", "fmt", "--all", "--", "--check"],
            ["cargo", "test", "--workspace"],
            ["cargo", "clippy", "--workspace", "--all-targets"],
            ["just", "python-develop"],
            ["just", "validate-python-api"],
            ["just", "docs"],
            [sys.executable, "-m", "unittest", "discover", "-s", "plugins/gaanim-dev/tests"],
            [sys.executable, "plugins/gaanim-dev/scripts/audit.py"],
        ]
    return []


def _visual(
    repo: Path,
    examples: list[str],
    dry_run: bool,
    bless: bool,
) -> int:
    if not examples:
        print("SKIP: no visual examples were selected or inferred", file=sys.stderr)
        return 2
    if shutil.which("cargo") is None:
        print("SKIP: cargo is unavailable, so the visual runner cannot be built", file=sys.stderr)
        return 2

    build = ["cargo", "build", "-p", "gaanim_editor", "--bin", "gaanim"]
    if _run(repo, build, dry_run) != 0:
        return 1

    runner = repo / "target" / "debug" / ("gaanim.exe" if os.name == "nt" else "gaanim")
    if not dry_run and not runner.is_file():
        print(f"SKIP: visual runner was not produced at {runner}", file=sys.stderr)
        return 2

    status = 0
    runner_arg = str(runner)
    for example in examples:
        source = repo / "examples" / f"{example}.py"
        baseline = repo / "tests" / "visual" / example / "baseline"
        if not source.is_file() or (not bless and not baseline.is_dir()):
            print(f"SKIP: {example} has no source or approved baseline", file=sys.stderr)
            status = 2 if status == 0 else status
            continue
        command = [runner_arg, "--diff", "--example", f"examples/{example}.py"]
        if bless:
            command.append("--bless")
        command.append("--no-gui")
        result = _run(repo, command, dry_run)
        if result != 0:
            report = repo / "tests" / "visual" / example / "report" / "index.html"
            print(f"Visual report: {report}", file=sys.stderr)
            status = 1
    return status


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("profile", choices=("fast", "api", "visual", "full"))
    parser.add_argument("--repo", type=Path, default=Path.cwd())
    parser.add_argument("--base")
    parser.add_argument("--example", action="append", default=[])
    parser.add_argument("--dry-run", action="store_true")
    parser.add_argument("--bless", action="store_true", help="Update selected visual baselines")
    parser.add_argument(
        "--allow-bless",
        action="store_true",
        help="Confirm that the user explicitly approved baseline replacement",
    )
    args = parser.parse_args(argv)

    if args.bless and not args.allow_bless:
        parser.error("--bless requires --allow-bless after explicit user approval")

    try:
        repo = impact_tool.find_repo_root(args.repo)
        change = impact_tool.analyze_paths(repo, impact_tool.changed_files(repo, args.base))
        explicit_examples = [_normalize_example(value) for value in args.example]
    except (FileNotFoundError, RuntimeError, ValueError) as error:
        print(f"verify: {error}", file=sys.stderr)
        return 2

    commands = _profile_commands(repo, args.profile, change)
    if not commands and args.profile != "visual":
        print("No checks selected for the current change.")
    for command in commands:
        status = _run(repo, command, args.dry_run)
        if status != 0:
            return status

    if args.profile == "visual":
        examples = list(dict.fromkeys(explicit_examples or change.visual_examples))
        return _visual(repo, examples, args.dry_run, args.bless)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
