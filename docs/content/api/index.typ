#import "../../components/section.typ": docs-chapter

#show: docs-chapter.with(
  title: "API Reference",
  description: "Gaanim API — Scene, Text, Mobjects, Animations, Themes",
  route: "/api/",
  code-langs: (),
  updated: datetime.today().display(),
)

= API Reference

Toda la superficie pública de Gaanim — una carta técnica sin adornos.

#html.div(style: "display: grid; grid-template-columns: repeat(auto-fit, minmax(260px, 1fr)); gap: 14px; margin: 20px 0 28px;", [
  #html.a(href: "scene/", style: "display: block; background: var(--bg-card); border: 1.5px solid var(--code-border); padding: 18px; text-decoration: none !important;", [
    #html.div(style: "font-family: var(--font-code); font-size: 0.6rem; font-weight: 800; letter-spacing: 0.1em; text-transform: uppercase; color: var(--accent-blue); margin-bottom: 6px; border-bottom: 2px solid var(--accent-blue); display: inline-block; padding-bottom: 2px;", [01 — CORE])
    #html.div(style: "font-weight: 800; font-size: 1.05rem; letter-spacing: -0.02em; color: var(--text-main);", [Scene])
    #html.div(style: "font-size: 0.84rem; color: var(--text-muted); margin-top: 6px;", [Constructor, viewport, factories, timeline, slides, cámara y export. Entrada única.])
    #html.div(style: "margin-top: 12px; font-family: var(--font-code); font-size: 0.62rem; font-weight: 800; color: var(--accent-blue); border: 1.5px solid var(--code-border); display: inline-block; padding: 3px 8px; background: #eff6ff;", [SCENE →])
  ])
  #html.a(href: "text/", style: "display: block; background: var(--bg-card); border: 1.5px solid var(--code-border); padding: 18px; text-decoration: none !important;", [
    #html.div(style: "font-family: var(--font-code); font-size: 0.6rem; font-weight: 800; letter-spacing: 0.1em; text-transform: uppercase; color: var(--accent-violet); margin-bottom: 6px; border-bottom: 2px solid var(--accent-violet); display: inline-block; padding-bottom: 2px;", [TEXT — UNIFIED])
    #html.div(style: "font-weight: 800; font-size: 1.05rem; letter-spacing: -0.02em; color: var(--text-main);", [Text])
    #html.div(style: "font-size: 0.84rem; color: var(--text-muted); margin-top: 6px;", [Prose, matemática, partes semánticas, flujo responsive, selecciones y transiciones estructurales.])
    #html.div(style: "margin-top: 12px; font-family: var(--font-code); font-size: 0.62rem; font-weight: 800; color: var(--accent-violet); border: 1.5px solid var(--code-border); display: inline-block; padding: 3px 8px; background: #f5f3ff;", [TEXT →])
  ])
  #html.a(href: "mobjects/", style: "display: block; background: var(--bg-card); border: 1.5px solid var(--code-border); padding: 18px; text-decoration: none !important;", [
    #html.div(style: "font-family: var(--font-code); font-size: 0.6rem; font-weight: 800; letter-spacing: 0.1em; text-transform: uppercase; color: var(--accent-violet); margin-bottom: 6px; border-bottom: 2px solid var(--accent-violet); display: inline-block; padding-bottom: 2px;", [02 — DRAWABLES · 58])
    #html.div(style: "font-weight: 800; font-size: 1.05rem; letter-spacing: -0.02em; color: var(--text-main);", [Mobjects])
    #html.div(style: "font-size: 0.84rem; color: var(--text-muted); margin-top: 6px;", [58 factories con preview animado — primitivas, flechas, polígonos, paths, plots, texto/math, media, editorial y reactivos.])
    #html.div(style: "margin-top: 12px; font-family: var(--font-code); font-size: 0.62rem; font-weight: 800; color: var(--accent-violet); border: 1.5px solid var(--code-border); display: inline-block; padding: 3px 8px; background: #f5f3ff;", [MOBJECTS →])
  ])
  #html.a(href: "animations/", style: "display: block; background: var(--bg-card); border: 1.5px solid var(--code-border); padding: 18px; text-decoration: none !important;", [
    #html.div(style: "font-family: var(--font-code); font-size: 0.6rem; font-weight: 800; letter-spacing: 0.1em; text-transform: uppercase; color: var(--accent-cyan); margin-bottom: 6px; border-bottom: 2px solid var(--accent-cyan); display: inline-block; padding-bottom: 2px;", [03 — ANIM · 22])
    #html.div(style: "font-weight: 800; font-size: 1.05rem; letter-spacing: -0.02em; color: var(--text-main);", [Animations])
    #html.div(style: "font-size: 0.84rem; color: var(--text-muted); margin-top: 6px;", [move, fade, write, create, grow, spin, wiggle, transform y easing. Todo sobre Drawable.])
    #html.div(style: "margin-top: 12px; font-family: var(--font-code); font-size: 0.62rem; font-weight: 800; color: var(--accent-cyan); border: 1.5px solid var(--code-border); display: inline-block; padding: 3px 8px; background: #ecfeff;", [ANIMATIONS →])
  ])
  #html.a(href: "themes/", style: "display: block; background: var(--bg-card); border: 1.5px solid var(--code-border); padding: 18px; text-decoration: none !important;", [
    #html.div(style: "font-family: var(--font-code); font-size: 0.6rem; font-weight: 800; letter-spacing: 0.1em; text-transform: uppercase; color: var(--accent-emerald); margin-bottom: 6px; border-bottom: 2px solid var(--accent-emerald); display: inline-block; padding-bottom: 2px;", [04 — SYSTEM])
    #html.div(style: "font-weight: 800; font-size: 1.05rem; letter-spacing: -0.02em; color: var(--text-main);", [Colors & Themes])
    #html.div(style: "font-size: 0.84rem; color: var(--text-muted); margin-top: 6px;", [Paleta, brushes, presets light/dark y tokens de estilo.])
    #html.div(style: "margin-top: 12px; font-family: var(--font-code); font-size: 0.62rem; font-weight: 800; color: var(--accent-emerald); border: 1.5px solid var(--code-border); display: inline-block; padding: 3px 8px; background: #ecfdf5;", [THEMES →])
  ])
])

== Cómo leer esta API

Cada entrada muestra:

- *Badge* — `FACTORY` (crea Drawable), `METHOD` (sobre Drawable/Anim), `FUNCTION`.
- *Firma* — tipos y retorno en mono.
- *Parámetros* — Nombre | Tipo | Default | Descripción.
- *Retorno* — tipo y qué representa.
- *Ejemplo* — bloque `python` con `# show-code: true` y `scene.export("preview.webp", fps=30)`. El archivo exportado se detecta automáticamente — ya no hace falta `# output: preview.webp`. El preview aparece debajo del código a ancho completo; click para ampliar al 94% del viewport.

Los ejemplos con preview son escenas mínimas y autocontenidas:

```python
# show-code: true
from gaanim import BLUE, Scene
scene = Scene(480, 270, background="#0f172a")
node = scene.circle(60).fill(BLUE).at(0, 0)
scene.play([node.create().duration(1.0)])
scene.export("preview.webp", fps=30)
```

Copiar, pegar, `gaanim archivo.py`.

== Mapa rápido

- #link("/api/scene/", "Scene — viewport, factories, timeline, slides, cámara, export")
- #link("/api/text/", "Text — prosa, matemática, estilo, flujo, selecciones y transiciones")
- #link("/api/visualization/", "Visualization — ejes, funciones, datos, estadística y cálculo")
- #link("/api/mobjects/", "Mobjects — 40+ factories por categoría")
- #link("/api/animations/", "Animations — 22 anims + timing/easing")
- #link("/api/themes/", "Themes — colores y brushes")
- #link("/api/layout/", "Layout v2 — row, column, grid, stack y constraints") · #link("/api/assets/", "Assets — imágenes y SVG") · #link("/api/audio/", "Audio")

== Siguiente

#link("/api/scene/", "Empezar por Scene →") · #link("/api/text/", "Dominar Text →") · #link("/api/mobjects/", "Ver Mobjects →") · #link("/examples/basic/", "Ejemplos completos →")
