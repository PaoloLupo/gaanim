#import "../../components/section.typ": docs-chapter
#import "escenas.typ": catalogo, scene-code

#show: docs-chapter.with(
  title: "Galería de ejemplos",
  description: "Vistas previas de escenas completas, listas para copiar y ejecutar",
  route: "/ejemplos/",
  nav: "Galería",
)

Cada tarjeta es una escena completa. Ábrela para ver el código, cópialo en un
archivo `.py` y ejecútalo con `gaanim archivo.py`.

#context {
  let entries = catalogo.filter(entry => entry.at("preview", default: true)).map(entry => {
    let result = stdx.compile-code-cell(raw(scene-code(entry), lang: "python", block: true), lang: "python")
    (entry: entry, webp: result.webp)
  })
  let eyebrow(entry) = if entry.page == "basicos" { "Básico" } else { "Avanzado" }
  if target() in ("bundle", "html") {
    html.div(class: "home-cards", {
      for item in entries {
        let entry = item.entry
        html.a(href: entry.page + "/#" + entry.label, class: "home-card", {
          if item.webp.len() > 0 {
            image("../../" + item.webp, alt: entry.title)
          }
          html.span(class: "home-card-eyebrow", eyebrow(entry))
          html.span(class: "home-card-title", entry.title)
          html.span(class: "home-card-body", entry.summary)
        })
      }
    })
  } else {
    grid(
      columns: (1fr, 1fr),
      gutter: 14pt,
      ..entries.map(item => block(breakable: false, {
        if item.webp.len() > 0 { image("../../" + item.webp, width: 100%) }
        text(size: 7.5pt, weight: "bold", fill: rgb("#4f46e5"), upper(eyebrow(item.entry)))
        linebreak()
        strong(item.entry.title)
        linebreak()
        text(size: 9pt, item.entry.summary)
      })),
    )
  }
}

Los #link("/ejemplos/avanzados/")[ejemplos avanzados] incluyen además una
composición vertical (9:16) y una escena 3D, que no aparecen aquí: la primera
se exporta con una resolución vertical y la API 3D todavía es experimental.
