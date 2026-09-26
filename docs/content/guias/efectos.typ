#import "../../components/section.typ": docs-chapter

#show: docs-chapter.with(
  title: "Efectos",
  description: "Glow, blur y sombra animables, postprocesado WGSL y lo que aún no está disponible",
  route: "/guias/efectos/",
)

= Acabado visual

En esta guía añades profundidad y brillo a los objetos con `glow`, `blur` y
`shadow`, los animas para levantar una tarjeta, hacer latir una luz o enfocar
un objeto, y aplicas un postprocesado WGSL a todo el cuadro.

```python
from gaanim import BLACK, CYAN, WHITE, Scene

scene = Scene(frame=(16, 9), background="#1e293b")
tarjeta = scene.geometry.rounded_rect(3, 2, 0.2).fill("#f8fafc").move_to(-4.5, 0)
tarjeta.shadow(BLACK, 0, -0.05, 0.05)
orbe = scene.geometry.circle(0.8).fill(CYAN)
hexagono = scene.geometry.regular_polygon(6, 1.0).fill(WHITE).move_to(4.5, 0)
hexagono.blur(0.15)
scene.play([
    tarjeta.animate.shadow(BLACK, 0, -0.3, 0.35).scale_to(1.06).duration(0.8),
    orbe.animate.glow(CYAN, radius=0.6, intensity=2.0).duration(0.4).repeat(2, yoyo=True),
    hexagono.animate.blur(0.0).duration(0.8),
])
scene.wait(0.3)
scene.render()
# output: preview.webp
```

La tarjeta se levanta, la luz late y el hexágono se enfoca. Los tres efectos
usan unidades de escena, así que se ven igual a cualquier resolución.

== Glow, blur y sombra

Los tres son setters fluidos de cualquier drawable, texto incluido:

- `glow(color, radius=0.16, intensity=1.0)`: un halo del color indicado.
  Sirve para destacar un elemento activo o dar aspecto de luz.
- `blur(sigma=0.04)`: desenfoque gaussiano. Aleja un fondo o suaviza una
  forma decorativa.
- `shadow(color, x=0.08, y=-0.08, blur=0.06)`: sombra desplazada y
  desenfocada. El alfa del color regula su opacidad; una sombra sutil separa
  una tarjeta del fondo.
- `no_effects()` quita los tres sin tocar relleno ni trazo.

```python
from gaanim import Scene

scene = Scene(frame=(16, 9), background="#0f172a")
titulo = scene.text("Título", role="title").fill("#e2e8f0").move_to(0, 2.5).glow("#38BDF8")
mancha = scene.geometry.circle(2).fill("#1E3A8A").move_to(-4, -1).blur(0.15)
tarjeta = scene.geometry.rounded_rect(4, 2, 0.2).fill("#18202E").move_to(2, -1)
tarjeta.shadow("#00000080", x=0.12, y=-0.12, blur=0.1)
scene.render()
```

Funcionan sobre rellenos y trazos, también con pinceles de gradiente, y los
efectos que no cambian se reutilizan desde la caché del renderizador. En un
`Text` conservan el handle especializado, así que puedes seguir encadenando
métodos de texto.

== Animar efectos

`animate.glow(...)`, `animate.blur(sigma)` y `animate.shadow(...)` interpolan
desde el estado actual hasta el indicado, y se combinan con otros destinos de
la misma animación, como `scale_to`. Un efecto que no existía crece desde
cero; `glow(None)`, `blur(0)` y `shadow(None)` lo desvanecen.

Tres patrones cubren la mayoría de usos:

- *Levantar*: una sombra más lejana y difusa junto con un leve `scale_to`
  hacen que una tarjeta "se despegue" del fondo al seleccionarla.
- *Latir*: `glow(...)` con `.repeat(n, yoyo=True)` hace pulsar una luz y
  vuelve al estado inicial con un número par de ciclos.
- *Enfocar*: parte de `blur(0.3)` y anima a `blur(0.0)`. Para texto, usa
  `text.animate.blur_in(...)`, que además escalona las unidades.

```python
from gaanim import BLACK, GOLD, Scene

scene = Scene(frame=(16, 9), background="#1e293b")
tarjeta = scene.geometry.rounded_rect(4, 2.5, 0.2).fill("#f8fafc")
tarjeta.shadow(BLACK, 0, -0.3, 0.35)
estrella = scene.geometry.star(5, 0.8, 0.35).fill(GOLD).move_to(5, 2).glow(GOLD, radius=0.4)
scene.play([
    tarjeta.animate.shadow(BLACK, 0, -0.05, 0.05).scale_to(1.0).duration(0.6),
    estrella.animate.glow(None).duration(0.6),
])
scene.play(tarjeta.animate.shadow(None).duration(0.4))
scene.render()
```

Los efectos no pueden animarse sobre una selección de texto
(`texto["x"].animate.glow(...)` lanza `TypeError`); para destacar una parte,
usa el resaltador o un cambio de color, descritos en
#link("/guias/tipografia-cinetica/")[Tipografía cinética].

== Postprocesado de todo el cuadro

`PostProcess.shader(fuente)` aplica una función WGSL sobre todo lo que la
escena dibuja en 2D: fondo, texto, formas, imágenes y Lottie. La fuente define
`gaanim_post(uv, resolution, time)` y lee el color ya renderizado con
`gaanim_scene(uv)`. `time` es el segundo exacto de la línea de tiempo, así que
un efecto que cambia con el tiempo coincide en la vista previa, los seeks y la
exportación.

```python
from gaanim import CYAN, GOLD, PostProcess, Scene

acabado = PostProcess.shader("""
fn grano(p: vec2<f32>) -> f32 {
    return fract(sin(dot(p, vec2<f32>(12.9898, 78.233))) * 43758.5453);
}
fn gaanim_post(uv: vec2<f32>, resolution: vec2<f32>, time: f32) -> vec4<f32> {
    let color = gaanim_scene(uv);
    let vineta = smoothstep(0.95, 0.3, distance(uv, vec2<f32>(0.5)));
    let ruido = (grano(floor(uv * resolution) + floor(time * 24.0)) - 0.5) * 0.08;
    return vec4<f32>(color.rgb * vineta + ruido, color.a);
}
""")
scene = Scene(frame=(16, 9), background="#334155", post=acabado)
scene.geometry.rounded_rect(6, 3.5, 0.3).fill("#f8fafc")
punto = scene.geometry.circle(0.5).fill(CYAN).move_to(-2, 0)
scene.geometry.star(5, 0.6, 0.25).fill(GOLD).move_to(2, 0)
scene.play(punto.animate.shift_by(1.5, 0).duration(1.0))
scene.render()
# output: preview.webp
```

Dónde se declara:

- `Scene(post=...)` lo aplica a toda la escena.
- `scene.canvas.post = ...` lo cambia después; `None` lo quita.
- `scene.segment(nombre, post=...)` lo reemplaza mientras el segmento está
  activo; `post=False` dibuja ese segmento sin postprocesado.

El shader trabaja en píxeles de salida: expresa radios y desplazamientos como
fracción de `uv` (o divide por `resolution`) para que el efecto se vea igual a
cualquier resolución. También puedes cargar la fuente de un archivo:
`PostProcess.shader(Path("assets/grade.wgsl"))`.

Para un fondo procedural o animado, `Background.shader(...)` acepta una
función WGSL análoga, `gaanim_background(uv, resolution, time)`, que se dibuja
detrás de la escena.

== Lo que todavía no existe

El postprocesado actual es un único shader por escena o segmento que recibe
solo `uv`, `resolution` y `time`. Estas capacidades todavía *no* están
disponibles:

- encadenar varios postprocesos o pasarles valores animados desde Python;
- presets de acabado listos para usar (grano, viñeta, aberración cromática,
  corrección de color o LUT), bloom de varios pasos, desenfoque de movimiento,
  ecos y estelas;
- modos de fusión por objeto en general (solo el resaltador de texto acepta
  `blend="multiply"`);
- fondos vivos predefinidos y transiciones entre segmentos basadas en shaders.

Mientras tanto, escribe el acabado en tu propio shader, como el grano y la
viñeta del ejemplo anterior, o usa `glow` para simular brillo.

Además, el postprocesado no se aplica con una cámara en perspectiva (las
mallas 3D no pasan por el renderizador vectorial) ni a la exportación SVG y
otras salidas vectoriales.

== Referencia

- #link("/referencia/themes/")[Temas y colores]: efectos visuales,
  `PostProcess`, `Background.shader` y pinceles.
- #link("/referencia/animations/")[Animaciones]: `animate.glow`,
  `animate.blur` y `animate.shadow`.
- #link("/referencia/text/")[Texto]: `blur_in` y el animador de rango con
  `blur`.
