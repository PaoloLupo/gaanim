#let lesson-box(title, body) = context if target() in ("bundle", "html") {
  html.elem("aside", attrs: (class: "lesson-box"), {
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
