#import "../../components/section.typ": docs-chapter

#show: docs-chapter.with(
  title: "Temas y colores",
  description: "Reglas visuales centralizadas, colores CSS y paletas reutilizables",
  route: "/api/themes/",
  code-langs: (),
  updated: datetime.today().display(),
)

= Temas y colores

Use a scene background and the exported color constants to define the visual
language of an animation. Built-in themes are available through `scene.canvas`.

For technical or LaTeX-style material, prefer a near-black background, white or
soft-gray text, muted blue for structural emphasis, and reserve saturated
semantic colors (green, red, gold) for positive, negative, or exceptional
meaning. The built-in title cards, bullets, and bar charts follow this quieter
default.

```python
from gaanim import BLACK, BLUE, GOLD, WHITE, Scene

scene = Scene(1280, 720, background=BLACK)
circle = scene.circle(80).fill(BLUE).stroke(GOLD, 4)
label = scene.text("Colorful scene", role="title").fill(WHITE)
```

You can update the background inside the authored scene bounds through
`scene.canvas`:

```python
from gaanim import Color

scene.canvas.background = Color(40, 42, 54)
```

== Fondos con gradientes y WGSL

`Scene.background` and `scene.canvas.background` accept the same `Brush`
gradients used by drawables. Gradient coordinates are scene coordinates, so a
full-width linear gradient on a 1280×720 scene runs from `x=-640` to `x=640`:

```python
from gaanim import Brush, Scene

sky = Brush.linear(
    ["#071022", "#164E8A", "#7DD3FC"],
    start=(-640, 0),
    end=(640, 0),
)
scene = Scene(1280, 720, background=sky)
```

For procedural or animated art, `Background.shader(source, fallback=...)`
accepts a WGSL function with this signature:

```wgsl
fn gaanim_background(
    uv: vec2<f32>,
    resolution: vec2<f32>,
    time: f32,
) -> vec4<f32> {
    let pulse = 0.5 + 0.5 * sin(time * 2.0);
    return vec4<f32>(uv.x, uv.y, pulse, 1.0);
}
```

`uv=(0, 0)` is the top-left and `uv=(1, 1)` the bottom-right. `resolution` is
the effective scene size in pixels. `time` is the absolute timeline position
in seconds; it follows playback and exact seeks, making snapshots and exports
deterministic. The shader covers the same authored scene rectangle shown by the
editor's bounds overlay; letterboxed space outside that rectangle uses
`fallback`. The function is validated when the `Background` is created and
cached as a Vello texture for the active resolution and time. Resizing the
editor re-rasterizes it. Legacy two-argument functions without `time` remain
accepted as static backgrounds. `fallback` defaults to black and is also used
for native 3D clearing, automatic text contrast, or a GPU rasterization failure.

```python
from gaanim import Background, Scene

shader = Background.shader("""
fn gaanim_background(
    uv: vec2<f32>,
    resolution: vec2<f32>,
    time: f32,
) -> vec4<f32> {
    let center = vec2<f32>(0.35 + 0.1 * sin(time), 0.45);
    let glow = exp(-8.0 * distance(uv, center));
    return vec4<f32>(0.02, 0.08 + 0.4 * glow, 0.18 + 0.6 * glow, 1.0);
}
""", fallback="#071022")
scene = Scene(1280, 720, background=shader)
```

== Temas incluidos

`technical` is the sober dark default for mathematical explanations and
technical documentation. `presentation` is optimized for projection: deep navy
background, warm gold titles, bright body text and cooler secondary labels.
`paper` uses a literal white canvas with restrained dark ink. An explicit
`.fill(...)` always takes precedence.

Component defaults also follow the selected theme. Editorial cards, badges,
banners, callouts, bar charts, tables and code panels inherit compatible
foreground, accent, panel and rule colors. Bar charts include value labels and
reserve enough vertical space to keep them inside their bounds.

```python
from gaanim import Scene

scene = Scene(1280, 720)
scene.canvas.set_theme("presentation")

title = scene.text("Fourier transform", role="title")
subtitle = scene.text("Frequency-domain representation", role="subtitle")
equation = scene.text("$F(k) = integral f(x) e^(-i k x) dif x$")
```

The aliases `scientific`, `deck`, and `light` map to `technical`,
`presentation`, and `paper`. The former `thesis` alias is no longer accepted.

== Esquemas de color conocidos

The same short API includes established editor and terminal palettes:

```python
scene.canvas.set_theme("dracula")
scene.canvas.set_theme("nord")
scene.canvas.set_theme("solarized-dark")
scene.canvas.set_theme("solarized-light")
scene.canvas.set_theme("gruvbox-dark")
scene.canvas.set_theme("tokyo-night")
scene.canvas.set_theme("catppuccin-mocha")
scene.canvas.set_theme("catppuccin-latte")
```

Use `Theme.schemes()` when a tool or GUI needs to enumerate every built-in
scheme.

== Temas personalizados y derivados

`Theme` is the single configuration object for semantic colors, structured
typography, selector rules, data palettes, layout tokens, and font files. It
can be installed directly with `Scene(theme=...)`; pass a scheme name to
derive it and override only what changes:

```python
from gaanim import AxesStyle, Scene, StrokeStyle, Style, TextStyle, Theme, colors

theme = Theme(
    "nord",
    name="my-slides",
    colors={
        "title": "#A3D9FF",
        "accent": "#FFB86C",
        "chart": "#88C0D0",
    },
    fonts={
        "text": "Inter",
        "code": "JetBrains Mono",
    },
    sizes={"title": 72, "body": 34},
    text={
        "body": TextStyle(size=32, letter_spacing=0.1),
        "label": TextStyle(size=24, weight=600),
    },
    styles={
        "shape": Style(fill="accent"),
        "line": Style(stroke=StrokeStyle("foreground", 3, cap="round")),
        ".warning": Style(fill=colors.tailwind.rose[600]),
        "axes": AxesStyle(
            grid=StrokeStyle("rule", 1),
            labels=TextStyle(size=24),
        ),
    },
    series=[colors.tailwind.blue[600], colors.tailwind.amber[500]],
    layout={"page_padding": 56, "column_gap": 48},
    font_files={
        "Inter": "assets/Inter-Regular.ttf",
        "JetBrains Mono": "assets/JetBrainsMono-Regular.ttf",
    },
)

scene = Scene(1920, 1080, theme=theme)
```

The `text` dictionary reuses the same `TextStyle` accepted by structured
`Text` and `part(...)`. It is an overlay: omitted properties continue to come
from the semantic role. `TextPart` styles and explicit drawable methods retain
higher priority.

== Cascada de selectores

Theme rules may target a family (`shape`, `line`, `text`, `axes`, `plot`), an
exact factory name (`circle`, `rounded_rect`, `arrow`), a semantic part such as
`axes/grid` or `axes/labels`, or a user class such as `.warning`.

```python
from gaanim import Scene, Style, Theme

theme = Theme(
    "paper",
    colors={"brand": "#2563eb", "danger": "oklch(58% .24 25)"},
    styles={
        "shape": Style(fill="brand"),
        ".danger": Style(fill="danger"),
    },
)
scene = Scene(theme=theme)
ordinary = scene.circle(60)
warning = scene.square(100).style_class("danger")
explicit = scene.circle(40).fill("gold")
```

Precedence is base theme, family, exact type or semantic part, ordered user
classes, constructor values, then fluent overrides. Rules are materialized
when the scene compiles, so changing the active theme also updates already
authored compatible objects. Imported asset paints remain source-controlled
unless explicitly styled.

`StrokeStyle` carries paint, width, cap, join, miter limit, dash pattern, and
dash offset. It can be reused in a theme or applied directly with
`drawable.stroke_style(style)`. Invalid metrics, selectors, and unresolved
tokens raise `ValueError`.

Font files are read when `Theme` is created and embedded in the canvas runtime,
so exports do not depend on the font being installed on the presentation
computer. TTF and OTF files are supported by the underlying font registry.

To start without inheriting a named scheme, omit the first argument:

```python
brand = Theme(
    name="brand",
    colors={
        "background": "#10131A",
        "foreground": "#F8FAFC",
        "muted": "#94A3B8",
        "title": "#FDE68A",
        "accent": "#38BDF8",
        "chart": "#22C55E",
        "panel": "#18202E",
        "header": "#202B3D",
        "rule": "#475569",
    },
    fonts={"text": "Aptos", "code": "Consolas"},
)
```

Another `Theme` can be the first argument, which makes modification and reuse
explicit:

```python
print_theme = Theme(
    brand,
    name="brand-print",
    colors={"background": "white", "foreground": "#172033"},
)
```

Built-in color roles are `background`, `foreground`, `muted`, `title`, `accent`,
`chart`, `panel`, `header`, `rule`, `success`, `warning`, and `danger`. The last
three drive editorial variants and can be overridden through `Theme(colors=...)`.
Font roles are `text`, `all`, `title`,
`subtitle`, `heading`, `body`, `caption`, `label`, `math`, and `code`. The
`colors` dictionary may also define arbitrary non-empty tokens for selector
rules.

== CSS Color 4 y Tailwind

Every `ColorLike` position accepts CSS Color 4 syntax. `Color(...)` also accepts
a literal directly, and the explicit constructors are useful when values are
computed:

```python
from gaanim import Color, colors

navy = Color("#0f172a")
accent = Color("oklch(62.3% 0.214 259.815)")
translucent = Color("rgb(37 99 235 / 65%)")
computed = Color.from_hsl(215, 0.9, 0.55, 0.8)
perceptual = Color.from_oklch(0.68, 0.17, 240)
tailwind_blue = colors.tailwind.blue[500]
```

`colors.tailwind` contains all 26 color families and the 50–950 scales from
Tailwind CSS v4.3.3, including `mauve`, `olive`, `mist`, and `taupe`. The
embedded version is available as `colors.tailwind.version`.

Layout templates consume named spacing values through
`scene.canvas.layout_token(name)`. The default scale includes `space_xs`,
`space_sm`, `space_md`, `space_lg`, `page_padding`, `page_padding_wide`,
`page_padding_x`, `column_gap`, `vertical_padding`, `vertical_padding_x`, and
`lower_third_offset`. Custom themes may override these or add project-specific
tokens through the `layout={...}` argument.

== Tokens de tema y legibilidad

Manual vector objects can consume the same semantic tokens as components:

```python
scene.rounded_rect(420, 180, 24) \
    .fill(scene.canvas.color("panel")) \
    .stroke(scene.canvas.color("accent"), 3)

divider = scene.line(-400, 0, 400, 0) \
    .stroke(theme.color("rule"), 2)

page = scene.column(
    [title, body],
    padding=scene.canvas.layout_token("page_padding"),
    gap=scene.canvas.layout_token("space_lg"),
)
```

`Theme.validate()` and `scene.canvas.validate_theme()` return actionable
warnings for insufficient foreground, title, muted, or panel contrast and for
invalid typography. They return an empty list when the core combinations are
ready:

```python
warnings = scene.canvas.validate_theme()
if warnings:
    raise ValueError("\n".join(warnings))
```

Validation is advisory rather than automatic rejection, so intentional
low-contrast animation states remain possible.

== Pinceles y gradientes

`Drawable.fill(...)` and `Drawable.stroke(...)` accept either an ordinary
`ColorLike` value or a reusable `Brush`. Gradient coordinates use the
drawable's local coordinate space, so the paint follows later transforms.

```python
from gaanim import Brush

gradient = Brush.linear(
    ["#7AA2F7", "#BB9AF7", "#F7768E"],
    start=(-240, 0),
    end=(240, 0),
)

card = scene.rounded_rect(480, 220, 28).fill(gradient)
```

Radial and angular gradients use the same color-list convention. Colors are
distributed uniformly and two or more stops are required:

```python
orb = scene.circle(120).fill(
    Brush.radial(
        ["white", scene.canvas.color("accent"), "#0000"],
        center=(-30, 35),
        radius=150,
    )
)

ring = scene.circle(110).no_fill().stroke(
    Brush.sweep(
        ["#7DCFFF", "#9ECE6A", "#E0AF68", "#F7768E", "#7DCFFF"],
        center=(0, 0),
    ),
    20,
)
```

`extend="pad"` is the default. Use `"repeat"` or `"reflect"` for repeating
ramps. Sweep angles are expressed in degrees; linear points and radial radii
use scene units.

== Efectos visuales

Effects use the same fluent `Drawable` surface. Defaults keep common calls
short, while radius, intensity, offset, and blur remain configurable:

```python
title.glow("#38BDF8")
background_blob.blur(12)
card.shadow("#00000080", x=10, y=-10, blur=8)
```

`glow`, `blur`, and `shadow` are compiled into retained vector fragments, so
unchanged effects are cached. They work on vector fills and strokes, including
gradient brushes. `no_effects()` removes all three without changing the
drawable's fill or stroke.

== Constantes de color

#table(
  columns: (1fr, 1fr, 1fr),
  [*Name*], [*Hex*], [*Usage*],
  [`BLUE`], [`#3b82f6`], [Primary blue],
  [`GOLD`], [`#eab308`], [Gold/yellow],
  [`RED`], [`#ef4444`], [Red],
  [`GREEN`], [`#22c55e`], [Green],
  [`WHITE`], [`#ffffff`], [White],
  [`BLACK`], [`#000000`], [Black],
  [`YELLOW`], [`#facc15`], [Yellow],
  [`ORANGE`], [`#f97316`], [Orange],
  [`PURPLE`], [`#a855f7`], [Purple],
  [`PINK`], [`#ec4899`], [Pink],
  [`GRAY`], [`#6b7280`], [Gray],
  [`CYAN`], [`#06b6d4`], [Cyan],
  [`CORAL`], [`#ff7f50`], [Coral],
  [`NAVY`], [`#1e3a5f`], [Navy],
  [`TEAL`], [`#14b8a6`], [Teal],
)

== Colores personalizados

`Color` receives RGBA channels from 0 through 255:

```python
from gaanim import Color

custom = Color(128, 51, 204, 255)
circle = scene.circle(80).fill(custom)
```
