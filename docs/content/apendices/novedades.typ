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

= 0.15.0

Sin publicar. Los gráficos declarativos dibujan tartas y donuts con la marca
`arc`, que se abren porción a porción al entrar y reparten sus porciones por
clave al cambiar de datos, y la paleta categórica se elige: el tema la fija
para toda la escena y cada campo puede tener la suya. El 3D se rasteriza en
la GPU: se ve mejor y deja de ralentizar las escenas densas, y suma toros,
flechas, tubos y texto en un plano. Lee «Al actualizar».

== Gráficos

- `ChartSpec.mark("arc")` dibuja una porción por fila según su parte del
  total del canal `theta`, en el sentido del reloj desde las 12. Sus opciones
  son `inner_radius` (un donut), `pad_angle` (la separación entre
  porciones), `start_angle`, `radius` y `label_color`; el canal `label`
  escribe cada fila en el centro de su porción y un campo `radius` dibuja un
  gráfico de rosa.
- `chart.animate.grow()` hace crecer la tarta: una manecilla gira desde el
  ángulo inicial, cada porción se abre en su sitio cuando la manecilla la
  cruza y su etiqueta aparece al completarse.
- `chart.animate.to(...)` entre dos gráficos de arcos lleva cada porción a la
  de su clave barriendo sus ángulos, en lugar de deformar su contorno; las
  porciones que salen se cierran y las que llegan se abren.
- `Palette` reparte colores entre categorías: una incluida por nombre
  (`gaanim`, `tableau10`, `okabe_ito`, `set2`, `dark2`, `paired`, `tab10`…),
  tus propios colores o los de un `ColorMap`, con `reversed()` y `rotate()`.
- `Theme(palette=...)` colorea las categorías de todos los gráficos, leyendas,
  tartas y carreras de barras, y `scene.canvas.palette()` la devuelve para
  tus propios objetos. `Scale.category().colors(...)` acepta también una
  `Palette` o un nombre.
- Con más categorías que colores, la paleta vuelve a empezar algo más clara
  u oscura en cada vuelta, en lugar de repetir colores.
- `scene.viz.chart(spec, width=..., height=...)` fija el área del gráfico sin
  encoger el texto, como `cartesian_2d`; `chart.animate.to(...)` la conserva.
- Las celdas de un `heatmap` miden, por defecto, el paso entre valores de `x`
  y de `y`, así que columnas separadas más de una unidad ya no dejan huecos;
  `cell_width` y `cell_height` lo fijan a mano. Las celdas iguales ya no
  muestran costuras finas entre filas.
- `plane.point_ref(x, y)` es un punto en coordenadas de datos que sigue
  valores reactivos (un `Parameter` movido por `drive_from_samples`, un
  `Computed`), la ventana de `view_to` y la posición del plano.
- Un eje `log` marca también su última potencia de la base:
  `Axis.log(1, 1000)` perdía el 1000. Un eje `symlog` marca el 0 y las
  potencias de la base a cada lado (−100, −10, −1, 0, 1, 10, 100) en lugar de
  pasos lineales que se amontonaban en sus extremos; `ticks(step)` sigue
  fijando un paso lineal.

== Animación y geometría

- `drawable.animate.wipe(direction)` revela un objeto detrás de un borde
  recto que cruza su caja, por ejemplo las marcas de un gráfico de izquierda
  a derecha, y `drawable.wipe(reveal)` mueve ese borde con un `Parameter` o
  un `Computed`, al ritmo de una simulación. El borde sigue al objeto si se
  mueve o cambia de tamaño.
- `drive_from_samples(..., absolute=True)` toma las muestras de `x`, `y`,
  `xy` y `rotation` como posiciones y ángulos propios, sin restar la pose
  inicial.
- Una serie de `drive_from_samples` no toca su canal antes del cursor donde
  se declara: un objeto que entra con `fade_in()` y después tiene su
  `opacity` conducida por una serie se ve fundir. Volver a conducir el mismo
  canal más adelante toma el relevo desde el nuevo cursor en lugar de borrar
  la serie anterior.
- `scene.geometry.group(a, b)` acepta los miembros sueltos además de una
  lista.
- `deg(90)` convierte grados a los radianes que piden los ángulos, como en
  `rotate_to_3d(deg(-30), deg(45), 0)`. Con un `Parameter`, un `Computed` o
  `scene.time` devuelve un `Computed` que lo sigue.
- `space.parametric_surface(f, u_domain, v_domain)` dibuja superficies
  paramétricas `(u, v) → (x, y, z)` en un espacio 3D. Con `inputs` reactivas
  la malla se vuelve a muestrear y una superficie se dobla en otra;
  `color(u, v)` pinta cada punto con un valor fijo del material (o uno
  reactivo con `color_inputs`) y `material` la ilumina por las dos caras.
  Con `vectorized=True` las funciones reciben arrays de numpy con toda la
  rejilla y se llaman una vez en lugar de una por vértice. Lo muestra
  `gallery/catenoid_helicoid`.
- Las superficies reactivas (`surface` y `parametric_surface`) se vuelven
  a muestrear solo cuando cambian sus entradas, no en cada fotograma.
- `space.surface(..., vectorized=True)` calcula toda la rejilla con una
  llamada con arrays de numpy, como `parametric_surface`, en lugar de una
  llamada de Python por vértice.
- `space.surface(..., material=Material3D.matte())` ilumina la superficie
  por las dos caras con `lighting_3d`, como `parametric_surface`: conserva
  sus colores por altura, el relieve se ve por la luz y proyecta y recibe
  sombras. Lo muestra `3d/plots_3d`.

== Tiempo y texto

- `scene.wait_until(seconds=t)` lleva el cursor a un segundo absoluto de la
  línea de tiempo, como `beat=` y `bar=` lo llevan a la rejilla de tempo. Si
  el cursor ya pasó `t`, lanza `ValueError` y dice cuánto se alargó lo
  anterior. Sirve para colocar subtítulos en instantes que calcula una
  simulación.
- `Theme(font_files={clave: archivo})` avisa con un `UserWarning` cuando la
  clave no es la familia que declara el archivo, y nombra esa familia: el
  texto solo encuentra la fuente por ella.

== 3D

- El 3D se rasteriza en la GPU, con búfer de profundidad y antialiasing MSAA,
  en lugar de proyectar cada triángulo en la CPU y dibujarlo con Vello. Una
  malla o una línea se sube una vez cuando cambia, así que el coste de un
  fotograma depende de cuántos objetos 3D hay y no de cuántos triángulos o
  puntos tienen. En las medidas de referencia, `gallery/surface_plot`,
  `3d/plots_3d`, `3d/materials_3d` y el atractor de Lorenz de
  `gallery/lorenz_attractor`, 8 curvas de 6.500 puntos, pasan de 40–62 fps
  en su peor segundo a la frecuencia de la pantalla (144 Hz).
- La luz de las primitivas con `Material3D` se calcula en cada píxel y en
  luz lineal: los brillos son redondos y nítidos y las luces puntuales se
  reflejan en su sitio.
- Las superficies y las líneas que se cruzan se cortan donde corresponde, y
  la geometría que atraviesa el plano cercano se recorta en lugar de
  desaparecer.
- El 3D se dibuja igual en la vista previa, las exportaciones, los paquetes
  `.gaanim`, el reproductor web y las vistas previas de Presenter View.
- Sombras: las mallas opacas proyectan sombra de la luz principal, o de las
  cuatro primeras luces de `light_3d`, sobre lo que recibe luz, con un borde
  nítido al contacto que se suaviza con la distancia. `lighting_3d(shadows=...)`, que se aceptaba sin efecto, las
  enciende (por defecto) o las apaga.
- Una malla iluminada abierta o de dos caras, como una superficie con
  `material` o un cono sin tapa, no se da sombra en la cara que aparta de
  la luz, como ya pasaba con los sólidos: su sombra no acaba en un borde
  escalonado ni la salpican puntos. Se nota en
  `gallery/catenoid_helicoid`.
- Contornos: `Material3D(..., outline=color, outline_width=...)` o
  `material.outlined(color)` dibujan la silueta y los pliegues de una malla, o
  el borde de un plano, en ese color, como el contorno de una ilustración;
  se animan con el material.
- Una línea 3D mide el ancho de su trazo, `.stroke(color, ancho)`, en lugar
  de 1,5 px fijos, y `animate.stroke_width(...)` lo anima. Sin trazo mide
  2 px de una salida de 1080 px de alto: en 4K es igual de visible que en
  1080p.
- Una línea 3D que se dibuja con `create` se guarda una vez en los paquetes
  `.gaanim`, que de cada fotograma solo apuntan hasta dónde se ve, y se
  sube una vez a la GPU mientras crece. El paquete del atractor de
  `gallery/lorenz_attractor` pasa de 5,9 MB a 1,5 MB.
- Las mallas 3D iguales se suben una vez a la GPU, y las opacas con la
  misma geometría y el mismo material se dibujan juntas, en una llamada: una
  escena con 8.000 esferas sueltas pasa de 21 a 69 fps.
- Los números y los títulos de los ejes de `scene.viz.cartesian_3d` se ven:
  miden lo que los de los ejes 2D (los estilos `axes/numbers` y
  `axes/labels` del tema) y miran a la cámara. Antes medían menos de un
  píxel. En la esquina donde empiezan los tres ejes solo se escribe el
  número del primero que los muestra, para que no se monten. Cada número
  guarda en pantalla la misma distancia a su eje que en 2D, con la cámara
  cerca, lejos o mirando a lo largo del eje, en el lado que la caja deja
  libre; cada título queda pasado el final de su eje y de sus números.
- Las etiquetas con `.billboard()` y los números y títulos de los ejes 3D
  viven en el mundo 3D: las dibuja el pase 3D, de frente a la cámara y del
  tamaño de su texto, y las mallas y las líneas que tienen delante las
  tapan píxel a píxel. Se acercan a la cámara la mitad de su lado mayor,
  así que una superficie en la que se apoyan no las corta, y los números
  que se ven a través de la caja de sus ejes quedan justo delante de su
  rejilla, detrás del gráfico, que los tapa. Las que contienen una imagen,
  una escena Lottie o un efecto 2D se dibujan encima del 3D y se desvanecen
  enteras tras las mallas opacas.
  Con la cámara en perspectiva aparecen en su punto 3D; antes caían fuera
  de la imagen.
- Nuevas primitivas iluminadas: `scene.geometry.torus(...)` crea toros;
  `scene.geometry.arrow_3d(start, end)`, flechas sólidas entre dos puntos
  para los vectores de una escena 3D, con el astil y la punta proporcionales
  a su longitud, y `scene.geometry.tube(points, radius)`, tubos a lo largo de
  una polilínea que no se retuercen, abiertos con tapas o cerrados en lazo.
- `scene.geometry.flat_3d(source)` convierte el relleno de un texto, una
  ecuación o un dibujo 2D en una lámina iluminada de dos caras que se coloca
  con `move_to_3d` y `rotate_to_3d`: texto en el suelo, en una pared o en la
  cara de un sólido. El ejemplo `3d/shapes_3d` muestra las cuatro.
- `drawable.in_3d()` coloca un objeto 2D (texto, ecuaciones, formas,
  grupos) en el mundo 3D sin dejar de ser el mismo objeto: `move_to_3d` y
  `rotate_to_3d` lo sitúan, la perspectiva lo inclina y las mallas de
  delante lo tapan píxel a píxel. El pase 3D lo dibuja desde sus curvas, así
  que sus bordes siguen nítidos a cualquier tamaño, y conserva sus
  animaciones, colores, degradados y trazos. Proyecta sombra como una
  malla opaca, y con `in_3d(material)` se ilumina y recibe sombras. Tapa lo
  translúcido que tiene detrás, como una superficie opaca, donde es al
  menos medio opaco. Lo muestra `3d/drawables_in_3d`.
- `trim()` y `animate.trim(...)` recortan también las líneas 3D de
  `polyline_3d`, que antes se quedaban enteras. Cuentan segmentos, no
  longitud: una línea de muestras tomadas a intervalos iguales de tiempo,
  recortada de `end=0` a `end=1` con `Easing.LINEAR`, crece con su extremo
  sobre el objeto que la traza. Lo muestra `gallery/heavy_top`.
- `scene.camera.animate.orbit(..., dolly=0.8)` gira alrededor del objetivo
  mientras se acerca o se aleja, en un solo movimiento: `orbit` y `dolly` no
  se pueden reproducir a la vez.

== Editor

- Al reproducir un paquete `.gaanim`, la barra de overlays, la paleta de
  comandos y la hoja de atajos dejan fuera el layout y el panel de
  animaciones, que el paquete no guarda, y el inspector muestra la cámara
  grabada.

== Al actualizar

- El 3D cambia de aspecto: bordes con MSAA en lugar del antialiasing de
  Vello, luz por píxel y orden por profundidad. Vuelve a aprobar las
  capturas de referencia de tus escenas 3D.
- Las escenas 3D iluminadas tienen sombras. Para el aspecto anterior, usa
  `lighting_3d(..., shadows=False)`.
- Una etiqueta con `.billboard()` ya no queda encima de todo el 3D: lo que
  tiene delante la tapa píxel a píxel. Si debe verse siempre, fíjala a la
  imagen con `.hud()` o acércala a la cámara.
- Las líneas 3D con `.stroke(color, ancho)` miden ese ancho; quita el ancho
  o ajústalo si las querías de 1,5 px. Las que no tienen trazo miden 2 px a
  1080p en lugar de 1,5 px a cualquier tamaño.
- Un objeto translúcido se ve como una capa: su superficie más cercana se
  mezcla una vez sobre lo que tiene detrás, y los objetos translúcidos se
  ordenan de atrás hacia delante como objetos enteros. Una malla con colores
  transparentes ya no deja ver sus caras traseras a través de las
  delanteras.
- Las caras planas de una primitiva iluminada reciben la luz en cada píxel,
  así que un brillo puede recorrerlas.
- Los paquetes `.gaanim` con 3D grabados con versiones anteriores se abren y
  se reproducen con el nuevo 3D, pero su verificación informa de diferencias
  en los fotogramas con 3D: vuelve a grabarlos.

- Un gráfico de más de cuatro categorías sin colores propios usa ahora diez
  colores en lugar de repetir los cuatro primeros, y una carrera de barras
  sin `colors` toma la paleta del tema (`gaanim`) en lugar de `tableau10`.
  Para recuperar sus colores, pasa `colors=Palette("tableau10").colors()` o
  usa `Theme(palette="tableau10")`.
- Una serie de `drive_from_samples` declarada después del inicio ya no fija
  su primera muestra desde el segundo 0: hasta su cursor, el canal muestra la
  pose declarada o lo que lo anime antes. Si contabas con ese valor previo,
  declara la serie antes o ajusta la pose.

= 0.14.0

Publicada el 15 de octubre de 2026. El editor reúne tus preferencias en una
ventana *Settings*, se maneja entero desde el teclado con una paleta de
comandos y una lista de saltos con vista previa, y redibuja la línea de
tiempo con secciones, rombos en las pausas y un cabezal dorado. Home muestra
los recientes como tarjetas con portada. El panel *Script & narration* se
acopla a la derecha con un objetivo para la charla, compartido con Presenter
View, y prácticas cronometradas contra él (ver
#link("/guias/ensayar-y-grabar/")[Ensayar y grabar una charla]). Corrige
las exportaciones que se quedaban en 0 % al abrir Gaanim desde Neovim o VS
Code y el bloom que se convertía en una mancha blanca con la ventana grande.

== Ajustes

- *Settings* (`Ctrl+,`, la barra lateral de Home o *More controls*) reúne en
  una ventana lo que antes estaba repartido: *Recording*, *Transcription* y
  *Dictation* (antes plegables del panel de narración), *Prompter*,
  *Preview*, *Integrations* y *Updates*. Cada cambio se guarda al momento en
  un solo `settings.json` en la carpeta de datos de Gaanim, y la ventana
  reabre en la última sección. *Reset section* devuelve la sección a sus
  valores por defecto; el token de Scribe y el relay guardado conservan sus
  botones.
- El prompter lee a un ritmo automático (el de la grabación, si no el medido
  al dictar, si no 150 palabras por minuto) o a uno fijo, de 80 a 240, y
  recuerda el tamaño del texto.
- La resolución de la vista previa, el comando del editor de código y la
  comprobación diaria de actualizaciones, que solo eran variables de entorno,
  tienen su ajuste. Las variables siguen mandando, y *Settings* muestra el
  ajuste bloqueado con su nombre. El comando del editor avisa si su programa
  no está en el `PATH`, y las rutas de Whisper que no llevan a nada salen en
  rojo.
- *Updates → Check now* busca una versión nueva al instante, aunque la
  comprobación diaria esté apagada, y actualiza también desde el editor.
- *Preview* elige qué cuenta la insignia de la esquina tras cada recarga
  (avisos y errores, solo errores o nada) y si aparece el aviso «Scene
  ready». La `✕` de la insignia la oculta desde entonces.

== Editor

- `Ctrl+Shift+P` abre una paleta de comandos: cada acción del editor, cada
  sección de *Settings*, *Back to Home* y los proyectos recientes, filtrados
  al escribir y con su atajo. `Ctrl+P` lista los segmentos y sus stops por
  nombre, con la diapositiva tal como se ve allí y sus notas, y `Enter` salta.
  `?` muestra todos los atajos.
- La barra de reproducción ancha trae *Settings* y *Back to Home*, que vuelve
  a Home sin cerrar Gaanim; la intermedia muestra tantos botones como caben.
  Al abrir o recargar un proyecto se queda visible unos segundos.
- La línea de tiempo nombra las secciones de los segmentos llamados
  `Sección · parte` en una fila encima, alterna dos grises entre segmentos,
  dibuja los stops como rombos (huecos hasta llegar a ellos) y los marcadores
  de `scene.marker` en violeta. Una línea sobre cada diapositiva dice cómo va
  su guion, y el cabezal es dorado.
- La ventana reabre con el tamaño y la posición que tenía, y *Back to Home* o
  cambiar de proyecto preguntan antes si hay una grabación en curso.
- La interfaz sigue la paleta de la marca: grafito, violeta para acentos y
  selección, dorado para las pausas, casillas, deslizadores y desplegables a
  juego, y JetBrains Mono en el texto monoespaciado. Home muestra el logo.
- Una exportación que falla ofrece *Try again*, *Copy error* y, si falta
  FFmpeg, cómo instalarlo; un error de Home que nombra Python, uv o FFmpeg
  lleva a la página del entorno.

== Home

- *Recent* alterna entre la lista y una cuadrícula de tarjetas con la portada
  de cada entrada, y recuerda la elección. El editor guarda la portada de un
  proyecto al cargar la escena: `scene.thumbnail(t)`, si no el primer stop, si
  no el instante del primer segmento que más muestra; siempre en la carpeta
  de datos de Gaanim, nunca en el proyecto.
- Un paquete `.gaanim` exportado dentro de la carpeta de un proyecto
  reciente va con él, con un botón que lo reproduce. Un campo busca por
  nombre o ruta, y una chincheta fija una entrada arriba, también tras
  *Clear list*.

== Script & narration y prácticas

- El panel se acopla a la derecha y la escena se encoge a su lado. Muestra
  el objetivo de la charla (*TARGET*), que es el mismo que fija Presenter
  View, lo que suman las tomas y la última práctica frente a él, las últimas
  prácticas, un mapa de la charla, el guion de la diapositiva en pantalla con
  sus palabras y su tiempo de lectura, una lista de diapositivas filtrable
  (*No time*, *Over*, por nombre) y los controles para grabar o practicar
  (ver #link("/guias/presentaciones/#panel-narracion")[El panel Script &
  narration]).
- Cada diapositiva recibe su parte del objetivo: su `target=`, si no lo que
  dejan los objetivos, repartido según cuánto dice su guion.
- *Practice* presenta la charla cronometrada sin grabar nada y la guarda como
  un ensayo, el mismo registro que llena Presenter View. El prompter dice en
  cada momento si vas adelantado o retrasado (ver
  #link("/guias/presentaciones/#practicar")[Practicar contra un objetivo]).
- *Test level* abre el micrófono para ver su nivel sin grabar, y el
  medidor y el guion de la diapositiva actual están siempre a mano.
- Nueva guía, #link("/guias/ensayar-y-grabar/")[Ensayar y grabar una charla],
  con un proyecto de ejemplo y capturas: del guion a una charla que cabe en
  su tiempo, y de ahí al vídeo o a la presentación.

== Correcciones

- Las exportaciones desde el editor abierto por un plugin de Neovim o VS Code
  (`--listen stdio`) se quedaban en 0 % y fallaban a los 120 segundos: el
  proceso de exportación heredaba la tubería de entrada del plugin. Ya no
  hereda la entrada estándar.
- Con la ventana grande, una escena con muchos objetos y post-proceso
  (bloom, aberración cromática) se llenaba de una mancha blanca: Vello se
  saltaba el fotograma por falta de memoria en la GPU y el post-proceso se
  aplicaba una y otra vez sobre el anterior. La vista previa detecta ahora
  esos fotogramas y los repite con más memoria, como ya hacía la exportación.
- Una grabación `.gaanim` que falla o se cancela ya no deja
  `nombre.gaanim.partial`.
- Las flechas (`→`) se dibujan en el texto de la interfaz, y la línea de
  estado del prompter recorta el nombre de la diapositiva en lugar de
  montarse sobre el aviso de avance.

== Al actualizar

- Las preferencias de `narration.json` y `poll-relay.txt` pasan solas a
  `settings.json` la primera vez, y esos archivos se borran.
- `narration/` (el guion y las tomas) vive en la carpeta de assets del
  proyecto aunque el script no llame a `scene.assets.load_project(...)`. Un
  proyecto que ya guardaba `narration/` junto al script, y no en sus assets,
  sigue igual. Una ruta relativa de `scene.narration_script(...)` se busca
  entonces en esos assets.

= 0.13.1

Publicada el 14 de octubre de 2026. Corrige el reproductor de los paquetes
`.gaanim` en formato 6: uno largo, con las tablas en varios segmentos, se
quedaba congelado en la primera pausa o en el primer salto de la línea de
tiempo. No hace falta volver a exportar nada.

== Cambios

- Los paquetes en formato 6 se reproducen enteros y saltan a cualquier
  instante. Un segmento guarda las recetas de dibujo de todos los bloques que
  las usaron primero, y al cargarlo el reproductor las leía todas, también
  las que remiten a segmentos que el bloque abierto no carga; fallaba con
  «a frame refers to a path of a segment it does not list» y se quedaba en el
  último fotograma. Ahora cada receta se lee la primera vez que un fotograma
  la dibuja. `gaanim check` no lo detectaba porque recorre los fotogramas en
  orden.

== Al actualizar

- Los paquetes grabados con 0.13.0 son correctos: 0.13.1 los reproduce tal
  cual, sin `gaanim pack` ni una nueva exportación.

= 0.13.0

Publicada el 14 de octubre de 2026. El guion de la charla pasa a un archivo
Markdown con una sección por diapositiva y marcas donde avanzas, que
Presenter View y el editor muestran en un prompter. La charla se dicta con
Scribe y se graba diapositiva a diapositiva para ajustar sus tiempos, y
Presenter View cuenta hacia atrás la duración prevista y dice si vas
adelantado o retrasado. Un editor de código puede manejar la vista previa
(`--listen`) y leer los mensajes de Gaanim en JSON, y `gaanim inspect` da la
línea del script de cada parte de la escena. Los paquetes `.gaanim` pasan al
formato 6, que se abre con mucho menos y guarda el 3D como mallas, y
`gaanim pack --web` los aligera para publicarlos. El reproductor ligero se
maneja desde la página que lo inserta, con el elemento `<gaanim-player>`, y
el playground descarga una página HTML con la animación dentro. Toda la
interfaz del editor está en inglés. Lee «Al actualizar»: un `scene.stop()`
dentro de la transición de entrada de su diapositiva cambia de momento, y el
guion manda sobre las `notes=`.

== Guion y prompter

- El guion de la charla vive en un archivo Markdown, el mismo de la
  narración (`narration/script.md`, o el que nombra
  `scene.narration_script(...)`): una sección `## nombre` por diapositiva,
  con el nombre de su segmento, y una línea `[stop]` o `[stop nombre]`
  (también `[stop: nombre]`) donde avanzas. El texto antes de la n-ésima
  marca se dice mientras la diapositiva espera en su n-ésimo stop. La
  sección de una diapositiva sustituye a sus `notes=`; las diapositivas sin
  sección conservan las suyas, que también admiten marcas. Los encabezados
  pueden ser cualquier nombre de diapositiva (antes debían ser una clave de
  toma válida), los `###` son notas que no se muestran y los comentarios HTML
  se ignoran. Ver #link("/guias/presentaciones/#guion")[El guion de la
  charla].
- *Script & narration → Create script* escribe el archivo con una sección por
  diapositiva, a partir de sus `notes=`, y una marca por stop. Cuando el
  código gana diapositivas o stops, *Open and complete* añade lo que falta
  sin tocar lo escrito (los archivos CRLF siguen en CRLF) y abre el guion en
  tu editor de código (`GAANIM_EDITOR`, o uno conocido en el `PATH`) en lo
  primero que añadió.
- `gaanim check` avisa cuando una diapositiva tiene más o menos marcas que
  stops, cuando una marca con nombre no coincide con su stop y cuando una
  sección no corresponde a ninguna diapositiva ni bloque de voz.
- Al guardar el guion, el editor actualiza las notas sin volver a compilar la
  escena, y los paquetes `.gaanim` lo llevan dentro con sus marcas. La
  exportación a PDF con `--notes` imprime cada marca como una flecha
  (`→ grafico`).
- El prompter muestra el tramo que toca decir como un subtítulo, palabra a
  palabra y siguiendo la línea de tiempo, con el aviso de avanzar (`▶ stop`,
  o el nombre de la siguiente diapositiva), que late cuando la diapositiva ya
  espera allí. En Presenter View, `T` llena con él la columna del orador; en
  el editor va sobre la barra de reproducción, con `Shift+T` o *More controls
  → Prompter*. Lee al ritmo con que grabaste ese tramo, o al tuyo medido en
  tus dictados, o a 150 palabras por minuto. Ver
  #link("/guias/presentaciones/#prompter")[El prompter].
- En la barra de tiempo del editor, un cuadrado en la esquina de cada
  diapositiva dice cómo va su guion: verde si todos sus tramos tienen texto,
  ámbar si solo algunos.

== Dictar y cronometrar la charla

- *Script & narration → Record the talk* presenta la charla desde la primera
  diapositiva y guarda el tiempo de cada una y de cada tramo. *TIMING* los
  compara con sus objetivos y el total con la duración prevista, con un
  veredicto: cuánto queda, cuánto sobra o que cabe. *Re-record* vuelve a
  grabar una diapositiva o un solo stop, y *Keep the audio, to listen back*
  guarda el audio de cada tramo para escucharlo desde su fila. Los tiempos y
  el audio se guardan en la carpeta de datos de Gaanim, nunca en el
  proyecto. Ver #link("/guias/presentaciones/#grabar-charla")[Grabar la
  charla y ajustar sus tiempos].
- Mientras grabas, el micrófono puede ir a Scribe Live, de Zoom, y cada frase
  que transcribe pasa al tramo del guion que estabas diciendo; el texto
  anterior queda debajo en un comentario `<!-- previous: … -->`. El token se
  escribe en el panel, se guarda en el Administrador de credenciales de
  Windows o se lee de `GAANIM_SCRIBE_TOKEN`, y nunca entra en el proyecto. A
  Zoom solo le llegan el audio y los ajustes de la sesión. Una sesión a la
  vez, sin reconectar tras un fallo, y se cierra sola en su límite de
  minutos (60 por defecto), tras 2 minutos sin voz o al pulsar `Esc`. Ver
  #link("/guias/presentaciones/#dictar")[Dictar el guion con Scribe].
- La clave `language` de `gaanim.toml` (`es-ES`, `es`, `es-PE`…) dice en qué
  idioma se da la charla, para Scribe y otras herramientas de voz; *Pin in
  gaanim.toml* la escribe sin tocar el resto del archivo. `gaanim check`
  avisa de las claves que parecen credenciales (`token`, `secret`,
  `password`, `api_key`), que Gaanim nunca lee. Ver
  #link("/referencia/gaanim-toml/#language")[`language`].

== Presenter View

- `scene.slides.target(20 * 60)` fija la duración de la charla y
  `scene.segment(..., target=120)` la de cada diapositiva, en segundos. La
  de la charla también se elige en Presenter View, con el botón junto a
  `LEFT`, o con `gaanim --present --duration 20m` (`1h30m`, `90s`, `20:00` o
  minutos). `LEFT` cuenta hacia atrás lo que queda, `PACE` dice cuánto vas
  adelantado o retrasado según los objetivos, y bajo el nombre de la
  diapositiva se lee `0:45 of 1:30 on this slide` y lo que tardaste en el
  ensayo anterior. Cada presentación de más de 30 segundos se guarda como
  ensayo (los 20 últimos por archivo; en el reproductor web, durante la
  sesión). Los paquetes `.gaanim` guardan los objetivos. Ver
  #link("/guias/presentaciones/#tiempo-de-la-charla")[El tiempo de la
  charla].

== Editores de código

- `gaanim --listen stdio` (o `--listen 127.0.0.1:PUERTO`) deja que un editor
  maneje la vista previa con JSON por líneas: `seek` a un tiempo, segmento,
  `Section` o stop; `follow` a una línea del script; `play`, `pause`, `loop`
  de un segmento o un tramo y `unloop`. Gaanim responde con los eventos
  `ready` (con la línea de tiempo de la escena), `reload`, `playhead`
  (tiempo, segmento, stop, si reproduce y el bucle), `failed` (el error del
  script con su línea) y `error`. Solo escucha en la propia máquina. Ver
  #link("/referencia/cli/#listen")[Controlar la vista previa desde un
  editor].
- `gaanim inspect mi-charla [--json]` lista los segmentos, `Section`, stops,
  marcadores, bloques de voz, la toma en directo y cada `play` con su tiempo
  y la línea del script que los crea. Ver
  #link("/referencia/cli/#gaanim-inspect")[`gaanim inspect`].
- `--message-format json` en `gaanim check`, `gaanim export` y la vista
  previa escribe cada mensaje como un objeto JSON por línea: mensajes con su
  archivo y línea, diagnósticos y veredicto de `check`, progreso por
  porcentaje y recargas con sus tiempos. Ver
  #link("/referencia/cli/#message-format")[Mensajes para herramientas].
- Todo es opcional: sin estas opciones, la vista previa y la CLI se
  comportan como antes, y `gaanim init` no añade nada propio de ningún
  editor.

== Exportación

- `gaanim export --frame`, `--from` y `--to` aceptan, además de segundos y
  marcadores, el nombre de un segmento, una clave de `Section` o el nombre
  de un stop. `--frame` de una diapositiva toma su primer stop, el mismo
  fotograma que su página del PDF; `--from` empieza en su comienzo y `--to`
  acaba en su final. Un nombre compartido por varios stops es ambiguo y el
  error los lista con su tiempo. Ver
  #link("/referencia/cli/#tramos-nombres")[Tramos y marcadores].

== Paquetes

- Formato 6: las tablas (trazados, imágenes, recetas, textos, shaders y
  mallas 3D) van en segmentos de unos 2 MB, y abrir un paquete solo lee su
  manifiesto, la escena, el primer bloque y los segmentos que usa. En los
  170 ejemplos, lo que hace falta para abrirlos baja un 74 %. Gaanim y los
  reproductores siguen abriendo los paquetes en formato 5.
- Las escenas 3D se guardan como mallas, matrices y luces, una sola vez, y se
  proyectan de nuevo al leerlas: `gallery/surface_plot` pasa de 54 MB a
  1,1 MB y `3d/camera_3d` de 15 MB a 0,16 MB.
- `gaanim pack entrada.gaanim salida.gaanim` reescribe un paquete en el
  formato actual, fotograma a fotograma. Con `--web`, guarda los trazados en
  una rejilla de menos de 1/32 de píxel en 4K y deja fuera lo que solo usan
  `gaanim check` y el desenfoque de movimiento: los ejemplos ocupan un 39 %
  menos y abrirlos necesita un 89 % menos. Ver
  #link("/referencia/cli/#exportar-paquetes")[Paquetes].
- Abrir un paquete ya no espera a las huellas de los fotogramas, que solo usa
  `gaanim check`, y la portada se guarda con la mejor compresión (la mitad
  de tamaño).

== Reproductor web

- El reproductor ligero se maneja desde la página que lo inserta, en
  cualquier sitio: recibe `gaanim:command` (`play`, `pause`, `toggle`,
  `seek`, `loop`, `mute`) y envía `gaanim:status` con el tiempo, la
  duración y su estado; el reproductor completo responde a los mismos
  mensajes. Ver #link("/guias/compartir/#controlar-desde-la-pagina")[Controlarlo
  desde la página].
- El elemento `<gaanim-player>` (`insertar/gaanim-player.js`) inserta el
  reproductor con la proporción de la escena, sin escribir `aspect-ratio`, y
  ofrece `play()`, `pause()`, `seek()`, `currentTime`, `duration`, `paused`,
  `ended`, `loop` y `muted`, con sus eventos.
- Arranca antes: descarga el archivo mientras compila el módulo, y un
  archivo grande que se lee por partes solo baja el resto mientras se
  reproduce; `preload=auto` lo descarga entero como antes.
- Cuando el reproductor no tiene la forma de la escena, oscurece lo que la
  rodea y marca su borde, como la vista previa del editor.
- El playground descarga la grabación como una página `.html` con el
  reproductor ligero dentro, que se abre con doble clic, sin servidor, o va
  en un `<iframe>`: en *Insertar* o en el menú *Descargar*. Ver
  #link("/guias/compartir/#pagina-html")[Una página HTML].

== Editor

- Toda la interfaz del editor está en inglés, como ya lo estaba Presenter
  View: la pantalla de inicio, la barra de reproducción, la exportación, el
  inspector, la consola, la línea de tiempo de animaciones, las encuestas,
  *Script & narration* y los mensajes del reproductor web. Las claves de los
  enlaces (`segmento=`, `pausa=`) y los nombres de los archivos de
  resultados no cambian.

== Correcciones

- Un `scene.stop()` escrito mientras la transición de entrada de su
  diapositiva aún corre espera a que termine. Antes, Presenter View, el PDF y
  `--capture-stops` mostraban la diapositiva a medio entrar.
- `gaanim check` verifica los paquetes con grupos que comparten un brillo, un
  desenfoque o una sombra; antes rechazaba sus fotogramas (92 de 307 en
  `effects/glow_blur_shadow`).

== Al actualizar

- Un `scene.stop()` dentro de la transición de entrada de su diapositiva se
  mueve al final de la transición, y lo que viene después se retrasa lo
  mismo. Los enlaces y las capturas tomadas en ese stop cambian de momento.
- Si tu proyecto ya tiene `narration/script.md`, una sección con el nombre
  de una diapositiva sustituye a sus `notes=`. Un guion que no se puede leer
  (un encabezado vacío o repetido) hace que `scene.segment` lance
  `ValueError`, como ya hacía `scene.voiceover`.
- Los paquetes grabados con 0.13.0 están en formato 6, que las versiones
  anteriores de Gaanim no abren. `gaanim pack` pasa los de formato 5 al
  actual, más rápidos de abrir en la web.
- Vuelve a grabar los paquetes con efectos compartidos (brillo, desenfoque o
  sombra en varios objetos iguales) grabados antes para que `gaanim check`
  los verifique.
- Los menús y botones del editor tienen sus nombres en inglés: el panel
  *Narración* es ahora *Script & narration*, y *Más controles*, *More
  controls*.

= 0.12.0

Publicada el 13 de octubre de 2026. Los ejemplos se ordenan por temas, con
una sección en la web donde cada uno se reproduce sobre su captura, y una
galería de escenas cortas en los colores de la marca sustituye a los ejemplos
básicos y avanzados. Las diapositivas se exportan a PDF con las notas del
orador, y los vídeos a cualquier frecuencia y a MOV con Apple ProRes, con
canal alfa, para montarlos en un editor de vídeo. Llegan luces 3D que se
colocan y se animan, el tema `gaanim`, el resaltado de sintaxis de
`scene.text.code` con JetBrains Mono y un reproductor ligero que marca las
pausas en su barra. Corrige los paquetes `.gaanim` de escenas 3D, cuatro
fallos de `animate.transform_to` con SVG y degradados, la cámara que sigue o
se acerca a un objeto reactivo y la selección de fracciones en las
ecuaciones, y los rastros se dibujan como curvas suaves. Lee «Al actualizar»:
varios cambios alteran cómo se ven escenas existentes (énfasis, `write`,
bloques de código, cajas escaladas, ecuaciones de varias filas).

== Exportación

- `gaanim export --fps` elige los fotogramas por segundo de cualquier vídeo,
  de 1 a 240, en lugar de los 30 o 60 de la calidad: un entero (`24`), una
  fracción (`24000/1001`) o un decimal (`23.976`, que significa esa fracción
  exacta). Antes solo se aplicaba al grabar un paquete, y la forma de lograr un
  vídeo a 24 fps era grabar un paquete a 24 y exportarlo. La duración y el
  audio no cambian, y el desenfoque de movimiento usa la tasa elegida. El
  diálogo de exportación del editor tiene el mismo selector. Ver
  #link("/referencia/cli/#fotogramas-por-segundo")[Fotogramas por segundo].
- Un `--output` terminado en `.mov` exporta Apple ProRes, que Premiere,
  DaVinci Resolve y Final Cut importan sin conversión: 422 HQ, o 4444 con
  canal alfa con `--transparent`, con el color etiquetado como BT.709 y el
  audio sin comprimir. Sirve para poner rótulos y _lower thirds_ de Gaanim
  encima de un montaje; WebM también lleva alfa, pero los editores lo leen
  mal. El diálogo del editor suma MOV a sus formatos y una casilla
  *Conservar la transparencia* para MOV, WebM, WebP y PNG.
- Un `--output` terminado en `.pdf` exporta las diapositivas: una página por
  cada `scene.stop()` y otra para el final, con el fotograma que muestra el
  Presenter en esa pausa, sin pérdida y al tamaño de `--width` y `--height`.
  Con `--notes`, tras cada diapositiva va una página con sus notas, en texto
  que se puede buscar. Sirve para repartir la charla a quien no tiene
  Gaanim. Un paquete `.gaanim` también se exporta a
  PDF, sin Python, y el diálogo del editor suma PDF a sus formatos con una
  casilla para las notas. Ver
  #link("/referencia/cli/#diapositivas-pdf")[Diapositivas en PDF].

== Actualizaciones

- La pantalla de inicio avisa cuando hay una versión nueva de Gaanim (consulta
  GitHub como mucho una vez al día) y la instala con *Actualizar*; *Reiniciar*
  abre la versión nueva. `gaanim update` hace lo mismo desde la terminal y
  `gaanim update --check` solo lo comprueba. Gaanim verifica la suma SHA-256
  del paquete y que el programa nuevo arranca antes de reemplazar nada, y deja
  la instalación como estaba si algo falla. `GAANIM_NO_UPDATE_CHECK=1`
  desactiva el aviso. Ver
  #link("/referencia/cli/#gaanim-update")[`gaanim update`].

== Animación

- `write()` y `unwrite()` animan pieza a pieza:
  cada pieza traza su contorno en la primera mitad de su parte, un tramo de
  Bézier tras otro, fino y centrado, en el color de su trazo, y en la segunda
  mitad aparece su relleno y el trazo toma su ancho. La curva se aplica al
  avance de cada pieza, y sin duración duran 1 s con menos de 15 piezas y 2 s
  con más. Se conservan `by`, `order`, `seed`, `stagger`, la tiza y la punta
  de lápiz. Ver #link("/referencia/animations/#api-anim-write")[`Anim.write`].

== Texto

- `scene.text.code` resalta la sintaxis: las palabras clave, las cadenas, los
  números, los comentarios, las funciones y los tipos toman los colores de
  código del tema. Los temas de editores (`dracula`, `nord`, `solarized-*`,
  `gruvbox-dark`, `tokyo-night`, `catppuccin-*`) usan su esquema original;
  `technical`, `presentation`, `paper` y los temas propios lo derivan de la
  paleta, y los tokens `code_keyword`, `code_string`, `code_number`,
  `code_comment`, `code_function`, `code_type` y `code` de `Theme(colors=...)`
  lo cambian. `theme="esquema.tmTheme"` usa un esquema de TextMate o Sublime Text,
  `highlight=False` lo deja monocromo y `font` elige la fuente, con JetBrains
  Mono como respaldo. Con un `background` en el que los colores del tema
  no se leen, como un panel oscuro en `paper`, el texto pasa al color de fondo
  del tema y el acento y los tokens se ajustan hasta leerse. Antes,
  `language` solo se mostraba en la cabecera.
  Ver #link("/referencia/text/#api-typography-code")[`Typography.code`].
- El código usa JetBrains Mono, incluida en Gaanim con todos sus pesos y su
  cursiva: el rol `code` (que antes pedía Consolas y, sin ella, caía en otra
  fuente) y los bloques de `scene.text.code` (antes DejaVu Sans Mono). Así el
  código se ve igual en Windows, macOS, Linux, el playground y los paquetes
  `.gaanim`. Un tema con `fonts={"code": ...}` sigue eligiendo la suya.

== Matemáticas

- `scene.viz.matrix(..., fractions="inline")` escribe los racionales exactos
  en una línea (`3/2`), tan altos como las demás entradas; antes había que
  rodearlo con un `cell_factory`. Ver
  #link("/referencia/matrices/#api-visualization-matrix")[`Visualization.matrix`].
- `TextStyle.size` devuelve el tamaño que fija el estilo.

== Layout

- Una porción de un `ZoneSet`, como `zonas[1:]`, es otro `ZoneSet`: se
  recorre, se indexa y tiene `fill()`. Ver
  #link("/referencia/layout/#api-zoneset-fill")[`ZoneSet`].
- El tema tiene un color `shadow`, el de `shadow=True` en las cajas: negro
  suave sobre fondos claros y casi opaco sobre fondos oscuros. Cámbialo con
  `Theme(colors={"shadow": ...})`.

== Estilo y 3D

- Tema integrado `gaanim`, con los colores de la marca: papel sobre grafito,
  acentos violeta, títulos en oro y reglas en bruma. Sus series empiezan por
  el violeta y el oro, y su mapa de calor va del grafito al oro pasando por
  la bruma y el violeta. Ver #link("/referencia/themes/")[Temas].
- `scene.geometry.light_3d()` crea luces puntuales o direccionales para las
  primitivas con material: se colocan con `move_to_3d`, se animan y siguen a
  otros objetos como cualquier dibujable, y alumbran en su color. Mientras la
  escena muestra luces, sustituyen a la luz principal fija; `fade_in` y
  `fade_out` las encienden y las apagan. Ver
  #link("/referencia/geometria/")[Geometría].
- `space.surface(..., color_map=...)` colorea la superficie por altura con un
  `ColorMap` o con cualquier mapa integrado por su nombre (`"viridis"`,
  `"plasma"`…). Sin él conserva la rampa de siempre.

== Ejemplos

- Una galería, `examples/gallery/`, con 23 escenas cortas en los colores de
  la marca (el tema `gaanim`), agrupadas en conceptos básicos, animaciones,
  gráficas, cámara y 3D, y avanzadas. Va desde el logo construido con
  primitivas hasta los epiciclos de Fourier, pasando por un vector que barre
  el plano, sumas de Riemann que se refinan, una cámara que sigue un punto,
  una lupa sobre un copo de Koch y una superficie 3D. Abre la sección
  #link("/ejemplos/gallery/")[Gallery] de los ejemplos y sustituye a «Ejemplos
  básicos» y «Ejemplos avanzados», cuyas direcciones llevan ahora a la
  galería. El playground ofrece estas escenas en su menú de ejemplos.
- Los ejemplos del repositorio se ordenan por temas:
  `examples/animation/easings.py`, `examples/camera/camera_moves.py`,
  `examples/3d/primitives_3d.py`… Cada uno recorre las variantes que ofrece
  la API de su tema, con el tema por defecto y una etiqueta con el código que
  construye cada variante. `examples/examples.toml` los cataloga y
  `just run easings` abre uno por su nombre. Los scripts sueltos de antes se
  reescribieron en inglés dentro de su tema, o pasaron a `tests/` los que
  eran pruebas.
- La documentación tiene una sección #link("/ejemplos/")[Examples] con todos
  los ejemplos por tema: una portada con los temas en cinco bloques, Showcase
  destacado y un filtro, y una página por tema con cada ejemplo, su captura,
  su código y su enlace al repositorio. Sale del catálogo al compilar la
  documentación, así que un ejemplo nuevo aparece sin tocarla.

== Editor

- El título de la ventana dice qué versión de Gaanim se está ejecutando:
  «Gaanim 0.12.0 — proyecto», con «(dev)» en una build de desarrollo, para
  distinguirla del release instalado. También en la Presenter View y al
  reproducir un paquete.

== Reproductor web

- La barra del reproductor ligero (`reproductor/insertar/`) marca cada pausa
  de la escena (`scene.stop()`) en su línea de tiempo; las ya pasadas se ven
  en violeta, como el progreso.
- La opción `full=click` del reproductor ligero espera a un clic antes de
  abrir el reproductor completo para una escena que lo necesita, y avisa a la
  página que lo contiene para que ofrezca su propio botón (ver
  #link("/guias/compartir/#insertar")[Insertar en un blog o sitio web]).

== Correcciones

- Las partes de una ecuación que contienen una fracción (`frac(a, b)` o
  `a/b`) o una función que solo compone sus argumentos (`sqrt`, `abs`,
  `bold`, `binom`…) se pueden seleccionar: `surrounding_rect`, `fill`,
  `write(by="part")` y las transformaciones las alcanzan. Antes la selección
  quedaba vacía y el marco o el color no aparecían. Seleccionar la fracción
  entera incluye su barra.
- `transform_to` entre marcas de un plano (`riemann_sum`, `plot_data`,
  `scatter_data`…) transforma su forma; antes solo cambiaba el color, porque
  la marca se redibujaba desde sus datos en cada fotograma.
- `scene.camera.animate.pan_to(dibujable)` centra el dibujable donde se
  dibuja, también cuando sigue a un punto reactivo, y al terminar la cámara
  se queda allí; antes iba a la posición con la que se declaró y la
  siguiente animación de cámara partía de la pose anterior a la panorámica.
- `scene.camera.animate.orbit` gira alrededor del eje `up` de la cámara: con
  `look_at(..., up=(0, 0, 1))`, como en los ejes 3D, rodea la escena en vez de
  pasar por encima. Una órbita tras otra parte de donde terminó la anterior.
- Un dibujable con `hidden()` dentro de un grupo, o una marca de un plano,
  espera a su propia animación: el `create` del plano o la entrada del grupo
  ya no lo muestran.
- Un plano con el eje X abajo (`crossing("min")`) numera el cero de su eje Y.
  Solo se omite el número del eje Y donde los dos ejes se cruzan en el mismo
  valor, como en el origen.
- Un paquete `.gaanim` guarda bien las escenas 3D. Antes, en cada fotograma
  salvo el primero de cada bloque, todas las piezas de una malla se dibujaban
  como la última (un cubo quedaba en una cara, una esfera en un triángulo) y
  las rejillas de unos ejes 3D en una sola línea, al reproducir, presentar o
  exportar el paquete. Hay que volver a grabar los paquetes con 3D.
- El reproductor ligero (`reproductor/insertar/`) abre en el reproductor
  completo las escenas con cristal; antes las dibujaba sin el efecto. Al
  pasarle una escena le da la dirección completa del archivo, que antes se
  perdía en un sitio bajo una ruta base, como GitHub Pages.
- Los temas `solarized-light` y `catppuccin-latte` pasan su propio
  `validate()`. El título de `solarized-light` (`#B08500`) y el color atenuado
  de `catppuccin-latte` (Overlay2, `#7C7F93`) son algo más oscuros y llegan al
  contraste de 3:1 sobre el fondo; antes se quedaban en 2,98:1 y 2,83:1.
- `scene.camera.animate.orthographic()` desde una cámara en perspectiva dura
  lo que pide `scene.play` (un segundo por defecto): el ojo se aleja por su eje
  de visión mientras el campo de visión se cierra, el objetivo sigue centrado y
  la proyección pasa a ortográfica al terminar. Antes cambiaba de golpe y no
  avanzaba el cursor. Desde una cámara ortográfica anima el zoom.
- Los docstrings de `Brush.solid`, `linear`, `radial` y `sweep` explican sus
  parámetros: las coordenadas están en unidades locales del objeto, y un grupo
  pasa el pincel a cada miembro, así que un gradiente no cruza el grupo. La
  referencia de los pinceles lo explica también, con cómo hacer que un
  gradiente recorra varias formas.
- El nombre del lenguaje de `scene.text.code` va en monoespaciada dentro de la
  cabecera, y el código empieza bajo la regla, desde el margen izquierdo del
  panel. Antes el nombre, con la letra del cuerpo, se salía por la esquina, y
  el código se centraba en la mitad izquierda: un fragmento ancho se salía del
  panel.
- Un relleno o un trazo con degradado se interpola durante el morph hacia el
  del destino (posición y paradas) y sigue siendo un degradado al terminar;
  dos degradados de tipos distintos pasan por un color liso a mitad. Antes se
  pintaba del color de la primera parada desde el inicio del morph. Vale para
  `transform_to`, `replacement_transform_to`, `magic_move` y
  `transform_matching_shapes`.
- `transform_to` de un SVG importado, de una de sus partes `<g>` o de un grupo
  se transforma pieza a pieza en lugar de desaparecer: las partes con el mismo
  nombre o `id` de SVG se emparejan, a cualquier profundidad, y las demás
  hojas se emparejan por forma dentro de su pareja. Las hojas que sobran del
  objeto se desvanecen y las del destino aparecen donde está el destino. El
  objeto conserva su identidad, así que sus `part()` siguen valiendo después,
  y el destino sigue siendo una plantilla oculta.
- Un contorno se empareja con el de destino en el sentido de recorrido que
  menos mueve sus puntos. Una forma y su espejo, escritos en sentidos
  opuestos, se reducían a una línea a mitad del morph; ahora conservan su
  área, y el resultado ya no depende de cómo se escribió el camino.
- `transform_to` deja libre la opacidad del objeto, como indicaba la
  referencia: un `animate.opacity` del mismo objeto en el mismo `play` lo funde
  mientras se transforma. Antes el `play` fallaba por usar los dos el canal
  `opacity`, y los mensajes de error se remitían el uno al otro. El error de
  dos animaciones que se pisan ya solo propone combinarlas en una cuando las
  dos son de propiedades.
- `PostProcess.lut`, `PostProcess.shader`, `Background.shader`,
  `Transition.shader`, la imagen de `Transition.preset("luma")` y las fuentes
  de `Theme(font_dir=..., font_files=...)` buscan una ruta relativa como los
  demás archivos de una escena: en el directorio de trabajo, luego en la
  carpeta `assets` del script y luego junto a él. Antes solo la buscaban en el
  directorio de trabajo, así que un script lanzado desde otra carpeta no
  encontraba su `.cube`, su `.wgsl`, su imagen o sus fuentes.
- `gaanim --diff` captura por defecto a 960 píxeles en el lado largo, o a la
  altura de la baseline que ya existe, y guarda los fotogramas opacos en RGB
  con la máxima compresión de PNG: una baseline pesa unas cinco veces menos
  que a 1920 en RGBA. Las baselines que ya tenías se siguen comparando a su
  tamaño; `--height` elige otro.
- `create`, `write` y los demás trazos que se dibujan de a poco ya no dejan
  franjas, barras ni gotas en sus primeros fotogramas. Una curva hecha de
  muchos trozos, como `implicit` o `contour`, se llenaba de franjas
  horizontales al empezar a dibujarse, y un `plot` mostraba un rectángulo en
  su origen: al recortar el trazo quedaban segmentos tan cortos que el
  trazador de Vello los convertía en esas manchas. Ahora los trozos visibles
  de menos de una diezmilésima de unidad se omiten.
- `scene.slides.bullets` alinea cada punto a la izquierda, justo después de
  su viñeta. Antes cada texto se centraba en la columna, así que los puntos
  cortos quedaban separados de su viñeta y desalineados entre sí.
- La vista previa gira la cámara como las exportaciones. Con `rotate_to`,
  `bind_2d(rotation=...)` o `follow_path(orient=True)` giraba la escena al
  revés, el `hud()` giraba el doble en vez de quedar fijo y, con la cámara
  lejos del origen, su centro se desplazaba de la pantalla hasta dejar la
  curva seguida fuera de vista. Las exportaciones y `--diff` no cambian.
- Las etiquetas `billboard` de una cámara ortográfica girada quedan derechas
  también al exportar; antes solo lo estaban en la vista previa equivocada.
- `camera.frame_to` y `camera.animate.frame_to` sin `margin` dejan media unidad
  por lado. Antes usaban 40 unidades, un valor de cuando el margen se medía en
  píxeles, y la vista se alejaba hasta un zoom de 0,1.
- Al terminar, `camera.animate.follow` y `frame_to(..., dynamic=True)` dejan
  la cámara donde la llevaron, y el siguiente movimiento parte de ahí. Antes
  volvía de golpe a la pose de antes de la toma. La pose final sale de dónde
  acaba el objetivo al final del `play`; si lo mueve otra cosa en tiempo de
  ejecución, como un updater, la cámara salta a esa pose al terminar.
- `move_to`, `align_to`, `next_to(aligned_edge=...)`, `to_corner`,
  `anchor_point` y `with_port` aceptan el nombre de un anclaje
  (`anchor="top_left"`), como ya hacía `place`; antes lanzaban `TypeError`.
- `CameraView.follow` y `Drawable.follow` sin `delay` empiezan en el cursor, y
  `no_camera_view()` quita la vista desde el cursor. Antes un `follow` llamado
  después de `view.animate.pan_to(...)` se aplicaba desde la creación de la
  vista y anulaba ese movimiento, y `no_camera_view()` al final de un segmento
  quitaba la vista en todo el segmento.
- `camera.animate.to(state)` entre una pose 2D y una en perspectiva, y
  `camera.animate.reset()` desde una en perspectiva, se mueven en perspectiva
  desde o hacia la vista equivalente a la 2D, y solo pasan a ortográfica en su
  extremo 2D. Antes la cámara salía del plano de la escena, así que las
  primitivas 3D desaparecían la primera mitad del movimiento, y un `reset`
  pasaba a ortográfica de golpe con la cámara aún inclinada, de modo que el
  contenido 2D se salía de la vista.
- `animate.flash()` dispara rayos alrededor del objeto (`lines`, `length`,
  `gap`, `color`, `width`) y `animate.circumscribe()` dibuja un marco a su
  alrededor y lo borra, como `flash_around(time_width=1.0)`. Antes el primero
  fundía y agrandaba el objeto y el segundo solo lo agrandaba un 10 %.
- `with_pen_tip()` dibuja una punta en el extremo de cada trazo mientras
  `write`, `create` o `draw_border_then_fill` lo trazan; antes no hacía nada.
- `grow_from_edge`, `grow_from_point`, `spin_in_from_nothing`, `indicate`,
  `wiggle`, `blink`, `broadcast` y `scene.spotlight` respetan su lugar en
  `sequence`, `stagger` o con `delay`; antes empezaban todos a la vez.
- `animate.indicate()` crece 1,2 veces y tiñe el objeto del `"accent"` del
  tema, de su `"foreground"` si el objeto ya tiene el color del acento, o de
  `color=`; antes solo crecía un 10 %. `animate.wiggle()` acepta
  `amplitude` y `count`, y sacude 0,1 unidades a cada lado en lugar de 0,05.
- En una selección de texto, `pulse()` late dos veces, `wave()` levanta los
  glifos uno tras otro y `highlight()` los tiñe del acento sin moverlos;
  `indicate()` crece y tiñe. Antes los cuatro eran casi el mismo salto.
- `magic_move(...)` sin `duration` y los movimientos de
  `scene.camera.animate` toman la `duration` de `scene.play` cuando no fijan
  la suya; antes duraban siempre un segundo.
- `clear_drive()` deja el objeto girado como lo tenía su `look_at`; antes
  volvía a su rotación propia.
- `scene.geometry.cross()` y `right_angle()` toman el estilo de línea del
  tema y se ven sin `stroke`; antes eran invisibles con el tema por defecto.
- Los trazos discontinuos de las formas cerradas conservan sus huecos con
  remates `"round"` o `"square"` (el trazo interior por defecto los
  agrandaba), y un guion de longitud cero con esos remates dibuja un punto.
- `stroke(paint, width)` después de `stroke_style(...)` conserva los guiones,
  remates y uniones, y `StrokeStyle` acepta `align`.
- El estilo raíz de un texto subraya, tacha y contornea todo el texto, como ya
  hacía en una parte, y `word_spacing` separa las palabras en la raíz. Un
  `stroke` sin `stroke_width` contornea con 0,02. Una parte con `baseline`
  sube o baja dentro de su línea; antes saltaba a otra o desaparecía.
- `max_lines` limita las líneas: `overflow="clip"` corta el texto tras la
  última palabra que cabe y `"ellipsis"` lo termina con «…». Antes no
  recortaba nada y, en una caja, ponía una palabra por línea.
- Un `wrap` numérico ajusta el texto dentro de una caja; antes lo dejaba en
  una sola línea.
- `text.tracking(...)` espacia bien los glifos que siguen a una coma.
- `text["parte"].fill(...)` después de un `play` cambia el color de la parte
  en el cursor, como una selección literal; antes no hacía nada.
- `role="code"` muestra `*` y `_` tal cual (antes `"stagger(*grows)"` lanzaba
  `ValueError`).
- `write(order="random", seed=n)` cambia el orden con la semilla, y `write()`
  sobre un grupo, como un bloque de `scene.text.code`, escribe cada texto en
  orden de lectura en lugar de empezar por su última línea. Ya no deja
  recuadros alrededor de las etiquetas `content(...)` de CeTZ.
- `text.become(...)` después de un `play` cambia el texto en el cursor, y
  `animate.fade_transform_to(otro_texto)` deja visible el texto de destino.
- Los trazos negros de los documentos Typst (tablas, `rect`, `line`, CeTZ)
  toman el color del texto del documento y se ven sobre fondos oscuros.
- Los documentos Typst escriben su texto en el color del rol `body` del tema,
  como `scene.text`, en lugar de blanco o negro puro, y empiezan con `theme`,
  un diccionario con los colores del tema (`theme.accent`, `theme.rule`…)
  para dárselos a sus paquetes, que piensan en papel blanco y eligen negro y
  blanco por su cuenta. Ver
  #link("/referencia/text/#colores-del-tema-en-typst")[Colores del tema en Typst].
- `write()` traza de un tirón las curvas que un paquete de Typst dibuja muestra
  a muestra (simple-plot, CeTZ); antes se veían punteadas.
- Las ediciones de una matriz (`insert_row`, `remove_column`, `swap_rows`,
  `reorder_columns`, `become`…) la dejan donde estaba, con su escala y sus
  animaciones; antes la reconstruían en el origen.
- `matrix.morph_to(destino)` termina en el destino: las entradas viajan hasta
  las suyas, los delimitadores se transforman en los del destino y aparecen
  sus entradas sin pareja. Antes se reordenaban dentro de la matriz de origen
  y el destino se quedaba con los delimitadores vacíos.
- `matrix.place(zona)` devuelve la matriz, no la caja de su layout.
- `order="simultaneous"` empieza todas las entradas a la vez aunque haya
  `stagger`; antes las escalonaba por filas. Sin `order`, un `stagger` las
  sigue escalonando fila a fila.
- `matrix.rank()` dibuja `rank = 2`; antes Typst no conocía `rank` y no se
  veía nada.
- Escalar una matriz, o cualquier caja, agranda también lo que contiene;
  antes solo separaba las entradas. Los delimitadores automáticos abarcan las
  filas según el tamaño de `entry_style`, y un `delimiter_size` explícito
  sigue al número de filas tras `become()`.
- Una ecuación con matemática inválida, como `a dot.circle b`, lanza
  `ValueError` al crearla, como dice la documentación; antes solo escribía el
  error de Typst y no dibujaba nada.
- Las filas de una ecuación partida con `\` ya no se pisan: guardan la
  separación de Typst entre filas.
- Las selecciones de partes encuentran sus glifos: `eq["inertia"]` con partes
  anidadas (antes no seleccionaba nada al animarla, y `highlight()` no se
  veía) y una parte cuyo texto aparece antes en la ecuación (antes
  seleccionaba la primera aparición). El color de `part(..., color=...)` tiñe
  solo la parte, no cada aparición de su texto.
- `write(by="part")` escribe las partes una tras otra; antes empezaban casi a
  la vez.
- `reveal("wipe")` deja el término oculto hasta su turno, como `"fade"`;
  antes se veía mientras se escribía el resto y luego se barría.
- `item(basis=...)` acepta `"N%"`, `"hug"` y `"fill"`, como dice la
  documentación; antes solo aceptaba longitudes.
- `fit="cover"` recorta la capa donde está su caja aunque la caja se haya
  movido con `move_to` o `place`; antes recortaba en el sitio donde se
  resolvió y la capa desaparecía.
- `box.detach(hijo)` en una caja movida deja al hijo donde estaba; antes se
  dibujaba en el sitio de la caja sin mover, desde el principio de la escena.
  Cambiar de padre ocurre en la línea de tiempo: el hijo sigue a la caja
  hasta el `detach` y desde ahí se mueve solo.
- Pasar un hijo de una caja a otra (`a.detach(hijo)` y luego
  `b.add(hijo, duration=...)`) lo desliza desde donde está; antes no se veía
  desde el principio de la escena hasta entrar en la segunda caja, y su
  posición anterior se leía respecto a ella.
- `point_on_curve`, `tangent_on_curve`, `normal_on_curve` y
  `curvature_on_curve` siguen la curva tal como se dibuja: movida, girada,
  escalada, animada o dentro de unos ejes. Antes usaban los puntos declarados
  y se quedaban atrás.
- `curvature_on_curve` dibuja el círculo también en los extremos de una curva
  abierta y en la costura de una cerrada; antes no dibujaba nada, o un
  círculo de radio 1 en el origen.
- `bind_rotation_from` ve el giro de `Updater.rotate`, y
  `bind_translation_from_rotation` sigue a `Updater.rotate` y a
  `rotate_to(Parameter)` más allá de media vuelta; antes la cremallera
  retrocedía al pasar de media vuelta.
- `fill` colorea entera una flecha sólida: su contorno sigue al relleno salvo
  que `stroke` fije otro. Antes `always_redraw_arc(...).fill(color)` solo
  coloreaba el interior de la punta.
- Los trackers de curvas, `always_redraw_arc` y `point_between(alpha=...)`
  aceptan un `Computed`.
- El ejemplo de `Parameter.add_updater_fn` en la referencia calcula el valor a
  partir del tiempo transcurrido; el anterior volvía a 0 al hacer seek.
- `transform_matching(..., mode="tex")` y `mode="text"` terminan mostrando el
  destino; antes dejaban ocultos el origen y el destino. Un `mode`
  desconocido lanza `ValueError` en vez de usar `"shapes"`.
- `transform_matching_shapes` y `transform_matching` muestran las piezas
  nuevas del destino, que antes no aparecían, y al final muestran el destino y
  ocultan el origen: las animaciones siguientes del destino mueven lo que se
  ve.
- `with_pivot(x, y)` en una parte de un SVG (`svg.part(...)`) lee el punto en
  coordenadas de escena, también en `animate.rotate_by`, aunque el SVG esté
  escalado o movido; antes lo leía en el marco del documento SVG.
- `Transition.morph` y `Transition.magic_move` con un resorte
  (`Easing.BOUNCY`, `Easing.SNAPPY`...) dejan que el movimiento se pase del
  destino y vuelva, y no parpadean: antes el sobrepaso mostraba el estado
  final de golpe y el rebote volvía a mostrar el contenido saliente. Con un
  easing explícito, el morph ya no lo suaviza otra vez. `cross_fade` y
  `fade_through` tampoco vuelven a mostrar el segmento saliente.
- `fx.spotlight([a, b])` abre un hueco alrededor de cada objetivo; antes abría
  uno solo alrededor de la caja conjunta.
- `drawable.post_process(False)` deja un objeto, con sus miembros, fuera del
  postprocesado de la escena y del de su segmento: se dibuja encima del cuadro
  procesado, así que un HUD o una etiqueta se leen bajo `pixelate`, `glitch`
  o `halftone`. Funciona en la vista previa, al exportar y en los `.gaanim`;
  un reproductor anterior dibuja esos objetos procesados.
- `segment.motion_blur(...)` da a un segmento su propio desenfoque de
  movimiento, o ninguno con `None`, en lugar del de `scene.canvas.motion_blur`.
- El mensaje de un `.cube` que `PostProcess.lut` no puede leer habla de una
  LUT, no de un «WGSL asset».
- Los ejemplos de `PostProcess.glitch` y de los pulsos con `Parameter` en la
  referencia y en la guía de efectos usan `.repeat(2, yoyo=True)`, que sube y
  vuelve; `.repeat(1, yoyo=True)` solo subía.
- La referencia de `scene.persist` explica que un objeto nuevo persistido
  justo después de `scene.segment(...)` entra con la transición de ese
  segmento.
- `text.animate.move_to(x, y)` deja el texto donde lo pone
  `text.move_to(x, y)`: la primera línea base centrada en el punto (el centro
  visual si tiene varias líneas). Antes centraba la caja visual, así que un
  texto colocado con `move_to` y luego animado a otro punto bajaba además de
  desplazarse.
- `scene.media.image(ruta, width=…, height=…, fit="cover")` y su equivalente
  en `video` recortan el sobrante por igual a ambos lados, como
  `.frame(…, fit="cover")`; antes recortaban desde la esquina superior
  izquierda.
- Una cuadrícula con columnas `"auto"` dentro de otra caja mide lo mismo que
  sola. Antes sus columnas se repartían el ancho por igual: la cuadrícula
  quedaba más ancha y, con textos que pueden partirse, el más ancho se partía
  en dos líneas.
- `box.reflow()` hace sitio a un hijo escalado o girado desde que se colocó,
  como dice la documentación; antes lo medía con su tamaño original y se
  solapaba con sus hermanos. `scale_to`, `rotate_to` y los demás ajustes de
  forma inmediatos funcionan sobre un hijo de una caja; antes lanzaban
  `LayoutOwnershipError`. Una caja anidada escalada ocupa el sitio de su
  tamaño escalado, como cualquier otro hijo; antes ocupaba el de su tamaño
  sin escalar.
- `obj.animate.place(zona, fit="stretch")` estira el objeto a la zona; antes
  lo escalaba como `"contain"`.
- El botón de descarga de la portada baja el paquete más reciente que exista
  para tu sistema y, si GitHub no responde, abre la página de la última
  release. Antes podía apuntar al de una versión aún sin publicar y dar un
  404. Las releases se publican cuando ya tienen sus paquetes.
- Las restricciones entre posiciones ya no encogen objetos: dos relaciones
  en conflicto sobre los bordes de una caja, como `(sq.center_x ==
  caja.left).weak()` y `(sq.center_x == caja.right).strong()`, dejan la caja
  de su tamaño; antes la reducían a cero. `(texto.width <= 0.5).weak()` ya
  no aplasta los glifos.
- `group.animate.fill(color)` y `group.animate.stroke(color, ancho)` animan a
  todos los miembros del grupo, también en grupos anidados, como ya hacían
  `group.fill()` y `group.stroke()`; antes no cambiaban nada.
- `glow`, `blur`, `shadow` y `no_effects` después del primer `play` o `wait`
  son cortes en el cursor, como `fill`; antes se ignoraban y el objeto seguía
  con los efectos de su declaración. `blur(0)` quita el desenfoque y sirve de
  inicio nítido para `animate.blur`; antes lanzaba `ValueError`.
- `L.column`, `L.row`, `L.grid` y `L.stack` mantienen su dirección aunque un
  `style=` o una clase traigan otro `direction`.
- El stub de `Zone.arrange` incluye `direction="wrap"`, que ya funcionaba.
- Exportar a MP4, WebM, GIF, WebP o MOV crea la carpeta de salida si no
  existe, como ya hacían la secuencia PNG y el paquete. Antes fallaba con
  «FFmpeg stopped accepting frames»: por ejemplo al exportar desde el editor
  un script suelto, cuya carpeta por defecto es `exports/` junto al script.
- `blend("hue")`, `"saturation"`, `"color"` y `"luminosity"` ya no pintan
  columnas negras sobre un degradado gris: ciertos niveles de gris salían
  negros en toda la caja del objeto mezclado, también fuera de él, en la vista
  previa, al exportar y en los reproductores web.
- `box.set(color=…)` recolorea los textos que la caja creó a partir de
  cadenas, como al crearla, y con `duration` lo anima; antes solo valía para
  las cadenas añadidas después. Un texto de una caja recoloreado (con `set` o
  con `fill`) conserva el color cuando la caja cambia de ancho y lo vuelve a
  partir; antes recuperaba el de su declaración.
- `box.replace(viejo, nuevo)` y `box.remove(viejo)` en una caja con fondo
  dejan ver al hijo viejo hasta el cambio; antes, si se creó antes que sus
  hermanos, el fondo lo tapaba desde el principio de la escena.
- Un texto oculto con `opacity(0)`, o que salió con un fundido, sigue oculto
  al colocarlo en una caja; antes la caja lo mostraba opaco hasta que se
  animaba su opacidad.
- `box.reveal()` y `box.cascade(…)` hacen aparecer un `rolling_number` o un
  `progress_ring` de la caja como cualquier otra pieza; antes pasaban por el
  `animate` de su valor y el contador se veía desde el principio.
- `scene.geometry.traced_path(objeto)` de un objeto simulado con
  `add_updater_fn(…, fixed_dt=…)` que además tiene animaciones propias, como
  un `fade_in`, sigue la simulación al buscar un instante (en el editor y en
  las capturas). Antes el objeto volvía a su posición inicial en la mayoría de
  los cuadros y el rastro acababa con una recta hacia ella; había que trazar
  un punto con `follow(objeto)`.
- Un `traced_path` con `dissipating_time` ya no deja en algunos cuadros un
  bloque relleno que parte de su cola: cuando la cola estaba a punto de
  alcanzar su siguiente muestra, el primer tramo daba media vuelta y el trazo
  quedaba abierto. Los dos extremos de un rastro siguen su recorrido; antes
  podían acabar en un pequeño gancho.
- `race.bar(nombre).animate.opacity(…)` atenúa una fila de una carrera de
  barras, y el desvanecimiento de las filas que salen del top sigue
  funcionando encima. Antes lanzaba un error sobre un `Parameter` inaccesible.
- `chart.layer(nombre)` con una capa que el gráfico no tiene (`grid`,
  `guides` o `labels`) dice por qué falta y qué capas tiene. Antes respondía
  con la lista de nombres válidos, que incluía el pedido.

== Al actualizar

- Un `hidden()` dentro de un grupo, o en una marca de un plano, ya no aparece
  con la entrada del grupo o el `create` del plano. Si contabas con ello,
  anima el objeto (`fade_in`, `create`…) o quita el `hidden()`.
- Una órbita de cámara con `up=(0, 0, 1)` rodea la escena en vez de pasar por
  encima; si buscabas ese giro, usa `look_at` con el ojo que quieras.
- Los planos con el eje X abajo muestran el cero del eje Y.
- `scene.camera.animate.orthographic()` ya no es instantáneo: dura un
  segundo, o la `duration` de `scene.play`, y avanza el cursor. Para el salto
  de antes, usa `scene.camera.orthographic()` o `.duration(0)`.
- El título de `solarized-light` y el color `muted` de `catppuccin-latte` son
  un poco más oscuros.
- `text.animate.move_to(x, y)` sin `anchor` lleva el texto a su línea base,
  como `text.move_to`. Un texto que se animaba a un punto pensando en su
  centro queda más arriba; pasa `anchor="center"` para conservar la posición
  anterior.
- `image(…, width=…, height=…, fit="cover")` y `video(…, fit="cover")`
  muestran el centro de la imagen en lugar de su esquina superior izquierda.
  Para encuadrar otra parte, usa `crop`.
- Las formas con degradado que se transforman, o que entran o salen en un
  `magic_move`, se ven con su degradado durante y después del morph en lugar
  del color de su primera parada. Si contabas con ese color liso, dale un
  `fill` liso al objeto y al destino.
- Si transformabas un SVG hoja a hoja (con un `id` en cada camino) para
  esquivar el fallo, sigue funcionando; ahora también puedes transformar el
  SVG o el grupo entero.
- Los fotogramas intermedios de un morph entre contornos de sentidos opuestos
  cambian (los extremos son los mismos). Si normalizabas el sentido de los
  contornos para evitar el colapso, ya no hace falta.
- Los bloques de `scene.text.code` salen coloreados y con el código alineado
  arriba a la izquierda. Para recuperar el aspecto monocromo, pasa
  `highlight=False`; si colocabas otros objetos respecto al código, revisa su
  posición.
- El texto con `role="code"` y los bloques de código cambian de fuente a
  JetBrains Mono, un poco más ancha que Consolas: revisa lo que alineabas con
  ellos. Para volver a la anterior, nómbrala en el tema, como
  `Theme(fonts={"code": "Consolas"})`, o en `font=` del bloque.
- En el editor, un WebM exportado desde un paquete abierto o desde un script
  fuera de un proyecto ya no conserva el canal alfa por defecto, como
  ocurre con `gaanim export`: marca *Conservar la transparencia* si la escena
  tiene un fondo transparente.
- `indicate()` crece más (1,2 en lugar de 1,1) y tiñe el objeto: para el
  salto de antes, usa `indicate(color=..., scale_factor=1.1)` con el color
  del propio objeto. `wiggle()` sacude el doble; `wiggle(amplitude=0.05)`
  recupera la sacudida anterior.
- Las escenas que usaban `flash()`, `circumscribe()` o los énfasis de texto
  `pulse`, `wave` y `highlight` se ven distintas: revisa sus tomas.
- Un `play(..., duration=...)` que incluye `magic_move` sin `duration` o
  movimientos de cámara sin `.duration()` ahora los alarga o acorta a esa
  duración. Fija `.duration(1)` si contabas con el segundo de antes.
- Los efectos que antes ignoraban su lugar en un `sequence` o `stagger` ahora
  esperan su turno, así que la escena puede durar más.
- `fill_level(direction=...)` nombra siempre hacia dónde crece el relleno:
  `"right"` crece desde el borde izquierdo y `"left"` desde el derecho, como
  `"up"` crece desde abajo. Antes `"left"` y `"right"` nombraban el borde de
  partida: cámbialos entre sí.
- `cross()` y `right_angle()` se ven con el trazo del tema; si les dabas
  un `stroke`, no cambia nada.
- Con `role="code"`, un `\$` muestra la barra invertida y el dólar, como el
  resto del código; antes mostraba solo el dólar. Quita la barra si la
  escribías para escapar el dólar.
- Un `word_spacing` en la raíz, que antes no tenía efecto, ahora separa las
  palabras: quítalo si no lo querías.
- Los textos con `max_lines` se cortan; con `overflow="visible"` se ven
  todas las líneas.
- `write()` sobre un grupo con textos (ejes, matrices, bloques de código)
  escribe cada texto desde su primer carácter: los fotogramas intermedios
  cambian, el resultado final no.
- `write()` y `unwrite()` sin duración duran 2 s en textos y grupos de 15
  piezas o más, así que la escena puede alargarse: fija `.duration(1)` si
  contabas con el segundo de antes. Sus fotogramas intermedios cambian (la
  mitad del tiempo de cada pieza traza el contorno y la otra mitad lo
  rellena), y un objeto con trazo lo dibuja fino y luego lo engrosa.
- Los documentos de `scene.text.typst` usan el color de texto del tema en
  lugar de blanco o negro puro. Si querías blanco, ponlo en el documento con
  `#set text(fill: white)`. Un documento que definía su propio `theme`
  sigue usando el suyo.
- Una caja escalada o girada escala y gira también a sus hijos. Si la
  escalabas para separarlos, usa `gap` o `row_gap`/`column_gap`.
- `write()` agrupado con `by="word"`, `"line"` o `"part"` escribe hasta
  cuatro grupos uno tras otro y más con solape, así que sus fotogramas
  intermedios cambian; `stagger=0.2` recupera el solape anterior.
- Las ecuaciones con varias filas ocupan más alto; `line_spacing` por debajo
  de 1 las vuelve a juntar.
- `scene.text.equation` lanza `ValueError` con matemática inválida: corrige la
  fuente que antes no se dibujaba.
- `matrix.morph_to(destino)` muestra el destino y oculta el origen al
  terminar; antes el origen tomaba el aspecto del destino y el destino seguía
  oculto.
- Un objeto conserva su ancho y su alto frente a las restricciones medias y
  débiles: si una `.weak()` o `.medium()` tenía que redimensionarlo, ahora
  aparece en `check_layout()` sin cumplirse. Hazla `.strong()` para que lo
  redimensione.
- `shadow=True` sobre un fondo oscuro dibuja una sombra casi opaca en lugar
  de la suave que no se veía. Para la de antes, pasa
  `shadow={"color": "#00000055"}`.
- `box.reflow()` mide a los hijos con su escala actual: si escalabas un hijo
  con `.animate` y luego llamabas a `reflow()`, sus hermanos se apartan. Una
  caja escalada antes de meterla en otra también les hace sitio.
- Un `L.column(..., style=estilo)` cuyo estilo trae `direction="row"` vuelve
  a ser una columna: pasa `direction="row"` en la llamada o usa `L.row`.
- Si construías una curva ya en su sitio para que `point_on_curve` y los
  demás trackers coincidieran con ella, sigue funcionando; ahora también
  puedes moverla después.
- Una flecha con `fill(color)` y sin `stroke` tiene ahora el contorno del
  mismo color; antes era el del tema. Si querías el contorno del tema, fíjalo
  con `stroke(...)`.
- Para iluminar la caja conjunta de varios objetivos con `fx.spotlight`,
  pásale un grupo: `fx.spotlight(scene.geometry.group([a, b]))`.
- Si compensabas `with_pivot` en las partes de un SVG pasando coordenadas del
  documento, pasa ahora el punto de la escena. La fórmula de la guía
  #link("/guias/personajes/")[Personajes] ya lo hacía.
- `Transition.morph(..., easing=Easing.SMOOTH)` se ve algo más rápido al
  principio y al final: el easing ya no se aplica dos veces.
- Después de `transform_matching_shapes`, el origen queda oculto y el destino
  visible: anima el destino para seguir moviendo lo que se ve.

= 0.11.0

Publicada el 10 de octubre de 2026. Gaanim llega al navegador: el playground
ejecuta escenas de Python sin instalar nada, y el reproductor web se inserta
en un blog o un sitio web como una figura más. Trae capas de parallax con
profundidad de campo, texto que sigue una trayectoria, notaciones a mano
alzada, movimientos de cámara (seguir una ruta, barridos y dolly zoom) y
extrusión 3D, y el editor suma un inspector, una consola, un panel de
animaciones, capturas de fotogramas y secuencias PNG. Lee «Al actualizar» si
alguna escena mueve objetos por una trayectoria.

== Playground y reproductor web

- El #link("https://paololupo.github.io/gaanim/playground/")[playground]
  ejecuta escenas en el navegador. Escribes Python en un editor con
  autocompletado, firmas y la documentación de la API al pasar el cursor, y
  la escena se graba en un `.gaanim` que el reproductor web muestra al lado;
  un error marca su línea, también dentro de un módulo. Cada trabajo es un
  proyecto, como en un editor de código: el explorador crea, sube (archivos o
  carpetas enteras), renombra, mueve y elimina archivos, `main.py` importa los
  módulos de sus carpetas, y las imágenes, la música y las fuentes que subes
  (`Theme(font_dir="assets/fonts", …)`) se quedan en tu navegador. Hay varios
  proyectos: se crean vacíos o desde un ejemplo de la galería, se duplican y
  se abren desde un `.zip`, también la carpeta de un proyecto de la
  aplicación. Los paquetes de PyPI se declaran en `pyproject.toml` (la lista
  de `uv add`) o en `requirements.txt` y se instalan con micropip.
  `Ctrl+S` guarda y ejecuta, *En vivo* ejecuta al dejar de escribir, la
  grabación muestra su avance y su tiempo, y *Detener* cambia a un Python que
  ya se estaba cargando. *Compartir* copia un enlace con los archivos de
  texto, y *Descargar* convierte la grabación en vídeo MP4 o WebM, con su
  audio, o en GIF, en el navegador, o guarda el `.gaanim` o el proyecto en un
  `.zip`. Tiene tema claro y oscuro, el mismo que esta documentación. Usa
  Pyodide (Python 3.14 en WebAssembly); lo que necesita FFmpeg, como los
  vídeos como recurso, solo funciona en la aplicación. Ver
  #link("/empezar/playground/")[El playground].
- *Insertar*, en el playground, prepara la escena para un blog o un sitio
  web: descarga el `.gaanim` con el nombre que elijas, compone el `<iframe>`
  con la dirección donde lo publicarás y la proporción de la escena, y muestra
  cómo se verá.
- Un reproductor ligero, `reproductor/insertar/?src=`, pone una animación
  en otra página: descarga unos 3 MB en lugar de los 24 del reproductor
  completo, empieza cuando está a la vista y se pausa al salir de ella, para
  que quien lee la entrada vea la animación al llegar a ella. `loop` la
  repite, `controls=0` oculta su barra fina (un clic pausa o reanuda) y
  `autoplay=0` espera un clic. Se detiene en cada `scene.stop()` hasta un
  clic y repite el bucle de la pausa (`loop=`) mientras espera, como al
  presentar. Una escena con posprocesado o con fondos,
  transiciones o efectos de shader se abre en el reproductor completo. El
  playground también lo usa. Ver
  #link("/guias/compartir/#insertar")[Insertar en un blog o sitio web].

== Motion design

- *Capas de parallax.* `scene.layer(depth)` devuelve una `ParallaxLayer`, y
  lo que le añades con `.add(...)` se mueve con `1/depth` del paneo de la
  cámara y se acerca con `zoom ** (1/depth)`: el fondo lejano apenas se
  mueve y el primer plano se mueve más. Cada capa se ordena por su
  profundidad; `depth=math.inf` queda fija como un HUD. Ver
  #link("/referencia/scene/#parallax")[Capas de parallax].
- *Profundidad de campo.* `scene.camera.depth_of_field(focus, aperture=...,
  max_blur=...)` desenfoca cada capa según lo lejos que esté de la distancia
  de enfoque, también en exportaciones, capturas y paquetes; lo que no está
  en una capa, y los HUD, nunca se desenfoca. `scene.camera.animate.focus_to`
  cambia el foco de una capa a otra (rack focus) y puede ir a la vez que un
  movimiento de cámara. Ver
  #link("/referencia/scene/#profundidad-campo")[Profundidad de campo].
- *Texto en una trayectoria.* `scene.text.on_path(texto, ruta, align=...,
  orient=..., reverse=..., offset=...)` coloca las letras a lo largo de un
  trazado, y `text.path_offset(...)` o `animate.path_offset(...)` las hacen
  correr por él. Ver #link("/referencia/text/#texto-trayectoria")[Texto
  sobre una trayectoria].
- *Notaciones a mano alzada.* `objeto.annotate` (y `seleccion.annotate`
  sobre parte de un texto) subraya, encierra en una caja o un círculo,
  tacha, cruza o pone corchetes con un trazo irregular, como dibujado a
  mano. Cada marca sigue a su objeto y se dibuja con `animate.create()`. Ver
  #link("/referencia/drawable/#anotaciones")[Anotaciones a mano alzada].
- *Movimientos de cámara.* `scene.camera.animate.follow_path(ruta,
  orient=...)` lleva la cámara por un trazado, `whip_pan(destino)` es un
  barrido rápido que en exportaciones y capturas lleva desenfoque de
  movimiento, y `dolly_zoom(factor)` acerca la cámara en perspectiva
  mientras el sujeto conserva su tamaño. Ver
  #link("/referencia/scene/#camara")[Cámara].
- *Extrusión 3D.* `scene.geometry.extrude(objeto, depth, bevel=...)`
  convierte una figura, un texto, una ecuación, un grupo o un SVG en un
  sólido 3D. Ver #link("/referencia/geometria/")[Geometría].
- `Axonometric` se importa desde `gaanim`; en 0.10.2 solo estaba en
  `gaanim.gaanim_core`.

En la web, estas animaciones se reproducen en el reproductor ligero, el mismo
que se inserta en un blog: pasa el cursor por encima para ver su barra.

```python
# output: preview.webp
# player
from gaanim import *

scene = Scene(frame=(16, 9))
title = scene.text("Anotaciones", role="title").move_to(0, 2.6)
card = scene.geometry.rounded_rect(3.2, 1.6, 0.2).fill(BLUE).move_to(-3.2, -0.6)
word = scene.text("importante").move_to(3.2, -0.6)
ul = title.annotate.underline(color=RED, seed=1)
box = card.annotate.box(color=GOLD, seed=2)
circle = word.annotate.circle(color=RED, seed=3)
scene.play([ul.animate.create(), box.animate.create()], duration=1.2)
scene.play([circle.animate.create()], duration=0.9)
scene.render()
```

```python
# output: preview.webp
# player
import math
from gaanim import *

scene = Scene(frame=(16, 9), background="#1d2b53")
sky = scene.layer(depth=math.inf).add(scene.geometry.circle(0.9).fill(GOLD).move_to(4.5, 2.6))
far = scene.layer(depth=3.0)
for i in range(6):
    far.add(scene.geometry.polygon([(-2.4, 0), (0, 2.6), (2.4, 0)]).fill("#3b4a7a").move_to(-8 + i * 4.2, -1.4))
for i in range(8):
    scene.geometry.rect(0.5, 1.2).fill("#8a5a3b").move_to(-6 + i * 3, -2.0)
near = scene.layer(depth=0.6)
for i in range(5):
    near.add(scene.geometry.circle(0.9).fill("#1f4d33").move_to(-7 + i * 5, -3.3))
scene.play([scene.camera.animate.pan_to(6, 0).duration(3)])
scene.render()
```

```python
# output: preview.webp
# player
from gaanim import *

scene = Scene(frame=(16, 9))
circle = scene.geometry.circle(2.4).no_fill().stroke(GRAY, 0.03)
ring = scene.text.on_path("GAANIM · MOTION · DESIGN · ", circle, reverse=True, size=0.5)
scene.play([ring.animate.path_offset(1.0).duration(4).easing(Easing.LINEAR)])
scene.render()
```

== Editor

- *Inspector.* Un clic en la vista previa selecciona un objeto, y el
  inspector muestra su tipo y su nombre, la línea del script que lo creó (con
  *Abrir* y *Copiar*, y las líneas que la llamaron), su posición, rotación,
  escala, tamaño, relleno, trazo y opacidad, su capa y su profundidad de
  parallax. *Dentro de…* sube al grupo que lo contiene. Sin selección muestra
  la cámara, profundidad de campo incluida.
- *Consola* (`J`): avisos de Gaanim y de Python, lo que imprime `print`, los
  errores con su traza y las recargas, cada fila con un enlace a su línea,
  con filtros por gravedad y texto. Mientras está oculta, una marca arriba a
  la izquierda cuenta los problemas.
- *Animaciones* (`T`): una línea de tiempo sobre la barra de reproducción,
  con una fila por objeto animado y la cámara y un bloque por cada `play`.
  Un clic en un bloque va a su inicio y selecciona el objeto; un doble clic
  abre la línea del `play`.
- `Ctrl+Mayús+S` (o el botón de captura) guarda el fotograma actual como PNG,
  sin overlays, en `<output_dir>/captures/`.
- El diálogo de exportación graba secuencias PNG, nombra el archivo como el
  proyecto (o el script, si no es `main.py`), elige la carpeta con el
  selector del sistema y avisa antes de reemplazar un archivo.
- *Abrir* usa el editor de `GAANIM_EDITOR` (con `{file}` y `{line}`) o,
  sin esa variable, el primero de `code`, `cursor`, `codium`, `zed` o `subl`
  que encuentre. Ver #link("/referencia/cli/")[CLI].

== Correcciones

- Seguir una trayectoria (`move_along`, `path_arc`, rotar alrededor de un
  pivote externo) usa la longitud de arco exacta: el objeto ya no da saltos
  de hasta 0,15 unidades ni retrocede, y con `orient=True` no gira 180° un
  instante. Con orientación es de dos a cuatro veces más rápido.

== Al actualizar

- Los objetos que siguen una trayectoria pueden quedar unos píxeles más allá
  o más acá que en 0.10.2 donde el muestreo anterior saltaba, y su
  orientación final usa la tangente exacta. No hay forma de volver al
  muestreo anterior, que era un error; si una composición dependía de esa
  posición, ajusta la fracción de la trayectoria.
- En el editor, un clic en un texto selecciona el texto entero, no un glifo,
  y entre objetos con el mismo `z_index` gana el más pequeño.

= 0.10.2

Publicada el 6 de octubre de 2026. Recargar y abrir presentaciones largas es más rápido: medir con
`bounds()` reutiliza lo ya compilado, un `traced_path` ya no encarece los
saltos, guardar sin cambios conserva la escena, cambiar un asset recompila
solo lo que lo usa, el script arranca mientras se prepara la ventana y
`GAANIM_RELOAD_PROFILE=1` dice en qué se va el tiempo. Trae capas de dibujo con nombre, un aviso de
textos tapados en `gaanim check` y dibujos axonométricos. Corrige un fallo de
Typst en sesiones largas, el ancho de los trazos de contornos cerrados, las
recargas tras guardar con herramientas que escriben un temporal y tres
detalles de la API. Lee «Al actualizar» si alguna escena traza contornos
cerrados o entra con `fade_in` a un objeto con opacidad declarada.

== Cambios

- Las mediciones con `bounds()` (y `Anim.target_bounds`, `scatter` y
  `avoid_boxes`) comparten una compilación mientras no escribas nada entre
  ellas, y los miembros de un mismo layout recién creado se miden con una
  sola. Junto con el cambio siguiente, el Python de una recarga de la
  presentación de prueba (43 segmentos) bajó de 3,5 a 1,8 s.
- Un `traced_path` ya no obliga a cada salto en la línea de tiempo a recorrer
  la escena entera desde t=0, ni desactiva los puntos de control de los
  segmentos: en esa presentación, capturar sus 95 pausas pasó de casi tres
  minutos a 13 s. El rastro sale idéntico.
- Si guardas sin cambiar nada de lo que se compila (por ejemplo, solo un
  comentario), la recarga conserva la escena y sus puntos de control, y el
  aviso dice `(unchanged, N segments)`.
- Las recargas ya no recompilan los módulos de Python del proyecto que no
  cambiaron.
- El aviso de recarga incluye `seek`: lo que tardó volver a la posición donde
  estabas, que antes no contaba en el total.
- `GAANIM_RELOAD_PROFILE=1` escribe en la terminal qué costó cada recarga (el
  script por segmento, las mediciones y las líneas que compilaron la escena,
  las fases de la recompilación) y el arranque, fase por fase. Sin esa
  variable, Gaanim avisa en la terminal y en `gaanim check` cuando medir con
  `bounds()` compiló la escena a un coste notable, con las líneas del script
  responsables.
- La vista previa arranca en Vulkan (Metal en macOS) cuando la GPU lo admite,
  sin iniciar también DX12: en Windows llega al primer fotograma unos 0,3 s
  antes y sin las esperas de varios segundos que a veces causaba DX12.
- Al abrir un script, este empieza a ejecutarse mientras se preparan la
  ventana y la GPU, y encontrar su entorno de Python ya no arranca procesos:
  se leen el `pyvenv.cfg` y los metadatos del paquete instalado. En la
  presentación de prueba, la primera escena llega unos 1,3 s antes.
- Guardar un asset (una imagen, un Lottie, un archivo de datos) ya no vacía
  todas las cachés ni recompila la escena entera: se olvida solo lo que las
  cachés guardaban de ese archivo, y la recarga sigue siendo incremental
  desde el primer segmento que lo usa. Si la escena no lo lee, la recarga
  queda en `unchanged`. El aviso de recarga dice qué archivo cambió.
- Editar una diapositiva recompila con menos coste fijo: las huellas que
  deciden qué segmentos se reutilizan se calculan en paralelo, las
  transiciones entre segmentos y las capas procedurales ya no recorren todos
  los clips una vez cada una, y el punto de control se copia mientras se
  restaura la escena. Al editar la última diapositiva de la presentación de
  prueba, el replay pasó de 0,45 a 0,25 s (las huellas, de 100 a 17 ms). Un
  cambio inmediato sobre un objeto de un segmento anterior ya no impide
  reutilizar el segmento que lo declaró.
- Una medición con `bounds()` que compila la escena reanuda la compilación
  de la anterior desde el segmento donde cambió la escena, y la siguiente
  ejecución del script reanuda la última, salvo desde un segmento con
  funciones de Python (reactivas, updaters), que son nuevas en cada
  ejecución. En la presentación de prueba, la segunda medición de cada
  recarga pasó de unos 0,3 a 0,16 s.
- Capas de dibujo con nombre: `scene.z_layers("modelo", "overlay")` y
  `drawable.z_layer("overlay")` dejan lo que va encima (ventanas, lupas,
  etiquetas) sobre cualquier `z_index` del resto, que pasa a ordenar solo
  dentro de cada capa. Sin capas, todo se dibuja como antes.
- `gaanim check` avisa cuando una figura opaca dibujada encima tapa por
  completo un texto en una pausa o al final de un segmento, y dice cuál,
  detrás de qué y dónde. `covers_on_purpose()` marca una figura, o un grupo,
  que tapa textos a propósito, como un diálogo sobre el modelo, para que no
  avise ni haga fallar `--strict`.
- Dibujos axonométricos: `scene.geometry.axonometric("isometric", ...)`
  dibuja puntos de un modelo 3D (con `z` hacia arriba) como polígonos y
  polilíneas planos, con el mismo estilo que el resto de la escena.
  `depth_sort` ordena las caras como un pintor y `animate_to` pasa de una
  vista a otra (de planta a isométrica, por ejemplo) moviendo los vértices.
  Ver #link("/referencia/geometria/")[Geometría].

== Correcciones

- Compilar Typst en una sesión larga ya no falla con «out of file ids». Cada
  compilación gastaba uno de los ~65 000 identificadores de archivo de Typst;
  al agotarse, las miniaturas del presentador dejaban de generarse y fallaba
  toda compilación Typst posterior.
- `bounds()` justo después de un cambio inmediato sobre un objeto ya animado,
  como un `move_to`, mide la posición nueva. Según cómo sumaran las
  duraciones de la escena, medía un instante antes del cambio.
- `with_pivot` sobre un objeto ya mostrado cambia el pivote en el cursor sin
  moverlo, y las rotaciones, escalas y sesgos siguientes lo usan. Antes se
  ignoraba sin aviso.
- `points()` y `animate.points()` aceptan una polilínea con `closed=True`, que
  sigue cerrada. Antes lanzaban `ValueError`.
- `fade_in()` termina en la opacidad declarada o fijada del objeto, o en la
  que tenía antes de un `fade_out`, en lugar de 1. También las entradas por
  trazo (`create`, `write`) tras un `fade_out`.
- `stroke(color, ancho)` en un contorno cerrado, con el trazo hacia dentro
  (el valor por defecto), dibuja el ancho entero dentro de la figura, como
  `"outside"` lo dibuja entero fuera. Antes se veía la mitad: el trazo
  centrado, recortado por el contorno.
- Guardar un `.py` escribiendo un temporal y renombrándolo sobre el archivo
  (la herramienta Edit de Claude Code, `sed -i`) o restaurarlo con `git
  checkout` recarga como un cambio de código, con el replay incremental. Antes
  contaba como un cambio de asset: vaciaba las cachés y recompilaba la escena
  entera. Ni los temporales de guardado ni la carpeta del archivo, que Windows
  también da por cambiada, cuentan ya como cambios.

== Al actualizar

- Los trazos de contornos cerrados hacia dentro (sin `align` o con
  `align="inside"`), incluidos los de los glifos, se ven con el ancho
  declarado: el doble que en 0.10.1. Para conservar el aspecto anterior,
  divide entre dos su ancho. Los trazos centrados, hacia fuera o de caminos
  abiertos no cambian.
- Un objeto declarado con una opacidad menor que 1 que entra con `fade_in`
  (o con `create` o `write` tras un `fade_out`) termina en esa opacidad, no
  en 1. Si contabas con llegar a 1, quita la opacidad de la declaración.

= 0.10.1

Publicada el 4 de octubre de 2026. Corrige una caída de fotogramas al mostrar
la barra de reproducción en presentaciones largas abiertas desde su script.

== Correcciones

- La barra de reproducción ya no frena el editor en presentaciones con muchas
  diapositivas. Calculaba los capítulos recorriendo todos los clips una vez
  por escena en cada fotograma, y la vista previa bajaba a unos 30 fps
  mientras la barra estaba visible. Los paquetes `.gaanim` no se veían
  afectados.

= 0.10.0

Publicada el 4 de octubre de 2026. Rendimiento de la vista previa y de la
exportación: exportar es entre un 13 y un 49 % más rápido, saltar en la línea
de tiempo cuesta entre un 7 y un 35 % menos, el editor descansa cuando no hay
nada que cambiar y GIF, WebP, WebM y secuencias PNG se exportan más rápido.
Lee «Al actualizar»: los GIF y WebP salen a 30 fps como mucho y las
exportaciones respetan los fps que pidas.

== Cambios

- La exportación prepara el fotograma siguiente mientras la GPU dibuja el
  actual, y los fotogramas que repiten el anterior (un `wait()`, una pausa)
  no se vuelven a dibujar.
- La exportación y las capturas reutilizan el dibujo de cada objeto que no
  cambió; los efectos por objeto independientes se dibujan con un solo
  render, y las mallas 3D con sombreado suave ya no recalculan sus
  degradados en cada fotograma.
- Las mallas 3D se proyectan y se preparan en varios núcleos: la escena 3D del
  benchmark exporta en la mitad de tiempo que en la 0.9 y se reproduce en el
  editor a 144 fps en lugar de 90.
- Los ecos de un grupo se evalúan una vez por nivel de eco en lugar de una vez
  por copia, y los efectos por objeto que cambian de tamaño ya no recompilan
  sus pases en cada fotograma.
- El motion blur prepara cada subfotograma mientras la GPU dibuja el anterior
  y los promedia en varios núcleos: exportar con motion blur tarda la mitad.
- Las sombras difuminadas, los brillos y el desenfoque se difuminan en la
  GPU una sola vez por objeto: un texto con sombra que se mueve exporta cinco
  veces más rápido.
- Los MP4 y WebM pasan a YUV en la GPU, que envía a FFmpeg menos de la mitad
  de bytes por fotograma.
- Los morph de una figura, un texto o una fórmula a otra emparejan sus
  contornos una vez por animación en lugar de en cada fotograma.
- Una escena sin segmentos que encadena cientos de animaciones salta desde el
  último `play` en el que todas terminaron, en lugar de repetirlas desde el
  principio.
- Las capturas de `--diff` codifican los PNG en varios hilos mientras se
  dibujan los fotogramas siguientes.
- Exportar arranca entre 0,3 y 0,5 s antes en Windows, porque la GPU se busca
  primero en Vulkan.
- Saltar en la línea de tiempo reutiliza lo ya dibujado: la línea de tiempo
  ya no reescribe valores que no cambian.
- Con la vista previa en pausa o en el Inicio, el editor espera a la
  siguiente entrada en lugar de recalcular la escena en cada refresco.
- WebM usa varios núcleos y se exporta un tercio más rápido; las secuencias
  PNG se codifican en varios hilos y el borrador de MP4 usa `veryfast`.

== Correcciones

- Los GIF y WebP en calidad estándar o de producción salían a 60 fps y se
  veían a cámara lenta en los navegadores. Ahora salen a 30 fps.
- `scene.export(..., fps=...)` y los ajustes del diálogo de exportación ya no
  se sustituyen por los del preset.
- Las imágenes de los efectos por objeto que dejan de dibujarse se liberan.
- Un FFmpeg que escribe mucho en su salida de error ya no puede bloquear la
  exportación.
- Las escenas 3D con más de unos 8000 triángulos visibles con sombreado suave
  ya no fallan al exportar.
- La recarga en caliente ya no vigila `.venv`, `.git` ni `target`.
- Un contador animado con Typst ya no hace crecer la memoria sin fin.
- Al arrastrar la barra de tiempo, el vídeo ya no parpadea con fotogramas
  anteriores.
- `Updater.bob`, `Updater.pulse` y `Updater.rotate` con un número dependen
  solo del tiempo: ya no varían con la cadencia de fotogramas, oscilan
  alrededor de la posición animada del objeto en lugar de la que tenía al
  añadirlos y se combinan en un mismo objeto (antes el último sustituía a los
  anteriores).

== Al actualizar

- Los GIF y WebP salen a 30 fps como mucho en cualquier calidad.
- Los fps que pases a `scene.export` se respetan; antes se usaban los del
  preset.
- Los MP4 con `libx264` usan `-tune animation`, y los WebM salen algo más
  grandes con la misma CRF.
- Las sombras difuminadas, los brillos y el desenfoque son un difuminado
  gaussiano real: sus bordes cambian levemente y un objeto desenfocado
  muestra su color exacto.
- Las aristas de las mallas 3D opacas se sellan con un trazo centrado: la
  silueta sobresale medio píxel más que antes.
- Un objeto con `Updater.pulse` y `Updater.rotate` ahora late y gira; antes
  solo giraba. `remove_updater()` devuelve un objeto con `Updater.bob` o
  `Updater.pulse` a su posición y escala.

= 0.9.0

Publicada el 3 de octubre de 2026. Transiciones por shader: cada segmento se
dibuja por separado y la GPU los mezcla con una función WGSL, como en
gl-transitions. Llegan cinco listas para usar y la posibilidad de escribir las
tuyas. Y el audio anima la escena: el volumen, las bandas de frecuencia y los
golpes de una pista mueven, giran y tiñen cualquier objeto. Cualquier objeto
puede ser de vidrio, esmerilado o líquido como el _Liquid Glass_ de iOS, pasar
por sus propios shaders, mostrarse a través de un mate o fundirse como gotas
con los metaballs. Además, los SVG
con transformaciones conservan sus degradados, recortes y trazos, los grupos
se funden como una capa y las sombras de texto cuestan una por objeto. Lee «Al
actualizar»: los paquetes `.gaanim` de 0.8 deben grabarse de nuevo y el
interlineado por defecto es más compacto.

== Cambios

- `Transition.preset(nombre, duración, **ajustes)` aplica una transición
  incorporada: `cross_zoom`, `directional_warp`, `ripple`, `glitch_displace` o
  `luma`, que revela el segmento entrante siguiendo una imagen en escala de
  grises (`image=`). Lee #link("/guias/transiciones/")[Transiciones].
- `Transition.shader(wgsl, duración, uniforms={...})` acepta tu propia función
  `transition(uv)`, que lee `gaanim_from(uv)`, `gaanim_to(uv)` y `progress`.
- Cada segmento se dibuja con su fondo; los objetos persistentes y los
  overlays quedan nítidos por encima de la mezcla. Funcionan en la vista
  previa, la exportación, los paquetes y el reproductor web, y un seek cae en
  el mismo fotograma que la reproducción.
- Una pista de audio da señales de 0 a 1 que animan cualquier cosa que acepte
  un número, y siguen a la pista donde suena: `level()` (el volumen),
  `band(low, high)` (una banda de frecuencias) y `pulse(decay)` (los golpes).
  `beats()` da los instantes de los golpes y `duration`, lo que dura la pista.
  `pulse` y `beats` aceptan una banda (`low=40, high=120` sigue el bombo).
- `Audio.tempo()` estima el BPM y la fase del primer pulso para
  `scene.tempo`, y `Audio.analysis()` da el análisis como listas de Python
  para colocar cortes a mano. `Audio.level(range_db=24)` mide el volumen en
  decibelios.
- `Updater.rotate`, `Updater.wiggle` y el nuevo `Falloff.source` aceptan una
  señal del tiempo, así que el audio también gira, hace temblar y tiñe con
  `drive("fill", ...)`. Lee #link("/referencia/audio/")[Audio].
- `Audio.spectrum(bandas, normalize=)`, `Audio.waveform(puntos, span=)` y
  `Audio.samples(puntos, span=)` (la onda con signo, para un osciloscopio)
  dan una señal por banda o por instante reciente, y `scene.viz.equalizer` las dibuja con
  la forma (`"bar"`, `"capsule"`, `"dot"`, `"line"`, `"area"` o la tuya), la
  disposición (`"row"`, `"mirror"`, `"radial"` o la tuya) y los colores que
  elijas. También sirven como uniforms de un shader, y
  `PostProcess.shader(..., audio=)` le da el espectrograma entero
  (`gaanim_audio_spectrum(x, ago)`). `Background.shader` y
  `Transition.shader` aceptan uniforms que siguen señales (y el fondo,
  `audio=`). Lee
  #link("/referencia/audio/")[Audio].
- `scene.geometry.polyline` acepta señales en sus coordenadas y
  `closed=True`.
- `Drawable.shader_effect(efecto, margin=)` aplica pases de `PostProcess`
  (presets, tu WGSL o una cadena) a un solo objeto y sus hijos, en su sitio
  y con uniforms animados por `Parameter` o por el audio. Lee
  #link("/referencia/drawable/")[Drawable].
- Los paquetes `.gaanim` graban la tiza y los efectos de shader por objeto.
- `Drawable.liquid_glass()` convierte un objeto en vidrio líquido como el
  _Liquid Glass_ de iOS: el centro deja ver lo que hay detrás sin deformarlo
  y el borde lo curva como una lente, lo desliza a lo largo del contorno
  (`twist`), lo separa en colores (aberración cromática, `dispersion`) y
  brilla con una línea fina de luz. Sus ajustes (`bevel`, `refraction`,
  `blur`, `transparency`…) son los de `Drawable.glass`, que hace vidrio
  esmerilado; `backdrop_blur` solo desenfoca lo que hay detrás. El borde
  sigue la forma real del objeto, así que unos metaballs de vidrio se funden
  como gotas. Lee #link("/referencia/drawable/")[Drawable].
- `Drawable.matte(fuente, mode=)` muestra un objeto solo a través de otro
  (alpha o luma, normales o invertidos), y `scene.geometry.metaballs` funde
  bolas como gotas. Lee #link("/referencia/drawable/")[Drawable] y
  #link("/referencia/geometria/")[Geometría].
- `echo(..., start=, end=)` limita la estela a un intervalo de la escena.
- `drawable.copy()` duplica un objeto; en un SVG copia también sus partes con
  nombre y sus recortes (`copia.part("cabeza")`).
- `keyframes(offset=[...])` mueve el objeto relativo a donde está al empezar.
  `position` fija su origen, no su centro visual como `move_to`.
- Sin carpeta de recursos, las rutas relativas se buscan también en la
  carpeta `assets/` del script y en la del script.
- `GAANIM_FRAME_PROFILE=1` informa de caminos, segmentos y recortes.
- El fondo llena la vista aunque la cámara salga del cuadro de la escena.
- `readout` y `variable` aceptan `font` y `weight`.

== Correcciones

- `uniforms=` de `Background.shader`, `PostProcess.shader` y
  `Transition.shader` acepta cualquier `Mapping`, y los stubs lo tipan así:
  un diccionario de señales de audio ya no da error en el comprobador de
  tipos.
- Los alias de tipo de los stubs (`Paint`, `ColorLike`, `Padding`…) existen
  también al ejecutar la escena: `from gaanim.gaanim_core import Paint` ya no
  lanza `ImportError`. `Paint`, `ColorLike`, `ColorMapLike` y
  `BackgroundLike` se importan también desde `gaanim`.
- Un `blur_in` con `.delay(...)` mantiene sus glifos transparentes durante
  el retardo; antes dejaba una trama tenue donde iba a entrar el texto.
- Un `Background.shader` sigue animándose y siguiendo el audio durante una
  transición por shader o con máscara; antes se congelaba hasta que acababa.
- Los degradados y los `clip-path` de un SVG bajo `<g transform>` o un
  `viewBox` desplazado siguen la transformación, y el trazo de una figura
  cerrada va centrado, como en SVG.
- Un grupo o un SVG que se funde se compone como una sola capa: lo que una
  pieza tapa sigue tapado.
- La sombra de un texto o un grupo se dibuja una vez, desde la silueta de sus
  miembros, en lugar de una por glifo.
- `echo` en reposo no apila sus copias sobre el objeto.
- `quantity(1600, "kg/m^3")` escribe «kg/m³» sin hueco antes de la barra.
- La vista previa ya no desborda los búferes de Vello con muchas curvas
  (decenas de fórmulas o tiza), que dejaban una mancha blanca.
- Con fuentes variables, `write()` y los trazos de texto ya no muestran los
  contornos que se solapan dentro de cada glifo.

== Al actualizar

- Los paquetes `.gaanim` pasan a la versión 5 del formato: Gaanim 0.9 no abre
  los grabados con 0.8 ni anteriores. Vuelve a grabarlos con
  `gaanim export <proyecto> --output charla.gaanim`.
- `line_spacing` vale 1.0 por defecto en lugar de 1.2: las líneas quedan a
  un tamaño de letra, más cerca de como se veían en 0.7. Pasa
  `line_spacing=1.2` para el espaciado de 0.8.
- Los trazos de los SVG importados van centrados; usa
  `.stroke_align("inside")` si un dibujo dependía del trazo hacia dentro.
- La sombra de un grupo es una sola, bajo todos sus miembros.
- Un emisor de partículas con `opacity()` se funde como una sola capa: las
  partículas solapadas ya no suman su brillo.
- Con el mismo `z_index`, un objeto de `scene.persist(...)` queda encima del
  contenido de los segmentos, también al terminar una transición por shader.
- `format` en `readout`, `variable` y las cotas sigue la especificación de
  Python: `"05.2f"` rellena con ceros y un formato no admitido (`d`, `s`…)
  lanza `ValueError`.

= 0.8.1

Publicada el 3 de octubre de 2026. Corrige lo que se vio al hacer los primeros
videos con 0.8.0: sombras de curvas sin relleno, cotas demasiado gruesas, un
`stagger` que cambiaba de orden y documentos Typst que se ensanchaban hasta la
página. Llegan `gaanim export --frame`, contadores con ancho reservado sin
ceros, trazos que escalan con su figura, updaters con velocidad o amplitud
animable, escritura con tiza, arcos de elipse, magnitudes con unidades,
reparto automático sin solapes y una guía de personajes por partes. Lee «Al actualizar» si usas
`scene.text.typst` con párrafos largos.

== Cambios

- `scene.geometry.double_arrow(..., body_width=…)` acepta el grosor del cuerpo,
  como `arrow`, para cotas finas. Para una cota completa con líneas de
  extensión, usa `scene.mechanics.dimension`. Lee
  #link("/referencia/geometria/")[Geometría].
- `gaanim export <escena> --output f.png --frame 11.4` escribe exactamente un
  PNG con lo que se ve en ese instante. Acepta varios instantes o marcadores
  separados por comas, que se escriben como `f_1.png`, `f_2.png`… Lee
  #link("/referencia/cli/")[CLI].
- `scene.viz.rolling_number(..., pad=" ")` deja vacías las posiciones que
  reserva `min_digits` en lugar de dibujar ceros: el contador ocupa siempre lo
  mismo y el prefijo no se desplaza.
- `drawable.scale_stroke_with_object()` o `stroke(..., scale_with_object=True)`
  hacen que el ancho del trazo siga la escala de la figura y de sus grupos; en
  un grupo o un SVG se aplica a todos sus trazos.
- `Updater.rotate(velocidad)` y las amplitudes y la frecuencia de
  `Updater.wiggle` aceptan un `Parameter`: animarlo acelera un giro o calma un
  temblor sin saltos, y un seek cae en el mismo fotograma que la reproducción.
  Lee #link("/guias/movimiento/")[Movimiento].
- `write(brush="chalk", seed=…)` y `drawable.chalk(seed, roughness)` dibujan
  como con tiza: un contorno que tiembla un poco y un grano que rompe la
  pintura, iguales para la misma semilla. Los paquetes `.gaanim` aún no
  graban tiza. Lee #link("/referencia/drawable/")[Drawable].
- `scene.geometry.ellipse_arc(cx, cy, rx, ry, inicio, barrido)` dibuja arcos de
  elipse, como la base de un cilindro en perspectiva.
- `quantity(valor, unidad)` escribe magnitudes con unidades como `siunitx`:
  `quantity(181, "L")` y `quantity(1, "m^3")` dejan el mismo espacio fino.
  Lee #link("/referencia/text/")[Texto].
- `scene.layout.scatter(items, region, avoid=[...], gap=..., seed=...)` reparte
  objetos en una región sin solaparse entre sí ni con `avoid`, igual para la
  misma semilla. Lee #link("/referencia/layout/")[Layout].
- La guía #link("/guias/personajes/")[Personajes con SVG por partes] anima un
  personaje dibujado en Inkscape con `part`, `with_pivot` y `look_at`.

== Al actualizar

- Un documento de `scene.text.typst(...)` sin `width=` ya no reparte un párrafo
  largo en líneas al ancho de una página A4: corta línea solo donde lo indica
  el texto. Pasa `width="16cm"` (o el ancho que quieras) para que se ajuste, o
  para columnas `1fr`.
- Un `stagger` con `total` o `easing` pero sin `origin` sigue el orden de la
  lista. Si querías el orden por distancia, pasa `origin="start"`.

== Correcciones

- `shadow()` sobre un camino abierto o sin relleno proyecta la sombra de su
  trazo; antes rellenaba la región entre el trazo y la cuerda que une sus
  extremos, también en un `group`. Los objetos con relleno conservan su sombra.
- `scene.text.typst(...)` ajusta la página a su contenido, así que un
  `#align(center)` centra dentro del contenido y `move_to` deja el documento
  centrado.
- `stagger(..., total=…, easing=…)` sin `origin` ni `grid` escalona en el orden
  de la lista, y `distribute` también.

= 0.8.0

Publicada el 2 de octubre de 2026. El público entra a la presentación: desde su
teléfono vota encuestas, responde cuestionarios con puntos y cuenta atrás, se
une a un equipo y aparece en la escena como el personaje que armó. La escena
muestra esos datos como tú decidas, y al terminar tienes los resultados en
hojas de cálculo. Llegan también partículas, _magic move_, carreras de barras,
seguimiento con inercia y sonidos anclados a animaciones. *El interlineado
cambia*: lee «Al actualizar».

== Público en vivo

- Los teléfonos se conectan a un relay que despliegas gratis en Cloudflare
  con `gaanim relay init` y configuras con `gaanim relay use <URL>`. Lee
  #link("/publico/")[Público en vivo].
- `scene.poll(...)` abre una encuesta y `scene.quiz(...)` un cuestionario con
  respuesta correcta, tiempo y puntos, con selección múltiple
  (`multiple=True`) o una imagen sobre la pregunta. `load_questions(...)` lee
  las preguntas de un Markdown o un CSV.
- La API da datos, no diseños: `votes`, `percent`, `remaining`, `revealed`, la
  clasificación (`scene.leaderboard()`), los equipos (`scene.teams(...)`) y
  cada jugador (`scene.audience()`) son valores en vivo que mueven lo que
  quieras. Lee #link("/referencia/publico/")[la referencia].
- `quiz.reveal()` revela la respuesta en los teléfonos cuando la presentación
  avanza; `quiz.revealed()` vale 0 hasta entonces y 1 después.
- `scene.stop(until=...)` avanza sola cuando el público cumple una condición.
- `scene.character(...)` y `scene.live_zone(audience, comportamiento)` dibujan
  a cada persona como su personaje; el comportamiento es una función de Python
  que Gaanim compila para que un `.gaanim` la ejecute sin Python. Lee
  #link("/publico/zonas-vivas/")[Zonas vivas].
- `scene.rehearsal(...)` juega la escena con un público inventado en la vista
  previa y en las exportaciones.
- Cada partida se guarda en `resultados/` como `jugadores.csv`,
  `respuestas.csv` y `preguntas.csv`; `scene.roster("Código de alumno")` pide
  un dato más al unirse. La vista del presentador tiene *Save now* y *Open
  folder*, y `gaanim relay results` los descarga después. Lee
  #link("/publico/presentar/#resultados")[Guardar los resultados].
- Ejemplos: `la_gran_trivia`, `quiz_orbit_demo`, `quiz_game_demo`,
  `audience_poll_demo` y `live_zones_demo`.

== Motion design

- `scene.fx.particles(...)` y `scene.fx.confetti(...)`: partículas
  deterministas.
- `magic_move(antes, despues)` y `Transition.magic_move`: los objetos con el
  mismo nombre viajan a su nuevo estado.
- `scene.viz.bar_race(...)`: una carrera de barras desde fotogramas clave.
- `Anim.settle(...)` rebota al final de un movimiento y
  `Drawable.follow(..., delay=...)` sigue con retraso.
- `Anim.sound(...)` ancla un sonido a una animación y `scene.media.sfx(...)`
  lo coloca en un instante.
- `Anim.keyframes(...)`: varias paradas en un solo clip, con un easing por
  tramo y trayectoria suave opcional; también en `Parameter.animate`.
- `Anim.throw(...)` lanza un objeto que rebota hasta reposar y
  `Anim.inertia(...)` lo desliza hasta el punto de `snap` más cercano.
- `Composition.speed_ramp(...)` y `Composition.time_remap(...)`: cámara lenta
  y aceleraciones dentro de una composición.
- `Drawable.modifiers`: zigzag, esquinas redondeadas, pucker y bloat, torsión,
  wiggle y offset no destructivos, con parámetros animables.

== Reproductor web

- Un enlace `?src=` empieza a reproducir antes de descargar el archivo entero:
  el reproductor lee el final del `.gaanim` y pide cada parte cuando hace
  falta, mientras el resto llega en segundo plano.
- El enlace puede apuntar a un momento (`#segmento=5`, `#pausa=12`,
  `#t=83.5`) y, con `#present`, ofrece el botón *Presentar*.
- El botón de enlace de la barra copia el enlace al archivo o al instante que
  se ve; el reproductor funciona dentro de un `<iframe>`. Lee
  #link("/guias/compartir/")[Compartir sin Python].
- El reproductor web reproduce el audio del paquete y recibe votos en vivo al
  presentar un paquete con encuestas (lee
  #link("/publico/presentar/#web")[En el reproductor web]). Al presentar, el
  aviso del Presenter View, `F11` y el panel inferior ponen la presentación en
  pantalla completa.
- La barra muestra al pasar el cursor una miniatura del momento con el nombre
  y el número del segmento; un clic en el nombre del segmento actual abre la
  lista de segmentos con buscador. Tiene control de volumen cuando la escena
  tiene audio.
- *Actualiza el relay*: la versión 13 del protocolo permite presentar desde
  el navegador.

== Ajustes

- `Drawable.hidden()` deja oculto un objeto creado a mitad de segmento hasta
  que una animación lo muestra.
- `dashed_line` acepta los mismos extremos que `line` y los sigue si se
  mueven.
- `Readout.move_to` y `Variable.move_to` aceptan `TextAnchor`; `spacing=` fija
  el espacio entre sus términos.
- Dentro de un layout, un readout reserva el ancho del valor más ancho que
  mostrará y alinea el número a la derecha, así que sus vecinos no se mueven
  mientras cuenta. `reserve=` añade un valor para conteos en vivo sin máximo
  conocido, como `votes`.
- `slides.brand(number_anchor=...)` pone el número de diapositiva aparte del
  pie.
- `gaanim check` dice el segmento, el instante y el texto de una caja que se
  sale de su contenedor.
- `gaanim --diff --height <PX>` captura a otra resolución.
- Las llaves (`geometry.brace` y `selection.animate.brace`) se dibujan como
  una llave tipográfica: brazos rectos, rizos del tamaño de su profundidad,
  una punta y un grosor que se afina hacia los extremos.

== Al actualizar

- `line_spacing` es la distancia entre líneas base en múltiplos del tamaño de
  letra, como `line-height` en CSS. Con 1.2, las líneas quedaban a 0.9 veces
  el tamaño y ahora a 1.2: los textos de varias líneas ocupan más alto.
- Un readout colocado con `move_to` queda centrado justo en ese punto.
- En una caja que se ajusta a su contenido, un hijo que crece mide su
  contenido aunque tenga `basis`.
- `dashed_line(x1=…, y1=…)` con palabras clave deja de funcionar: pasa las
  coordenadas por posición.
- Una llave es una figura rellena sin trazo: coloréala con `fill`. Un
  `stroke` dibuja su contorno, y un tema la pinta como la familia `glyph`, no
  como `line`.
- `line`, `arc`, `traced_path` y `tracking_line` nacen sin relleno, como
  `polyline`: ya no hace falta `no_fill()`. Un arco o un trazo seguido que
  dejaba ver un relleno blanco entre sus extremos ahora solo dibuja su trazo;
  dale `fill()` si lo querías.
- Reemplaza todos los archivos de la carpeta de Gaanim: el soporte de Python
  cambió su interfaz.

== Correcciones

- `Box.bounds()` mide el tamaño real de una caja dentro de un layout, y
  `surrounding_rect` se dibuja alrededor de cajas.
- Un texto que crece con `basis=0` se reparte en líneas en el espacio libre de
  su fila.
- Un readout dentro de un layout queda dentro de su celda, también cuando su
  número crece.
- Una caja dentro de otra dibuja su fondo aunque su único contenido sea un
  readout.
- `highlight` sobre una parte de un texto la agranda en su sitio; antes la
  desplazaba sobre sus vecinas.
- Una fila con `width="fill"` dentro de una columna que se ajusta a su
  contenido toma el ancho de la caja que la contiene, y una fila cuyo texto se
  reparte en líneas mide el alto de todas ellas.
- `Theme(fonts=...)` respeta la familia de `title` aunque también se defina
  `text`.
- Los fondos con shader y el postprocesado siguen moviéndose en una pausa, y
  las animaciones de entrada no se ven antes de tiempo al saltar entre
  segmentos.
- Las teclas de navegación entre pausas funcionan con el foco en un panel, y
  `P` fija la ventana encima.
- En el teléfono, el nombre del equipo se lee en blanco sobre su color; antes
  quedaba gris y casi no se veía.
- En teléfonos de menos de 360 px de ancho, la barra superior muestra el
  apodo y los puntos completos.
- Presenter View muestra en vivo lo que ve el público, con los votos y los
  jugadores de la sala; antes mostraba una miniatura con los del ensayo.

= 0.7.2

Publicada el 29 de septiembre de 2026. Una caja se puede recorrer, buscar,
animar en cascada y reordenar (`walk`, `find`, `cascade`, `stagger`, `swap`…), y
un solo movimiento del cursor puede escalar, teñir, girar o desvanecer cientos
de elementos con los nuevos falloffs. La reproducción ya no se vuelve más lenta
a medida que avanza una presentación larga, y recargar una escena que mide
cajas del layout es mucho más rápido. Las cajas que entran con `grow_*` ya no
se ven antes de su turno y el overlay *Layout* del editor no mezcla segmentos.
No hace falta cambiar tus escenas.

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

== Rendimiento

- Cada fotograma parte del inicio del segmento en que cae, en lugar de
  reconstruir la escena desde el principio: el coste de un fotograma ya no
  crece a lo largo de la presentación. En una presentación de 41 segmentos, la
  reproducción pasaba de 100 fps al principio a 55 fps al final y ahora se
  mantiene en torno a 100 fps de principio a fin, y exportarla tarda un 30 %
  menos. El resultado es idéntico fotograma a fotograma;
  `GAANIM_SEGMENT_CHECKPOINTS=0` vuelve al comportamiento anterior para
  comparar. Lee #link("/referencia/cli/")[CLI].
- Guardar el estado de la escena es unas cuatro veces más rápido. Lo hace al
  cargar y la primera vez que la reproducción entra en cada segmento, que en
  esa presentación cuesta un solo fotograma de unos 50 ms.
- `bounds()` sobre un objeto de una caja recién declarada mide solo su árbol de
  cajas en lugar de recompilar toda la escena: en la misma presentación, la
  parte de Python de una recarga bajó de 3,5 s a 1,4 s.
- Los fondos y bordes de las cajas ya no se redibujan en cada fotograma cuando
  no cambian.
- `GAANIM_FRAME_PROFILE=1` informa además de cuántos objetos están visibles.

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
