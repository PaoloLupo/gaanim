#import "../../components/section.typ": docs-chapter

#show: docs-chapter.with(
  title: "Themes",
  description: "Built-in color themes for gaanim scenes",
  route: "/api/themes/",
  code-langs: (),
  updated: datetime.today().display(),
)

= Themes

Gaanim includes built-in themes that define background, primary, secondary, accent, and muted colors. Mobjects automatically adopt the theme's primary color by default.

== Using Themes

```python
from gaanim import Scene, Theme

# Set theme at scene creation
scene = Scene(1280, 720, theme=Theme.DRACULA)

# Switch theme mid-scene
scene.set_theme(Theme.GRUVBOX)

# Access theme colors
bg = scene.theme.background
accent = scene.theme.accent
```

== Built-in Themes

=== Theme.DARK (Catppuccin Mocha)

The default theme. Dark background with soft pastel colors.

```python
scene = Scene(1280, 720, theme=Theme.DARK)
```

- Background: `#1e1e2e` (dark blue-gray)
- Primary: `#cdd6f4` (soft white)
- Accent: `#89b4fa` (soft blue)

=== Theme.LIGHT (Catppuccin Latte)

Light theme for bright environments or printed materials.

```python
scene = Scene(1280, 720, theme=Theme.LIGHT)
```

- Background: `#eff1f5` (light gray)
- Primary: `#4c4f69` (dark gray)
- Accent: `#1e66f5` (blue)

=== Theme.DRACULA

Classic Dracula color scheme with vibrant purple and pink accents.

```python
scene = Scene(1280, 720, theme=Theme.DRACULA)
```

- Background: `#282a36` (dark purple-gray)
- Primary: `#f8f8f2` (off-white)
- Accent: `#bd93f9` (purple)

=== Theme.GRUVBOX

Warm retro color scheme with earthy tones.

```python
scene = Scene(1280, 720, theme=Theme.GRUVBOX)
```

- Background: `#282828` (dark brown)
- Primary: `#ebdbb2` (warm white)
- Accent: `#fe8019` (orange)

== Color Constants

Gaanim provides these color constants for use with `.fill()` and `.stroke()`:

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

== Custom Colors

Use `Color` for custom RGBA values (0–255 per channel):

```python
from gaanim import Color

custom = Color(128, 51, 204, 255)  # RGBA (0-255)
circle = scene.circle(80).fill(custom)
```
