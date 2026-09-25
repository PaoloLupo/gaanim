#import "../../components/section.typ": docs-chapter
#import "../../components/api.typ": api-entry

#show: docs-chapter.with(
  title: "Texto",
  description: "Prosa, matemáticas, partes semánticas, flujo responsive, selecciones y animación estructural",
  route: "/api/text/",
)

= Texto

`scene.text()` es la fábrica general de prosa, títulos, párrafos, matemáticas y
contenido mixto. `scene.text.equation()` es su atajo para matemáticas en bloque.
Ambas devuelven el mismo `Text` especializado: un `Drawable` vectorial que
conserva la estructura semántica, es medido intrínsecamente por Layout v2 y
expone selecciones locales y animaciones específicas de texto.

```python
from gaanim import GOLD, Scene, TextFlow, TextStyle, part

scene = Scene(frame=(16, 9), background="#0f172a")
formula = part("formula", "$E = ", part("mass", "m", color=GOLD), " c^2$")
copy = scene.text(
    "La energía es ",
    formula,
    role="body",
    style=TextStyle(size=0.34),
    flow=TextFlow(wrap=6.5, align="center"),
).move_to(0, 0)

scene.play([copy.animate.write(by="part", stagger=0.06).duration(1.2)])
scene.play([copy["formula"]["mass"].animate.indicate().duration(0.6)])
```

Usa un rol para la prosa y `scene.text.equation()` para una ecuación independiente.
`scene.text.typst()` permite crear documentos Typst arbitrarios, pero no ofrece la
API de selección estructurada de `Text` descrita aquí.

== Responsabilidades

- `TextStyle` controla el aspecto de los glifos y las métricas tipográficas.
- `TextFlow` controla la composición interna de líneas.
- Solo Layout v2 controla caja exterior, relleno, ajuste, crecimiento, pistas,
  restricciones, propiedad y recomposición.
- `TextSelection` señala glifos dentro de un `Text`; nunca se convierte en hijo
  independiente de Layout.

Esta separación evita tener un segundo solucionador de cajas de texto. Consulta
#link("/api/layout/", "Layout v2") para dimensionar y colocar contenedores.

== Scene.text

#api-entry(
  name: "Scene.text",
  kind: "factory",
  signature: "text(*content, role=None, style=None, flow=None, font=None, math_font=None, size=None, weight=None, italic=None, color=None, opacity=None, letter_spacing=None, word_spacing=None, baseline=None, wrap=None, text_align=None, line_spacing=None, max_lines=None, overflow=None, direction=None, hyphenate=None, lang=None, markup=None) -> Text",
  params: (
    (name: "content", type: "str | TextPart | TextParts", default: none, desc: [One or more composable strings, semantic parts, or compact ordered part groups. The flattened result must not be empty.]),
    (name: "role", type: "TextRole | None", default: "None", desc: [Semantic role. Fully mathematical content infers `math`; everything else infers `body`.]),
    (name: "style", type: "TextStyle | None", default: "None", desc: [Reusable visual and metric overlay.]),
    (name: "flow", type: "TextFlow | None", default: "None", desc: [Reusable internal line-composition options.]),
    (name: "style overrides", type: "keyword arguments", default: "None", desc: [Direct font, metric, color, opacity, spacing, and baseline values.]),
    (name: "flow overrides", type: "keyword arguments", default: "None", desc: [Direct wrap, alignment, line limit, overflow, direction, hyphenation, and `lang` values.]),
    (name: "markup", type: "bool | None", default: "None", desc: [Interpret `*strong*` and `_emphasis_`. `False` keeps `*` and `_` literal while `$...$` math still applies. `None` uses the theme's `text_markup`, which is `True` by default.]),
  ),
  returns: (type: "Text", desc: [Structured vector text measured by the same intrinsic Layout v2 pass in every context.]),
  desc: [Direct keywords override `TextStyle` and `TextFlow`. The `color` argument
    accepts `ColorLike`: a `Color`, CSS/hex string, or RGB/RGBA tuple of bytes.
    `None` inherits the style/theme color. This also applies to `equation`.
    Invalid content, delimiters, roles, metrics, or flow values raise `TypeError` or `ValueError`.],
)[
```python
# show-code: true
from gaanim import GOLD, Scene, TextFlow, part
scene = Scene(frame=(16, 9), background="#0f172a")
formula = part("formula", "$E = ", part("mass", "m", color=GOLD), " c^2$")
copy = scene.text(
    "La energía es ", formula,
    role="body",
    flow=TextFlow(wrap=5, align="center", line_spacing=1.2),
).move_to(0, 0)
scene.play([copy.animate.write(by="part").duration(1.0)])
# output: text_factory.webp
scene.render()
```
]

== Typography.equation

#api-entry(
  name: "Typography.equation",
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
scene = Scene(frame=(16, 9), background="#0f172a")
equation = scene.text.equation(
    part("sum_force", "sum F_t"),
    "=",
    parts(mass="m", acceleration="a_t"),
).move_to(0, 0)
equation["acceleration"].fill(GOLD)
scene.play([equation.animate.write(by="part").duration(1.0)])
# output: equation_factory.webp
scene.render()
```
]

== Roles

The accepted roles are:

```text
"title" | "subtitle" | "kicker" | "heading" | "body" |
"caption" | "label" | "code" | "math"
```

The default text configuration uses these sizes in scene units:
`title=0.64`, `subtitle=0.48`, `kicker=0.32`, `heading=0.48`, `body=0.40`,
`caption=0.32`, `label=0.36`, `code=0.36`, and `math=0.44`. Prose uses New Computer
Modern by default, code uses Consolas, and math uses New Computer Modern Math.
The active theme and explicit style may replace the resolved color and
typography. Under a theme, `kicker` resolves to the palette's `accent` color,
which makes it the natural small line above a title:

```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
kicker = scene.text("MISMO TERREMOTO. TRES EDIFICIOS.", role="kicker").move_to(0, 3.77)
title = scene.text("¿Cuál sufrirá más?", role="title").move_to(0, 3)
```

Resolution order is:

```text
role/theme -> TextStyle/TextFlow -> direct scene.text keywords
           -> local part style -> later TextSelection.fill / Text.fill
```

== Medición sin spawn

#api-entry(
  name: "Typography.measure",
  kind: "method",
  signature: "measure(content, *, role=None, size=None, font=None, color=None, wrap=None, weight=None, style=None, markup=None, flow=None, line_spacing=None) -> tuple[float, float]",
  params: (
    (name: "content", type: "str", default: none, desc: [Text to measure; must not be empty.]),
    (name: "role", type: "TextRole | None", default: "None", desc: [Role whose theme defaults resolve size, family, and color (`body` when omitted).]),
    (name: "size, font, color", type: "float | str | Color | None", default: "None", desc: [Explicit overrides, resolved exactly as on the spawned text object.]),
    (name: "wrap", type: "float | None", default: "None", desc: [Fixed composition width; `None` measures a single unwrapped block.]),
    (name: "weight", type: "int | None", default: "None", desc: [Font weight override.]),
    (name: "style", type: "TextStyle | None", default: "None", desc: [Reusable typography (italic, spacing, …); `size`, `font`, `weight` and `color` override it.]),
    (name: "markup", type: "bool | None", default: "None", desc: [Same as `scene.text`: `False` measures `*` and `_` as literal characters; `None` follows the theme.]),
    (name: "flow", type: "TextFlow | None", default: "None", desc: [Paragraph settings (alignment, line spacing, hyphenation, …). Its `"auto"` wrap measures unwrapped because no layout width is offered.]),
    (name: "line_spacing", type: "float | None", default: "None", desc: [Line-height multiplier, as in `scene.text`; overrides `flow`.]),
  ),
  returns: (type: "tuple[float, float]", desc: [Laid-out `(width, height)` in scene units.]),
  desc: [Runs the same Typst pipeline that renders `scene.text` and shares its cache, so a later spawn of the same text reuses the measurement. Use it to size boxes to their content instead of guessing widths.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
width, height = scene.text.measure("PGA = 0.35 g", role="label")
_, paragraph_height = scene.text.measure("Primera línea\nSegunda línea", line_spacing=1.6)
box = scene.geometry.rounded_rect(width + 0.56, height + 0.32, 0.14).move_to(0, -4.14)
```
]

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

scene = Scene(frame=(16, 9), background="#0f172a")
copy = scene.text(
    "Normal, _emphasis_, *strong* and *_both_*.",
    size=0.36,
).move_to(0, 0)
scene.play([copy.animate.write(by="word", stagger=0.05).duration(1.2)])
scene.play([copy.words[3].animate.indicate().duration(0.6)])
# output: text_inline_markup.webp
scene.render()
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
- For technical content full of `_` or `*` (labels such as `tb:dist_comp`,
  identifiers such as `X1_2`), pass `markup=False`: every `*` and `_` stays
  literal, backslashes included, while `$...$` math still applies.
  `text.become(...)` keeps the current mode unless `markup=` is given.
- `Theme(text_markup=False)` makes that the default for `scene.text`,
  `scene.text.measure`, `badge` and `chip`; a call that passes `markup=`
  keeps its own choice.

```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
label = scene.text("Valores: V_e del piso 1 (tb:agriet_xy)", markup=False)
```

#api-entry(
  name: "parts",
  kind: "factory",
  signature: "parts(mapping: Mapping[str, str] | None = None, /, **content: str) -> TextParts",
  params: (
    (name: "mapping", type: "Mapping[str, str] | None", default: "None", desc: [Ordered names and their plain text, named with strings as in `part()`. Keeps insertion order and accepts names that are not Python identifiers, such as `"tb:dist"`.]),
    (name: "content", type: "keyword str entries", default: none, desc: [Shortcut for identifier names: `parts(mass="m")` equals `parts({"mass": "m"})`.]),
  ),
  returns: (type: "TextParts", desc: [Immutable ordered group accepted by `scene.text()`, `scene.text.equation()`, `Text.become()`, and `part()`.]),
  desc: [Inside `$...$`, adjacent sibling entries become distinct Typst math tokens and retain Typst's native tight spacing. Empty input, mixing a mapping with keyword entries, repeated or empty names, or wholly empty content raise `ValueError`; a non-mapping positional argument, non-string names, or non-string values raise `TypeError`. Use `part()` for local styles or nesting.],
)[
```python
# show-code: true
from gaanim import GOLD, Scene, parts
scene = Scene(frame=(16, 9), background="#0f172a")
equation = scene.text.equation(
    "-",
    parts(mass_left="m", gravity="g sin(theta)"),
    "=",
    parts(mass_right="m", length="L", acceleration="theta''"),
).move_to(0, 0)
scene.play([equation.animate.write(by="part").duration(1.2)])
scene.play([equation["gravity"].animate.indicate().duration(0.6)])
scene.play([equation["acceleration"].animate.fill(GOLD).duration(0.6)])
# output: compact_text_parts.webp
scene.render()
```

A mapping names the parts with strings, like `part()` does, and admits names
that keyword arguments cannot express:

```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
label = scene.text(parts({"tb:dist": "d = ", "x-1": "4.2 m"}))
label["tb:dist"].fill(GOLD)
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
scene = Scene(frame=(16, 9), background="#0f172a")
formula = part(
    "formula",
    "$",
    part("variable", "x", color=BLUE),
    " dot 5 = ",
    part("result", "25", color=GOLD),
    "$",
)
text = scene.text("Resultado: ", formula).move_to(0, 0)
scene.play([text.animate.write(by="part", stagger=0.05).duration(1.1)])
# output: text_parts.webp
scene.render()
```
]

=== Delimitadores matemáticos

- `$...$` switches the unified Typst compositor into mathematics.
- `scene.text.equation(*content)` supplies `$ ... $` for a standalone equation;
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
    (name: "size", type: "float | None", default: "None", desc: [Positive finite size in scene units.]),
    (name: "weight", type: "int | None", default: "None", desc: [Numeric weight from 1 through 1000.]),
    (name: "color / stroke", type: "Color | None", default: "None", desc: [Glyph fill and optional outline color.]),
    (name: "opacity", type: "float | None", default: "None", desc: [Whole-Text alpha from 0 through 1.]),
    (name: "spacing", type: "float | None", default: "None", desc: [Non-negative letter and word spacing in scene units.]),
    (name: "decorations", type: "Sequence[str]", default: "()", desc: [`underline`, `strike`, or `strikethrough`.]),
    (name: "baseline", type: "float | None", default: "None", desc: [Finite baseline offset in scene units; positive values move glyphs upward.]),
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
    size=0.42,
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
  signature: "TextFlow(*, wrap=\"auto\", align=\"left\", line_spacing=1.2, max_lines=None, overflow=\"clip\", direction=\"auto\", hyphenate=False, lang=None)",
  params: (
    (name: "wrap", type: "\"auto\" | False | float", default: "\"auto\"", desc: [Use the offered width, preserve a line except explicit newlines, or cap typographic width. Numeric widths must be positive and finite.]),
    (name: "align", type: "left | center | right | justify", default: "\"left\"", desc: [Internal paragraph alignment.]),
    (name: "line_spacing", type: "float", default: "1.2", desc: [Positive line-height multiplier.]),
    (name: "max_lines", type: "int | None", default: "None", desc: [Optional limit of at least one line.]),
    (name: "overflow", type: "visible | clip | ellipsis", default: "\"clip\"", desc: [Behavior beyond `max_lines`.]),
    (name: "direction", type: "auto | ltr | rtl", default: "\"auto\"", desc: [Text direction passed to the compositor.]),
    (name: "hyphenate", type: "bool", default: "False", desc: [Enable Typst hyphenation.]),
    (name: "lang", type: "str | None", default: "None", desc: [Lowercase ISO 639 code (`"es"`, `"en"`, …) choosing hyphenation patterns and language typography. `None` keeps the English default; other values raise `ValueError`. `scene.text(..., lang="es")` sets it directly.]),
  ),
  returns: (type: "TextFlow", desc: [Reusable immutable internal composition options.]),
  desc: [`wrap="auto"` uses Layout v2's offered width or the free scene's safe-frame offer. Direct `scene.text` flow keywords override this object.],
)[
```python
# show-code: true
from gaanim import GOLD, Scene, TextFlow
scene = Scene(frame=(16, 9), background="#0f172a", margin=0.375)
body = scene.text(
    "El mismo texto se mide con el ancho que ofrece su tarjeta de Layout v2.",
    role="body",
    flow=TextFlow(wrap="auto", align="justify", line_spacing=1.25),
)
page = scene.layout.column(
    [scene.text("Texto responsive", role="heading").fill(GOLD), body],
    within="safe", width="fill", height="fill", padding=0.35, gap=0.225,
)
scene.play([page.animate.fade_in().duration(0.7)])
# output: text_flow.webp
scene.render()
```
]

When `scene.text` receives neither `flow` nor `text_align`, an explicit anchor
in `move_to` also chooses the alignment of a text with explicit line breaks: `Anchor.*_LEFT` and
`TextAnchor.BASELINE_LEFT` align left, `*_RIGHT` align right, and centered
anchors center. A right-anchored note therefore reads flush right without
repeating the side:

```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
note = scene.text("Fuente: ensayo 3\nEscala 1:50").move_to(7.5, -4, Anchor.BOTTOM_RIGHT)
```

An explicit `flow` or `text_align` always wins. Text without an anchor, and
single-line text, keep the default left alignment: centering a single line
would not move its glyphs, but it would widen its layout box and any gradient
mapped to it.

`overflow="visible"` leaves the limited block unclipped. `"clip"` clips it.
`"ellipsis"` is a distinct public/cache value but currently uses the same
visual clipping as `"clip"`; an ellipsis glyph is not emitted yet.

== Integración con Layout

A free `Text` and that same `Text` inside a row, column, grid, stack, or nested
layout use the same intrinsic text measurer. `wrap="auto"` makes the leaf
width-sensitive. Numeric wrapping is further limited by the owner's offered
constraints.

Metric changes and `become` invalidate measurement and request reflow from the
owning Layout. A replacing `text.animate.transform_to(target)` transition uses
the same duration for text and reflow. Transient `indicate`, `pulse`,
`wiggle`, `wave`, `highlight`, and `focus` do not change measurement.

Layout owns translation. Once managed, a `Text` rejects manual placement such
as `move_to`, `shift_by`, `next_to`, and positional animations; configure its
`scene.layout.item(...)` or Layout owner instead. Cross-scene or incompatible-owner
transition targets raise `LayoutOwnershipError`.

== Consultas y selecciones

#api-entry(
  name: "TextSelection and TextQuery",
  kind: "method",
  signature: "text[name] | text[index_or_slice] | text.graphemes|words|lines|parts[index_or_slice] -> TextSelection",
  params: (
    (name: "name", type: "str", default: none, desc: [A top-level part; continue indexing to navigate nested semantic paths. A string that names no part selects every literal occurrence of that text instead (case- and whitespace-insensitive).]),
    (name: "index", type: "int", default: none, desc: [Supports negative indices.]),
    (name: "slice", type: "slice", default: none, desc: [Contiguous non-empty range; step must equal 1.]),
  ),
  returns: (type: "TextSelection", desc: [Deferred local selection inside its owning `Text`.]),
  desc: [Direct numeric indexing selects rendered Unicode graphemes. Query views expose graphemes, Unicode words, explicit lines, and semantic parts. A grapheme, word, or line slice selects the rendered text from its first to its last unit, including punctuation between words, at the position where the slice starts. Strings that are neither a part nor text in the `Text`, and invalid ranges, raise `KeyError`, `IndexError`, `TypeError`, or `ValueError`.],
)[
```python
# show-code: true
from gaanim import BLUE, GOLD, Scene, part
scene = Scene(frame=(16, 9), background="#0f172a")
copy = scene.text(
    "La ", part("concept", "energía"), " depende de ",
    part("formula", "$", part("mass", "m"), " c^2$"),
).move_to(0, 0)
copy["concept"].fill(GOLD)
copy["formula"]["mass"].fill(BLUE)
scene.play([copy.animate.write(by="word").duration(1.0)])
scene.play(copy.words[1].animate.pulse().duration(0.6))
scene.play(copy["formula"]["mass"].animate.focus().duration(0.6))
# output: text_selection.webp
scene.render()
```
]

`text.words` follows Unicode word boundaries, so punctuation such as `,` or
`:` is not a word: `"uno dos, tres"` has three words and `words[1:3]` selects
`dos, tres`. `text["dos, tres"]` selects the same literal text by content.

The current `lines` query follows explicit `\n` boundaries in the structured
source. It does not expose lines created only by responsive wrapping. `parts`
is depth-first over the semantic tree. A selection remains attached to one
`Text`, so it cannot be inserted separately into Layout.

`selection.fill(color)` recompiles a semantic part with its local paint. In
math, the styled part remains in the same Typst equation as its neighbors, so
the color change does not add whitespace or move adjacent terms.

Selection animations resolve the authored mathematical source to the glyphs
that Typst emitted. Literal matching has priority; when it finds nothing,
Typst's math parser and Codex convert symbol names and modifiers such as
`theta`, `sum`, or `arrow.r.long`, official shorthands such as `<=`, and prime
syntax such as `theta''` to the same Unicode used by the rendered equation.
Unknown identifiers such as `sin` remain unchanged.

=== Superficie de TextSelection

```text
selection.fill(color) -> TextSelection
selection.marker(color=None, *, skew=0.05, blend="normal", opacity=0.45, padding=None) -> TextSelection
selection.animate.fill(color).duration(seconds) -> Anim
selection.animate.opacity(opacity).duration(seconds) -> Anim
selection.animate.fill(color).opacity(value) -> Anim

selection.animate.indicate() -> Anim
selection.animate.pulse() -> Anim
selection.animate.wiggle() -> Anim
selection.animate.wave() -> Anim
selection.animate.highlight() -> Anim
selection.animate.focus() -> Anim
selection.animate.cancel() -> Anim
selection.animate.reveal(style="fade") -> Anim
selection.animate.brace(label="", *, above=False) -> Anim
selection.animate.annotate(label, offset=(0, 0.6)) -> Anim
selection.animate.marker(color=None, *, skew=0.05, blend="normal", opacity=0.45, padding=None) -> Anim

selection.animate.morph_to(target_selection).duration(seconds) -> Anim
selection.animate.copy_to(target_selection).duration(seconds) -> Anim
```

The `animate` proxy is deliberately local: it accepts fill
and opacity, while transform, scale, rotation, material, and stroke targets
raise `TypeError`. `cancel` draws a diagonal mark and dims the glyphs; the next
replacing text transition retires both. `reveal` makes only the selected glyphs
appear with `"fade"`, `"wipe"` (stroke trace, then fill) or `"from_below"` (fade
with a short rise), so a term can enter an equation that is already visible.
`brace` draws a brace under the selection, or over it with `above=True`, and
fades in `label`; `annotate` places `label` at `offset` from the selection
center with a leader line. The brace, line, and labels are new objects in the
selection's color and remain on screen.

`marker` sweeps a highlighter band behind every rendered line the selection
spans. Each band covers the full glyph
height of its line plus `padding` (world units; `None` uses 10% of the line
height), grows from the left edge, and the lines follow one another, sharing
the duration by band length. `color` defaults to a highlighter yellow whose
alpha is multiplied by `opacity`; `skew` tilts each band in radians (positive
rises to the right) and is capped at a quarter of the band thickness so long
lines stay covered. Bands are drawn behind the glyphs, at the text's
`z_index`, but above objects authored before the text, such as a card. Per-object
blend modes do not exist yet, so `blend="multiply"` is accepted and composited
like `"normal"`; because the band sits behind the glyphs, the text keeps its
color either way. An unknown `blend`, a non-finite `skew`, `opacity` outside
`[0, 1]`, or a negative `padding` raises `ValueError`. The static
`selection.marker(...)` places the finished bands at the current timeline
cursor. Like the brace, the bands are new objects that stay on screen and do
not follow later movements of the text.

```python
# show-code: true
from gaanim import Scene
scene = Scene(frame=(16, 9), background="#f8f5ee")
quote = scene.text("Lo que no se mide no se puede mejorar", color="#1f2937").move_to(0, 0)
scene.play([quote.words[4:9].animate.marker(skew=0.04).duration(1.2)])
# output: text_marker.webp
scene.render()
```
Every animation descriptor above can be placed directly in `scene.play([...])`.

== Animaciones de texto completo

=== Entrada y salida

```text
text.animate.write(*, by="grapheme", order="forward", stagger=0) -> Anim
text.animate.fade_in() -> Anim

text.animate.unwrite() -> Anim
text.animate.fade_out() -> Anim
```

`by` accepts `grapheme`, `word`, `line`, or `part`; `order` accepts `forward`,
`reverse`, `center`, or `random`; `stagger` must be finite and non-negative.
The glyphs of one word, line, or part start together, with the segmentation of
`text.words`, `text.lines` (explicit `\n` lines), and `text.parts`; a glyph
belongs to its innermost part. Punctuation and text outside every group join the
neighboring group. Groups start in `order`: `center` starts the middle group
first and then those equally far from it, and `random` is a fixed pseudo-random
permutation, so previews, seeks, and exports agree. `stagger` is the lag ratio
between consecutive groups; `None` adapts it to the number of groups.
Timing is configured uniformly after choosing the effect, so
`text.animate.write().duration(0.8)` and
`text.animate.write(by="word").duration(0.8)` are the intended forms.

=== Revelados con máscara, blur-in y tracking

```text
text.animate.reveal(style="slide_up", *, by="line", mask=True, stagger=0.06) -> Anim
text.animate.conceal(style="slide_up", *, by="line", mask=True, stagger=0.06) -> Anim
text.animate.blur_in(sigma=0.3, *, by="grapheme", stagger=0.02) -> Anim
text.animate.tracking(value) -> Anim
text.tracking(value) -> Text
```

`reveal` hace entrar el texto unidad por unidad, con `stagger` segundos entre
unidades. `by` acepta `grapheme`, `word`, `line` (líneas explícitas `\n`) o
`part`, con la segmentación de `text.graphemes`/`words`/`lines`/`parts`. Con
`style="slide_up"` (predeterminado) cada unidad sube una altura de fila desde
detrás de una máscara vectorial recortada a su fila, así que también funciona en
el export SVG; `"slide_down"` cae desde arriba. Con `mask=False` los
deslizamientos recorren menos distancia y aparecen con un fundido. `"fade"`,
`"scale"` y `"blur"` ignoran `mask`. La duración cubre toda la cascada: si el
`stagger` no cabe, se comprime. `easing` suaviza cada unidad (ease-out cúbico
por defecto). Los glifos permanecen ocultos hasta que empieza el revelado, como
con `fade_in`. `conceal` es la salida correspondiente: desde el reposo, las
unidades salen en orden de lectura, la primera primero, con `stagger` segundos
entre ellas, y quedan ocultas. Los deslizamientos conservan la dirección del
revelado: con `"slide_up"` cada unidad sale hacia arriba por el borde de su
máscara y con `"slide_down"` hacia abajo; `"fade"`, `"scale"` y `"blur"`
vuelven a su estado oculto. Las unidades aceleran al salir (ease-in cúbico por
defecto).

`blur_in` hace entrar cada unidad desde un desenfoque gaussiano transparente de
`sigma` unidades de escena, el mismo efecto que `blur`. `tracking` añade `value`
unidades de escena entre glifos vecinos sin volver a componer el párrafo: cada
fila crece desde su borde izquierdo, su centro o su borde derecho según la
alineación del texto, y `0` restaura el espaciado original. `text.tracking(v)`
fija el valor al instante. `scene.play` rechaza dos animaciones simultáneas que
escriben el mismo canal de un glifo (por ejemplo `tracking` con un `reveal` que
desliza), así que combina `tracking` con animaciones que no mueven glifos, como
`blur_in`.

En una selección (`text["x"].animate.reveal("from_below")`) `reveal` conserva
su forma local con `"fade"`, `"wipe"` o `"from_below"`; ahí `by`, `mask` y
`stagger` lanzan `TypeError`. Sobre un `Drawable` que no es `Text` estas
animaciones lanzan `TypeError`, y un estilo, una unidad o un valor negativo
lanzan `ValueError`.

```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
>>>title = scene.text("Tipografía cinética", role="title").move_to(0, 1.5)
>>>quote = scene.text("Lo que no se mide no se puede mejorar").move_to(0, 0)
>>>headline = scene.text("Titular\nen dos líneas", role="heading").move_to(0, -2)
title.animate.reveal(by="line", style="slide_up", mask=True, stagger=0.06)
quote.animate.reveal(by="word", style="blur", stagger=0.04)
headline.animate.conceal(by="line", style="slide_up")

title.tracking(0.4)
scene.play([
    title.animate.blur_in(sigma=0.3, by="grapheme", stagger=0.02).duration(1.2),
    title.animate.tracking(0.0).duration(1.2),
])
```

=== Animador de rango

#api-entry(
  name: "Text.animator",
  kind: "method",
  signature: "animator(by=\"grapheme\", shape=\"smooth\", order=\"forward\", seed=0) -> TextAnimator",
  params: (
    (name: "by", type: "str", default: "\"grapheme\"", desc: [Unidad: `grapheme`, `word`, `line` (explícita) o `part`. La puntuación se une a su vecina.]),
    (name: "shape", type: "str", default: "\"smooth\"", desc: [Perfil del selector. `square`, `ramp`, `smooth`, `ease_in` y `ease_out` llevan cada unidad del estado "fuera" al reposo (revelado); `triangle` y `round` suben al estado "fuera" y vuelven (ola).]),
    (name: "order", type: "str", default: "\"forward\"", desc: [Qué unidad alcanza primero el rango: `forward`, `reverse`, `center` o `random`.]),
    (name: "seed", type: "int", default: "0", desc: [Fija la permutación de `order="random"`.]),
  ),
  returns: (type: "TextAnimator", desc: [Selector de rango reutilizable sobre las unidades del texto.]),
  desc: [Motor por glifo al estilo de los Text Animators de After Effects. `animator.set(offset=None, opacity=None, scale=None, rotation=None, blur=None, tracking=None, color=None)` define el estado "fuera" (influencia 1) y devuelve el animador: `offset` en unidades de escena, `opacity` absoluta, `scale` y `rotation` (radianes, menos de media vuelta) alrededor del centro de cada unidad, `blur` como sigma gaussiana, `tracking` entre glifos y `color` como relleno sólido. `animator.animate` es un proxy `TextAnimatorAnimation` de solo lectura, y `animator.animate.sweep(start=0.0, end=1.0, *, stagger=None)` devuelve un `Anim` que mueve el rango de `start` a `end`: `0` está antes de la primera unidad y `1` después de la última, y `sweep(1, 0)` lo recorre hacia atrás. `stagger` es el retraso en segundos entre unidades (`None` lo adapta). `easing` suaviza la transición de cada unidad. Rust evalúa la influencia de cada unidad a partir de su índice normalizado y la forma del selector, así que un seek a `t` coincide con la reproducción continua y no se invoca Python por fotograma. Nombres inválidos lanzan `ValueError`; un `Drawable` que no es `Text` lanza `TypeError`.],
)[
```python
from gaanim import Scene, parallel

scene = Scene(frame=(16, 9), background="#0f172a")
title = scene.text("Tipografía cinética", role="title").move_to(0, 0)
wave = title.animator(by="grapheme", shape="smooth", order="forward", seed=0)
wave.set(offset=(0, -0.4), opacity=0.0, scale=0.6, rotation=0.2)  # estado "fuera"
scene.play([wave.animate.sweep().duration(1.2)])  # recorre el rango 0 → 1

bob = title.animator(by="word", shape="round").set(offset=(0, 0.3))
scene.play(parallel(bob.animate.sweep().duration(1.0)))  # una ola que pasa
```
]

`reveal`, `conceal` y `blur_in` son presets de este animador, y
`write(by=..., order=...)` usa la misma segmentación y los mismos órdenes.

=== Énfasis y anotación

```text
text.animate.indicate() -> Anim
text.animate.wiggle() -> Anim
```

These operate on the complete `Text`; the typed selection proxy adds `pulse`,
`wave`, `highlight`, `focus`, `cancel`, `reveal`, `brace`, `annotate`, and
`marker` for local subsets.

== Transiciones estructurales

#api-entry(
  name: "Text.animate.transform_to",
  kind: "method",
  signature: "text.animate.transform_to(target).duration(seconds) -> Anim",
  params: (
    (name: "target", type: "Text", default: none, desc: [Destination text in the same scene and a compatible Layout ownership context.]),
    (name: "duration", type: "float", default: "1.0", desc: [Positive finite seconds, shared with Layout reflow.]),
  ),
  returns: (type: "Anim", desc: [Deferred structural transition accepted by `scene.play()`.]),
  desc: [The pure proxy does not change either text until `scene.play` commits it. Ownership violations raise `LayoutOwnershipError`.],
)[
```python
# show-code: true
from gaanim import GOLD, Scene, part
scene = Scene(frame=(16, 9), background="#0f172a")
before = scene.text("$x + ", part("obsolete", "3"), " = 7$").move_to(0, 0)
after = scene.text("$x = ", part("result", "4", color=GOLD), "$").move_to(0, 0)
scene.play([before.animate.write().duration(0.8)])
scene.play([before["obsolete"].animate.cancel().duration(0.5)])
scene.play([before.animate.transform_to(after).duration(0.8)])
# output: text_transition.webp
scene.render()
```
]

For local transitions, `selection.animate.morph_to(target)` replaces one
selection and `selection.animate.copy_to(target)` preserves the source while
moving a copy to the target.

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

== TextAnchor y posicionamiento

#api-entry(
  name: "Text.move_to",
  kind: "method",
  signature: ".move_to(x, y, anchor: Anchor | TextAnchor = None) -> Text",
  params: (
    (name: "x / y", type: "float", default: none, desc: [Target point in scene units.]),
    (name: "anchor", type: "Anchor | TextAnchor | None", default: "None", desc: [Geometric bounds anchor or baseline-left/center/right text anchor; positional or keyword.]),
  ),
  returns: (type: "Text", desc: [The same specialized fluent handle.]),
  desc: [A single line defaults to `TextAnchor.BASELINE_CENTER`; a multiline block without an explicit anchor keeps visual-center placement. Explicit `TextAnchor` values use the first visual line. A single `Drawable` or `AnchorPoint` argument aligns the visual center to that reference without creating a reactive follow. Layout-owned text raises `LayoutOwnershipError`, and non-anchor values raise `TypeError`.],
)[
```python
# show-code: true
from gaanim import Anchor, Scene, TextAnchor
scene = Scene(frame=(16, 9))
scene.text("baseline left").move_to(-2.75, 0.75, anchor=TextAnchor.BASELINE_LEFT)
scene.text.equation("frac(x_1^2, y_2) = 1").move_to(0, 0)
scene.text("geometric corner").move_to(-2.75, -1.25, anchor=Anchor.TOP_LEFT)
```
]

== Capacidades heredadas de Drawable

`Text` remains a `Drawable`. It preserves its specialized subtype through the
common fluent methods:

```text
text.fill(color).stroke(color, width).opacity(alpha).z_index(layer)
text.move_to(x, y).move_to_3d(x, y, z).next_to(other, direction)
text.align_to(other, anchor).to_edge(direction).to_corner(anchor)
text.scale_to(factor).rotate_to(radians).with_pivot(x, y)
text.billboard().hud()
```

For single-line text, `text.move_to(x, y)` places the visual horizontal center on
`x` and the typographic baseline on `y`. `scene.text.equation(...)` returns the same
`Text` type and follows the same rule, so words and equations with different
ascenders, descenders, fractions, scripts, or authored sizes can share a
stable baseline:

```python
from gaanim import Scene, TextAnchor

scene = Scene(frame=(16, 9))
word = scene.text("Typography").move_to(0, 1)
equation = scene.text.equation("frac(x_1^2, y_2) = 1").move_to(
    0, -0.5, TextAnchor.BASELINE_CENTER
)
left = scene.text("left aligned").move_to(
    -4, -2, TextAnchor.BASELINE_LEFT
)
```

`TextAnchor.BASELINE_LEFT`, `BASELINE_CENTER`, and `BASELINE_RIGHT` select the
horizontal point on the baseline. On multiline text, an explicit `TextAnchor`
uses the first visual line; omitting the anchor keeps the block's visual center
behavior. Geometric anchors remain bounds-based: two texts placed with
`Anchor.TOP_LEFT` share their top-left corner, but do not necessarily share a
baseline. `Drawable.at` accepts only geometric `Anchor` values; passing a
`TextAnchor` to a non-text drawable raises `TypeError`.

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
- `KeyError`: a string that is neither a semantic part nor text in the
  `Text`, no shared automatic expansion anchor.
- `IndexError`: query index out of range or empty slice.
- `ValueError`: selection slices with a step other than 1.
- `LayoutOwnershipError`: manual positioning of managed text or a transition
  across scenes/incompatible owners.

== Véase también

- #link("/api/layout/", "Layout v2 — outer boxes, ownership, constraints, and reflow")
- #link("/api/animations/", "Animations — generic Drawable animations and timing")
- #link("/api/themes/", "Themes — role colors and typography context")
- #link("/api/mobjects/", "Mobjects — raw Typst documents and other drawables")
