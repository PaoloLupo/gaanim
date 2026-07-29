#import "../../components/section.typ": docs-chapter

#show: docs-chapter.with(
  title: "Colors",
  description: "Color constants and viewport backgrounds",
  route: "/api/themes/",
  code-langs: (),
  updated: datetime.today().display(),
)

= Colors

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
label = scene.title("Colorful scene").fill(WHITE)
```

You can update the viewport background through `scene.canvas`:

```python
from gaanim import Color

scene.canvas.background = Color(40, 42, 54)
```

== Built-in themes

`technical` is the sober dark default for mathematical explanations and
technical documentation. `presentation` is optimized for projection: deep navy
background, warm gold titles, bright body text and cooler secondary labels.
`paper` uses a literal white canvas with restrained dark ink. An explicit
`.fill(...)` always takes precedence.

Component defaults also follow the selected theme. Title cards, bullets,
captions, callouts, bar charts, tables and code panels inherit compatible
foreground, accent, panel and rule colors. Bar charts include value labels and
reserve enough vertical space to keep them inside their bounds.

```python
from gaanim import Scene

scene = Scene(1280, 720)
scene.canvas.set_theme("presentation")

title = scene.title("Fourier transform")
subtitle = scene.subtitle("Frequency-domain representation")
equation = scene.equation("F(k) = integral f(x) e^(-i k x) dif x")
```

The aliases `scientific`, `thesis`, `deck`, and `light` map to `technical`,
`presentation`, `presentation`, and `paper`.

== Known color schemes

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

== Custom and derived themes

`Theme` is the single configuration object for semantic colors, typographic
roles, sizes, and font files. Pass a scheme name to derive it and override only
what changes:

```python
from gaanim import Scene, Theme

theme = Theme(
    "nord",
    name="my-thesis",
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
    font_files={
        "Inter": "assets/Inter-Regular.ttf",
        "JetBrains Mono": "assets/JetBrainsMono-Regular.ttf",
    },
)

scene = Scene(1920, 1080)
scene.canvas.set_theme(theme)
```

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

Color roles are `background`, `foreground`, `muted`, `title`, `accent`,
`chart`, `panel`, `header`, and `rule`. Font roles are `text`, `all`, `title`,
`subtitle`, `body`, `caption`, `math`, and `code`; size roles use the six
individual text roles.

== Theme tokens and readability

Manual vector objects can consume the same semantic tokens as components:

```python
scene.rounded_rect(420, 180, 24) \
    .fill(scene.canvas.color("panel")) \
    .stroke(scene.canvas.color("accent"), 3)

divider = scene.line(-400, 0, 400, 0) \
    .stroke(theme.color("rule"), 2)
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

== Brushes and gradients

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

== Color constants

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

== Custom colors

`Color` receives RGBA channels from 0 through 255:

```python
from gaanim import Color

custom = Color(128, 51, 204, 255)
circle = scene.circle(80).fill(custom)
```
