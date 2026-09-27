#!/usr/bin/env python3
"""Write the third-party notices shipped with the Gaanim release binaries.

The notices cover every crate compiled into ``gaanim``, ``gaanim-core`` and
``gaanim-play`` for the host platform, plus the third-party assets embedded in
them. Build-only dependencies (build scripts and procedural macros) do not end
up in the binaries and are left out. Identical license texts are printed once,
with the crates that carry them.

Usage: python scripts/third_party_licenses.py [--output FILE] [--target TRIPLE]
"""

from __future__ import annotations

import argparse
import json
import re
import subprocess
import sys
from collections import defaultdict
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
# Packages whose binaries are distributed.
SHIPPED_PACKAGES = ("gaanim_editor", "gaanim_launcher")
LICENSE_FILE = re.compile(
    r"^(licen[cs]es?|copying|copyright|notice|unlicense|authors|ofl|ufl)([-_.].*)?$",
    re.IGNORECASE,
)
# Subdirectories searched too: license folders, and the font folders of
# crates that embed fonts (their OFL or UFL texts live there).
LICENSE_DIRS = re.compile(r"^(licen[cs]es?|fonts?|assets)$", re.IGNORECASE)
# Third-party assets compiled into the binaries, with their license texts.
EMBEDDED_ASSETS = (
    (
        "DejaVu Sans Bold (crates/gaanim_objects/assets/fonts/DejaVuSans-Bold.ttf)",
        ROOT / "crates/gaanim_objects/assets/fonts/LICENSE-DejaVu.txt",
    ),
)


def host_triple() -> str:
    output = subprocess.run(
        ["rustc", "-vV"], check=True, capture_output=True, text=True
    ).stdout
    return next(
        line.split(":", 1)[1].strip()
        for line in output.splitlines()
        if line.startswith("host:")
    )


def cargo_metadata(target: str) -> dict:
    output = subprocess.run(
        [
            "cargo",
            "metadata",
            "--format-version",
            "1",
            "--filter-platform",
            target,
        ],
        cwd=ROOT,
        check=True,
        capture_output=True,
        text=True,
    ).stdout
    return json.loads(output)


def shipped_packages(metadata: dict) -> list[dict]:
    """Third-party packages linked into the shipped binaries."""
    packages = {package["id"]: package for package in metadata["packages"]}
    nodes = {node["id"]: node for node in metadata["resolve"]["nodes"]}
    workspace = set(metadata["workspace_members"])
    roots = [
        package_id
        for package_id in workspace
        if packages[package_id]["name"] in SHIPPED_PACKAGES
    ]
    missing = set(SHIPPED_PACKAGES) - {packages[root]["name"] for root in roots}
    if missing:
        raise SystemExit(f"shipped packages not found: {', '.join(sorted(missing))}")

    seen: set[str] = set()
    stack = list(roots)
    while stack:
        package_id = stack.pop()
        if package_id in seen:
            continue
        seen.add(package_id)
        for dependency in nodes[package_id]["deps"]:
            # A `null` kind is a normal dependency; dev and build ones are
            # not linked into the binary.
            if any(kind["kind"] is None for kind in dependency["dep_kinds"]):
                stack.append(dependency["pkg"])

    shipped = []
    for package_id in seen - workspace:
        package = packages[package_id]
        kinds = {kind for target in package["targets"] for kind in target["kind"]}
        if kinds == {"proc-macro"}:
            continue
        shipped.append(package)
    return sorted(shipped, key=lambda package: (package["name"], package["version"]))


def license_texts(package: dict) -> list[tuple[str, str]]:
    directory = Path(package["manifest_path"]).parent
    folders = [directory] + [
        path for path in directory.iterdir() if path.is_dir() and LICENSE_DIRS.match(path.name)
    ]
    files = {
        path
        for folder in folders
        for path in folder.iterdir()
        if path.is_file() and LICENSE_FILE.match(path.name)
    }
    if package.get("license_file"):
        explicit = directory / package["license_file"]
        if explicit.is_file():
            files.add(explicit)
    texts = []
    for path in sorted(files):
        text = path.read_text(encoding="utf-8", errors="replace").strip()
        if text:
            texts.append((path.relative_to(directory).as_posix(), text))
    return texts


def normalized(text: str) -> str:
    return re.sub(r"\s+", " ", text).strip()


def render(packages: list[dict], target: str, version: str) -> str:
    rule = "=" * 78
    lines = [
        f"Avisos de terceros de Gaanim {version} ({target})",
        rule,
        "",
        "Gaanim se distribuye bajo MIT OR Apache-2.0 (LICENSE-MIT, LICENSE-APACHE).",
        "Los binarios incluyen además los componentes de terceros listados aquí,",
        "cada uno bajo su propia licencia. Las licencias duales permiten elegir",
        "cualquiera de las opciones indicadas.",
        "",
        rule,
        "Recursos incluidos",
        rule,
    ]
    for name, path in EMBEDDED_ASSETS:
        lines += ["", f"--- {name}", "", path.read_text(encoding="utf-8").strip()]

    lines += ["", rule, f"Paquetes de Rust ({len(packages)})", rule, ""]
    by_text: dict[str, dict] = {}
    without_files = []
    for package in packages:
        label = f"{package['name']} {package['version']}"
        expression = package.get("license") or "sin declarar"
        repository = package.get("repository") or ""
        lines.append(f"{label} — {expression}" + (f" — {repository}" if repository else ""))
        texts = license_texts(package)
        if not texts:
            without_files.append(package)
        for file_name, text in texts:
            entry = by_text.setdefault(normalized(text), {"text": text, "users": []})
            entry["users"].append(f"{label} ({file_name})")

    lines += ["", rule, f"Textos de licencia ({len(by_text)} distintos)", rule]
    for index, entry in enumerate(
        sorted(by_text.values(), key=lambda entry: entry["users"][0]), 1
    ):
        lines += ["", f"--- Texto {index}. Lo usan:"]
        lines += [f"    {user}" for user in entry["users"]]
        lines += ["", entry["text"]]

    if without_files:
        lines += [
            "",
            rule,
            "Paquetes que no incluyen el texto de su licencia",
            rule,
            "",
            "Estos paquetes declaran su licencia sin incluir su texto. Los textos",
            "SPDX están en https://spdx.org/licenses/; los de MIT y Apache-2.0",
            "también en LICENSE-MIT y LICENSE-APACHE junto a este archivo.",
            "",
        ]
        by_expression: dict[str, list[str]] = defaultdict(list)
        for package in without_files:
            authors = ", ".join(package.get("authors") or []) or "autores sin declarar"
            by_expression[package.get("license") or "sin declarar"].append(
                f"{package['name']} {package['version']} — {authors}"
            )
        for expression in sorted(by_expression):
            lines += [f"{expression}:"] + [f"    {entry}" for entry in by_expression[expression]]
    return "\n".join(lines) + "\n"


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--output", type=Path, default=ROOT / "THIRD-PARTY-NOTICES.txt")
    parser.add_argument("--target", help="Rust target triple (default: host)")
    args = parser.parse_args()

    target = args.target or host_triple()
    metadata = cargo_metadata(target)
    version = next(
        package["version"]
        for package in metadata["packages"]
        if package["name"] == "gaanim_editor" and package["id"] in metadata["workspace_members"]
    )
    packages = shipped_packages(metadata)
    undeclared = [package["name"] for package in packages if not package.get("license")]
    if undeclared:
        print(f"warning: no declared license: {', '.join(undeclared)}", file=sys.stderr)
    args.output.write_text(render(packages, target, version), encoding="utf-8")
    print(f"{len(packages)} third-party packages -> {args.output}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
