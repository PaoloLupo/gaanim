#import "../../components/section.typ": docs-chapter

#show: docs-chapter.with(
  title: "Novedades",
  description: "Cambios visibles de cada versión de Gaanim y cómo migrar entre ellas",
  route: "/apendices/novedades/",
)

Qué cambió en cada versión, de la más reciente a la más antigua. Cada sección
indica también qué tienes que ajustar en tus escenas al actualizar. Para
instalar una versión nueva, sigue
#link("/empezar/instalacion/")[Instalación].

= Próxima versión

Cambios todavía sin publicar.

== Cambios

- En la línea de tiempo del editor, la escena actual se distingue con el
  color de acento, un subrayado y un contorno. La escena bajo el cursor se
  ilumina, y queda contorneada cuando un clic saltaría a su inicio. Una línea
  vertical separa cada escena de la siguiente en los carriles de escenas y
  de reproducción.

= 0.4.1

Publicada el 26 de septiembre de 2026. Además de correcciones, cambia el tema
por defecto y la escala de los SVG: lee «Al actualizar» antes de actualizar
un proyecto existente.

== Al actualizar

- `Scene()` usa por defecto el tema `technical`: fondo gris casi negro
  `#121212`, texto y ejes claros y formas sin color propio rellenas con el
  acento ámbar `#F2A541`. Antes una escena empezaba sin tema y en blanco, y el
  texto y las formas sin color no se veían. `scene.canvas.theme` vale
  `"technical"`, y `scene.canvas.color(...)` y `validate_theme()` ya no lanzan
  `ValueError` en una escena nueva.
- El tema `technical` (y `Theme()` sin base) cambia su fondo azulado
  `#0B1018` por el gris neutro `#121212`, con texto `#E6E6E6`, texto atenuado
  `#A0A0A0`, panel `#1C1C1C`, cabecera `#2A2A2A` y regla `#707070`, y sus
  acentos azules por un ámbar `#F2A541` (formas y énfasis) y un gris neutro
  `#BDBDBD` para los datos de los gráficos: ningún color del tema tiene tinte
  azul.
- El tema `presentation` usa la misma base neutra: fondo `#121212` (antes el
  azul marino `#070B16`), texto `#F2F2F2`, texto atenuado `#A8A8A8` y paneles
  y reglas grises; conserva sus títulos y acentos dorados, y sus gráficos
  pasan del azul `#5B8FFF` al coral `#F4845F`.
- Las operaciones booleanas (`union`, `intersection`, `difference`, `xor`) y
  las máscaras de `fill_level` aproximan las curvas con una tolerancia de
  `0.0025` unidades en lugar de `0.25`, un valor que había quedado en escala de
  píxeles: los círculos vuelven a salir suaves en vez de poligonales.
- `scene.viz.matrix(...)` vuelve a aplicar los valores documentados en
  unidades lógicas: `row_gap` y `column_gap` de `0.24` (antes `24`) y un
  delimitador automático de `max(0.72, 0.6 × filas)` (antes
  `max(72, 60 × filas)`). Con los valores en píxeles, una matriz de 3 × 3 dejaba
  sus entradas fuera del marco y el paréntesis de una de 2 × 2 lo tapaba entero;
  las matrices con separaciones y `delimiter_size` explícitos no cambian.
- Lo explícito sigue ganando: `Scene(background=...)` conserva su fondo,
  `Scene(theme=...)` y `scene.canvas.set_theme(...)` sustituyen al tema
  predeterminado y los colores de cada objeto ganan a los del tema.
- Para recuperar el lienzo anterior, sin tema y blanco, usa
  `Scene(theme=None)` o `scene.canvas.set_theme(None)`; `gaanim check` avisa
  si el texto y las formas sin color no se verían sobre el fondo.
- En el stub, los nombres de tema tienen el tipo `ThemeName`, así que el
  editor autocompleta los temas incluidos y sus alias.
- `scene.media.svg(path)` importa el documento a 100 píxeles SVG por unidad
  lógica, la misma equivalencia que el resto del motor (un trazo de 3 px mide
  0.03, como el trazo por defecto), en lugar de una unidad por píxel. Un SVG de
  360 × 220 px ahora mide 3.6 × 2.2 unidades en vez de desbordar el marco de
  16 × 9; los trazos, degradados, `clipPath`, texto y filtros del archivo
  escalan igual. Es un cambio incompatible: si tus escenas compensaban la
  escala anterior con `scale_to(f)` o `scale_by(f)`, multiplica ese factor por
  100 (`scale_to(0.025)` pasa a `scale_to(2.5)`) o quítalo si el tamaño
  natural te sirve. Los anchos fijados con `.stroke(color, ancho)` no cambian.
- El texto de un SVG con la familia genérica `sans-serif` usa la DejaVu Sans
  que trae Gaanim en cualquier sistema. Antes salía en Arial si estaba
  instalada y desaparecía en los equipos Linux que no la tienen.
- `gaanim check` imprime su informe con el nuevo formato de la terminal
  (`pass`, `warning`, `error`, `fail`) en lugar de las líneas `PASS:`,
  `WARN:` y `ERROR:`. Si un script leía ese texto, usa el código de salida,
  que no cambia: `0` sin errores, `1` con problemas y `2` si el proyecto no
  carga.
- `scene.geometry.line(p0, p1)` con dos puntos fijos es ahora una línea
  normal: acompaña al grupo que la contiene cuando lo desplazas. Las líneas
  con extremos que son objetos o referencias siguen recalculándose.

== Cambios

- La terminal muestra solo lo que te sirve, con color y columnas alineadas:
  el logo de Gaanim al abrir la vista previa, exportar o pedir `--help`, y
  una línea por evento con su etiqueta (`watch`, `ready`, `reload`, `export`,
  `python`…), donde los detalles secundarios aparecen atenuados. Los mensajes
  internos de Bevy, wgpu, winit y egui (adaptador, shaders, ventana) solo se
  muestran si son errores; los de ALSA se resumen en un único aviso `audio`,
  y las trazas de Python empiezan en tu código y resaltan el error.
  `gaanim check` usa el mismo formato: `pass`, `warning`, `error` o `fail`.
  Consulta
  #link("/apendices/solucion-de-problemas/")[Solución de problemas]
  para ver de nuevo todos los mensajes o quitar el color.
- `gaanim --help` y la ayuda de cada comando están reescritas: incluyen
  ejemplos, las teclas del modo presentación, las variables de entorno y el
  enlace a esta documentación.
- Las exportaciones respetan `WGPU_BACKEND` (`vulkan`, `dx12`, `metal`, `gl`)
  igual que la vista previa, para elegir el backend de la GPU cuando el que
  se elige por defecto falla.
- `move_along(path, start=0.5, end=0.2)` recorre el tramo al revés en lugar
  de lanzar `ValueError`, y `move_along` sobre una flecha sólida sigue su eje
  de la cola a la punta.
- `stroke(color, ancho, align=...)`, con `"inside"`, `"center"` u
  `"outside"`, coloca el contorno en drawables, glifos de texto y `SurroundingRect`. Con
  `"outside"` puedes dar un halo a un texto.
- `--sections` y `--from` aceptan el nombre de un `SectionStep`.

== Correcciones

- Las exportaciones ya no salen en negro cuando la escena tiene muchas
  líneas o muchos objetos translúcidos, sobre todo a resoluciones altas. Si un
  fotograma no se puede dibujar, la exportación falla indicando cuál en vez de
  escribirlo en negro.
- `rotate_by` con pivote, `grow_from_center`, `grow_from_point`,
  `grow_from_edge`, `spin_in_from_nothing`, `fade_in_from` y `move_along` ya
  no muestran al principio la pose de un momento posterior de la línea de
  tiempo. Las rotaciones con pivote de más de media vuelta giran de forma
  continua.
- `show_passing_flash` mantiene el objeto oculto antes del destello, y
  `trim()` y los destellos funcionan también en líneas que se recalculan cada
  fotograma (líneas entre extremos, conectores, gráficas reactivas).
- `follow()` sigue correctamente a los puntos de `point_on_curve`.
- Un grupo declarado después de `wait` o `play` ya no oculta a sus miembros
  que eran visibles desde el principio.
- Los números de `readout` y `rolling_number` usan por defecto el color del
  texto del cuerpo; antes quedaban sin relleno si no había tema ni color.
- Los conflictos entre animaciones del mismo canal nombran el tipo de objeto y
  las dos animaciones implicadas.
- En Windows, `__file__`, `sys.argv` y `sys.path` usan rutas normales, sin el
  prefijo `\\?\`.

= 0.4.0

La versión del diseño de movimiento: resortes, easings expresivos,
repeticiones, escalonado espacial, animación de texto, transiciones y
narración.

== Movimiento

- `Easing.spring(bounce=...)` con un rebote exacto, masa y velocidad inicial,
  y los presets `GENTLE`, `QUICK`, `SNAPPY`, `BOUNCY` y `SMOOTH_SPRING`.
- Nuevos easings: `Easing.back`, `elastic`, `bounce`, `slow_mo`, `rough`,
  `squish`, `from_svg`, `steps(jump=...)` y `Easing.custom`.
- `Anim.repeat(count, yoyo=, delay=)`, `Anim.loop(...)` y
  `Composition.repeat(...)`.
- `move_along(..., orient=True)` orienta el objeto según la trayectoria y
  `Anim.path_arc(ángulo)` curva un `move_to` o `shift_by`.
- `stagger` acepta `origin=`, `grid=`, `total=` y `easing=` para escalonar
  por distancia; `distribute` reparte valores con el mismo orden.
- `grow_from_edge` y `grow_from_point`.
- `scene.random(seed)` y `scene.noise(...)` dan azar y ruido reproducibles;
  `Updater.wiggle` y `Updater.oscillate` añaden movimiento procedimental que
  se suma a las animaciones.
- `trim()` y `animate.trim(...)` recortan trazos; `animate.glow`,
  `animate.blur` y `animate.shadow` animan efectos.
- `label("nombre")` y `Composition.insert(item, at=...)` colocan animaciones
  respecto de etiquetas; `scene.marker("nombre")` marca instantes de la línea
  de tiempo que el editor muestra y que `gaanim export --from/--to` acepta.

== Texto

- `Text.animator(...)` anima rangos de caracteres, palabras o líneas con
  desplazamiento, opacidad, escala, rotación, desenfoque, espaciado y color.
- `reveal(...)` y `conceal(...)` con máscaras, `blur_in(...)` y
  `tracking(...)`.
- `typewriter(...)` con cursor, `backspace(...)` y `retype(...)`;
  `scramble(...)` y `scramble_to(...)`.
- `selection.animate.marker(...)` subraya como un rotulador; las selecciones
  de texto también tienen `reveal`, `brace` y `annotate`.
- `write(by=, order=)` respeta por fin sus argumentos.
- Las marcas negativas de los ejes usan el signo menos tipográfico;
  `readout` y `variable` aceptan `decimal_separator`; `scene.text.measure`
  acepta `flow` y `line_spacing`; `lang` elige el idioma de la separación
  silábica.

== Transiciones y cámara

- `Transition.wipe`, `clock_wipe`, `iris`, `blinds`, `push` y
  `Transition.morph`; todas las transiciones aceptan `easing=`.
  `Overlay.flash` y `Overlay.light_leak` se superponen al corte.
- `camera.animate.shake(trauma=...)` produce un temblor que decae de forma
  natural.
- `zoom_to` y `frame_to` interpolan el zoom de forma exponencial por defecto.
- Los conectores de `scene.geometry.connector` crecen desde la cola con
  `grow_arrow()`.

== Narración

- `scene.voiceover(clave)` marca bloques cuyo ritmo marca una toma grabada, y
  `scene.live_take(clave)` sustituye las pausas por las que hiciste al
  presentar. Los textos pueden vivir en un guion Markdown.
- El editor graba el micrófono con teleprompter, medidor de nivel y
  normalización con FFmpeg.
- El audio de la vista previa sigue al cabezal al saltar.

== Otros cambios

- `scene.geometry.points(...)` dibuja miles de puntos como un solo objeto.
- `gaanim export --from/--to` exporta un tramo; `gaanim --version` y
  `gaanim export --help`.
- `drive_from_samples(..., "xy")` mueve los dos ejes con pares `(x, y)`, y
  controlar `"x"` y luego `"y"` conserva ambos canales.
- Las secuencias PNG aceptan `%d` o `%0Nd` en el nombre.
- El Inicio del editor permite crear o abrir proyectos, lista los recientes y
  comprueba uv, Python 3.14 y FFmpeg.

== Correcciones

- La exportación usa la misma cámara que la vista previa: el temblor, los
  vínculos, el seguimiento y el encuadre dinámico ya aparecen en los videos.
- `traced_path` se reconstruye correctamente al saltar a otro instante.
- Las cadenas de `Anim` no válidas lanzan `TypeError` o `ValueError` en vez
  de un error interno.
- `gaanim --diff --capture-stops` sin una versión aprobada de pausas captura
  y termina con éxito; varias correcciones en selecciones de texto con
  ligaduras y en varias líneas.

== Al actualizar

- Si un zoom de cámara debe interpolarse de forma lineal, añade
  `interpolation="linear"` a `zoom_to` o `frame_to`.

= 0.3.0

- *Postprocesado:* `PostProcess.shader(código_wgsl)` aplica un shader a toda
  la imagen 2D en el editor, al presentar, en las capturas y en la
  exportación. Se configura para toda la escena con `Scene(post=...)` o por
  segmento con `scene.segment(..., post=...)`.
- *Recarga al cambiar recursos:* guardar una imagen, un SVG, un Lottie, un
  shader o un documento Typst del proyecto recarga la escena (ver
  #link("/guias/proyectos/")[Proyectos y exportación]).
- Los documentos de `scene.text.typst()` tienen el mismo tamaño que el texto
  del cuerpo.
- Los objetos ya no aparecen completos antes de su `write` o `create` tras
  rebobinar.

== Al actualizar

- Si compensabas el tamaño de `scene.text.typst()` con `scale_to(2)` o
  `scale_to(3)`, quítalo.

= 0.2.1 a 0.2.3

- *0.2.3:* Gaanim exige Python 3.14 (exactamente 3.14 en Linux, 3.14 o
  posterior en Windows) y encuentra en Linux los intérpretes instalados con
  uv. Si tu `.venv` usa otra versión, recréalo con `uv venv --python 3.14`.
- *0.2.2:* la recarga en caliente es incremental, a partir del primer
  segmento que cambió. Nueva barra de reproducción para líneas de tiempo
  densas, nuevos diálogos de exportación y nuevo estilo de Presenter View;
  `Space` solo pausa y ya no avanza de paso. Las líneas discontinuas se
  dibujan guion a guion con `create`.
- *0.2.1:* medir texto es mucho más rápido, lo que acelera la recarga de
  escenas con muchas insignias, chips o tarjetas.

= 0.2.0

La versión que separa `Scene` en capacidades y pasa de píxeles a unidades
lógicas.

- `Scene` orquesta el tiempo y la presentación; las fábricas se agrupan en
  `scene.geometry`, `scene.text`, `scene.layout`, `scene.media`, `scene.viz`,
  `scene.slides`, `scene.mechanics` y `scene.assets`.
- Las escenas se componen en un fotograma lógico de 16 × 9; la resolución se
  elige al previsualizar o exportar.
- La reactividad se expresa con `computed()` y entradas explícitas en lugar
  de expresiones simbólicas.
- `scene.cursor` y `scene.stops`; `gaanim --diff --capture-stops`.
- `scene.sections` con agenda y barra de progreso; `--sections` y `--from`
  para ensayar una parte de una presentación.
- Los ejes regeneran sus marcas y remuestrean sus curvas en cada `view_to`, y
  recortan los datos a la ventana; `data_to_scene` para anotaciones.
- `markup=False` en textos; `parts()` acepta un diccionario de nombres;
  `font_dir` y marcado en los temas; `side=` y tipografía en
  `dimension_between`; `grow_arrow` sin deformar la punta.
- Los valores por defecto que seguían en píxeles pasan a unidades de escena.
- Si falta FFmpeg, la exportación lo explica.
- El paquete incluye esta documentación para los asistentes de código.

== Migrar desde 0.1

Gaanim 0.2 convierte `Scene` en el orquestador del tiempo y la presentación.
Las fábricas conservan sus firmas y comportamiento, pero se acceden mediante
una capacidad ligada a la misma escena. No existen alias para los métodos
planos retirados.

=== Unidades lógicas

Las escenas ya no se componen en píxeles. `Scene()` usa un fotograma lógico
centrado de `16×9`; la resolución pertenece al editor o al comando de
exportación.

```python
>>>from gaanim import Scene
# Antes: composición acoplada a 1920×1080
<<< scene = Scene(1920, 1080, margin=60)
<<< circle = scene.geometry.circle(120).move_to(360, -120)

# Ahora: las mismas proporciones en unidades lógicas (escala 1920 / 16 = 120)
scene = Scene(frame=(16, 9), margin=0.5)
circle = scene.geometry.circle(1).move_to(3, -1)
```

`Scene(1280, 720)` produce un `TypeError` de migración. Usa
`Scene(frame=(16, 9))` y elige los píxeles con `--width` y `--height`. Una
salida con aspecto distinto requiere `--fit contain` o `--fit cover`.

=== Equivalencias

#table(
  columns: (1fr, 1fr),
  [*0.1*], [*0.2*],
  [`scene.circle(r)`], [`scene.geometry.circle(radius)`],
  [`scene.equation(...)`], [`scene.text.equation(...)`],
  [`scene.row(children)`], [`scene.layout.row(children)`],
  [`scene.image(path)`], [`scene.media.image(path)`],
  [`scene.parameter(value)`], [`scene.viz.parameter(value)`],
  [`scene.badge(text)`], [`scene.slides.badge(text)`],
  [`scene.force_at(...)`], [`scene.mechanics.force_at(...)`],
  [`scene.assets_dir(path)`], [`scene.assets.assets_dir(path)`],
  [`dot.at(x, y)`], [`dot.move_to(x, y)`],
  [`dot.move_to(x, y)` (animado)], [`dot.animate.move_to(x, y)`],
  [`tracker.animate_to(v)`], [`tracker.animate.set(v)`],
  [`space.animate_view(x, y)`], [`space.animate.view_to(x, y)`],
  [`matrix.scale_by(k)` (álgebra)], [`matrix.scalar_multiply(k)`],
  [`AnimationGroup(a, b)`], [`parallel(a, b)`],
  [`Succession(a, b)`], [`sequence(a, b)`],
  [`LaggedStart(a, b, lag=t)`], [`stagger(a, b, each=t)`],
  [`scene.play(items, lag=t)`], [`scene.play(stagger(*items, each=t))`],
  [`.smooth()`], [`.easing(Easing.SMOOTH)`],
  [`.linear()`], [`.easing(Easing.LINEAR)`],
  [`.spring()`], [`.easing(Easing.spring(stiffness=90, damping=12))`],
  [`.ease("spring")`], [`.easing(Easing.spring(stiffness=300, damping=20))`],
  [`.steps(n)`], [`.easing(Easing.steps(n))`],
  [`scene.play(items, rate="linear")`], [`scene.play(items, easing=Easing.LINEAR)`],
  [`obj.animate.write(0.8)`], [`obj.animate.write().duration(seconds=0.8)`],
  [`obj.at_anchor(x, y, anchor)`], [`obj.move_to(x, y, anchor=anchor)`],
  [`obj.color(value)`], [`obj.fill(value)`],
)

Los métodos inmediatos (como `fill` o `move_to` sin `.animate`) son cortes
reversibles en el cursor actual y no consumen tiempo. `animate` nunca se invoca: es una propiedad de solo lectura. El `Anim`
resultante es una descripción pura y `Scene.play` valida el lote completo antes
de incorporarlo a la línea de tiempo.

`scene.text("Hola")` no cambia. Desde 0.2, `scene.text` es una capacidad
invocable `Typography` que también expone `equation`, `typst`, `measure` y
`code`.

`Easing` y `EasingCurve` se importan desde `gaanim`. Son objetos tipados e
inmutables, por lo que el autocompletado enumera presets y familias y los
nombres desconocidos fallan en vez de degradarse a otra curva.

=== Qué permanece en Scene

`play`, `wait`, `stop`, `fade_out_all`, `segment`, `link`, `reuse`, `persist`,
`release`, `render` y `snapshots` continúan directamente en `Scene`. `canvas`
expone ahora `frame_width`, `frame_height`, `aspect_ratio` y los límites del
área segura; `camera` opera sobre esas mismas unidades lógicas.

=== Expresiones simbólicas

`gaanim_expr`, `_Expr` y `gaanim.math` se eliminaron sin adaptadores. Usa
estas equivalencias:

- `gm.sin(parameter)` pasa a `computed(f, inputs=[parameter])`, con
  `f = lambda value: math.sin(value)`.
- `lambda x: amplitude * gm.sin(x)` pasa a una función que recibe la
  amplitud como argumento, `lambda x, amplitude: ...`, con
  `inputs=[amplitude]`.
- Un `readout` derivado declara sus entradas:
  `scene.viz.readout(area, inputs=[radius])`, con
  `area = lambda r: math.pi * r**2`.
- Extremos, cámara y valores escalares aceptan `float`, `Parameter`,
  `Variable` o `Computed`; pasar un parámetro directamente conserva su
  identidad.
- `plot(function, derivative=2)` pasa a `plot(function, derivative=d2)`,
  donde `d2` es la segunda derivada escrita como función. La derivada debe tener la
  misma firma y las mismas entradas; no hay derivación numérica implícita.
- El tiempo se declara con `inputs=[scene.viz.time]`.

La guía de #link("/guias/reactividad/")[Reactividad] explica el modelo actual.

=== Ejemplo completo

```python
from gaanim import BLUE, Scene

scene = Scene(frame=(16, 9))
circle = scene.geometry.circle(0.96).fill(BLUE)
title = scene.text("Capacidades", role="title")
page = scene.layout.column([title, circle], within="safe", gap=0.24)
scene.play([page.animate.fade_in()])
scene.render()
```
