#import "../../components/section.typ": docs-chapter

#show: docs-chapter.with(
  title: "Referencia de la API",
  description: "Firmas, parámetros y contratos de la superficie pública de Gaanim",
  route: "/api/",
  code-langs: (),
)

#let card(href, eyebrow, title, body) = html.a(href: href, class: "home-card", {
  html.span(class: "home-card-eyebrow", eyebrow)
  html.span(class: "home-card-title", title)
  html.span(class: "home-card-body", body)
})

= Referencia de la API

La explicación está escrita en español. Los nombres de clases, métodos,
parámetros, valores literales y mensajes que aparecen en el código se conservan
en inglés porque forman parte de la API ejecutable.

#html.div(class: "home-cards", {
  card("scene/", "Núcleo", "Escena", [`Scene`, viewport, línea de tiempo, segmentos, cámara y exportación.])
  card("text/", "Texto", "Texto y ecuaciones", [Prosa, matemáticas con Typst, partes semánticas, flujo, selecciones y transiciones.])
  card("visualization/", "Datos", "Visualización", [Ejes, funciones, `ChartSpec`, campos vectoriales, estadística y cálculo.])
  card("layout/", "Composición", "Layout", [Filas, columnas, grids, capas, anclas y restricciones.])
  card("assets/", "Proyecto", "Recursos", [Imágenes, SVG, fuentes y el manifiesto `gaanim.toml`.])
  card("audio/", "Sonido", "Audio", [Pistas sincronizadas con la línea de tiempo y marcas por palabra.])
  card("mobjects/", "Objetos", "Objetos", [Fábricas de `scene.geometry`, `media`, `slides` y `viz`: primitivas, trayectorias, medios y objetos reactivos.])
  card("matrices/", "Matemáticas", "Matrices", [`scene.viz.matrix`: selecciones, cambios estructurales, morph y álgebra.])
  card("animations/", "Tiempo", "Animaciones", [`.animate`, entradas, énfasis, transformaciones, composición y easing.])
  card("themes/", "Estilo", "Colores y temas", [Paleta, pinceles, degradados, efectos y temas reutilizables.])
})

== Cómo leer esta API

Cada entrada muestra:

- *Insignia*: `FÁBRICA` (crea un `Drawable`), `MÉTODO` (de `Drawable`, `Anim` u
  otra clase), `CLASE` o `FUNCIÓN`.
- *Firma*: tipos y retorno en tipografía monoespaciada.
- *Parámetros*: nombre, tipo, valor predeterminado y descripción.
- *Retorno*: tipo y qué representa.
- *Ejemplo*: una escena mínima y autocontenida. Si muestra una vista previa, el
  ejecutable la generó a partir de ese mismo código.

Los ejemplos se pueden copiar y ejecutar tal cual con `gaanim archivo.py`:

```python
from gaanim import BLUE, Scene

scene = Scene(frame=(16, 9), background="#0f172a")
node = scene.geometry.circle(0.75).fill(BLUE).move_to(0, 0)
scene.play([node.animate.create().duration(1.0)])
scene.render()
```

¿Buscas un método concreto? Pulsa `Ctrl K` o `/` y escribe su nombre o la tarea,
en español o en inglés.

== Siguiente

Si es tu primera vez, empieza por #link("/api/scene/")[Escena]. Para ver la API
en escenas completas, consulta los #link("/examples/basic/")[ejemplos].
