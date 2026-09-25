#import "../../components/section.typ": docs-chapter
#import "../../components/api.typ": api-entry

#show: docs-chapter.with(
  title: "Diapositivas",
  description: "Componentes editoriales de scene.slides: insignias, tarjetas, rótulos, listas, tablas e identidad de presentación",
  route: "/referencia/diapositivas/",
  nav: "Diapositivas",
)

= Diapositivas

`scene.slides` reúne componentes editoriales listos para presentaciones y
videos explicativos: insignias, tarjetas, rótulos, listas, tablas y la
identidad visual de cada segmento. Cada componente se mide a partir de su texto
y devuelve un grupo `Drawable` que se coloca y anima como cualquier objeto.

```python
# show-code: true
from gaanim import Scene

scene = Scene(frame=(16, 9), theme="presentation")
heading = scene.slides.section_header("Resultados", kicker="03").move_to(0, 2.6)
metric = scene.slides.stat_card("98%", "Precisión", delta="+4.2%", variant="success").move_to(-2.2, -0.4)
note = scene.slides.card("Conclusión", "El solver converge en 12 ms.", variant="accent").move_to(2.4, -0.4)
scene.play([heading.animate.fade_in(), metric.animate.grow_from_center(), note.animate.fade_in()], duration=1.0)
# output: preview.webp
scene.render()
```

Los componentes comparten opciones de estilo:

- `variant`: tono semántico del tema (`"neutral"`, `"accent"`, `"success"`,
  `"warning"` o `"danger"`).
- `appearance`: `"soft"` (fondo suave), `"solid"` (fondo lleno) u `"outline"`
  (solo borde).
- `color`, `background` y `border`: colores explícitos que sustituyen los del tema.
- Las medidas (`width`, `padding`, `gap`, `radius`…) están en unidades de escena.

Texto vacío, variantes desconocidas o medidas inválidas lanzan `ValueError`.

== Insignias y chips

Etiquetas cortas de estado, categoría o metadatos que se ajustan a su texto.

#api-entry(
  name: "SlideKit.badge",
  kind: "factory",
  params: ((name: "text", type: "str", default: none, desc: [Etiqueta no vacía.]), (name: "radius", type: "float | None", default: "None", desc: [`None` da forma de píldora a partir de la altura medida.]), (name: "min_width", type: "float | None", default: "None", desc: [Ancho mínimo del panel.]), (name: "font / weight", type: "str | None / int | None", default: "None", desc: [Familia y peso de la etiqueta; sustituyen a `style`.]), (name: "style", type: "TextStyle | None", default: "None", desc: [Tipografía sobre el rol `label` del tema; su color, si lo tiene, sustituye el de la variante.]), (name: "markup", type: "bool | None", default: "None", desc: [`False` deja `*` y `_` literales, como en `scene.text`; `None` sigue `text_markup` del tema.])),
  desc: [Insignia en el origen cuyo panel se mide con el mismo texto que se dibuja. Colócala con `.move_to(...)`.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
tag = scene.slides.badge("LISTO", variant="success", appearance="solid").move_to(-2.4, 1.2)
code = scene.slides.badge("_vel_max", font="DejaVu Sans Mono", weight=600, markup=False)
scene.play([tag.animate.grow_from_center()])
```
]

#api-entry(
  name: "SlideKit.chip",
  kind: "factory",
  desc: [Insignia compacta con un punto opcional del color de la variante (`dot=True`), para filtros, estados y metadatos. Acepta las mismas opciones tipográficas que `badge`.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
live = scene.slides.chip("En vivo", variant="danger", appearance="outline")
```
]

== Tarjetas

Paneles de altura automática para contenido, métricas y citas.

#api-entry(
  name: "SlideKit.card",
  kind: "factory",
  params: ((name: "title", type: "str", default: none, desc: [Encabezado.]), (name: "body / footer", type: "str | None", default: "None", desc: [Cuerpo con ajuste de línea y pie opcional.]), (name: "width / min_height", type: "float", default: "4.2 / 1.8", desc: [Ancho fijo y altura mínima.])),
  desc: [Tarjeta con título, cuerpo y pie. Cada hueco usa un rol de texto del tema y se mide al construirla.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
result = scene.slides.card("Resultado", "El solver converge.", "12 ms", variant="accent")
```
]

#api-entry(
  name: "SlideKit.stat_card",
  kind: "factory",
  desc: [Tarjeta de métrica con valor, etiqueta y variación opcional (`delta`). El valor y la variación usan el tono de la variante; no se infiere signo ni formato numérico.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
metric = scene.slides.stat_card("98%", "Precisión", delta="+4.2%", variant="success")
```
]

#api-entry(
  name: "SlideKit.quote_card",
  kind: "factory",
  desc: [Cita con comillas tipográficas y atribución opcional alineada a la derecha.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
quote = scene.slides.quote_card("La claridad importa.", "Gaanim", appearance="outline")
```
]

== Rótulos y encabezados

Portadas, cabeceras de sección y rótulos anclados a los bordes seguros.

#api-entry(
  name: "SlideKit.title_card",
  kind: "factory",
  desc: [Portada centrada con título, regla y subtítulo opcional, animable como un solo grupo. `panel=True` la enmarca; `accent` colorea la regla.],
)[
```python
# show-code: true
from gaanim import Scene
scene = Scene(frame=(16, 9), background="#0f172a")
opening = scene.slides.title_card("Movimiento vectorial", "Una explicación técnica", panel=True)
scene.play([opening.animate.fade_in().duration(0.7)])
# output: preview.webp
scene.render()
```
]

#api-entry(
  name: "SlideKit.section_header",
  kind: "factory",
  desc: [Encabezado de sección con antetítulo (`kicker`) y subtítulo opcionales. `align` (`left`, `center` o `right`) alinea todos los huecos; `rule=True` añade una regla de acento horizontal.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
heading = scene.slides.section_header("Método", kicker="02", align="center")
```
]

#api-entry(
  name: "SlideKit.banner",
  kind: "factory",
  desc: [Franja anclada al borde seguro superior o inferior (`position`). Con `width=None` ocupa el ancho seguro menos `margin`; su altura sigue al título y al subtítulo.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
notice = scene.slides.banner("Simulación completa", position="bottom", variant="success")
```
]

#api-entry(
  name: "SlideKit.lower_third",
  kind: "factory",
  desc: [Rótulo inferior anclado a una esquina segura (`side="left"` o `"right"`), con antetítulo, título y subtítulo que usan los roles del tema y se ajustan al ancho.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
speaker = scene.slides.lower_third("Ada Lovelace", "Matemática", kicker="PONENTE")
```
]

#api-entry(
  name: "SlideKit.callout",
  kind: "factory",
  desc: [Llamada con tarjeta, texto y conector que apunta a `target` desde `offset` y lo sigue de forma nativa, sin callbacks de Python por fotograma.],
)[
```python
# show-code: true
from gaanim import GOLD, Scene
scene = Scene(frame=(16, 9), background="#0f172a")
mass = scene.geometry.dot(0.15).fill(GOLD).move_to(-0.5, 0)
note = scene.slides.callout("Masa en movimiento", mass, offset=(1.625, 0.875))
scene.play([mass.animate.shift_by(1, 0).duration(1.0), note.animate.fade_in().duration(0.4)])
# output: preview.webp
scene.render()
```
]

== Listas y tablas

Agendas con viñetas y tablas técnicas compactas.

#api-entry(
  name: "SlideKit.bullets",
  kind: "factory",
  desc: [Lista con viñetas como un solo objeto. Ajusta `width`, `gap`, `bullet_radius` y `bullet_color`.],
)[
```python
# show-code: true
from gaanim import GOLD, Scene
scene = Scene(frame=(16, 9), background="#0f172a")
agenda = scene.slides.bullets(["Preparar", "Animar", "Exportar"], gap=0.6, bullet_color=GOLD).move_to(0, 0.5)
scene.play([agenda.animate.fade_in().duration(0.6)])
# output: preview.webp
scene.render()
```
]

#api-entry(
  name: "SlideKit.table",
  kind: "factory",
  desc: [Tabla con cabecera y reglas. Cada fila debe tener tantas celdas como cabeceras. Para tablas con celdas combinadas usa `scene.text.typst(...)` (ver #link("/referencia/text/")[Texto]).],
)[
```python
# show-code: true
from gaanim import Scene
scene = Scene(frame=(16, 9), background="#0f172a")
tbl = scene.slides.table(["Método", "Error", "Tiempo"], [["Base", "0.18", "48 ms"], ["GPU", "0.04", "15 ms"]]).move_to(0, 0)
scene.play([tbl.animate.fade_in().duration(0.6)])
# output: preview.webp
scene.render()
```
]

== Identidad de la presentación

Logo, pie, regla y número de diapositiva comunes a todos los segmentos.

#api-entry(
  name: "SlideKit.brand",
  kind: "method",
  params: ((name: "logo", type: "str | None", default: "None", desc: [SVG o imagen; se ajusta a 0.6 unidades de alto en la esquina segura superior derecha.]), (name: "footer", type: "str | None", default: "None", desc: [Texto del pie.]), (name: "slide_numbers / rule", type: "bool", default: "True / True", desc: [Número de diapositiva y regla del pie.]), (name: "show_on_cover", type: "bool", default: "False", desc: [Muestra la identidad también en el primer segmento.]), (name: "logo_scale", type: "float", default: "1.0", desc: [Multiplica el tamaño del logo.])),
  desc: [Configura la identidad que se dibuja en cada segmento, por encima del contenido. Llámalo una vez, al principio. Consulta #link("/guias/presentaciones/")[Presentaciones].],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9), theme="presentation")
scene.slides.brand(logo="assets/logo.svg", footer="LAB · 2026")
scene.segment("Portada")
scene.wait(0.5)
scene.segment("Contenido")
scene.wait(0.5)
```
]
