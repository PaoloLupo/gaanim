#import "../../components/section.typ": docs-chapter
#import "../../components/tutorial.typ": experimental
#import "escenas.typ": catalogo, scene-block

#show: docs-chapter.with(
  title: "Ejemplos avanzados",
  description: "Escenas completas de texto, datos, composición, reactividad, movimiento y 3D",
  route: "/ejemplos/avanzados/",
  nav: "Avanzados",
)

Estas escenas no forman una segunda referencia de la API. Cada una responde a
una pregunta de diseño y combina varias capacidades que suelen usarse juntas.
Copia una, ábrela con `gaanim archivo.py` y cambia una sola decisión cada vez.

#for entry in catalogo.filter(entry => entry.page == "avanzados") [
  #heading(level: 1, entry.title) #label(entry.label)

  #if entry.at("experimental", default: false) { experimental() }

  #entry.summary

  #scene-block(entry)
]

= Cómo elegir la siguiente escena

- Si el problema es la legibilidad, empieza por el texto estructurado y
  #link("/guias/layout/")[Layout].
- Si una forma depende de otra, usa un parámetro y geometría reactiva
  (#link("/guias/reactividad/")[Reactividad]).
- Si el contenido es editorial, prefiere componentes con tema antes que
  coordenadas sueltas.
- Si necesitas profundidad, valida primero material, luz y cámara por separado
  (#link("/guias/camara-y-3d/")[Cámara y 3D]).
