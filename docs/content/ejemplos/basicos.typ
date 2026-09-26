#import "../../components/section.typ": docs-chapter
#import "escenas.typ": catalogo, scene-block

#show: docs-chapter.with(
  title: "Ejemplos básicos",
  description: "Escenas pequeñas y completas para empezar a copiar y modificar",
  route: "/ejemplos/basicos/",
  nav: "Básicos",
)

Escenas cortas que usan lo esencial: figuras, texto, ecuaciones, grupos y
animaciones en paralelo o escalonadas. Cada una es un script completo: cópiala
en un archivo `.py`, ábrela con `gaanim archivo.py` y cambia una sola decisión
cada vez para ver qué hace.

Todas siguen el mismo patrón: crean una `Scene`, construyen objetos con sus
fábricas, pasan a `scene.play([...])` las animaciones de `.animate` y terminan
con `scene.render()`. Si es la primera vez que ves este patrón, empieza por
#link("/empezar/primera-animacion/")[Tu primera animación].

#for entry in catalogo.filter(entry => entry.page == "basicos") [
  #heading(level: 1, entry.title) #label(entry.label)

  #entry.summary

  #scene-block(entry)
]

Cuando domines estas escenas, sigue con los
#link("/ejemplos/avanzados/")[ejemplos avanzados] o vuelve a la
#link("/ejemplos/")[galería].
