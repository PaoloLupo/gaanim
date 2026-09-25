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

## Executable blocks

Every `python` block in `docs/content` runs through `gaanim-core` during the
docs build, and the build fails when any of them reports an error (pass
`--allow-example-errors` to `docs compile` to finish anyway while drafting). A
block exports when it has `# output:`, is checked when it calls `render()`, and
otherwise runs as a fragment against the embedded module, so every name it uses
must exist. Each block runs in its own directory that links the sample project
in `docs/fixtures/`; add small files there for examples that load assets by
relative path. Directives are the block's own comment lines:

| Directive | Effect |
|---|---|
| `# output: preview.webp` | Export the scene and show the animation next to the code. |
| `# show-code: true` | Keep the code visible next to its preview. |
| `# continue` | Replay the page's previous executed block first (hidden, output muted, its `render()` dropped). Use it for fragments that build on earlier code. |
| `>>> line` | Execute a setup line without displaying it. |
| `<<< line` | Display a line without executing it. |
| `# no-run: <motivo>` | Show the block without running it. The reason is mandatory; reserve it for code that cannot run in the docs build, such as a reference to the reader's own files. |

The builder queues uncached blocks, runs them in parallel (`--jobs N`,
default: CPU cores), then compiles again. Cached results record the stub and
runtime version; when either changes, a cached preview is re-checked instead of
re-rendered, and a failure is always retried by the next build.

Each `api-entry` name must use real stub symbols (`Class.member`, with bare
members after `/` inheriting the class). The build fails on a name that
`gaanim_core.pyi` does not expose. An entry without a `signature` shows the
stub's. When adding a page, also add it to `site-map` in
`docs/components/section.typ`.
