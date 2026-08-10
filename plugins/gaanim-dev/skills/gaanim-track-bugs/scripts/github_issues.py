#!/usr/bin/env python3
"""Deterministic GitHub Issue operations for the Gaanim bug tracker skill."""

from __future__ import annotations

import argparse
import json
import re
import shutil
import subprocess
import sys
from collections.abc import Sequence
from typing import Any


DEFAULT_REPOSITORY = "PaoloLupo/gaanim"
BUG_LABEL = "bug"
PENDING = "Pendiente de determinar."
REPOSITORY_PATTERN = re.compile(r"^[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+$")


class TrackerError(RuntimeError):
    """An actionable failure that must stop the tracker workflow."""


def _gh_executable() -> str:
    executable = shutil.which("gh")
    if executable is None:
        raise TrackerError("GitHub CLI (`gh`) is not installed or is not on PATH.")
    return executable


def _run_gh(args: Sequence[str], *, input_text: str | None = None) -> str:
    try:
        completed = subprocess.run(
            [_gh_executable(), *args],
            input=input_text,
            capture_output=True,
            text=True,
            encoding="utf-8",
            errors="replace",
            check=False,
        )
    except OSError as error:
        raise TrackerError(f"Could not run GitHub CLI: {error}") from error

    if completed.returncode != 0:
        detail = completed.stderr.strip() or completed.stdout.strip() or "unknown error"
        raise TrackerError(f"GitHub CLI failed: {detail}")
    return completed.stdout.strip()


def _parse_json(raw: str, *, expected_type: type[Any]) -> Any:
    try:
        value = json.loads(raw)
    except json.JSONDecodeError as error:
        raise TrackerError("GitHub CLI returned malformed JSON.") from error
    if not isinstance(value, expected_type):
        raise TrackerError(
            f"GitHub CLI returned {type(value).__name__}; expected {expected_type.__name__}."
        )
    return value


def _run_json(args: Sequence[str], *, expected_type: type[Any]) -> Any:
    return _parse_json(_run_gh(args), expected_type=expected_type)


def _validate_repository(repository: str) -> str:
    repository = repository.strip()
    if not REPOSITORY_PATTERN.fullmatch(repository):
        raise TrackerError("Repository must use the OWNER/NAME form.")
    return repository


def _ensure_ready(repository: str) -> None:
    _run_gh(["auth", "status", "--hostname", "github.com"])
    details = _run_json(
        ["repo", "view", repository, "--json", "nameWithOwner"],
        expected_type=dict,
    )
    actual = details.get("nameWithOwner")
    if not isinstance(actual, str) or actual.casefold() != repository.casefold():
        raise TrackerError(f"Could not validate access to GitHub repository {repository}.")


def _required_text(value: str, field: str, *, maximum: int | None = None) -> str:
    value = value.strip()
    if not value:
        raise TrackerError(f"{field} must not be empty.")
    if maximum is not None and len(value) > maximum:
        raise TrackerError(f"{field} must be at most {maximum} characters.")
    return value


def _section(value: str | None) -> str:
    if value is None:
        return PENDING
    value = value.strip()
    return value or PENDING


def render_issue_body(
    *,
    summary: str | None,
    steps: str | None,
    actual: str,
    expected: str | None,
    evidence: str | None,
    environment: str | None,
) -> str:
    actual = _required_text(actual, "Observed behavior")
    sections = (
        ("Resumen", _section(summary)),
        ("Pasos para reproducir", _section(steps)),
        ("Resultado actual", actual),
        ("Resultado esperado", _section(expected)),
        ("Evidencia", _section(evidence)),
        ("Entorno", _section(environment)),
    )
    return "\n\n".join(f"## {heading}\n\n{content}" for heading, content in sections)


def create_issue(
    repository: str,
    *,
    title: str,
    actual: str,
    summary: str | None = None,
    steps: str | None = None,
    expected: str | None = None,
    evidence: str | None = None,
    environment: str | None = None,
    dry_run: bool = False,
) -> dict[str, Any]:
    repository = _validate_repository(repository)
    title = _required_text(title, "Title", maximum=256)
    body = render_issue_body(
        summary=summary,
        steps=steps,
        actual=actual,
        expected=expected,
        evidence=evidence,
        environment=environment,
    )
    preview: dict[str, Any] = {
        "dryRun": dry_run,
        "repository": repository,
        "title": title,
        "labels": [BUG_LABEL],
        "body": body,
    }
    if dry_run:
        return preview

    _ensure_ready(repository)
    url = _run_gh(
        [
            "issue",
            "create",
            "--repo",
            repository,
            "--title",
            title,
            "--label",
            BUG_LABEL,
            "--body-file",
            "-",
        ],
        input_text=body,
    )
    match = re.fullmatch(r"https://github\.com/[^/]+/[^/]+/issues/(\d+)/?", url)
    if match is None:
        raise TrackerError("GitHub CLI did not return a valid created issue URL.")
    return {
        **preview,
        "dryRun": False,
        "number": int(match.group(1)),
        "url": url,
    }


ISSUE_LIST_FIELDS = "number,title,state,updatedAt,url,labels"


def list_issues(repository: str, *, limit: int = 100) -> list[dict[str, Any]]:
    repository = _validate_repository(repository)
    _ensure_ready(repository)
    return _run_json(
        [
            "issue",
            "list",
            "--repo",
            repository,
            "--state",
            "open",
            "--label",
            BUG_LABEL,
            "--limit",
            str(limit),
            "--json",
            ISSUE_LIST_FIELDS,
        ],
        expected_type=list,
    )


def search_issues(
    repository: str, *, query: str, limit: int = 20
) -> list[dict[str, Any]]:
    repository = _validate_repository(repository)
    query = _required_text(query, "Search query")
    _ensure_ready(repository)
    return _run_json(
        [
            "issue",
            "list",
            "--repo",
            repository,
            "--state",
            "all",
            "--label",
            BUG_LABEL,
            "--search",
            query,
            "--limit",
            str(limit),
            "--json",
            ISSUE_LIST_FIELDS,
        ],
        expected_type=list,
    )


def show_issue(repository: str, *, number: int) -> dict[str, Any]:
    repository = _validate_repository(repository)
    if number <= 0:
        raise TrackerError("Issue number must be positive.")
    _ensure_ready(repository)
    return _run_json(
        [
            "issue",
            "view",
            str(number),
            "--repo",
            repository,
            "--comments",
            "--json",
            "number,title,body,state,url,labels,comments,author,createdAt,updatedAt",
        ],
        expected_type=dict,
    )


def _positive_integer(value: str) -> int:
    parsed = int(value)
    if parsed <= 0:
        raise argparse.ArgumentTypeError("must be a positive integer")
    return parsed


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repo", default=DEFAULT_REPOSITORY, help="GitHub OWNER/NAME")
    subparsers = parser.add_subparsers(dest="command", required=True)

    create = subparsers.add_parser("create", help="Preview or create a bug issue")
    create.add_argument("--title", required=True)
    create.add_argument("--actual", required=True, help="Observed behavior")
    create.add_argument("--summary")
    create.add_argument("--steps")
    create.add_argument("--expected")
    create.add_argument("--evidence")
    create.add_argument("--environment")
    create.add_argument("--dry-run", action="store_true")

    list_parser = subparsers.add_parser("list", help="List open bug issues")
    list_parser.add_argument("--limit", type=_positive_integer, default=100)

    search = subparsers.add_parser("search", help="Search open and closed bug issues")
    search.add_argument("--query", required=True)
    search.add_argument("--limit", type=_positive_integer, default=20)

    show = subparsers.add_parser("show", help="Show one issue with comments")
    show.add_argument("number", type=_positive_integer)
    return parser


def main(argv: Sequence[str] | None = None) -> int:
    args = build_parser().parse_args(argv)
    try:
        if args.command == "create":
            result: Any = create_issue(
                args.repo,
                title=args.title,
                actual=args.actual,
                summary=args.summary,
                steps=args.steps,
                expected=args.expected,
                evidence=args.evidence,
                environment=args.environment,
                dry_run=args.dry_run,
            )
        elif args.command == "list":
            result = list_issues(args.repo, limit=args.limit)
        elif args.command == "search":
            result = search_issues(args.repo, query=args.query, limit=args.limit)
        else:
            result = show_issue(args.repo, number=args.number)
    except TrackerError as error:
        print(f"error: {error}", file=sys.stderr)
        return 1

    json.dump(result, sys.stdout, ensure_ascii=False, indent=2)
    sys.stdout.write("\n")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
