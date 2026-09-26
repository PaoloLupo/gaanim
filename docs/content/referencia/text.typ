#import "../../components/section.typ": docs-chapter
#import "../../components/api.typ": api-entry

#show: docs-chapter.with(
  title: "Texto",
  description: "scene.text: prosa, ecuaciones, partes semánticas, estilo, flujo, selecciones, documentos Typst y medición",
  route: "/referencia/text/",
  nav: "Texto",
)

= Texto

`scene.text(...)` crea prosa, títulos, párrafos, matemáticas y contenido mixto;
`scene.text.equation(...)` es su atajo para ecuaciones en bloque. Ambas
devuelven un `Text`: un `Drawable` vectorial que conserva la estructura
semántica, se mide con el mismo motor que #link("/referencia/layout/")[Layout]
y permite seleccionar y animar partes sueltas.

```python
# show-code: true
from gaanim import GOLD, Scene, TextFlow, part
scene = Scene(frame=(16, 9), background="#0f172a")
formula = part("formula", "$E = ", part("mass", "m", color=GOLD), " c^2$")
copy = scene.text(
    "La energía es ", formula,
    role="body",
    flow=TextFlow(wrap=5, align="center", line_spacing=1.2),
).move_to(0, 0)
scene.play([copy.animate.write(by="part").duration(1.0)])
# output: text_factory.webp
scene.render()
```

Cada pieza tiene una responsabilidad:

- `TextStyle` controla el aspecto de los glifos y las métricas tipográficas.
- `TextFlow` controla cómo se componen las líneas dentro del texto.
- Layout controla la caja exterior: relleno, ajuste, crecimiento y posición.
- `TextSelection` señala glifos dentro de un `Text`; nunca es un hijo
  independiente de Layout.

== Crear texto

Las dos fábricas de texto y los roles tipográficos del tema.

#api-entry(
  name: "Scene.text",
  kind: "factory",
  signature: "scene.text(*content, role=None, style=None, flow=None, font=None, math_font=None, size=None, weight=None, italic=None, color=None, opacity=None, letter_spacing=None, word_spacing=None, baseline=None, wrap=None, text_align=None, line_spacing=None, max_lines=None, overflow=None, direction=None, hyphenate=None, lang=None, markup=None) -> Text",
  params: (
    (name: "content", type: "str | TextPart | TextParts", default: none, desc: [Una o varias cadenas, partes semánticas o grupos de partes. El resultado no puede estar vacío.]),
    (name: "role", type: "str | None", default: "None", desc: [Rol tipográfico. Si todo es matemático se infiere `math`; si no, `body`.]),
    (name: "style / flow", type: "TextStyle | None / TextFlow | None", default: "None", desc: [Estilo y flujo reutilizables.]),
    (name: "font … baseline", type: "argumentos con nombre", default: "None", desc: [Ajustes directos de estilo: fuente, métricas, color, opacidad, espaciado y línea base. Sustituyen a `style`.]),
    (name: "wrap … lang", type: "argumentos con nombre", default: "None", desc: [Ajustes directos de flujo: ajuste de línea, alineación, límite de líneas, desbordamiento, dirección, guiones e idioma. Sustituyen a `flow`.]),
    (name: "markup", type: "bool | None", default: "None", desc: [Interpreta `*negrita*` y `_cursiva_`. `False` deja `*` y `_` literales (la matemática `$…$` sigue activa); `None` usa `text_markup` del tema, que por defecto es `True`.]),
  ),
  desc: [`$…$` activa la matemática y `\$` escribe un dólar literal. `color` acepta un `Color`, una cadena CSS/hex o una tupla RGB(A); `None` hereda el color del estilo o del tema. `lang="es"` con `hyphenate=True` da párrafos justificados con guiones en español. Contenido, delimitadores, roles, métricas o flujos inválidos lanzan `TypeError` o `ValueError`.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
title = scene.text("Movimiento circular", role="title")
note = scene.text("Radio $r = 1.5$", size=0.36, color="#94a3b8").move_to(0, -1)
```
]

#api-entry(
  name: "Typography.equation",
  kind: "factory",
  params: (
    (name: "content", type: "str | TextPart | TextParts", default: none, desc: [Ecuación sin los delimitadores `$`.]),
    (name: "opciones", type: "igual que scene.text", default: "None", desc: [Comparte todo el estilo y el flujo de `scene.text` (salvo `lang` y `markup`).]),
  ),
  desc: [Envuelve el contenido como `$ … $`: esos espacios hacen que Typst componga una ecuación en bloque. Cada frontera entre piezas es un espacio normal de Typst, así que Typst decide el espaciado de operadores e identificadores. Sin rol ni tamaño explícitos usa el tamaño matemático de 0.44 unidades. Contenido vacío lanza `ValueError`.],
)[
```python
# show-code: true
from gaanim import GOLD, Scene, part, parts
scene = Scene(frame=(16, 9), background="#0f172a")
equation = scene.text.equation(
    part("sum_force", "sum F_t"),
    "=",
    parts(mass="m", acceleration="a_t"),
).move_to(0, 0)
equation["acceleration"].fill(GOLD)
scene.play([equation.animate.write(by="part").duration(1.0)])
# output: equation_factory.webp
scene.render()
```
]

=== Roles

Los roles son `title`, `subtitle`, `kicker`, `heading`, `body`, `caption`,
`label`, `code` y `math`. Sus tamaños predeterminados, en unidades de escena,
son: `title` 0.64, `subtitle` 0.48, `kicker` 0.32, `heading` 0.48, `body` 0.40,
`caption` 0.32, `label` 0.36, `code` 0.36 y `math` 0.44. La prosa usa New
Computer Modern, el código Consolas y la matemática New Computer Modern Math.
Con un tema, `kicker` toma el color `accent`, ideal para la línea corta sobre un
título:

```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
kicker = scene.text("MISMO TERREMOTO. TRES EDIFICIOS.", role="kicker").move_to(0, 3.77)
title = scene.text("¿Cuál sufrirá más?", role="title").move_to(0, 3)
```

El estilo se resuelve en este orden, de menor a mayor prioridad: rol y tema,
`TextStyle`/`TextFlow`, argumentos directos de `scene.text`, estilo local de cada
`part` y, por último, `selection.fill(...)` o `text.fill(...)` posteriores.

== Partes semánticas y matemáticas

Nombra fragmentos del contenido para colorearlos, animarlos o transformarlos
después sin contar caracteres.

#api-entry(
  name: "part",
  kind: "function",
  signature: "part(name, *content, style=None, font=None, math_font=None, size=None, weight=None, italic=None, color=None, opacity=None, letter_spacing=None, word_spacing=None, baseline=None) -> TextPart",
  params: (
    (name: "name", type: "str", default: none, desc: [Nombre no vacío y único entre sus hermanos.]),
    (name: "content", type: "str | TextPart | TextParts", default: none, desc: [Contenido anidado.]),
    (name: "style / estilo directo", type: "TextStyle | argumentos con nombre", default: "None", desc: [Tipografía local que hereda todo el subárbol.]),
  ),
  desc: [Subárbol semántico inmutable. La ruta de nombres anidados es estable y la usan las selecciones y las transiciones de texto. Nombres repetidos o vacíos, métricas inválidas o contenido anidado inválido lanzan `ValueError` o `TypeError`.],
)[
```python
# show-code: true
from gaanim import BLUE, GOLD, Scene, part
scene = Scene(frame=(16, 9), background="#0f172a")
formula = part(
    "formula",
    "$",
    part("variable", "x", color=BLUE),
    " dot 5 = ",
    part("result", "25", color=GOLD),
    "$",
)
text = scene.text("Resultado: ", formula).move_to(0, 0)
scene.play([text.animate.write(by="part", stagger=0.05).duration(1.1)])
# output: text_parts.webp
scene.render()
```
]

#api-entry(
  name: "parts",
  kind: "function",
  signature: "parts(mapping=None, /, **content: str) -> TextParts",
  params: (
    (name: "mapping", type: "Mapping[str, str] | None", default: "None", desc: [Nombres y texto en orden. Admite nombres que no son identificadores de Python, como `"tb:dist"`.]),
    (name: "content", type: "str con nombre", default: none, desc: [Atajo para nombres que son identificadores: `parts(mass="m")` equivale a `parts({"mass": "m"})`.]),
  ),
  desc: [Grupo ordenado de partes simples, equivalente a varias llamadas a `part()`. Dentro de `$…$`, las entradas contiguas son tokens distintos de Typst y conservan su espaciado ajustado. Usa `part()` para estilos locales o anidamiento. Entradas vacías, repetidas o mezclar un mapa con argumentos con nombre lanzan `ValueError`; nombres o valores que no son cadenas, `TypeError`.],
)[
```python
# show-code: true
from gaanim import GOLD, Scene, parts
scene = Scene(frame=(16, 9), background="#0f172a")
equation = scene.text.equation(
    "-",
    parts(mass_left="m", gravity="g sin(theta)"),
    "=",
    parts(mass_right="m", length="L", acceleration="theta''"),
).move_to(0, 0)
scene.play([equation.animate.write(by="part").duration(1.2)])
scene.play([equation["gravity"].animate.indicate().duration(0.6)])
scene.play([equation["acceleration"].animate.fill(GOLD).duration(0.6)])
# output: compact_text_parts.webp
scene.render()
```

Un mapa admite nombres que los argumentos con nombre no pueden expresar:

```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
label = scene.text(parts({"tb:dist": "d = ", "x-1": "4.2 m"}))
label["tb:dist"].fill(GOLD)
```
]

=== Énfasis en línea

Las cadenas admiten un marcado mínimo inspirado en Typst: `*negrita*`,
`_cursiva_` y ambos anidados, `*_así_*`. Se compila en las mismas pasadas que
`TextStyle`, así que funciona con ajuste de línea, selecciones, animaciones,
partes y medición.

```python
# show-code: true
from gaanim import GOLD, Scene

scene = Scene(frame=(16, 9), background="#0f172a")
copy = scene.text(
    "Normal, _cursiva_, *negrita* y *_ambas_*.",
    size=0.36,
).move_to(0, 0)
scene.play([copy.animate.write(by="word", stagger=0.05).duration(1.2)])
scene.play([copy.words[2].animate.indicate().duration(0.6)])
# output: text_inline_markup.webp
scene.render()
```

- `\\*` y `\\_` escriben los delimitadores literales.
- El marcado puede cruzar cadenas contiguas y fronteras de `part()` sin añadir
  huecos.
- Dentro de `$…$` los marcadores son sintaxis matemática: `x_1` es un subíndice
  y `*` una multiplicación.
- Los guiones bajos dentro de palabras (`snake_case`), los marcadores repetidos
  (`__init__`) y una expresión con espacios como `5 * 4` quedan literales.
- Un delimitador de apertura sin cierre, o un anidamiento cruzado, lanza
  `ValueError`.
- Para contenido técnico lleno de `_` o `*` (`tb:dist_comp`, `X1_2`) pasa
  `markup=False`: todo queda literal, barras invertidas incluidas, y `$…$`
  sigue siendo matemática. `text.become(...)` conserva el modo salvo que pases
  `markup=`.
- `Theme(text_markup=False)` lo hace predeterminado para `scene.text`,
  `scene.text.measure`, `badge` y `chip`.

```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
label = scene.text("Valores: V_e del piso 1 (tb:agriet_xy)", markup=False)
```

=== Delimitadores matemáticos

- `$…$` cambia el compositor Typst a matemáticas; `$$…$$` usa el mismo
  compositor y no crea un objeto distinto.
- `scene.text.equation(...)` añade `$ … $`: no escribas los delimitadores.
- `\$` escribe un dólar literal y un delimitador sin pareja lanza `ValueError`.
- Si todo el contenido es matemático, el rol inferido es `math`; si mezcla prosa
  y matemáticas, `body`.
- Dentro de la matemática, cada frontera entre piezas es un espacio normal de
  Typst, así que no hace falta escribir `"= "`. Fuera de la matemática las
  fronteras son exactas.
- Los estilos locales de una parte quedan dentro de la misma ecuación Typst:
  cambiar su color, fuente o tamaño nunca añade huecos artificiales.
- La sintaxis puede cruzar fronteras: `part("x", "x"), "_1"` se compila como
  `x _1` y Typst mantiene `_1` como subíndice.

== Estilo y flujo

Aspecto de los glifos y composición de las líneas, como valores reutilizables.

#api-entry(
  name: "TextStyle",
  kind: "class",
  signature: "TextStyle(*, font=None, math_font=None, fallbacks=(), size=None, weight=None, italic=None, color=None, stroke=None, stroke_width=None, opacity=None, letter_spacing=None, word_spacing=None, decorations=(), baseline=None)",
  params: (
    (name: "font / math_font", type: "str | None", default: "None", desc: [Familias de prosa y de matemáticas.]),
    (name: "fallbacks", type: "Sequence[str]", default: "()", desc: [Familias de respaldo en orden.]),
    (name: "size", type: "float | None", default: "None", desc: [Tamaño positivo en unidades de escena.]),
    (name: "weight", type: "int | None", default: "None", desc: [Peso numérico de 1 a 1000.]),
    (name: "color / stroke / stroke_width", type: "Color | None / Color | None / float | None", default: "None", desc: [Relleno de los glifos y contorno opcional.]),
    (name: "opacity", type: "float | None", default: "None", desc: [Opacidad de todo el texto, de 0 a 1.]),
    (name: "letter_spacing / word_spacing", type: "float | None", default: "None", desc: [Espaciado adicional no negativo, en unidades de escena.]),
    (name: "decorations", type: "Sequence[str]", default: "()", desc: [`underline`, `strike` o `strikethrough`.]),
    (name: "baseline", type: "float | None", default: "None", desc: [Desplazamiento de la línea base; positivo sube los glifos.]),
  ),
  desc: [Estilo tipográfico inmutable y reutilizable. No tiene ancho, alto, relleno ni ajuste de caja: eso es de Layout. `stroke`, `stroke_width` y `opacity` del estilo raíz afectan a todo el texto; en las partes anidadas se resuelven fuente, tamaño, peso, cursiva, color, espaciado, decoraciones y línea base. Valores inválidos lanzan `ValueError`.],
)[
```python
>>>from gaanim import *
display = TextStyle(
    font="New Computer Modern",
    math_font="New Computer Modern Math",
    size=0.42,
    weight=650,
    color=GOLD,
    letter_spacing=0.02,
    decorations=("underline",),
)
>>>scene = Scene(frame=(16, 9))
heading = scene.text("Resultados", style=display)
```
]

#api-entry(
  name: "TextFlow",
  kind: "class",
  signature: "TextFlow(*, wrap=\"auto\", align=\"left\", line_spacing=1.2, max_lines=None, overflow=\"clip\", direction=\"auto\", hyphenate=False, lang=None)",
  params: (
    (name: "wrap", type: "\"auto\" | False | float", default: "\"auto\"", desc: [`"auto"` usa el ancho que ofrece Layout (o el área segura); `False` mantiene una línea salvo saltos explícitos; un número limita el ancho tipográfico.]),
    (name: "align", type: "str", default: "\"left\"", desc: [`left`, `center`, `right` o `justify`.]),
    (name: "line_spacing", type: "float", default: "1.2", desc: [Multiplicador positivo de la altura de línea.]),
    (name: "max_lines / overflow", type: "int | None / str", default: "None / \"clip\"", desc: [Límite de líneas y qué pasa después: `visible`, `clip` o `ellipsis` (hoy se recorta igual que `clip`, sin dibujar la elipsis).]),
    (name: "direction", type: "str", default: "\"auto\"", desc: [`auto`, `ltr` o `rtl`.]),
    (name: "hyphenate / lang", type: "bool / str | None", default: "False / None", desc: [Guiones de Typst y código ISO 639 en minúsculas (`"es"`, `"en"`…) que elige los patrones; `None` usa inglés.]),
  ),
  desc: [Opciones inmutables de composición de líneas. Los argumentos directos de `scene.text` sustituyen a este objeto.],
)[
```python
# show-code: true
from gaanim import GOLD, Scene, TextFlow
scene = Scene(frame=(16, 9), background="#0f172a", margin=0.375)
body = scene.text(
    "El mismo texto se mide con el ancho que ofrece su tarjeta de Layout v2.",
    role="body",
    flow=TextFlow(wrap="auto", align="justify", line_spacing=1.25),
)
page = scene.layout.column(
    [scene.text("Texto responsive", role="heading").fill(GOLD), body],
    within="safe", width="fill", height="fill", padding=0.35, gap=0.225,
)
scene.play([page.animate.fade_in().duration(0.7)])
# output: text_flow.webp
scene.render()
```
]

Sin `flow` ni `text_align`, el anclaje de `move_to` también alinea un texto con
saltos de línea explícitos: `Anchor.*_LEFT` y `TextAnchor.BASELINE_LEFT` alinean
a la izquierda, `*_RIGHT` a la derecha y los anclajes centrados centran. Un
`flow` o `text_align` explícitos siempre ganan; el texto de una sola línea
conserva la alineación izquierda.

```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
note = scene.text("Fuente: ensayo 3\nEscala 1:50").move_to(7.5, -4, Anchor.BOTTOM_RIGHT)
```

== Posición

Colocación por línea base para que palabras y ecuaciones se alineen.

#api-entry(
  name: "Text.move_to",
  kind: "method",
  signature: "move_to(x, y, anchor: Anchor | TextAnchor | None = None) | move_to(reference) | move_to(point) -> Text",
  params: (
    (name: "x / y", type: "float | fuente reactiva", default: none, desc: [Punto de destino en unidades de escena.]),
    (name: "anchor", type: "Anchor | TextAnchor | None", default: "None", desc: [Anclaje geométrico, o `TextAnchor.BASELINE_LEFT`, `BASELINE_CENTER` o `BASELINE_RIGHT` sobre la línea base.]),
  ),
  desc: [Una sola línea se coloca por defecto con `TextAnchor.BASELINE_CENTER`: el centro horizontal visual en `x` y la línea base en `y`. Así, textos y ecuaciones con distintas ascendentes, fracciones o tamaños comparten línea base. Un bloque de varias líneas sin anclaje usa su centro visual, y un `TextAnchor` explícito usa la primera línea. Los anclajes geométricos (`Anchor.TOP_LEFT`…) se basan en los límites y no garantizan una línea base común. Un `Drawable` o un `AnchorPoint` centran el texto sobre la referencia sin seguirla. Un texto gestionado por Layout lanza `LayoutOwnershipError`.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
word = scene.text("Tipografía").move_to(0, 1)
equation = scene.text.equation("frac(x_1^2, y_2) = 1").move_to(0, -0.5, TextAnchor.BASELINE_CENTER)
left = scene.text("alineado a la izquierda").move_to(-4, -2, TextAnchor.BASELINE_LEFT)
corner = scene.text("esquina").move_to(-7, 4, Anchor.TOP_LEFT)
```
]

`Text` hereda todos los métodos de #link("/referencia/drawable/")[`Drawable`] y
conserva su tipo al encadenarlos.

#api-entry(
  name: "Text.glow / blur / shadow / no_effects",
  kind: "method",
  signature: "glow(color, radius=0.16, intensity=1.0) · blur(sigma=0.04) · shadow(color, x=0.08, y=-0.08, blur=0.06) · no_effects() -> Text",
  desc: [Los efectos de `Drawable`, redefinidos para devolver el `Text` y conservar el anclaje tipográfico de un `move_to` posterior.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
title = scene.text("Gaanim", role="title").glow(BLUE).move_to(0, 1, TextAnchor.BASELINE_CENTER)
```
]

== Selecciones

Señalan grafemas, palabras, líneas o partes de un texto para darles estilo o
animarlas sin separarlas del texto.

#api-entry(
  name: "TextSelection",
  kind: "class",
  signature: "text[name] · text[index] · text[start:end] · selection[name] -> TextSelection",
  params: (
    (name: "name", type: "str", default: none, desc: [Una parte de primer nivel; sigue indexando para bajar por partes anidadas. Una cadena que no es parte selecciona cada aparición literal de ese texto, sin distinguir mayúsculas ni espacios.]),
    (name: "index / slice", type: "int | slice", default: none, desc: [Grafemas renderizados; admite índices negativos y rangos contiguos con paso 1.]),
  ),
  desc: [Selección diferida dentro de su `Text`. Un rango de grafemas, palabras o líneas selecciona el texto desde la primera hasta la última unidad, puntuación incluida. Cadenas que no son parte ni texto lanzan `KeyError`; índices fuera de rango, `IndexError`.],
)[
```python
# show-code: true
from gaanim import BLUE, GOLD, Scene, part
scene = Scene(frame=(16, 9), background="#0f172a")
copy = scene.text(
    "La ", part("concept", "energía"), " depende de ",
    part("formula", "$", part("mass", "m"), " c^2$"),
).move_to(0, 0)
copy["concept"].fill(GOLD)
copy["formula"]["mass"].fill(BLUE)
scene.play([copy.animate.write(by="word").duration(1.0)])
scene.play(copy.words[1].animate.pulse().duration(0.6))
scene.play(copy["formula"]["mass"].animate.focus().duration(0.6))
# output: text_selection.webp
scene.render()
```
]

#api-entry(
  name: "Text.graphemes / words / lines / parts",
  kind: "property",
  signature: "graphemes · words · lines · parts -> TextQuery",
  desc: [Vistas indexables (`TextQuery`) sobre las unidades del texto. `words` sigue los límites de palabra de Unicode, así que la puntuación no es una palabra: `"uno dos, tres"` tiene tres y `words[1:3]` selecciona `dos, tres`. `lines` sigue los saltos `\n` explícitos, no las líneas creadas por el ajuste. `parts` recorre el árbol semántico en profundidad. `len(query)` cuenta las unidades y `"mass" in text.parts` comprueba si existe una parte, también con rutas anidadas como `"formula.mass"`.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
copy = scene.text("uno dos, tres")
tail = copy.words[1:3]
if "mass" in copy.parts:
    copy["mass"].fill(GOLD)
```
]

#api-entry(
  name: "TextSelection.fill",
  kind: "method",
  desc: [Colorea los glifos seleccionados de forma persistente. En matemáticas, la parte sigue en la misma ecuación Typst: cambiar su color no añade espacios ni mueve los términos vecinos.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
formula = scene.text.equation("E = ", part("mass", "m"), " c^2")
formula["mass"].fill(GOLD)
```
]

#api-entry(
  name: "TextSelection.marker",
  kind: "method",
  desc: [Coloca al instante bandas de rotulador detrás de cada línea seleccionada, con los mismos argumentos que `animate.marker`. Aparecen en el cursor (el inicio de la escena durante la declaración) y no siguen movimientos posteriores del texto.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
quote = scene.text("Lo que no se mide no se puede mejorar")
quote.words[0:2].marker(opacity=0.35)
```
]

#api-entry(
  name: "TextSelection.animate / TextSelectionAnimation.fill / opacity",
  kind: "property",
  desc: [Proxy `TextSelectionAnimation` limitado a los glifos seleccionados. Acepta `fill` y `opacity`, combinables (`animate.fill(RED).opacity(0.6)`); los destinos de transformación, escala, rotación, material o trazo lanzan `TypeError`. Cada método devuelve un `Anim` para `scene.play`.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
eq = scene.text("$E = ", part("mass", "m"), " c^2$")
scene.play([eq.animate.write().duration(0.8), eq["mass"].animate.fill(RED).opacity(0.7).duration(0.8)])
```
]

#api-entry(
  name: "TextSelectionAnimation.indicate / pulse / wiggle / wave / highlight / focus",
  kind: "method",
  signature: "indicate() · pulse() · wiggle() · wave() · highlight() · focus() -> Anim",
  desc: [Énfasis transitorios sobre la selección. No cambian la medición del texto ni provocan reflow.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
eq = scene.text.equation("F = ", part("mass", "m"), " a")
scene.play([eq["mass"].animate.highlight().duration(0.8)])
scene.play([eq["mass"].animate.wave().duration(0.8)])
```
]

#api-entry(
  name: "TextSelectionAnimation.cancel",
  kind: "method",
  desc: [Tacha la selección con una marca diagonal y atenúa sus glifos. La siguiente transición de texto que reemplace el contenido retira ambos.],
  none,
)

#api-entry(
  name: "TextSelectionAnimation.reveal",
  kind: "method",
  desc: [Hace aparecer solo los glifos seleccionados con `"fade"`, `"wipe"` (traza y luego rellena) o `"from_below"` (fundido con una subida corta), para que un término entre en una ecuación ya visible. Un estilo desconocido lanza `ValueError`. Para revelar un texto completo por líneas o palabras, usa `Anim.reveal` (ver #link("/referencia/animations/")[Animaciones]).],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
eq = scene.text.equation("a^2 + b^2 = ", part("rhs", "c^2"))
scene.play([eq["rhs"].animate.reveal("from_below").duration(0.6)])
```
]

#api-entry(
  name: "TextSelectionAnimation.brace",
  kind: "method",
  desc: [Dibuja una llave bajo la selección (sobre ella con `above=True`) y hace aparecer `label`. La llave y la etiqueta son objetos nuevos, del color de los glifos, que permanecen en pantalla.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
eq = scene.text.equation("E = ", part("mass", "m"), " c^2")
scene.play([eq["mass"].animate.brace("masa")])
```
]

#api-entry(
  name: "TextSelectionAnimation.annotate",
  kind: "method",
  desc: [Coloca `label` a `offset` del centro de la selección con una línea guía. La línea y la etiqueta permanecen; un desplazamiento no finito lanza `ValueError`.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
eq = scene.text.equation("E = m ", part("c", "c"), "^2")
scene.play([eq["c"].animate.annotate("velocidad de la luz", offset=(0, 0.8))])
```
]

#api-entry(
  name: "TextSelectionAnimation.marker",
  kind: "method",
  params: (
    (name: "color", type: "Color | None", default: "None", desc: [Color del rotulador; por defecto un amarillo cuyo alfa se multiplica por `opacity`.]),
    (name: "skew", type: "float", default: "0.05", desc: [Inclinación de cada banda en radianes (positivo sube hacia la derecha), limitada para cubrir líneas largas.]),
    (name: "blend", type: "str", default: "\"normal\"", desc: [`"normal"` pinta la banda encima de lo que tiene debajo; `"multiply"` la multiplica con el fondo o la tarjeta, como tinta sobre papel (sobre negro no se ve). En ambos casos va detrás de los glifos.]),
    (name: "opacity", type: "float", default: "0.45", desc: [De 0 a 1.]),
    (name: "padding", type: "float | None", default: "None", desc: [Margen en unidades del mundo; `None` usa el 10 % de la altura de línea.]),
  ),
  desc: [Pasa un rotulador detrás de cada línea que abarca la selección. Cada banda cubre la altura de los glifos de su línea más `padding` y crece desde la izquierda; las líneas se suceden y se reparten la duración según su longitud. Las bandas quedan detrás de los glifos pero delante de lo creado antes del texto, y permanecen. Un `blend` desconocido, un `skew` no finito, una opacidad fuera de `[0, 1]` o un `padding` negativo lanzan `ValueError`.],
)[
```python
# show-code: true
from gaanim import Scene
scene = Scene(frame=(16, 9), background="#f8f5ee")
quote = scene.text("Lo que no se mide no se puede mejorar", color="#1f2937").move_to(0, 0)
scene.play([quote.words[4:9].animate.marker(skew=0.04).duration(1.2)])
# output: text_marker.webp
scene.render()
```
]

#api-entry(
  name: "TextSelectionAnimation.morph_to / copy_to",
  kind: "method",
  signature: "morph_to(target: TextSelection) · copy_to(target: TextSelection) -> Anim",
  desc: [Transiciones locales entre partes: `morph_to` sustituye la selección por el destino y `copy_to` conserva el origen mientras una copia viaja hasta el destino.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
energy = scene.text("$E = ", part("mass", "m"), " c^2$").move_to(0, 1)
momentum = scene.text("$p = ", part("mass", "m"), " v$").move_to(0, -1)
scene.play([energy["mass"].animate.copy_to(momentum["mass"]).duration(0.8)])
```
]

Las animaciones de selección resuelven el código matemático escrito a los
glifos que emitió Typst. Primero se busca el fragmento literal; si no aparece,
el analizador matemático de Typst convierte nombres de símbolo (`theta`, `sum`,
`arrow.r.long`), atajos (`<=`) y primas (`theta''`) al mismo Unicode que la
ecuación renderizada.

== Animación de texto completo

La escritura por grupos, los revelados con máscara, el desenfoque de entrada, el
tracking, la máquina de escribir y el efecto de decodificación están en
#link("/referencia/animations/")[Animaciones] (`Anim.write`, `Anim.reveal`,
`Anim.conceal`, `Anim.blur_in`, `Anim.tracking`, `Anim.typewriter`,
`Anim.scramble`). Aquí están el animador de rango, que los generaliza, y el
tracking inmediato.

#api-entry(
  name: "Text.animator",
  kind: "method",
  params: (
    (name: "by", type: "str", default: "\"grapheme\"", desc: [Unidad: `grapheme`, `word`, `line` (explícita) o `part`. La puntuación se une a su vecina.]),
    (name: "shape", type: "str", default: "\"smooth\"", desc: [Perfil del selector. `square`, `ramp`, `smooth`, `ease_in` y `ease_out` llevan cada unidad del estado «fuera» al reposo (revelado); `triangle` y `round` suben al estado «fuera» y vuelven (ola).]),
    (name: "order", type: "str", default: "\"forward\"", desc: [Qué unidad alcanza primero el rango: `forward`, `reverse`, `center` o `random`.]),
    (name: "seed", type: "int", default: "0", desc: [Fija la permutación de `order="random"`.]),
  ),
  desc: [Selector de rango por glifo al estilo de los _Text Animators_ de After Effects. Rust evalúa la influencia de cada unidad a partir de su índice y la forma del selector, así que un seek coincide con la reproducción y no se llama a Python por fotograma. `reveal`, `conceal` y `blur_in` son preajustes de este animador. Nombres inválidos lanzan `ValueError`; sobre un objeto que no es `Text`, `TypeError`.],
)[
```python
from gaanim import Scene, parallel

scene = Scene(frame=(16, 9), background="#0f172a")
title = scene.text("Tipografía cinética", role="title").move_to(0, 0)
wave = title.animator(by="grapheme", shape="smooth", order="forward", seed=0)
wave.set(offset=(0, -0.4), opacity=0.0, scale=0.6, rotation=0.2)  # estado "fuera"
scene.play([wave.animate.sweep().duration(1.2)])  # recorre el rango 0 → 1

bob = title.animator(by="word", shape="round").set(offset=(0, 0.3))
scene.play(parallel(bob.animate.sweep().duration(1.0)))  # una ola que pasa
scene.render()
```
]

#api-entry(
  name: "TextAnimator.set / animate",
  kind: "method",
  params: (
    (name: "offset", type: "(float, float) | None", default: "None", desc: [Desplazamiento en unidades de escena.]),
    (name: "opacity", type: "float | None", default: "None", desc: [Opacidad absoluta de 0 a 1.]),
    (name: "scale / rotation", type: "float | None", default: "None", desc: [Alrededor del centro de cada unidad; la rotación en radianes, menos de media vuelta.]),
    (name: "blur / tracking", type: "float | None", default: "None", desc: [Sigma gaussiana y espacio extra entre glifos, en unidades de escena.]),
    (name: "color", type: "Color | None", default: "None", desc: [Relleno sólido.])),
  desc: [Define el estado «fuera» (influencia 1) y devuelve el animador. Los valores omitidos conservan lo definido antes; los canales nunca definidos quedan en reposo. Cada barrido captura el estado al llamar a `sweep()`. Valores inválidos lanzan `ValueError`.],
  none,
)

#api-entry(
  name: "TextAnimatorAnimation.sweep",
  kind: "method",
  desc: [Accesible como `animator.animate.sweep(...)`. Devuelve un `Anim` que mueve el rango de `start` a `end`: `0` está antes de la primera unidad y `1` después de la última, así que `sweep(1, 0)` lo recorre hacia atrás. `stagger` es el retraso en segundos entre unidades (`None` lo adapta) y `easing` suaviza cada unidad. Funciona en `scene.play`, `parallel`, `sequence` y `stagger`; los barridos de entrada mantienen su primer fotograma hasta empezar.],
  none,
)

#api-entry(
  name: "Text.tracking",
  kind: "method",
  desc: [Fija al instante `value` unidades de escena extra entre glifos vecinos, sin recomponer el párrafo: cada fila crece desde su borde izquierdo, su centro o su borde derecho según la alineación. `0` restaura el espaciado. Anímalo con `animate.tracking(value)`.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
title = scene.text("Tipografía", role="title").tracking(0.4)
scene.play([title.animate.tracking(0.0).duration(1.2)])
```
]

=== Transiciones estructurales

`text.animate.transform_to(target)` (ver `Anim.transform_to` en
#link("/referencia/animations/")[Animaciones]) empareja las partes semánticas de
dos textos: los términos compartidos viajan a su nuevo sitio y el resto aparece
o desaparece. Cada glifo conserva su color, también los de `part()`, y al
terminar queda visible el destino. La transición usa la misma duración para el
texto y para el reflow de su Layout. Un destino de otra escena lanza
`ValueError`. Entre textos gestionados por Layouts distintos (por ejemplo,
celdas de dos matrices) la transformación es un morph de forma única: el origen
conserva su identidad y adopta el contorno, el color y la línea base del
destino.

```python
# show-code: true
from gaanim import GOLD, Scene, part
scene = Scene(frame=(16, 9), background="#0f172a")
before = scene.text("$x + ", part("obsolete", "3"), " = 7$").move_to(0, 0)
after = scene.text("$x = ", part("result", "4", color=GOLD), "$").move_to(0, 0)
scene.play([before.animate.write().duration(0.8)])
scene.play([before["obsolete"].animate.cancel().duration(0.5)])
scene.play([before.animate.transform_to(after).duration(0.8)])
# output: text_transition.webp
scene.render()
```

== Cambiar el contenido

Sustituye el contenido de un texto sin crear otro objeto.

#api-entry(
  name: "Text.become",
  kind: "method",
  desc: [Reemplaza el contenido conservando la identidad del `Text`, incrementa su versión estructural y pide reflow a su Layout. Es una llamada inmediata de autoría, no un `Anim`: no la pongas en `scene.play`. `markup=None` conserva el modo de marcado actual. Contenido o delimitadores inválidos lanzan `ValueError`.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
copy = scene.text("Calculando…")
scene.wait(0.5)
copy.become("Resultado: ", part("value", "$42$", color=GOLD))
```
]

== Documentos Typst, código y medición

Para lo que no cabe en un `Text`: documentos Typst completos, bloques de código y
medidas antes de crear nada.

#api-entry(
  name: "Typography.typst",
  kind: "factory",
  params: (
    (name: "source", type: "str | os.PathLike", default: none, desc: [Marcado Typst en línea o la ruta de un archivo `.typ` (relativa a la carpeta de assets).]),
    (name: "width", type: "str | float | int | None", default: "None", desc: [Ancho de página Typst antes de escalar: `"16cm"`, `"800pt"` o un número en puntos.]),
  ),
  desc: [Compila un documento Typst arbitrario (tablas con celdas combinadas, estructuras matemáticas propias, paquetes `@preview/…`) como un `Drawable`. Conserva sus proporciones y se escala para que su texto de 11 pt mida lo mismo que el rol `body`. No ofrece las selecciones de `Text`. Una fuente vacía lanza `ValueError` y un archivo ilegible, `RuntimeError`.],
)[
```python
# show-code: true
from gaanim import Scene
scene = Scene(frame=(16, 9), background="#0f172a")
tbl = scene.text.typst('#table(columns: 2, [*Método*], [*Error*], [Base], [0.18], [GPU], [0.04])')
scene.play([tbl.animate.fade_in().duration(0.6)])
# output: preview.webp
scene.render()
```

También carga archivos:

```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
from pathlib import Path
title = scene.text.typst(Path("assets/title.typ"))
```
]

#api-entry(
  name: "Typography.code",
  kind: "factory",
  desc: [Bloque de código monoespaciado dentro de un marco de `width` × `height`, con `font_size`, `background`, `color` y `accent` opcionales. `language` queda registrado para el resaltado futuro: hoy no se colorean tokens.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
snippet = scene.text.code("result = mass * acceleration", language="python", width=6, height=1.2)
scene.play([snippet.animate.fade_in().duration(0.5)])
```
]

#api-entry(
  name: "Typography.measure",
  kind: "method",
  params: (
    (name: "content", type: "str", default: none, desc: [Texto no vacío.]),
    (name: "role", type: "str | None", default: "None", desc: [Rol cuyos valores del tema resuelven tamaño, familia y color (`body` si se omite).]),
    (name: "size / font / color / weight", type: "—", default: "None", desc: [Ajustes explícitos, resueltos igual que en `scene.text`.]),
    (name: "wrap", type: "float | None", default: "None", desc: [Ancho fijo de composición; `None` mide un bloque sin ajuste.]),
    (name: "style / flow / line_spacing / markup", type: "—", default: "None", desc: [Como en `scene.text`. El `wrap="auto"` de un `flow` mide sin ajuste porque no hay ancho ofrecido.]),
  ),
  desc: [Devuelve `(ancho, alto)` en unidades de escena sin crear el texto. Usa el mismo proceso Typst que `scene.text` y comparte su caché. Úsalo para dimensionar cajas al contenido en lugar de adivinar.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
width, height = scene.text.measure("PGA = 0.35 g", role="label")
_, paragraph_height = scene.text.measure("Primera línea\nSegunda línea", line_spacing=1.6)
box = scene.geometry.rounded_rect(width + 0.56, height + 0.32, 0.14).move_to(0, -4.14)
```
]

== Integración con Layout

Un `Text` libre y el mismo `Text` dentro de una fila, columna, grid o stack usan
el mismo medidor. `wrap="auto"` lo hace sensible al ancho ofrecido; un ajuste
numérico queda además limitado por ese ancho. Un texto que cabe conserva sus
líneas naturales, y uno que se ajusta mantiene las líneas con las que se midió:
un contenedor que abraza su contenido (`"hug"`) nunca lo reparte en más líneas. Los cambios de métrica, `become` y
`animate.transform_to` invalidan la medición y piden reflow; los énfasis
transitorios (`indicate`, `pulse`, `wiggle`, `wave`, `highlight`, `focus`) no.

Layout es dueño de la traslación: un `Text` gestionado rechaza `move_to`,
`shift_by`, `next_to` y las animaciones de posición con `LayoutOwnershipError`.
Configura su `scene.layout.item(...)` o su contenedor. Los efectos visuales y
las transformaciones no posicionales siguen disponibles. Consulta
#link("/referencia/layout/")[Layout].

== Errores y casos límite

- `TypeError`: contenido que no es `str` ni `TextPart`, o claves de consulta del
  tipo equivocado.
- `ValueError`: contenido vacío, matemáticas desequilibradas, partes hermanas
  repetidas, rol, estilo o flujo inválidos, agrupación u orden inválidos, rangos
  con paso distinto de 1.
- `KeyError`: una cadena que no es parte semántica ni texto del `Text`.
- `IndexError`: índice fuera de rango o rango vacío.
- `LayoutOwnershipError`: posicionar a mano un texto gestionado, o una
  transición entre escenas o propietarios incompatibles.
