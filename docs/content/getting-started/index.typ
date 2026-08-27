#import "../../components/section.typ": docs-chapter

#show: docs-chapter.with(
  title: "Instalación y primeros pasos",
  description: "Instala Gaanim y crea tu primera animación",
  route: "/getting-started/",
  updated: datetime.today().display(),
)

= Instalación — resumen

Para la guía completa (usuario final con zip + `uv`, y desarrollo local desde fuente) ver #link("/getting-started/installation/")[Instalación].

== Requisitos previos

- *Rust* (edition 2024) — via #link("https://rustup.rs", "rustup")
- *Python >=3.12* — 3.12 mínimo, 3.14 también soportado
- *uv recomendado* — #link("https://docs.astral.sh/uv/")[uv] para venvs
- *GPU con Vulkan* — para Vello/Bevy

== Preparación rápida para desarrollo

```bash
git clone https://github.com/user/gaanim
cd gaanim
just bootstrap        # .venv + build/hatchling
just build            # debug: gaanim_launcher + gaanim-core
just doctor           # verifica build y gaanim --help via launcher
```

El zip de usuario final (`gaanim-v0.1.0-windows-x64.zip`) ya contiene `gaanim.exe` (launcher) + `gaanim-core.exe` y no requiere compilar. Ver detalle en #link("/getting-started/installation/")[Instalación / Usuario final].

== Verifica la instalación

```bash
just doctor           # compila y prueba launcher
gaanim --help         # si tienes el zip en PATH
```

= Tu primera animación

Crea un archivo llamado `my_animation.py`:

```python
from gaanim import BLACK, BLUE, GOLD, Scene

scene = Scene(1280, 720, background=BLACK)

circle = scene.geometry.circle(80).fill(BLUE).stroke(GOLD, 4)
text = scene.text("Hello World", role="title")

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

Ejecútalo:

```bash
gaanim my_animation.py
```

Se abrirá la ventana de previsualización de Gaanim. Pulsa `Escape` para cerrarla.

= Exportación

Para exportar en lugar de abrir la previsualización:

```python
<<< # MP4
<<< scene.render()  # luego: gaanim export . --output output.mp4
<<<
<<< # WebM
<<< scene.render()  # luego: gaanim export . --output overlay.webm
<<<
<<< # Animated WebP
<<< scene.render()  # luego: gaanim export . --output preview.webp
<<<
<<< # Any supported video extension
<<< scene.render()  # luego: gaanim export . --output tiktok.mp4
```

= Siguientes pasos

- #link("/manual/guia-rapida/", "Guía rápida") — crea un proyecto de movimiento circular
- #link("/guides/slides/", "Presentaciones") — flujo completo para presentaciones en vivo

- #link("/api/scene/", "API de Scene") — consulta el contrato técnico de la escena
- #link("/api/mobjects/", "Objetos") — explora las figuras y objetos disponibles
- #link("/api/animations/", "Animaciones") — consulta animaciones y funciones de tiempo
- #link("/examples/basic/", "Ejemplos") — escenas completas y ejecutables
