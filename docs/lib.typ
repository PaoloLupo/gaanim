#let docs(
  content-base: "/",
  asset-base: "assets/",
) = {
  assert(content-base.starts-with("/") and content-base.ends-with("/"))
  assert(asset-base.ends-with("/"))

  set stdx.config(
    content-base: content-base,
    asset-base: asset-base,
  )

  context if target() == "bundle" {
    include "assets/index.typ"
  } else {
    set page(
      paper: "a4",
      margin: (inside: 2.35cm, outside: 1.85cm, top: 2.35cm, bottom: 2.2cm),
      binding: left,
      header: context {
        let page-num = counter(page).get().first()
        if page-num > 2 [
          #text(fill: rgb("#64748b"), size: 8pt, weight: "medium", [GAANIM · MANUAL])
          #h(1fr)
          #text(fill: rgb("#94a3b8"), size: 8pt, [Animación vectorial con Python])
          #v(3pt)
          #line(length: 100%, stroke: 0.45pt + rgb("#cbd5e1"))
        ]
      },
      footer: context {
        let page-num = counter(page).get().first()
        if page-num > 2 [
          #line(length: 100%, stroke: 0.45pt + rgb("#cbd5e1"))
          #v(3pt)
          #align(center, text(fill: rgb("#64748b"), size: 8.5pt, weight: "medium", str(page-num)))
        ]
      },
    )
    set text(font: "Aleo", size: 10.2pt, fill: rgb("#172033"))
    set par(justify: true, leading: 0.72em)
    set heading(numbering: "1.1.")
    show raw: set text(font: "Victor Mono", size: 8.4pt)

    // Portada independiente.
    block(height: 100%)[
      #align(center + horizon)[
        #rect(width: 42pt, height: 5pt, fill: rgb("#4f46e5"), radius: 2.5pt)
        #v(1.1cm)
        #text(size: 38pt, weight: "bold", fill: rgb("#4338ca"))[Gaanim]
        #v(0.75em)
        #text(size: 17pt, weight: "medium", fill: rgb("#334155"))[Libro de animación vectorial]
        #v(0.55em)
        #text(size: 11pt, fill: rgb("#64748b"))[Aprende construyendo escenas; consulta la API al final]
        #v(2.1cm)
        #line(length: 58%, stroke: 0.8pt + rgb("#c7d2fe"))
        #v(0.7cm)
        #text(size: 9pt, tracking: 0.08em, weight: "bold", fill: rgb("#6366f1"))[PYTHON · VELLO · BEVY · GPU]
      ]
    ]
    pagebreak()

    // Índice en sus propias páginas; solo capítulos y secciones principales.
    [
      #text(size: 9pt, tracking: 0.12em, weight: "bold", fill: rgb("#6366f1"))[GUÍA DE LECTURA]
      #v(6pt)
      #text(size: 26pt, weight: "bold", fill: rgb("#1e293b"))[Índice general]
      #v(0.8em)
      #outline(title: none, depth: 1, indent: 1.2em)
    ]
    pagebreak()
  }

  include "content/index.typ"
}
