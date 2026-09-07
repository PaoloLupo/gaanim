"""Run focused Cargo development commands with consistent Bevy dynamic linking.

Use `--dry-run` before the command to inspect it without invoking Cargo or Rust.
`exec` runs an existing binary or harness with the development DLL search paths.
"""

from __future__ import annotations

import json
import os
from pathlib import Path
import subprocess
import sys
import tomllib


ROOT = Path(__file__).resolve().parents[1]


def manifests(repo: Path) -> dict[str, dict]:
    workspace = tomllib.loads((repo / "Cargo.toml").read_text(encoding="utf-8"))
    return {
        data["package"]["name"]: data
        for member in workspace["workspace"]["members"]
        for data in [tomllib.loads((repo / member / "Cargo.toml").read_text(encoding="utf-8"))]
    }


def cargo_command(repo: Path, args: list[str]) -> list[str]:
    if not args or args[0] not in {"build", "check", "test", "clippy", "run", "tree"}:
        raise ValueError("Use build/check/test/clippy/run/tree with -p <crate> or --workspace.")
    options = args[1:args.index("--")] if "--" in args else args[1:]
    if any(arg == "--release" or arg == "-r" or arg.startswith("--profile") for arg in options):
        raise ValueError("This helper is for dev/test only; use the release recipes for release builds.")
    packages = manifests(repo)
    selected: set[str] = set()
    for index, arg in enumerate(options):
        if arg == "--workspace":
            selected.update(packages)
        elif arg in {"-p", "--package"}:
            if index + 1 == len(options):
                raise ValueError("Missing package name.")
            selected.add(options[index + 1])
        elif arg.startswith("--package="):
            selected.add(arg.split("=", 1)[1])
    if not selected or selected - packages.keys():
        raise ValueError("Select existing workspace package names with -p, or explicitly use --workspace.")
    features = [
        f"{name}/dev-dynamic"
        for name in sorted(selected)
        if "dev-dynamic" in packages[name].get("features", {})
    ]
    flags = ["--features", ",".join(features)] if features else []
    return ["cargo", args[0], *flags, *args[1:]]


def library_environment(repo: Path) -> dict[str, str]:
    # Metadata and --print only inspect configuration; neither compiles anything.
    metadata = json.loads(subprocess.check_output(
        ["cargo", "metadata", "--offline", "--no-deps", "--format-version", "1"],
        cwd=repo, text=True,
    ))
    rustlib = subprocess.check_output(
        ["rustc", "--print", "target-libdir"], cwd=repo, text=True,
    ).strip()
    debug = Path(metadata["target_directory"]) / "debug"
    key = "PATH" if os.name == "nt" else (
        "DYLD_FALLBACK_LIBRARY_PATH" if sys.platform == "darwin" else "LD_LIBRARY_PATH"
    )
    env = os.environ.copy()
    # Preserve Python, FFmpeg, and other caller-supplied search directories.
    previous = env.get(key, "")
    paths = [str(debug / "deps"), str(debug), rustlib]
    if previous:
        paths.append(previous)
    elif sys.platform == "darwin":
        paths.extend([str(Path.home() / "lib"), "/usr/local/lib", "/usr/lib"])
    env[key] = os.pathsep.join(paths)
    return env


def main(argv: list[str] | None = None) -> int:
    args = list(sys.argv[1:] if argv is None else argv)
    dry_run = bool(args and args[0] == "--dry-run")
    if dry_run:
        args.pop(0)
    try:
        is_exec = bool(args and args[0] == "exec")
        command = args[1:] if is_exec else cargo_command(ROOT, args)
        if not command:
            raise ValueError("exec requires a command.")
        print("+ " + subprocess.list2cmdline(command), flush=True)
        if dry_run:
            return 0
        # Cargo manages binary paths for run/test; adding Rust's library path
        # also covers doctests and subprocesses launched by test harnesses.
        needs_libraries = is_exec or args[0] in {"run", "test"}
        env = library_environment(ROOT) if needs_libraries else None
        return subprocess.run(command, cwd=ROOT, env=env, check=False).returncode
    except (ValueError, OSError, subprocess.CalledProcessError) as error:
        print(f"dev: {error}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
