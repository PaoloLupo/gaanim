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
technical documentation. It uses New Computer Modern with a restrained
white/gray hierarchy. `paper` uses a literal white canvas with a restrained
black ink fill for unfilled vector text, so it remains readable. An explicit
`.fill(...)` always takes precedence.

```python
from gaanim import Scene

scene = Scene(1280, 720)
scene.canvas.set_theme("technical")

title = scene.title("Fourier transform")
subtitle = scene.subtitle("Frequency-domain representation")
equation = scene.equation("F(k) = integral f(x) e^(-i k x) dif x")
```

The aliases `scientific` and `light` map to `technical` and `paper`.

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
