#import "../../components/section.typ": docs-chapter

#show: docs-chapter.with(
  title: "Referencia de la API",
  description: "API de Gaanim: Scene, texto, objetos, animaciones y temas",
  route: "/api/",
  code-langs: (),
  updated: datetime.today().display(),
)

= Referencia de la API

Toda la superficie pública de Gaanim — una carta técnica sin adornos.

La explicación está escrita en español. Los nombres de clases, métodos,
parámetros, valores literales y mensajes que aparecen en el código se conservan
en inglés porque forman parte de la API ejecutable.

#html.div(style: "display: grid; grid-template-columns: repeat(auto-fit, minmax(260px, 1fr)); gap: 14px; margin: 20px 0 28px;", [
  #html.a(href: "scene/", style: "display: block; background: var(--bg-card); border: 1.5px solid var(--code-border); padding: 18px; text-decoration: none !important;", [
    #html.div(style: "font-family: var(--font-code); font-size: 0.6rem; font-weight: 800; letter-spacing: 0.1em; text-transform: uppercase; color: var(--accent-blue); margin-bottom: 6px; border-bottom: 2px solid var(--accent-blue); display: inline-block; padding-bottom: 2px;", [01 — NÚCLEO])
    #html.div(style: "font-weight: 800; font-size: 1.05rem; letter-spacing: -0.02em; color: var(--text-main);", [Scene])
    #html.div(style: "font-size: 0.84rem; color: var(--text-muted); margin-top: 6px;", [Constructor, viewport, factories, timeline, slides, cámara y export. Entrada única.])
    #html.div(style: "margin-top: 12px; font-family: var(--font-code); font-size: 0.62rem; font-weight: 800; color: var(--accent-blue); border: 1.5px solid var(--code-border); display: inline-block; padding: 3px 8px; background: #eff6ff;", [SCENE →])
  ])
  #html.a(href: "text/", style: "display: block; background: var(--bg-card); border: 1.5px solid var(--code-border); padding: 18px; text-decoration: none !important;", [
    #html.div(style: "font-family: var(--font-code); font-size: 0.6rem; font-weight: 800; letter-spacing: 0.1em; text-transform: uppercase; color: var(--accent-violet); margin-bottom: 6px; border-bottom: 2px solid var(--accent-violet); display: inline-block; padding-bottom: 2px;", [TEXT — UNIFICADO])
    #html.div(style: "font-weight: 800; font-size: 1.05rem; letter-spacing: -0.02em; color: var(--text-main);", [Text])
    #html.div(style: "font-size: 0.84rem; color: var(--text-muted); margin-top: 6px;", [Prose, matemática, partes semánticas, flujo responsive, selecciones y transiciones estructurales.])
    #html.div(style: "margin-top: 12px; font-family: var(--font-code); font-size: 0.62rem; font-weight: 800; color: var(--accent-violet); border: 1.5px solid var(--code-border); display: inline-block; padding: 3px 8px; background: #f5f3ff;", [TEXT →])
  ])
  #html.a(href: "mobjects/", style: "display: block; background: var(--bg-card); border: 1.5px solid var(--code-border); padding: 18px; text-decoration: none !important;", [
    #html.div(style: "font-family: var(--font-code); font-size: 0.6rem; font-weight: 800; letter-spacing: 0.1em; text-transform: uppercase; color: var(--accent-violet); margin-bottom: 6px; border-bottom: 2px solid var(--accent-violet); display: inline-block; padding-bottom: 2px;", [02 — OBJETOS · 58])
    #html.div(style: "font-weight: 800; font-size: 1.05rem; letter-spacing: -0.02em; color: var(--text-main);", [Objetos])
    #html.div(style: "font-size: 0.84rem; color: var(--text-muted); margin-top: 6px;", [58 fábricas con vista previa animada: primitivas, flechas, polígonos, trayectorias, gráficos, texto y matemáticas, medios, composición editorial y objetos reactivos.])
    #html.div(style: "margin-top: 12px; font-family: var(--font-code); font-size: 0.62rem; font-weight: 800; color: var(--accent-violet); border: 1.5px solid var(--code-border); display: inline-block; padding: 3px 8px; background: #f5f3ff;", [MOBJECTS →])
  ])
  #html.a(href: "animations/", style: "display: block; background: var(--bg-card); border: 1.5px solid var(--code-border); padding: 18px; text-decoration: none !important;", [
    #html.div(style: "font-family: var(--font-code); font-size: 0.6rem; font-weight: 800; letter-spacing: 0.1em; text-transform: uppercase; color: var(--accent-cyan); margin-bottom: 6px; border-bottom: 2px solid var(--accent-cyan); display: inline-block; padding-bottom: 2px;", [03 — ANIM · 22])
    #html.div(style: "font-weight: 800; font-size: 1.05rem; letter-spacing: -0.02em; color: var(--text-main);", [Animaciones])
    #html.div(style: "font-size: 0.84rem; color: var(--text-muted); margin-top: 6px;", [move, fade, write, create, grow, spin, wiggle, transform y easing. Todo sobre Drawable.])
    #html.div(style: "margin-top: 12px; font-family: var(--font-code); font-size: 0.62rem; font-weight: 800; color: var(--accent-cyan); border: 1.5px solid var(--code-border); display: inline-block; padding: 3px 8px; background: #ecfeff;", [ANIMATIONS →])
  ])
  #html.a(href: "matrices/", style: "display: block; background: var(--bg-card); border: 1.5px solid var(--code-border); padding: 18px; text-decoration: none !important;", [
    #html.div(style: "font-family: var(--font-code); font-size: 0.6rem; font-weight: 800; letter-spacing: 0.1em; text-transform: uppercase; color: var(--accent-cyan); margin-bottom: 6px;", [MATH — MATRIX])
    #html.div(style: "font-weight: 800; font-size: 1.05rem; color: var(--text-main);", [Matrices])
    #html.div(style: "font-size: 0.84rem; color: var(--text-muted); margin-top: 6px;", [Selecciones 2D, cambios estructurales, morph y álgebra SymPy animable.])
  ])
  #html.a(href: "themes/", style: "display: block; background: var(--bg-card); border: 1.5px solid var(--code-border); padding: 18px; text-decoration: none !important;", [
    #html.div(style: "font-family: var(--font-code); font-size: 0.6rem; font-weight: 800; letter-spacing: 0.1em; text-transform: uppercase; color: var(--accent-emerald); margin-bottom: 6px; border-bottom: 2px solid var(--accent-emerald); display: inline-block; padding-bottom: 2px;", [04 — SISTEMA])
    #html.div(style: "font-weight: 800; font-size: 1.05rem; letter-spacing: -0.02em; color: var(--text-main);", [Colores y temas])
    #html.div(style: "font-size: 0.84rem; color: var(--text-muted); margin-top: 6px;", [Paleta, pinceles, temas claros y oscuros, y tokens de estilo.])
    #html.div(style: "margin-top: 12px; font-family: var(--font-code); font-size: 0.62rem; font-weight: 800; color: var(--accent-emerald); border: 1.5px solid var(--code-border); display: inline-block; padding: 3px 8px; background: #ecfdf5;", [THEMES →])
  ])
])

== Cómo leer esta API

Cada entrada muestra:

- *Insignia* — `FACTORY` (crea un `Drawable`), `METHOD` (método de `Drawable` o `Anim`) y `FUNCTION`.
- *Firma* — tipos y retorno con tipografía monoespaciada.
- *Parámetros* — Nombre | Tipo | Valor predeterminado | Descripción.
- *Retorno* — tipo y qué representa.
- *Ejemplo* — bloque `python` con `# show-code: true`, `# output: preview.webp`
  y `scene.render()`. El ejecutable genera la vista previa debajo del código.

Los ejemplos con preview son escenas mínimas y autocontenidas:

```python
# show-code: true
from gaanim import BLUE, Scene
scene = Scene(480, 270, background="#0f172a")
node = scene.circle(60).fill(BLUE).at(0, 0)
scene.play([node.create().duration(1.0)])
# output: preview.webp
scene.render()
```

Copiar, pegar, `gaanim archivo.py`.

== Mapa rápido

- #link("/api/scene/", "Scene — viewport, fábricas, línea de tiempo, diapositivas, cámara y exportación")
- #link("/api/text/", "Text — prosa, matemática, estilo, flujo, selecciones y transiciones")
- #link("/api/visualization/", "Visualización — ejes, funciones, datos, estadística y cálculo")
- #link("/api/mobjects/", "Objetos — más de 40 fábricas por categoría")
- #link("/api/matrices/", "Matrices — selección, mutación, morph y álgebra")
- #link("/api/animations/", "Animaciones — 22 animaciones, tiempo y curvas de ritmo")
- #link("/api/themes/", "Temas — colores y pinceles")
- #link("/api/layout/", "Layout v2 — filas, columnas, grids, capas y restricciones") · #link("/api/assets/", "Recursos — imágenes y SVG") · #link("/api/audio/", "Audio")

== Siguiente

#link("/api/scene/", "Empezar por Scene →") · #link("/api/text/", "Dominar Text →") · #link("/api/mobjects/", "Ver Mobjects →") · #link("/examples/basic/", "Ejemplos completos →")
