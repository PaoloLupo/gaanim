#import "../../components/section.typ": docs-chapter
#import "../../components/api.typ": api-entry

#show: docs-chapter.with(
  title: "Temas y colores",
  description: "Reglas visuales centralizadas, colores CSS y paletas reutilizables",
  route: "/api/themes/",
  code-langs: (),
  updated: datetime.today().display(),
)

= Temas y colores

Define el lenguaje visual de una animación con el fondo de la escena y las
constantes de color exportadas. Los temas incluidos se administran mediante
`scene.canvas`.

Para material técnico o de estilo LaTeX conviene usar un fondo casi negro,
texto blanco o gris suave y azul apagado para la estructura. Reserva los
colores semánticos saturados —verde, rojo y dorado— para significados positivos,
negativos o excepcionales. Las tarjetas de título, viñetas y gráficas de barras
incluidas siguen de forma predeterminada este lenguaje más sobrio.

```python
from gaanim import BLACK, BLUE, GOLD, WHITE, Scene

scene = Scene(1280, 720, background=BLACK)
circle = scene.circle(80).fill(BLUE).stroke(GOLD, 4)
label = scene.text("Colorful scene", role="title").fill(WHITE)
```

También puedes cambiar mediante `scene.canvas` el fondo dentro de los límites
creados para la escena:

```python
from gaanim import Color

scene.canvas.background = Color(40, 42, 54)
```

== Fondos con gradientes y WGSL

`Scene.background` y `scene.canvas.background` aceptan los mismos gradientes
`Brush` que los objetos dibujables. Sus coordenadas pertenecen a la escena: en
una escena de 1280×720, un gradiente lineal de ancho completo va de `x=-640` a
`x=640`:

```python
from gaanim import Brush, Scene

sky = Brush.linear(
    ["#071022", "#164E8A", "#7DD3FC"],
    start=(-640, 0),
    end=(640, 0),
)
scene = Scene(1280, 720, background=sky)
```

Para arte procedural o animado, `Background.shader(source, fallback=...)`
acepta una función WGSL con esta firma:

```wgsl
fn gaanim_background(
    uv: vec2<f32>,
    resolution: vec2<f32>,
    time: f32,
) -> vec4<f32> {
    let pulse = 0.5 + 0.5 * sin(time * 2.0);
    return vec4<f32>(uv.x, uv.y, pulse, 1.0);
}
```

`uv=(0, 0)` es la esquina superior izquierda y `uv=(1, 1)` la inferior derecha.
`resolution` es el tamaño efectivo en píxeles y `time` la posición absoluta de
la línea temporal en segundos. Como `time` sigue tanto la reproducción como las
búsquedas exactas, las capturas y exportaciones son deterministas. El shader
cubre el mismo rectángulo de escena que muestra el editor; el espacio de bandas
exterior usa `fallback`. La función se valida al crear `Background` y se guarda
como textura de Vello para la resolución y el tiempo activos. Redimensionar el
editor vuelve a rasterizarla. `fallback` vale negro por defecto y también se usa
para limpiar el fondo 3D, calcular contraste automático o recuperarse de un
fallo de rasterización en la GPU.

Una cadena es WGSL inline. Para cargar un asset `.wgsl`, pasa un `pathlib.Path`;
el archivo se lee y valida al crear el fondo, por lo que un asset ilegible lanza
`RuntimeError` y WGSL inválido lanza `ValueError`.

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
scene = Scene(1280, 720, background=shader)

from pathlib import Path
asset_shader = Background.shader(Path("assets/background.wgsl"), fallback="#071022")
```

== Temas incluidos

`technical` es el tema oscuro y sobrio predeterminado para explicaciones
matemáticas y documentación técnica. `presentation` está optimizado para
proyección: fondo azul marino, títulos dorados cálidos, cuerpo brillante y
etiquetas secundarias más frías. `paper` usa un lienzo blanco con tinta oscura
contenida. Un `.fill(...)` explícito siempre tiene prioridad.

Los valores predeterminados de los componentes también siguen el tema elegido.
Tarjetas, insignias, bandas, avisos, gráficas de barras, tablas y paneles de
código heredan colores compatibles de primer plano, acento, panel y regla. Las
gráficas de barras incluyen etiquetas de valor y reservan altura para mantenerlas
dentro de sus límites.

```python
from gaanim import Scene

scene = Scene(1280, 720)
scene.canvas.set_theme("presentation")

title = scene.text("Fourier transform", role="title")
subtitle = scene.text("Frequency-domain representation", role="subtitle")
equation = scene.text("$F(k) = integral f(x) e^(-i k x) dif x$")
```

Para cambiar solo las familias tipográficas no hace falta crear un `Theme`.
`set_fonts` acepta sobreescrituras independientes para texto, matemáticas y
código; lo omitido conserva su valor anterior:

#api-entry(
  name: "Canvas.set_fonts",
  signature: "set_fonts(*, font=None, math_font=None, code_font=None) -> None",
  params: (
    (name: "font", type: "str | None", default: "None", desc: [Familia para títulos, cuerpo, subtítulos, encabezados, pies y etiquetas.]),
    (name: "math_font", type: "str | None", default: "None", desc: [Familia para ecuaciones y fragmentos matemáticos.]),
    (name: "code_font", type: "str | None", default: "None", desc: [Familia para texto con rol `code`.]),
  ),
  returns: (type: "None", desc: [Configura el lienzo; no crea objetos.]),
  desc: [Las familias suministradas tienen prioridad sobre el tema activo y siguen vigentes si el tema cambia después. Un `font` explícito en un objeto conserva mayor prioridad. Una familia vacía produce `ValueError`.],
)[```python
from gaanim import Scene

scene = Scene(1280, 720)
scene.canvas.set_fonts(
    font="Inter",
    math_font="New Computer Modern Math",
    code_font="JetBrains Mono",
)
scene.text("Texto y $x^2$", role="body")
scene.text("fn main() {}", role="code")
```]

Los nombres disponibles de los temas se enumeran a continuación.

== Esquemas de color conocidos

La misma API breve incluye paletas conocidas de editores y terminales:

```python
scene.canvas.set_theme("dracula")
scene.canvas.set_theme("nord")
scene.canvas.set_theme("solarized-dark")
scene.canvas.set_theme("solarized-light")
scene.canvas.set_theme("gruvbox-dark")
scene.canvas.set_theme("tokyo-night")
scene.canvas.set_theme("catppuccin-mocha")
scene.canvas.set_theme("catppuccin-latte")
```

Usa `Theme.schemes()` cuando una herramienta o interfaz necesite enumerar todos
los esquemas incluidos.

== Temas personalizados y derivados

`Theme` reúne colores semánticos, tipografía estructurada, reglas de selectores,
paletas de datos, tokens de layout y archivos de fuentes. Se instala directamente
con `Scene(theme=...)`; pasa el nombre de un esquema para derivarlo y sustituir
solo lo que cambia:

```python
from gaanim import AxesStyle, Scene, StrokeStyle, Style, TextStyle, Theme, colors

theme = Theme(
    "nord",
    name="my-slides",
    colors={
        "title": "#A3D9FF",
        "accent": "#FFB86C",
        "chart": "#88C0D0",
    },
    fonts={
        "text": "Inter",
        "code": "JetBrains Mono",
    },
    sizes={"title": 72, "body": 34},
    text={
        "body": TextStyle(size=32, letter_spacing=0.1),
        "label": TextStyle(size=24, weight=600),
    },
    styles={
        "shape": Style(fill="accent"),
        "line": Style(stroke=StrokeStyle("foreground", 3, cap="round")),
        ".warning": Style(fill=colors.tailwind.rose[600]),
        "axes": AxesStyle(
            grid=StrokeStyle("rule", 1),
            labels=TextStyle(size=24),
        ),
    },
    series=[colors.tailwind.blue[600], colors.tailwind.amber[500]],
    layout={"page_padding": 56, "column_gap": 48},
    font_files={
        "Inter": "assets/Inter-Regular.ttf",
        "JetBrains Mono": "assets/JetBrainsMono-Regular.ttf",
    },
)

scene = Scene(1920, 1080, theme=theme)
```

El diccionario `text` reutiliza el mismo `TextStyle` que aceptan `Text`
estructurado y `part(...)`. Funciona como una capa: las propiedades omitidas
siguen viniendo del rol semántico. Los estilos de `TextPart` y los métodos
explícitos del objeto dibujable conservan mayor prioridad.

== Cascada de selectores

Las reglas pueden seleccionar una familia (`shape`, `line`, `text`, `axes`,
`plot`), el nombre exacto de una fábrica (`circle`, `rounded_rect`, `arrow`),
una parte semántica como `axes/grid` o `axes/labels`, o una clase del usuario
como `.warning`.

```python
from gaanim import Scene, Style, Theme

theme = Theme(
    "paper",
    colors={"brand": "#2563eb", "danger": "oklch(58% .24 25)"},
    styles={
        "shape": Style(fill="brand"),
        ".danger": Style(fill="danger"),
    },
)
scene = Scene(theme=theme)
ordinary = scene.circle(60)
warning = scene.square(100).style_class("danger")
explicit = scene.circle(40).fill("gold")
```

La precedencia es: tema base, familia, tipo exacto o parte semántica, clases del
usuario en orden, valores del constructor y, finalmente, cambios fluidos. Las
reglas se materializan al compilar la escena, por lo que cambiar el tema activo
actualiza también objetos compatibles ya creados. La pintura de recursos
importados permanece controlada por su origen salvo que se estilice explícitamente.

`StrokeStyle` contiene pintura, ancho, remate, unión, límite de inglete, patrón y
desplazamiento de guiones. Puede reutilizarse en un tema o aplicarse directamente
con `drawable.stroke_style(style)`. Métricas o selectores inválidos y tokens sin
resolver producen `ValueError`.

Los archivos de fuente se leen al crear `Theme` y se incorporan al runtime del
lienzo. Así, las exportaciones no dependen de que la fuente esté instalada en
el equipo de presentación. El registro subyacente admite archivos TTF y OTF.

Para comenzar sin heredar un esquema con nombre, omite el primer argumento:

```python
brand = Theme(
    name="brand",
    colors={
        "background": "#10131A",
        "foreground": "#F8FAFC",
        "muted": "#94A3B8",
        "title": "#FDE68A",
        "accent": "#38BDF8",
        "chart": "#22C55E",
        "panel": "#18202E",
        "header": "#202B3D",
        "rule": "#475569",
    },
    fonts={"text": "Aptos", "code": "Consolas"},
)
```

Otro `Theme` puede ser el primer argumento para hacer explícitas su modificación
y reutilización:

```python
print_theme = Theme(
    brand,
    name="brand-print",
    colors={"background": "white", "foreground": "#172033"},
)
```

Los roles de color incluidos son `background`, `foreground`, `muted`, `title`,
`accent`, `chart`, `panel`, `header`, `rule`, `success`, `warning` y `danger`.
Los tres últimos controlan variantes editoriales y pueden sustituirse con
`Theme(colors=...)`. Los roles de fuente son `text`, `all`, `title`, `subtitle`,
`heading`, `body`, `caption`, `label`, `math` y `code`. El diccionario `colors`
también puede definir tokens arbitrarios no vacíos para las reglas de selectores.

== CSS Color 4 y Tailwind

Todo argumento `ColorLike` acepta la sintaxis CSS Color 4. `Color(...)` también
recibe directamente un literal; los constructores explícitos son útiles cuando
los valores se calculan:

```python
from gaanim import Color, colors

navy = Color("#0f172a")
accent = Color("oklch(62.3% 0.214 259.815)")
translucent = Color("rgb(37 99 235 / 65%)")
computed = Color.from_hsl(215, 0.9, 0.55, 0.8)
perceptual = Color.from_oklch(0.68, 0.17, 240)
tailwind_blue = colors.tailwind.blue[500]
```

`colors.tailwind` contiene las 26 familias y las escalas 50–950 de Tailwind CSS
v4.3.3, incluidas `mauve`, `olive`, `mist` y `taupe`. La versión incorporada
está disponible en `colors.tailwind.version`.

== Mapas de color científicos

`ColorMap` ofrece 39 mapas canónicos de Matplotlib y 39 de Scientific Colour
Maps. El catálogo se incorpora como datos Rust: no necesita Julia, Matplotlib,
archivos externos ni un registro global durante la ejecución.

```python
from gaanim import ColorMap

viridis = ColorMap("viridis")
vik = ColorMap.named("vik")
custom = ColorMap.from_colors(["#0f172a", "#06b6d4", "#f8fafc"])

midpoint = vik.sample(0.5)
swatches = viridis.colors(8)
transparent = custom.with_alpha(0.65)
reverse = viridis.reversed()
scientific_names = ColorMap.names("scientific")
```

Las posiciones se normalizan al intervalo 0–1 y los mapas continuos interpolan
componentes sRGB. Los mapas categóricos conservan escalones discretos. Los
nombres no distinguen mayúsculas de minúsculas; un nombre, categoría, posición,
alpha o lista de colores inválida produce `ValueError` inmediatamente.

#api-entry(
  name: "ColorMap",
  kind: "type",
  signature: "ColorMap(name) | ColorMap.from_colors(colors, positions=None)",
  params: (
    (name: "name", type: "str", default: none, desc: [Nombre incluido de Matplotlib o Scientific Colour Maps.]),
    (name: "colors", type: "sequence[ColorLike]", default: none, desc: [Al menos dos colores para un mapa personalizado.]),
    (name: "positions", type: "sequence[float] | None", default: "None", desc: [Posiciones estrictamente crecientes entre 0 y 1; por defecto se distribuyen uniformemente.]),
  ),
  returns: (type: "ColorMap", desc: [Valor inmutable, clonable y reutilizable por líneas 3D, campos y futuras escalas.]),
  desc: [`sample`, `colors`, `reversed`, `with_alpha` y `names` forman la API completa del catálogo.],
)[]

Las plantillas de layout consumen valores de espaciado con nombre mediante
`scene.canvas.layout_token(name)`. La escala predeterminada incluye `space_xs`,
`space_sm`, `space_md`, `space_lg`, `page_padding`, `page_padding_wide`,
`page_padding_x`, `column_gap`, `vertical_padding`, `vertical_padding_x`, and
`lower_third_offset`. Un tema personalizado puede sustituirlos o añadir tokens
propios del proyecto mediante el argumento `layout={...}`.

== Tokens de tema y legibilidad

Los objetos vectoriales manuales pueden consumir los mismos tokens semánticos
que los componentes:

```python
scene.rounded_rect(420, 180, 24) \
    .fill(scene.canvas.color("panel")) \
    .stroke(scene.canvas.color("accent"), 3)

divider = scene.line(-400, 0, 400, 0) \
    .stroke(theme.color("rule"), 2)

page = scene.column(
    [title, body],
    padding=scene.canvas.layout_token("page_padding"),
    gap=scene.canvas.layout_token("space_lg"),
)
```

`Theme.validate()` y `scene.canvas.validate_theme()` devuelven advertencias
accionables si el contraste de primer plano, título, texto atenuado o panel es
insuficiente, o si la tipografía es inválida. Devuelven una lista vacía cuando
las combinaciones principales están listas:

```python
warnings = scene.canvas.validate_theme()
if warnings:
    raise ValueError("\n".join(warnings))
```

La validación es orientativa, no un rechazo automático; por eso siguen siendo
posibles los estados animados de bajo contraste intencional.

== Pinceles y gradientes

`Drawable.fill(...)` y `Drawable.stroke(...)` aceptan un `ColorLike` normal o
un `Brush` reutilizable. Las coordenadas del gradiente usan el espacio local del
objeto, de modo que la pintura acompaña sus transformaciones posteriores.

```python
from gaanim import Brush

gradient = Brush.linear(
    ["#7AA2F7", "#BB9AF7", "#F7768E"],
    start=(-240, 0),
    end=(240, 0),
)

card = scene.rounded_rect(480, 220, 28).fill(gradient)
```

Los gradientes radiales y angulares usan la misma convención de lista de colores.
Los colores se distribuyen uniformemente y se requieren dos paradas como mínimo:

```python
orb = scene.circle(120).fill(
    Brush.radial(
        ["white", scene.canvas.color("accent"), "#0000"],
        center=(-30, 35),
        radius=150,
    )
)

ring = scene.circle(110).no_fill().stroke(
    Brush.sweep(
        ["#7DCFFF", "#9ECE6A", "#E0AF68", "#F7768E", "#7DCFFF"],
        center=(0, 0),
    ),
    20,
)
```

`extend="pad"` es el valor predeterminado. Usa `"repeat"` o `"reflect"` para
repetir la rampa. Los ángulos de barrido se expresan en grados; los puntos
lineales y radios usan unidades de escena.

== Efectos visuales

Los efectos usan la misma interfaz fluida de `Drawable`. Los valores
predeterminados mantienen breves las llamadas comunes, mientras radio,
intensidad, desplazamiento y desenfoque siguen siendo configurables:

```python
title.glow("#38BDF8")
background_blob.blur(12)
card.shadow("#00000080", x=10, y=-10, blur=8)
```

`glow`, `blur` y `shadow` se compilan como fragmentos vectoriales retenidos, por
lo que los efectos sin cambios se reutilizan desde la caché. Funcionan sobre
rellenos y trazos, incluidos los pinceles de gradiente. `no_effects()` elimina
los tres sin cambiar el relleno ni el trazo del objeto. En `Text`, los cuatro
conservan el handle especializado y el anclaje tipográfico de una llamada
posterior a `at()`.

== Constantes de color

#table(
  columns: (1fr, 1fr, 1fr),
  [*Nombre*], [*Hex*], [*Uso*],
  [`BLUE`], [`#3b82f6`], [Azul principal],
  [`GOLD`], [`#eab308`], [Dorado/amarillo],
  [`RED`], [`#ef4444`], [Rojo],
  [`GREEN`], [`#22c55e`], [Verde],
  [`WHITE`], [`#ffffff`], [Blanco],
  [`BLACK`], [`#000000`], [Negro],
  [`YELLOW`], [`#facc15`], [Amarillo],
  [`ORANGE`], [`#f97316`], [Naranja],
  [`PURPLE`], [`#a855f7`], [Morado],
  [`PINK`], [`#ec4899`], [Rosa],
  [`GRAY`], [`#6b7280`], [Gris],
  [`CYAN`], [`#06b6d4`], [Cian],
  [`CORAL`], [`#ff7f50`], [Coral],
  [`NAVY`], [`#1e3a5f`], [Azul marino],
  [`TEAL`], [`#14b8a6`], [Verde azulado],
)

== Colores personalizados

`Color` recibe canales RGBA de 0 a 255:

```python
from gaanim import Color

custom = Color(128, 51, 204, 255)
circle = scene.circle(80).fill(custom)
```
