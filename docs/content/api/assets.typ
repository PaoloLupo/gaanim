#import "../../components/section.typ": docs-chapter

#show: docs-chapter.with(
  title: "Assets",
  description: "Portable image and SVG paths, manifests, and preloading",
  route: "/api/assets/",
  code-langs: (),
  updated: datetime.today().display(),
)

= Assets

Set one asset directory per scene so relative image and SVG paths remain
portable when a project is moved or rendered from another working directory.

```python
from gaanim import Scene

scene = Scene()
scene.assets_dir("assets")

logo = scene.svg("logo.svg")
cover = scene.image("cover.png")
```

Absolute paths continue to work and take precedence over `assets_dir`.

== Project manifest

Create a complete starter from the CLI:

```text
gaanim init video my-video
gaanim init presentation my-deck
gaanim init thesis my-defense
```

Every project contains `main.py`, `gaanim.toml`, `assets/`, `exports/`, a README,
and a project `.gitignore`. The generated manifest is:

```toml
name = "my-deck"
kind = "presentation"
entry = "main.py"
assets_dir = "assets"
output_dir = "exports"
```

Then load it before creating drawables:

```python
scene = Scene()
scene.load_project()  # reads ./gaanim.toml
```

The CLI accepts either the entry script or its project directory (`gaanim my-deck`,
`gaanim check my-deck`). The asset directory is resolved relative to the manifest,
not the process working directory. `load_project("path/to/gaanim.toml")` accepts
an explicit manifest path.

== Preloading

Use `preload` to validate raster and SVG files before playback. Raster images
are decoded into the same cache used by `scene.image`.

```python
scene.preload(["logo.svg", "cover.png", "diagram.webp"])
```

Failures identify the asset that could not be resolved or decoded. The scene currently
consumes `assets_dir`; `name`, `kind`, `entry`, and `output_dir` describe the project
workflow and leave room for future export presets.

== Refreshing changed files

When a raster asset changes on disk without restarting the process, clear the
decoded image cache before rebuilding the affected drawables:

```python
scene.reload_assets()
cover = scene.image("cover.png")
```

SVG files are parsed again whenever `scene.svg(...)` creates a drawable.

== Advanced SVG

`scene.svg(...)` keeps the document as vector geometry. The importer resolves:

- nested groups, CSS, transforms, `viewBox` and `<use>`;
- solid, linear and radial fills or strokes, including gradient spread modes;
- `clipPath` geometry applied without rasterizing the document;
- SVG text converted to font outlines using installed system fonts;
- common `feGaussianBlur` and `feDropShadow` filters through Gaanim's retained
  vector effects.

Named source groups, paths and text remain addressable:

```python
diagram = scene.svg("architecture.svg")
diagram.part("database").indicate(0.6)
diagram.part("caption").fade_to(0.5)
```

The import is intentionally vector-first. Pattern paints, luminance/alpha
masks, embedded raster images and arbitrary SVG filter graphs are not yet
preserved. For portable text outlines, make sure the requested font is
installed on every render machine or convert text to paths in the source SVG.
