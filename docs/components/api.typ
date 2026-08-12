#import "section.typ": docs-chapter, code-cell

// API entry card — params, return, kind badge + live rendered example
// Body python blocks with `# show-code: true` (and optional `# output:` + `scene.export`) compile to code + WebP preview.
// Plain fragment blocks without magic comments render as static code (no execution).

#let badge-color(kind) = {
  if kind == "factory" { rgb("#6366f1") }
  else if kind == "method" { rgb("#7c3aed") }
  else if kind == "class" { rgb("#0891b2") }
  else { rgb("#88c0d0") }
}

#let api-entry(
  name: none,
  kind: "function",
  signature: none,
  params: (),
  returns: none,
  desc: none,
  body,
) = context {
  assert.ne(name, none, message: "api-entry: name is required")

  let kind-label = upper(if kind == "factory" { "fábrica" }
    else if kind == "method" { "método" }
    else if kind == "class" { "clase" }
    else if kind == "function" { "función" }
    else { kind })

  if target() != "bundle" {
    block(
      width: 100%,
      stroke: 0.5pt + rgb("#e2e8f0"),
      inset: 10pt,
      radius: 6pt,
      fill: rgb("#ffffff"),
      [
        #heading(level: 3, name)
        #v(2pt)
        #box(fill: badge-color(kind), inset: (x: 5pt, y: 2pt), radius: 3pt)[
          #text(fill: white, weight: "bold", size: 8pt, kind-label)
        ]
        #h(6pt)
        #if signature != none [
          #raw(signature)
        ]
        #if desc != none [
          #v(4pt)
          #desc
        ]
        #if params.len() > 0 [
          #v(6pt)
          #text(weight: "bold", size: 9pt, [Parámetros:])
          #v(2pt)
          #table(
            columns: (auto, auto, auto, 1fr),
            stroke: 0.3pt + rgb("#cbd5e1"),
            fill: (x, y) => if y == 0 { rgb("#f1f5f9") } else { none },
            [*Nombre*], [*Tipo*], [*Predeterminado*], [*Descripción*],
            ..params.map(p => (
              raw(p.at("name", default: "-")),
              raw(p.at("type", default: "-")),
              if p.at("default", default: none) == none { text(fill: rgb("#dc2626"), [obligatorio]) } else { raw(str(p.at("default"))) },
              p.at("desc", default: [-]),
            )).flatten()
          )
        ]
        #if returns != none [
          #v(4pt)
          #text(weight: "bold", size: 9pt, [Devuelve: ])
          #raw(returns.at("type", default: "-"))
          #if returns.at("desc", default: none) != none [
            — #returns.at("desc")
          ]
        ]
        #if body != none [
          #v(6pt)
          #text(weight: "bold", size: 9pt, [Ejemplo:])
          #v(2pt)
          #show raw.where(lang: "python"): it => {
            if it.has("label") and it.label == <_stop> {
              it
            } else {
              let t = str(it.text)
              if t.contains("show-code") or t.contains("output:") {
                code-cell(it)
              } else {
                it
              }
            }
          }
          #body
        ]
      ]
    )
  } else {
    html.div(class: "api-entry", {
      // header: badge + name + signature
      html.div(class: "api-entry-header", {
        html.span(class: "api-badge badge-" + kind, kind-label)
        html.span(class: "api-name", name)
        if signature != none {
          html.span(class: "api-signature", signature)
        }
      })

      if desc != none {
        html.div(class: "api-desc", desc)
      }

      // params table
      if params.len() > 0 {
        html.div(class: "api-params", {
          html.div(class: "api-params-title", "Parámetros")
          html.table({
            html.thead(html.tr({
              html.th("Nombre")
              html.th("Tipo")
              html.th("Predeterminado")
              html.th("Descripción")
            }))
            html.tbody({
              for p in params {
                html.tr({
                  html.td(html.elem("code", p.at("name", default: "-")))
                  html.td(html.elem("code", p.at("type", default: "-")))
                  let def = p.at("default", default: none)
                  html.td(if def == none { html.span(class: "api-required", "obligatorio") } else { html.elem("code", str(def)) })
                  html.td(p.at("desc", default: [-]))
                })
              }
            })
          })
        })
      }

      // returns
      if returns != none {
        html.div(class: "api-returns", {
          html.span(class: "api-returns-label", "Devuelve: ")
          html.elem("code", returns.at("type", default: "-"))
          if returns.at("desc", default: none) != none {
            html.span(" — ")
            returns.at("desc")
          }
        })
      }

      // example slot — enables gaanim compilation locally even when parent docs-chapter has code-langs: ()
      // Only blocks with `# show-code: true` or `# output:` are executed; plain fragments stay static.
      if body != none {
        html.div(class: "api-example", {
          html.div(class: "api-example-label", "Ejemplo")
          [
            #show raw.where(lang: "python"): it => {
              if it.has("label") and it.label == <_stop> {
                it
              } else {
                let t = str(it.text)
                if t.contains("show-code") or t.contains("output:") {
                  code-cell(it)
                } else {
                  it
                }
              }
            }
            #body
          ]
        })
      }
    })
  }
}
