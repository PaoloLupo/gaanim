#import "../../components/section.typ": docs-chapter
#import "../../components/api.typ": api-entry

#show: docs-chapter.with(
  title: "Temas y colores",
  description: "Colores, pinceles, fondos, postprocesado, temas y mapas de color",
  route: "/referencia/themes/",
)

= Temas y colores

El aspecto de una escena sale de tres capas: el tema activo (colores
semánticos, tipografía, reglas por tipo de objeto y tokens de espaciado), el
fondo y los estilos que escribes en cada objeto, que siempre ganan.

```python
from gaanim import BLUE, GOLD, Scene

scene = Scene(frame=(16, 9), theme="technical")
circle = scene.geometry.circle(1).fill(BLUE).stroke(GOLD, 0.05)
label = scene.text("Escena con tema", role="title").move_to(0, 2.5)
scene.play([circle.animate.create(), label.animate.write()])
scene.render()
```

Toda escena empieza con el tema `technical`: `Scene()` equivale a
`Scene(theme="technical")`, `scene.canvas.theme` vale `"technical"` y el fondo
es gris casi negro (`#121212`), con texto y ejes claros y formas rellenas con
el color de acento. Otro tema (`Scene(theme=...)` o
`scene.canvas.set_theme(...)`) sustituye al predeterminado, un fondo explícito
(`Scene(background=...)`) gana al del tema y los colores que pones en cada
objeto ganan a los del tema.

Para un lienzo sin tema, pasa `Scene(theme=None)` o llama a
`scene.canvas.set_theme(None)`. Sin tema el fondo vuelve a ser blanco (salvo
que hayas elegido uno) y el texto, las formas y los ejes sin estilo propio son
blancos, así que dales color o usa un fondo que contraste; `gaanim check`
avisa cuando no se verían. Sin tema, `scene.canvas.color(...)` y
`scene.canvas.validate_theme()` lanzan `ValueError`, y los componentes como
`badge` o `card` usan los colores de `technical`.

```python
from gaanim import Scene

scene = Scene(frame=(16, 9), theme=None, background="white")
dot = scene.geometry.circle(1).fill("#2563eb")
label = scene.text("Sin tema", color="black").move_to(0, 2)
scene.render()
```

== Colores

Todo argumento `ColorLike` acepta un `Color`, una cadena CSS Color 4 (`"#0f172a"`,
`"tomato"`, `"oklch(62% 0.2 260)"`, `"rgb(37 99 235 / 65%)"`) o una tupla
`(r, g, b)` o `(r, g, b, a)` de enteros de 0 a 255. Una sintaxis inválida o un
componente fuera de rango lanzan `ValueError`.

#api-entry(
  name: "Color",
  kind: "class",
  signature: "Color(value: str) | Color(r: int, g: int, b: int, a: int = 255)",
  params: (
    (name: "value", type: "str", default: none, desc: [Cualquier color CSS Color 4.]),
    (name: "r, g, b, a", type: "int", default: "a = 255", desc: [Canales de 0 a 255.]),
  ),
  returns: (type: "Color", desc: [Un color RGBA inmutable.]),
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
navy = Color("#0f172a")
accent = Color("oklch(62.3% 0.214 259.815)")
translucent = Color("rgb(37 99 235 / 65%)")
purple = Color(128, 51, 204)
circle = scene.geometry.circle(1).fill(purple)
```
]

#api-entry(
  name: "Color.from_hex",
  kind: "factory",
  params: ((name: "value", type: "str", default: none, desc: [`"#rgb"`, `"#rrggbb"` o `"#rrggbbaa"`.]),),
  returns: (type: "Color"),
  none,
)

#api-entry(
  name: "Color.from_rgb",
  kind: "factory",
  params: ((name: "r, g, b", type: "int", default: none, desc: [Canales de 0 a 255.]),),
  returns: (type: "Color", desc: [Un color opaco.]),
  none,
)

#api-entry(
  name: "Color.from_rgba",
  kind: "factory",
  params: ((name: "r, g, b, a", type: "int", default: none, desc: [Canales de 0 a 255; `a = 0` es transparente.]),),
  returns: (type: "Color"),
  none,
)

#api-entry(
  name: "Color.from_hsl",
  kind: "factory",
  params: (
    (name: "h", type: "float", default: none, desc: [Tono en grados.]),
    (name: "s, l", type: "float", default: none, desc: [Saturación y luminosidad, de 0 a 1.]),
    (name: "a", type: "float", default: "1.0", desc: [Opacidad, de 0 a 1.]),
  ),
  returns: (type: "Color"),
  none,
)

#api-entry(
  name: "Color.from_oklch",
  kind: "factory",
  params: (
    (name: "l", type: "float", default: none, desc: [Luminosidad perceptual, de 0 a 1.]),
    (name: "c", type: "float", default: none, desc: [Croma.]),
    (name: "h", type: "float", default: none, desc: [Tono en grados.]),
    (name: "a", type: "float", default: "1.0", desc: [Opacidad, de 0 a 1.]),
  ),
  returns: (type: "Color", desc: [Un color definido en un espacio perceptual, útil para paletas con luminosidad uniforme.]),
)[
```python
>>>from gaanim import *
hexa = Color.from_hex("#ff8800")
rgb = Color.from_rgb(15, 23, 42)
glass = Color.from_rgba(255, 255, 255, 64)
computed = Color.from_hsl(215, 0.9, 0.55, 0.8)
perceptual = Color.from_oklch(0.68, 0.17, 240)
```
]

`colors.tailwind` contiene las 26 familias de Tailwind CSS v4.3.3, con escalas
de 50 a 950: `colors.tailwind.blue[500]` o `colors.tailwind["rose"][600]`.
`colors.tailwind.families()` las enumera y `colors.tailwind.version` da la
versión incluida.

```python
>>>from gaanim import *
from gaanim import colors

brand = colors.tailwind.blue[600]
warning = colors.tailwind["amber"][500]
print(len(colors.tailwind.families()), colors.tailwind.version)
```

=== Constantes de color

#table(
  columns: (auto, auto, auto),
  inset: 7pt,
  [*Nombre*], [*Valor*], [*Color*],
  [`WHITE`], [`#FFFFFF`], [Blanco],
  [`BLACK`], [`#000000`], [Negro],
  [`GRAY`], [`#808080`], [Gris],
  [`BLUE`], [`#193264`], [Azul oscuro],
  [`NAVY`], [`#1B1F3B`], [Azul marino],
  [`TEAL`], [`#2E86AB`], [Azul verdoso],
  [`CYAN`], [`#4BE5E5`], [Cian],
  [`GREEN`], [`#4BE57C`], [Verde],
  [`YELLOW`], [`#F5D04B`], [Amarillo],
  [`GOLD`], [`#FFD700`], [Dorado],
  [`ORANGE`], [`#FF9F43`], [Naranja],
  [`CORAL`], [`#FF6464`], [Coral],
  [`RED`], [`#E54B4B`], [Rojo],
  [`PINK`], [`#FF7AB6`], [Rosa],
  [`PURPLE`], [`#9B59B6`], [Morado],
)

== Pinceles y gradientes

`Drawable.fill(...)` y `Drawable.stroke(...)` aceptan un `ColorLike` o un
`Brush` reutilizable. Las coordenadas de un gradiente están en el espacio
local del objeto, así que la pintura acompaña sus transformaciones. Los colores
se reparten uniformemente y hacen falta al menos dos. `extend` decide qué pasa
fuera de la rampa: `"pad"` (predeterminado) prolonga los extremos, `"repeat"`
la repite y `"reflect"` la repite en espejo.

#api-entry(
  name: "Brush.solid",
  kind: "factory",
  params: ((name: "color", type: "ColorLike", default: none, desc: [Color del pincel.]),),
  returns: (type: "Brush", desc: [Un pincel de color uniforme.]),
  none,
)

#api-entry(
  name: "Brush.linear",
  kind: "factory",
  params: (
    (name: "colors", type: "Sequence[ColorLike]", default: none, desc: [Colores de la rampa, en orden.]),
    (name: "start, end", type: "tuple[float, float]", default: none, desc: [Extremos del gradiente, en unidades locales.]),
    (name: "extend", type: "\"pad\" | \"repeat\" | \"reflect\"", default: "\"pad\"", desc: [Comportamiento fuera de la rampa.]),
  ),
  returns: (type: "Brush", desc: [Un gradiente lineal.]),
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
gradient = Brush.linear(["#7AA2F7", "#BB9AF7", "#F7768E"], start=(-3, 0), end=(3, 0))
card = scene.geometry.rounded_rect(6, 2.75, 0.35).fill(gradient)
```
]

#api-entry(
  name: "Brush.radial",
  kind: "factory",
  params: (
    (name: "colors", type: "Sequence[ColorLike]", default: none, desc: [Colores del centro hacia fuera.]),
    (name: "center", type: "tuple[float, float]", default: "(0.0, 0.0)", desc: [Centro, en unidades locales.]),
    (name: "radius", type: "float", default: none, desc: [Radio del último color.]),
    (name: "extend", type: "\"pad\" | \"repeat\" | \"reflect\"", default: "\"pad\"", desc: [Comportamiento fuera de la rampa.]),
  ),
  returns: (type: "Brush", desc: [Un gradiente radial.]),
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9), theme="technical")
orb = scene.geometry.circle(1.5).fill(
    Brush.radial(["white", scene.canvas.color("accent"), "#0000"], center=(-0.375, 0.44), radius=1.875)
)
```
]

#api-entry(
  name: "Brush.sweep",
  kind: "factory",
  params: (
    (name: "colors", type: "Sequence[ColorLike]", default: none, desc: [Colores alrededor del centro.]),
    (name: "center", type: "tuple[float, float]", default: "(0.0, 0.0)", desc: [Centro, en unidades locales.]),
    (name: "start_angle, end_angle", type: "float", default: "0.0, 360.0", desc: [Ángulos del barrido, en grados.]),
    (name: "extend", type: "\"pad\" | \"repeat\" | \"reflect\"", default: "\"pad\"", desc: [Comportamiento fuera de la rampa.]),
  ),
  returns: (type: "Brush", desc: [Un gradiente angular.]),
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
ring = scene.geometry.circle(1.375).no_fill().stroke(
    Brush.sweep(["#7DCFFF", "#9ECE6A", "#E0AF68", "#F7768E", "#7DCFFF"], center=(0, 0)),
    0.25,
)
```
]

`animate.fill(paint)` y `animate.stroke(paint, width)` interpolan entre pinturas:
dos gradientes del mismo tipo interpolan su geometría y sus colores (con
números de paradas distintos, se normalizan sus posiciones), y un color sólido
puede convertirse en gradiente y al revés. Cambiar entre tipos incompatibles,
como lineal y radial, o entre modos de `extend` distintos, lanza un error al
programar la animación. Consulta
#link("/referencia/animations/#api-anim-fill")[`Anim.fill`].

`StrokeStyle` agrupa todo un trazo reutilizable; se aplica con
#link("/referencia/drawable/#api-drawable-stroke-style")[`Drawable.stroke_style`]
o dentro de un tema.

#api-entry(
  name: "StrokeStyle",
  kind: "class",
  signature: "StrokeStyle(paint: Paint, width: float = 0.02, *, cap: str = \"round\", join: str = \"round\", miter_limit: float = 4.0, dashes: Sequence[float] = (), dash_offset: float = 0.0)",
  params: (
    (name: "paint", type: "Paint", default: none, desc: [Color, `Brush` o nombre de un token del tema.]),
    (name: "width", type: "float", default: "0.02", desc: [Ancho en unidades lógicas.]),
    (name: "cap", type: "\"butt\" | \"round\" | \"square\"", default: "\"round\"", desc: [Remate de los extremos.]),
    (name: "join", type: "\"bevel\" | \"miter\" | \"round\"", default: "\"round\"", desc: [Unión entre segmentos.]),
    (name: "miter_limit", type: "float", default: "4.0", desc: [Límite de inglete.]),
    (name: "dashes", type: "Sequence[float]", default: "()", desc: [Longitudes alternas de trazo y hueco; vacío es continuo.]),
    (name: "dash_offset", type: "float", default: "0.0", desc: [Desplazamiento del patrón.]),
  ),
  returns: (type: "StrokeStyle"),
  desc: [Métricas inválidas lanzan `ValueError`.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
dashed = StrokeStyle(GOLD, 0.04, cap="butt", dashes=(0.2, 0.1))
guide = scene.geometry.line(-4, 0, 4, 0).stroke_style(dashed)
```
]

== Fondos

`Scene(background=...)`, `scene.canvas.background` y
`scene.segment(..., background=...)` aceptan un color, un `Brush` o un
`Background`. Sus coordenadas son las de la escena: con el marco de 16 × 9, un
gradiente lineal de ancho completo va de `x=-8` a `x=8`. El fondo explícito
tiene prioridad sobre el del tema.

#api-entry(
  name: "Canvas.background",
  kind: "property",
  signature: "background: BackgroundLike | None",
  returns: (type: "BackgroundLike | None", desc: [El fondo de la escena; se puede leer y asignar.]),
  desc: [Sin fondo explícito devuelve el del tema, o `None` si no hay tema.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
scene.canvas.background = Color(40, 42, 54)
scene.canvas.background = Brush.linear(["#071022", "#164E8A", "#7DD3FC"], start=(-8, 0), end=(8, 0))
```
]

#api-entry(
  name: "Background",
  kind: "class",
  signature: "Background(paint: Paint)",
  params: ((name: "paint", type: "Paint", default: none, desc: [Color o `Brush` que cubre toda la escena.]),),
  returns: (type: "Background"),
  none,
)

#api-entry(
  name: "Background.shader",
  kind: "factory",
  params: (
    (name: "source", type: "str | os.PathLike[str]", default: none, desc: [WGSL en línea, o la ruta de un asset `.wgsl` (con `pathlib.Path`), que se lee al crear el fondo.]),
    (name: "fallback", type: "ColorLike | None", default: "None", desc: [Color fuera del marco, para limpiar el fondo 3D, calcular el contraste automático y si falla la rasterización; negro si se omite.]),
  ),
  returns: (type: "Background", desc: [Un fondo procedural que sigue la línea de tiempo.]),
  desc: [WGSL inválido lanza `ValueError` y un asset ilegible, `RuntimeError`.],
)[
```python
from gaanim import Background, Scene

shader = Background.shader("""
fn gaanim_background(
    uv: vec2<f32>,
    resolution: vec2<f32>,
    time: f32,
) -> vec4<f32> {
    let center = vec2<f32>(0.35 + 0.1 * sin(time), 0.45);
    let glow = exp(-8.0 * distance(uv, center));
    return vec4<f32>(0.02, 0.08 + 0.4 * glow, 0.18 + 0.6 * glow, 1.0);
}
""", fallback="#071022")
scene = Scene(frame=(16, 9), background=shader)

from pathlib import Path
asset_shader = Background.shader(Path("assets/background.wgsl"), fallback="#071022")
```
]

`source` debe definir esta función:

```wgsl
fn gaanim_background(uv: vec2<f32>, resolution: vec2<f32>, time: f32) -> vec4<f32>
```

- `uv=(0, 0)` es la esquina superior izquierda y `uv=(1, 1)` la inferior
  derecha: `uv.y` crece hacia abajo, aunque la escena use `y` hacia arriba.
  Usa `1.0 - uv.y` para trabajar hacia arriba.
- `resolution` es el tamaño efectivo en píxeles.
- `time` es la posición absoluta de la línea de tiempo en segundos, así que
  las búsquedas, las capturas y la exportación son deterministas.

El shader cubre el mismo rectángulo de escena que muestra el editor. En el
visor se ejecuta en la misma GPU que dibuja la escena y solo se repite cuando
cambian el tiempo o el tamaño en píxeles. Las exportaciones y las capturas
producen los mismos píxeles.

#api-entry(
  name: "Background.fallback",
  kind: "property",
  returns: (type: "Color", desc: [El color representativo del fondo, usado para limpiar y para el contraste.]),
  none,
)

== Postprocesado

Un `PostProcess` aplica una función WGSL sobre todo lo que la escena dibuja en
2D: fondo, texto, formas, imágenes y Lottie. Se ejecuta en el editor, en el
modo presentación, en las capturas y en la exportación, siempre con el tiempo
exacto de la línea de tiempo.

#api-entry(
  name: "PostProcess.shader",
  kind: "factory",
  params: (
    (name: "source", type: "str | os.PathLike[str]", default: none, desc: [WGSL en línea, o la ruta de un asset `.wgsl` que se lee al crear el objeto.]),
  ),
  returns: (type: "PostProcess", desc: [Un postprocesado para `Scene(post=...)`, `scene.canvas.post` o `scene.segment(..., post=...)`.]),
  desc: [WGSL inválido lanza `ValueError` y un asset ilegible, `RuntimeError`.],
)[
```python
from gaanim import PostProcess, Scene

vignette = PostProcess.shader("""
fn gaanim_post(uv: vec2<f32>, resolution: vec2<f32>, time: f32) -> vec4<f32> {
    let color = gaanim_scene(uv);
    let edge = smoothstep(0.9, 0.35, distance(uv, vec2<f32>(0.5)));
    return vec4<f32>(color.rgb * edge, color.a);
}
""")
scene = Scene(frame=(16, 9), post=vignette)
```
]

`source` debe definir esta función:

```wgsl
fn gaanim_post(uv: vec2<f32>, resolution: vec2<f32>, time: f32) -> vec4<f32>
```

- `uv=(0, 0)` es la esquina superior izquierda del cuadro de la cámara y
  `uv=(1, 1)` la inferior derecha.
- `resolution` es el tamaño del cuadro en píxeles de salida.
- `time` es la posición absoluta de la línea de tiempo en segundos.
- `gaanim_scene(uv)` devuelve el color ya renderizado en ese punto: sRGB con
  alfa sin premultiplicar. Fuera del cuadro devuelve el píxel más cercano del
  borde.
- El resultado se limita a `[0, 1]`.

Solo se procesa el cuadro de la cámara. El shader recibe píxeles, así que un
radio fijo como `1.0 / 1920.0` cambia con la resolución de salida: para un
número de píxeles usa `offset / resolution` y, para que se vea igual a
cualquier resolución, expresa el radio como fracción de `uv`. Con una cámara
en perspectiva la escena se dibuja sin postprocesado, y tampoco se aplica a la
salida SVG.

#api-entry(
  name: "PostProcess.source",
  kind: "property",
  returns: (type: "str", desc: [El código WGSL de la función.]),
  none,
)

#api-entry(
  name: "Canvas.post",
  kind: "property",
  signature: "post: PostProcess | None",
  returns: (type: "PostProcess | None", desc: [El postprocesado de la escena; se puede leer y asignar.]),
  desc: [`None` lo quita. Un segmento puede cambiarlo mientras está activo: `post=otro` usa otro shader, `post=False` dibuja el segmento sin postprocesado y `post=None` (el predeterminado) hereda el de la escena.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
from pathlib import Path

vignette = PostProcess.shader(Path("assets/grade.wgsl"))
scene.canvas.post = vignette
scene.segment("intro")                 # hereda scene.canvas.post
scene.wait(0.5)
scene.segment("datos", post=False)     # sin postprocesado
scene.wait(0.5)
```
]

== Temas

Un tema reúne colores semánticos, tipografía, reglas de estilo por tipo de
objeto, paletas de datos y tokens de espaciado. Los componentes (tarjetas,
insignias, bandas, gráficas, tablas, paneles de código) toman de él sus
colores de primer plano, acento, panel y regla. Un `.fill(...)` explícito
siempre tiene prioridad.

#api-entry(
  name: "Canvas.set_theme",
  kind: "method",
  params: ((name: "theme", type: "ThemeName | Theme | None", default: none, desc: [Nombre o alias de un tema incluido, un `Theme`, o `None` para quitar el tema.]),),
  returns: (type: "None", desc: [Instala el tema en la escena, o lo quita.]),
  desc: [Equivale a `Scene(theme=...)` después de crear la escena y sustituye al tema `technical` predeterminado. El fondo del tema reemplaza al actual salvo que la escena tenga un fondo explícito. Las reglas se aplican al compilar, así que también alcanzan a los objetos creados antes de la llamada. Un nombre desconocido lanza `ValueError`.],
)[
```python
from gaanim import Scene

scene = Scene(frame=(16, 9))
scene.canvas.set_theme("presentation")
title = scene.text("Transformada de Fourier", role="title")
subtitle = scene.text("Representación en frecuencia", role="subtitle").move_to(0, -1)
scene.play([title.animate.write(), subtitle.animate.fade_in()])
scene.render()
```
]

#table(
  columns: (auto, auto, 1fr),
  inset: 7pt,
  [*Tema*], [*Alias*], [*Aspecto*],
  [`technical` (predeterminado)], [`scientific`], [Oscuro y sobrio, para matemáticas y documentación técnica: fondo `#121212`, texto `#E6E6E6`, datos en gris `#BDBDBD`, acento ámbar `#F2A541` y rejilla `#707070`.],
  [`presentation`], [`deck`], [Para proyectar: el mismo fondo gris casi negro que `technical` (`#121212`), títulos y acentos dorados, gráficos en coral `#F4845F` y cuerpo brillante.],
  [`paper`], [`light`], [Lienzo blanco con tinta oscura.],
  [`dracula`, `nord`, `tokyo-night`], [`tokyo`], [Paletas oscuras de editores conocidos.],
  [`solarized-dark`, `solarized-light`], [], [Solarized en sus dos variantes.],
  [`gruvbox-dark`], [`gruvbox`], [Gruvbox oscuro.],
  [`catppuccin-mocha`, `catppuccin-latte`], [`catppuccin`, `mocha`, `latte`], [Catppuccin oscuro y claro.],
)

Los nombres no distinguen mayúsculas. En el stub, `Scene(theme=...)`,
`Canvas.set_theme` y `Theme(base)` tipan el nombre como `ThemeName`, un
`Literal` con los once nombres y los ocho alias de la tabla, así que el editor
los autocompleta. `Theme.schemes()` devuelve solo los nombres canónicos.

#api-entry(
  name: "Canvas.theme",
  kind: "property",
  signature: "theme: str | None",
  returns: (type: "str | None", desc: [Nombre del tema activo: `"technical"` en una escena nueva, o `None` después de `Scene(theme=None)` o `set_theme(None)`.]),
  desc: [Es de solo lectura: para cambiarlo usa `set_theme`.],
  none,
)

#api-entry(
  name: "Theme.schemes",
  kind: "method",
  returns: (type: "list[str]", desc: [Los nombres canónicos de los temas incluidos, sin alias.]),
)[
```python
>>>from gaanim import *
print(Theme.schemes())
```
]

#api-entry(
  name: "Canvas.set_fonts",
  kind: "method",
  params: (
    (name: "font", type: "str | None", default: "None", desc: [Familia para títulos, cuerpo, subtítulos, encabezados, pies y etiquetas.]),
    (name: "math_font", type: "str | None", default: "None", desc: [Familia para ecuaciones y fragmentos matemáticos.]),
    (name: "code_font", type: "str | None", default: "None", desc: [Familia para texto con rol `code`.]),
  ),
  returns: (type: "None", desc: [Configura el lienzo; no crea objetos.]),
  desc: [Cambia solo las familias, sin crear un `Theme`. Lo omitido conserva su valor anterior. Las familias tienen prioridad sobre el tema activo y siguen vigentes si el tema cambia después; un `font` explícito en un objeto sigue ganando. Una familia vacía lanza `ValueError`.],
)[
```python
from gaanim import Scene

scene = Scene(frame=(16, 9), theme="technical")
scene.canvas.set_fonts(
    font="Inter",
    math_font="New Computer Modern Math",
    code_font="JetBrains Mono",
)
scene.text("Texto y $x^2$", role="body")
scene.text("fn main() {}", role="code").move_to(0, -1)
```
]

#api-entry(
  name: "Canvas.color",
  kind: "method",
  params: ((name: "role", type: "str", default: none, desc: [Rol o token de color del tema activo.]),),
  returns: (type: "Color", desc: [El color resuelto.]),
  desc: [Sirve para que tus propios objetos usen los colores del tema. Sin tema activo, o con un rol desconocido, lanza `ValueError`.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9), theme="technical")
panel = scene.geometry.rounded_rect(5.25, 2.25, 0.3) \
    .fill(scene.canvas.color("panel")) \
    .stroke(scene.canvas.color("accent"), 0.04)
```
]

#api-entry(
  name: "Canvas.layout_token",
  kind: "method",
  params: ((name: "name", type: "str", default: none, desc: [Nombre del token.]),),
  returns: (type: "float", desc: [El valor en unidades lógicas.]),
  desc: [Resuelve un espaciado del tema activo. Sin tema usa la escala predeterminada: `space_xs`, `space_sm`, `space_md`, `space_lg`, `page_padding`, `page_padding_wide`, `page_padding_x`, `column_gap`, `vertical_padding`, `vertical_padding_x` y `lower_third_offset`. Un nombre desconocido lanza `ValueError`.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9), theme="technical")
>>>title = scene.text("Título", role="title")
>>>body = scene.text("Cuerpo", role="body")
page = scene.layout.column(
    [title, body],
    padding=scene.canvas.layout_token("page_padding"),
    gap=scene.canvas.layout_token("space_lg"),
)
```
]

#api-entry(
  name: "Canvas.validate_theme",
  kind: "method",
  returns: (type: "list[str]", desc: [Advertencias; vacía si las combinaciones principales son legibles.]),
  desc: [Comprueba el contraste de primer plano, título, texto atenuado y panel sobre el fondo, y la tipografía. Es orientativa: no rechaza nada, porque un estado animado puede tener poco contraste a propósito. Sin tema activo lanza `ValueError`.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9), theme="technical")
warnings = scene.canvas.validate_theme()
if warnings:
    raise ValueError("\n".join(warnings))
```
]

=== Temas propios

#api-entry(
  name: "Theme",
  kind: "class",
  signature: "Theme(base: ThemeName | Theme | None = None, *, name=None, colors=None, fonts=None, sizes=None, text=None, styles=None, series=None, heatmap=None, layout=None, font_files=None, font_dir=None, text_markup=None)",
  params: (
    (name: "base", type: "ThemeName | Theme | None", default: "None", desc: [Tema del que se deriva; solo se sustituye lo que pasas. Sin él, se parte de la paleta de `technical` con el nombre `custom`.]),
    (name: "name", type: "str | None", default: "None", desc: [Nombre del tema nuevo.]),
    (name: "colors", type: "dict[str, ColorLike] | None", default: "None", desc: [Roles de color y tokens propios para las reglas.]),
    (name: "fonts", type: "dict[str, str] | None", default: "None", desc: [Familias por rol de fuente.]),
    (name: "sizes", type: "dict[str, float] | None", default: "None", desc: [Tamaños por rol de texto, en unidades lógicas.]),
    (name: "text", type: "dict[TextRole, TextStyle] | None", default: "None", desc: [`TextStyle` por rol, como capa sobre el rol semántico.]),
    (name: "styles", type: "dict[str, Style | AxesStyle] | None", default: "None", desc: [Reglas por selector (ver abajo).]),
    (name: "series", type: "Sequence[ColorLike] | None", default: "None", desc: [Colores categóricos de las gráficas.]),
    (name: "heatmap", type: "Sequence[ColorLike] | None", default: "None", desc: [Colores continuos de los mapas de calor.]),
    (name: "layout", type: "dict[str, float] | None", default: "None", desc: [Tokens de espaciado, nuevos o sustituidos.]),
    (name: "font_files", type: "dict[str, str] | None", default: "None", desc: [Familia → archivo de fuente.]),
    (name: "font_dir", type: "str | os.PathLike | None", default: "None", desc: [Carpeta cuyos `.ttf`, `.otf`, `.ttc` y `.otc` se registran (sin subcarpetas).]),
    (name: "text_markup", type: "bool | None", default: "None", desc: [Con `False`, `*` y `_` son literales por defecto en `scene.text`, `badge` y `chip`; `None` hereda el valor de la base.]),
  ),
  returns: (type: "Theme", desc: [Un tema reutilizable para `Scene(theme=...)` o `set_theme`.]),
  desc: [Los archivos de fuente se leen al crear el tema y se incorporan a la escena, así que la exportación no depende de que estén instalados. Con `font_dir`, cada cara se resuelve por la familia, el peso y el estilo que declara su archivo: con `fonts={"text": "Aleo"}`, `weight=700` encuentra la negrita. Selectores, tokens, roles o métricas inválidos, una carpeta sin fuentes o una fuente ilegible lanzan `ValueError`; un archivo o carpeta inexistente, `OSError`.],
)[
```python
from gaanim import AxesStyle, Scene, StrokeStyle, Style, TextStyle, Theme, colors

theme = Theme(
    "nord",
    name="mis-diapositivas",
    colors={"title": "#A3D9FF", "accent": "#FFB86C", "chart": "#88C0D0"},
    fonts={"text": "Aleo", "code": "Victor Mono"},
    sizes={"title": 0.72, "body": 0.34},
    text={
        "body": TextStyle(size=0.32, letter_spacing=0.1),
        "label": TextStyle(size=0.24, weight=600),
    },
    styles={
        "shape": Style(fill="accent"),
        "line": Style(stroke=StrokeStyle("foreground", 3, cap="round")),
        ".warning": Style(fill=colors.tailwind.rose[600]),
        "axes": AxesStyle(grid=StrokeStyle("rule", 1), labels=TextStyle(size=0.24)),
    },
    series=[colors.tailwind.blue[600], colors.tailwind.amber[500]],
    layout={"page_padding": 0.7, "column_gap": 0.6},
    font_files={
        "Aleo": "assets/fonts/Aleo-VariableFont.ttf",
        "Victor Mono": "assets/fonts/VictorMono-VariableFont.ttf",
    },
)
scene = Scene(frame=(16, 9), theme=theme)
```
]

Los roles de color incluidos son `background`, `foreground`, `muted`, `title`,
`accent`, `chart`, `panel`, `header`, `rule`, `success`, `warning` y `danger`;
`colors` también acepta tokens propios no vacíos para las reglas. Los roles de
fuente son `text`, `all`, `title`, `subtitle`, `heading`, `body`, `caption`,
`label`, `math` y `code`.

```python
>>>from gaanim import *
brand = Theme(
    name="marca",
    colors={
        "background": "#10131A", "foreground": "#F8FAFC", "muted": "#94A3B8",
        "title": "#FDE68A", "accent": "#38BDF8", "chart": "#22C55E",
        "panel": "#18202E", "header": "#202B3D", "rule": "#475569",
    },
    fonts={"text": "Aptos", "code": "Consolas"},
)
print_theme = Theme(brand, name="marca-impresa", colors={"background": "white", "foreground": "#172033"})
plain = Theme("paper", font_dir="assets/fonts", fonts={"text": "Aleo"}, text_markup=False)
scene = Scene(theme=plain)
scene.text("tb:dist_comp")          # literal, sin cursiva
scene.text("*Nota*", markup=True)   # negrita
```

#api-entry(
  name: "Theme.name",
  kind: "property",
  returns: (type: "str", desc: [Nombre del tema.]),
  none,
)

#api-entry(
  name: "Theme.text_markup",
  kind: "property",
  returns: (type: "bool", desc: [Modo de markup por defecto del texto creado con este tema.]),
  none,
)

#api-entry(
  name: "Theme.color",
  kind: "method",
  params: ((name: "role", type: "str", default: none, desc: [Rol o token de color.]),),
  returns: (type: "Color", desc: [El color del tema.]),
  desc: [Como `Canvas.color`, pero sobre un `Theme` que no tiene por qué estar activo.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
theme = Theme("nord")
divider = scene.geometry.line(-5, 0, 5, 0).stroke(theme.color("rule"), 0.025)
```
]

#api-entry(
  name: "Theme.layout_token",
  kind: "method",
  params: ((name: "name", type: "str", default: none, desc: [Nombre del token.]),),
  returns: (type: "float", desc: [El valor en unidades lógicas.]),
  desc: [Un token desconocido lanza `ValueError`.],
  none,
)

#api-entry(
  name: "Theme.validate",
  kind: "method",
  returns: (type: "list[str]", desc: [Advertencias de contraste y tipografía; vacía si todo es legible.]),
  none,
)

=== Reglas por selector

Las claves de `styles` seleccionan una familia (`shape`, `line`, `text`,
`axes`, `plot`), el nombre exacto de una fábrica (`circle`, `rounded_rect`,
`arrow`), una parte semántica como `axes/grid` o `axes/labels`, o una clase
propia como `.warning`, que se asigna con
#link("/referencia/drawable/#api-drawable-style-class")[`Drawable.style_class`].
La precedencia es: tema base, familia, tipo exacto o parte semántica, clases
en orden, valores del constructor y, por último, los setters del objeto. Las
reglas se aplican al compilar, así que cambiar de tema actualiza también los
objetos ya creados. La pintura de los recursos importados sigue la de su
archivo salvo que la cambies explícitamente.

#api-entry(
  name: "Style",
  kind: "class",
  signature: "Style(*, fill: Paint | None = None, stroke: StrokeStyle | None = None, opacity: float | None = None, text: TextStyle | None = None)",
  params: (
    (name: "fill", type: "Paint | None", default: "None", desc: [Relleno; una cadena puede nombrar un token del tema.]),
    (name: "stroke", type: "StrokeStyle | None", default: "None", desc: [Trazo completo.]),
    (name: "opacity", type: "float | None", default: "None", desc: [Opacidad.]),
    (name: "text", type: "TextStyle | None", default: "None", desc: [Tipografía.]),
  ),
  returns: (type: "Style", desc: [Una regla; lo que omites no cambia.]),
)[
```python
>>>from gaanim import *
theme = Theme(
    "paper",
    colors={"brand": "#2563eb", "danger": "oklch(58% .24 25)"},
    styles={"shape": Style(fill="brand"), ".danger": Style(fill="danger")},
)
scene = Scene(theme=theme)
ordinary = scene.geometry.circle(0.75)
warning = scene.geometry.square(1.25).style_class("danger")
explicit = scene.geometry.circle(0.5).fill("gold")
```
]

#api-entry(
  name: "AxesStyle",
  kind: "class",
  signature: "AxesStyle(*, axis=None, grid=None, minor_grid=None, ticks=None, numbers=None, labels=None)",
  params: (
    (name: "axis, grid, minor_grid, ticks", type: "StrokeStyle | None", default: "None", desc: [Trazos de los ejes, la cuadrícula, la cuadrícula secundaria y las marcas.]),
    (name: "numbers, labels", type: "TextStyle | None", default: "None", desc: [Tipografía de los números y de las etiquetas.]),
  ),
  returns: (type: "AxesStyle", desc: [Una regla para el selector `axes`.]),
  none,
)

== Mapas de color

`ColorMap` ofrece 39 mapas canónicos de Matplotlib y 39 de Scientific Colour
Maps, incluidos en Gaanim: no hace falta Matplotlib ni ningún archivo externo.
Los nombres no distinguen mayúsculas. Las posiciones se normalizan a `[0, 1]`;
los mapas continuos interpolan en sRGB y los categóricos conservan escalones.
Un nombre, categoría, posición, alfa o lista de colores inválidos lanzan
`ValueError`. Donde un parámetro acepta `ColorMapLike`, también vale el nombre
como cadena.

#api-entry(
  name: "ColorMap",
  kind: "class",
  signature: "ColorMap(name: str)",
  params: ((name: "name", type: "str", default: none, desc: [Nombre de un mapa incluido.]),),
  returns: (type: "ColorMap", desc: [Un mapa inmutable.]),
)[
```python
>>>from gaanim import *
viridis = ColorMap("viridis")
midpoint = viridis.sample(0.5)
swatches = viridis.colors(8)
reverse = viridis.reversed()
```
]

#api-entry(
  name: "ColorMap.named",
  kind: "factory",
  params: ((name: "name", type: "str", default: none, desc: [Nombre de un mapa incluido.]),),
  returns: (type: "ColorMap", desc: [Igual que `ColorMap(name)`.]),
  none,
)

#api-entry(
  name: "ColorMap.from_colors",
  kind: "factory",
  params: (
    (name: "colors", type: "Sequence[ColorLike]", default: none, desc: [Al menos dos colores.]),
    (name: "positions", type: "Sequence[float] | None", default: "None", desc: [Posiciones estrictamente crecientes entre 0 y 1; por defecto, uniformes.]),
  ),
  returns: (type: "ColorMap", desc: [Un mapa continuo propio, sin nombre ni categoría.]),
)[
```python
>>>from gaanim import *
custom = ColorMap.from_colors(["#0f172a", "#06b6d4", "#f8fafc"], positions=[0.0, 0.4, 1.0])
transparent = custom.with_alpha(0.65)
```
]

#api-entry(
  name: "ColorMap.names",
  kind: "method",
  params: ((name: "category", type: "\"matplotlib\" | \"scientific\" | None", default: "None", desc: [Filtra por catálogo.]),),
  returns: (type: "list[str]", desc: [Nombres de los mapas incluidos.]),
)[
```python
>>>from gaanim import *
print(ColorMap.names("scientific")[:5])
```
]

#api-entry(
  name: "ColorMap.name",
  kind: "property",
  returns: (type: "str | None", desc: [Nombre del mapa; `None` en uno propio.]),
  none,
)

#api-entry(
  name: "ColorMap.category",
  kind: "property",
  returns: (type: "str | None", desc: [`"matplotlib"`, `"scientific"` o `None`.]),
  none,
)

#api-entry(
  name: "ColorMap.categorical",
  kind: "property",
  returns: (type: "bool", desc: [`True` si el mapa es de escalones, como `tab10`.]),
  none,
)

#api-entry(
  name: "ColorMap.sample",
  kind: "method",
  params: ((name: "position", type: "float", default: none, desc: [Posición en `[0, 1]`.]),),
  returns: (type: "Color", desc: [El color en esa posición.]),
  none,
)

#api-entry(
  name: "ColorMap.colors",
  kind: "method",
  params: ((name: "count", type: "int", default: none, desc: [Número de colores.]),),
  returns: (type: "list[Color]", desc: [Colores repartidos uniformemente.]),
  none,
)

#api-entry(
  name: "ColorMap.reversed",
  kind: "method",
  returns: (type: "ColorMap", desc: [El mapa invertido.]),
  none,
)

#api-entry(
  name: "ColorMap.with_alpha",
  kind: "method",
  params: ((name: "alpha", type: "float", default: none, desc: [Opacidad en `[0, 1]`.]),),
  returns: (type: "ColorMap", desc: [El mapa con esa opacidad.]),
  none,
)

== Efectos visuales

`glow`, `blur` y `shadow` se aplican a cualquier objeto y funcionan sobre
rellenos y trazos, incluidos los gradientes; `no_effects()` los quita. Están
en #link("/referencia/drawable/#api-drawable-glow")[Drawable] y sus versiones
animadas en #link("/referencia/animations/#api-anim-glow")[Animaciones].
