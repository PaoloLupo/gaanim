#import "../../components/section.typ": docs-chapter

#show: docs-chapter.with(
  title: "Objetos, texto y estilos",
  description: "Cómo crear, agrupar y dar apariencia al contenido de una escena",
  route: "/manual/objetos/",
  updated: datetime.today().display(),
  code-langs: (),
)

= Objetos dibujables

Las primitivas, el texto, las imágenes y los grupos comparten operaciones de
`Drawable`: posición, escala, rotación, relleno, trazo, opacidad, clases de
tema y animaciones.

```python
orbit = scene.geometry.circle(140).stroke(BLUE, 4).no_fill()
point = scene.geometry.dot(12).fill(YELLOW).move_to(140, 0)
label = scene.text("r", role="body").fill(WHITE).move_to(70, 24)
system = scene.geometry.group([orbit, point, label])
```

Un grupo permite transformar varias piezas como una unidad sin perder sus
handles individuales.

== Texto y matemáticas

`scene.text` crea texto vectorial medible. El contenido entre delimitadores
matemáticos se compone con Typst:

```python
formula = scene.text("$x(t) = r cos(t)$", role="subtitle")
```

Los roles (`title`, `subtitle`, `body`, etc.) conectan el texto con el tema.
Para documentos complejos usa partes semánticas y selecciones, explicadas en
#link("/api/text/")[la referencia de Text].

== Estilo local y temas

El estilo local es ideal para una excepción. Un `Theme` es preferible cuando
varios objetos deben compartir colores, fuentes o medidas. Mantén separadas la
identidad visual y la geometría para poder adaptar la escena a otro formato.

== Recursos externos

Coloca imágenes, SVG, fuentes y modelos dentro de `assets/` y carga el
manifiesto con `scene.assets.load_project("gaanim.toml")`. Así las rutas no dependen
del directorio desde el que ejecutaste Gaanim.

== Siguiente paso

Continúa con #link("/manual/animaciones/")[Animaciones y tiempo].
