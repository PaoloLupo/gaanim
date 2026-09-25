#import "book.typ": book-outline
#let html-section(
  title: none,
  title-content: none,
  has-summary: none,
  route: none,
  kind: none,
  description: none,
  body,
) = context {
  if target() not in ("bundle", "html") {
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

    // Authors write site-absolute routes such as `link("/referencia/scene/")`. Emit
    // them relative to this page so the site works under any base path,
    // e.g. GitHub Pages' `/<repo>/`.
    show link: it => {
      if type(it.dest) == str and it.dest.starts-with("/") and not it.dest.starts-with("//") {
        let relative = prefix + it.dest.slice(1)
        link(if relative == "" { "./" } else { relative }, it.body)
      } else {
        it
      }
    }

    document(route + "index.html", title: title, html.html(lang: "es", {
      html.head({
        html.meta(charset: "utf-8")
        html.meta(
          name: "viewport",
          content: "width=device-width, initial-scale=1",
        )

        html.meta(name: "description", content: description)
        html.elem("meta", attrs: (name: "theme-color", content: "#ffffff", media: "(prefers-color-scheme: light)"))
        html.elem("meta", attrs: (name: "theme-color", content: "#121212", media: "(prefers-color-scheme: dark)"))
        html.meta(name: "view-transition", content: "same-origin")
        // ICO first with an explicit size so browsers that read SVG prefer the SVG.
        html.elem("link", attrs: (rel: "icon", href: prefix + "assets/brand/favicon.ico", sizes: "32x32"))
        html.elem("link", attrs: (rel: "icon", href: prefix + "assets/brand/gaanim-icon.svg", type: "image/svg+xml"))
        html.elem("link", attrs: (rel: "apple-touch-icon", href: prefix + "assets/brand/apple-touch-icon.png"))
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

        // Parts and chapters in reading order, from `index.typ`.
        let outline-parts = book-outline()
        let pages = outline-parts.map(part => part.pages).flatten()
        let current = pages.position(page => page.route == route)
        let section-name = outline-parts
          .find(part => part.pages.any(page => page.route == route))
        let section-name = if section-name == none { none } else { section-name.title }
        let href(route) = if prefix + route == "" { "./" } else { prefix + route }

        html.header(class: "site-header", {
          html.elem("button", attrs: (
            id: "nav-toggle-btn",
            class: "icon-btn nav-toggle-btn",
            type: "button",
            "aria-label": "Mostrar u ocultar la navegación",
          ), "☰")
          html.a(href: if prefix == "" { "./" } else { prefix }, class: "brand", {
            // 2 px per logo unit keeps the pixel symbol on whole device pixels.
            for (variant, file) in (("light", "gaanim-logo.svg"), ("dark", "gaanim-logo-dark.svg")) {
              html.elem("img", attrs: (
                class: "brand-logo only-" + variant,
                src: prefix + "assets/brand/" + file,
                width: "142",
                height: "32",
                alt: "Gaanim",
              ))
            }
            html.span(class: "brand-tag", "docs")
          })
          html.elem("button", attrs: (id: "search-trigger", class: "search-trigger", type: "button", "aria-label": "Buscar"), {
            html.span(class: "search-trigger-icon", "⌕")
            html.span(class: "search-trigger-text", "Buscar en la API y las guías…")
            html.elem("kbd", attrs: (class: "search-trigger-kbd"), "Ctrl K")
          })
          html.nav(class: "header-links", {
            html.a(href: prefix + "empezar/instalacion/", "Empezar")
            html.a(href: prefix + "tutorial/antes-de-empezar/", "Tutorial")
            html.a(href: prefix + "guias/layout/", "Guías")
            html.a(href: prefix + "referencia/", "Referencia")
            html.a(href: "https://github.com/PaoloLupo/gaanim", class: "header-github", "GitHub")
          })
          html.elem("button", attrs: (id: "theme-toggle-btn", class: "icon-btn theme-toggle-btn", type: "button", "aria-label": "Cambiar tema"), "")
        })

        html.div(class: "layout-container", {
          html.aside(class: "nav-sidebar", id: "global-nav-sidebar", {
            html.elem("nav", attrs: ("aria-label": "Documentación"), html.ul({
              html.li(html.a(href: href(""), class: if route == "/" { "nav-active" } else { "" }, "Inicio"))
              for part in outline-parts {
                let details-content = {
                  html.summary(part.title)
                  html.ul({
                    for page in part.pages {
                      let active-class = if page.route == route { "nav-active" } else { "" }
                      html.li(html.a(href: href(page.route), class: active-class, page.title))
                    }
                  })
                }
                html.li(
                  if part.pages.any(page => page.route == route) {
                    html.details(open: true, details-content)
                  } else {
                    html.details(details-content)
                  },
                )
              }
            }))
          })
          html.div(class: "nav-backdrop", id: "nav-backdrop", [])

          html.div(class: "main-grid", {
            if route != "/" {
              html.elem("nav", attrs: (class: "breadcrumb", "aria-label": "Ruta"), {
                html.a(href: prefix, "Inicio")
                if section-name != none and section-name != title {
                  html.span(class: "breadcrumb-sep", "/")
                  html.span(section-name)
                }
                html.span(class: "breadcrumb-sep", "/")
                html.span(class: "breadcrumb-current", title)
              })
            }
            context {
              [#html.main(class: if route == "/" { "page-home" } else { "page" }, {
                  // Book numbering ("53.9.1.") belongs to the PDF, not the web.
                  set heading(numbering: none)
                  // The page header already shows the title; drop a first
                  // section heading that merely repeats it.
                  show heading.where(level: 1): it => {
                    if it.body.has("text") and it.body.text == title { none } else { it }
                  }
                  if route != "/" {
                    html.header(class: "page-header", {
                      html.h1(title)
                      if description != none { html.p(class: "page-lead", description) }
                    })
                  }
                  body
                  // Reading order: previous and next page of the book.
                  if current != none {
                    let link-to(page, direction, label) = html.a(href: href(page.route), class: "page-nav-" + direction, {
                      html.span(class: "page-nav-label", label)
                      html.span(class: "page-nav-title", page.title)
                    })
                    html.elem("nav", attrs: (class: "page-nav", "aria-label": "Páginas contiguas"), {
                      if current > 0 { link-to(pages.at(current - 1), "prev", "← Anterior") }
                      if current + 1 < pages.len() { link-to(pages.at(current + 1), "next", "Siguiente →") }
                    })
                  }
                }) #chapter-label]
            }
            html.footer(class: "site-footer", {
              html.span([Gaanim · Documentación generada con Typst])
              html.a(href: prefix + "documentation.pdf", "Descargar PDF")
              html.a(href: "https://github.com/PaoloLupo/gaanim/issues", "Reportar un problema")
            })
          })

          if kind == "Chapter" and route != "/" {
            html.aside(class: "toc-sidebar", {
              html.div(class: "toc-title", "En esta página")

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
  body,
) = {
  assert.ne(title, none, message: "title is required")
  assert.eq(type(title), str, message: "title must be a string")

  if title-fmt == auto {
    title-fmt = title
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

// Source of the page's last executed cell, replayed by `# continue`.
#let cell-chain = state("docs-cell-chain", none)

#let code-cell(
  it,
  lang: "python",
  id: "",
) = {
  context {
    let continues = str(it.text).split("\n").any(line => line.trim() == "# continue")
    let prelude = if continues { cell-chain.get() } else { "" }
    assert(
      prelude != none,
      message: "`# continue` needs an executed Python block earlier on the same page",
    )
    let result = stdx.compile-code-cell(it, lang: lang, id: id, prelude: prelude)

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
      let error-text = text(fill: rgb("c53030"), weight: 500, size: 9pt, result.stderr.trim())
      // The class lets CI detect examples that failed to run.
      result-items.push(
        if target() in ("bundle", "html") { html.div(class: "docs-example-error", error-text) } else { error-text },
      )
    }

    // Header
    let header-element = if result.caption.len() > 0 {
      if target() in ("bundle", "html") {
        html.div(class: "code-header", [
          #html.span(style: "color: var(--accent-purple); font-weight: bold;", "Código:")
          _ #result.caption _
        ])
      } else {
        text(fill: rgb("#4f46e5"), weight: "bold", size: 9pt, [Código: ]) + text(style: "italic", size: 9pt, result.caption)
      }
    } else {
      none
    }

    // Layout: side-by-side if WebP exists, otherwise stacked
    let layout-content = if target() not in ("bundle", "html") {
      block(
        width: 100%,
        stroke: 0.5pt + rgb("#cbd5e1"),
        inset: 8pt,
        radius: 4pt,
        fill: rgb("#f8fafc"),
        [
          #if header-element != none [ #header-element #v(4pt) ]
          #if not result.hide_code or not has-webp [
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
    } else if result.hide_code and not has-webp and result-items.len() > 0 {
      // Result only: the cell asked to hide its code (`# hide-code`) and shows
      // what it printed. A cell with nothing to show falls through to the code.
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

    cell-chain.update(result.chain) + calc-vars.update(old => old + result.vars) + layout-content
  }
}

// Every Python block runs with the real runtime unless its first lines say
// `# no-run: <motivo>`. The reason is required so a skipped block stays a
// deliberate, reviewable exception.
#let python-block(it) = {
  if it.has("label") and it.label == <_stop> {
    return it
  }
  let lines = str(it.text).split("\n")
  let marker = lines.find(line => line.trim().starts-with("# no-run"))
  if marker == none {
    return code-cell(it, lang: "python")
  }
  let reason = marker.trim().slice("# no-run".len()).trim()
  assert(
    reason.starts-with(":") and reason.slice(1).trim() != "",
    message: "`# no-run` needs a reason: `# no-run: <motivo>`",
  )
  let shown = lines.filter(line => not line.trim().starts-with("# no-run")).join("\n")
  [#raw(shown.trim(), lang: "python", block: true) <_stop>]
}


// =============================================================================
// Chapter with automatic show rules
// =============================================================================

#let docs-chapter(
  route: none,
  title: none,
  description: none,
  nav: none,
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
  context if target() not in ("bundle", "html") {
    pagebreak(to: "odd")
  }

  [#metadata((
    title: title,
    nav: nav,
    route: route,
    description: description,
  )) <blog-post>]

  // Each page starts its own `# continue` chain.
  cell-chain.update(none)

  show raw.where(lang: "python"): it => {
    if "python" not in code-langs { it } else { python-block(it) }
  }

  docs-section(
    route: route,
    title: title,
    description: description,
    ..args,
    kind: "Chapter",
    body,
  )
}
