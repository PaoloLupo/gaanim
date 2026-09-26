#import "../components/book.typ": book-part

#context if target() in ("bundle", "html") {
  include "home.typ"
  include "redirects.typ"
}

#book-part("I", "Empezar", description: "Instala Gaanim, crea tu primera animación y aprende cómo piensa una escena")
#include "empezar/instalacion.typ"
#include "empezar/primera-animacion.typ"
#include "empezar/como-piensa-gaanim.typ"

#book-part("II", "Tutorial: del círculo al seno", description: "Un proyecto completo, capítulo a capítulo, que siempre se puede ejecutar", numbered: true)
#include "tutorial/01-antes-de-empezar.typ"
#include "tutorial/02-primera-escena.typ"
#include "tutorial/03-objetos-estilo.typ"
#include "tutorial/04-animar-tiempo.typ"
#include "tutorial/05-componer-explicar.typ"
#include "tutorial/06-reactividad.typ"
#include "tutorial/07-circulo-al-seno.typ"
#include "tutorial/08-terminar-proyecto.typ"

#book-part("III", "Guías", description: "Cómo resolver cada tarea: composición, reactividad, presentaciones y producción")
#include "guias/layout.typ"
#include "guias/movimiento.typ"
#include "guias/tipografia-cinetica.typ"
#include "guias/transiciones.typ"
#include "guias/efectos.typ"
#include "guias/reactividad.typ"
#include "guias/camara-y-3d.typ"
#include "guias/presentaciones.typ"
#include "guias/proyectos.typ"
#include "guias/capturas-y-comparacion.typ"

#book-part("IV", "Ejemplos", description: "Escenas completas listas para copiar y ejecutar")
#include "ejemplos/index.typ"
#include "ejemplos/basicos.typ"
#include "ejemplos/avanzados.typ"

#book-part("V", "Referencia", description: "Firmas, parámetros y contratos de la superficie pública")
#include "referencia/index.typ"
#include "referencia/scene.typ"
#include "referencia/drawable.typ"
#include "referencia/geometria.typ"
#include "referencia/text.typ"
#include "referencia/layout.typ"
#include "referencia/visualization.typ"
#include "referencia/matrices.typ"
#include "referencia/medios.typ"
#include "referencia/audio.typ"
#include "referencia/assets.typ"
#include "referencia/diapositivas.typ"
#include "referencia/mecanica.typ"
#include "referencia/animations.typ"
#include "referencia/themes.typ"
#include "referencia/cli.typ"
#include "referencia/gaanim-toml.typ"

#book-part("VI", "Apéndices", description: "Novedades de cada versión, glosario y solución de problemas")
#include "apendices/novedades.typ"
#include "apendices/glosario.typ"
#include "apendices/solucion-de-problemas.typ"
