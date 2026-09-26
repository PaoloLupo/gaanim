# Public API to Typst documentation map

Update the narrowest existing page. Add a new page only when no current page
has the correct user-facing concept, then include it from
`docs/content/index.typ`.

| API area | Primary Typst page |
|---|---|
| `Scene`, canvas frame and safe area, timeline, segments, sections, camera, output | `docs/content/referencia/scene.typ` |
| `Drawable` handle: style, transforms, effects, anchors, reactive bindings | `docs/content/referencia/drawable.typ` |
| `scene.geometry`: primitives, paths, booleans, reactive geometry, 3D primitives | `docs/content/referencia/geometria.typ` |
| `scene.text`, `Text`, `TextStyle`, `TextFlow`, selections, Typst, measurement | `docs/content/referencia/text.typ` |
| `scene.viz`: coordinate spaces, calculus, data series, `Parameter`/`computed`, readouts, `ChartSpec`, vector fields | `docs/content/referencia/visualization.typ` |
| `scene.viz.matrix`, `Matrix`, `MatrixSelection`, matrix algebra | `docs/content/referencia/matrices.typ` |
| `scene.media`: images, SVG, video, Lottie, glTF | `docs/content/referencia/medios.typ` |
| `scene.slides`: editorial components and presentation branding | `docs/content/referencia/diapositivas.typ` |
| `scene.mechanics`: dimensions, springs, forces, supports, gears | `docs/content/referencia/mecanica.typ` |
| `Anim`, transitions, updaters, easing, writing | `docs/content/referencia/animations.typ` |
| `scene.layout`: rows, columns, grids, stacks, items, reflow, constraints, templates | `docs/content/referencia/layout.typ` |
| `Color`, `ColorMap`, brushes, backgrounds, post-processing, themes | `docs/content/referencia/themes.typ` |
| `AssetManager`, preloading, SVG/Lottie/glTF import behavior | `docs/content/referencia/assets.typ` |
| `scene.media.audio`, voiceover and narration | `docs/content/referencia/audio.typ` |
| `gaanim` command line: `init`, `check`, `export`, presenting, `--diff` | `docs/content/referencia/cli.typ` |
| `gaanim.toml` manifest schema and project resolution | `docs/content/referencia/gaanim-toml.typ` |

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
stub's. The site menu, previous/next links and the PDF follow `docs/content/index.typ`,
so a new page only needs its `#include` there.
