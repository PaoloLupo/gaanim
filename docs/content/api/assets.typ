#import "../../components/section.typ": docs-chapter

#show: docs-chapter.with(
  title: "Recursos",
  description: "Rutas portables de imágenes, SVG y glTF, manifiestos y precarga",
  route: "/api/assets/",
  code-langs: (),
  updated: datetime.today().display(),
)

= Recursos

Set one asset directory per scene so relative image, SVG, and glTF paths remain
portable when a project is moved or rendered from another working directory.

```python
from gaanim import Scene

scene = Scene()
scene.assets_dir("assets")

logo = scene.svg("logo.svg")
cover = scene.image("cover.png")
robot = scene.gltf("robot.glb")
```

Absolute paths continue to work and take precedence over `assets_dir`.

== Manifiesto del proyecto

Create a complete starter from the CLI:

```text
gaanim init video my-video
gaanim init slides my-deck
```

Every project contains `main.py`, `gaanim.toml`, `assets/`, `exports/`, a README,
and a project `.gitignore`. The generated manifest is:

```toml
name = "my-deck"
kind = "slides"
entry = "main.py"
assets_dir = "assets"
output_dir = "exports"
```

Then load it before creating drawables:

```python
scene = Scene()
scene.load_project()  # reads gaanim.toml beside this Python script
```

The CLI accepts either the entry script or its project directory (`gaanim my-deck`,
`gaanim check my-deck`). The asset directory is resolved relative to the manifest,
not the process working directory. With no argument, `load_project()` finds the
manifest next to the calling script even when Gaanim was started elsewhere.
`load_project("path/to/gaanim.toml")` accepts an explicit manifest path.

== Precarga

Use `preload` to validate raster, SVG, and glTF files before playback. Raster images
are decoded into the same cache used by `scene.image`.

```python
scene.preload(["logo.svg", "cover.png", "diagram.webp"])
```

Failures identify the asset that could not be resolved or decoded. The scene currently
consumes `assets_dir`; `name`, `kind`, `entry`, and `output_dir` describe the project
workflow and leave room for future export presets.

== Actualización de archivos modificados

When a raster asset changes on disk without restarting the process, clear the
decoded image cache before rebuilding the affected drawables:

```python
scene.reload_assets()
cover = scene.image("cover.png")
```

SVG files are parsed again whenever `scene.svg(...)` creates a drawable.

glTF metadata is cached by canonical path and modification time. `reload_assets()`
also clears this cache; the editor then removes the old native scene instance and
all of its descendants before rebuilding it.

== Modelos 3D glTF

`Scene.gltf(path, *, scene=None) -> Drawable` imports local glTF 2.0 `.gltf`
and `.glb` files. `scene` accepts a scene name, a zero-based index, or `None`
for the file's default scene.

```python
model = scene.gltf("robot.glb", scene="Presentation")
arm = model.part("Robot/Rig/Arm")

print(model.parts())       # tuple of stable selectors
print(model.animations())  # Blender Action names
```

A short node name is available only when it is unique. A hierarchical path
disambiguates repeated names; duplicate full paths receive the stable suffix
`#<node-index>`. Lookup errors list the candidate selectors.

Gaanim preserves the exported units, orientation, node hierarchy, PBR
metallic-roughness materials, normals, UVs, textures, skins, bones, and morph
targets. One glTF unit is one Gaanim world unit; models are not centered or
scaled automatically. Imported cameras and lights are removed in favor of the
Gaanim camera and neutral default light. Unsupported glTF extensions or missing
external buffers/textures fail with the source path in the error.

The visual payload is loaded through Bevy's native glTF loader. Gaanim creates
one stable wrapper for each node: manual wrapper transforms compose above the
Blender-authored node transform and skeletal/morph animation instead of
overwriting it. Materials are cloned per instance so opacity animation cannot
mutate another import of the same file.

== SVG avanzado

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
