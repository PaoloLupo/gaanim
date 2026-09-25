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

#let feature(title, body) = html.div(class: "home-feature", {
  html.span(class: "home-feature-title", title)
  html.span(class: "home-feature-body", body)
})

// The hero scene is drawn by assets/hero.js on the canvas behind the copy; the
// empty stage marks where the animation plays.
#html.div(class: "home-hero", [
  #html.elem("canvas", attrs: (class: "home-hero-canvas", "aria-hidden": "true"))
  #html.div(class: "home-hero-grid", [
    #html.div(class: "home-hero-copy", [
      #html.div(class: "home-hero-kicker", [Animación vectorial · Python · GPU])
      #html.h1([Anima ideas con #html.span(class: "home-hero-accent", [código])])
      #html.div(class: "home-hero-desc", [Gaanim describe escenas en Python, las
        muestra en vivo mientras escribes y las renderiza con trazos vectoriales en la
        GPU, listas para exportar a video.])
      #html.div(class: "home-hero-cta", [
        #html.a(href: "empezar/primera-animacion/", class: "primary", [Primera animación →])
        #html.a(href: "empezar/instalacion/", class: "secondary", [Instalar])
        #html.a(href: "tutorial/antes-de-empezar/", class: "secondary", [Tutorial])
      ])
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
    ])
    #html.elem("div", attrs: (class: "home-hero-stage", "aria-hidden": "true"))
  ])
])
#html.elem("script", attrs: (type: "module", src: "assets/hero.js"))

#html.div(class: "home-features", {
  feature("Escenas en Python", [Una API fluida: setters inmediatos para el estado y
    `.animate` para describir el tiempo.])
  feature("Vectores en la GPU", [Vello sobre Bevy: trazos nítidos a cualquier
    resolución y vista previa con recarga al guardar.])
  feature("Texto y matemáticas", [Tipografía y ecuaciones con Typst, animables glifo
    a glifo con `write`.])
  feature("Exporta a video", [MP4, WebM con transparencia, WebP, GIF y secuencias
    PNG desde la misma escena.])
})

== Empieza en tres pasos

#html.div(class: "home-steps", {
  html.div(class: "home-step", {
    html.span(class: "home-step-number", "1")
    html.span(class: "home-step-title", "Instala y crea un proyecto")
    html.span(class: "home-step-body", [Descarga el ejecutable `gaanim` y crea un proyecto con
      `gaanim init video mi-video`. #link("/empezar/instalacion/")[Ver instalación.]])
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

== Elige por dónde seguir

#let door(href, eyebrow, title, body, links) = html.div(class: "home-card home-door", {
  html.a(href: href, class: "home-door-main", {
    html.span(class: "home-card-eyebrow", eyebrow)
    html.span(class: "home-card-title", title)
    html.span(class: "home-card-body", body)
  })
  html.ul(class: "home-door-links", {
    for (label, target) in links {
      html.li(html.a(href: target, label))
    }
  })
})

#html.div(class: "home-cards home-doors", {
  door("empezar/instalacion/", "Empezar", "Primeros pasos", [Instala Gaanim, anima tu primera escena y entiende cómo piensa.], (
    ("Instalación", "empezar/instalacion/"),
    ("Primera animación", "empezar/primera-animacion/"),
    ("Cómo piensa Gaanim", "empezar/como-piensa-gaanim/"),
  ))
  door("tutorial/antes-de-empezar/", "Tutorial", "Del círculo al seno", [Un proyecto completo en ocho capítulos que siempre se puede ejecutar.], (
    ("Antes de empezar", "tutorial/antes-de-empezar/"),
    ("Animar el tiempo", "tutorial/animar-tiempo/"),
    ("Terminar el proyecto", "tutorial/terminar-proyecto/"),
  ))
  door("guias/layout/", "Guías", "Resolver una tarea", [Composición, movimiento, texto animado, presentaciones y producción.], (
    ("Layout", "guias/layout/"),
    ("Presentaciones", "guias/presentaciones/"),
    ("Proyectos y exportación", "guias/proyectos/"),
  ))
  door("referencia/", "Referencia", "Consultar la API", [Firmas, parámetros y ejemplos de cada objeto y animación.], (
    ("Escena", "referencia/scene/"),
    ("Animaciones", "referencia/animations/"),
    ("Texto y ecuaciones", "referencia/text/"),
  ))
})

#html.div(class: "home-more", [
  #link("/ejemplos/basicos/")[Ejemplos] ·
  #link("/apendices/novedades/")[Novedades] ·
  #link("documentation.pdf")[Toda la documentación en PDF] ·
  #link("https://github.com/PaoloLupo/gaanim")[Código fuente] ·
  #link("https://github.com/PaoloLupo/gaanim/issues")[Reportar un problema]
])
