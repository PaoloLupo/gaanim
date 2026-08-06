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
) = {
  assert.ne(name, none, message: "api-entry: name is required")

  let kind-label = upper(kind)

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
        html.div(class: "api-params-title", "Parameters")
        html.table({
          html.thead(html.tr({
            html.th("Name")
            html.th("Type")
            html.th("Default")
            html.th("Description")
          }))
          html.tbody({
            for p in params {
              html.tr({
                html.td(html.elem("code", p.at("name", default: "-")))
                html.td(html.elem("code", p.at("type", default: "-")))
                let def = p.at("default", default: none)
                html.td(if def == none { html.span(class: "api-required", "required") } else { html.elem("code", str(def)) })
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
        html.span(class: "api-returns-label", "Returns: ")
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
        html.div(class: "api-example-label", "Example")
        {
          show raw.where(lang: "python"): it => {
            if it.has("label") and it.label == <_stop> {
              it
            } else {
              let t = str(it.text)
              if t.contains("show-code") or t.contains("# output") {
                code-cell(it)
              } else {
                it
              }
            }
          }
          body
        }
      })
    }
  })
}
