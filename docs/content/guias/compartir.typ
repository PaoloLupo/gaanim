#import "../../components/section.typ": docs-chapter

#show: docs-chapter.with(
  title: "Compartir sin Python",
  description: "Graba una escena o presentación en un solo archivo .gaanim que se reproduce, presenta y exporta sin Python",
  route: "/guias/compartir/",
)

En esta guía aprenderás a grabar una escena o una presentación en un solo
archivo `.gaanim` (un _paquete de reproducción_) y a reproducirlo, presentarlo
o convertirlo en vídeo en otro equipo que no tiene Python, ni tu proyecto, ni
tus recursos.

= Qué guarda un paquete

Un paquete no guarda tu script: guarda lo que la escena dibujó. Gaanim ejecuta
la escena igual que para exportar un vídeo y, en cada fotograma, registra lo
que el renderizador 2D compone (trazados, pinceles, degradados, imágenes,
efectos, máscaras, transiciones, cámara y posprocesado) en lugar de píxeles.
Junto a los fotogramas guarda la estructura de la escena: segmentos, notas del
orador, pausas con nombre, bucles ambientales, marcadores, escenas, el fondo y
el audio.

Por eso cualquier código que la escena ejecuta mientras se reproduce queda
_horneado_ en el paquete: updaters, animaciones personalizadas, funciones
reactivas, `always_redraw`, números aleatorios y cualquier otro callback de
Python o de Rust. Al reproducir no se ejecuta nada de tu código; solo se
componen los fotogramas grabados. Las animaciones Lottie y dotLottie también
se graban así: cada fotograma distinto que dibujan se guarda como escena
vectorial, así que el paquete no necesita el archivo Lottie ni sus imágenes.

= Grabar

La extensión `.gaanim` en `--output` graba un paquete:

```bash
gaanim export mi-charla --output mi-charla.gaanim
gaanim export mi-video --output mi-video.gaanim --fps 30
```

`--fps` elige cuántos fotogramas por segundo se graban (60 por defecto). Un
vídeo exportado desde el paquete tiene esa misma frecuencia, así que elige la
del vídeo que quieras obtener después. `--from`, `--to`, `--transparent` y
`--encoder` no se aplican a un paquete: siempre se graba la escena completa.

Grabar no rasteriza nada, así que suele ser rápido: una presentación de un
minuto tarda unos segundos. Una presentación larga con decenas de miles de
objetos tarda más, porque cada fotograma recorre la escena completa; una
presentación de 41 segmentos, 29 000 objetos y 4½ minutos se graba en unos 8
minutos y ocupa 18 MB. Los paquetes de los ejemplos de Gaanim ocupan entre 20 y
500 KB; una escena con mil instancias animadas, unos 11 MB.

= Reproducir y presentar

```bash
gaanim mi-charla.gaanim                          # reproducir
gaanim --present --monitor 1 mi-charla.gaanim    # presentar
gaanim mi-charla.gaanim --from resultados        # ensayar desde un segmento
```

Se abre el mismo editor, con la misma barra de reproducción, navegación entre
pausas y segmentos, bucles, Presenter View con notas y miniaturas, vista
general y atajos de teclado (ver
#link("/guias/presentaciones/")[Presentaciones]). Lo que no tiene sentido sin
el script no está: la recarga al guardar y la narración.

También puedes abrirlo desde el inicio de Gaanim (`gaanim` sin argumentos):
la tarjeta *Play .gaanim* (`Ctrl Shift O`) elige el archivo, y también
puedes arrastrarlo a la ventana. Los archivos que abres aparecen en
*Recent* junto a tus proyectos. En un equipo sin Python, el inicio se abre
igual y reproduce archivos `.gaanim`; para crear o abrir proyectos te indica
cómo instalarlo. Sin FFmpeg, el paquete se reproduce sin audio y se exporta
solo a secuencias PNG.

`gaanim` no carga Python para reproducir, presentar, exportar ni comprobar un
paquete: solo lo carga al ejecutar un script. Por eso basta con copiar el
paquete de Gaanim de la release, sin instalar Python.

La inspección (`I`) sigue funcionando: los fotogramas son vectoriales, así que
el zoom los redibuja nítidos.

= Abrir con doble clic

`gaanim register` asocia los archivos `.gaanim` con Gaanim para tu usuario,
sin permisos de administrador:

- el doble clic reproduce el archivo;
- el menú contextual ofrece *Present*;
- el explorador de archivos muestra su portada.

El botón *Associate .gaanim files* de la sección _Environment_ del inicio hace lo
mismo, y `gaanim unregister` lo deshace. La asociación apunta al `gaanim` desde
el que la creas: si mueves la carpeta de Gaanim, vuelve a registrarla.

- *Windows*: escribe en `HKCU\Software\Classes`. Si antes elegiste otra
  aplicación para los `.gaanim`, Windows pregunta una vez con cuál abrirlos.
  La portada la dibuja `gaanim_thumbnail_handler.dll`, que viene en el zip
  junto a `gaanim.exe`; debe quedarse en esa carpeta.
- *Linux*: instala en `~/.local/share` el tipo MIME `application/x-gaanim`, la
  entrada `gaanim.desktop`, un miniaturizador y los iconos. Las portadas se ven
  en Nautilus, Nemo, Caja y Thunar (con tumbler); Dolphin (KDE) todavía no.

== Portada

Al grabar, Gaanim guarda en el paquete una portada de 512 píxeles de ancho: el
fotograma que se ve en la primera pausa (`scene.stop()`), porque el primer
fotograma suele estar vacío, o, sin pausas, el fotograma del primer segmento
que más muestra. Para elegir otro, llama a `scene.thumbnail()` justo después del
plano que quieres, o pásale un instante de la línea de tiempo:

```python
>>>from gaanim import BLUE, Scene
>>>scene = Scene(frame=(16, 9))
>>>diagrama = scene.geometry.circle(2).stroke(BLUE, 0.1)
scene.play(diagrama.animate.create())
scene.thumbnail()        # la portada es este plano
scene.thumbnail(0.5)     # o el fotograma de los 0,5 s
```

La portada se dibuja con la GPU, como una exportación; sin GPU el paquete se
graba igual, sin portada, y el explorador muestra el icono de los `.gaanim`:
una hoja con un video animado vectorial y el logo de Gaanim. Para extraerla, sin
GPU:

```bash
gaanim thumbnail mi-charla.gaanim portada.png       # 512 px
gaanim thumbnail mi-charla.gaanim portada.png 256   # lado mayor de 256 px
```

= En el navegador y el teléfono

El reproductor web abre un `.gaanim` sin instalar nada, en la PC o en el
teléfono:
#link("https://paololupo.github.io/gaanim/reproductor/")[paololupo.github.io/gaanim/reproductor].
Toca o arrastra el archivo; no sale de tu equipo. Es el mismo reproductor de
escritorio compilado para la web, con la misma barra de reproducción.

Es experimental: necesita un navegador con WebGPU (Chrome o Edge recientes,
Safari 26, Firefox en Windows) y descarga unos 25 MB la primera vez. En las
miniaturas del Presenter View web no se ven los postprocesos ni los fondos
animados por shader.

El audio del paquete suena en la página de la presentación (el Presenter View
queda en silencio). El navegador solo deja empezar el sonido tras un clic o
una tecla en la página: si el paquete tiene audio, un aviso lo recuerda. Un
audio con `speed` distinto de 1 cambia de tono, cosa que en el escritorio no
pasa.

== La barra de reproducción

- *Ubicar un momento*: al pasar el cursor por la línea de tiempo, una tarjeta
  muestra una miniatura de ese momento, el nombre del segmento, su número
  (`12 / 61`), la pausa y el tiempo. Las miniaturas se preparan la primera vez
  que pasas el cursor y son las mismas del Presenter View.
- *Buscar un segmento*: haz clic en el nombre del segmento actual, junto al
  tiempo. Se abre la lista de todos, con su número, sus pausas y cuándo
  empiezan; escribe parte de un nombre o un número y pulsa `Enter`, o haz clic
  en uno, para ir a él. Las flechas `↑` `↓` y `Tab` (`Shift+Tab` hacia atrás)
  recorren los segmentos de la lista, `Enter` va al marcado y `Esc` la cierra.
  La barra sigue visible mientras la lista, o cualquier menú suyo, esté
  abierta.
- *Volumen*: el altavoz silencia y el deslizador regula el volumen; solo
  aparecen si la escena tiene audio. El volumen se recuerda en la próxima
  visita. En barras estrechas están en el menú *More controls*.

Lo mismo vale para la barra del editor de escritorio.

== Compartir un enlace <compartir-un-enlace>

Para compartir un enlace, publica el `.gaanim` en un sitio que permita
abrirlo desde otra página y añade su dirección con `?src=`:

```
https://paololupo.github.io/gaanim/reproductor/?src=https://tu-sitio.com/mi-charla.gaanim
```

El archivo va directamente del sitio donde lo publicaste al navegador de quien
abre el enlace: no pasa por ningún servidor de Gaanim, y la página no registra
qué archivos se abren. Un `.gaanim` son solo datos, así que abrirlo no ejecuta
código de su autor.

Si el sitio lo permite, el reproductor no espera a la descarga completa:
primero descarga el final del archivo, donde están el índice y las tablas, y
empieza a reproducir; los fotogramas llegan a medida que hacen falta y el resto
se descarga en segundo plano (la línea verde de arriba muestra cuánto). Si
saltas a una parte que todavía no llegó, verás *Cargando…* un momento. Si el
sitio no admite descargas por partes, se descarga el archivo entero antes de
empezar, con una barra de progreso.

#table(
  columns: (auto, 1fr),
  inset: 7pt,
  [*Alojamiento*], [*¿Funciona?*],
  [GitHub Pages], [Sí, también por partes.],
  [`raw.githubusercontent.com` (un archivo de un repositorio de GitHub)], [Sí, también por partes. GitHub no admite archivos de más de 100 MB en un repositorio.],
  [Tu propio sitio o un CDN], [Sí, si envía la cabecera `Access-Control-Allow-Origin` (CORS); por partes si además admite `Range`.],
  [Descargas de GitHub Releases], [No: redirigen a otra dirección sin CORS.],
  [Google Drive, Dropbox, OneDrive], [Por lo general no: sus enlaces de descarga pasan por páginas intermedias o no envían CORS.],
)

Cuando no se puede abrir, el reproductor explica el motivo: que el sitio no
permite abrirlo desde otra página (CORS), que la dirección no existe, que no
hay conexión, que no es un `.gaanim` o está dañado, o que se grabó con un
formato más nuevo que el del reproductor (recarga la página para actualizarlo).

Para publicarlo en GitHub Pages:

+ Crea un repositorio (o usa uno existente) y sube el `.gaanim`, por ejemplo
  a `charlas/mi-charla.gaanim`.
+ En *Settings → Pages*, elige publicar desde la rama principal.
+ Al cabo de un minuto, el archivo está en
  `https://<usuario>.github.io/<repositorio>/charlas/mi-charla.gaanim`.
+ El enlace es
  `https://paololupo.github.io/gaanim/reproductor/?src=https://<usuario>.github.io/<repositorio>/charlas/mi-charla.gaanim`.

=== Enlazar un momento <enlazar-un-momento>

Como en los sitios de vídeo, lo que va después de `#` elige dónde empieza:

#table(
  columns: (auto, 1fr),
  inset: 7pt,
  [*Fragmento*], [*Empieza*],
  [`#segmento=5`], [Al principio del quinto segmento.],
  [`#segmento=resultados`], [Al principio del segmento que se llama así (sin distinguir mayúsculas). Codifica los espacios como `%20`.],
  [`#segmento=resultados&pausa=2`], [En la segunda pausa de ese segmento, detenido ahí.],
  [`#pausa=12`], [En la duodécima pausa de toda la presentación.],
  [`#t=83.5` o `#t=1:23.5`], [A los 83,5 segundos.],
  [`#present`], [Muestra el botón *Present*, que abre el Presenter View; se combina con los demás: `#segmento=5&present`.],
)

El botón de enlace de la barra (dos eslabones) copia el enlace al archivo
abierto. *Copy link to this moment* añade el fragmento de lo que se ve:
la pausa en la que está detenida la presentación o, si no, el segundo actual.
Solo los archivos abiertos con `?src=` tienen enlace; uno que abriste desde tu
equipo hay que publicarlo antes.

El botón *Present* existe porque el navegador solo deja abrir otra ventana
tras un clic: el enlace no puede abrir el Presenter View por sí solo.

== Insertar en un blog o sitio web <insertar>

Una animación corta puede ir en una entrada de blog como una figura más: se
ve nítida a cualquier tamaño y suele pesar menos que el mismo vídeo. Publica
el `.gaanim` como para #link(<compartir-un-enlace>)[compartir un enlace] y
pega en la entrada:

```html
<iframe src="https://paololupo.github.io/gaanim/reproductor/insertar/?src=https://tu-blog.com/animaciones/circulo.gaanim&amp;loop&amp;controls=0"
  title="Animación: círculo" loading="lazy" allow="fullscreen; clipboard-write"
  style="width: 100%; aspect-ratio: 16 / 9; border: 0"></iframe>
```

`reproductor/insertar/` es un reproductor ligero hecho para esto: solo dibuja
el archivo, sin la barra del editor ni el Presenter View, y su primera visita
descarga unos 3 MB en lugar de los 24 del reproductor completo. La animación
empieza cuando al menos una cuarta parte del marco está a la vista y se pausa
cuando sale de ella: quien lee la entrada la ve desde el principio al llegar a
ella, no mientras lee lo anterior. Si la pausa a mano, sigue pausada. Una
pausa de la escena (`scene.stop()`) la detiene hasta un clic, como en una
presentación; si la pausa tiene un bucle (`loop=`), lo repite mientras espera,
y el clic sigue desde su final. Estas opciones cambian cómo se reproduce (en HTML, el `&` que
las separa se escribe `&amp;`):

#table(
  columns: (auto, 1fr),
  inset: 7pt,
  [*Opción*], [*Efecto*],
  [`loop`], [Repite la animación entera, como un GIF.],
  [`controls=0`], [Oculta la barra. Un clic o un toque en la animación la pausa o la reanuda, y al terminar la repite desde el principio.],
  [`autoplay=0`], [No empieza sola: un botón en el centro espera un clic.],
  [`preload=auto`], [Descarga el archivo entero aunque no se reproduzca. Sin esta opción, un archivo de más de 2 MB en un sitio que admite descargas por partes baja al principio solo lo que necesita para abrirse, y el resto mientras se reproduce: quien pasa de largo no lo descarga entero.],
  [`full=click`], [Si la escena necesita el reproductor completo (ver abajo), no lo abre solo: muestra un aviso y lo abre al hacer clic.],
  [`#t=2`], [Empieza a los dos segundos (también `#t=1:05`).],
)

Sin `controls=0`, una barra fina aparece al pasar el cursor por la animación y
mientras está pausada: reproducir o pausar, la posición, con una marca en cada
pausa de la escena, el tiempo, el bucle, el sonido si la escena lo tiene y la
pantalla completa. Sin `loop`, la
animación se detiene en su último fotograma con un botón para repetirla.

El reproductor ligero deja fuera el posprocesado, los fondos de shader, las
transiciones de shader, los efectos de shader sobre objetos y el cristal, que
se dibujan en pasadas propias de la GPU. Cuando la escena usa alguno, abre el
reproductor completo con las mismas opciones, así que la animación siempre se
ve como la grabaste; solo descarga más. Con `full=click` espera a un clic
antes de abrirlo, para que una página con varias animaciones no descargue el
reproductor completo en cada una; la página que lo contiene recibe además el
mensaje `gaanim:status` de tipo `full`, con la dirección del reproductor
completo en `url`, por si prefiere mostrar su propio botón (ver
#link(<controlar-desde-la-pagina>)[Controlarlo desde la página]).

`aspect-ratio` da al marco la proporción de la escena: `16 / 9` para el
`frame=(16, 9)` por defecto, `1 / 1` para una cuadrada. Así el marco ocupa el
ancho de la columna sin franjas. Si el marco no tiene la proporción de la
escena, el reproductor oscurece las franjas y marca el borde de la escena con
una línea fina, como el editor; en pantalla completa, las franjas son negras. `loading="lazy"` hace que el navegador no lo
cargue hasta que la entrada se acerca a él, y `allow` deja funcionar la
pantalla completa.

El #link("https://paololupo.github.io/gaanim/playground/")[playground] lo
prepara por ti: tras ejecutar la escena, *Insertar* descarga el `.gaanim` con
el nombre que elijas, compone el código con la dirección donde lo publicarás y
la proporción de la escena, y muestra cómo se verá con las opciones que
marques.

Antes de publicarlo, `gaanim pack` puede dejarlo más ligero sin Python:

```bash
gaanim pack circulo.gaanim animaciones/circulo.gaanim --web
```

Con `--web` guarda los trazados en una rejilla tan fina que, en un vídeo 4K,
ningún punto se mueve más de 1/64 de píxel en cada eje (solo cambia el
suavizado de algunos píxeles de borde), y los vértices de las mallas y líneas
3D en una rejilla de 2#super[−17] de su tamaño, que tampoco mueve ninguno más
de 1/64 de píxel aunque la malla llene un vídeo 4K. Deja fuera lo que el
reproductor no usa: los resúmenes de `gaanim check` y los subfotogramas del
desenfoque de movimiento. En los ejemplos de esta documentación pesa, en la
mediana, un 41 % menos, y hasta un 97 % menos en escenas con muchas formas en
movimiento. Conserva el original para exportar vídeo o comprobarlo.

Quien lee la entrada necesita un navegador con WebGPU, igual que para el
reproductor completo; sin él, el marco explica qué falta. Las demás
animaciones de la página, y las de visitas siguientes, toman el reproductor
de la caché del navegador.

=== Sin sitio donde publicarla: una página HTML <pagina-html>

Si no tienes dónde publicar el `.gaanim`, o el sitio no deja configurar CORS,
el playground descarga una página HTML que lleva dentro la animación y el
reproductor ligero: en *Insertar*, el botón bajo la vista previa, o la
entrada `.html` del menú *Descargar*. Se abre con doble clic, sin conexión ni
servidor, y se comparte como cualquier archivo; subida a un sitio, también
puede ir en un `<iframe>`, y responde a los mismos mensajes que el
reproductor ligero (ver
#link(<controlar-desde-la-pagina>)[Controlarlo desde la página]). Sigue las
opciones marcadas en *Insertar*: bucle, inicio al verse y barra de controles.

Pesa unos 4 MB más que la animación: el reproductor va dentro, y la
animación, escrita en base64, ocupa un tercio más que su `.gaanim`. El
reproductor completo no cabe, así que una escena con posprocesado, fondos,
transiciones o efectos de shader, o cristal, se ve sin ellos; el playground
avisa al descargarla. La página guarda el reproductor del día en que se
descargó, sin actualizarse con el sitio, y quien la abre necesita un
navegador con WebGPU.

=== Con el elemento `<gaanim-player>`

En lugar del `<iframe>`, puedes cargar el script del reproductor una vez por
página y escribir un elemento por animación:

```html
<script type="module" src="https://paololupo.github.io/gaanim/reproductor/insertar/gaanim-player.js"></script>

<gaanim-player src="animaciones/circulo.gaanim" loop controls="0"
  label="Animación: círculo"></gaanim-player>
```

El elemento crea el marco por ti. Ocupa el ancho de la columna, toma la
proporción de la escena en cuanto la abre (16:9 hasta entonces) y no carga
hasta que la página se acerca a él. `src` puede ser relativa a la página.
Acepta las opciones de la tabla como atributos (`loop`, `controls="0"`,
`autoplay="0"`, `preload="auto"`, `full="click"`), además de:

- `start`: el segundo en que empieza, como `#t=`.
- `label`: lo que anuncia un lector de pantalla.
- `ratio`: la proporción desde el principio, como `ratio="1 / 1"`. Un
  `aspect-ratio` en el estilo de la página tiene prioridad sobre ambas.

Desde JavaScript, el elemento se maneja como un `<video>`:

- Métodos: `play()`, `pause()` y `seek(segundos)`.
- Propiedades: `currentTime` (al asignarla, salta a ese instante), `duration`,
  `paused`, `ended`, `loop` y `muted`.
- Eventos: `opened`, `play`, `pause`, `timeupdate`, `stop` (una pausa de la
  escena), `ended` y `error`. Cada uno lleva en `detail` el mensaje del
  reproductor (ver abajo).

Los métodos actúan una vez abierta la animación, es decir, después del evento
`opened`.

```js
const animacion = document.querySelector("gaanim-player");
animacion.addEventListener("ended", () => console.log("Terminó"));
document.querySelector("#otra-vez").addEventListener("click", () => {
  animacion.seek(0);
  animacion.play();
});
```

=== Controlarlo desde la página <controlar-desde-la-pagina>

Con un `<iframe>`, la página que lo contiene se comunica con el reproductor
mediante `postMessage`, sea del sitio que sea.

*Lo que avisa el reproductor.* Envía mensajes
`{ type: "gaanim:status", kind, time, duration, playing, ended, muted }`.
Los cinco últimos campos llegan una vez abierta la animación. Cuando la
animación pasa al reproductor completo, sus mensajes traen solo `time`,
`playing` y `muted`, y no avisan de `stop` ni de `ended`.

#table(
  columns: (auto, 1fr),
  inset: 7pt,
  [*`kind`*], [*Cuándo*],
  [`ready`], [El reproductor ya puede abrir archivos.],
  [`opened`], [Abrió la animación. Incluye su tamaño en píxeles (`width`, `height`), `fps` y los instantes de sus pausas (`stops`).],
  [`redirect`], [La animación necesita el reproductor completo, que ocupa el marco a continuación, con los mismos datos de tamaño; con `full=click`, el aviso es `full`.],
  [`play`, `pause`], [Empezó o se detuvo.],
  [`timeupdate`], [Cuatro veces por segundo mientras se reproduce, y tras cada salto.],
  [`stop`], [Llegó a una pausa de la escena: `stop` es su instante, y `loop` el bucle que repite mientras espera, si lo tiene.],
  [`ended`], [Llegó al final, sin `loop`.],
  [`error`], [No pudo abrir o reproducir la animación; `message` dice por qué.],
)

*Las órdenes que acepta.* Recibe `{ type: "gaanim:command", command }`:

- `"play"`, `"pause"` y `"toggle"`, para reproducir, pausar o alternar.
- `"seek"`, con `time` en segundos.
- `"loop"` y `"mute"`, con `value` igual a `true` o `false`.

Por ejemplo, para dar al marco la proporción de la escena y saltar a los dos
segundos:

```js
const marco = document.querySelector("iframe");
window.addEventListener("message", (event) => {
  if (event.source !== marco.contentWindow || event.data?.type !== "gaanim:status") return;
  if (event.data.kind === "opened") {
    marco.style.aspectRatio = `${event.data.width} / ${event.data.height}`;
    marco.contentWindow.postMessage(
      { type: "gaanim:command", command: "seek", time: 2 },
      "https://paololupo.github.io",
    );
  }
});
```

== Controles táctiles

En pantallas táctiles, los controles funcionan como en un reproductor de vídeo
del móvil:

- Toca la escena para mostrar u ocultar los controles; se ocultan solos tras
  unos segundos. Mientras se ven, los botones grandes del centro van a la pausa
  anterior, reproducen o pausan, y van a la pausa siguiente.
- Toca dos veces el lado derecho o el izquierdo para ir a la pausa siguiente o
  anterior (sin pausas, avanza o retrocede 10 segundos). Cada toque rápido más
  sigue avanzando.
- Desliza a la izquierda o a la derecha para ir a la pausa siguiente o
  anterior.
- Arrastra la barra para moverte por la presentación.

== Presentar

Pulsa el botón de presentar de la barra: se abre el Presenter View en otra
ventana, el mismo que en el escritorio, con la diapositiva actual y la
siguiente, las notas y el cronómetro. Lleva la página de la presentación al
proyector y ponla en pantalla completa: el aviso que aparece tiene el botón
*Pantalla completa*, y también sirven `F11` y el botón del panel inferior.
El navegador solo concede la pantalla completa a un clic o una tecla en esa
página, y el clic que abrió el Presenter View ya se usó, por eso hace falta
uno más. Deja el Presenter View en tu pantalla. Las teclas, los clics y el
panel inferior mueven las dos ventanas a la vez, desde cualquiera de ellas;
si cierras el Presenter View, `P` lo vuelve a abrir. Si el navegador bloquea
la ventana, permite las ventanas emergentes de la página y pulsa `P`.

Un paquete con encuestas recibe votos en vivo también en la web: ver
#link("/publico/presentar/#paquetes")[Presentar con público].

= Exportar el paquete a vídeo

```bash
gaanim export mi-charla.gaanim --output mi-charla.mp4
gaanim export mi-charla.gaanim --output clip.webm --from intro --to cierre
gaanim export mi-charla.gaanim --output vertical.mp4 --width 1080 --height 1920 --fit cover
```

Admite los mismos formatos y opciones que exportar un script
(ver #link("/referencia/cli/")[`gaanim export`]), incluido
`--from`/`--to` con nombres de marcadores y el audio de la escena. La
frecuencia del vídeo es siempre la del paquete; `--quality` elige la
compresión. El botón *Export* del editor hace lo mismo con el paquete
abierto.

= Desde el editor

Con un script o proyecto abierto, el botón *Export* ofrece el formato
*Gaanim*: graba la escena en un `.gaanim` sin salir del editor. La calidad
elige la frecuencia (Borrador, 30 fps; Estándar y Producción, 60 fps), salvo
que elijas otra en *Fotogramas por segundo*; la resolución no se aplica,
porque el paquete es vectorial.

= Comparar capturas

`gaanim --diff --example mi-charla.gaanim` captura el fotograma de cada pausa
del paquete y lo compara con la versión aprobada, sin Python (ver
#link("/guias/capturas-y-comparacion/")[Capturas y comparación visual]). Las
capturas son idénticas a las que `--capture-stops` obtiene del script.

= Reproducibilidad

Un paquete reproduce la escena tal como la exportación la dibuja, fotograma a
fotograma:

- *Mismos fotogramas que un vídeo.* Los fotogramas se graban en la misma
  rejilla de tiempos que usa la exportación, con los mismos pasos. Un vídeo
  exportado desde el paquete es idéntico, píxel a píxel, al que exporta el
  script con la misma frecuencia y el mismo tamaño.
- *Callbacks con estado.* Algunos updaters y callbacks acumulan estado según
  los instantes que visita la línea de tiempo. La rejilla se graba en un mundo
  que visita exactamente los instantes que visita una exportación, así que su
  estado evoluciona igual. Los instantes que caen entre fotogramas de la
  rejilla (el final, los límites de segmento, las pausas, los finales de los
  bucles ambientales y los marcadores) también se graban, para que una
  presentación en pausa muestre exactamente el instante de la pausa. Si la
  escena tiene updaters, trazos acumulados, ecos, squash o animaciones
  personalizadas, cuyo estado depende de los instantes visitados, esos
  instantes se graban aparte,
  en un segundo mundo que sigue la misma rejilla, sin alterar los demás
  fotogramas; si no, se graban en el mismo mundo y la grabación tarda la
  mitad.
- *Cualquier resolución.* Los fotogramas son vectoriales y guardan qué
  márgenes dependen de la resolución de salida, así que exportar el paquete a
  cualquier tamaño da los mismos píxeles que exportar el script a ese tamaño.
- *Motion blur.* Cada fotograma de una escena con
  `scene.canvas.motion_blur(...)` guarda también los subfotogramas que la
  exportación promedia, y el vídeo del paquete los promedia igual. La
  reproducción en el editor muestra el fotograma sin desenfoque, como la
  vista previa del script.
- *Integridad.* Cada archivo interno lleva su hash BLAKE3 en el manifiesto, y
  cada fotograma, un resumen de lo que compone. Un paquete dañado o
  modificado se rechaza en lugar de mostrarse mal. `gaanim check
  mi-charla.gaanim` vuelve a componer todos los fotogramas y los compara con
  sus resúmenes, sin Python:

  ```text
    ▸ check     mi-charla.gaanim · mi-charla
                Frames:     262 at 60 fps · 4.35 seconds · 1920×1080
                Structure:  5 segments · 8 stops · 0 markers · 0 audio tracks
    ✓ pass      every frame composes as it was recorded
  ```

Entre dos fotogramas grabados, el reproductor mantiene el último. A velocidad
normal eso equivale al vídeo; a cámara lenta verás los fotogramas de la
rejilla repetidos, porque no hay nada que interpolar sin ejecutar la escena.

= Límites

La grabación se niega, con un mensaje, cuando la escena contiene clips de
vídeo (`scene.media.video`), que el paquete aún no puede reproducir
exactamente. En ese caso exporta un vídeo. El contenido 3D sí se graba: el
paquete guarda sus mallas, materiales y luces, y el reproductor las proyecta
en cada fotograma como la vista previa.

Los paquetes son de un solo uso: no se pueden editar ni volver a convertir en
script. Guarda el proyecto original para seguir trabajando.

= Dentro del archivo

Un `.gaanim` es un ZIP con estas entradas:

#table(
  columns: (auto, 1fr),
  inset: 7pt,
  [*Entrada*], [*Contenido*],
  [`manifest.json`], [Formato y versión, generador, frecuencia, duración, tamaño, bloques de fotogramas con los segmentos que usa cada uno, y el hash BLAKE3 de cada entrada.],
  [`scene.bin`], [Título, fondo, color de borrado, posprocesados, segmentos con notas y pausas, marcadores, escenas y pistas de audio.],
  [`segments/NNNNNN.bin`], [Trazados, imágenes, recetas de fragmentos, cadenas, mallas y líneas 3D, y los shaders de las transiciones y de los efectos por objeto, cada uno guardado una sola vez. Un segmento se cierra al llegar a unos 2 MB y va justo antes del bloque de fotogramas que lo completó.],
  [`frames/NNNNNN.bin`], [Bloques de 60 fotogramas; cada bloque empieza con un fotograma completo y el resto guarda solo lo que cambió.],
  [`scenes/NNNNNN.bin`], [Cada fotograma distinto de una animación Lottie, como escena vectorial de Vello.],
  [`index.bin`, `digests.bin`], [Instante y resumen BLAKE3 de cada fotograma. Un fotograma de la rejilla de la exportación ocupa un byte en el índice.],
  [`media/*`], [Archivos de audio, direccionados por contenido.],
  [`thumbnail.png`], [Portada opcional de 512 píxeles de ancho, que el explorador de archivos lee sin decodificar el resto.],
)

Los números se guardan sin pérdida: los reales conservan todos sus bits, de
modo que el paquete compone exactamente lo que compuso la escena.

Salvo `manifest.json`, `media/*` y `thumbnail.png`, cada entrada guarda sus datos comprimidos
con Zstandard. En una presentación larga ocupa menos de la mitad que con la
compresión propia del ZIP; en un paquete pequeño, alrededor de un 10 % menos.

Un reproductor abre el paquete con el manifiesto, `scene.bin`, el primer
bloque y los segmentos que usa, y descarga los demás bloques con sus
segmentos a medida que los reproduce. Así, una presentación larga empieza a
verse sin esperar a descargar los trazados de todas sus diapositivas.

Este es el formato 10. Guarda las mallas y líneas 3D que cambian de un
fotograma a otro (una superficie que se dobla, una curva que se mueve) como el
cambio de sus vértices respecto de los fotogramas anteriores de su segmento,
sin perder un bit: en `gallery/catenoid_helicoid` el paquete pasa de 62 a
30 MB, y a 12 MB con `--web`. Un morph entre mallas 3D (`transform_to`) guarda
sus dos rejillas una vez y cada fotograma como cuánto avanza entre ellas: el
paquete de `3d/morph_3d` ocupa 1,9 MB, donde guardar el cambio de cada
fotograma ocupaba 76. Las copias de una malla (`points_3d`, `instances`) se
guardan una vez por conjunto, y cada fotograma solo cuánto han crecido.
Las imágenes que rellenan objetos en el mundo 3D van a la tabla de imágenes,
una vez cada una, y la sombra y el desenfoque de cada objeto 3D, con su
fotograma. Gaanim sigue leyendo los formatos 5 a 9 (versiones 0.9 a 0.16 y
las de desarrollo de la 0.17), así que los paquetes ya publicados siguen abriéndose. Los grabados con
versiones anteriores a la 0.9 ya no se abren: vuelve a grabarlos desde su
script.
