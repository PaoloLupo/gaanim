#import "../components/book.typ": book-part

#context if target() == "bundle" {
  include "home.typ"
}

#book-part("I", "Aprender el lenguaje de Gaanim", description: "Del primer objeto a una escena con composición, estilo y tiempo")
#include "manual/introduccion.typ"
#include "getting-started/index.typ"
#include "getting-started/installation.typ"
#include "manual/guia-rapida.typ"
#include "manual/escena.typ"
#include "manual/objetos.typ"
#include "manual/animaciones.typ"

#book-part("II", "Proyecto práctico: del círculo al seno", description: "Construiremos una explicación visual completa y razonaremos cada decisión")
#include "guia/01-antes-de-empezar.typ"
#include "guia/02-primera-escena.typ"
#include "guia/03-objetos-estilo.typ"
#include "guia/04-animar-tiempo.typ"
#include "guia/05-componer-explicar.typ"
#include "guia/06-reactividad.typ"
#include "guia/07-circulo-al-seno.typ"
#include "guia/08-terminar-proyecto.typ"

#book-part("III", "Taller de escenas", description: "Recetas para explorar texto, datos, Layout, reactividad, 3D y producción")
#include "examples/basic.typ"
#include "examples/advanced.typ"
#include "manual/avanzado.typ"
#include "guides/layout.typ"
#include "guides/projects.typ"
#include "guides/slides.typ"
#include "guides/visual-regression.typ"
#include "guides/performance.typ"
#include "guides/migration-0-2.typ"

#book-part("IV", "Referencia de la API", description: "Apéndice de firmas, parámetros y contratos de la superficie pública actual")
#include "api/index.typ"
#include "api/scene.typ"
#include "api/text.typ"
#include "api/visualization.typ"
#include "api/layout.typ"
#include "api/assets.typ"
#include "api/audio.typ"
#include "api/mobjects.typ"
#include "api/matrices.typ"
#include "api/animations.typ"
#include "api/themes.typ"
