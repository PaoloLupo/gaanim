#import "../../components/section.typ": docs-chapter

#show: docs-chapter.with(
  title: "Getting Started",
  description: "Install gaanim and create your first animation",
  route: "/getting-started/",
  updated: datetime.today().display(),
)

= Installation

== Prerequisites

- *Rust* (edition 2024) — install via #link("https://rustup.rs", "rustup")
- *Python* 3.10+ — with pip
- *Vulkan-compatible GPU* — for rendering (most modern GPUs work)

== Setup

Clone the repository and bootstrap the environment:

```bash
git clone https://github.com/user/gaanim
cd gaanim
```

Create a virtual environment and install maturin (the Rust→Python build tool):

```bash
just bootstrap
```

This creates `.venv/` and installs `maturin` into it.

Build the Python extension:

```bash
just build           # Debug mode (faster compile, slower runtime)
just build-release   # Release mode (slower compile, faster runtime)
```

== Verify Installation

```bash
just doctor
```

This compiles the workspace and verifies the Python extension is importable.

= Your First Animation

Create a file `my_animation.py`:

```python
# output: first_animation.webp
# show-code: true
# caption: First animation — circle and title
from gaanim import BLUE, GOLD, Scene

scene = Scene(1280, 720, title="My First Animation")

circle = scene.circle(80).fill(BLUE).stroke(GOLD, 4)
text = scene.title("Hello World")

scene.play(
    circle.animate().grow_from_center().duration(2.0).spring(),
    text.animate().write(duration=2.0).smooth(),
)

scene.wait(1.0)
scene.play(
    circle.animate().shift(200, 0).duration(1.5).smooth(),
    text.animate().fade_out().duration(0.5),
)

scene.export("first_animation.webp", fps=30, quality="draft")
```

Run it:

```bash
python my_animation.py
```

This opens a Vulkan GPU preview window. Press `Escape` to close.

= Exporting

To export instead of previewing:

```python
<<< # MP4 (YouTube standard)
<<< scene.export("output.mp4", fps=60, aspect_ratio="youtube", quality="standard")
<<<
<<< # Transparent WebM
<<< scene.export("overlay.webm", fps=30, transparent=True)
<<<
<<< # Animated WebP
<<< scene.export("preview.webp", fps=30, quality="draft")
<<<
<<< # TikTok vertical
<<< scene.export("tiktok.mp4", fps=30, aspect_ratio="tiktok")
```

= Next Steps

- #link("/api/scene/", "Scene API") — learn the core Scene class
- #link("/api/mobjects/", "Mobjects") — all available shapes and objects
- #link("/api/animations/", "Animations") — animation types and rate functions
- #link("/examples/basic/", "Examples") — complete working examples
