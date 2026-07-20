# AGENTS.md — gaanim

GPU-accelerated 2D vector animation engine (Manim-style). Rust workspace using Bevy ECS + Vello renderer, with Python bindings via PyO3/Maturin.

## Repo layout

- **Workspace root:** `Cargo.toml` defines 12 crates under `crates/`.
- **Key crates (bottom-up):**
  - `gaanim_core` — re-exports `peniko`/`kurbo`/`glam`, error types.
  - `gaanim_math` — `SpatialTransform`, `Camera`, `RateFunc`, `Bounds3D`.
  - `gaanim_scene` — ECS components, `SceneSet` system ordering, hierarchy propagation.
  - `gaanim_animation` — tween entities, signals, writing system.
  - `gaanim_timeline` — BTree-indexed clips, snapshot seek.
  - `gaanim_renderer` — Vello 0.7 backend, `bevy_vello` 0.13.1, fragment retain caching.
  - `gaanim_objects` — primitive bundles (circle, rect, etc.), text objects.
  - `gaanim_text` — cosmic-text/HarfBuzz shaping, Typst math compilation.
  - `gaanim_api` — fluent Rust builder API; depends on most core crates.
  - `gaanim_python` — PyO3 0.28 extension (`cdylib`), thin wrapper over `gaanim_api`.
- **No README exists.** Design/roadmap docs are `implementation_plan.md` and `engine_improvements.md` (Spanish; aspirational — verify against actual code).

## Developer commands

All commands assume `just` is installed. Do not run `cargo build` at the workspace root expecting a working Python extension.

| Task | Command |
|------|---------|
| Type-check workspace | `just check` (alias: `cargo check --workspace`) |
| Lint workspace | `just clippy` |
| Build Python extension (debug) | `just build` → runs `maturin develop` inside `crates/gaanim_python` |
| Build Python extension (release) | `just build-release` |
| Build wheel | `just wheel` → outputs to `target/wheels/` |
| Run example by name | `just run <name>` (e.g., `just run math_animation`) |
| Sanity check | `just doctor` — compiles workspace + imports extension |
| Bootstrap venv | `just bootstrap` — creates `.venv` and installs `maturin` |
| Full clean | `just clean` — deletes `.venv` and `cargo clean` |

**Required order:** `just bootstrap` (once) → `just build` (or `build-release`) before running any Python example.

## Python bridge specifics

- **Python package name:** `gaanim`
- **Native module name:** `gaanim.gaanim_core` (maps to `#[pymodule] fn gaanim_core` in `crates/gaanim_python/src/lib.rs`)
- **Maturin config:** `crates/gaanim_python/pyproject.toml` (`module-name = "gaanim.gaanim_core"`)
- The `.venv` is gitignored but expected locally. It already exists in this checkout.

## Build / toolchain quirks

- **Bevy 0.18** is the current ECS target; abstractions are designed with 0.19 migration in mind. Do not import `bevy::ecs::*` directly outside `gaanim_scene` — use re-exports from `gaanim_scene` or `gaanim_core`.
- **Vello 0.7**, **bevy_vello 0.13.1**, **PyO3 0.28**.
- Rust editions vary: most crates use **2024**; `gaanim_python` uses **2021** (required by PyO3 cdylib constraints).
- `Cargo.lock` exists locally but is **gitignored** (library/workspace convention).
- Workspace profiles: `dev` uses `opt-level = 1` for workspace crates, `opt-level = 3` for dependencies.

## Testing

- No dedicated `tests/` directories exist yet. Inline unit tests are present in a few crates (e.g., `gaanim_math`).
- Run per-crate tests: `cargo test -p <crate>`
- The `just doctor` command is the fastest verification that the Rust→Python bridge is intact.

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
