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

For a minimal reproducible project, put `gaanim.toml` next to the script:

```toml
assets_dir = "assets"
```

Then load it before creating drawables:

```python
scene = Scene()
scene.load_project()  # reads ./gaanim.toml
```

The asset directory is resolved relative to the manifest, not the process
working directory. `load_project("path/to/gaanim.toml")` accepts an explicit
manifest path.

== Preloading

Use `preload` to validate raster and SVG files before playback. Raster images
are decoded into the same cache used by `scene.image`.

```python
scene.preload(["logo.svg", "cover.png", "diagram.webp"])
```

Failures identify the asset that could not be resolved or decoded. The current
manifest intentionally contains only `assets_dir`; fonts and export presets are
planned extensions.

== Refreshing changed files

When a raster asset changes on disk without restarting the process, clear the
decoded image cache before rebuilding the affected drawables:

```python
scene.reload_assets()
cover = scene.image("cover.png")
```

SVG files are parsed again whenever `scene.svg(...)` creates a drawable.
