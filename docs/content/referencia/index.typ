#import "../../components/section.typ": docs-chapter

#show: docs-chapter.with(
  title: "Referencia de la API",
  description: "Firmas, parámetros y comportamiento de la superficie pública de Gaanim",
  route: "/referencia/",
  nav: "Referencia",
)

#let card(href, eyebrow, title, body) = html.a(href: href, class: "home-card", {
  html.span(class: "home-card-eyebrow", eyebrow)
  html.span(class: "home-card-title", title)
  html.span(class: "home-card-body", body)
})

= Referencia de la API

Cada página describe un namespace de la escena (`scene.geometry`,
`scene.text`, `scene.viz`…) o una clase central. Las explicaciones están en
español; los nombres de clases, métodos, parámetros y valores literales se
conservan en inglés porque forman parte de la API ejecutable.

#html.div(class: "home-cards", {
  card("scene/", "Núcleo", "Escena", [`Scene`, lienzo, línea de tiempo, segmentos, cámara y salida.])
  card("drawable/", "Núcleo", "Drawable", [El handle de todos los objetos: estilo, posición, efectos, anclajes y relaciones reactivas.])
  card("geometria/", "scene.geometry", "Geometría", [Primitivas, líneas, flechas, trayectorias, booleanas, geometría reactiva y 3D.])
  card("text/", "scene.text", "Texto", [Prosa, ecuaciones, partes semánticas, estilo, flujo, selecciones y documentos Typst.])
  card("layout/", "scene.layout", "Layout", [Filas, columnas, grids, capas, reglas por hijo, reflow y restricciones.])
  card("visualization/", "scene.viz", "Visualización", [Espacios de coordenadas, cálculo, datos, valores reactivos, gráficos y campos vectoriales.])
  card("matrices/", "scene.viz", "Matrices", [Matrices seleccionables y mutables, morph entre estados y álgebra.])
  card("medios/", "scene.media", "Medios", [Imágenes, SVG, video, Lottie y modelos glTF.])
  card("audio/", "scene.media", "Audio", [Pistas sincronizadas y narración grabada en el editor.])
  card("diapositivas/", "scene.slides", "Diapositivas", [Insignias, tarjetas, rótulos, listas, tablas e identidad de la presentación.])
  card("mecanica/", "scene.mechanics", "Mecánica", [Cotas, barras, muelles, fuerzas, apoyos y engranajes reactivos.])
  card("animations/", "Tiempo", "Animaciones", [`.animate`, entradas, énfasis, transformaciones, composición y easing.])
  card("themes/", "Estilo", "Colores y temas", [Paleta, pinceles, degradados, efectos y temas reutilizables.])
  card("assets/", "Proyecto", "Recursos", [Carpeta de assets, precarga, SVG, Lottie y glTF en detalle.])
})

== Cómo leer una ficha

Cada ficha documenta un símbolo:

- *Insignia*: `FÁBRICA` (crea un objeto en la escena), `MÉTODO`, `PROPIEDAD`,
  `CLASE` o `FUNCIÓN`.
- *Firma*: tipos, valores predeterminados y retorno, tomados del stub de tipos
  de Gaanim salvo que la ficha los escriba.
- *Parámetros*: los que necesitan explicación, con su tipo y su valor
  predeterminado.
- *Ejemplo*: código que se ejecuta con el runtime real al construir esta
  documentación. Si muestra una vista previa, se generó con ese mismo código.

Los ejemplos con `scene.render()` son escenas completas que puedes guardar como
`main.py` y abrir con `gaanim`:

```python
from gaanim import BLUE, Scene

scene = Scene(frame=(16, 9), background="#0f172a")
node = scene.geometry.circle(0.75).fill(BLUE).move_to(0, 0)
scene.play([node.animate.create().duration(1.0)])
scene.render()
```

Los demás son fragmentos: muestran solo las líneas relevantes y dan por hecho
una escena como la anterior.

¿Buscas un método concreto? Pulsa `Ctrl K` o `/` y escribe su nombre o la
tarea, en español o en inglés.

== Por dónde empezar

Si es tu primera vez, lee #link("/referencia/scene/")[Escena] y
#link("/referencia/drawable/")[Drawable]: todo lo demás se construye sobre
ellos. Para ver la API en escenas completas, consulta los
#link("/ejemplos/basicos/")[ejemplos].
