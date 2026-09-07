---
name: gaanim-test-change
description: Select and run change-aware Gaanim verification profiles for Rust crates, the PyO3/Python API contract, Typst docs, examples, visual regression, and performance smoke. Use when testing a change, validating a fix or feature, checking CI parity, comparing snapshots, inspecting reports, or explicitly approving visual baselines.
---

# Test a Gaanim change

Select validation by affected behavior and report skipped checks distinctly
from passes. Reuse successful checks from this session when their inputs have
not changed; broaden only for an uncovered risk, new failure, or requested CI parity.
Resolve `PLUGIN_ROOT` as the directory two levels above this `SKILL.md`; invoke
bundled scripts by absolute path while keeping the Gaanim repository as cwd.

## Select a profile

For changes limited to `AGENTS.md`, skill prose, or plugin metadata, review the
diff, validate changed skill/plugin structure, and run `scripts/audit.py`.
Do not invoke a build profile merely because several instruction files changed.

Read `../../references/verification-matrix.md`, then preview with:

```text
python <PLUGIN_ROOT>/scripts/verify.py <fast|api|visual|performance|full> --dry-run
```

Apply the matrix's compilation reuse guidance before execution. Batch edits,
preserve the existing Cargo cache inputs, and coordinate shared-cache builds.
Choose direct affected-layer checks if a profile would compile unrelated targets;
record which required checks they cover. Never treat a stale binary as validation.

In a checkout with `scripts/dev.py`, use `just test-package`, `just check-package`,
or `just dev` so development validation keeps `bevy/dynamic_linking` enabled.
Prefer the checkout's verification script if the installed copy is older.
Do not append a workspace check after focused tests pass without a concrete
integration reason. Use `just dev-exec` for already-current native dev runners.

- Choose `fast` for local implementation feedback.
- Choose `api` for public Rust/Python or Typst changes.
- Choose `visual` for rendering, transforms, layout, text, scene, animation, or
  timeline behavior. Pass one or more `--example <name>` values when impact
  inference is too broad or empty.
- Choose `performance` for runtime hot paths or harness changes. It uses the
  smoke workload unless `--benchmark-profile standard` is passed.
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

Performance runs do not authorize visual baseline replacement. Read
`../../references/performance-testing.md` before interpreting or changing a
budget, and report whether the result was informational or enforced.
