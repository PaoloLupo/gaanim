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
componen los fotogramas grabados.

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

Grabar es rápido porque no se rasteriza nada: una presentación de un minuto
tarda unos segundos. Los paquetes de los ejemplos de Gaanim ocupan entre 20 y
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

`gaanim` reconoce el paquete y lo abre con `gaanim-play`, un ejecutable que se
instala junto a `gaanim` y no enlaza Python. Si en el equipo solo copias
`gaanim-play`, también puedes llamarlo directamente con las mismas opciones:

```bash
gaanim-play --present mi-charla.gaanim
```

La inspección (`I`) sigue funcionando: los fotogramas son vectoriales, así que
el zoom los redibuja nítidos.

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
  bucles ambientales y los marcadores) se graban aparte, en un segundo mundo
  que sigue la misma rejilla, para que una presentación en pausa muestre
  exactamente el instante de la pausa sin alterar los demás fotogramas.
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
  modificado se rechaza en lugar de mostrarse mal.

Entre dos fotogramas grabados, el reproductor mantiene el último. A velocidad
normal eso equivale al vídeo; a cámara lenta verás los fotogramas de la
rejilla repetidos, porque no hay nada que interpolar sin ejecutar la escena.

= Límites

La grabación se niega, con un mensaje, cuando la escena contiene algo que el
paquete aún no puede reproducir exactamente. En esos casos exporta un vídeo:

- contenido 3D (mallas, superficies, modelos glTF y ejes 3D);
- animaciones Lottie;
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
  [`index.bin`, `digests.bin`], [Instante y resumen BLAKE3 de cada fotograma.],
  [`media/*`], [Archivos de audio, direccionados por contenido.],
)

Los números se guardan sin pérdida: los reales conservan todos sus bits, de
modo que el paquete compone exactamente lo que compuso la escena.
