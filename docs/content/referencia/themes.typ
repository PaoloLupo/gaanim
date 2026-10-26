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
avisa cuando no se verían. Sin tema, `scene.colors`, `scene.canvas.color(...)` y
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

Para anotar tus propias funciones, importa esos tipos desde `gaanim`:
`ColorLike`; `Paint`, que también acepta un `Brush`; `ColorMapLike`, y
`BackgroundLike`, que también acepta un `Background`. Funcionan en el
comprobador de tipos y al ejecutar la escena. El resto de alias de los stubs
(`Padding`, `TextAlign`, `Endpoint`…) se importa desde `gaanim.gaanim_core`.

```python
from gaanim import Paint, Scene

def disc(scene: Scene, paint: Paint):
    return scene.geometry.circle(1).fill(paint)

scene = Scene(frame=(16, 9))
disc(scene, "#2563eb").move_to(-2, 0)
disc(scene, (220, 38, 38)).move_to(2, 0)
scene.render()
```

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

Un grupo pasa el pincel a cada miembro, que lo pinta en sus propias unidades
locales: un gradiente no cruza el grupo, sino que se repite en cada pieza. Para
que un solo gradiente recorra varias formas, únelas con
`scene.geometry.union(...)` o da a cada una un pincel desplazado según su
posición.

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
    Brush.radial(["white", scene.colors.accent, "#0000"], center=(-0.375, 0.44), radius=1.875)
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
  signature: "StrokeStyle(paint: Paint, width: float = 0.02, *, cap: str = \"round\", join: str = \"round\", miter_limit: float = 4.0, dashes: Sequence[float] = (), dash_offset: float = 0.0, align: str | None = None)",
  params: (
    (name: "paint", type: "Paint", default: none, desc: [Color, `Brush` o nombre de un token del tema.]),
    (name: "width", type: "float", default: "0.02", desc: [Ancho en unidades lógicas.]),
    (name: "cap", type: "\"butt\" | \"round\" | \"square\"", default: "\"round\"", desc: [Remate de los extremos.]),
    (name: "join", type: "\"bevel\" | \"miter\" | \"round\"", default: "\"round\"", desc: [Unión entre segmentos.]),
    (name: "miter_limit", type: "float", default: "4.0", desc: [Límite de inglete.]),
    (name: "dashes", type: "Sequence[float]", default: "()", desc: [Longitudes alternas de trazo y hueco; vacío es continuo. Un trazo de longitud cero con remate `"round"` o `"square"` dibuja un punto o un cuadrado.]),
    (name: "dash_offset", type: "float", default: "0.0", desc: [Desplazamiento del patrón.]),
    (name: "align", type: "\"inside\" | \"center\" | \"outside\" | None", default: "None", desc: [Posición del trazo en los contornos cerrados, como en `Drawable.stroke`; `None` conserva la del objeto.]),
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
  desc: [El fondo se ajusta al cuadro de la escena (16 × 9 por defecto) y se mueve con la cámara como el resto del mundo. Si la cámara se desplaza o se aleja más allá del cuadro, el fondo sigue llenando la vista: sus bordes se prolongan, así que un degradado o un shader continúan con el color de su borde en lugar de dejar una franja vacía.],
  none,
)

#api-entry(
  name: "Background.shader",
  kind: "factory",
  params: (
    (name: "source", type: "str | os.PathLike[str]", default: none, desc: [WGSL en línea, o la ruta de un asset `.wgsl` (con `pathlib.Path`), que se lee al crear el fondo. Una ruta relativa se busca en el directorio de trabajo, luego en la carpeta `assets` del script y luego junto a él.]),
    (name: "fallback", type: "ColorLike | None", default: "None", desc: [Color fuera del marco, para calcular el contraste automático y si falla la rasterización; negro si se omite.]),
    (name: "uniforms", type: "Mapping[str, float | Parameter | Computed] | None", default: "None", desc: [Campos `f32` de `gaanim_uniforms`, como en `PostProcess.shader`: números fijos o un `Parameter`, `Variable`, `Computed`, `scene.time` o una señal de audio, evaluados en cada fotograma, así que el fondo reacciona a la música. Como máximo 32, con nombres que sean identificadores WGSL.]),
    (name: "audio", type: "Audio | None", default: "None", desc: [Da al fondo el espectrograma de la pista con `gaanim_audio_spectrum(x, ago)` y las demás funciones `gaanim_audio_*` (ver #link("/referencia/audio/")[Audio]); reserva el uniform `gaanim_audio_time`.]),
  ),
  returns: (type: "Background", desc: [Un fondo procedural que sigue la línea de tiempo.]),
  desc: [WGSL inválido, un nombre de uniform inválido o reservado lanzan `ValueError` y un asset ilegible, `RuntimeError`. Como el `Parameter` o la pista se crean con la escena, un fondo con uniforms se asigna después: `scene.canvas.background = Background.shader(...)`. Los paquetes `.gaanim` guardan los valores de cada fotograma.],
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
- `gaanim_frame_size(resolution)` devuelve el tamaño del marco en unidades de
  la escena (por ejemplo `(16, 9)`), para dibujar en las mismas unidades que
  las formas.

El shader cubre el mismo rectángulo de escena que muestra el editor. En el
visor se ejecuta en la misma GPU que dibuja la escena y solo se repite cuando
cambian el tiempo o el tamaño en píxeles. Las exportaciones y las capturas
producen los mismos píxeles.

#api-entry(
  name: "Background.image",
  kind: "factory",
  signature: "Background.image(path) -> Background",
  desc: [Cubre el cuadro de la escena con la imagen de `path`, como una textura de papel: centrada y escalada hasta cubrirlo, recortando lo que no cabe, como `background-size: cover` en CSS. Su color promedio es el de respaldo (el borrado de la ventana y el contraste del texto). Una ruta relativa se busca en el directorio de trabajo, luego en la carpeta `assets` del script y luego junto a él; la imagen se lee al crear el fondo y un archivo ilegible lanza `RuntimeError`.],
  none,
)

#api-entry(
  name: "Background.fallback",
  kind: "property",
  returns: (type: "Color", desc: [El color representativo del fondo, usado para limpiar y para el contraste.]),
  none,
)

=== Fondos vivos

Fondos procedurales listos para usar, construidos sobre `Background.shader`:
se mueven con el tiempo de la línea de tiempo y la misma semilla da siempre la
misma imagen. Los colores se mezclan en luz lineal. Un valor inválido lanza
`ValueError`.

#api-entry(
  name: "Background.mesh_gradient",
  kind: "factory",
  signature: "mesh_gradient(colors, *, speed=0.1, seed=0) -> Background",
  desc: [Manchas suaves de 2 a 8 `colors` que derivan por órbitas y se mezclan como un _mesh gradient_. `speed` escala el movimiento (`0` lo congela) y `seed` elige otra disposición.],
)[
```python
# show-code: true
from gaanim import WHITE, Background, Scene
scene = Scene(frame=(16, 9))
scene.canvas.background = Background.mesh_gradient(
    ["#0b1040", "#6a2cc8", "#12b8a6", "#f25f8b"], speed=0.3, seed=4
)
scene.text("Mesh gradient", role="title").fill(WHITE)
scene.wait(2.0)
# output: preview.webp
scene.render()
```
]

#api-entry(
  name: "Background.noise_gradient",
  kind: "factory",
  signature: "noise_gradient(colors, *, scale=1.5, speed=0.05, seed=0) -> Background",
  desc: [Ruido fBm con dominio deformado, al estilo de los gradientes de Stripe, recorriendo una rampa de 2 a 8 `colors`. `scale` es el número de rasgos por altura del cuadro.],
  none,
)

#api-entry(
  name: "Background.aurora",
  kind: "factory",
  signature: "aurora(colors, *, sky=None, speed=0.2, seed=0) -> Background",
  desc: [Cortinas de aurora boreal, una por color (1 a 8), sobre un cielo `sky` oscuro (azul noche si se omite).],
  none,
)

#api-entry(
  name: "Background.dot_grid",
  kind: "factory",
  signature: "dot_grid(spacing=0.4, *, radius=0.03, color=None, background=None, drift=(0.0, 0.0)) -> Background",
  desc: [Puntos cada `spacing` unidades de la escena, con uno en el origen. `radius` está en unidades y es como mucho la mitad de `spacing`; `color` (blanco translúcido por defecto, respeta su alfa) sobre `background` oscuro. `drift` desplaza la rejilla en unidades por segundo.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
scene.canvas.background = Background.dot_grid(0.5, radius=0.05, color="#8fa3ff80", drift=(0.25, 0.0))
```
]

== Postprocesado

Un `PostProcess` aplica una función WGSL sobre todo lo que la escena dibuja en
2D: fondo, texto, formas, imágenes y Lottie. Se ejecuta en el editor, en el
modo presentación, en las capturas y en la exportación, siempre con el tiempo
exacto de la línea de tiempo.

#api-entry(
  name: "PostProcess.shader",
  kind: "factory",
  params: (
    (name: "source", type: "str | os.PathLike[str]", default: none, desc: [WGSL en línea, o la ruta de un asset `.wgsl` que se lee al crear el objeto. Una ruta relativa se busca en el directorio de trabajo, luego en la carpeta `assets` del script y luego junto a él.]),
    (name: "uniforms", type: "Mapping[str, float | Parameter | Computed] | None", default: "None", desc: [Valores que el shader lee como campos `f32` de `gaanim_uniforms` (`gaanim_uniforms.amount`). Un `Parameter`, `Variable`, `Computed` o `scene.time` se evalúa en cada fotograma, así que animarlo anima el efecto. Como máximo 32 por pasada, con nombres que sean identificadores WGSL.]),
    (name: "audio", type: "Audio | None", default: "None", desc: [Da al shader el espectrograma de la pista: `gaanim_audio_spectrum(x, ago)`, `gaanim_audio_level(ago)`, `gaanim_audio_band(low, high, ago)` y `gaanim_audio_x(hz)`. Reserva el uniform `gaanim_audio_time`. Ver #link("/referencia/audio/")[Audio].]),
  ),
  returns: (type: "PostProcess", desc: [Un postprocesado para `Scene(post=...)`, `scene.canvas.post` o `scene.segment(..., post=...)`.]),
  desc: [WGSL inválido, un campo sin declarar, un nombre inválido o un número no finito lanzan `ValueError`, y un asset ilegible, `RuntimeError`.],
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
cualquier resolución, expresa el radio como fracción de `uv`. También se
aplica con una cámara en perspectiva, pero no a la salida SVG.

#api-entry(
  name: "PostProcess.source / uniforms",
  kind: "property",
  signature: "source: str · uniforms: list[str]",
  returns: (type: "str · list[str]", desc: [El código WGSL de la función y los nombres de sus uniforms, en orden.]),
  none,
)

=== Uniforms animables y cadenas

Un uniform enlazado a un `Parameter` se anima como cualquier valor, y el
efecto se ve igual en el editor y en la exportación. Varias pasadas se
encadenan con una lista: cada una lee con `gaanim_scene` lo que escribió la
anterior.

```python
# show-code: true
from gaanim import GOLD, PostProcess, Scene
scene = Scene(frame=(16, 9), background="#0b1020")
amount = scene.viz.parameter(0.0)
split = PostProcess.shader("""
fn gaanim_post(uv: vec2<f32>, resolution: vec2<f32>, time: f32) -> vec4<f32> {
    let shift = vec2<f32>(gaanim_uniforms.amount * 0.02, 0.0);
    let base = gaanim_scene(uv);
    return vec4<f32>(gaanim_scene(uv + shift).r, base.g, gaanim_scene(uv - shift).b, base.a);
}
""", uniforms={"amount": amount})
scene.canvas.post = [split, PostProcess.vignette(0.5)]
ring = scene.geometry.circle(2).no_fill().stroke(GOLD, 0.2)
scene.play([amount.animate.set(1.0).duration(0.5).repeat(2, yoyo=True)])
# output: preview.webp
scene.render()
```

=== Presets de acabado

Pasadas listas para usar, sin escribir WGSL. Sus valores aceptan un número o
un `Parameter` para animarlos. Los tamaños se miden en píxeles de un cuadro de
1080 de alto, así que se ven igual a cualquier resolución de salida, y el ruido
cambia cada 1/24 s del tiempo de la línea de tiempo, así que un fotograma es
siempre el mismo. Un número fuera de rango lanza `ValueError`.

#api-entry(
  name: "PostProcess.grain",
  kind: "factory",
  signature: "grain(amount=0.06, size=1.0, animated=True) -> PostProcess",
  desc: [Grano de película: ruido de brillo de `amount` en celdas de `size` píxeles. Con `animated=False` el patrón queda fijo.],
  none,
)

#api-entry(
  name: "PostProcess.vignette",
  kind: "factory",
  signature: "vignette(strength=0.35, softness=0.6) -> PostProcess",
  desc: [Oscurece hacia las esquinas hasta `strength` (0 a 1). `softness` (0 a 1) marca desde dónde empieza la caída.],
  none,
)

#api-entry(
  name: "PostProcess.chromatic_aberration",
  kind: "factory",
  signature: "chromatic_aberration(amount=0.004) -> PostProcess",
  desc: [Separa el rojo hacia fuera y el azul hacia dentro, `amount` del cuadro en las esquinas (0 a 0.5).],
  none,
)

#api-entry(
  name: "PostProcess.color_grade",
  kind: "factory",
  signature: "color_grade(exposure=0.0, contrast=1.0, saturation=1.0, temperature=0.0) -> PostProcess",
  desc: [Corrección de color: `exposure` en pasos y `temperature` (-1 frío a 1 cálido) en luz lineal; después `contrast` alrededor del gris medio y `saturation` alrededor de la luminancia. `1.0` deja contraste y saturación igual.],
  none,
)

#api-entry(
  name: "PostProcess.tint",
  kind: "factory",
  signature: "tint(color, amount=1.0) -> PostProcess",
  desc: [Monocromo en `color`, como una copia en sepia: el brillo de cada píxel va del negro a `color` (donde el píxel brilla tanto como `color`) y luego al blanco, así que la imagen conserva su brillo. `amount` (0 a 1) lo mezcla con los colores originales; el alfa del color se ignora. Sirve también como `shader_effect`, para teñir una sola foto. Un `amount` fuera de 0 a 1 lanza `ValueError`.],
  none,
)

#api-entry(
  name: "PostProcess.duotone",
  kind: "factory",
  signature: "duotone(shadows, highlights, amount=1.0) -> PostProcess",
  desc: [Duotono: el brillo de cada píxel va de `shadows` (el negro) a `highlights` (el blanco) y los medios tonos mezclan los dos, como una impresión a dos tintas. `amount` (0 a 1) lo mezcla con los colores originales; el alfa de los colores se ignora. Sirve también como `shader_effect`, para poner una foto en los colores de una marca. Un `amount` fuera de 0 a 1 lanza `ValueError`.],
  none,
)

#api-entry(
  name: "PostProcess.overlay",
  kind: "factory",
  signature: "overlay(path, blend=\"multiply\", opacity=1.0) -> PostProcess",
  desc: [Pone la imagen de `path` sobre el cuadro, como una textura de papel o de grano: lo cubre centrada y recortada, como `Background.image`, y se mezcla con lo de debajo. `"multiply"` oscurece como la tinta (el blanco no cambia nada), `"screen"` aclara, `"overlay"` hace las dos cosas y `"normal"` pinta encima; `opacity` (0 a 1) y el alfa de la imagen escalan la mezcla. Como pase de la escena o de un segmento cubre todos los objetos, así que la textura se pone una vez y no en cada segmento. Una imagen de más de 2048 píxeles en su lado largo se reduce al leerla. Un `blend` desconocido o una `opacity` fuera de 0 a 1 lanzan `ValueError`, y un archivo ilegible, `RuntimeError`.],
  none,
)

#api-entry(
  name: "PostProcess.lut",
  kind: "factory",
  signature: "lut(path, strength=1.0) -> PostProcess",
  desc: [Aplica la tabla 3D de un archivo `.cube` (`LUT_3D_SIZE` de 2 a 65, con `DOMAIN_MIN`/`DOMAIN_MAX` opcionales), mezclada según `strength`. El archivo se lee al crear el objeto (una ruta relativa se busca en el directorio de trabajo, luego en la carpeta `assets` del script y luego junto a él): si no se puede leer lanza `RuntimeError` y una tabla inválida, `ValueError`.],
  none,
)

#api-entry(
  name: "PostProcess.halftone / dither / crt / pixelate / glitch",
  kind: "factory",
  signature: "halftone(dot=6.0) · dither(levels=4) · crt(strength=1.0) · pixelate(size=8.0) · glitch(intensity=0.5, seed=0)",
  desc: [`halftone`: puntos de imprenta en celdas de `dot` píxeles, más grandes cuanto más clara es la celda. `dither`: tramado ordenado (Bayer 4×4) a `levels` valores por canal. `crt`: pantalla curva, líneas de barrido y máscara RGB. `pixelate`: bloques de `size` píxeles. `glitch`: franjas que saltan de lado con separación RGB 12 veces por segundo; `seed` cambia el patrón y `intensity` se puede animar para dar golpes.],
)[
```python
# show-code: true
from gaanim import BLUE, GOLD, PostProcess, Scene
scene = Scene(frame=(16, 9), background="#101826")
burst = scene.viz.parameter(0.0)
scene.canvas.post = [PostProcess.glitch(intensity=burst, seed=3), PostProcess.grain(0.05)]
scene.geometry.circle(1.6).fill(GOLD).move_to(-3, 0)
scene.geometry.rounded_rect(3, 3, 0.3).fill(BLUE).move_to(3, 0)
scene.play([burst.animate.set(1.0).duration(0.3).repeat(2, yoyo=True)])
# output: preview.webp
scene.render()
```
]

#api-entry(
  name: "PostProcess.bloom",
  kind: "factory",
  signature: "bloom(threshold=0.8, intensity=0.6, radius=0.5) -> PostProcess",
  desc: [Bloom multipaso: lo que supera `threshold` (brillo sRGB de 0 a 1, con rodilla suave) brilla. Extrae las zonas claras, las reduce en una cadena de mips y las vuelve a sumar con un filtro tienda, como el bloom de Blender. `intensity` escala la luz añadida y `radius` (0 a 1) la extiende de un halo ceñido a una neblina amplia. La luz se suma en luz lineal y los núcleos muy brillantes se vuelven blancos en vez de saturarse. Su alcance es una fracción del cuadro, así que se ve igual a cualquier resolución. Complementa el `glow` vectorial de cada objeto.],
)[
```python
# show-code: true
from gaanim import PostProcess, Scene
scene = Scene(frame=(16, 9), background="#05060d")
glow = scene.viz.parameter(0.3)
scene.canvas.post = PostProcess.bloom(threshold=0.55, intensity=glow, radius=0.6)
ring = scene.geometry.circle(1.6).no_fill().stroke("#ff4fd8", 0.12).move_to(-3, 0)
star = scene.geometry.regular_polygon(5, 1.6).no_fill().stroke("#ffe45c", 0.12).move_to(3, 0)
scene.play([ring.animate.create(), star.animate.create()])
scene.play([glow.animate.set(1.4).duration(0.6).repeat(2, yoyo=True)])
# output: preview.webp
scene.render()
```
]

#api-entry(
  name: "Canvas.motion_blur",
  kind: "method",
  params: (
    (name: "shutter_angle", type: "float | None", default: "180.0", desc: [Grados de un cuadro que el obturador está abierto, de 0 a 720: 180 es el estándar del cine y 360 difumina todo el cuadro. `None` lo desactiva.]),
    (name: "samples", type: "int", default: "8", desc: [Subcuadros promediados por cuadro, de 2 a 64.]),
    (name: "phase", type: "float | None", default: "None", desc: [Dónde se abre el obturador respecto al tiempo del cuadro, en grados de un cuadro; por defecto se centra en él.]),
  ),
  returns: (type: "None"),
  desc: [Desenfoque de movimiento como el de una cámara de cine en las exportaciones y las capturas: cada cuadro promedia en luz lineal `samples` búsquedas exactas repartidas por el obturador, así que es determinista, y nunca mezcla dos segmentos. La duración del obturador sigue los fps de la exportación (las capturas usan 60). El visor interactivo se ve nítido, y renderizar cuesta `samples` veces más. Un valor fuera de rango lanza `ValueError`. Usa `drawable.motion_blur(False)` para mantener nítido un objeto, y #link("/referencia/scene/#api-segment-motion_blur")[`Segment.motion_blur`] para darle a un segmento su propio obturador.],
)[
```python
# show-code: true
from gaanim import CORAL, GOLD, WHITE, Scene
scene = Scene(frame=(16, 9), background="#101826")
scene.canvas.motion_blur(180, samples=6)
title = scene.text("Motion blur", role="title").fill(WHITE).move_to(0, 3).motion_blur(False)
ball = scene.geometry.circle(0.5).fill(CORAL).move_to(-6, 0)
bar = scene.geometry.rect(4, 0.35).fill(GOLD).move_to(0, -2)
scene.play([ball.animate.move_to(6, 0).duration(0.6), bar.animate.rotate_by(6.2832).duration(1.0)])
# output: preview.webp
scene.render()
```
]

#api-entry(
  name: "Canvas.post",
  kind: "property",
  signature: "post: PostProcess | list[PostProcess] | None",
  returns: (type: "PostProcess | list[PostProcess] | None", desc: [El postprocesado de la escena; se puede leer y asignar.]),
  desc: [Una lista encadena las pasadas en orden. `None` o una lista vacía lo quitan. Un segmento puede cambiarlo mientras está activo: `post=otro` (o una lista) usa otra cadena, `post=False` dibuja el segmento sin postprocesado y `post=None` (el predeterminado) hereda el de la escena. `drawable.post_process(False)` deja un objeto fuera de todas las pasadas. Un uniform que lee un valor de otra escena lanza `ValueError` y cualquier otro valor, `TypeError`.],
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
  [`gaanim`], [], [Los colores de la marca: papel `#F7F6FF` sobre grafito `#121212`, acento violeta `#7C6CFF`, títulos en oro `#FFC933`, gráficos en violeta claro `#A99FFF` y reglas en bruma `#3F37A8`.],
)

Los nombres no distinguen mayúsculas. En el stub, `Scene(theme=...)`,
`Canvas.set_theme` y `Theme(base)` tipan el nombre como `ThemeName`, un
`Literal` con los doce nombres y los ocho alias de la tabla, así que el editor
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
  name: "Scene.colors",
  kind: "property",
  signature: "colors: ThemeColors",
  returns: (type: "ThemeColors", desc: [Los colores del tema activo por rol, como atributos.]),
  desc: [Es la forma de dar a tus objetos los colores del tema: `scene.colors.accent` en lugar de `scene.canvas.color("accent")`. Cada rol es un atributo, así que el editor lo autocompleta y el verificador de tipos detecta un nombre mal escrito. Cada lectura resuelve el tema activo en ese momento: después de `set_theme`, la misma expresión da el color del tema nuevo, y un objeto conserva el color que recibió. Los roles que añade un tema propio con `Theme(colors={"marca": ...})`, o un rol que llega en una variable, se leen con corchetes: `scene.colors["marca"]`. `in`, `len` y la iteración recorren los roles del tema, y `palette` es su paleta categórica, como `scene.canvas.palette()`. Sin tema activo, toda lectura lanza `ValueError`; un rol desconocido, `KeyError`.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9), theme="technical")
colors = scene.colors
card = scene.geometry.rounded_rect(5.25, 2.25, 0.3).fill(colors.panel).stroke(colors.rule, 0.04)
dot = scene.geometry.dot(0.25).fill(colors.accent)
scene.play([card.animate.stroke(colors.accent, 0.04)])
```
]

#table(
  columns: (auto, 1fr),
  inset: 7pt,
  [*Rol*], [*Uso*],
  [`background`], [El fondo del cuadro.],
  [`foreground` (`primary`, `body`)], [El texto y la tinta predeterminada.],
  [`muted` (`secondary`)], [Texto secundario y marcas discretas.],
  [`title`], [Títulos.],
  [`accent`], [El color que atrae la mirada.],
  [`chart`], [Marcas de gráficos sin color propio.],
  [`panel`], [Paneles y tarjetas detrás del contenido.],
  [`header`], [Cabeceras de tablas y paneles.],
  [`rule` (`border`)], [Reglas, rejillas y bordes.],
  [`success`, `warning`, `danger`], [Buenas noticias, precaución y errores.],
  [`shadow`], [Sombras.],
)

#api-entry(
  name: "Canvas.color",
  kind: "method",
  params: ((name: "role", type: "str", default: none, desc: [Rol o token de color del tema activo.]),),
  returns: (type: "Color", desc: [El color resuelto.]),
  desc: [El mismo color que `Scene.colors`, con el rol como cadena. Sin tema activo, o con un rol desconocido, lanza `ValueError`.],
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
  name: "Canvas.palette",
  kind: "method",
  returns: (type: "Palette", desc: [La paleta categórica del tema activo; `gaanim` sin tema.]),
  desc: [Sirve para colorear tus propias categorías como las colorean los gráficos.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
teams = ["Norte", "Sur", "Este"]
colors = scene.canvas.palette().colors(len(teams))
dots = [scene.geometry.dot(0.3).fill(color).move_to(index - 1, 0) for index, color in enumerate(colors)]
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
  signature: "Theme(base: ThemeName | Theme | None = None, *, name=None, colors=None, fonts=None, sizes=None, text=None, styles=None, series=None, heatmap=None, palette=None, layout=None, font_files=None, font_dir=None, text_markup=None)",
  params: (
    (name: "base", type: "ThemeName | Theme | None", default: "None", desc: [Tema del que se deriva; solo se sustituye lo que pasas. Sin él, se parte de la paleta de `technical` con el nombre `custom`.]),
    (name: "name", type: "str | None", default: "None", desc: [Nombre del tema nuevo.]),
    (name: "colors", type: "dict[str, ColorLike] | None", default: "None", desc: [Roles de color y tokens propios para las reglas.]),
    (name: "fonts", type: "dict[str, str] | None", default: "None", desc: [Familias por rol de fuente. `"text"` nombra todos los roles de texto y `"all"` además `math` y `code`; un rol nombrado aparte, como `"title"`, conserva su familia.]),
    (name: "sizes", type: "dict[str, float] | None", default: "None", desc: [Tamaños por rol de texto, en unidades lógicas.]),
    (name: "text", type: "dict[TextRole, TextStyle] | None", default: "None", desc: [`TextStyle` por rol, como capa sobre el rol semántico.]),
    (name: "styles", type: "dict[str, Style | AxesStyle] | None", default: "None", desc: [Reglas por selector (ver abajo).]),
    (name: "series", type: "Sequence[ColorLike] | None", default: "None", desc: [Colores de serie: el primero colorea los gráficos sin canal `color`.]),
    (name: "heatmap", type: "Sequence[ColorLike] | None", default: "None", desc: [Colores continuos de los mapas de calor. Un heatmap cuyo campo `color` lleva `Scale.colors` usa los de su escala.]),
    (name: "palette", type: "PaletteLike | None", default: "None", desc: [Paleta categórica de la escena: colorea las categorías de todos los gráficos, leyendas, tartas y carreras de barras. Una `Palette`, un nombre (`"okabe_ito"`) o una lista de colores; por defecto, `gaanim`. `Scale.category().colors(...)` la sustituye en un campo concreto.]),
    (name: "layout", type: "dict[str, float] | None", default: "None", desc: [Tokens de espaciado, nuevos o sustituidos.]),
    (name: "font_files", type: "dict[str, str] | None", default: "None", desc: [Familia → archivo de fuente. La clave es la familia que declara el propio archivo, la que se usa en `fonts`; no es un alias. Con otra clave, el tema avisa con un `UserWarning` que nombra la familia del archivo.]),
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
        "line": Style(stroke=StrokeStyle("foreground", 0.04, cap="round")),
        ".warning": Style(fill=colors.tailwind.rose[600]),
        "axes": AxesStyle(grid=StrokeStyle("rule", 0.01), labels=TextStyle(size=0.24)),
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
`accent`, `chart`, `panel`, `header`, `rule`, `success`, `warning`, `danger` y
`shadow` (la sombra de `shadow=True` en las cajas: negro suave sobre un fondo
claro y casi opaco sobre uno oscuro, salvo que el tema elija otro);
`colors` también acepta tokens propios no vacíos para las reglas. Los tokens
`code`, `code_keyword`, `code_string`, `code_number`, `code_comment`,
`code_function` y `code_type` eligen los colores con los que
#link("/referencia/text/#api-typography-code")[`scene.text.code`] resalta el
código; sin ellos, los temas de editores usan su esquema y los demás los
derivan de la paleta. Los roles de
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
`glyph` para las llaves, `axes`, `plot`), el nombre exacto de una fábrica (`circle`, `rounded_rect`,
`arrow`), una parte semántica como `axes/grid`, `axes/labels` o las de los
#link("/referencia/visualization/#mapas")[mapas] (`map/area`, `map/line`,
`map/graticule`, `map/labels`, `map/frame`), o una clase
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

== Paletas categóricas

Una `Palette` es una lista ordenada de colores que se reparten entre las
categorías por turno: las de un campo `color` de texto, las porciones de una
tarta o las barras de una carrera. La del tema (`Theme(palette=...)`) colorea
toda la escena, y `Scale.category().colors(...)` la sustituye en un campo
concreto. Cuando hay más categorías que colores, la paleta vuelve a empezar y
cada vuelta aclara u oscurece un poco sus colores, así que dos categorías nunca
comparten color. Donde un parámetro acepta `PaletteLike`, también vale el
nombre de una paleta o una lista de colores.

Incluidas: `gaanim` (la predeterminada, de diez colores), `tableau10`,
`okabe_ito` (siete colores distinguibles con daltonismo; sin el negro, que
desaparece sobre un fondo oscuro), `set2`, `dark2`, `paired` y los mapas
categóricos `tab10`, `tab20`, `tab20b` y `tab20c`. Los nombres no distinguen
mayúsculas.

#api-entry(
  name: "Palette",
  kind: "class",
  signature: "Palette(colors: PaletteLike)",
  params: ((name: "colors", type: "Palette | str | Sequence[ColorLike]", default: none, desc: [Una paleta, el nombre de una incluida o al menos un color.]),),
  returns: (type: "Palette", desc: [Una paleta inmutable.]),
  desc: [Un nombre desconocido o una lista vacía lanzan `ValueError`.],
)[
```python
>>>from gaanim import *
okabe = Palette("okabe_ito")
brand = Palette(["#7c6cff", GOLD, (40, 180, 160)])
scene = Scene(frame=(16, 9), theme=Theme("gaanim", palette=okabe.rotate(1)))
```
]

#api-entry(
  name: "Palette.names",
  kind: "method",
  returns: (type: "list[str]", desc: [Nombres de las paletas incluidas.]),
  none,
)

#api-entry(
  name: "Palette.from_colormap",
  kind: "factory",
  params: (
    (name: "colormap", type: "ColorMapLike", default: none, desc: [Mapa del que se toman los colores.]),
    (name: "count", type: "int", default: none, desc: [Número de colores, al menos uno.]),
  ),
  returns: (type: "Palette", desc: [`count` colores repartidos de un extremo al otro del mapa.]),
)[
```python
>>>from gaanim import *
ramp = Palette.from_colormap("viridis", 5)
```
]

#api-entry(
  name: "Palette.name",
  kind: "property",
  returns: (type: "str | None", desc: [Nombre de una paleta incluida; `None` en una propia o derivada.]),
  none,
)

#api-entry(
  name: "Palette.colors",
  kind: "method",
  params: ((name: "count", type: "int | None", default: "None", desc: [Número de categorías; sin él, los colores propios de la paleta.]),),
  returns: (type: "list[Color]", desc: [Los colores de las primeras `count` categorías, con las vueltas incluidas.]),
  none,
)

#api-entry(
  name: "Palette.color",
  kind: "method",
  params: ((name: "index", type: "int", default: none, desc: [Posición de la categoría, desde 0.]),),
  returns: (type: "Color", desc: [Su color; `palette[index]` hace lo mismo y `len(palette)` cuenta los colores propios.]),
  none,
)

#api-entry(
  name: "Palette.reversed",
  kind: "method",
  returns: (type: "Palette", desc: [Los mismos colores en orden inverso.]),
  none,
)

#api-entry(
  name: "Palette.rotate",
  kind: "method",
  params: ((name: "steps", type: "int", default: none, desc: [Posiciones que avanza; negativo, hacia atrás.]),),
  returns: (type: "Palette", desc: [Los mismos colores empezando `steps` más tarde: `rotate(1)` pone el segundo primero.]),
  none,
)

== Efectos visuales

`glow`, `blur` y `shadow` se aplican a cualquier objeto y funcionan sobre
rellenos y trazos, incluidos los gradientes; `no_effects()` los quita. Están
en #link("/referencia/drawable/#api-drawable-glow")[Drawable] y sus versiones
animadas en #link("/referencia/animations/#api-anim-glow")[Animaciones].
