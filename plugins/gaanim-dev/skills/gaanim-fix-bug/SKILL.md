---
name: gaanim-fix-bug
description: Diagnose and fix Gaanim defects with a reproducible case, root-cause analysis, focused regression coverage, and change-aware verification. Use for crashes, incorrect animation or rendering, timeline seek errors, layout or transform bugs, PyO3/Python API failures, documentation examples that do not run, visual regressions, or failing Gaanim tests and CI checks.
---

# Fix a Gaanim bug

Prove the failure and its cause before changing implementation.
Resolve `PLUGIN_ROOT` as the directory two levels above this `SKILL.md`; invoke
bundled scripts by absolute path while keeping the Gaanim repository as cwd.

## Workflow

1. Inspect `git status --short` and preserve all unrelated edits and generated
   artifacts.
2. Read `../../references/repo-map.md`. Trace the failing behavior to the
   lowest owning layer instead of patching the public facade first.
3. Reproduce with the smallest existing test, example, or new focused test.
   Record the exact command and failure. If reproduction is impossible, state
   the missing condition and continue with safe diagnostics only.
4. Form a cause hypothesis and verify it against code paths, state transitions,
   and neighboring tests. Distinguish the trigger from the defect.
5. Add a regression test that fails for the defect, then implement the narrowest
   fix. Avoid unrelated cleanup.
6. If the fix changes public behavior, read `../../references/python-bridge.md`
   and `../../references/api-doc-map.md`; update the contract and docs rather
   than silently changing semantics.
7. Use `python <PLUGIN_ROOT>/scripts/impact.py` and
   `../../references/verification-matrix.md` to escalate from the focused test
   to the relevant API, docs, or visual checks.
8. For raster differences, read `../../references/visual-testing.md`. Never
   bless a baseline as a bug fix without explicit approval of the new image.

Finish with reproduction, root cause, regression coverage, fix scope, commands
run, and any validation that could not execute.
