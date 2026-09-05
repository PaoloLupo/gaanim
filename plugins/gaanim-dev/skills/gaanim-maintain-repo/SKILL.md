---
name: gaanim-maintain-repo
description: Audit and maintain the Gaanim repository's workspace, just commands, CI alignment, agent guidance, Python API contract, Typst documentation structure, generated artifacts, and dirty-worktree safety. Use for repository health checks, AGENTS.md or README maintenance, command drift, crate/workspace changes, CI workflow updates, cleanup planning, or preparing the repo for collaborative agent work.
---

# Maintain the Gaanim repository

Prefer evidence from live configuration and preserve user-owned work.
Resolve `PLUGIN_ROOT` as the directory two levels above this `SKILL.md`; invoke
bundled scripts by absolute path while keeping the Gaanim repository as cwd.

## Audit

1. Read `../../references/repo-map.md`.
2. Inspect `git status --short`; identify pre-existing modifications before
   proposing or making repository-wide edits.
3. Run `python <PLUGIN_ROOT>/scripts/audit.py`. Use `--format json` for automation and
   `--strict` only when the user or CI explicitly wants heuristic warnings to
   fail.
4. Run `python <PLUGIN_ROOT>/scripts/impact.py --format json` before changing manifests,
   commands, CI, public API, or documentation.
5. Resolve objective errors against `Cargo.toml`, crate manifests, `justfile`,
   implementation, and CI. Update `AGENTS.md` or README when guidance has
   drifted; do not alter working code merely to match stale prose.

## Repository rules

- Use `just` recipes for supported developer workflows. Use direct `cargo`
  commands only for focused crate tests or CI-parity flags absent from a recipe.
- Let Cargo use its default all-core build parallelism. Do not add
  `CARGO_BUILD_JOBS` or another fixed job cap unless the user explicitly asks
  for a resource limit.
- Preserve the established compilation cache and batch validation after edits.
  Follow `../../references/verification-matrix.md` when selecting build checks;
  instruction maintenance alone needs no compilation.
- Keep the performance contract aligned across the `benchmark` recipe, runner,
  budgets, scheduled CI evidence, and performance guide.
- Do not run `just clean`, delete environments, overwrite baselines, or remove
  generated files unless the user explicitly requested the destructive scope.
- Keep `Cargo.lock` and generated media handling consistent with `.gitignore`.
- Keep visual baselines intentional and versioned; keep `current/` and `report/`
  local.
- Preserve the centralized system-ordering and dependency-direction rules in
  `repo-map.md` when adding crates or systems.

For instruction maintenance, inspect loaded skill rules and references for
unconditional approval pauses, scope expansion, and redundant validation.
Keep real invariants and make recommendations conditional on the changed layer.

After maintenance, re-run the audit. For skill prose, validate changed skills
and review references; use `$gaanim-test-change` profiles when implementation or
build behavior changes. Report remaining warnings separately from failures.
