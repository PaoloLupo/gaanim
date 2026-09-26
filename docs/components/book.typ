// A part of the book. Besides the PDF divider page, it records its title so
// the site navigation can group the chapters that follow it.
#let book-part(number, title, description: none, numbered: false) = {
  [#metadata((title: title, numbered: numbered)) <book-part>]
  context if target() not in ("bundle", "html") {
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

// The site's navigation, in reading order: the home page, then each part with
// its chapters. Derived from `index.typ`, so a page cannot be in the book and
// missing from the menu. Routes are relative (`referencia/scene/`); the home
// page is `""`.
#let book-outline() = {
  let parts = ()
  for entry in query(selector(<book-part>).or(<blog-post>)) {
    let value = entry.value
    if entry.label == <book-part> {
      parts.push((title: value.title, numbered: value.numbered, pages: ()))
    } else if parts.len() > 0 {
      let route = value.route.trim("/", at: start)
      let page = (title: value.at("nav", default: none), route: route)
      let page = if page.title == none { (..page, title: value.title) } else { page }
      let part = parts.pop()
      if part.numbered {
        page.title = str(part.pages.len() + 1) + ". " + page.title
      }
      part.pages.push(page)
      parts.push(part)
    }
  }
  parts
}
