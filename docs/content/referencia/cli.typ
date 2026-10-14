#import "../../components/section.typ": docs-chapter

#show: docs-chapter.with(
  title: "Línea de comandos",
  description: "El comando gaanim: previsualizar, crear proyectos, validar, exportar, presentar y comparar capturas",
  route: "/referencia/cli/",
  nav: "CLI",
)

= Línea de comandos

El ejecutable `gaanim` abre el editor, crea proyectos y exporta escenas. Todos
los comandos que reciben `<SCRIPT_O_PROYECTO>` aceptan la ruta de un script
Python o la carpeta de un proyecto: en una carpeta se ejecuta el `entry` de su
#link("/referencia/gaanim-toml/")[`gaanim.toml`].

```bash
gaanim                                   # abre el Inicio
gaanim mi-video                          # previsualiza con hot reload
gaanim init video mi-video               # crea un proyecto
gaanim check mi-video                    # valida sin abrir ventana
gaanim inspect mi-charla --json          # la línea de tiempo y sus líneas
gaanim export mi-video --output exports/video.mp4
gaanim export mi-charla --output mi-charla.gaanim   # graba un paquete
gaanim --present --monitor 1 mi-charla   # presenta a pantalla completa
gaanim --present mi-charla.gaanim        # presenta un paquete, sin Python
gaanim --diff --example mi-video         # compara capturas con un baseline
gaanim update                            # instala la versión más reciente
gaanim --version
```

`gaanim --help` resume el uso y `gaanim <comando> --help` lista las opciones de
`init`, `check`, `inspect`, `export` y `--diff`. `--help`, `--version` y `update`
funcionan aunque no haya un Python compatible instalado, igual que reproducir,
presentar y exportar un paquete `.gaanim` (ver
#link("/guias/compartir/")[Compartir sin Python]); el resto de comandos
necesita el entorno de Python que Gaanim detecta junto al proyecto (ver
#link("/guias/proyectos/")[Proyectos]).

== Códigos de salida

#table(
  columns: (auto, 1fr),
  inset: 7pt,
  [*Código*], [*Significado*],
  [`0`], [El comando terminó bien.],
  [`1`], [`check` encontró errores (o advertencias con `--strict`), la exportación falló o `update` no pudo actualizar.],
  [`2`], [Argumentos inválidos, proyecto o manifiesto inválido, entorno de Python no disponible, o un script que falla o no llama a `scene.render()`.],
)

= Previsualizar

```bash
gaanim [--present] [--monitor <ÍNDICE>] [--duration <TIEMPO>] [--sections <LISTA>] [--from <NOMBRE>] [--listen <DIRECCIÓN>] <SCRIPT_O_PROYECTO>
gaanim [--present] [--monitor <ÍNDICE>] [--duration <TIEMPO>] [--sections <LISTA>] [--from <NOMBRE>] [--listen <DIRECCIÓN>] <PAQUETE.gaanim>
```

Sin argumentos, `gaanim` abre el Inicio: crear un proyecto, abrir una carpeta o
volver a uno de los diez proyectos recientes. Con un script o proyecto abre el
editor, que vuelve a ejecutar el script al guardar (hot reload) y recarga la
escena cuando cambia un asset del proyecto. Con un paquete `.gaanim`, `gaanim`
reproduce los fotogramas grabados con las mismas opciones, sin cargar Python.

#table(
  columns: (auto, 1fr),
  inset: 7pt,
  [*Opción*], [*Efecto*],
  [`--present`], [Abre la escena en modo presentación: pantalla completa sin bordes para la audiencia y Presenter View para quien presenta. Requiere `<SCRIPT_O_PROYECTO>`.],
  [`--monitor <ÍNDICE>`], [Monitor de la presentación, contado desde `0`. Sin él se usa el monitor principal. Solo es válido junto con `--present`.],
  [`--duration <TIEMPO>`], [Duración prevista de la charla: `20m`, `1h30m`, `90s`, `20:00` o un número de minutos. Presenter View la cuenta hacia atrás y muestra si vas adelantado o retrasado (ver #link("/guias/presentaciones/#tiempo-de-la-charla")[El tiempo de la charla]).],
  [`--sections <LISTA>`], [Reproduce solo los segmentos indicados, separados por comas, por ejemplo `resultados,cierre`.],
  [`--from <NOMBRE>`], [Empieza en ese segmento y sigue hasta el final. Combinado con `--sections`, recorta la lista a partir de ese nombre.],
  [`--listen <stdio|HOST:PUERTO>`], [Deja que un editor de código mueva la vista previa y sepa dónde está, con JSON por líneas (ver #link(<listen>)[Controlar la vista previa desde un editor]).],
  [`--message-format <FORMATO>`], [`json`: mensajes, recargas y errores del script con su línea en JSON por líneas (ver #link(<message-format>)[Mensajes para herramientas]).],
)

Cada nombre de `--sections` y `--from` puede ser el nombre completo de un
segmento, la clave de un `Section` (selecciona todos sus pasos) o el nombre de
un `SectionStep`. No distingue mayúsculas de minúsculas. Para un nombre con
comas, pásalo solo (`--sections "Tiempo, lugar y orientación"`: si el valor
completo nombra un segmento, se usa tal cual) o escribe sus comas como `\,`
dentro de una lista (`--sections "Tiempo\, lugar y orientación,Cierre"`). Un
nombre que no
selecciona nada muestra un error con los nombres disponibles y se reproduce la
escena completa. La escena se
construye entera igualmente, así que objetos, cámara y tema llegan al primer
segmento elegido en el mismo estado que en una reproducción completa; solo se
limitan la reproducción y la navegación entre pausas.

```bash
gaanim mi-charla --sections portada,resultados
gaanim --present --monitor 1 mi-charla --from resultados
```

Los atajos de teclado del modo presentación y de Presenter View están en
#link("/guias/presentaciones/")[Presentaciones].

== Controlar la vista previa desde un editor <listen>

Con `--listen`, un editor de código mueve la vista previa y sabe dónde está:
lleva la vista previa a la diapositiva del cursor sin reiniciarla y muestra en
qué segmento y pausa está. Cada mensaje es un objeto JSON en una línea.

- `--listen stdio`: las órdenes llegan por la entrada estándar y los eventos
  salen por la salida estándar, para un editor que lanza `gaanim` como
  proceso hijo. Lo que el script escriba con `print` también sale por ahí:
  descarta las líneas que no sean un objeto JSON.
- `--listen 127.0.0.1:7801` (o `localhost:7801`, `[::1]:7801`): escucha en ese
  puerto de este ordenador; cada editor que se conecta envía órdenes y recibe
  los eventos, empezando por el último `ready` y el último `playhead`. Solo
  acepta direcciones locales.

Órdenes:

```json
{"cmd": "seek", "time": 12.5}
{"cmd": "seek", "segment": "Resultados"}
{"cmd": "seek", "segment": "resultados", "stop": "grafico"}
{"cmd": "seek", "stop": "grafico"}
{"cmd": "follow", "file": "C:/charla/main.py", "line": 42}
{"cmd": "play"}
{"cmd": "pause"}
```

`seek` se detiene en un instante, en un segmento o un `Section` (por su
clave), o en una pausa por su nombre, dentro del segmento indicado si lo hay;
los nombres no distinguen mayúsculas. Un segmento se muestra donde descansa por
primera vez: en su primera pausa (en un `Section`, la primera de sus pasos),
como su primera página del PDF, o en su final si no tiene pausas. Su inicio
todavía puede mostrar la diapositiva anterior. `follow` se detiene
en el instante que representa esa línea del script, con las mismas reglas que
#link(<gaanim-inspect>)[`gaanim inspect`]: lo creado en esa línea o, si no,
en la línea más cercana por encima en el mismo archivo. Un segmento cuenta
como en `seek`, una pausa o un marcador por su instante, un voiceover por su
inicio y un `play` por su final, cuando lo que anima ya llegó.

Eventos:

```json
{"event": "reload"}
{"event": "ready", "duration": 42.0}
{"event": "playhead", "time": 12.5, "segment": "Resultados", "stop": "grafico", "playing": false}
{"event": "failed", "message": "NameError: name 'cirlce' is not defined", "file": "C:/charla/main.py", "line": 17}
{"event": "error", "message": "seek: no segment or section named \"Cierre\""}
```

`reload` llega cuando el script se volvió a ejecutar y la escena se
reconstruye, y `ready` cuando ya se ve. `playhead` llega al moverse: enseguida
tras un salto, una pausa o al cambiar de segmento o de pausa, y como mucho
diez veces por segundo mientras se reproduce. `stop` es `null` antes de la
primera pausa del segmento y `""` en una pausa sin nombre. `failed` trae la
línea del error del script cuando el traceback la da, y `error`, por qué no
se pudo cumplir una orden.

```bash
gaanim mi-charla --listen stdio
gaanim --present mi-charla --listen 127.0.0.1:7801
```

= `gaanim init`

```bash
gaanim init <video|slides> [DIRECTORIO] [--force]
```

Crea un proyecto ejecutable con un `main.py` mínimo: `video` genera una
escena de dos segundos (un título y un círculo) y `slides`, dos diapositivas
con notas y pausas para Presenter View. Sin
`DIRECTORIO` se usa `gaanim-video` o `gaanim-slides`.

#table(
  columns: (auto, 1fr),
  inset: 7pt,
  [*Archivo*], [*Contenido*],
  [`main.py`], [Escena inicial del tipo elegido.],
  [`gaanim.toml`], [Manifiesto del proyecto (ver #link("/referencia/gaanim-toml/")[`gaanim.toml`]).],
  [`pyproject.toml`], [Proyecto uv sin paquete propio (`requires-python = ">=3.14"`) que declara `gaanim` como dependencia.],
  [`.python-version`], [`3.14`.],
  [`assets/`, `exports/`], [Carpetas de recursos y de salidas, con un `.gitkeep`.],
  [`README.md`, `AGENTS.md`], [Instrucciones de uso y guía para asistentes de código.],
  [`.gitignore`], [Ignora exportaciones, capturas y vídeos generados.],
)

Después de escribir los archivos, `init` prepara `.venv` con uv e instala el
paquete de autoría de Gaanim; si no lo consigue, avisa y el proyecto sigue
siendo válido. Si algún archivo del scaffold ya existe, falla. `--force`
reescribe solo esos archivos y no borra nada de `assets/` ni de otras carpetas.

= `gaanim check`

```bash
gaanim check <SCRIPT_O_PROYECTO> [--strict] [--message-format json]
```

Ejecuta el script sin abrir ventana y revisa la escena resultante. Imprime la
duración y el tamaño del marco, una línea por problema (`error` o `warning`) y
termina con `pass` o `fail`:

```text
  ▸ check     charla/main.py · presentation
              3 segments · 5 stops · 42.0 seconds · 16×9
  ! warning   segment `intro` contains unnamed stops; names improve Presenter View
  ✓ pass      with 1 warning
```

- En toda escena: que la línea de tiempo dure algo, que no queden textos de
  plantilla que empiecen por `[` y, si quitaste el tema (`theme=None` o
  `set_theme(None)`), que el fondo no sea blanco o casi blanco, porque el
  texto, las formas y los ejes sin color propio también son blancos y no se
  verían.
- Que ningún texto quede tapado por completo por una figura opaca dibujada
  encima en una pausa o al final de un segmento. El aviso dice el texto, la
  figura y dónde se tapa por primera vez; súbelo con `z_layer` o `z_index`.
  Un texto oculto a propósito (con opacidad 0) o detrás de una figura
  translúcida, de una imagen o de una figura marcada con
  `covers_on_purpose()` no cuenta.
- Si la escena usa segmentos (una presentación): que exista al menos un
  segmento, que ninguno dure cero segundos, que cada uno tenga notas y que
  las pausas tengan nombre; avisa si el marco no es 16:9 o si la escena dura
  menos de un segundo.

Con un paquete `.gaanim`, `check` no necesita Python: describe el paquete y
vuelve a componer cada fotograma para compararlo con el resumen grabado
(ver #link("/guias/compartir/")[Compartir sin Python]).

Los errores devuelven el código `1`. `--strict` también devuelve `1` cuando
solo hay advertencias. Si el script lanza una excepción o no termina con
`scene.render()`, `check` lo informa y devuelve `2`.

```bash
gaanim check mi-charla --strict
```

Con `--message-format json`, el informe sale en JSON por líneas, cada
diagnóstico con su archivo y línea cuando los tiene (ver
#link(<message-format>)[Mensajes para herramientas]).

= `gaanim inspect` <gaanim-inspect>

```bash
gaanim inspect <SCRIPT_O_PROYECTO> [--json]
```

Ejecuta el script sin abrir ventana y lista su línea de tiempo: cada segmento
con su inicio y su final, sus pausas, los marcadores, los voiceovers y la toma
en vivo, cada uno con el archivo y la línea del script que lo creó. Sirve para
que un editor de código sepa qué diapositiva tiene el cursor sin leer el
Python.

```text
charla/main.py · 3 segments · 4 stops · 1 markers · 42.00 s
   0.00–12.00   Portada                 main.py:8
   4.50           stop titulo           main.py:14
  12.00–30.00   resultados · 1 · 1 · Datos
  20.00           marker climax         main.py:31
```

La línea es la del script que hizo la llamada: si una función tuya crea el
segmento, la de la llamada dentro de esa función; un segmento que crea un
`Section` lleva la línea del `Section`. Lo que no sale de un script, como el
segmento implícito, no tiene línea.

Con `--json` imprime un solo objeto en una línea, la última de la salida
estándar: lo que el script escriba con `print` sale antes. `schema` es `1` y
solo crecerá con campos nuevos. Los tiempos van en segundos, y `file` y `line` son `null` cuando no
hay línea:

```json
{
  "schema": 1,
  "script": "C:/charla/main.py",
  "duration": 42.0,
  "segments": [
    {"name": "Portada", "section": null, "start": 0.0, "end": 12.0,
     "file": "C:/charla/main.py", "line": 8,
     "stops": [{"name": "titulo", "time": 4.5, "file": "C:/charla/main.py", "line": 14}]}
  ],
  "sections": [{"key": "resultados", "start": 12.0, "end": 30.0}],
  "markers": [{"name": "climax", "time": 20.0, "file": "C:/charla/main.py", "line": 31}],
  "voiceovers": [{"key": "intro", "start": 0.0, "end": 6.2, "file": "C:/charla/main.py", "line": 10}],
  "live_take": null,
  "plays": [{"start": 0.0, "end": 1.5, "file": "C:/charla/main.py", "line": 11}]
}
```

`section` es la clave del `Section` que generó el segmento, y `sections` da
cada clave una vez, del inicio de su primer segmento al final del último.
`plays` tiene cada `play` con su inicio y su final: con ellos un editor pasa
de una línea al instante que muestra. Un paquete `.gaanim` no guarda líneas,
así que `inspect` solo acepta scripts y proyectos.

= `gaanim export`

```bash
gaanim export <SCRIPT_O_PROYECTO> --output <ARCHIVO> [OPCIONES]
gaanim export <PAQUETE.gaanim> --output <VÍDEO_O_PDF> [OPCIONES]
```

Renderiza la escena completa, o un tramo, a un archivo. El script no cambia:
la resolución y la calidad se eligen aquí, no en `Scene`. Las pausas
(`scene.stop()`) no detienen la exportación de un vídeo; un PDF tiene una
página por pausa (ver #link(<diapositivas-pdf>)[Diapositivas en PDF]).

== Formatos

La extensión de `--output` elige el formato.

#table(
  columns: (auto, 1fr, auto),
  inset: 7pt,
  [*Extensión*], [*Formato*], [*Transparencia*],
  [`.mp4`], [H.264. Necesita FFmpeg.], [No],
  [`.webm`], [VP9. Necesita FFmpeg.], [Sí],
  [`.mov`], [Apple ProRes con audio PCM, para editores de vídeo: 422 HQ, o 4444 con `--transparent`. Necesita FFmpeg.], [Sí],
  [`.webp`], [WebP animado. Necesita FFmpeg.], [Sí],
  [`.gif`], [GIF. Necesita FFmpeg.], [No],
  [`.png`], [Secuencia de PNG, un archivo por fotograma. No necesita FFmpeg.], [Sí],
  [`.pdf`], [Diapositivas: una página por cada pausa de la presentación y otra para el final, con las notas del orador si se piden con `--notes`. No necesita FFmpeg.], [No],
  [`.gaanim`], [Paquete de reproducción: todos los fotogramas que dibuja la escena, con sus segmentos, pausas, marcadores y audio, en un archivo que se reproduce, presenta y exporta sin Python.], [No],
)

En una secuencia PNG, un patrón `%d` o `%0Nd` en el nombre se sustituye por el
número de fotograma desde 0 (`frames/f_%04d.png` escribe `f_0000.png`,
`f_0001.png`…). Sin patrón, el número se añade al nombre: `frame.png` escribe
`frame_00000.png`. El audio de la escena se mezcla en MP4, WebM y MOV.

== Opciones

#table(
  columns: (auto, auto, 1fr),
  inset: 7pt,
  [*Opción*], [*Predeterminado*], [*Efecto*],
  [`-o`, `--output <ARCHIVO>`], [obligatorio], [Archivo de salida; su extensión elige el formato.],
  [`--quality <PRESET>`], [`standard`], [`draft`: 30 fps y codificación rápida. `standard`: 60 fps y equilibrio. `production`: 60 fps y la mejor compresión, más lenta. GIF y WebP se quedan en 30 fps.],
  [`--fps <TASA>`], [la de `--quality`], [Fotogramas por segundo, de 1 a 240: un entero (`24`), una fracción (`24000/1001`) o un decimal (`23.976`). Sustituye a la tasa de la calidad.],
  [`--width <PX>`], [`1920`], [Ancho de salida en píxeles.],
  [`--height <PX>`], [`1080`], [Alto de salida en píxeles.],
  [`--fit <MODO>`], [`error`], [Qué hacer si la proporción de salida no coincide con el marco de la escena: `error` falla, `contain` añade bandas y `cover` recorta los bordes.],
  [`--encoder <CODIFICADOR>`], [`auto`], [Solo MP4: `auto`, `libx264`, `nvenc`, `amf`, `qsv` o `vaapi`.],
  [`--transparent`], [desactivado], [Conserva el canal alfa en WebM, MOV, WebP y PNG. MP4 y GIF lo rechazan.],
  [`--from <SEGUNDOS|NOMBRE>`], [`0`], [Inicio del tramo exportado: segundos, un marcador, un segmento (su inicio) o una pausa.],
  [`--to <SEGUNDOS|NOMBRE>`], [fin de la escena], [Fin del tramo exportado; un segmento lo termina en su final.],
  [`--frame <SEGUNDOS|NOMBRE>[,…]`], [—], [Escribe exactamente un PNG en cada instante, en lugar de un tramo; un segmento da su primera pausa. Necesita una salida `.png` y no se combina con `--from` ni `--to`.],
  [`--notes`], [desactivado], [Solo PDF: añade tras la última página de cada diapositiva una página con sus notas (`scene.segment(notes=...)`).],
  [`--message-format <FORMATO>`], [`human`], [`json`: mensajes y progreso con su porcentaje en JSON por líneas, sin barra (ver #link(<message-format>)[Mensajes para herramientas]).],
)

La composición se define en unidades lógicas, así que `--width` y `--height`
cambian la nitidez, no la disposición. Con una proporción distinta de la del
marco (por ejemplo 1080×1920 para una escena 16×9) hace falta `--fit contain`
o `--fit cover`.

Con `--encoder auto`, MP4 prueba NVENC, AMF y QSV con una codificación real y
usa el primero que funciona; si ninguno funciona, usa `libx264`. Un
codificador explícito no tiene alternativa: si falla, la exportación falla.
VAAPI nunca se prueba automáticamente, porque un driver defectuoso puede
bloquear la GPU.

`--transparent` solo tiene efecto si la escena tiene un fondo con alfa, por
ejemplo `Scene(background="#00000000")`.

== Fotogramas por segundo <fotogramas-por-segundo>

Sin `--fps`, un vídeo sale a 30 fps con `--quality draft` y a 60 con
`standard` o `production`; GIF y WebP no pasan de 30. `--fps` elige otra tasa
para cualquier formato: 24 para cine y animación, 25 o 50 para PAL, y las
fracciones NTSC 24000/1001, 30000/1001 y 60000/1001. Un decimal que redondea
una de ellas (`23.976`, `29.97`, `59.94`) significa esa fracción exacta, que es
la que lleva el archivo. La duración y el audio no cambian: solo cuántos
fotogramas la cubren. El desenfoque de movimiento reparte sus muestras en el
intervalo de cada fotograma de esa tasa.

```bash
gaanim export mi-video --output exports/cine.mp4 --fps 24
gaanim export mi-video --output exports/ntsc.mp4 --fps 24000/1001
```

== Editores de vídeo <editores-video>

Un `.mov` es Apple ProRes, el formato de intercambio que Premiere, DaVinci
Resolve y Final Cut importan sin conversión. Sin `--transparent` es ProRes 422
HQ; con `--transparent`, ProRes 4444 con canal alfa, para poner rótulos,
_lower thirds_ y gráficos de Gaanim encima de un montaje. El color se guarda
etiquetado como BT.709 y el audio, sin comprimir (PCM de 24 bits). ProRes no
tiene niveles de compresión: `--quality` solo elige los fps, y los archivos
pesan mucho más que un MP4.

```bash
gaanim export rotulo.py --output rotulo.mov --transparent
gaanim export mi-video --output master.mov --fps 25
```

== Tramos y marcadores <tramos-nombres>

`--from` y `--to` aceptan segundos o un nombre, y se pueden combinar. Los
nombres se resuelven después de ejecutar el script, en este orden:

+ un #link("/referencia/scene/#api-scene-marker")[`scene.marker`], con su
  nombre exacto: su instante;
+ un segmento, la clave de un `Section` (todos sus pasos) o el nombre de un
  `SectionStep`, sin distinguir mayúsculas, como en `--from` al
  previsualizar: `--from` empieza en su inicio y `--to` termina en su final;
+ el nombre de una pausa (`scene.stop("nombre")`), sin distinguir
  mayúsculas: el instante en que descansa.

Un número son siempre segundos. Un nombre que no existe produce un error con
los marcadores, segmentos y pausas definidos, y el de una pausa que se repite
en varios segmentos es ambiguo: el error dice dónde está cada una, para pasar
sus segundos. El audio se recorta al
mismo tramo, una secuencia PNG numera sus fotogramas desde 0 y un `--to`
posterior al final se limita a la duración de la escena. `--to` debe quedar
después de `--from`.

```bash
gaanim export mi-video --output exports/video.mp4 --quality production
gaanim export mi-video --output exports/vertical.mp4 --width 1080 --height 1920 --fit cover
gaanim export overlay.py --output overlay.webm --transparent
gaanim export rotulo.py --output rotulo.mov --transparent
gaanim export mi-video --output exports/tramo.mp4 --from 12 --to 15
gaanim export mi-video --output exports/climax.mp4 --from climax --to fin
gaanim export mi-charla --output exports/resultados.mp4 --from resultados --to resultados
gaanim export mi-video --output frames/f_%04d.png --quality draft
```

== Fotogramas sueltos

Para revisar la composición en un instante, `--frame` escribe exactamente un
PNG con lo que se ve en ese momento, sin exportar un tramo. Acepta segundos y
nombres separados por comas, con las mismas reglas que
#link(<tramos-nombres>)[`--from`]; una coma dentro de un nombre se escribe
`\,`. Un segmento o `Section` da el instante de su primera pausa, el mismo de
su primera página en el #link(<diapositivas-pdf>)[PDF], o su final si no tiene
pausas; una pausa, el instante en que descansa, el mismo que captura
`--diff --capture-stops`. Con un solo instante escribe el archivo de
`--output`; con varios añade su posición al nombre (`fotos/f.png` escribe
`f_1.png`, `f_2.png`…). Respeta `--width`, `--height`, `--fit` y
`--transparent`.

```bash
gaanim export mi-video --output revision.png --frame 11.4
gaanim export mi-video --output fotos/f.png --frame 3,11.4,climax
gaanim export mi-charla --output problema.png --frame Problema
```

== Diapositivas en PDF <diapositivas-pdf>

Con `--output` terminado en `.pdf`, `export` escribe una página por cada punto
en el que se detiene una presentación: cada `scene.stop()`, en orden, y el
final de la escena si la última pausa no está ahí. Cada página es el
fotograma que muestra el Presenter en esa pausa, al tamaño de `--width` y
`--height` (con `--fit` si la proporción no coincide), sin pérdida. Las páginas
miden 960 puntos de ancho, como una diapositiva panorámica, con la proporción
de la escena.

Con `--notes`, tras la última página de cada diapositiva (cada
`scene.segment(...)`) va una página con su nombre y sus notas, en texto que se
puede buscar y copiar. Una diapositiva sin notas no añade página. `--from`,
`--to`, `--frame`, `--fps`, `--transparent` y `--encoder` no se aplican.

```bash
gaanim export mi-charla --output mi-charla.pdf
gaanim export mi-charla --output material.pdf --notes
gaanim export mi-charla.gaanim --output mi-charla.pdf   # sin Python
```

Un paquete se exporta a PDF igual, sin ejecutar Python. El diálogo
*Export* del editor también ofrece el formato PDF, con una casilla para las
notas.

== Paquetes

Con `--output` terminado en `.gaanim`, `export` graba un paquete de la escena
completa; `--from`, `--to`, `--transparent`, `--notes` y `--encoder` no se aplican, y
`--fps` (60 por defecto) debe ser un entero. Con un paquete como entrada,
`export` lo renderiza a vídeo o a PDF sin ejecutar Python: acepta los demás formatos y
opciones, incluidos los tramos con marcadores, y el vídeo tiene la frecuencia
con la que se grabó el paquete; un `--fps` distinto es un error. El resultado
es idéntico, píxel a píxel, a exportar el script con esa frecuencia y el mismo
tamaño.

```bash
gaanim export mi-charla --output mi-charla.gaanim --fps 30
gaanim export mi-charla.gaanim --output mi-charla.mp4 --quality production
```

`gaanim pack` vuelve a escribir un paquete en el formato actual, sin Python
ni GPU. Un paquete grabado con una versión anterior pasa así al formato que
los reproductores abren con su primer bloque. Con `--web`, además, guarda los
trazados en una rejilla y deja fuera lo que solo usan `gaanim check` y el
desenfoque de movimiento al exportar vídeo. Queda más ligero para
#link("/guias/compartir/#insertar")[insertarlo en otras páginas]:

```bash
gaanim pack mi-charla.gaanim mi-charla.gaanim       # al formato actual
gaanim pack mi-charla.gaanim web/charla.gaanim --web
```

La guía #link("/guias/compartir/")[Compartir sin Python] explica qué guarda
un paquete, cómo se reproducen los callbacks y sus límites.

= `gaanim relay`

Configura el relay que lleva los votos de las encuestas (`scene.poll`) a la
presentación. No necesita Python. La guía
#link("/publico/primera-encuesta/")[Tu primera encuesta]
explica el despliegue.

```bash
gaanim relay                          # muestra el relay en uso y de dónde sale
gaanim relay init [CARPETA] [--force] # escribe el relay (por defecto ./gaanim-relay)
gaanim relay use https://…workers.dev # lo guarda para todas tus presentaciones
gaanim relay forget                   # olvida el relay guardado
gaanim relay reset [RUTA]             # partida nueva en la sesión del proyecto de RUTA
gaanim relay results [RUTA] [--output CARPETA]  # guarda los resultados de la partida
gaanim relay key <ARCHIVO.gaanim|CÓDIGO> [--src URL]  # clave para presentar en la web
```

El orden de prioridad es `GAANIM_POLL_RELAY`, luego `[polls] relay` del
proyecto y por último el guardado con `gaanim relay use`. La dirección debe
empezar por `https://` (o `http://` para probar con `npx wrangler dev`).

`gaanim relay reset` empieza una partida nueva, como *New game* en la vista
del presentador: borra votos, respuestas y jugadores de la sesión del proyecto
al que pertenece `RUTA`, un script o una carpeta (por defecto la actual).

`gaanim relay results` descarga la partida que el relay guarda para ese
proyecto y escribe sus planillas (`jugadores.csv`, `respuestas.csv` y
`preguntas.csv`) en `resultados/` dentro de la carpeta de salida del
proyecto, o en `--output`. Sirve si la presentación se cerró antes de
guardarlas; el relay olvida una partida doce horas después de su última
actividad. Ver
#link("/publico/presentar/#resultados")[Guardar los resultados].

`gaanim relay key` imprime la clave de presentador de la sesión de un paquete
(o de un código de sesión), para presentarla desde el reproductor web en vez
de esta computadora. Con `--src` imprime el enlace completo al reproductor,
terminado en `#clave=…`. Ver
#link("/publico/presentar/#web")[En el reproductor web].

= `gaanim update` <gaanim-update>

Instala la versión más reciente de Gaanim sobre la carpeta del `gaanim` que
lo ejecuta. No necesita Python.

```bash
gaanim update --check   # dice si hay una versión nueva y dónde están sus novedades
gaanim update           # la descarga y la instala
```

Busca entre las releases de GitHub la más reciente con el paquete de tu
sistema (Windows o Ubuntu x64) y, si es más nueva que la que usas, descarga el
paquete, comprueba su suma SHA-256, lo descomprime en `.gaanim-update/` dentro
de la instalación y comprueba que el `gaanim` nuevo arranca. Solo entonces
cambia los archivos: los que reemplaza pasan a `.gaanim-update/old/`, porque
Windows no deja sobrescribir un programa abierto pero sí moverlo, y se borran
la próxima vez que se abre Gaanim. Si un paso falla, la instalación queda como
estaba. Cada proyecto recibe el paquete de autoría nuevo la próxima vez que se
abre.

Un `gaanim` compilado desde el código (en la carpeta `target/` de Cargo) no se
actualiza así: actualízalo con `git pull` y vuelve a compilar.

La pantalla de inicio consulta GitHub como mucho una vez al día y, si hay una
versión nueva, ofrece instalarla con el mismo proceso. Desactiva esa consulta
con la variable `GAANIM_NO_UPDATE_CHECK=1`.

= `gaanim --diff`

```bash
gaanim --diff --example <SCRIPT_O_PROYECTO> [OPCIONES]
gaanim --diff --baseline <DIR> --current <DIR> [OPCIONES]
```

Captura fotogramas exactos de una escena y los compara con un baseline
aprobado y guarda un informe JSON y HTML (`report/index.html`) que se revisa
en el navegador. Las capturas de
`--example escenas/intro.py` viven en `tests/visual/escenas/intro/`
(`baseline/`, `current/` y `report/`). El flujo completo está en
#link("/guias/capturas-y-comparacion/")[Capturas y comparación visual].

#table(
  columns: (auto, 1fr),
  inset: 7pt,
  [*Opción*], [*Efecto*],
  [`-e`, `--example <SCRIPT_O_PROYECTO>`], [Captura la escena y la compara con su baseline. Con un paquete `.gaanim` captura sus pausas sin Python, como `--capture-stops`.],
  [`--bless`], [Guarda la captura como baseline y termina. Sobrescribe el baseline anterior.],
  [`--capture-only`], [Escribe solo `current/` y termina, sin comparar.],
  [`--no-capture`], [Compara los PNG que ya están en `current/`.],
  [`--capture-stops`], [Captura el fotograma de cada `scene.stop()` en lugar de los tiempos de `scene.snapshots`; el script no necesita cambios.],
  [`--stops <LISTA>`], [Con `--capture-stops`, solo esas pausas: números y rangos contados desde 1, como `3-7,12`.],
  [`--sections <LISTA>`, `--from <NOMBRE>`], [Con `--capture-stops`, solo las pausas de esos segmentos, con las mismas reglas que al previsualizar.],
  [`--height <PX>`], [Captura los fotogramas con esa altura en píxeles (de 16 a 8192); el ancho sigue la proporción de la escena. Por defecto, la altura de la baseline que ya existe, o 960 en el lado largo si no hay ninguna. Una captura más chica es más rápida para revisar muchas pausas; si su tamaño no coincide con el de la baseline, `--diff` lo dice en vez de comparar. No se aplica a paquetes `.gaanim`.],
  [`--tests-root <DIR>`], [Carpeta raíz de las capturas; por defecto `tests/visual`.],
  [`-b`, `--baseline <DIR>`, `-c`, `--current <DIR>`], [Modo manual: compara dos carpetas cualesquiera.],
  [`-o`, `--output <DIR>`], [Carpeta del informe.],
  [`--pixel-threshold <0..255>`], [Diferencia por canal que se ignora.],
  [`--max-changed-ratio <0..1>`], [Fracción de píxeles distintos que se tolera.],
)

Sin `--capture-stops`, el script elige los instantes: cuando existe la
variable de entorno `GAANIM_SNAPSHOTS`, debe llamar a
#link("/referencia/scene/#api-scene-snapshots")[`scene.snapshots`] con esa ruta.

```bash
gaanim --diff --example escenas/intro.py --bless
gaanim --diff --example escenas/intro.py --pixel-threshold 4
gaanim --diff --example mi-charla --capture-stops --stops 3-7 --capture-only
```

= Mensajes para herramientas: `--message-format json` <message-format>

`gaanim check`, `gaanim export` y la vista previa aceptan
`--message-format json`: cada línea que escribirían para una persona sale
como un objeto JSON en una línea de la salida estándar, para que un editor de
código o un script la lea sin interpretar texto. Sin la opción, o con
`--message-format human`, la salida es la de siempre. Los tracebacks de Python
siguen saliendo por la salida de errores, y lo que el script escriba con
`print` sale tal cual: descarta las líneas que no sean un objeto JSON.

Cada objeto tiene un `type`:

- `message`: una línea de estado, con `level` (`debug`, `info`, `success`,
  `warning` o `error`), `label`, `message` y el `file`, `line` y `column` del
  script a los que se refiere, o `null`. Un error del script trae la línea que
  nombra su traceback; `column` todavía es siempre `null`.
- `detail` (`key`, `value`) y `hint` (`message`): las líneas que acompañan a un
  mensaje.
- `diagnostic`: un error o aviso de `check`, con `level`, `message`, `file`,
  `line` y `column`. Un aviso sobre un segmento trae la línea que lo creó.
- `check`: el veredicto de `check`, con `result` (`pass` o `fail`), `errors`,
  `warnings`, `segments`, `stops`, `duration`, `width` y `height`. El código de
  salida es el de siempre.
- `progress`: el avance de una exportación, con `current`, `total` y `percent`,
  una vez por cada punto porcentual. La barra de progreso no se dibuja.
- `reload`: en la vista previa, `phase` `start` cuando un cambio vuelve a
  ejecutar el script (con `cause`, `source` o `assets`) y `end` cuando la
  escena nueva ya se ve, con `seconds` y lo que tardaron `python`, `replay` y
  `seek`.

```bash
gaanim check mi-charla --message-format json
gaanim export mi-charla -o charla.mp4 --message-format json
gaanim mi-charla --message-format json --listen stdio
```

```json
{"type":"diagnostic","level":"warning","message":"segment `Problema` has no speaker notes","file":"C:/charla/main.py","line":31,"column":null}
{"type":"check","script":"C:/charla/main.py","kind":"presentation","result":"pass","errors":0,"warnings":1,"segments":6,"stops":9,"duration":84.5,"width":16,"height":9}
{"type":"progress","current":1530,"total":5070,"percent":30}
{"type":"message","level":"error","label":"python","message":"NameError: name 'cirlce' is not defined","file":"C:/charla/main.py","line":17,"column":null}
```

Con `--listen stdio`, los eventos del canal de control comparten la salida
estándar; se distinguen porque llevan `event` en lugar de `type`.

= Variables de entorno

#table(
  columns: (auto, 1fr),
  inset: 7pt,
  [*Variable*], [*Efecto*],
  [`GAANIM_SNAPSHOTS`], [La define `gaanim --diff` con la carpeta donde `scene.snapshots` debe capturar. No la definas a mano.],
  [`GAANIM_EDITOR`], [Comando con el que el inspector, la consola, las animaciones, el panel de error y *Open and complete* abren una línea del script o del guion, con `{file}` y `{line}` donde van la ruta y el número de línea: `code --goto {file}:{line}`, `zed {file}:{line}`, `idea --line {line} {file}`. Las comillas agrupan palabras con espacios. Sin `{file}`, la ruta se añade al final. Sin la variable, Gaanim prueba `code`, `cursor`, `codium`, `zed` y `subl` en el `PATH`, y si no encuentra ninguno abre el archivo con la aplicación predeterminada.],
  [`GAANIM_INCREMENTAL=0`], [Desactiva la recompilación incremental del hot reload: cada guardado recompila la escena entera.],
  [`GAANIM_RELOAD_PROFILE=1`], [Escribe en la terminal qué costó cada recarga: el tiempo del script y de sus segmentos más lentos, las mediciones con `bounds()` (cuántas se compilaron aparte, cuántas compilaron la escena y en qué línea del script, cuántas reanudaron una compilación anterior, cuántas la reutilizaron), las fases del replay (las huellas que deciden qué segmentos se reutilizan, la limpieza o la restauración de la escena anterior, la compilación), los segmentos que más tardaron en compilarse y la captura del fotograma de t=0. Al abrir un script, también cuánto tardó cada fase del arranque: el entorno de Python, el intérprete, la preparación de la app y de la GPU, el primer fotograma y la primera escena lista.],
  [`GAANIM_PREVIEW_RESOLUTION`], [Resolución de la previsualización del editor. Con `auto` (por defecto), si al reproducir no llega a 60 fps porque dibujar cuesta demasiado, baja al 75 % y luego al 50 % de los píxeles, y vuelve a la resolución completa al pausar. `full` la mantiene siempre completa; una fracción como `0.5` la fija. Nunca afecta a las exportaciones.],
  [`GAANIM_FRAME_PROFILE=1`], [Reproduce la línea de tiempo entera sin detenerse en las pausas y escribe en la terminal, una vez por segundo, el coste del seek, de la composición y del render, la resolución de la previsualización, el tamaño de la escena de Vello (caminos, segmentos y recortes, el mayor del segundo; un número alto de recortes suele venir de sombras o máscaras por glifo) y cuántos objetos están visibles. Al terminar lista los segundos más lentos y cierra el editor.],
  [`GAANIM_SEGMENT_CHECKPOINTS=0`], [Desactiva los puntos de control por segmento. Por defecto, al reproducir o recorrer un segmento, Gaanim guarda el estado de la escena en su inicio y cada fotograma parte de ahí, así que el coste de un fotograma no crece a lo largo de la presentación. Con `0`, cada fotograma se reconstruye desde el principio de la escena. El resultado es idéntico en ambos casos.],
  [`GAANIM_CHECKPOINT_TIMINGS=1`], [Escribe en la terminal cuánto tarda cada punto de control por segmento en guardarse.],
  [`GAANIM_POLL_RELAY`], [Dirección del relay de las encuestas. Tiene prioridad sobre `[polls] relay` y `gaanim relay use`.],
  [`GAANIM_NO_UPDATE_CHECK=1`], [La pantalla de inicio no consulta GitHub para avisar de versiones nuevas. `gaanim update` sigue funcionando.],
  [`GAANIM_POLL_SESSION`], [Código de seis caracteres que usan las encuestas en lugar del que Gaanim guarda para el proyecto, por ejemplo para que varias personas presenten el mismo proyecto o para capturas reproducibles.],
)
