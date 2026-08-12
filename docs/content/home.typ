#import "../components/section.typ": docs-chapter

#show: docs-chapter.with(
  title: "Gaanim",
  description: "Motor de animación vectorial 2D acelerado por GPU para Python",
  route: "/",
  code-langs: (),
)

#html.div(class: "home-hero", [
  #html.div(style: "font-family: var(--font-code); font-size: 0.68rem; font-weight: 700; letter-spacing: 0.14em; text-transform: uppercase; color: var(--accent-gold); margin-bottom: 10px;", [● REC &nbsp;·&nbsp; GPU · Vello · Bevy · Python])
  #html.h1(style: "margin: 0;", [Da vida a tus ideas.])
  #html.div(class: "home-hero-desc", [Manim-style, GPU-accelerated. Escribe Python, exporta MP4/WebM/WebP/GIF. Timeline fluida, 30+ animaciones, tipografía y math con Typst. Hecho para quienes viven en el viewport.])
  #html.div(class: "home-hero-cta", [
    #html.a(href: "guia/antes-de-empezar/", class: "primary", [Leer la guía →])
    #html.a(href: "getting-started/", class: "secondary", [Instalar])
    #html.a(href: "api/", class: "secondary", [Referencia de la API])
    #html.a(href: "examples/basic/", class: "secondary", [Ver ejemplos])
  ])
])

== Por qué Gaanim

*GPU sin dolor.* Vello (compute) + Bevy ECS + PyO3. Python fluido por fuera, Rust rápido por dentro.

*API que encadena.* `scene.circle(80).fill(BLUE).at(0, 100).play([circle.create().spring()])` — sin boilerplate, sin `.animate()`.

*Tipografía de verdad.* Texto y ecuaciones como vectores via Typst. Selección por glifo, `write` animado, `color_by` y tags semánticos.

*Export listo para publicar.* MP4/WebM/WebP/GIF/PNG, presets `youtube`/`tiktok`/`instagram`, segmentos semánticos con `segment()` + `stop()` y export por segmento.

== En 30 segundos

```python
from gaanim import BLUE, GOLD, Scene

scene = Scene(1280, 720)
circle = scene.circle(80).fill(BLUE).at(-160, 0)
rect = scene.rect(180, 100).fill(GOLD).at(160, 0)

scene.play([
    circle.create().duration(1.0).spring(),
    rect.grow_from_center().duration(1.0),
])
scene.play([circle.move(200, 0).duration(0.8).smooth()])
scene.export("demo.mp4", fps=30)
```

#html.div(style: "display: grid; grid-template-columns: repeat(auto-fit, minmax(220px, 1fr)); gap: 14px; margin: 28px 0;", [
  #html.div(style: "background: var(--bg-card); border: 1px solid var(--code-border); border-radius: 12px; padding: 18px;", [
    #html.div(style: "font-family: var(--font-code); font-size: 0.68rem; font-weight: 700; letter-spacing: 0.08em; color: var(--accent-gold); margin-bottom: 8px;", [30+ ANIMACIONES])
    #html.div(style: "font-weight: 700; margin-bottom: 6px;", [Todo lo que necesitas])
    #html.div(style: "font-size: 0.88rem; color: var(--text-muted);", [write, create, fade, spin, wiggle, transform… con easing spring/smooth/linear.])
  ])
  #html.div(style: "background: var(--bg-card); border: 1px solid var(--code-border); border-radius: 12px; padding: 18px;", [
    #html.div(style: "font-family: var(--font-code); font-size: 0.68rem; font-weight: 700; letter-spacing: 0.08em; color: var(--accent-violet); margin-bottom: 8px;", [VECTORES REALES])
    #html.div(style: "font-weight: 700; margin-bottom: 6px;", [Typst dentro])
    #html.div(style: "font-size: 0.88rem; color: var(--text-muted);", [Ecuaciones, párrafos y SVG importados como paths animables.])
  ])
  #html.div(style: "background: var(--bg-card); border: 1px solid var(--code-border); border-radius: 12px; padding: 18px;", [
    #html.div(style: "font-family: var(--font-code); font-size: 0.68rem; font-weight: 700; letter-spacing: 0.08em; color: var(--accent-cyan); margin-bottom: 8px;", [TIMELINE])
    #html.div(style: "font-weight: 700; margin-bottom: 6px;", [Control total])
    #html.div(style: "font-size: 0.88rem; color: var(--text-muted);", [play, wait, segment, stop, link, camera pan/zoom/follow/shake.])
  ])
])

== Siguiente

#link("/guia/antes-de-empezar/", "Empezar la guía →") · #link("/api/", "Referencia de la API →") · #link("/guides/projects/", "Proyectos →") · #link("/guides/slides/", "Presentaciones →")
