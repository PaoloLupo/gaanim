---
name: gaanim-test-change
description: Select and run change-aware Gaanim verification profiles for Rust crates, the PyO3/Python API contract, Typst docs, examples, and visual regression. Use when testing a change, validating a fix or feature, checking CI parity, comparing snapshots, inspecting visual reports, or explicitly approving visual baselines.
---

# Test a Gaanim change

Use a focused-first validation ladder and report skipped checks distinctly from
passes.
Resolve `PLUGIN_ROOT` as the directory two levels above this `SKILL.md`; invoke
bundled scripts by absolute path while keeping the Gaanim repository as cwd.

## Select a profile

Read `../../references/verification-matrix.md`, then preview with:

```text
python <PLUGIN_ROOT>/scripts/verify.py <fast|api|visual|full> --dry-run
```

- Choose `fast` for local implementation feedback.
- Choose `api` for public Rust/Python or Typst changes.
- Choose `visual` for rendering, transforms, layout, text, scene, animation, or
  timeline behavior. Pass one or more `--example <name>` values when impact
  inference is too broad or empty.
- Choose `full` for repository-wide, release-sensitive, or final CI-parity
  validation.

Pass `--base <ref>` when the requested scope is a branch range rather than only
the current worktree.

## Visual safety

Read `../../references/visual-testing.md` before visual work. Compare with:

```text
python <PLUGIN_ROOT>/scripts/verify.py visual --example camera_demo
```

If comparison fails, inspect the printed `report/index.html` path and explain
the difference. Never use `--bless` to remove a failure. Only after the user
explicitly approves the exact visual change, run both interlock flags:

```text
python <PLUGIN_ROOT>/scripts/verify.py visual --example camera_demo --bless --allow-bless
```

Do not describe unavailable visual execution as passing. Finish with each
command, pass/fail/skip status, report paths, and untested risk.
