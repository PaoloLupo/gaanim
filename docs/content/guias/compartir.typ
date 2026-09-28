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
la tarjeta *Reproducir .gaanim* (`Ctrl Shift O`) elige el archivo, y también
puedes arrastrarlo a la ventana. Los archivos que abres aparecen en
*Recientes* junto a tus proyectos. En un equipo sin Python, el inicio se abre
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
- el menú contextual ofrece *Presentar*;
- el explorador de archivos muestra su portada.

El botón *Asociar archivos .gaanim* de la sección _Entorno_ del inicio hace lo
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

Para compartir un enlace, publica el `.gaanim` en un sitio que permita
abrirlo desde otra página (por ejemplo GitHub Pages o tu propia web) y añade su
dirección con `?src=`:

```
https://paololupo.github.io/gaanim/reproductor/?src=https://tu-sitio.com/mi-charla.gaanim
```

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

Para presentar, pulsa el botón de presentar de la barra: se abre el
Presenter View en otra ventana, el mismo que en el escritorio, con la
diapositiva actual y la siguiente, las notas y el cronómetro. Lleva la página
de la presentación al proyector y ponla en pantalla completa (`F11` o el
botón de la barra); deja el Presenter View en tu pantalla. Las teclas, los clics y el dock mueven las
dos ventanas a la vez, desde cualquiera de ellas; si cierras el Presenter
View, `P` lo vuelve a abrir. Si el navegador bloquea la ventana, permite las
ventanas emergentes de la página y pulsa `P`.

Es experimental: necesita un navegador con WebGPU (Chrome o Edge recientes,
Safari 26, Firefox en Windows), descarga unos 25 MB la primera vez y todavía
no reproduce el audio. En las miniaturas del Presenter View web no se ven los
postprocesos ni los fondos animados por shader.

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
compresión. El botón Exportar del editor hace lo mismo con el paquete
abierto.

= Desde el editor

Con un script o proyecto abierto, el botón Exportar ofrece el formato
*Gaanim*: graba la escena en un `.gaanim` sin salir del editor. La calidad
elige la frecuencia (Borrador, 30 fps; Estándar y Producción, 60 fps); la
resolución no se aplica, porque el paquete es vectorial.

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

La grabación se niega, con un mensaje, cuando la escena contiene algo que el
paquete aún no puede reproducir exactamente. En esos casos exporta un vídeo:

- contenido 3D (mallas, superficies, modelos glTF y ejes 3D);
- clips de vídeo (`scene.media.video`).

Los paquetes son de un solo uso: no se pueden editar ni volver a convertir en
script. Guarda el proyecto original para seguir trabajando.

= Dentro del archivo

Un `.gaanim` es un ZIP con estas entradas:

#table(
  columns: (auto, 1fr),
  inset: 7pt,
  [*Entrada*], [*Contenido*],
  [`manifest.json`], [Formato y versión, generador, frecuencia, duración, tamaño, bloques de fotogramas y el hash BLAKE3 de cada entrada.],
  [`scene.bin`], [Título, fondo, color de borrado, posprocesados, segmentos con notas y pausas, marcadores, escenas y pistas de audio.],
  [`tables/*.bin`], [Trazados, imágenes, recetas de fragmentos y cadenas, cada uno guardado una sola vez.],
  [`frames/NNNNNN.bin`], [Bloques de 60 fotogramas; cada bloque empieza con un fotograma completo y el resto guarda solo lo que cambió.],
  [`scenes/NNNNNN.bin`], [Cada fotograma distinto de una animación Lottie, como escena vectorial de Vello.],
  [`index.bin`, `digests.bin`], [Instante y resumen BLAKE3 de cada fotograma.],
  [`media/*`], [Archivos de audio, direccionados por contenido.],
  [`thumbnail.png`], [Portada opcional de 512 píxeles de ancho, que el explorador de archivos lee sin decodificar el resto.],
)

Los números se guardan sin pérdida: los reales conservan todos sus bits, de
modo que el paquete compone exactamente lo que compuso la escena.

Salvo `manifest.json`, `media/*` y `thumbnail.png`, cada entrada guarda sus datos comprimidos
con Zstandard. En una presentación larga ocupa menos de la mitad que con la
compresión propia del ZIP; en un paquete pequeño, alrededor de un 10 % menos.
Este es el formato 2, que escribe y lee Gaanim 0.6.1 (y el reproductor web
actual). Los paquetes grabados con 0.6.0 usan el formato 1 y ya no se abren:
vuelve a grabarlos desde su script.
