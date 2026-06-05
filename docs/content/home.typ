#import "../components/section.typ": docs-chapter

#show: docs-chapter.with(
  title: "Gaanim",
  description: "GPU-accelerated 2D vector animation engine for Python",
  route: "/",
  code-langs: (),
)

= Gaanim

GPU-accelerated 2D vector animation engine, Manim-style. Built in Rust with Bevy ECS + Vello renderer, with Python bindings via PyO3/Maturin.

== Features

- *GPU-accelerated rendering* via Vello (compute-based 2D renderer)
- *Fluent Python API* with method chaining
- *Rich primitives*: circles, rectangles, lines, arrows, polygons, stars, equations, text
- *30+ animation types*: write, create, fade, shift, scale, rotate, indicate, circumscribe, and more
- *Rate functions*: linear, smooth, spring, bounce, elastic, cubic bezier
- *Glyph-level selection* for text and equations
- *Themes*: Dark (Catppuccin Mocha), Light (Catppuccin Latte), Dracula, Gruvbox
- *Export*: MP4, WebM, WebP, GIF, PNG sequences
- *Multi-scene engine* with transitions for presentations

== Quick Start

```bash
# Clone and bootstrap
git clone https://github.com/user/gaanim
cd gaanim
just bootstrap    # Create .venv, install maturin
just build        # Build Python extension

# Run an example
just run math_animation
```

#link("/getting-started/", "Getting Started →")

== Links

- #link("/api/scene/", "API Reference")
- #link("/examples/basic/", "Examples")
