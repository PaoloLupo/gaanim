# AGENTS.md — gaanim

GPU-accelerated 2D vector animation engine (Manim-style). Rust workspace using Bevy ECS + Vello renderer, with Python bindings via PyO3/Maturin.

## Repo layout

- **Workspace root:** `Cargo.toml` defines 20 workspace members: 19 crates under
  `crates/` plus the `docs` application.
- **Key crates (bottom-up):**
  - `gaanim_core` — re-exports `peniko`/`kurbo`/`glam`, error types.
  - `gaanim_math` — `SpatialTransform`, `Camera`, `RateFunc`, `Bounds3D`.
  - `gaanim_expr` — native scalar/vector expression trees and differentiation.
  - `gaanim_scene` — ECS components, `SceneSet` system ordering, hierarchy propagation.
  - `gaanim_animation` — tween entities, signals, writing system.
  - `gaanim_timeline` — BTree-indexed clips, snapshot seek.
  - `gaanim_media` — FFmpeg-backed video frames and timeline-synchronized preview audio.
  - `gaanim_renderer` — Vello 0.9 backend, `bevy_vello` 0.14.0, fragment retain caching.
  - `gaanim_objects` — primitive bundles (circle, rect, etc.), text objects.
  - `gaanim_layout` — anchors, grids, regions, flow, and positioning queries.
  - `gaanim_visualization` — scales, coordinate spaces, sampling, data, and statistics.
  - `gaanim_text` — cosmic-text/HarfBuzz shaping, Typst math compilation.
  - `gaanim_api` — fluent Rust builder API; depends on most core crates.
  - `gaanim_python` — PyO3 0.28 module embedded by the editor plus the pure-Python authoring wheel.
  - `gaanim_project` — shared project scaffolding, manifests, recent-project state,
    and side-effect-free Python/uv environment discovery.
  - `gaanim_editor`, `gaanim_launcher`, `gaanim_export`, and `gaanim_diff` — application hosting, launch, export, and visual comparison tools.
- **Repository overview:** `README.md` is the current user/developer entry point.
  `engine_improvements.md` is aspirational; verify proposals against code and tests.

## Developer commands

All commands assume `just` is installed. Do not run `cargo build` at the workspace root expecting a working Python extension.

| Task | Command |
|------|---------|
| Type-check workspace | `just check` (alias: `cargo check --workspace`) |
| Lint workspace | `just clippy` |
| Build application binaries (debug) | `just build` |
| Build application binaries (release) | `just build-release` |
| Install authoring wheel in `.venv` | `just python-develop` |
| Build wheel | `just wheel` → outputs to `target/wheels/` |
| Run example by name | `just run <name>` (e.g., `just run math_animation`) |
| Validate installed Python API | `just validate-python-api` |
| Validate all export formats | `just test-exports` |
| Measure runtime budgets | `just benchmark smoke` (or `standard`) |
| Build documentation | `just docs` |
| Sanity check | `just doctor` — checks workspace, builds application binaries, and runs `--help` |
| Bootstrap venv | `just bootstrap` — creates `.venv` and installs `build`/`hatchling` |
| Full clean | `just clean` — deletes `.venv` and `cargo clean` |

**Python bridge order:** `just bootstrap` (once) → `just python-develop` installs
the authoring-only helpers and types. Plain Python cannot import or execute
Gaanim scenes: `just run`, `just validate-python-api`, and `just test-exports`
all use the application host, which owns the native runtime.

## Python bridge specifics

- **Python package name:** `gaanim`
- **Native module name:** `gaanim.gaanim_core` (maps to `#[pymodule] fn gaanim_core` in `crates/gaanim_python/src/lib.rs`)
- **Maturin config:** `crates/gaanim_python/pyproject.toml` (`module-name = "gaanim.gaanim_core"`)
- The `.venv` is gitignored and expected locally after `just bootstrap`.

## Build / toolchain quirks

- **Bevy 0.19** is the current ECS target. Do not import `bevy::ecs::*` directly outside `gaanim_scene` — use re-exports from `gaanim_scene` or `gaanim_core`.
- **Vello 0.9**, **bevy_vello 0.14.0**, **bevy_egui 0.42.0**, **PyO3 0.28**.
- Rust editions vary: most crates use **2024**; `gaanim_python` uses **2021**.
- `Cargo.lock` exists locally but is **gitignored** (library/workspace convention).
- Workspace profiles: `dev` uses `opt-level = 1` for workspace crates, `opt-level = 3` for dependencies.

## Testing

- Inline unit tests are present in several crates. Repository-level Python/API
  and visual fixtures live under `tests/`; per-crate Rust integration-test
  directories are not yet common.
- Run per-crate tests: `cargo test -p <crate>`
- Run workspace tests: `cargo test --workspace`.
- Verify the Rust→Python bridge with `just python-develop` followed by
  `just validate-python-api`.
- Verify the five output formats, including audio streams in MP4/WebM, with
  `just test-exports` (requires FFmpeg and ffprobe).
- Use `just doctor` for a fast application/toolchain sanity check.

## Agent plugin

- `plugins/gaanim-dev` packages repository-aware skills for adding features,
  synchronizing Typst docs, fixing bugs, testing changes, and maintenance.
- Inspect change impact with
  `python plugins/gaanim-dev/scripts/impact.py --format json`.
- Audit objective contracts with `python plugins/gaanim-dev/scripts/audit.py`.
- Preview change-aware validation with
  `python plugins/gaanim-dev/scripts/verify.py fast --dry-run`.
- Audit warnings are heuristic by default. Objective invariant failures return
  a nonzero status; pass `--strict` only when warnings should also fail.

## Visual regression diffs

- `gaanim_diff` provides exact timeline-seek snapshots, PNG comparison, JSON/HTML reports, and a native egui viewer. The main `gaanim` binary exposes it through `--diff`.
- Snapshot fixtures are organized globally per example: `tests/visual/<example-relative-path-without-.py>/{baseline,current,report}`. For example, `examples/visual_diff_demo.py` maps to `tests/visual/visual_diff_demo/`.
  - `baseline/` is the approved fixture and should be versioned when intentionally changed.
  - `current/` and `report/` are generated local artifacts and are gitignored.
- Examples intended for visual regression must conditionally call `scene.snapshots(os.environ["GAANIM_SNAPSHOTS"], seeks)` when `GAANIM_SNAPSHOTS` is set. The diff CLI injects that environment variable and runs this capture headlessly.
- Windows workflow (activate `.venv` first so the editor binary can load Python):

  ```powershell
  . .\.venv\Scripts\Activate.ps1
  target/debug/gaanim.exe --diff --example examples/visual_diff_demo.py --bless
  target/debug/gaanim.exe --diff --example examples/visual_diff_demo.py
  ```

  `--bless` overwrites the example baseline, so use it only after intentionally approving a visual change. The normal command captures `current/`, compares it with `baseline/`, and opens egui. Add `--no-gui` for CI; use `--pixel-threshold` and `--max-changed-ratio` to tolerate controlled raster differences.
- Legacy/manual comparison remains available with `--baseline`, `--current`, and `--output`, but prefer `--example` so paths stay deterministic.

## Code conventions

- **No wrapper types** for graphics primitives: use `peniko::Color`, `peniko::Brush`, `kurbo::BezPath`, `kurbo::Affine` directly in ECS components.
- **3D-ready types:** `glam::DVec3` / `DQuat` for transforms; `Bounds3D` for bounding boxes. 2D is the z=0 projection.
- **System ordering:** all ordering is centralized in `gaanim_scene::hierarchy::SceneSet`. Add new systems into the appropriate `SceneSet` phase rather than ad-hoc `before`/`after`.
- **Python API style:** fluent chaining on `PyMobject` (e.g., `.fill(BLUE).at(0,0)`). Mutations are deferred via `Arc<Mutex<MobjectSpec>>` and replayed at startup.

## Gotchas

- Generated artifacts (`.svg`, `.png`, `.mp4`, `.webp`, `.webm`) are gitignored. Examples may write these to the repo root.
