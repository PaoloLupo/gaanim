# Runtime performance contract

Use the versioned harness instead of ad-hoc shell timing:

```text
just benchmark smoke
just benchmark standard
```

Both recipes build and invoke the native `gaanim` release executable.
Results live in `target/performance/runtime-benchmark.json`; command logs and
snapshot manifests remain under `target/performance/artifacts/`. Each sample's
frames and videos are deleted once validated (a standard preview of a heavy
scene writes about 500 MB); pass `--keep-artifacts` to inspect them.

## Profiles and semantics

| Scenario | Current observable operation |
|---|---|
| `reload` | Second Python scene load plus ECS replay after warming the same interpreter and world |
| `seek` | Deterministically dispersed exact seeks through timeline, GPU rendering, readback, and PNG capture |
| `preview` | Dense 1920x1080 headless capture; excludes window presentation and vsync |
| `export` | H.264 draft export; `standard` renders 300 frames at 1920x1080 |
| `playback` | Opt-in: the editor's window plays the timeline once (`GAANIM_FRAME_PROFILE`), with vsync |

`playback` runs only when named (`--scenarios playback`), since it opens a
window. A sample is the 95th-percentile frame time of one playback; its
phases are frame time p50/p95, main-world time p50/p95 (the CPU work of a
frame, which vsync does not hide), and per-frame seek, fragment compile and
render-world averages from the `GAANIM_PLAYBACK_TIMINGS` line. A frame time
at the display's refresh means playback keeps up; compare `main_*` between
builds.

The report records p50/p95 scenario latency, FPS at those latency percentiles,
and peak RSS when available. Reload latency excludes process startup; its raw
process duration remains diagnostic. Linux samples the process tree, including
FFmpeg; Windows reports the peak working set of the Gaanim process alone
(`memory_scope: process-peak-working-set`); macOS samples the process RSS.

Each run records the `gpu_adapter` the executable announced
(`GAANIM_GPU_ADAPTER backend=... type=... name=...`). Export runs split their
time into `setup`, `update` (timeline and ECS), `scene_build` (composition),
`render_gpu` (Vello and readback calls, of which `readback_wait` is blocked on
the GPU), `encoder_wait` and `finalize`; the timing line also counts
`reused_frames`, held frames sent again without rendering. Exports compose
frame N+1 while the GPU draws frame N, so `readback_wait` near zero means the
GPU is hidden behind the CPU work.

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
python tests/benchmark_runtime.py --scene examples/performance_repeater.py --scenarios playback
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
