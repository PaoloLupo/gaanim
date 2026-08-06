#import "../../components/section.typ": docs-chapter

#show: docs-chapter.with(
  title: "Getting Started",
  description: "Install gaanim and create your first animation",
  route: "/getting-started/",
  updated: datetime.today().display(),
)

= Installation — resumen

Para la guía completa (usuario final con zip + `uv`, y desarrollo local desde fuente) ver #link("/getting-started/installation/")[Instalación].

== Prerequisitos

- *Rust* (edition 2024) — via #link("https://rustup.rs", "rustup")
- *Python >=3.12* — 3.12 mínimo, 3.14 también soportado
- *uv recomendado* — #link("https://docs.astral.sh/uv/")[uv] para venvs
- *GPU con Vulkan* — para Vello/Bevy

== Setup rápido (dev)

```bash
git clone https://github.com/user/gaanim
cd gaanim
just bootstrap        # .venv + maturin
just build            # debug: gaanim_launcher + gaanim-core
just doctor           # verifica build y gaanim --help via launcher
```

El zip de usuario final (`gaanim-v0.1.0-windows-x64.zip`) ya contiene `gaanim.exe` (launcher) + `gaanim-core.exe` y no requiere compilar. Ver detalle en #link("/getting-started/installation/")[Instalación / Usuario final].

== Verify Installation

```bash
just doctor           # compila y prueba launcher
gaanim --help         # si tienes el zip en PATH
```

= Your First Animation

Create a file `my_animation.py`:

```python
from gaanim import BLACK, BLUE, GOLD, Scene

scene = Scene(1280, 720, background=BLACK)

circle = scene.circle(80).fill(BLUE).stroke(GOLD, 4)
text = scene.title("Hello World")

scene.play([
    circle.grow_from_center().duration(2.0).spring(),
    text.write().duration(2.0).smooth(),
])

scene.wait(1.0)
scene.play([
    circle.move(200, 0).duration(1.5).smooth(),
    text.fade_out().duration(0.5),
])

# Run with: gaanim my_animation.py
```

Run it:

```bash
gaanim my_animation.py
```

This opens the Gaanim preview window. Press `Escape` to close.

= Exporting

To export instead of previewing:

```python
<<< # MP4
<<< scene.export("output.mp4", fps=60)
<<<
<<< # WebM
<<< scene.export("overlay.webm", fps=30)
<<<
<<< # Animated WebP
<<< scene.export("preview.webp", fps=30)
<<<
<<< # Any supported video extension
<<< scene.export("tiktok.mp4", fps=30)
```

= Next Steps

- #link("/guides/thesis/", "Thesis presentations") - complete live-presentation workflow

- #link("/api/scene/", "Scene API") — learn the core Scene class
- #link("/api/mobjects/", "Mobjects") — all available shapes and objects
- #link("/api/animations/", "Animations") — animation types and rate functions
- #link("/examples/basic/", "Examples") — complete working examples
