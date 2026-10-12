#!/usr/bin/env python3
"""Check the examples against their style contract and keep their README current.

    python scripts/examples.py check [--strict]   contract, catalog and README
    python scripts/examples.py lint PATH... [--allow RULE...]
                                                  contract of scripts not yet in the catalog
    python scripts/examples.py readme             rewrite examples/README.md
    python scripts/examples.py path NAME          print the script NAME refers to
    python scripts/examples.py run NAME --binary GAANIM
                                                  open NAME in a dev build (`just run`)

The catalog, examples/examples.toml, lists every example that follows the
contract described in AGENTS.md ("Examples"). Scripts it does not list yet are
legacy: `check` counts them, and `--strict` fails on them.

NAME is a path relative to examples/ (with or without `.py`) or a file name,
which is unique across the tree.
"""

from __future__ import annotations

import argparse
import ast
import re
import subprocess
import sys
import tomllib
from dataclasses import dataclass, field
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
EXAMPLES = ROOT / "examples"
CATALOG = EXAMPLES / "examples.toml"
README = EXAMPLES / "README.md"

# Scene options an example may only set when they are its subject.
SCENE_OPTIONS = ("background", "frame", "margin", "theme", "post")
ALLOW = set(SCENE_OPTIONS) | {"colors", "titles"}
COLOR_CONSTANTS = {
    "BLACK", "BLUE", "CORAL", "CYAN", "GOLD", "GRAY", "GREEN", "NAVY",
    "ORANGE", "PINK", "PURPLE", "RED", "TEAL", "WHITE", "YELLOW", "tailwind",
}  # fmt: skip
COLOR_LITERAL = re.compile(r"^\s*(#[0-9a-fA-F]{3,8}|(rgb|rgba|hsl|hsla|oklch|oklab|lab|lch)\(.*\))\s*$")
TITLE_ROLES = {"title", "subtitle", "kicker", "heading"}
TITLE_FACTORIES = {"title_card", "section_header"}
OUTSIDE_PATH = re.compile(r"(^|[\\/])\.\.([\\/]|$)|(^|[\\/])(tests|docs)[\\/]")
EXAMPLE_KEYS = {"path", "description", "title", "web", "allow"}
TOPIC_KEYS = {"id", "title", "description"}


@dataclass
class Example:
    path: str
    description: str
    title: str
    web: bool = True
    allow: set[str] = field(default_factory=set)

    @property
    def topic(self) -> str:
        return self.path.split("/")[0] if "/" in self.path else ""

    @property
    def name(self) -> str:
        return Path(self.path).stem


@dataclass
class Catalog:
    topics: list[dict]
    examples: list[Example]


def load_catalog(errors: list[str]) -> Catalog:
    data = tomllib.loads(CATALOG.read_text(encoding="utf-8"))
    topics = data.get("topic", [])
    ids = [topic.get("id") for topic in topics]
    for topic in topics:
        if unknown := set(topic) - TOPIC_KEYS:
            errors.append(f"examples.toml: topic {topic.get('id')!r} has unknown keys {sorted(unknown)}")
        if not topic.get("id") or not topic.get("title") or not topic.get("description"):
            errors.append(f"examples.toml: topic {topic!r} needs an id, a title and a description")
    if len(set(ids)) != len(ids):
        errors.append("examples.toml: topic ids repeat")
    examples = []
    seen = set()
    for entry in data.get("example", []):
        path = entry.get("path", "")
        if unknown := set(entry) - EXAMPLE_KEYS:
            errors.append(f"examples.toml: {path} has unknown keys {sorted(unknown)}")
        if not path or not entry.get("description"):
            errors.append(f"examples.toml: every example needs a path and a description ({entry!r})")
            continue
        if path in seen:
            errors.append(f"examples.toml: {path} is listed twice")
        seen.add(path)
        allow = set(entry.get("allow", []))
        if unknown := allow - ALLOW:
            errors.append(f"examples.toml: {path} allows unknown rules {sorted(unknown)}")
        title = entry.get("title") or Path(path).stem.replace("_", " ").capitalize()
        example = Example(path, entry["description"], title, entry.get("web", True), allow)
        if example.topic and example.topic not in ids:
            errors.append(f"examples.toml: {path} is in folder {example.topic!r}, which is not a topic")
        if not (EXAMPLES / path).is_file():
            errors.append(f"examples.toml: {path} does not exist")
        examples.append(example)
    return Catalog(topics, examples)


# The repository quickstart stays at the top of examples/ and is not legacy.
QUICKSTART = "quickstart.py"


def scripts() -> list[Path]:
    """Every example script, skipping the insides of project folders and assets."""
    projects = [manifest.parent for manifest in EXAMPLES.rglob("gaanim.toml")]
    found = []
    for path in sorted(EXAMPLES.rglob("*.py")):
        if "assets" in path.relative_to(EXAMPLES).parts:
            continue
        if any(project in path.parents for project in projects):
            continue
        found.append(path)
    return found


def relative(path: Path) -> str:
    return path.relative_to(EXAMPLES).as_posix()


class Contract(ast.NodeVisitor):
    """Collect the style-contract violations of one example."""

    def __init__(self, allow: set[str]) -> None:
        self.allow = allow
        self.problems: list[str] = []

    def problem(self, node: ast.AST, rule: str, message: str) -> None:
        if rule not in self.allow:
            self.problems.append(f"line {getattr(node, 'lineno', '?')}: {message} [{rule}]")

    def visit_ImportFrom(self, node: ast.ImportFrom) -> None:
        if node.module and node.module.split(".")[0] == "gaanim":
            for alias in node.names:
                if alias.name in COLOR_CONSTANTS:
                    self.problem(node, "colors", f"imports the color constant {alias.name}; use theme colors")
        self.generic_visit(node)

    def visit_Call(self, node: ast.Call) -> None:
        callee = node.func
        name = callee.id if isinstance(callee, ast.Name) else callee.attr if isinstance(callee, ast.Attribute) else ""
        if name == "Scene":
            for keyword in node.keywords:
                if keyword.arg in SCENE_OPTIONS:
                    self.problem(node, keyword.arg, f"Scene({keyword.arg}=...) changes the shared look")
                elif keyword.arg is not None and keyword.arg != "design_resolution":
                    self.problem(node, keyword.arg, f"Scene({keyword.arg}=...) is not a known option")
        if name == "segment":
            for keyword in node.keywords:
                if keyword.arg in ("background", "post"):
                    self.problem(node, keyword.arg, f"segment({keyword.arg}=...) changes the shared look")
        # Canvas.set_theme changes the scene's theme; Lottie.set_theme only
        # picks a theme of a dotLottie package.
        if name == "set_theme" and isinstance(callee, ast.Attribute) and ast.unparse(callee.value).endswith("canvas"):
            self.problem(node, "theme", "set_theme() changes the shared theme")
        if name == "set_margin":
            self.problem(node, "margin", "set_margin() changes the shared safe area")
        if name == "Color" or (isinstance(callee, ast.Attribute) and isinstance(callee.value, ast.Name)
                               and callee.value.id == "Color"):
            self.problem(node, "colors", "builds a Color; use theme colors")
        if name in TITLE_FACTORIES:
            self.problem(node, "titles", f"{name}() puts a title in the frame")
        for keyword in node.keywords:
            if (keyword.arg == "role" and isinstance(keyword.value, ast.Constant)
                    and keyword.value.value in TITLE_ROLES):
                self.problem(node, "titles", f'text with role="{keyword.value.value}" titles the frame')
        if name == "print":
            self.problems.append(f"line {node.lineno}: print() belongs in tests, not in an example")
        self.generic_visit(node)

    def visit_Assign(self, node: ast.Assign) -> None:
        for target in node.targets:
            if (isinstance(target, ast.Attribute) and target.attr in ("background", "post")
                    and isinstance(target.value, ast.Attribute) and target.value.attr == "canvas"):
                self.problem(node, target.attr, f"assigns scene.canvas.{target.attr}")
        self.generic_visit(node)

    def visit_Constant(self, node: ast.Constant) -> None:
        if isinstance(node.value, str):
            if COLOR_LITERAL.match(node.value):
                self.problem(node, "colors", f"color literal {node.value!r}; use theme colors")
            if OUTSIDE_PATH.search(node.value):
                self.problems.append(f"line {node.lineno}: path {node.value!r} reaches outside the example's folder")

    def visit_Name(self, node: ast.Name) -> None:
        if node.id == "__file__":
            self.problems.append(f"line {node.lineno}: __file__; load assets by relative name from assets/")

    def visit_Assert(self, node: ast.Assert) -> None:
        self.problems.append(f"line {node.lineno}: assert belongs in tests, not in an example")
        self.generic_visit(node)


def canonical_tail(tree: ast.Module) -> bool:
    """True when the script ends with the snapshots-or-render block."""
    expected = ast.dump(ast.parse(
        'if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):\n'
        "    scene.snapshots(snapshots, SEEKS)\n"
        "else:\n"
        "    scene.render()\n"
    ).body[0])
    last = tree.body[-1] if tree.body else None
    if not isinstance(last, ast.If) or len(last.body) != 1:
        return False
    call = getattr(last.body[0], "value", None)
    if not isinstance(call, ast.Call) or len(call.args) != 2:
        return False
    # Compare with the seeks argument normalized away.
    shaped = ast.parse(ast.unparse(last))
    shaped.body[0].body[0].value.args[1] = ast.Name("SEEKS", ast.Load())
    return ast.dump(shaped.body[0]) == expected


def check_script(example: Example) -> list[str]:
    path = EXAMPLES / example.path
    source = path.read_text(encoding="utf-8")
    tree = ast.parse(source, filename=str(path))
    problems = []
    docstring = ast.get_docstring(tree)
    if not docstring:
        problems.append("missing module docstring")
    elif not docstring.splitlines()[0].rstrip().endswith("."):
        problems.append("the docstring's first line should be a sentence ending with a period")
    scenes = [
        node for node in ast.walk(tree)
        if isinstance(node, ast.Assign) and isinstance(node.value, ast.Call)
        and isinstance(node.value.func, ast.Name) and node.value.func.id == "Scene"
    ]
    if len(scenes) != 1 or [getattr(target, "id", None) for target in scenes[0].targets] != ["scene"]:
        problems.append("expected exactly one `scene = Scene(...)`")
    if not canonical_tail(tree):
        problems.append(
            'must end with `if snapshots := os.environ.get("GAANIM_SNAPSHOTS"): '
            "scene.snapshots(snapshots, [...])` `else: scene.render()`"
        )
    contract = Contract(example.allow)
    contract.visit(tree)
    return problems + contract.problems


def readme(catalog: Catalog, legacy: int) -> str:
    lines = [
        "# Examples",
        "",
        "<!-- Generated from examples.toml by `python scripts/examples.py readme`; do not edit. -->",
        "",
        "Each example is a complete scene about one topic. It uses the default theme",
        "and shows the variants the API offers for that topic, so it doubles as a",
        "reference: the labels in the frame are the code that produced each variant.",
        "",
        "Run one from a checkout with `just run <name>`, for example `just run easings`,",
        "or open it with `gaanim examples/<topic>/<name>.py`.",
        "",
    ]
    by_topic: dict[str, list[Example]] = {}
    for example in catalog.examples:
        by_topic.setdefault(example.topic, []).append(example)
    topics = [topic for topic in catalog.topics if topic["id"] in by_topic]
    if topics:
        lines += ["## Topics", ""]
        lines += [f"- [{topic['title']}](#{anchor(topic['title'])})" for topic in topics]
        lines.append("")
    for topic in topics:
        lines += [f"## {topic['title']}", "", topic["description"], "", "| Example | Description |", "| --- | --- |"]
        for example in by_topic[topic["id"]]:
            lines.append(f"| [{example.title}]({example.path}) | {example.description} |")
        lines.append("")
    if legacy:
        lines += [
            "## Not yet migrated",
            "",
            f"{legacy} older scripts still sit at the top of this folder. They are being",
            "rewritten into the topics above and keep working until then.",
            "",
        ]
    return "\n".join(lines)


def anchor(title: str) -> str:
    return re.sub(r"[^a-z0-9 -]", "", title.lower()).replace(" ", "-")


def resolve(name: str) -> Path:
    candidate = name.removeprefix("examples/").removesuffix(".py")
    direct = EXAMPLES / f"{candidate}.py"
    if direct.is_file():
        return direct
    project = EXAMPLES / candidate
    if (project / "gaanim.toml").is_file():
        return project
    # A project folder by its own name, in whichever topic it sits.
    projects = [manifest.parent for manifest in EXAMPLES.rglob("gaanim.toml") if manifest.parent.name == candidate]
    if len(projects) == 1:
        return projects[0]
    matches = [path for path in scripts() if path.stem == Path(candidate).name]
    if len(matches) == 1:
        return matches[0]
    if matches:
        listed = ", ".join(relative(path) for path in matches)
        raise SystemExit(f"{name!r} matches several examples: {listed}")
    raise SystemExit(f"no example named {name!r} under examples/")


def check(strict: bool) -> int:
    errors: list[str] = []
    catalog = load_catalog(errors)
    listed = {example.path for example in catalog.examples}
    for example in catalog.examples:
        if (EXAMPLES / example.path).is_file():
            errors += [f"{example.path}: {problem}" for problem in check_script(example)]
    every = scripts()
    stems: dict[str, list[str]] = {}
    for path in every:
        stems.setdefault(path.stem, []).append(relative(path))
    for stem, paths in sorted(stems.items()):
        if len(paths) > 1:
            errors.append(f"file name {stem!r} repeats: {', '.join(paths)}")
    legacy = [relative(path) for path in every if relative(path) not in listed | {QUICKSTART}]
    if strict:
        errors += [f"{path}: not in examples.toml" for path in legacy]
    expected = readme(catalog, len(legacy))
    if not README.is_file() or README.read_text(encoding="utf-8") != expected:
        errors.append("examples/README.md is out of date; run `python scripts/examples.py readme`")
    for error in errors:
        print(f"error: {error}")
    print(f"{len(catalog.examples)} examples follow the contract; {len(legacy)} legacy scripts remain.")
    return 1 if errors else 0


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    commands = parser.add_subparsers(dest="command", required=True)
    check_parser = commands.add_parser("check", help="check the contract, the catalog and the README")
    check_parser.add_argument("--strict", action="store_true", help="also fail on scripts the catalog does not list")
    lint_parser = commands.add_parser("lint", help="check scripts against the contract before cataloguing them")
    lint_parser.add_argument("paths", nargs="+", type=Path)
    lint_parser.add_argument("--allow", nargs="*", default=[], choices=sorted(ALLOW))
    commands.add_parser("readme", help="rewrite examples/README.md from the catalog")
    path_parser = commands.add_parser("path", help="print the script a name refers to")
    path_parser.add_argument("name")
    run_parser = commands.add_parser("run", help="open an example in a dev build of gaanim")
    run_parser.add_argument("name")
    run_parser.add_argument("--binary", required=True, help="the gaanim executable to run it with")
    args = parser.parse_args()
    if args.command == "check":
        return check(args.strict)
    if args.command == "lint":
        failed = False
        for path in args.paths:
            example = Example(relative(path.resolve()), "", "", allow=set(args.allow))
            problems = check_script(example)
            failed |= bool(problems)
            print(f"{example.path}: {'ok' if not problems else ''}")
            for problem in problems:
                print(f"  {problem}")
        return 1 if failed else 0
    if args.command == "readme":
        errors: list[str] = []
        catalog = load_catalog(errors)
        if errors:
            print("\n".join(f"error: {error}" for error in errors))
            return 1
        listed = {example.path for example in catalog.examples}
        legacy = sum(relative(path) not in listed | {QUICKSTART} for path in scripts())
        README.write_text(readme(catalog, legacy), encoding="utf-8", newline="\n")
        print(f"wrote {README.relative_to(ROOT).as_posix()}")
        return 0
    path = resolve(args.name).relative_to(ROOT).as_posix()
    if args.command == "path":
        print(path)
        return 0
    # Through dev.py, which puts the dynamically linked engine on the search path.
    binary = str((ROOT / args.binary).resolve())
    return subprocess.call([sys.executable, str(ROOT / "scripts" / "dev.py"), "exec", binary, path], cwd=ROOT)


if __name__ == "__main__":
    sys.exit(main())
