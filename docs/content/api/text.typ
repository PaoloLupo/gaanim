#import "../../components/section.typ": docs-chapter
#import "../../components/api.typ": api-entry

#show: docs-chapter.with(
  title: "Text",
  description: "Prosa, matemáticas, partes semánticas, flujo responsive, selecciones y animación estructural",
  route: "/api/text/",
  code-langs: (),
  updated: datetime.today().display(),
)

= Text

`scene.text()` is the general factory for prose, titles, paragraphs,
mathematics, and mixed content. `scene.equation()` is its display-math
convenience wrapper. Both return the same specialized `Text`: a vector
`Drawable` that keeps semantic structure, is measured intrinsically by Layout
v2, and exposes local selections and text-specific animations.

```python
from gaanim import GOLD, Scene, TextFlow, TextStyle, part

scene = Scene(640, 360, background="#0f172a")
formula = part("formula", "$E = ", part("mass", "m", color=GOLD), " c^2$")
copy = scene.text(
    "La energía es ",
    formula,
    role="body",
    style=TextStyle(size=34),
    flow=TextFlow(wrap=520, align="center"),
).at(0, 0)

scene.play([copy.write(1.2, by="part", stagger=0.06)])
scene.play([copy["formula"]["mass"].indicate(0.6)])
```

The old `title`, `subtitle`, `paragraph`, and `math` factories do not have
compatibility aliases. Use a role for prose or `scene.equation()` for a
standalone equation. `scene.typst()` remains available for arbitrary Typst
documents, but it does not provide the structured `Text` selection API
described here.

== Responsabilidades

- `TextStyle` controls glyph appearance and typographic metrics.
- `TextFlow` controls internal line composition.
- Layout v2 alone controls the outer box, padding, fit, growth, tracks,
  constraints, ownership, and reflow.
- `TextSelection` addresses glyphs inside one `Text`; it never becomes an
  independent Layout child.

This split means there is no second text box solver. See
#link("/api/layout/", "Layout v2") for container sizing and placement.

== Scene.text

#api-entry(
  name: "Scene.text",
  kind: "factory",
  signature: "text(*content, role=None, style=None, flow=None, font=None, math_font=None, size=None, weight=None, italic=None, color=None, opacity=None, letter_spacing=None, word_spacing=None, baseline=None, wrap=None, text_align=None, line_spacing=None, max_lines=None, overflow=None, direction=None, hyphenate=None) -> Text",
  params: (
    (name: "content", type: "str | TextPart | TextParts", default: none, desc: [One or more composable strings, semantic parts, or compact ordered part groups. The flattened result must not be empty.]),
    (name: "role", type: "TextRole | None", default: "None", desc: [Semantic role. Fully mathematical content infers `math`; everything else infers `body`.]),
    (name: "style", type: "TextStyle | None", default: "None", desc: [Reusable visual and metric overlay.]),
    (name: "flow", type: "TextFlow | None", default: "None", desc: [Reusable internal line-composition options.]),
    (name: "style overrides", type: "keyword arguments", default: "None", desc: [Direct font, metric, color, opacity, spacing, and baseline values.]),
    (name: "flow overrides", type: "keyword arguments", default: "None", desc: [Direct wrap, alignment, line limit, overflow, direction, and hyphenation values.]),
  ),
  returns: (type: "Text", desc: [Structured vector text measured by the same intrinsic Layout v2 pass in every context.]),
  desc: [Direct keywords override `TextStyle` and `TextFlow`. Invalid content, delimiters, roles, metrics, or flow values raise `TypeError` or `ValueError`.],
)[
```python
# show-code: true
from gaanim import GOLD, Scene, TextFlow, part
scene = Scene(480, 270, background="#0f172a")
formula = part("formula", "$E = ", part("mass", "m", color=GOLD), " c^2$")
copy = scene.text(
    "La energía es ", formula,
    role="body",
    flow=TextFlow(wrap=400, align="center", line_spacing=1.2),
).at(0, 0)
scene.play([copy.write(1.0, by="part")])
scene.export("text_factory.webp", fps=30)
```
]

== Scene.equation

#api-entry(
  name: "Scene.equation",
  kind: "factory",
  signature: "equation(*content, role=None, style=None, flow=None, font=None, math_font=None, size=None, weight=None, italic=None, color=None, opacity=None, letter_spacing=None, word_spacing=None, baseline=None, wrap=None, text_align=None, line_spacing=None, max_lines=None, overflow=None, direction=None, hyphenate=None) -> Text",
  params: (
    (name: "content", type: "str | TextPart | TextParts", default: none, desc: [Equation source without surrounding math delimiters.]),
    (name: "options", type: "same as Scene.text", default: "None", desc: [The complete style and flow surface is shared with `scene.text()`.]),
  ),
  returns: (type: "Text", desc: [A standalone structured equation with normal `Text` selections and animations.]),
  desc: [Wraps content internally as `$ ... $`. Those spaces are preserved because Typst uses them to distinguish a block equation from inline `$...$`. Every content boundary inside math becomes ordinary Typst whitespace, so Typst itself determines operator and identifier spacing. Empty content raises `ValueError`.],
)[
```python
# show-code: true
from gaanim import GOLD, Scene, part, parts
scene = Scene(640, 360, background="#0f172a")
equation = scene.equation(
    part("sum_force", "sum F_t"),
    "=",
    parts(mass="m", acceleration="a_t"),
).at(0, 0)
equation["acceleration"].fill(GOLD)
scene.play([equation.write(1.0, by="part")])
scene.export("equation_factory.webp", fps=30)
```
]

== Roles

The accepted roles are:

```text
"title" | "subtitle" | "heading" | "body" |
"caption" | "label" | "code" | "math"
```

The default text configuration uses these sizes in Typst/canvas points:
`title=64`, `subtitle=48`, `heading=40`, `body=32`, `caption=24`,
`label=28`, `code=28`, and `math=32`. Prose uses New Computer Modern by
default, code uses Consolas, and math uses New Computer Modern Math. The active
theme and explicit style may replace the resolved color and typography.

Resolution order is:

```text
role/theme -> TextStyle/TextFlow -> direct scene.text keywords
           -> local part style -> later TextSelection.fill / Text.fill
```

== Contenido estructurado y matemáticas

Strings, `TextPart`, and the compact `TextParts` group form one ordered content
tree. `TextParts` expands to ordinary sibling parts before the tree is stored,
so semantic paths remain identical to those created with repeated `part()`
calls and more stable than manual character ranges.

=== Marcado de énfasis en línea

Ordinary strings accept a small Typst-inspired markup language. `*strong*`
selects a bold face, `_emphasis_` selects italic, and the delimiters may be
nested as `*_strong emphasis_*`. Markup styling is compiled into the same
structured Typst runs as `TextStyle`, so it works with wrapping, selections,
animations, semantic parts, and Layout v2 measurement.

```python
# show-code: true
from gaanim import GOLD, Scene

scene = Scene(640, 360, background="#0f172a")
copy = scene.text(
    "Normal, _emphasis_, *strong* and *_both_*.",
    size=36,
).at(0, 0)
scene.play([copy.write(1.2, by="word", stagger=0.05)])
scene.play([copy.words[3].indicate(0.6, color=GOLD)])
scene.export("text_inline_markup.webp", fps=30)
```

- `\\*` and `\\_` produce literal delimiters.
- Markup may span adjacent strings and `part()` boundaries without adding a
  shaping gap.
- Markers inside `$...$` remain mathematical syntax; subscripts such as `x_1`
  and multiplication with `*` are not interpreted as prose emphasis.
- Intraword underscores (`snake_case`), repeated markers (`__init__`), and a
  spaced expression such as `5 * 4` remain literal.
- A valid opening delimiter without its matching close, or crossed nesting,
  raises `ValueError`. Escape a literal adjacent marker when it could be read
  as an opener.

#api-entry(
  name: "parts",
  kind: "factory",
  signature: "parts(**content: str) -> TextParts",
  params: (
    (name: "content", type: "keyword str entries", default: none, desc: [Ordered semantic names and their plain text.]),
  ),
  returns: (type: "TextParts", desc: [Immutable ordered group accepted by `scene.text()`, `scene.equation()`, `Text.become()`, and `part()`.]),
  desc: [Inside `$...$`, adjacent sibling entries become distinct Typst math tokens and retain Typst's native tight spacing. Empty input, empty names, or wholly empty content raise `ValueError`; non-string values raise `TypeError`. Use `part()` for local styles or nesting.],
)[
```python
# show-code: true
from gaanim import GOLD, Scene, parts
scene = Scene(640, 360, background="#0f172a")
equation = scene.equation(
    "-",
    parts(mass_left="m", gravity="g sin(theta)"),
    "=",
    parts(mass_right="m", length="L", acceleration="theta''"),
).at(0, 0)
equation["gravity"].fill(GOLD)
scene.play([equation.write(1.2, by="part")])
scene.export("compact_text_parts.webp", fps=30)
```
]

#api-entry(
  name: "part",
  kind: "factory",
  signature: "part(name, *content, style=None, font=None, math_font=None, size=None, weight=None, italic=None, color=None, opacity=None, letter_spacing=None, word_spacing=None, baseline=None) -> TextPart",
  params: (
    (name: "name", type: "str", default: none, desc: [Non-empty semantic name, unique among its siblings.]),
    (name: "content", type: "str | TextPart", default: none, desc: [Nested composable content.]),
    (name: "style", type: "TextStyle | None", default: "None", desc: [Typography inherited by the complete subtree.]),
    (name: "direct style", type: "keyword arguments", default: "None", desc: [Convenient local overlay for the listed visual and metric fields.]),
  ),
  returns: (type: "TextPart", desc: [Immutable semantic subtree accepted by `part()` and `scene.text()`.]),
  desc: [Duplicate sibling names, empty names, invalid metrics, or invalid nested content raise `ValueError` or `TypeError`.],
)[
```python
# show-code: true
from gaanim import BLUE, GOLD, Scene, part
scene = Scene(480, 270, background="#0f172a")
formula = part(
    "formula",
    "$",
    part("variable", "x", color=BLUE),
    " dot 5 = ",
    part("result", "25", color=GOLD),
    "$",
)
text = scene.text("Resultado: ", formula).at(0, 0)
scene.play([text.write(1.1, by="part", stagger=0.05)])
scene.export("text_parts.webp", fps=30)
```
]

=== Delimitadores matemáticos

- `$...$` switches the unified Typst compositor into mathematics.
- `scene.equation(*content)` supplies `$ ... $` for a standalone equation;
  omit those delimiters from its content.
- `$$...$$` currently uses the same vector math compositor; it does not create
  a separate public display-math object.
- `\$` produces a literal dollar sign.
- An unmatched delimiter raises `ValueError`.
- If every non-whitespace segment is mathematical, the inferred role is
  `math`; mixed prose and math infer `body` unless `role` is explicit.
- Every boundary between content nodes inside math becomes one ordinary Typst
  whitespace token. Typst itself determines the resulting operator and word
  spacing, so writing `"= "` is unnecessary.
- Local part properties stay inside the same Typst equation. Changing a
  part's color, font, size, weight, italic style, spacing, decoration, or
  baseline never introduces a synthetic `#h()` gap.
- Outside math, boundaries remain exact and no whitespace is inserted.
- Math syntax may still span boundaries. For example,
  `part("x", "x"), "_1"` is compiled as `x _1`; Typst keeps `_1` attached as
  the subscript rather than treating the inserted source whitespace as a
  fixed visual gap.

== TextStyle

#api-entry(
  name: "TextStyle",
  kind: "value",
  signature: "TextStyle(*, font=None, math_font=None, fallbacks=(), size=None, weight=None, italic=None, color=None, stroke=None, stroke_width=None, opacity=None, letter_spacing=None, word_spacing=None, decorations=(), baseline=None)",
  params: (
    (name: "font / math_font", type: "str | None", default: "None", desc: [Primary prose and mathematical font families.]),
    (name: "fallbacks", type: "Sequence[str]", default: "()", desc: [Ordered fallback font families for prose shaping.]),
    (name: "size", type: "float | None", default: "None", desc: [Positive finite size in Typst/canvas points.]),
    (name: "weight", type: "int | None", default: "None", desc: [Numeric weight from 1 through 1000.]),
    (name: "color / stroke", type: "Color | None", default: "None", desc: [Glyph fill and optional outline color.]),
    (name: "opacity", type: "float | None", default: "None", desc: [Whole-Text alpha from 0 through 1.]),
    (name: "spacing", type: "float | None", default: "None", desc: [Non-negative letter and word spacing in points.]),
    (name: "decorations", type: "Sequence[str]", default: "()", desc: [`underline`, `strike`, or `strikethrough`.]),
    (name: "baseline", type: "float | None", default: "None", desc: [Finite baseline offset in points; positive values move glyphs upward.]),
  ),
  returns: (type: "TextStyle", desc: [Reusable immutable typography overlay.]),
  desc: [It intentionally has no box width, height, padding, fit, growth, columns, or vertical alignment. Invalid values raise `ValueError`.],
)[
```python
from gaanim import GOLD, TextStyle

display = TextStyle(
    font="New Computer Modern",
    math_font="New Computer Modern Math",
    fallbacks=("Arial",),
    size=42,
    weight=650,
    italic=False,
    color=GOLD,
    opacity=0.95,
    letter_spacing=0.4,
    word_spacing=1.0,
    decorations=("underline",),
    baseline=0,
)
```
]

Root `stroke`, `stroke_width`, and `opacity` affect the complete `Text`.
Nested parts resolve font, size, weight, italic, color, spacing, decoration, and
baseline through the structured Typst tree. A later fluent `text.fill(...)`,
`text.stroke(...)`, or `text.opacity(...)` updates the complete drawable.

== TextFlow y ajuste de líneas

#api-entry(
  name: "TextFlow",
  kind: "value",
  signature: "TextFlow(*, wrap=\"auto\", align=\"left\", line_spacing=1.2, max_lines=None, overflow=\"clip\", direction=\"auto\", hyphenate=False)",
  params: (
    (name: "wrap", type: "\"auto\" | False | float", default: "\"auto\"", desc: [Use the offered width, preserve a line except explicit newlines, or cap typographic width. Numeric widths must be positive and finite.]),
    (name: "align", type: "left | center | right | justify", default: "\"left\"", desc: [Internal paragraph alignment.]),
    (name: "line_spacing", type: "float", default: "1.2", desc: [Positive line-height multiplier.]),
    (name: "max_lines", type: "int | None", default: "None", desc: [Optional limit of at least one line.]),
    (name: "overflow", type: "visible | clip | ellipsis", default: "\"clip\"", desc: [Behavior beyond `max_lines`.]),
    (name: "direction", type: "auto | ltr | rtl", default: "\"auto\"", desc: [Text direction passed to the compositor.]),
    (name: "hyphenate", type: "bool", default: "False", desc: [Enable Typst hyphenation.]),
  ),
  returns: (type: "TextFlow", desc: [Reusable immutable internal composition options.]),
  desc: [`wrap="auto"` uses Layout v2's offered width or the free scene's safe-frame offer. Direct `scene.text` flow keywords override this object.],
)[
```python
# show-code: true
from gaanim import GOLD, Scene, TextFlow
scene = Scene(480, 270, background="#0f172a", margin=30)
body = scene.text(
    "El mismo texto se mide con el ancho que ofrece su tarjeta de Layout v2.",
    role="body",
    flow=TextFlow(wrap="auto", align="justify", line_spacing=1.25),
)
page = scene.column(
    [scene.text("Texto responsive", role="heading").fill(GOLD), body],
    within="safe", width="fill", height="fill", padding=28, gap=18,
)
scene.play([page.fade_in().duration(0.7)])
scene.export("text_flow.webp", fps=30)
```
]

`overflow="visible"` leaves the limited block unclipped. `"clip"` clips it.
`"ellipsis"` is a distinct public/cache value but currently uses the same
visual clipping as `"clip"`; an ellipsis glyph is not emitted yet.

== Integración con Layout

A free `Text` and that same `Text` inside a row, column, grid, stack, or nested
layout use the same intrinsic text measurer. `wrap="auto"` makes the leaf
width-sensitive. Numeric wrapping is further limited by the owner's offered
constraints.

Metric changes and `become`, `morph_to`, `step_to`, or `expand_to` invalidate
measurement and request reflow from the owning Layout. A replacing transition
uses the same duration for text and reflow. Transient `indicate`, `pulse`,
`wiggle`, `wave`, `highlight`, and `focus` do not change measurement.

Layout owns translation. Once managed, a `Text` rejects manual placement such
as `at`, `move`, `next_to`, and positional animations; configure its
`scene.item(...)` or Layout owner instead. Cross-scene or incompatible-owner
transition targets raise `LayoutOwnershipError`.

== Consultas y selecciones

#api-entry(
  name: "TextSelection and TextQuery",
  kind: "method",
  signature: "text[name] | text[index_or_slice] | text.graphemes|words|lines|parts[index_or_slice] -> TextSelection",
  params: (
    (name: "name", type: "str", default: none, desc: [A top-level part; continue indexing to navigate nested semantic paths.]),
    (name: "index", type: "int", default: none, desc: [Supports negative indices.]),
    (name: "slice", type: "slice", default: none, desc: [Contiguous non-empty range; step must equal 1.]),
  ),
  returns: (type: "TextSelection", desc: [Deferred local selection inside its owning `Text`.]),
  desc: [Direct numeric indexing selects rendered Unicode graphemes. Query views expose graphemes, Unicode words, explicit lines, and semantic parts. Missing names and invalid ranges raise `KeyError`, `IndexError`, `TypeError`, or `ValueError`.],
)[
```python
# show-code: true
from gaanim import BLUE, GOLD, Scene, part
scene = Scene(480, 270, background="#0f172a")
copy = scene.text(
    "La ", part("concept", "energía"), " depende de ",
    part("formula", "$", part("mass", "m"), " c^2$"),
).at(0, 0)
copy["concept"].fill(GOLD)
copy["formula"]["mass"].fill(BLUE)
scene.play([copy.write(1.0, by="word")])
scene.play([copy.words[1].pulse(0.6), copy["formula"]["mass"].focus(0.6)])
scene.export("text_selection.webp", fps=30)
```
]

The current `lines` query follows explicit `\n` boundaries in the structured
source. It does not expose lines created only by responsive wrapping. `parts`
is depth-first over the semantic tree. A selection remains attached to one
`Text`, so it cannot be inserted separately into Layout.

`selection.fill(color)` recompiles a semantic part with its local paint. In
math, the styled part remains in the same Typst equation as its neighbors, so
the color change does not add whitespace or move adjacent terms.

=== Superficie de TextSelection

```text
selection.fill(color) -> TextSelection
selection.color_to(color, duration=None) -> Anim
selection.opacity_to(opacity, duration=None) -> Anim
selection.animate().fill(color).opacity(value) -> Anim

selection.indicate(duration=None) -> Anim
selection.pulse(duration=None) -> Anim
selection.wiggle(duration=None) -> Anim
selection.wave(duration=None) -> Anim
selection.highlight(duration=None) -> Anim
selection.focus(duration=None) -> Anim
selection.cancel(duration=None) -> Anim

selection.reveal(*, style="fade", duration=None) -> Anim
selection.brace(label, *, above=False, duration=None) -> Anim
selection.annotate(label, *, offset=(120, 80), duration=None) -> Anim

selection.morph_to(target_selection, *, duration=None) -> Anim
selection.copy_to(target_selection, *, duration=None) -> Anim
```

The compound `animate()` builder is deliberately local: it accepts fill/color
and opacity, while transform, scale, rotation, material, and stroke targets
raise `TypeError`. `reveal` accepts `fade`, `wipe`, or `from_below`. `cancel` draws a diagonal
mark and dims the glyphs; the next replacing text transition retires both.
`brace` and `annotate` use canvas-unit geometry around the selected glyphs.
Every animation descriptor above can be placed directly in `scene.play([...])`.

== Animaciones de texto completo

=== Entrada y salida

```text
text.write(duration=None, *, by="grapheme", order="forward", stagger=0) -> Anim
text.type_in(duration=None, *, by="grapheme", order="forward", stagger=0.04) -> Anim
text.reveal(duration=None, *, by="grapheme", order="forward", stagger=0) -> Anim
text.fade_in(duration=None) -> Anim
text.slide_in(direction="up", *, distance=24, duration=None) -> Anim

text.unwrite(duration=None) -> Anim
text.erase(duration=None) -> Anim
text.fade_out(duration=None) -> Anim
text.slide_out(direction="down", *, distance=24, duration=None) -> Anim
```

`by` accepts `grapheme`, `word`, `line`, or `part`; `order` accepts `forward`,
`reverse`, `center`, or `random`; `stagger` must be finite and non-negative.
The duration is the first optional positional argument, so `text.write(0.8)`
and `text.write(0.8, by="word")` are the intended forms. In the current
renderer, `by="part"` has a dedicated semantic schedule; the other grouping,
order, and stagger values are validated but share the vector write schedule.

=== Énfasis y anotación

```text
text.indicate(duration=None) -> Anim
text.pulse(duration=None) -> Anim
text.wiggle(duration=None) -> Anim
text.wave(duration=None) -> Anim
text.highlight(duration=None) -> Anim
text.focus(duration=None) -> Anim
text.cancel(duration=None) -> Anim
text.brace(label, *, above=False, duration=None) -> Anim
text.annotate(label, *, offset=(120, 80), duration=None) -> Anim
```

These operate on the complete `Text`; their selection equivalents use the
same engine on a local subset.

== Transiciones estructurales

#api-entry(
  name: "Text structural transitions",
  kind: "method",
  signature: "morph_to(target, *, match=\"auto\", duration=1.0) | step_to(target, *, matches=None, duration=1.0) | expand_to(target, *, anchor=\"part\", duration=1.0) -> Anim",
  params: (
    (name: "target", type: "Text", default: none, desc: [Destination text in the same scene and a compatible Layout ownership context.]),
    (name: "match", type: "auto | semantic | grapheme | shape", default: "\"auto\"", desc: [General matching strategy for `morph_to`.]),
    (name: "matches", type: "Mapping[str,str] | Sequence[tuple[str,str]] | None", default: "None", desc: [Explicit semantic path pairs for `step_to`.]),
    (name: "anchor", type: "str", default: "\"part\"", desc: [Shared semantic path for `expand_to`; `part` chooses the first shared path.]),
    (name: "duration", type: "float", default: "1.0", desc: [Positive finite seconds, shared with Layout reflow.]),
  ),
  returns: (type: "Anim", desc: [Deferred structural transition accepted by `scene.play()`.]),
  desc: [Semantic paths are considered before remaining glyph and shape matches. Unmatched source/target glyphs use exit/entry behavior. Ownership violations raise `LayoutOwnershipError`; a missing automatic expansion anchor raises `KeyError`.],
)[
```python
# show-code: true
from gaanim import GOLD, Scene, part
scene = Scene(480, 270, background="#0f172a")
before = scene.text("$x + ", part("obsolete", "3"), " = 7$").at(0, 0)
after = scene.text("$x = ", part("result", "4", color=GOLD), "$").at(0, 0)
scene.play([before.write(0.8)])
scene.play([before["obsolete"].cancel(0.5)])
scene.play([before.step_to(after, duration=0.8)])
scene.export("text_transition.webp", fps=30)
```
]

Matching behavior:

- `morph_to`: general transition; `auto` favors semantic paths, then remaining
  glyph/shape matching.
- `step_to`: derivation/equation step with optional explicit path mapping.
- `expand_to`: one-to-many expansion around a shared semantic part.
- `selection.morph_to`: replaces one local selection with another.
- `selection.copy_to`: preserves the source while moving a copy to the target.

== Sustitución de contenido con become

```text
text.become(
    *content,
    role=None,
    style=None,
    flow=None,
    duration=1.0,
) -> None
```

`become` preserves the Python `Text` identity, increments its structured
version, replaces content, and requests owner reflow. It is an immediate
authoring call, not an `Anim`, so do not put it inside `scene.play`. Invalid
content, delimiters, metrics, flow, or duration raise `TypeError` or
`ValueError`.

```text
copy.become("Resultado: ", part("value", "$42$", color=GOLD), duration=0.8)
```

== Capacidades heredadas de Drawable

`Text` remains a `Drawable`. It preserves its specialized subtype through the
common fluent methods:

```text
text.fill(color).stroke(color, width).opacity(alpha).z_index(layer)
text.at(x, y).at_3d(x, y, z).next_to(other, direction)
text.align_to(other, anchor).to_edge(direction).to_corner(anchor)
text.scaled(factor).rotated(radians).with_pivot(x, y)
text.billboard().hud()
```

Free text can also use generic Drawable animations such as move, rotate,
scale, fade transforms, or replacement transforms. Prefer the structural text
transitions when semantic matching matters. Layout-managed text retains visual
effects and non-positional transforms, but its owner controls translation.

== Errores y casos límite

- `TypeError`: content is not `str` or `TextPart`; query keys or explicit
  match mappings have the wrong type.
- `ValueError`: empty content, unbalanced math, duplicate sibling parts,
  invalid role/style/flow, invalid grouping/order/stagger, invalid direction,
  or non-positive transition duration.
- `KeyError`: unknown semantic part, no shared automatic expansion anchor.
- `IndexError`: query index out of range or empty slice.
- `ValueError`: selection slices with a step other than 1.
- `LayoutOwnershipError`: manual positioning of managed text or a transition
  across scenes/incompatible owners.

== Véase también

- #link("/api/layout/", "Layout v2 — outer boxes, ownership, constraints, and reflow")
- #link("/api/animations/", "Animations — generic Drawable animations and timing")
- #link("/api/themes/", "Themes — role colors and typography context")
- #link("/api/mobjects/", "Mobjects — raw Typst documents and other drawables")
