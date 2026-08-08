---
name: gaanim-add-feature
description: Implement complete Gaanim features across the owning Rust crate, gaanim_api, PyO3 bindings, Python stubs and exports, Typst API docs, examples, and tests. Use when adding or extending public Rust or Python behavior, animation primitives, scene methods, drawables, layouts, rendering features, or fluent API methods in the Gaanim repository.
---

# Add a Gaanim feature

Implement the smallest complete vertical slice. Preserve unrelated worktree
changes and verify every public layer that the feature actually crosses.
Resolve `PLUGIN_ROOT` as the directory two levels above this `SKILL.md`; invoke
bundled scripts by absolute path while keeping the Gaanim repository as cwd.

## Workflow

1. Read `../../references/repo-map.md` and inspect the relevant manifests and
   neighboring implementation before choosing the owning crate.
2. Run `python <PLUGIN_ROOT>/scripts/impact.py --format json` to record the current
   change surface. Inspect `git status --short` separately before editing.
3. Define observable behavior, failure cases, and compatibility constraints.
   Do not treat roadmap documents as implemented behavior without verifying
   code and tests.
4. Implement bottom-up in the owning crate. Add focused unit tests beside the
   logic when possible.
5. If the feature is public, read `../../references/python-bridge.md` and carry
   it through `gaanim_api`, PyO3, the stub, package exports, and a runnable
   example as applicable.
6. If the feature is user-facing, read `../../references/api-doc-map.md` and
   update the mapped Typst page in the same change.
7. Read `../../references/verification-matrix.md`, run the narrowest relevant
   tests, then expand. Use `$gaanim-test-change` for execution-heavy validation.
8. Re-run `impact.py` and explain any recommended layer deliberately omitted.

## Guardrails

- Place ordered ECS systems in `SceneSet`; do not add scattered ordering edges.
- Use native `peniko`, `kurbo`, and `glam` types and keep transforms 3D-ready.
- Preserve deferred Python mutation semantics and fluent chaining.
- Do not expose lower-level ECS details merely because the implementation uses
  them.
- Do not update visual baselines while implementing. Compare first and require
  explicit approval through `$gaanim-test-change` before blessing.

Finish with changed public behavior, documentation location, tests executed,
visual status, and remaining warnings.
