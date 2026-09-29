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

= 0.7.2

Publicada el 29 de septiembre de 2026. Una caja se puede recorrer, buscar,
animar en cascada y reordenar (`walk`, `find`, `cascade`, `stagger`, `swap`…), y
un solo movimiento del cursor puede escalar, teñir, girar o desvanecer cientos
de elementos con los nuevos falloffs. Las cajas que entran con `grow_*` ya no se
ven antes de su turno y el overlay *Layout* del editor no mezcla segmentos. No
hace falta cambiar tus escenas.

== Cambios

- `box.walk()` devuelve las piezas de una caja a cualquier profundidad, en
  orden de dibujo, con el fondo de cada caja antes de su contenido.
  `box.walk(boxes=True)` incluye también las cajas anidadas.
- `box.stagger(make, each=…)` anima todas esas piezas de una vez como un
  `stagger`: `make` recibe cada pieza y devuelve su animación (o `None` para
  saltarla). Lee #link("/referencia/layout/")[Layout].
- `box.find(type=…, where=…, text=…)` y `box.find_all(...)` buscan entre esas
  piezas; `text` compara con el contenido de un texto (`Text.content`).
- `box.each(make)` aplica un ajuste inmediato a cada pieza y devuelve la caja.
- `box.reveal()` revela una caja con la política que casi toda diapositiva
  pide: fondos y objetos con un fundido, textos que entran deslizándose y
  filetes (cajas sin hijos que dibujan algo) que crecen.
- `gaanim check` y `box.diagnostics()` avisan cuando el contenido de una caja se
  sale de ella o una caja se superpone con la anterior de su fila o columna. Un
  contenedor con recorte (`clip=True`) puede sacar contenido sin aviso.
- `box.cascade(each=…).fade_in()` (o cualquier otra animación) las anima todas
  en cascada, sin escribir la lambda de `stagger`.
- `box.move_child(hijo, a)`, `box.swap(a, b)` y `box.reverse()` reordenan los
  hijos, y con `duration=` se deslizan a su nuevo sitio. En un `stack` la
  posición no cambia qué hijo queda encima: usa `z_index`.
- `Falloff` calcula un valor por instancia a partir de su posición en el grupo
  (`Falloff.index`), su distancia a un objeto o un punto (`Falloff.distance`,
  `Falloff.linear`) o un ruido con semilla (`Falloff.noise`). Se reforma con
  `remap` e `invert` y se combina con `+`, `-`, `*`, `maximum` y `minimum`.
- `grupo.drive("scale", near.remap(1.0, 1.8))` conecta un falloff a un canal de
  cada miembro de un grupo: `scale`, `rotation`, `opacity`, `x`, `y` y, con
  `near.gradient(AZUL, ORO)`, `fill`. `grupo.look_at(cursor)` hace que cada
  miembro mire a un objeto que se mueve, y `clear_drive()` lo termina.
- Se evalúa en Rust por miembro y fotograma, sin llamar a Python: una rejilla
  de 20×12 con tres efectos cuesta alrededor de 0,1 ms por fotograma. Un seek
  cae en el mismo fotograma que la reproducción. Lee
  #link("/referencia/animations/#falloffs")[Falloffs].
- Énfasis nuevos: `animate.blink(3)` parpadea un objeto, `animate.broadcast()`
  lanza ondas concéntricas desde él, `animate.flash_around()` y
  `animate.flash_under()` pasan una ventana brillante por un marco o un
  subrayado (también sobre `texto["palabra"]`), `scene.fx.spotlight(objeto)`
  oscurece todo salvo un hueco alrededor del objetivo y
  `objeto.animated_boundary([AZUL, MORADO, CIAN])` dibuja un marco cuyo trazo
  cambia de color. Lee #link("/referencia/animations/")[Animaciones].
- `box.children`, `box[i]` y la iteración de una caja están tipados como
  `Drawable` en lugar de `Any`.

== Correcciones

- Una caja colocada por el layout, o un hijo suyo, ya no se ve entera antes de
  su turno cuando entra con `grow_from_center`, `grow_from_point`,
  `grow_from_edge` o `spin_in_from_nothing`.
- Un `geometry.connector` con extremos fijos que es hijo de una caja ya se
  coloca en su celda, como `geometry.arrow`, en lugar de dibujarse apilado con
  los demás.
- Una columna con hijos `.item(grow=1)` y altura automática ya cuenta el
  contenido de esos hijos: antes se medía casi sin altura y su contenido se
  desbordaba sobre la caja siguiente.
- Una caja, su fondo o sus hijos que entran con `grow_*` ya no aparecen antes de
  su turno cuando una caja se reorganiza después (`set`, `add`, `remove`,
  `swap`…).
- El overlay *Layout* del editor solo dibuja las cajas del segmento actual: los
  límites de los layouts de otros segmentos ya no quedan visibles.
- `just wheel` (y con él `just build-release-install`) borra las wheels de
  versiones anteriores antes de construir: ya no falla con «Expected one
  universal Gaanim wheel» tras subir de versión.
- `just build-release-install` ya no falla al borrar `gaanim-core` y
  `gaanim-play`, que dejaron de instalarse.

= 0.7.1

Publicada el 29 de septiembre de 2026. Los hijos de una caja se animan con
libertad: el layout decide dónde descansan, no cómo se mueven. Llegan
`text_box` para calcar diseños al píxel, avisos de cajas vacías y un overlay
*Layout* en el editor para inspeccionar cajas y zonas como en las herramientas
de desarrollo de un navegador. Exportar desde el editor ya no bloquea la
reproducción. No hace falta cambiar tus escenas.

== Cambios

- Los hijos de una caja aceptan todas las animaciones de `.animate`:
  `fade_in_from`, `shift_by`, `move_to`, `scale_by`, `rotate_by`,
  `grow_from_edge`, `spin_in_from_nothing`… Parten del sitio que les da la
  caja y no empujan a sus hermanos, como un `transform` de CSS. Si la caja se
  reorganiza después, el hijo conserva su desplazamiento, su escala y su
  giro. Colocar un hijo de forma inmediata (`move_to` sin `.animate`) sigue
  lanzando `LayoutOwnershipError`. Lee
  #link("/guias/layout/")[Animar dentro de una caja].
- `text_box="cap"` o `"ink"` en un texto o en una caja cambia la caja con que
  el layout mide y coloca el texto: de la altura de las mayúsculas a la línea
  base, o ajustada a la tinta. Sirve para calcar diseños en los que las
  mayúsculas tocan el borde. Por defecto sigue siendo `"line"`.
- `scene.layout.check_layout()`, `box.diagnostics()` y `gaanim check` avisan
  de las cajas con fondo o borde que quedan con ancho o alto cero, como una
  barra vacía sin `width="fill"`, e indican su ruta (`column[2] > row[1]`).
- El editor tiene un overlay *Layout* (`O` y después `K`) que inspecciona las
  cajas como las herramientas de desarrollo de un navegador: contornos de
  cajas y zonas, y, bajo el cursor, padding, celdas, márgenes y gaps con sus
  medidas en px.
- Una exportación desde el editor sigue en segundo plano: el diálogo se cierra
  y puedes seguir reproduciendo. La barra de tiempo marca bajo cada segmento lo
  ya exportado, la barra de reproducción muestra el porcentaje y permite
  cancelar, y la terminal muestra la barra de progreso de `gaanim export`.

== Correcciones

- El indicador de progreso de una exportación desde el editor ya no se queda en
  0.
- `animate.move_to(x, y)` sobre un objeto dentro de otro usa coordenadas de
  la escena, no las de su contenedor.

= 0.7.0

Publicada el 29 de septiembre de 2026. Llega un layout nuevo al estilo de CSS,
construido sobre Taffy (el motor de flexbox y grid que usan otros proyectos de
Rust): cajas con padding, gap, bordes, radio, sombra y recorte, estilos
reutilizables, zonas y estructura animada, para componer escenas sin
coordenadas (lee la guía de #link("/guias/layout/")[Layout]). Además, Gaanim
pasa a ser un solo ejecutable, que carga Python solo cuando abre un script, y
Vello dibuja también el 3D con un estilo propio. Lee «Al actualizar»: el layout
cambia de forma incompatible y desaparece la importación glTF.

== Al actualizar

- `scene.layout.item(objeto, grow=1)` pasa a ser `objeto.item(grow=1)`, que
  devuelve el propio objeto.
- `scene.layout.card([...], background=..., padding=...)` pasa a ser
  `scene.layout.box(..., background=..., padding=...)`. Los _ports_ de las
  tarjetas desaparecen: une conectores a los objetos.
- `row`, `column`, `grid` y `stack` reciben los hijos como argumentos
  (`row(a, b, c)`); una lista sigue funcionando porque se aplana. Una cadena
  se convierte en texto.
- `Layout` se llama `Box`; `configure(...)` es `set(...)` y
  `configure_item(...)` es `hijo.item(...)`. `add`, `remove` y `replace`
  aceptan `duration=` para animar el cambio.
- `layout_template` y `scene.layout.template` se sustituyen por
  `@component`: llama a la función directamente, `mi_plantilla(scene, ...)`.
- El texto dentro de una caja se mide por su caja de línea (ascendentes y
  descendentes de la fuente), así que textos del mismo estilo comparten
  altura y línea base. Las fórmulas `$…$` conservan su caja de tinta. Un
  texto puede desplazarse unos píxeles respecto a la versión anterior.
- Si una escena importa modelos glTF, quita las llamadas a
  `scene.media.gltf(...)` y a `animation(...)` de esos modelos.
- Reemplaza todos los archivos de la carpeta de Gaanim y borra `gaanim-core`
  y `gaanim-play`.

== Cambios

- Unidades de diseño: `"24px"` son píxeles de un fotograma de 1080 de alto
  (`Scene(design_resolution=...)`), `"50%"` del padre, `"1fr"` en grids y
  tokens del tema como `"space_md"`.
- Propiedades de caja al estilo CSS: `padding`, `gap`, `margin` (con
  `"auto"`), `justify`, `align`, `align_self`, `grow`, `shrink`, `basis`,
  `wrap`, `border`, `radius="full"`, `shadow`, `clip` y tipografía heredada
  (`font_size`, `color`, `weight`…).
- `BoxStyle` y `scene.layout.classes(...)` para estilos reutilizables con
  `style=` y `class_=`.
- `Box.set(..., duration=...)` y `hijo.item(..., duration=...)` animan
  cualquier cambio de propiedades; `advance=False` encadena cambios en
  paralelo.
- Zonas: `Zones.rows`, `Zones.columns` y `Zones.grid` dividen el área segura,
  una zona o un objeto en regiones con nombre; `objeto.place(zona, ...)` y
  `objeto.animate.place(...)` colocan objetos que siguen siendo libres.
- Ejemplos nuevos: `layout_boxes`, `layout_flex`, `layout_grid`,
  `layout_reflow`, `layout_zones`, `ui_dashboard`, `ui_mobile_app` y
  `ui_lower_third`.
- Gaanim es un solo ejecutable, `gaanim`. Ya no existen `gaanim-core` ni
  `gaanim-play`. El motor está en una biblioteca, `gaanim_engine`, en lugar de
  copiarse en dos ejecutables, y el soporte de Python en otra,
  `gaanim_python_plugin`, que `gaanim` carga solo al ejecutar un script. Al
  actualizar, reemplaza todos los archivos de la carpeta y borra
  `gaanim-core` y `gaanim-play`. En Ubuntu cambia la instalación (ver
  #link("/empezar/instalacion/")[Instalación]).
- El inicio abre proyectos aunque instales Python con Gaanim ya abierto: el
  soporte de Python se carga al abrir el proyecto, no al arrancar.
- Vello dibuja el 3D: proyecta las mallas de triángulos y las líneas 3D con
  la cámara de la escena, las ordena de atrás hacia delante y calcula la luz
  en la CPU con un estilo propio de Gaanim: las primitivas con `Material3D`
  conservan su color, con luz del cielo y del suelo, una luz principal suave
  desde arriba a la izquierda y un brillo leve en los bordes; las caras planas
  tienen un solo color y las curvas, degradados suaves. Las superficies, los
  gráficos 3D y las líneas no reciben luz. Sin búfer de profundidad, la
  geometría que se cruza puede ordenarse mal, y el 3D queda debajo del 2D.
  El contenido 3D no proyecta sombras: `lighting_3d(shadows=...)` se acepta
  pero no tiene efecto (ver #link("/guias/camara-y-3d/")[Cámara y 3D]).
- Se elimina la importación glTF. `scene.media.gltf(...)` lanza
  `NotImplementedError`, `Drawable.animations()` devuelve una tupla vacía y
  `Drawable.animation(...)` lanza `NotImplementedError`. `scene.assets.preload`
  ya no acepta `.gltf` ni `.glb`.
- Los paquetes `.gaanim` graban el contenido 3D, y las vistas previas de
  Presenter View lo muestran. El desenfoque de movimiento, el postprocesado y
  los fondos (también los de shader y degradado) se aplican a las escenas 3D
  y con cámara en perspectiva.

== Correcciones

- El reproductor web ya no se detiene al abrir un `.gaanim` con
  post-procesado o un fondo de shader: comprobar el shader esperaba a la GPU
  bloqueando el único hilo de la página. Ahora la comprobación se resuelve
  entre frames, y el efecto aparece un frame después.
- Un texto declarado con `opacity(0)` vuelve a verse con
  `animate.opacity(1)`: la opacidad ya no se copiaba también a cada glifo.
- `cancel()` y los demás efectos de selección encuentran una cadena entre
  comillas dentro de una ecuación, como `part("x", '"bloques vistos"')`.
- Un `z_index` puesto después de enlazar una propiedad reactiva, por ejemplo
  el nivel de `fill_level` con `computed`, ya no se ignora.
- `slides.brand` no dibuja la identidad en el primer segmento salvo con
  `show_on_cover=True`. El pie empieza en el borde izquierdo del área segura y
  se reduce si no cabe, y la regla ocupa el ancho del área segura.
- `checkmark` dibuja una marca rellena proporcional a su tamaño, en lugar de
  un disco blanco enorme, y un trazo explícito ya no muestra un rombo en la
  esquina interior.
- `create()` y `write()` vuelven a mostrar un objeto que un `fade_out()`
  anterior había dejado con opacidad 0.
- El editor vuelve a completar los parámetros de `scene.text(...)`,
  `scene.text.equation(...)`, `Text.become(...)` y `TextFlow(...)`: los tipos
  `TextRole`, `TextWrap`, `TextAlign`, `TextOverflow` y `TextDirection` vuelven
  a estar en los tipos del paquete.
- `scene.assets.load_project()` sin ruta busca el `gaanim.toml` más cercano
  subiendo desde la carpeta del módulo que la llama, así que funciona desde un
  paquete del proyecto (`capitulo4/estilo.py`) y no solo desde el script de
  entrada. Sin manifiesto, el error indica la carpeta de partida y cómo pasar
  la ruta (#link("https://github.com/PaoloLupo/gaanim/issues/266")[\#266]).
- Los métodos fluidos heredados de `Drawable` (`move_to`, `shift_by`,
  `fill`, `opacity`, `scale_by`, `next_to`…) devuelven el mismo objeto sobre
  el que se llaman, así que un `Readout`, una `Variable`, una `Dimension` o
  cualquier otro tipo compuesto conserva sus partes al encadenar:
  `scene.viz.readout(3.0).move_to(0, 0).number.fill("red")`. `part()` sigue
  devolviendo un objeto nuevo
  (#link("https://github.com/PaoloLupo/gaanim/issues/267")[\#267]).
- `--sections` selecciona segmentos cuyo nombre tiene comas: pasa el nombre
  solo (`--sections "Tiempo, lugar y orientación"`) o escribe sus comas como
  `\,` dentro de una lista. El error de un nombre desconocido lista las
  opciones entre comillas y explica cómo escribir una coma
  (#link("https://github.com/PaoloLupo/gaanim/issues/270")[\#270]).

= 0.6.2

Publicada el 28 de septiembre de 2026. El reproductor web abre el Presenter
View, y los `.gaanim` se reproducen sin saltos de cámara ni tirones. No hace
falta cambiar tus escenas.

== Cambios

- El reproductor web abre el Presenter View: al presentar, abre en otra
  ventana el mismo Presenter View del escritorio, con la diapositiva actual y
  la siguiente, notas, cronómetro, vista general y dock. Las dos ventanas van
  a la par desde cualquiera de ellas: teclas, clics, el dock, la barra de
  reproducción y las pantallas en negro o en blanco. `P` lo vuelve a abrir.
  Ver #link("/guias/compartir/")[Compartir sin Python].

== Correcciones

- Al reproducir un `.gaanim`, la cámara ya no salta frame a frame a la
  posición del inicio: si la pantalla refrescaba más rápido que el paquete,
  cada refresco sin frame nuevo mostraba la cámara de t=0.
- Los `.gaanim` con mucha geometría por segundo, como líneas de corriente,
  se reproducen sin tirones: los frames se decodifican a medida que se
  muestran, no un segundo entero de golpe, así que la preview ya no baja de
  resolución en esos tramos.

= 0.6.1

Publicada el 28 de septiembre de 2026. Llegan el reproductor web, la
licencia MIT OR Apache-2.0, `scene.launch`, la rejilla de tempo y paquetes
`.gaanim` más pequeños, con portada, icono propio y doble clic. Cambian el ancho de los trazos
escalados, la opacidad tras `transform_to` y la colocación de un SVG cuyas
partes mueves: lee «Al actualizar».

== Al actualizar

- El ancho de `.stroke(color, ancho)` se mide siempre en unidades de escena,
  como promete su referencia: `scale_to`, `scale_to_3d`, `matrix_to`, las
  inclinaciones, la escala de un grupo y sus animaciones cambian la forma,
  no el pincel. Antes la escala también ensanchaba el trazo, y una escala
  distinta por eje lo deformaba: `rect(1, 1).stroke(c, 0.03).scale_to_3d(5.5,
  2.8, 1)` dibujaba los lados verticales el doble de gruesos que los
  horizontales. Si una escena contaba con que la escala engrosara el trazo,
  escribe el ancho que quieres ver: `.stroke(c, 0.09)` para el trazo de antes
  con `scale_to(3)`. Una figura que crece desde cero muestra ya su trazo
  completo. Los trazos propios de un SVG importado siguen escalando con su
  `scale_to`, pero no con una animación de escala posterior. El ancho de un
  resplandor (`glow`) también queda en unidades de escena.
- `animate.transform_to(destino)` conserva la opacidad del objeto. Antes
  tomaba la del destino, así que un destino declarado con `.opacity(0)` dejaba
  invisible al objeto. Si usabas un grupo con `.opacity(0)` para esconder los
  destinos, ya no hace falta: se ocultan solos. Para cambiar la opacidad
  durante el morph, combínalo con `animate.opacity`. `replacement_transform_to`
  no cambia.
- Mover una parte de un SVG (`svg.part("g").shift_by(...)`) ya no mueve el SVG
  entero: las partes se colocan después de la raíz, así que `move_to`, la
  escala y el pivote de la raíz usan el dibujo tal como lo declara el archivo.
  Antes `move_to` centraba el SVG con la parte ya desplazada y todo el dibujo
  se movía para compensar; si lo corregías a mano, quita esa compensación.
- Los paquetes `.gaanim` usan el formato 2: sus datos van comprimidos con
  Zstandard. Una presentación larga ocupa menos de la mitad (la de 4½ minutos
  de #link("/guias/compartir/")[Compartir sin Python] baja de 42 a 18 MB) y un
  paquete pequeño, alrededor de un 10 % menos. El cambio rompe la
  compatibilidad en los dos sentidos: 0.6.1 ya no abre los paquetes grabados
  con 0.6.0 (vuelve a grabarlos desde su script con `gaanim export`), y 0.6.0
  no abre los de 0.6.1, así que quien los reciba debe actualizar o usar el
  reproductor web.

== Correcciones

- Acercar o alejar la vista con el modo interactivo al reproducir un `.gaanim`,
  en el escritorio o en el navegador, ya no deja copias de los fotogramas
  anteriores alrededor del marco.
- La ventana de un `.gaanim` se titula con el nombre del proyecto que lo
  grabó, no con el del script (`main`).
- Un tren de engranajes encadenado (`b.bind_rotation_from(a)`,
  `c.bind_rotation_from(b)`...) gira entero en la vista previa. Antes, desde la
  tercera rueda se quedaban quietas hasta hacer un seek o pausar, y con
  `Canvas.motion_blur` dejaban copias dobles: cada eslabón leía la rueda
  anterior con un fotograma de retraso.
- `traced_path` se reconstruye con su recorrido completo al saltar a otro
  instante, también al exportar un fragmento (`--from`), al capturar
  fotogramas sueltos y con `Canvas.motion_blur`, cuando la fuente se mueve con
  animaciones o sigue a un `Parameter` animado. Antes la estela salía vacía o
  desaparecía; `dissipating_time` fallaba igual.
- `Color(GOLD)` o `Color((r, g, b))` devuelven ese color, como cualquier
  parámetro que acepta uno. Los stubs tipan como `ColorLike` los parámetros de
  color que aceptan una cadena o una tupla (`glow`, `Overlay.flash`,
  `mechanics.gear`, `rolling_number`, `marker`...), así que un comprobador de
  tipos ya no marca como error código correcto.
- Una vista de cámara (`camera_view`) cuya pantalla o marco se transforma con
  `transform_to` en otra figura ajusta el aumento y el encuadre a la forma
  nueva. Antes el recorte seguía el contorno nuevo, pero el aumento y el
  encuadre se calculaban con el tamaño original, así que la vista salía
  estirada o miraba otra región.

== Cambios

- Los archivos `.gaanim` se comportan como documentos del sistema:
  `gaanim register` (o el botón *Asociar archivos .gaanim* del inicio) hace
  que el doble clic los reproduzca, añade *Presentar* al menú contextual y
  muestra su portada en el Explorador de Windows y en Nautilus, Nemo, Caja y
  Thunar. `gaanim unregister` lo deshace; ninguno pide permisos de
  administrador. Los `.gaanim` tienen su propio icono: una hoja con un video
  animado vectorial y el logo de Gaanim, que ahora tiene las esquinas
  redondeadas. Ver
  #link("/guias/compartir/")[Compartir sin Python].
- Cada `.gaanim` guarda una portada: el fotograma de la primera pausa o, sin
  pausas, el del primer segmento que más muestra. `scene.thumbnail()` elige
  otro, y `gaanim thumbnail archivo.gaanim portada.png` la extrae sin GPU. Un
  paquete sin portada se abre igual.
- La instalación en Ubuntu copia también `gaanim-play`, que reproduce los
  `.gaanim`.
- `scene.launch(...)`, o `scene.play(..., advance=False)`, empieza animaciones
  en el cursor sin moverlo: un giro que dura varios cortes sigue mientras
  programas lo demás. Animar el mismo canal antes de que termine lanza
  `ValueError`, y `gaanim check` avisa si la escena acaba antes que ella.
  `duration(0)` queda documentado como corte dentro de una composición.
- Rejilla de tempo: `scene.tempo(bpm, offset)`, `scene.beats(n)` y
  `scene.wait_until(beat=n)` o `wait_until(bar=n)` cortan a tiempo con la
  música, y avisan si un plano se pasó de su hueco. El editor dibuja una
  línea en cada compás de la barra de reproducción.
- `scene.media.audio(..., end=30.0, fade_out=1.5)` recorta y funde una música
  de fondo sin alargar el `play` que la activa.
- `echo(..., hold=True)` deja las copias congeladas cuando el objeto se
  detiene, como un papel cebolla, en lugar de que lo alcancen.
- `drawable.points([...])` fija los vértices de un polígono o una polilínea
  sin animarlos, igual que los demás setters.
- Con `role="code"`, `$` es literal: `scene.text("$ gaanim init",
  role="code")` ya no pide escribir `\$`.
- El ejemplo `examples/launch_tempo_demo.py` reúne `launch`, el tempo, el eco
  congelado, `points` y el `$` literal, con su referencia visual.
- Documentación: la guía de Efectos cubre postprocesos encadenados y
  animados, acabados listos, bloom, desenfoque de movimiento, ecos y fondos
  vivos; Movimiento añade repetidores, duplicadores y plexus. La referencia de
  animaciones lista qué animaciones son entradas y cuándo toca suelo
  `Easing.bounce`, y la de `hud()` explica cómo pasa por las transiciones.
- Reproductor web (experimental): abre un `.gaanim` en el navegador de la PC o
  del teléfono, sin instalar nada, con la misma barra de reproducción que el
  escritorio, en
  #link("https://paololupo.github.io/gaanim/reproductor/")[paololupo.github.io/gaanim/reproductor].
  Con `?src=` abre un archivo publicado en otro sitio. Todavía no reproduce el
  audio ni abre la ventana del Presenter View (ver
  #link("/guias/compartir/")[Compartir sin Python]). El sitio de la
  documentación lo enlaza desde la cabecera y la portada.
- Controles táctiles en el reproductor, al estilo de los reproductores de vídeo
  del móvil: un toque muestra u oculta los controles, con botones grandes para
  la pausa anterior, reproducir y la siguiente; dos toques en un lado, o
  deslizar, van a la pausa siguiente o anterior. También funcionan en el
  editor con una pantalla táctil.
- Gaanim tiene licencia: se distribuye bajo MIT OR Apache-2.0, a elección de
  quien lo usa. Los vídeos, imágenes, archivos `.gaanim` y scripts que creas
  con Gaanim son tuyos y no están sujetos a esa licencia. Las descargas
  incluyen `LICENSE-MIT`, `LICENSE-APACHE` y `THIRD-PARTY-NOTICES.txt` con los
  avisos de las bibliotecas y fuentes de terceros.

= 0.6.0

Publicada el 27 de septiembre de 2026. Llega el formato `.gaanim`: una
escena o presentación en un solo archivo que se reproduce, presenta y exporta
en cualquier equipo. No hace falta cambiar tus escenas.

== Cambios

- Paquetes de reproducción: `gaanim export mi-charla --output mi-charla.gaanim`
  graba la escena o presentación completa en un solo archivo que se
  reproduce, presenta y exporta a vídeo sin Python, sin el proyecto y sin sus
  recursos. Los callbacks, updaters y funciones reactivas quedan grabados como
  los fotogramas que produjeron, y un vídeo exportado desde el paquete es
  idéntico, píxel a píxel, al que exporta el script con la misma frecuencia y
  el mismo tamaño. Aún no admite 3D ni clips de vídeo (ver
  #link("/guias/compartir/")[Compartir sin Python]).
- `gaanim mi-charla.gaanim` y `gaanim --present mi-charla.gaanim` abren un
  paquete con el nuevo ejecutable `gaanim-play`, que se instala junto a
  `gaanim` y no enlaza Python. Presenter View muestra sus notas y miniaturas.
- `gaanim export mi-charla.gaanim --output mi-charla.mp4` convierte un paquete
  en vídeo con los formatos y opciones de siempre, y el botón Exportar del
  editor hace lo mismo con el paquete abierto. `--fps` elige la frecuencia al
  grabar un paquete.
- `gaanim check mi-charla.gaanim` vuelve a componer cada fotograma de un
  paquete y lo compara con el resumen grabado, sin Python.
- `gaanim --diff --example mi-charla.gaanim` captura y compara las pausas de un
  paquete, sin Python; coinciden con las de `--capture-stops` sobre el script.
- El inicio de Gaanim tiene la tarjeta Reproducir .gaanim (`Ctrl Shift O`),
  que abre el archivo en la misma ventana; también acepta arrastrarlo. Los
  archivos abiertos, desde el inicio o con `gaanim mi-charla.gaanim`,
  aparecen en Recientes junto a los proyectos. Sin Python instalado, `gaanim`
  abre igualmente el inicio para reproducir archivos `.gaanim`.
- El diálogo Exportar del editor ofrece el formato Gaanim para grabar un
  `.gaanim` del script abierto, con su barra de progreso; en la terminal,
  `gaanim export` muestra la suya.
- El `README.md` de los proyectos nuevos incluye cómo compartirlos como
  paquete.

== Correcciones

- `drawable.bounds()` sobre un objeto recién creado (antes del siguiente
  `play` o `wait`, sin animaciones, grupos posteriores ni layouts que actúen
  sobre él) ya no compila la escena entera: compila solo su declaración y
  tarda alrededor de un milisegundo, con el mismo resultado. En una
  presentación de 41 segmentos con 54 mediciones, `gaanim check` pasa de
  27,7 s a 1,7 s. En los demás casos sigue compilando la escena escrita hasta el
  cursor.
- Las presentaciones largas se reproducen, exportan y graban más rápido. Una
  presentación mantiene vivos los objetos de todos sus segmentos, y cada
  fotograma animado recorría todos ellos varias veces para restaurar unos
  pocos cientos: ahora el seek restaura en una sola pasada y la propagación de
  transformaciones, opacidad y cajas solo recalcula lo que cambió. Grabar el
  paquete de una presentación de 41 segmentos y 29 000 objetos pasa de 31 a 8½
  minutos, con los mismos fotogramas.
- Grabar un paquete ya no repite la línea de tiempo en un segundo mundo cuando
  la escena no tiene estado que dependa de los instantes visitados (updaters,
  trazos acumulados, ecos, squash o animaciones personalizadas).
- En un vídeo, pulsar → (o el botón de escena siguiente) mientras sonaba la
  transición de una escena volvía al inicio de esa misma escena en lugar de
  pasar a la siguiente; ← tampoco reconocía la escena en curso durante la
  transición. La navegación entre escenas ahora compara instantes exactos.

= 0.5.2

Publicada el 27 de septiembre de 2026. No hace falta cambiar tus escenas.

== Cambios

- Nuevas guías de composición en los overlays del editor (`M`): márgenes
  seguros al 90 % y al 80 % y tercios del marco.
- Durante la inspección (`I`), la grilla y los ejes cubren toda la vista y no
  solo el marco de salida. En escenas 3D, la grilla se dibuja sobre el plano
  XZ alrededor del punto que mira la cámara, con el eje X en rojo y el Z en
  azul (ver #link("/referencia/scene/")[Escena]).
- Con la inspección activa, la barra de overlays muestra el zoom de la vista,
  en azul cuando ya no coincide con la cámara de la escena, junto a un botón
  para restablecerla (`R`). El icono de teclado lista todos los atajos.
- El editor recuerda qué overlays dejaste activados entre sesiones; el modo
  en sí sigue empezando oculto.

== Correcciones

- En escenas 3D, el recuadro de selección y las coordenadas de los overlays se
  calculaban sobre el marco de salida en lugar de la ventana, así que quedaban
  desplazados respecto al objeto.

= 0.5.1

Publicada el 27 de septiembre de 2026. No hace falta cambiar tus escenas.

== Cambios

- La barra de overlays (`O`) y sus guías usan el mismo estilo que el resto del
  editor, con iconos, colores de la paleta y los atajos en cada botón. El
  punto bajo el cursor se ajusta a la grilla con Mayús, se copia con `Ctrl+C`
  como tupla lista para pegar y se oculta sobre los paneles. El objeto
  seleccionado se enmarca con su centro y su tamaño en unidades (ver
  #link("/referencia/scene/")[Escena]).
- En la inspección 2D (`I`), un clic sin arrastrar selecciona el objeto bajo
  el cursor, y la rueda acerca hacia el punto bajo el cursor en lugar del
  centro de la vista.
- El panel de error del script usa el estilo del editor: muestra arriba la
  excepción y el archivo y la línea donde se produjo, y debajo el traceback
  completo con el botón *Copiar*. El aviso de recarga también adopta el estilo
  del editor.

- La previsualización del editor ajusta su resolución para mantener la
  fluidez: si al reproducir no llega a 60 fps porque dibujar la escena cuesta
  demasiado, pasa al 75 % y luego al 50 % de los píxeles, y al pausar vuelve
  a la resolución completa, así que un fotograma detenido siempre se ve
  nítido. Si bajarla no acelera la reproducción (el límite está en la escena,
  no en el dibujo), deshace el cambio. El panel de `F12` muestra la
  resolución actual y `GAANIM_PREVIEW_RESOLUTION=full` la mantiene siempre
  completa (ver #link("/referencia/cli/")[Línea de comandos]). Las
  exportaciones no cambian.
- La previsualización ya no vuelve a rasterizar un fotograma idéntico al que
  muestra: en pausa, durante un `scene.wait()` o en una parada de la
  presentación, la GPU queda libre y el editor responde con más holgura. Los
  fotogramas con postprocesado o con un fondo de shader se siguen dibujando
  siempre, porque cambian fuera de la escena vectorial.
- Un objeto semitransparente pintado con un único color sólido (solo relleno o
  solo trazo; por ejemplo, las líneas de `connect` o las copias de un `repeat`
  con `opacity=(a, b)` cuando la figura solo tiene relleno) recibe la opacidad
  en su color en lugar de abrir una capa de opacidad. La imagen es la misma y
  dibujarlo cuesta mucho menos: con 89 círculos semitransparentes en
  movimiento y render por software, cada fotograma pasó de 152 a 41 ms. Se
  aplica a la previsualización y a las exportaciones. Los objetos con relleno
  y trazo a la vez siguen usando una capa, que evita que el trazo se mezcle
  con el relleno.
- `animate.custom` reutiliza el último resultado de tu función cuando se le
  pide el mismo progreso: como la función debe ser pura, los clips que ya
  terminaron (y los glifos de un texto que comparten la animación) no vuelven
  a llamar a Python en cada fotograma.
- Un número rodante en reposo ya no marca su contorno como cambiado en cada
  fotograma, así que el renderer no vuelve a compararlo.
- El editor muestra el primer fotograma algo antes: la previsualización solo
  prepara el antialiasing que usa.

== Correcciones

- En la inspección 2D del editor (`I`), arrastrar desplazaba la escena mucho
  más que el cursor: con el marco por defecto de 16 × 9 unidades, un píxel
  movía más de una unidad. Ahora la escena sigue al cursor al mismo ritmo en
  cualquier marco, zoom o rotación de cámara, y `W`, `A`, `S`, `D` la
  desplazan a una velocidad constante en pantalla.
- El arrastre ya no cuenta el mismo movimiento dos veces (el cursor y el
  movimiento bruto del ratón) ni salta al volver a entrar en la ventana.
- En la inspección 2D, la rueda iba al revés que en 3D: hacia arriba alejaba.
  Ahora en ambos casos hacia arriba acerca.
- La selección con el ratón ya funciona con las figuras 2D de las escenas de
  Python, que antes no se podían seleccionar.
- En la inspección, el clic que empieza un arrastre ya no avanza a la
  siguiente parada, y las flechas solo navegan entre paradas y escenas en
  lugar de mover también la cámara. Arrastrar o usar la rueda sobre la línea
  de tiempo, los paneles o la barra de overlays ya no mueve la cámara.

= 0.5.0

Publicada el 27 de septiembre de 2026. Cambian el pivote por defecto de las
figuras declaradas con coordenadas de escena, la entrada y la salida de las
vistas de cámara, los anchos de trazo fijados en un SVG y la separación de las
etiquetas de cota: lee «Al actualizar».

== Al actualizar

- Las figuras declaradas con coordenadas de escena (`line`, `polygon`,
  `polyline`, flechas, arcos, `curved_arrow_arc`, curvas, llaves y cotas
  estáticas) giran, escalan y se sesgan alrededor del centro de su caja cuando
  no tienen `with_pivot`. Antes su pivote era el origen de la escena, así que
  `arco.animate.rotate_by(...)` lo hacía orbitar alrededor de `(0, 0)` en vez
  de girar sobre sí mismo. Si una escena contaba con ese giro alrededor del
  origen, añade `.with_pivot(0, 0)`; si ya usabas `with_pivot` como remedio,
  puedes quitarlo cuando el pivote era el centro de la figura. Las figuras
  centradas en su origen, como `circle`, `rect` o `text`, no cambian.
- `CameraView.animate.pop_in()` es la salida simétrica de `pop_out()`:
  al llegar a la región oculta la pantalla y, en un `scene.camera.inset`,
  también el marco y los conectores; el siguiente `pop_out()` los vuelve a
  mostrar. Antes la pantalla seguía visible sobre la región (con una capa de
  vista, su contenido se veía encima de la escena) y el marco quedaba
  dibujado. Si los fundías tras `pop_in`, ya no hace falta; el fundido sigue
  funcionando sin parpadeos.
- Un `pop_out()` que hace de entrada deja la pantalla, y en un inset el marco
  y los conectores, ocultos hasta que empieza. Antes la pantalla esperaba
  visible sobre la región y el marco y los conectores de un inset se veían
  desde su declaración. Si querías ver el marco antes, anímalo aparte o usa
  `pop_out()` más tarde.
- El ancho fijado con `.stroke(color, ancho)` en un SVG importado, o en una
  de sus partes, se mide en unidades de escena aunque el SVG tenga
  `scale_to` o `scale_by`, igual que al animarlo con `animate.stroke(...)`.
  Antes se multiplicaba por esa escala, así que `.stroke(c, 1.6)` con
  `scale_to(0.58)` dibujaba 0.93 unidades mientras que el mismo ancho animado
  dibujaba 1.6. Escribe el ancho que quieres ver: `.stroke(c, 0.0093)` para
  el trazo fino de antes. Los trazos propios del archivo siguen escalando con
  el dibujo.
- `label_gap` de `mechanics.dimension_between` es la separación entre la
  línea y el borde más cercano de la anotación. Antes se medía hasta su
  centro, así que con los valores por defecto la etiqueta montaba sobre la
  línea. Si subías `label_gap` para despegarla, bájalo o quítalo.
- `scene.text.measure(...)` desaparece: `drawable.bounds()` mide cualquier
  objeto ya creado. Crea el texto y mide su caja
  (`scene.text("PGA", role="label").bounds().width`) en lugar de repetir su
  contenido y estilo. Desde 0.6.0, medir un objeto recién creado cuesta
  alrededor de un milisegundo; antes de esa versión cada llamada compilaba
  toda la escena escrita hasta ese punto.

== Cambios

- `drawable.bounds()` devuelve la caja de cualquier objeto (texto, fórmulas,
  SVG, imágenes, grupos, figuras transformadas) en unidades de escena y en el
  cursor actual, con `x`, `y`, `left`, `right`, `bottom`, `top`, `width`,
  `height` y `center`. Sirve para tachar un valor o ajustar una caja a su
  contenido sin repetir el texto.
- `scene.stop("nombre", loop=anim)` añade un bucle ambiental: mientras la
  presentación descansa en la pausa, la animación se repite en lugar de
  congelar la imagen, así que un movimiento continuo sigue mientras hablas. El
  siguiente paso sale del bucle; al exportar se reproduce una vez.
  `scene.stops` informa de su duración en `loop_duration`.
- `scene.camera.inset(..., aspect=)` o `size=(ancho, alto)` da a un inset
  rectangular su propia proporción, para encuadrar entero un panel alto o una
  barra de herramientas en lugar de recortarlo a 16:9.
- `imagen.pixel(px, py)` convierte un píxel del archivo original (desde la
  esquina superior izquierda) en un punto de anclaje que sirve en `move_to`,
  `scene.camera.inset`, `pan_to` o conectores y sigue a la imagen.
- `drawable.matrix_to(((a, b), (c, d)))` y `animate.matrix_to(...)` aplican
  una transformación lineal 2D cualquiera alrededor del pivote, por ejemplo
  para llevar una cara a una proyección isométrica sin trocearla.

- Cámaras secundarias: `pantalla.camera_view(marco)` convierte una figura
  cerrada en una pantalla que muestra lo que ve una segunda cámara, como un
  _picture-in-picture_ o una lupa, y devuelve un `CameraView`. El marco es la
  cámara: su centro marca adónde mira y su giro gira la vista, y sin `zoom=`
  su tamaño fija el aumento. La vista se recorta al contorno de la pantalla,
  entre su relleno y su trazo, y admite `fit`, `background`, `exclude` y
  `layers`; `no_camera_view()` la quita.
- `CameraView` tiene el vocabulario de `scene.camera`: `pan_to`, `zoom_to`,
  `rotate_to`, `follow` y `animate`. Con `zoom=` (un número, un `Parameter`
  o un `Computed`) el aumento es explícito, `animate.zoom_to` lo cambia a
  velocidad percibida constante y `view.zoom` lo expone para lecturas; sin
  marco, `center=` elige adónde mira. `animate.pop_out()` hace brotar la
  pantalla de la región que ve su cámara, sin saltos, y `animate.pop_in()` la
  devuelve.
- `scene.camera.inset(objetivo, zoom=, at=)` monta de una vez la pantalla, un
  marco que recuadra lo que muestra y dos conectores. Con `follow=True` el
  marco persigue a un objeto en movimiento y con `fixed=True` la pantalla se
  queda fija en la imagen aunque la cámara principal se mueva.
- `drawable.view_layer("nombre")` saca un objeto de la cámara principal: solo
  lo muestran las vistas que listan esa capa en `layers=`, por ejemplo una
  lupa de rayos X.

Ver `Drawable.camera_view` en #link("/referencia/drawable/")[Drawable], las
vistas de cámara en #link("/referencia/scene/")[Escena] y las cámaras
secundarias en #link("/guias/camara-y-3d/")[Cámara y 3D].

- `drawable.blend("screen")` cambia cómo se funde un objeto con lo que hay
  debajo: `"screen"` y `"add"` para luces y destellos, `"multiply"` para
  resaltadores, y el resto de modos habituales (`"overlay"`, `"soft_light"`,
  `"difference"`…).
- `animate.dash_offset(valor)` anima el desplazamiento de los guiones de un
  trazo, y `Updater.dash_flow(speed=)` los hace fluir sin fin, como hormigas
  en marcha o el flujo de una tubería. Ambos respetan los seeks y la
  exportación.
- `drawable.tip(end="arrow", start="dot")` pone puntas de flecha o puntos en
  los extremos de cualquier trazo (polilíneas, curvas, arcos). Siguen al
  extremo actual del camino, así que `animate.grow_arrow()`, `create()` y
  `trim` las llevan consigo.
- `scene.viz.progress_ring(0.0)` crea un anillo de progreso con un porcentaje
  rodante en el centro; se anima con `ring.animate.set(0.75)`.
  `scene.viz.countdown(10)` crea un temporizador que `timer.count_down()`
  vacía en tiempo real.

- `PostProcess.shader(src, uniforms={"amount": parametro})` enlaza valores
  del shader a un `Parameter`, así que el efecto se anima como cualquier otro
  valor. `scene.canvas.post` (y `post=` de `Scene` y de los segmentos) acepta
  una lista de pasadas que se encadenan en orden.
- Presets de acabado sin escribir WGSL: `PostProcess.grain`, `vignette`,
  `chromatic_aberration`, `color_grade`, `lut` (archivos `.cube`), `halftone`,
  `dither`, `crt`, `pixelate` y `glitch`. Sus valores también aceptan un
  `Parameter`.
- `PostProcess.bloom(threshold, intensity, radius)` hace brillar lo que supera
  el umbral con una cadena de mips, igual en el visor y en la exportación.
- Fondos vivos: `Background.mesh_gradient`, `noise_gradient`, `aurora` y
  `dot_grid`. Los shaders de fondo propios pueden llamar a
  `gaanim_frame_size(resolution)` para trabajar en unidades de la escena.
- `drawable.echo(5, delay=0.04, decay=0.6)` deja copias que siguen al objeto
  en el tiempo; son exactas en cualquier búsqueda y también salen en SVG.
- `animate.points([...])` mueve cada vértice de un polígono o una polilínea en
  línea recta a su nueva posición, sin remuestrear el contorno: con `echo`
  reproduce el símbolo de Gaanim fotograma a fotograma.
- `drawable.squash_stretch(0.1)` estira un objeto en la dirección de su
  velocidad y lo aplasta en la perpendicular, conservando el área.
- `drawable.stroke_profile([...])` y `stroke_taper(start, end)` dan trazos de
  grosor variable que se afinan en los extremos, también en `create` y
  `show_passing_flash`.
- `scene.geometry.repeat(...)` y `scene.geometry.duplicate(shape,
  Distribution.grid/circle/along/random/phyllotaxis(...))` crean copias
  agrupadas de una figura; `grupo.animate.count(n)` las hace aparecer una a
  una, fundiendo la siguiente con la parte fraccionaria, y
  `scene.geometry.connect(...)` une puntos con líneas vivas que se desvanecen
  con la distancia (plexus).
- `scene.canvas.motion_blur(180, samples=8)` difumina el movimiento en las
  exportaciones y capturas promediando subcuadros; `drawable.motion_blur(False)`
  mantiene nítido un objeto.

Ver `Drawable.blend`, `Drawable.tip`, `Drawable.echo` y `Drawable.motion_blur`
en #link("/referencia/drawable/")[Drawable], `Anim.dash_offset`, `Anim.points`, `Anim.count` y `Updater.dash_flow` en
#link("/referencia/animations/")[Animaciones], los anillos en
#link("/referencia/visualization/")[Visualización] y los fondos, el
postprocesado y `Canvas.motion_blur` en #link("/referencia/themes/")[Colores y temas].

== Correcciones

- `.hud()` fija de verdad el objeto a la imagen también en 2D: antes se movía
  con `scene.camera` al desplazarla, acercarla o girarla. Las escenas con la
  cámara 2D quieta se ven igual.
- `.hud()` funciona en figuras simples como rectángulos, círculos o
  polígonos, que ahora se dibujan encima de la escena como el resto de
  superposiciones; antes la llamada no tenía efecto en ellas.
- `grow_from_center()` crece desde el centro de la caja aunque la figura se
  declare con coordenadas absolutas, como una línea entre dos puntos o un
  polígono de vértices fijos. Antes escalaba desde el pivote, que en esas
  figuras es el origen de la escena, y la figura viajaba desde `(0, 0)` hasta
  su sitio mientras crecía. Ya no hace falta `with_pivot` como remedio.
- `grow_from_edge` y `grow_from_point` mantienen fijo el punto indicado también
  cuando el objeto tiene un pivote propio con `with_pivot`.
- Tras `grow_from_edge` o `grow_from_point`, un `scale_by` o `scale_to`
  posterior parte del tamaño declarado en lugar de escala cero.
- `scene.mechanics.dimension_between` se ve desde su declaración, como
  cualquier otro objeto sin animación de entrada. Antes la línea y las
  extensiones solo aparecían con una entrada en `scene.play` y, si tenía
  etiqueta, esta se veía sola. Con `fade_in` o `create` sigue oculta hasta
  que la entrada empieza.
- Un `rotate_by` con pivote (`with_pivot` o `.pivot(x, y)`) sobre un objeto ya
  girado empieza desde su pose actual; antes saltaba al empezar el giro.

= 0.4.2

Publicada el 26 de septiembre de 2026. No requiere cambios en tus escenas.

== Cambios

- `drawable.skew_to(x, y)` y `drawable.animate.skew_to(x, y)` sesgan un
  objeto o un grupo entero alrededor de su pivote, sin trocearlo: `x` desplaza
  cada punto en horizontal `x` veces su altura sobre el pivote, e `y` en
  vertical `y` veces su distancia horizontal a él. Con `with_pivot` en la base,
  `skew_to(0.15, 0)` inclina una vivienda como el corte de un pórtico en un
  sismo. Ver `Drawable.skew_to` en
  #link("/referencia/drawable/")[Drawable].
- `gaanim --diff` ya no abre un visor nativo: al comparar escribe
  `report/index.html`, que imprime al terminar y se abre en el navegador sin
  conexión. El informe añade una lista filtrable con miniaturas, siete modos de
  comparación (diff, versión aprobada, actual, lado a lado, cortina,
  superposición y parpadeo), un recuadro sobre el área cambiada, zoom con
  píxeles nítidos, atajos de teclado y enlaces a un fotograma concreto. Ver
  #link("/guias/capturas-y-comparacion/")[Capturas y comparación visual].
  La opción `--no-gui` desaparece y ahora es un error: quítala de tus scripts
  y de la integración continua.
- El ejecutable ya no incluye la interfaz, los sprites, el texto ni los gizmos
  de Bevy, que Gaanim no usaba: el lienzo de Vello se compone en la ventana con
  un pase propio. Las escenas se ven igual; solo puede cambiar qué línea 3D
  queda encima cuando dos líneas transparentes coinciden exactamente.

== Correcciones

- `Transition.zoom_through` muestra solo el segmento saliente mientras la
  cámara se acerca y solo el entrante mientras se aleja; antes los dos se
  veían durante toda la transición. El zoom se centra en `center`, que antes
  se ignoraba.
- `gaanim check` y `gaanim --diff` muestran en Windows las rutas sin el
  prefijo `\\?\`.
- `gaanim check` y la vista previa ya no pueden quedarse colgados buscando
  Python: el gestor de instalación de Python de Windows ya no intenta instalar
  una versión que falta, y cada comprobación se abandona a los 10 segundos.

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
  natural te sirve. En esta versión el ancho fijado con `.stroke(color,
  ancho)` escalaba con `scale_to`, así que también había que dividirlo entre
  100; a partir de la siguiente se mide en unidades de escena (ver «Sin
  publicar»).
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

- En la línea de tiempo del editor, la escena actual se distingue con el
  color de acento, un subrayado y un contorno. La escena bajo el cursor se
  ilumina, y queda contorneada cuando un clic saltaría a su inicio. Una línea
  vertical separa cada escena de la siguiente en los carriles de escenas y
  de reproducción.
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
