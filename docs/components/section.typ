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
    let book-body(content) = {
      set heading(offset: 1)
      show heading.where(level: 2): it => if repr(it.body) == "[" + title + "]" {
        none
      } else {
        block(
          width: 100%,
          breakable: false,
          above: 1.45em,
          below: 0.58em,
        )[
          #line(length: 34pt, stroke: 2.2pt + rgb("#6366f1"))
          #v(4pt)
          #text(size: 15.5pt, weight: "bold", fill: rgb("#1e293b"), it)
        ]
      }
      show heading.where(level: 3): it => block(
        width: 100%,
        breakable: false,
        above: 1.05em,
        below: 0.34em,
        inset: (left: 8pt),
        stroke: (left: 1.6pt + rgb("#a5b4fc")),
      )[
        #text(size: 11.7pt, weight: "bold", fill: rgb("#334155"), it)
      ]
      show heading.where(level: 4): it => block(
        breakable: false,
        above: 0.82em,
        below: 0.22em,
      )[
        #text(size: 9.9pt, weight: "bold", fill: rgb("#4f46e5"), it)
      ]
      content
    }

    [
      #block(
        width: 100%,
        breakable: false,
        inset: (left: 18pt, right: 18pt, top: 16pt, bottom: 18pt),
        stroke: (left: 4pt + rgb("#4f46e5")),
        fill: rgb("#f5f3ff"),
        radius: (right: 6pt),
      )[
        #text(size: 8pt, weight: "bold", tracking: 0.12em, fill: rgb("#6366f1"))[CAPÍTULO]
        #v(5pt)
        #heading(level: 1, title)
        #if description != none [
          #v(4pt)
          #text(size: 10pt, fill: rgb("#475569"), description)
        ]
      ]
      #v(1.4em)
      #book-body(body)
      #v(1.5em)
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

    document(route + "index.html", title: title, html.html(lang: "es", {
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
          "Inicio": "",
          "Fundamentos": (
            "Introducción": "manual/introduccion/",
            "Instalación rápida": "getting-started/",
            "Instalación detallada": "getting-started/installation/",
            "Guía rápida": "manual/guia-rapida/",
            "Scene": "manual/escena/",
            "Objetos": "manual/objetos/",
            "Animaciones": "manual/animaciones/",
          ),
          "Proyecto práctico": (
            "1. Antes de empezar": "guia/antes-de-empezar/",
            "2. Primera escena": "guia/primera-escena/",
            "3. Objetos y estilo": "guia/objetos-estilo/",
            "4. Animar el tiempo": "guia/animar-tiempo/",
            "5. Componer y explicar": "guia/componer-explicar/",
            "6. Dar vida a la escena": "guia/reactividad/",
            "7. Del círculo al seno": "guia/circulo-al-seno/",
            "8. Terminar el proyecto": "guia/terminar-proyecto/",
          ),
          "Taller de escenas": (
            "Ejemplos básicos": "examples/basic/",
            "Ejemplos avanzados": "examples/advanced/",
            "Temas avanzados": "manual/avanzado/",
            "Layout": "guides/layout/",
            "Proyectos": "guides/projects/",
            "Presentaciones": "guides/slides/",
            "Regresión visual": "guides/visual-regression/",
          ),
            "Referencia de la API": (
              "Índice": "api/",
              "Escena": "api/scene/",
              "Visualización": "api/visualization/",
              "Layouts": "api/layout/",
              "Objetos": "api/mobjects/",
              "Animaciones": "api/animations/",
              "Texto": "api/text/",
              "Colores y temas": "api/themes/",
              "Recursos": "api/assets/",
              "Audio": "api/audio/",
            ),
        )

        html.div(class: "layout-container", {
          html.aside(class: "nav-sidebar", id: "global-nav-sidebar", {
            html.h3("GaanIm")
            html.div(class: "docs-search", {
              html.div(class: "docs-search-label", "BUSCAR EN LA DOCUMENTACIÓN")
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
              html.h3("CONTENIDO")

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
        content = [#it #html.div(class: "last-updated", [Última actualización: #updated])]
      } else {
        content = [#it #text(fill: rgb("#64748b"), size: 8.5pt, [ (Última actualización: #updated)])]
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

    let has-webp = result.webp.len() > 0

    // La salida normal del exportador no aporta información cuando ya existe
    // una vista previa. Los errores siempre permanecen visibles.
    let result-items = ()

    if result.stdout.len() > 0 and not has-webp {
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

  set text(lang: "es")
  set heading(numbering: "1.1.")
  set math.equation(numbering: "1.")

  // Mantener el salto fuera del contexto diferido de html-section evita que
  // algunas aperturas queden desplazadas por encima del area imprimible.
  context if target() != "bundle" {
    pagebreak(to: "odd")
  }

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
