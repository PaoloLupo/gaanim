#import "../components/book.typ": book-part

#context if target() == "bundle" {
  include "home.typ"
}

#book-part("I", "Una animación, paso a paso", description: "Construiremos una explicación visual del movimiento circular y la curva seno")
#include "guia/01-antes-de-empezar.typ"
#include "guia/02-primera-escena.typ"
#include "guia/03-objetos-estilo.typ"
#include "guia/04-animar-tiempo.typ"
#include "guia/05-componer-explicar.typ"
#include "guia/06-reactividad.typ"
#include "guia/07-circulo-al-seno.typ"
#include "guia/08-terminar-proyecto.typ"

#book-part("II", "Después del primer proyecto", description: "Organización, presentaciones y validación visual")
#include "guides/projects.typ"
#include "guides/slides.typ"
#include "guides/visual-regression.typ"

#book-part("III", "Referencia de la API", description: "Firmas, parámetros, retornos y ejemplos de la superficie pública")
#include "api/index.typ"
#include "api/scene.typ"
#include "api/text.typ"
#include "api/visualization.typ"
#include "api/layout.typ"
#include "api/assets.typ"
#include "api/audio.typ"
#include "api/mobjects.typ"
#include "api/animations.typ"
#include "api/themes.typ"
