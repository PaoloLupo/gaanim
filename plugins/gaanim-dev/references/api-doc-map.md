# Public API to Typst documentation map

Update the narrowest existing page. Add a new page only when no current page
has the correct user-facing concept, then include it from
`docs/content/index.typ`.

| API area | Primary Typst page |
|---|---|
| `Scene`, viewport, timeline, camera, clipping, output | `docs/content/api/scene.typ` |
| constructors, `Drawable`, geometry, text, media, reactive objects | `docs/content/api/mobjects.typ` |
| `Anim`, transitions, updaters, easing, writing | `docs/content/api/animations.typ` |
| anchors, grids, regions, flow, stacks, tracks | `docs/content/api/layout.typ` |
| colors, brushes, themes, gradients, effects | `docs/content/api/themes.typ` |
| manifests, preloading, SVG asset behavior | `docs/content/api/assets.typ` |
| audio behavior | `docs/content/api/audio.typ` |

Follow `docs/components/api.typ` and neighboring entries. Keep each documented
signature identical to the callable Python surface, describe units/defaults
and observable failure behavior, and include a minimal executable example.

For a new public feature, update documentation in the same change. If the
change is deliberately internal, record that conclusion in the final report
instead of making an unrelated documentation edit.

Run `just docs` after editing Typst. Treat a successful build as structural
validation, not proof that the prose matches runtime behavior; compare the
example with the binding or execute it when practical.
