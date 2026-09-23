#import "../components/section.typ": docs-chapter

#show: docs-chapter.with(
  title: "Gaanim",
  description: "Ayuda y referencia de Gaanim, el motor de animación vectorial 2D acelerado por GPU para Python",
  route: "/",
)

#let card(href, eyebrow, title, body) = html.a(href: href, class: "home-card", {
  html.span(class: "home-card-eyebrow", eyebrow)
  html.span(class: "home-card-title", title)
  html.span(class: "home-card-body", body)
})

#html.div(class: "home-hero", [
  #html.div(class: "home-hero-kicker", [Documentación · Python · GPU])
  #html.h1(style: "margin: 0;", [Ayuda de Gaanim])
  #html.div(class: "home-hero-desc", [Escribe escenas en Python y anímalas con un
    renderer vectorial acelerado por GPU. Busca una función, sigue una receta o
    aprende desde cero.])
  #html.div(class: "home-search", {
    html.elem(
      "input",
      attrs: (
        id: "home-search-input",
        type: "search",
        placeholder: "Buscar: move_to, cámara, exportar MP4…",
        autocomplete: "off",
        spellcheck: "false",
        "aria-label": "Buscar en la documentación",
      ),
    )
    html.div(class: "home-search-hint", {
      [Busca por nombre o por tarea, en español o en inglés. Atajo: ]
      html.elem("kbd", "Ctrl K")
      [ o ]
      html.elem("kbd", "/")
    })
  })
  #html.div(class: "home-hero-cta", [
    #html.a(href: "manual/guia-rapida/", class: "primary", [Guía rápida →])
    #html.a(href: "getting-started/", class: "secondary", [Instalar])
    #html.a(href: "api/", class: "secondary", [Referencia de la API])
  ])
])

== Empieza en tres pasos

#html.div(class: "home-steps", {
  html.div(class: "home-step", {
    html.span(class: "home-step-number", "1")
    html.span(class: "home-step-title", "Instala y crea un proyecto")
    html.span(class: "home-step-body", [Descarga el ejecutable `gaanim` y crea un proyecto con
      `gaanim init video mi-video`. #link("/getting-started/")[Ver instalación.]])
  })
  html.div(class: "home-step", {
    html.span(class: "home-step-number", "2")
    html.span(class: "home-step-title", "Escribe la escena")
    html.span(class: "home-step-body", [Edita `main.py`: crea objetos con `scene.geometry`
      y descríbelos en el tiempo con `.animate` y `scene.play`.])
  })
  html.div(class: "home-step", {
    html.span(class: "home-step-number", "3")
    html.span(class: "home-step-title", "Previsualiza y exporta")
    html.span(class: "home-step-body", [`gaanim .` abre la vista previa con recarga al
      guardar; `gaanim export . --output video.mp4` genera el archivo final.])
  })
})

== Tu primera escena

La escena mide 16 × 9 unidades lógicas con el origen en el centro. Cada
animación se describe con `.animate` y se programa con `scene.play`:

```python
# show-code: true
from gaanim import BLUE, GOLD, WHITE, Easing, Scene

scene = Scene(frame=(16, 9), background="#0f172a")
title = scene.text("Hola, Gaanim", role="title").fill(WHITE).move_to(0, 3)
circle = scene.geometry.circle(1.2).fill(BLUE).move_to(-3, -0.5)
arrow = scene.geometry.arrow(-1.4, -0.5, 1.4, -0.5).fill(GOLD).stroke(GOLD, 0.02)
square = scene.geometry.rect(2.4, 2.4).fill(GOLD).move_to(3, -0.5)

scene.play([title.animate.write().duration(1.0)])
scene.play([
    circle.animate.create().duration(1.0),
    arrow.animate.create().duration(1.0),
    square.animate.grow_from_center().duration(1.0).easing(Easing.SMOOTH),
])
scene.play([circle.animate.shift_by(0, 1).duration(0.6)])
# output: preview.webp
scene.render()
```

== ¿Qué quieres hacer?

#html.div(class: "home-cards", {
  card("api/mobjects/", "Objetos", "Dibujar formas", [Círculos, flechas, polígonos, curvas, imágenes y SVG.])
  card("manual/animaciones/", "Animaciones", "Animar objetos", [`create`, `write`, `fade_in`, `indicate`, transformaciones y easings.])
  card("api/text/", "Texto", "Texto y ecuaciones", [Tipografía y matemáticas con Typst, selección de glifos y `write`.])
  card("api/visualization/", "Datos", "Gráficas y ejes", [Ejes, funciones, `ChartSpec`, campos vectoriales y estadística.])
  card("guides/layout/", "Layout", "Organizar la escena", [Anclas, grids, regiones y flujos que se adaptan al contenido.])
  card("api/scene/", "Escena", "Tiempo y cámara", [`play`, `wait`, segmentos, `stop()` y movimientos de cámara.])
  card("guides/slides/", "Presentaciones", "Dar una charla", [Segmentos, pasos con `stop()` y Presenter Mode.])
  card("api/themes/", "Estilo", "Colores y temas", [Paletas, degradados, efectos y temas reutilizables.])
  card("guides/projects/", "Proyectos", "Organizar el código", [Estructura con `src/`, recarga en vivo y exportación.])
})

== Referencia rápida

Las llamadas más usadas. Cada fila enlaza a su página de referencia.

#table(
  columns: 3,
  table.header[*Quiero…*][*Código*][*Referencia*],
  [Crear la escena], [`scene = Scene(frame=(16, 9))`], link("/api/scene/")[Escena],
  [Dibujar una forma], [`scene.geometry.circle(1.2).fill(BLUE)`], link("/api/mobjects/")[Objetos],
  [Colocar un objeto], [`obj.move_to(x, y)`], link("/api/mobjects/")[Objetos],
  [Escribir texto], [`scene.text("Hola", role="title")`], link("/api/text/")[Texto],
  [Escribir una ecuación], [`scene.text.equation("e^(i pi) + 1 = 0")`], link("/api/text/")[Texto],
  [Animar una propiedad], [`obj.animate.move_to(2, 0).duration(1.0)`], link("/api/animations/")[Animaciones],
  [Hacer aparecer un objeto], [`obj.animate.create()` · `write()` · `fade_in()`], link("/api/animations/")[Animaciones],
  [Resaltar un objeto], [`obj.animate.indicate()`], link("/api/animations/")[Animaciones],
  [Reproducir y esperar], [`scene.play([...])` · `scene.wait(1.0)`], link("/api/scene/")[Escena],
  [Mover la cámara], [`scene.camera.animate.zoom_to(1.5)`], link("/api/scene/")[Escena],
  [Pausar en una presentación], [`scene.stop("paso")`], link("/guides/slides/")[Presentaciones],
  [Exportar video], [`gaanim export . --output video.mp4`], link("/getting-started/installation/")[Instalación],
)

== Aprende paso a paso

#html.div(class: "home-cards home-cards-learn", {
  card("manual/introduccion/", "Fundamentos", "Manual", [Escena, objetos y animaciones explicados en orden.])
  card("guia/antes-de-empezar/", "Proyecto práctico", "Del círculo al seno", [Ocho capítulos para construir una explicación visual completa.])
  card("examples/basic/", "Taller", "Ejemplos", [Escenas listas para copiar, de lo básico a lo avanzado.])
})

== Más recursos

- #link("documentation.pdf")[Descargar toda la documentación en PDF]
- #link("https://github.com/PaoloLupo/gaanim")[Código fuente en GitHub]
- #link("https://github.com/PaoloLupo/gaanim/issues")[Reportar un problema o proponer una mejora]
