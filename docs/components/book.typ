#let book-part(number, title, description: none) = context {
  if target() != "bundle" {
    pagebreak(weak: true)
    block(height: 78%, width: 100%)[
      #align(center + horizon)[
        #text(size: 9pt, tracking: 0.16em, weight: "bold", fill: rgb("#6366f1"))[
          PARTE #number
        ]
        #v(0.65cm)
        #text(size: 29pt, weight: "bold", fill: rgb("#1e293b"), title)
        #if description != none [
          #v(0.55cm)
          #block(width: 72%)[
            #align(center, text(size: 11pt, fill: rgb("#64748b"), description))
          ]
        ]
        #v(0.8cm)
        #rect(width: 52pt, height: 4pt, fill: rgb("#4f46e5"), radius: 2pt)
      ]
    ]
  }
}
