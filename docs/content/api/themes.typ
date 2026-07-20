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
language of an animation. Theme objects are not part of the public Python API.

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
