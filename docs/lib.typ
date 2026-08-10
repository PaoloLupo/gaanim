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
      margin: (x: 2cm, top: 2.5cm, bottom: 2.5cm),
      header: context {
        let page-num = counter(page).get().first()
        if page-num > 1 [
          #text(fill: rgb("#64748b"), size: 9pt, [Gaanim Documentation])
          #h(1fr)
          #text(fill: rgb("#64748b"), size: 9pt, [GPU 2D Animation Engine])
          #v(2pt)
          #line(length: 100%, stroke: 0.5pt + rgb("#cbd5e1"))
        ]
      },
      footer: context {
        let page-num = counter(page).get().first()
        if page-num > 1 [
          #line(length: 100%, stroke: 0.5pt + rgb("#cbd5e1"))
          #v(2pt)
          #align(center, text(fill: rgb("#64748b"), size: 9pt, str(page-num)))
        ]
      },
    )
    set text(font: "Liberation Sans", size: 10pt, fill: rgb("#0f172a"))
    set par(justify: true, leading: 0.65em)

    align(center + horizon)[
      #v(2cm)
      #text(size: 32pt, weight: "bold", fill: rgb("#4f46e5"))[Gaanim]
      #v(0.8em)
      #text(size: 16pt, weight: "medium", fill: rgb("#334155"))[GPU-Accelerated 2D Vector Animation Engine]
      #v(0.5em)
      #text(size: 11pt, fill: rgb("#64748b"))[Comprehensive Technical Documentation & API Reference]
      #v(3cm)
      #align(left)[
        #text(size: 14pt, weight: "bold", fill: rgb("#1e293b"))[Table of Contents]
        #v(0.8em)
        #outline(indent: 1.5em)
      ]
    ]
    pagebreak()
  }

  include "content/index.typ"
}
