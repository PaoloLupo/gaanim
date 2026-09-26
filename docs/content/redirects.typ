// Routes of the documentation before the 2026 restructuring. Each old
// address keeps working as a page that forwards to its new location, so
// links from READMEs, projects and search engines do not break.
#let moved = (
  ("api/", "referencia/"),
  ("api/animations/", "referencia/animations/"),
  ("api/assets/", "referencia/assets/"),
  ("api/audio/", "referencia/audio/"),
  ("api/layout/", "referencia/layout/"),
  ("api/matrices/", "referencia/matrices/"),
  ("api/mobjects/", "referencia/geometria/"),
  ("api/scene/", "referencia/scene/"),
  ("api/text/", "referencia/text/"),
  ("api/themes/", "referencia/themes/"),
  ("api/visualization/", "referencia/visualization/"),
  ("examples/advanced/", "ejemplos/avanzados/"),
  ("examples/basic/", "ejemplos/basicos/"),
  ("getting-started/", "empezar/instalacion/"),
  ("getting-started/installation/", "empezar/instalacion/"),
  ("guia/animar-tiempo/", "tutorial/animar-tiempo/"),
  ("guia/antes-de-empezar/", "tutorial/antes-de-empezar/"),
  ("guia/circulo-al-seno/", "tutorial/circulo-al-seno/"),
  ("guia/componer-explicar/", "tutorial/componer-explicar/"),
  ("guia/objetos-estilo/", "tutorial/objetos-estilo/"),
  ("guia/primera-escena/", "tutorial/primera-escena/"),
  ("guia/reactividad/", "tutorial/reactividad/"),
  ("guia/terminar-proyecto/", "tutorial/terminar-proyecto/"),
  ("guias/avanzado/", "guias/reactividad/"),
  ("guides/layout/", "guias/layout/"),
  ("guides/migration-0-2/", "apendices/novedades/"),
  ("guides/performance/", "guias/proyectos/"),
  ("guides/projects/", "guias/proyectos/"),
  ("guides/slides/", "guias/presentaciones/"),
  ("guides/visual-regression/", "guias/capturas-y-comparacion/"),
  ("manual/animaciones/", "empezar/como-piensa-gaanim/"),
  ("manual/avanzado/", "guias/reactividad/"),
  ("manual/escena/", "empezar/como-piensa-gaanim/"),
  ("manual/guia-rapida/", "empezar/primera-animacion/"),
  ("manual/introduccion/", "empezar/como-piensa-gaanim/"),
  ("manual/objetos/", "empezar/como-piensa-gaanim/"),
  ("referencia/objetos/", "referencia/geometria/"),
)

#for (old, new) in moved {
  let prefix = "../" * old.split("/").filter(segment => segment != "").len()
  document(old + "index.html", title: "Gaanim", html.html(lang: "es", {
    html.head({
      html.meta(charset: "utf-8")
      html.elem("meta", attrs: ("http-equiv": "refresh", content: "0; url=" + prefix + new))
      html.elem("link", attrs: (rel: "canonical", href: prefix + new))
      html.title("Gaanim")
    })
    html.body(html.p([Esta página se movió a ] + html.a(href: prefix + new, new)))
  }))
}
