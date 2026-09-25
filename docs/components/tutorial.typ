#let lesson-box(title, body, class: "lesson-box") = context if target() in ("bundle", "html") {
  html.elem("aside", attrs: (class: class), {
    html.div(class: "lesson-box-title", title)
    body
  })
} else {
  block(
    width: 100%,
    breakable: false,
    inset: 11pt,
    radius: 5pt,
    fill: rgb("#eef2ff"),
    stroke: 0.6pt + rgb("#c7d2fe"),
  )[
    #text(size: 8.5pt, weight: "bold", fill: rgb("#4f46e5"), upper(title))
    #v(4pt)
    #body
  ]
}

#let checkpoint(body) = lesson-box("Punto de control", body)
#let idea(body) = lesson-box("Idea clave", body)

// Marks an API that still changes and has known bugs. `#experimental()` shows
// the standard notice for the 3D API; `#experimental[...]` a custom one.
#let experimental(..body) = lesson-box(
  "Experimental",
  body.pos().at(0, default: [
    La API 3D es experimental: todavía tiene errores conocidos y puede cambiar
    entre versiones. Úsala para explorar, pero no la tomes como estable.
  ]),
  class: "lesson-box lesson-box-experimental",
)
