#import "section.typ": docs-chapter, python-block

// API entry card — params, return, kind badge + live rendered example.
// Every Python block in the body runs through the Gaanim executable unless it
// says `# no-run: <motivo>` (see `python-block` in section.typ).

// Public symbols of `gaanim_core.pyi`, keyed by qualified name.
#let api-symbols = {
  let symbols = (:)
  for symbol in json(bytes(stdx.python-api())) {
    symbols.insert(symbol.name, symbol.signature)
  }
  symbols
}

// `Class.member` names an entry claims to document. A bare member after a
// separator belongs to the previous class: `Drawable.fill / stroke`.
#let api-entry-symbols(name) = {
  let owner = none
  let found = ()
  for token in name.split(regex("\\s*(/|,| and )\\s*")) {
    let token = token.trim()
    let qualified = token.match(regex("^([A-Z]\\w*)\\.(\\w+)$"))
    if qualified != none {
      owner = qualified.captures.at(0)
      found.push(token)
    } else if owner != none and token.match(regex("^[a-z_]\\w*$")) != none {
      found.push(owner + "." + token)
    }
  }
  found
}

// `Drawable` documents the methods reached through `.animate` too.
#let api-symbol-exists(symbol) = {
  let (owner, member) = symbol.split(".")
  let candidates = if owner in ("Drawable", "Anim") {
    ("Drawable." + member, "Anim." + member)
  } else {
    (symbol,)
  }
  candidates.any(candidate => candidate in api-symbols)
}

#let badge-color(kind) = {
  if kind == "factory" { rgb("#6366f1") }
  else if kind == "method" { rgb("#7c3aed") }
  else if kind == "class" { rgb("#0891b2") }
  else if kind == "property" { rgb("#059669") }
  else if kind == "constant" { rgb("#db2777") }
  else { rgb("#d97706") }
}

// Plain text of rich content, for the search index.
#let plain-text(it) = {
  if it == none { "" }
  else if type(it) == str { it }
  else if type(it) != content { "" }
  else if it.has("text") { plain-text(it.text) }
  else if it.has("children") { it.children.map(plain-text).join("") }
  else if it.has("body") { plain-text(it.body) }
  else if it.has("child") { plain-text(it.child) }
  else if repr(it.func()) == "space" { " " }
  else { "" }
}

// Stable anchor for an entry name, e.g. `Drawable.move_to` -> `api-drawable-move-to`.
#let api-anchor(name) = "api-" + lower(name).replace(regex("[^a-z0-9]+"), "-").trim("-")

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

  // The reference may not document what the stub does not expose.
  let symbols = api-entry-symbols(name)
  let missing = symbols.filter(symbol => not api-symbol-exists(symbol))
  assert(
    missing.len() == 0,
    message: "api-entry \"" + name + "\" names symbols missing from gaanim_core.pyi: " + missing.join(", "),
  )
  // Without a hand-written signature, show the stub's.
  let signature = if signature == none and symbols.len() == 1 and symbols.first() in api-symbols {
    api-symbols.at(symbols.first())
  } else {
    signature
  }

  let kind-labels = (
    factory: "fábrica",
    method: "método",
    class: "clase",
    function: "función",
    property: "propiedad",
    constant: "constante",
  )
  assert(
    kind in kind-labels,
    message: "api-entry \"" + name + "\": kind must be one of " + kind-labels.keys().join(", "),
  )
  let kind-label = upper(kind-labels.at(kind))

  if target() not in ("bundle", "html") {
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
          #show raw.where(lang: "python"): python-block
          #body
        ]
      ]
    )
  } else {
    let anchor = api-anchor(name)
    let page = query(selector(<blog-post>).before(here())).at(-1, default: none)
    [#metadata((
      name: name,
      kind: kind,
      signature: if signature == none { "" } else { signature },
      summary: plain-text(desc).replace(regex("\s+"), " ").trim(),
      route: if page == none { "/" } else { page.value.route },
      anchor: anchor,
    )) <api-entry-meta>]
    html.div(class: "api-entry", id: anchor, {
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

      if body != none {
        html.div(class: "api-example", {
          html.div(class: "api-example-label", "Ejemplo")
          [
            #show raw.where(lang: "python"): python-block
            #body
          ]
        })
      }
    })
  }
}
