#let html-section(
  title: none,
  title-content: none,
  has-summary: none,
  route: none,
  kind: none,
  description: none,
  body,
) = context {
  if target() != "bundle" {
    [
      #heading(level: 1, title)
      #v(0.5em)
      #body
      #v(1em)
    ]
  } else {
    let path-segments = route.split("/").filter(seg => seg.len() != 0)
    let depth = path-segments.len()
    let prefix = "../" * depth

    let route = route.trim("/")

    if not route.ends-with("/") {
      route += "/"
    }

    set stdx.config(
      asset-base: prefix + "assets/",
    )

    document(route + "index.html", title: title, html.html(lang: "en", {
      html.head({
        html.meta(charset: "utf-8")
        html.meta(
          name: "viewport",
          content: "width=device-width, initial-scale=1",
        )

        html.meta(name: "description", content: description)
        html.meta(name: "theme-color", content: "#6366f1")
        html.meta(name: "view-transition", content: "same-origin")
        html.link(href: prefix + "assets/base.css", rel: "stylesheet")
        html.title(title + " — Gaanim")
        // Inline script to prevent FOUC — runs before first paint
        html.elem(
          "script",
          attrs: (type: "text/javascript"),
          "if(localStorage.getItem('theme'))document.documentElement.setAttribute('data-theme',localStorage.getItem('theme'))",
        )
      })

      html.body({
        let chapter-label = label("chap-" + route.replace(regex("[^a-zA-Z0-9]"), "-"))

        let site-map = (
          "Home": "",
          "Getting Started": (
            "Overview": "getting-started/",
            "Instalación": "getting-started/installation/",
          ),
          "Guides": (
            "Proyectos": "guides/projects/",
            "Slides": "guides/slides/",
            "Regresión visual": "guides/visual-regression/",
            "Layout proposal": "guides/layout-proposal/",
          ),
            "API Reference": (
              "Overview": "api/",
              "Scene": "api/scene/",
              "Visualization": "api/visualization/",
              "Layouts": "api/layout/",
              "Mobjects": "api/mobjects/",
              "Animations": "api/animations/",
              "Colors": "api/themes/",
              "Assets": "api/assets/",
              "Audio": "api/audio/",
            ),
          "Examples": (
            "Basic": "examples/basic/",
            "Advanced": "examples/advanced/",
          ),
        )

        html.div(class: "layout-container", {
          html.aside(class: "nav-sidebar", id: "global-nav-sidebar", {
            html.h3("GaanIm")
            html.div(class: "docs-search", {
              html.div(class: "docs-search-label", "SEARCH DOCUMENTATION")
              html.elem(
                "input",
                attrs: (
                  id: "docs-search-input",
                  type: "search",
                  placeholder: "Buscar en la documentación",
                  autocomplete: "off",
                  spellcheck: "false",
                ),
              )
              html.div(class: "docs-search-hint", "Escribe para buscar · / para enfocar")
              html.div(id: "docs-search-results", class: "docs-search-results", [])
            })
            html.ul({
              for (key, val) in site-map.pairs() {
                if type(val) == str {
                  let active-class = if val == route { "nav-active" } else { "" }
                  html.li(html.a(href: prefix + val, class: active-class, key))
                } else if type(val) == dictionary {
                  let is-active = val.values().contains(route)
                  let details-content = {
                    html.summary(key)
                    html.ul({
                      for (sub-key, sub-val) in val.pairs() {
                        let active-class = if sub-val == route { "nav-active" } else { "" }
                        html.li(html.a(href: prefix + sub-val, class: active-class, sub-key))
                      }
                    })
                  }

                  html.li(
                    if is-active {
                      html.details(open: true, details-content)
                    } else {
                      html.details(details-content)
                    },
                  )
                }
              }
            })
          })

          html.div(class: "main-grid", {
            html.button(id: "nav-toggle-btn", class: "nav-toggle-btn", "☰")
            html.button(id: "theme-toggle-btn", class: "theme-toggle-btn", "")
            context {
              [#html.main({
                  body
                }) #chapter-label]
            }
          })

          if kind == "Chapter" {
            html.aside(class: "toc-sidebar", {
              html.h3("CONTENTS")

              outline(
                title: none,
                indent: 0pt,
                target: selector(heading).within(chapter-label),
              )
            })
          }
        })

        html.script(src: prefix + "assets/script.js")
      })
    }))
  }
}


#let docs-section(
  title: none,
  title-fmt: auto,
  subtitle: none,
  has-summary: false,
  introduction: false,
  route: none,
  kind: none,
  description: none,
  updated: none,
  body,
) = {
  assert.ne(title, none, message: "title is required")
  assert.eq(type(title), str, message: "title must be a string")

  if title-fmt == auto {
    title-fmt = title
  }

  show heading: it => context {
    let content = it
    if it.level == 1 and updated != none {
      if target() == "bundle" {
        content = [#it #html.div(class: "last-updated", [Last updated: #updated])]
      } else {
        content = [#it #text(fill: rgb("#64748b"), size: 8.5pt, [ (Last updated: #updated)])]
      }
    }
    content
  }

  html-section(
    title: title,
    title-content: title-fmt,
    has-summary: has-summary,
    route: route,
    kind: kind,
    description: description,
    body,
  )
}


// =============================================================================
// Code cell engine with WebP animation support
// =============================================================================

#let calc-vars = state("calc-vars", (:))

#let code-cell(
  it,
  lang: "python",
  id: "",
) = {
  context {
    let result = stdx.compile-code-cell(it, lang: lang, id: id)

    let source-raw = raw(result.code.trim(), lang: lang, block: true)
    let source-labeled = [#source-raw <_stop>]

    let source-code = source-labeled

    // Result items (stdout, stderr)
    let result-items = ()

    if result.stdout.len() > 0 {
      if result.stdout.starts-with("[typst]") {
        result-items.push(eval(result.stdout.replace("[typst]", "").trim(), mode: "markup"))
      } else {
        result-items.push(raw(result.stdout.trim(), block: true))
      }
    }

    if result.stderr.len() > 0 {
      result-items.push(
        text(fill: rgb("c53030"), weight: 500, size: 9pt, result.stderr.trim()),
      )
    }

    // Header
    let header-element = if result.caption.len() > 0 {
      if target() == "bundle" {
        html.div(class: "code-header", [
          #html.span(style: "color: var(--accent-purple); font-weight: bold;", "Code:")
          _ #result.caption _
        ])
      } else {
        text(fill: rgb("#4f46e5"), weight: "bold", size: 9pt, [Code: ]) + text(style: "italic", size: 9pt, result.caption)
      }
    } else {
      none
    }

    // Layout: side-by-side if WebP exists, otherwise stacked
    let has-webp = result.webp.len() > 0

    let layout-content = if target() != "bundle" {
      block(
        width: 100%,
        stroke: 0.5pt + rgb("#cbd5e1"),
        inset: 8pt,
        radius: 4pt,
        fill: rgb("#f8fafc"),
        [
          #if header-element != none [ #header-element #v(4pt) ]
          #if result.show_code or not has-webp [
            #source-code
            #v(4pt)
          ]
          #if has-webp [
            #align(center, image("../" + result.webp, width: 85%))
          ]
          #if result-items.len() > 0 [
            #v(4pt)
            #result-items.join()
          ]
        ]
      )
    } else if not result.show_code and not has-webp {
      // Result only (code hidden, no webp)
      html.div(class: "code-result-only", result-items.join())
    } else if has-webp {
      // Side-by-side: code left, WebP right
      html.div(class: "code-cell-anim", {
        html.div(class: "code-source", {
          if header-element != none { header-element }
          source-code
        })
        html.div(class: "anim-preview", {
          image("../" + result.webp, alt: "animation preview")
        })
      })
    } else {
      // Stacked: code + result
      html.div(class: "code-cell", {
        html.div(class: "code-grid", {
          if header-element != none { header-element }
          html.div(class: "code-source", source-code)
          html.div(class: "code-result", result-items.join())
        })
      })
    }

    calc-vars.update(old => old + result.vars) + layout-content
  }
}


// =============================================================================
// Chapter with automatic show rules
// =============================================================================

#let docs-chapter(
  route: none,
  title: none,
  description: none,
  updated: none,
  code-langs: ("python",),
  ..args,
  body,
) = {
  assert.ne(route, none, message: "route is required")
  assert.ne(title, none, message: "title is required")

  set text(lang: "en")
  set heading(numbering: "1.1.")
  set math.equation(numbering: "1.")

  [#metadata((
    title: title,
    route: route,
    description: description,
  )) <blog-post>]

  show raw.where(lang: "python"): it => {
    if "python" not in code-langs or (it.has("label") and it.label == <_stop>) {
      it
    } else {
      code-cell(it, lang: "python")
    }
  }

  docs-section(
    route: route,
    title: title,
    description: description,
    updated: updated,
    ..args,
    kind: "Chapter",
    body,
  )
}
