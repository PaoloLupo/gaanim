# Runtime performance contract

Use the versioned harness instead of ad-hoc shell timing:

```text
just benchmark smoke
just benchmark standard
```

Both recipes build and invoke the native `gaanim-core` release executable.
Results live in `target/performance/runtime-benchmark.json`; command logs and
generated artifacts remain under `target/performance/artifacts/`.

## Profiles and semantics

| Scenario | Current observable operation |
|---|---|
| `reload` | Cold executable startup plus Python scene load and `gaanim check`; not yet persistent watcher reload |
| `seek` | Deterministically dispersed exact seeks through timeline, GPU rendering, readback, and PNG capture |
| `preview` | Dense 1920x1080 headless capture; excludes window presentation and vsync |
| `export` | H.264 draft export; `standard` renders 300 frames at 1920x1080 |

The report records p50/p95 command latency, FPS at those latency percentiles,
and peak RSS when available. Linux samples the process tree, including FFmpeg;
other platforms may report a narrower scope or no memory value.

## Budget policy

`tests/performance/budgets.json` is the source of truth. Initial limits are
informational and the scheduled CI job is non-blocking. Use `--enforce` only
when enforcement is explicitly in scope.

Calibrate a limit from repeated `standard` runs on a stable runner. Preserve
the raw reports used for the decision, allow for ordinary variance, and state
which hardware/software population the limit represents. A budget increase
requires a root-cause note; a decrease requires more than one machine-local
sample.

Do not compare debug and release builds or mix profiles. A smoke run proves
wiring, not production performance.
