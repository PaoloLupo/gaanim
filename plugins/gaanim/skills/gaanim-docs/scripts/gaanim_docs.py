"""Locate, index, and search the Gaanim documentation that matches a project.

The authoring wheel ships the Typst sources under ``gaanim/_docs``. This tool
finds them without importing ``gaanim`` (the package refuses to import outside
the Gaanim application) and without third-party dependencies.

Resolution order:
1. ``--docs DIR`` or the ``GAANIM_DOCS`` environment variable.
2. The nearest ``.venv`` at or above ``--project`` (default: cwd).
3. The ``gaanim`` package visible to the current interpreter.
4. A Gaanim repository checkout at or above ``--project``.
"""

from __future__ import annotations

import argparse
import importlib.util
import os
import re
import sys
from dataclasses import dataclass
from pathlib import Path

DOC_SUFFIXES = (".typ", ".py")
INCLUDE = re.compile(r'^#include\s+"([^"]+)"', re.MULTILINE)
PART = re.compile(r'^#book-part\("([^"]+)",\s*"([^"]+)"', re.MULTILINE)
TITLE = re.compile(r'\btitle:\s*"([^"]*)"')
DESCRIPTION = re.compile(r'\bdescription:\s*"([^"]*)"')


@dataclass(frozen=True)
class DocsLocation:
    root: Path
    source: str
    version: str | None = None
    stubs: Path | None = None


def _is_docs_root(path: Path) -> bool:
    return (path / "index.typ").is_file()


def _dist_version(site_packages: Path) -> str | None:
    for metadata in sorted(site_packages.glob("gaanim-*.dist-info/METADATA")):
        for line in metadata.read_text(encoding="utf-8", errors="replace").splitlines():
            if line.startswith("Version:"):
                return line.split(":", 1)[1].strip()
    return None


def _checkout_version(repo: Path) -> str | None:
    pyproject = repo / "crates" / "gaanim_python" / "pyproject.toml"
    if pyproject.is_file():
        match = re.search(r'^version\s*=\s*"([^"]+)"', pyproject.read_text(encoding="utf-8"), re.MULTILINE)
        if match:
            return match.group(1)
    return None


def _from_package_dir(package: Path, site_packages: Path | None, source: str) -> DocsLocation | None:
    """Docs for a ``gaanim`` package directory: bundled copy or editable checkout."""
    version = _dist_version(site_packages) if site_packages else None
    bundled = package / "_docs"
    if _is_docs_root(bundled):
        return DocsLocation(bundled, source, version, package)
    # Editable checkout: crates/gaanim_python/gaanim -> <repo>/docs/content.
    repo = package.resolve().parents[2]
    checkout = repo / "docs" / "content"
    if _is_docs_root(checkout):
        return DocsLocation(
            checkout, f"{source} (editable checkout)", version or _checkout_version(repo), package
        )
    return None


def _site_packages(venv: Path) -> list[Path]:
    return [*venv.glob("Lib/site-packages"), *venv.glob("lib/python*/site-packages")]


def _from_site_packages(site_packages: Path) -> DocsLocation | None:
    package = site_packages / "gaanim"
    if (package / "__init__.py").is_file():
        return _from_package_dir(package, site_packages, f"installed package in {site_packages}")
    # Editable installs register the source directory through a .pth file.
    for pth in sorted(site_packages.glob("*gaanim*.pth")):
        for line in pth.read_text(encoding="utf-8", errors="replace").splitlines():
            line = line.strip()
            if not line or line.startswith(("#", "import ")):
                continue
            candidate = Path(line) / "gaanim"
            if (candidate / "__init__.py").is_file():
                return _from_package_dir(candidate, site_packages, f"editable install in {site_packages}")
    return None


def _ancestors(start: Path) -> list[Path]:
    start = start.resolve()
    return [start, *start.parents]


def locate(project: Path, explicit: str | None = None) -> DocsLocation | None:
    override = explicit or os.environ.get("GAANIM_DOCS")
    if override:
        path = Path(override).expanduser()
        return DocsLocation(path.resolve(), "explicit override") if _is_docs_root(path) else None

    for directory in _ancestors(project):
        venv = directory / ".venv"
        if venv.is_dir():
            for site_packages in _site_packages(venv):
                found = _from_site_packages(site_packages)
                if found:
                    return found

    spec = importlib.util.find_spec("gaanim")
    if spec and spec.submodule_search_locations:
        package = Path(next(iter(spec.submodule_search_locations)))
        found = _from_package_dir(package, package.parent, f"interpreter {sys.executable}")
        if found:
            return found

    for directory in _ancestors(project):
        checkout = directory / "docs" / "content"
        if _is_docs_root(checkout) and (directory / "crates" / "gaanim_python").is_dir():
            package = directory / "crates" / "gaanim_python" / "gaanim"
            return DocsLocation(checkout, "repository checkout", _checkout_version(directory), package)
    return None


def _page_summary(path: Path) -> str:
    head = path.read_text(encoding="utf-8", errors="replace")[:2000]
    title = TITLE.search(head)
    description = DESCRIPTION.search(head)
    parts = [title.group(1) if title else path.stem]
    if description:
        parts.append(description.group(1))
    return " — ".join(parts)


def index(root: Path) -> list[str]:
    lines: list[str] = []
    listed: set[Path] = set()
    main = (root / "index.typ").read_text(encoding="utf-8", errors="replace")
    events = sorted(
        [(m.start(), "part", m) for m in PART.finditer(main)]
        + [(m.start(), "include", m) for m in INCLUDE.finditer(main)],
        key=lambda event: event[0],
    )
    for _, kind, match in events:
        if kind == "part":
            lines.append(f"\n## Parte {match.group(1)}: {match.group(2)}")
            continue
        page = (root / match.group(1)).resolve()
        if page.is_file():
            listed.add(page)
            lines.append(f"{match.group(1)}  {_page_summary(page)}")
    extra = [
        path
        for path in sorted(root.rglob("*"))
        if path.is_file()
        and path.suffix in DOC_SUFFIXES
        and path.resolve() not in listed
        and path != root / "index.typ"
        and "__pycache__" not in path.parts
    ]
    if extra:
        lines.append("\n## Otros archivos")
        lines.extend(path.relative_to(root).as_posix() for path in extra)
    return lines


def search(root: Path, pattern: str, limit: int) -> list[str]:
    regex = re.compile(pattern, re.IGNORECASE)
    matches: list[str] = []
    for path in sorted(root.rglob("*")):
        if not path.is_file() or path.suffix not in DOC_SUFFIXES or "__pycache__" in path.parts:
            continue
        text = path.read_text(encoding="utf-8", errors="replace")
        for number, line in enumerate(text.splitlines(), start=1):
            if regex.search(line):
                matches.append(f"{path.relative_to(root).as_posix()}:{number}: {line.strip()}")
                if len(matches) >= limit:
                    return matches
    return matches


def main(argv: list[str] | None = None) -> int:
    if hasattr(sys.stdout, "reconfigure"):
        sys.stdout.reconfigure(encoding="utf-8")
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--project", type=Path, default=Path.cwd(), help="project directory (default: cwd)")
    parser.add_argument("--docs", help="use this docs/content directory directly")
    commands = parser.add_subparsers(dest="command")
    commands.add_parser("locate", help="print the documentation root and its origin")
    commands.add_parser("index", help="list pages in reading order with titles")
    finder = commands.add_parser("search", help="case-insensitive regex search over .typ and .py docs")
    finder.add_argument("pattern")
    finder.add_argument("--limit", type=int, default=80)
    args = parser.parse_args(argv)

    found = locate(args.project, args.docs)
    if found is None:
        print(
            "Gaanim documentation not found. Install the `gaanim` authoring package in the "
            "project's .venv (for example `uv sync`), or pass --docs / set GAANIM_DOCS.",
            file=sys.stderr,
        )
        return 1

    command = args.command or "locate"
    if command == "locate":
        print(f"root: {found.root}")
        print(f"source: {found.source}")
        print(f"version: {found.version or 'unknown'}")
        if found.stubs and (found.stubs / "gaanim_core.pyi").is_file():
            print(f"stubs: {found.stubs}")
        return 0
    if command == "index":
        print(f"# Gaanim docs ({found.version or 'unknown version'}) at {found.root}")
        print("\n".join(index(found.root)))
        return 0
    results = search(found.root, args.pattern, args.limit)
    print("\n".join(results) if results else f"No matches for {args.pattern!r} in {found.root}")
    return 0 if results else 1


if __name__ == "__main__":
    raise SystemExit(main())
