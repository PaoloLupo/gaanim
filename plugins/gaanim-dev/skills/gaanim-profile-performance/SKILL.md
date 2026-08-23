---
name: gaanim-profile-performance
description: Measure, compare, and evolve Gaanim runtime performance budgets for reload, exact seek, headless preview, and export. Use when profiling latency or memory, investigating a performance regression, calibrating p50/p95 budgets, changing the benchmark harness, or reviewing scheduled performance evidence in the Gaanim repository.
---

# Profile Gaanim performance

Produce comparable evidence from the native release executable and keep
provisional budgets distinct from enforced gates.
Resolve `PLUGIN_ROOT` as the directory two levels above this `SKILL.md`; invoke
bundled scripts by absolute path while keeping the Gaanim repository as cwd.

## Workflow

1. Read `../../references/performance-testing.md` and inspect `git status --short`.
2. Run `python <PLUGIN_ROOT>/scripts/impact.py --format json` before changing the
   harness, budgets, runtime hot paths, `justfile`, or CI.
3. Run `just benchmark smoke` to validate wiring. Use `just benchmark standard`
   for a comparable measurement only when the task warrants its 300-frame
   export cost.
4. Inspect `target/performance/runtime-benchmark.json` and the per-scenario logs.
   Compare only matching profile, platform, architecture, scene, and release
   build.
5. When diagnosing a regression, isolate whether it comes from Python scene
   loading, timeline seeks, GPU/readback, memory, or FFmpeg before changing a
   budget.
6. Re-run the same profile after the change and report both measurements,
   budget violations, memory scope, and unmeasured risk.

## Guardrails

- The executable is the runtime; never benchmark a plain Python import of the
  authoring wheel.
- Treat `reload` as persistent scene loading plus ECS replay inside one native
  process. Treat `preview` as headless dense capture, not window/vsync latency.
- Budgets remain informational unless the user or CI explicitly requests
  `--enforce`. Do not tighten them from one laptop or one sample.
- Use `--capture-only` for benchmark snapshots. Never bless or overwrite visual
  baselines during profiling.
- Keep performance artifacts under `target/`; do not version generated media or
  machine-specific reports.
- Let Cargo use its default all-core parallelism. Add a job limit only when the
  user explicitly requests a resource cap.

For harness changes, run its Python unit tests, the smoke profile, `just check`,
the plugin audit, and plugin utility tests. Finish with the report path and the
exact checks that were passed, skipped, or left informational.
