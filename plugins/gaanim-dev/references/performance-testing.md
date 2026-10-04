# Runtime performance contract

Use the versioned harness instead of ad-hoc shell timing:

```text
just benchmark smoke
just benchmark standard
```

Both recipes build and invoke the native `gaanim` release executable.
Results live in `target/performance/runtime-benchmark.json`; command logs and
generated artifacts remain under `target/performance/artifacts/`.

## Profiles and semantics

| Scenario | Current observable operation |
|---|---|
| `reload` | Second Python scene load plus ECS replay after warming the same interpreter and world |
| `seek` | Deterministically dispersed exact seeks through timeline, GPU rendering, readback, and PNG capture |
| `preview` | Dense 1920x1080 headless capture; excludes window presentation and vsync |
| `export` | H.264 draft export; `standard` renders 300 frames at 1920x1080 |

The report records p50/p95 scenario latency, FPS at those latency percentiles,
and peak RSS when available. Reload latency excludes process startup; its raw
process duration remains diagnostic. Linux samples the process tree, including
FFmpeg; other platforms may report a narrower scope or no memory value.

## Suites, scales and comparisons

`tests/performance/budgets.json` also defines suites of scenes:

| Suite | Scenes |
|---|---|
| `core` | `examples/performance_benchmark.py`, the scene the profile budgets were calibrated on |
| `gpu` | Heavy scenes: `performance_objects`, `performance_object_effects`, `performance_post`, `performance_heavy`, `performance_held`, `performance_3d`, `performance_particles`, `performance_repeater` |
| `scaling` | Selected scenes at growing `GAANIM_BENCHMARK_SCALE`, to expose superlinear growth |

```text
python tests/benchmark_runtime.py --suite gpu --profile standard
python tests/benchmark_runtime.py --suite scaling --scales 1 2 4
python tests/benchmark_runtime.py --suite gpu --scenarios export --export-format webm
python tests/benchmark_runtime.py --suite gpu --compare before.json --max-regression 0.10
```

Every scene reads `GAANIM_BENCHMARK_FRAMES` and `GAANIM_BENCHMARK_SCALE`;
`performance_heavy.py` also reads `GAANIM_BENCHMARK_MOTION_BLUR=<samples>`.
The report (`schema_version` 2) lists one run per scene and scale, with
`ms_per_frame_p50` for frame-based scenarios. `--compare` takes an earlier
schema 2 report and flags a p50 more than `--max-regression` slower for the
same scene, scale, scenario and export format. The `gpu` and `scaling`
suites keep absolute budgets off until they are calibrated; judge
performance changes with `--compare` on one machine.

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
