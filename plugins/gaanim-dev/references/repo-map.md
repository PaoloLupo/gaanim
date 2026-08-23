# Gaanim repository map

Treat live configuration and code as authoritative in this order:

1. `Cargo.toml`, crate manifests, `justfile`, and CI workflows.
2. Rust and Python implementation plus executable tests.
3. `README.md`, `AGENTS.md`, and published Typst documentation.
4. Roadmaps and design proposals, which may be aspirational.

## Dependency direction

Work bottom-up and keep dependencies flowing toward the public API:

`gaanim_core` → `gaanim_math` → `gaanim_scene` → `gaanim_animation` →
`gaanim_timeline` → renderer/objects/layout/text → `gaanim_api` →
`gaanim_python` → editor/launcher/export/diff/docs.

Confirm the exact dependency graph from crate manifests before editing. Do not
introduce a dependency cycle to match this simplified map.

## Non-negotiable conventions

- Use `peniko`, `kurbo`, and `glam` graphics types directly; do not add wrapper
  primitives.
- Keep transforms 3D-ready with `DVec3`, `DQuat`, and `Bounds3D` even when a
  feature projects to z=0.
- Import Bevy ECS details through `gaanim_scene` or `gaanim_core` outside the
  scene crate.
- Place ordered systems in the appropriate centralized `SceneSet` phase.
- Preserve fluent Python chaining and deferred `MobjectSpec` mutations.
- Inspect `git status --short` before and after work. Never discard unrelated
  dirty-worktree changes or generated artifacts owned by the user.

## Public layers

- Rust facade: `crates/gaanim_api`.
- Native Python bindings: `crates/gaanim_python/src`.
- Python typing: `crates/gaanim_python/gaanim/gaanim_core.pyi`.
- Python package exports and compatibility shims:
  `crates/gaanim_python/gaanim/__init__.py`.
- User documentation: `docs/content/**/*.typ`.
- Executable examples: `examples/*.py` and `docs/content/examples/*.py`.
- Performance contract: `tests/benchmark_runtime.py`,
  `tests/performance/budgets.json`, `examples/performance_benchmark.py`, and the
  scheduled informational CI job.
