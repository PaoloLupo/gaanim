#import "../../components/section.typ": docs-chapter

#show: docs-chapter.with(
  title: "Presentaciones",
  description: "Crea, ensaya y presenta diapositivas con notas y pausas",
  route: "/guias/presentaciones/",
)

En esta guía aprenderás a preparar una charla con Gaanim: diapositivas con
plantillas reutilizables, notas del orador, pausas donde esperar a que hables,
ensayos de una sola parte y la vista del presentador en un segundo monitor.

```python
# output: preview.webp
from gaanim import Scene, lecture, title_slide

scene = Scene(frame=(16, 9), margin=0.9)
scene.canvas.set_theme("presentation")
scene.slides.brand(footer="MI CHARLA", slide_numbers=True, rule=True)

cover = scene.segment("Portada", template=title_slide, notes="Presenta el tema.")
title = scene.text("Una idea clara", role="title")
subtitle = scene.text("Diapositivas reutilizables", role="subtitle")
cover.bind(title=title, subtitle=subtitle)
scene.play([title.animate.write().duration(0.8), subtitle.animate.fade_in().duration(0.6)])
scene.stop("portada")

content = scene.segment("Contenido", template=lecture, notes="Desarrolla la idea.")
heading = scene.text("Contenido", role="title")
body = scene.text("Una sola idea por diapositiva.", size=0.42)
content.bind(title=heading, body=body)
scene.play([heading.animate.fade_in().duration(0.4)])
scene.stop("titulo")
scene.play([body.animate.write().duration(0.8)])
scene.stop("mensaje")
scene.render()
```

La vista previa se reproduce de corrido porque, fuera del modo presentación,
las pausas no detienen nada. Al presentar, Gaanim espera en cada
`scene.stop(...)` hasta que avanzas.

= Crear el proyecto

```bash
gaanim init slides mi-charla
gaanim mi-charla
```

También puedes ejecutar `gaanim` sin argumentos, elegir *Nuevo proyecto* y
seleccionar *Slides*. Si aún no tienes Gaanim listo, sigue
#link("/empezar/instalacion/")[Instalación]. El proyecto de ejemplo no incluye
colores, fuentes ni contenido de ninguna institución: la identidad visual es
tuya.

= Diapositivas, pasos y notas

Una presentación de Gaanim es una escena normal organizada así:

- Cada `scene.segment("nombre", ...)` es una *diapositiva*; acepta una
  plantilla (`template=`) y notas (`notes=`). Su nombre y sus notas aparecen en la vista del presentador.
- Cada `scene.stop("nombre")` dentro de una diapositiva es un *paso*: el
  punto donde la reproducción espera a que avances.
- `segment.bind(...)` rellena los huecos de la plantilla con tus objetos.

`scene.slides.brand(...)` configura para toda la presentación el logo, el pie,
la numeración y la línea separadora. Las plantillas `title_slide`, `lecture`,
`comparison` y `credits` devuelven un layout adaptable y puedes reemplazarlas
por funciones de Python propias (ver #link("/guias/layout/")[Layout]).

= Comprobar y presentar

```bash
gaanim check mi-charla --strict
gaanim --present --monitor 1 mi-charla
```

`gaanim check` revisa segmentos, notas, paradas y el formato 16:9. Los índices
de monitor empiezan en cero. La exportación ignora las paradas y genera un
video continuo.

== Ensayar una parte

```bash
gaanim mi-charla --sections portada,contenido
gaanim --present mi-charla --from contenido
```

`--sections` reproduce solo los segmentos indicados y `--from` empieza en uno
y sigue hasta el final; si los combinas, `--from` recorta la lista. Cada
nombre selecciona el segmento con ese nombre exacto, todos los segmentos de
una `Section` con esa clave o el paso de una `Section` con ese nombre de
`SectionStep` (por ejemplo `--sections "Problemática · país sísmico"`). Un
nombre con comas se pasa solo o con sus comas escritas como `\,`, sin
distinguir mayúsculas. Un nombre desconocido se muestra como error con las
opciones disponibles, y entonces se reproduce todo.

El script se ejecuta completo: los objetos que persisten, la cámara y el tema
llegan a la primera diapositiva elegida exactamente como en la ejecución
entera, porque la selección *salta* a ese segmento en lugar de omitir código.
La reproducción y la navegación solo visitan lo elegido; al terminar un tramo
se salta al siguiente. Con `gaanim --diff ... --capture-stops` las mismas
opciones limitan las pausas capturadas (ver
#link("/guias/capturas-y-comparacion/")[Capturas y comparación visual]).

= La vista del presentador

Con `--present`, la audiencia ve la diapositiva a pantalla completa y tú ves
*Presenter View*, la vista del presentador. Se lee de arriba abajo:

- *Encabezado:* estado (`PLAYING`, `PAUSED` o `END`), `Slide n of N`, la hora,
  el tiempo transcurrido con un botón `↺` para reiniciarlo y una barra con un
  bloque por diapositiva y marcas de pasos; un clic salta a ese punto.
- *Now on screen:* nombre de la diapositiva, `Step k of K · nombre` y una
  vista grande de lo que ve la audiencia. Si la pantalla de la audiencia está
  en negro o en blanco, la vista lo indica. Debajo, unos botones permiten
  saltar al inicio de la diapositiva o a cualquiera de sus pasos.
- *Up Next:* la siguiente pausa, indicando si es otro paso de la misma
  diapositiva o la siguiente diapositiva.
- *Speaker notes:* tus notas, con tamaño ajustable mediante `A−`/`A+`.
- *Barra inferior:* primer paso, paso anterior, reproducir/pausa, paso
  siguiente, último paso, `Overview`, `Black` y `White`, el progreso de las
  vistas previas y la lista de atajos (icono de teclado).

== Atajos de teclado

#table(
  columns: 2,
  table.header[*Tecla*][*Acción*],
  [`→`, `Enter` o clic en la audiencia], [avanzar al siguiente paso],
  [`←` o `Backspace`], [volver al paso anterior],
  [`Space`], [pausar o reanudar la animación en curso; nunca salta de paso],
  [`Home` / `End`], [ir al primer o al último paso],
  [`O`], [vista general: busca por nombre de diapositiva, paso o notas; `Enter` salta a la primera coincidencia],
  [`B` / `W`], [poner la pantalla de la audiencia en negro o en blanco],
  [`P`], [volver a abrir la vista del presentador si la cerraste],
  [`Esc`], [cerrar la vista general o el negro/blanco; después, salir del modo presentación],
)

Saltar a una diapositiva muestra su estado inicial aunque la anterior termine
con una pausa en el mismo instante.

== Pantalla de la audiencia

Al llevar el cursor a la parte inferior de la pantalla completa aparece una
barra compacta con primer paso, paso anterior, reproducir/pausa, paso
siguiente, último paso, el nombre y número de la diapositiva y el progreso. Se
oculta al retirar el cursor o al pasar a la vista del presentador. Sus botones
y los atajos hacen lo mismo, así que un clic no avanza dos veces.

La pantalla completa, tanto en el editor como al presentar, ajusta el lienzo
al monitor sin deformarlo y rellena en negro las franjas sobrantes, sea cual
sea el fondo de la escena. En el editor, `Esc` o `F11` salen de la pantalla
completa.

== Vistas previas de las diapositivas

Las vistas previas se generan en segundo plano sin afectar a la presentación:
primero la diapositiva actual y la siguiente, luego el resto. Tras una recarga
en caliente, las anteriores siguen visibles, marcadas como
`Updating preview…`, hasta que llega su reemplazo. Se conservan al reabrir la
vista del presentador, se adaptan al tamaño y la densidad de píxeles de la
ventana y solo se regeneran al agrandarla. Si el renderizado falla, la barra
ofrece `Retry` y conserva las vistas ya generadas.

Las vistas previas incluyen el contenido 3D, igual que la pantalla de la
audiencia.

= Encuestas a la audiencia <encuestas-a-la-audiencia>

`scene.poll` hace una pregunta al público, al estilo de Kahoot, y te da los
datos para que tú decidas cómo presentarla:

```python
poll = scene.poll("¿Qué curva crece más rápido?", ["x²", "2ˣ", "x log x"], preview=[6, 14, 4])

card = scene.geometry.rounded_rect(4.2, 4.2, 0.25).fill(WHITE).no_stroke().move_to(-5, 0)
qr = poll.qr(3.6).move_to(-5, 0)                        # el QR, un drawable más
code = scene.text(poll.code, size=0.55).move_to(-5, -2.7)  # el código, por si no pueden escanear

for i, answer in enumerate(poll.options):
    y = 1.2 - 1.4 * i
    scene.text(answer, size=0.5).move_to(-1.6, y)
    poll.bar(i, length=6, thickness=0.6, radius=0.3).fill(RED).no_stroke().move_to(3.2, y)
    scene.viz.readout(poll.votes(i), format=".0f").move_to(6.8, y)

scene.stop()
```

La encuesta no dibuja nada por sí misma. Te da:

- `poll.qr(tamaño)`: el código QR como drawable, relleno de negro. Ponlo sobre
  un fondo claro con algo de margen para que los teléfonos lo lean.
- `poll.code` y `poll.url`: el código de seis caracteres y la dirección de
  votación, para mostrarlos como texto.
- `poll.votes(i)`, `poll.share(i)` (de 0 a 1) y `poll.total()`: `Parameter`
  que siguen los votos. Úsalos como cualquier parámetro: en `readout`, en
  `computed`, en puntos o en líneas reactivas (ver
  #link("/guias/reactividad/")[Reactividad]).
- `poll.bar(i, ...)`: una barra lista cuya longitud sigue a la respuesta `i`.
  Con `direction` crece hacia la derecha, la izquierda, arriba o abajo; con
  `scale="leader"` (por defecto) la respuesta que va ganando llena su barra, y
  con `"total"` cada barra mide su porcentaje. Sus límites son los de la barra
  completa, así que el layout no se mueve al llegar votos.
- `poll.close()`: deja de recibir votos en ese punto. Sin él, la encuesta se
  cierra al terminar el segmento donde se abrió. Los valores conservan el
  último conteo, así que un paso posterior puede comentar el resultado.

Al presentar, mientras la charla está dentro de la encuesta, los votos llegan
en vivo. Cada teléfono cuenta una vez y puede cambiar su voto mientras la
pregunta sigue abierta. Si vuelves a una encuesta, conserva sus votos. En la
previsualización, la exportación y las capturas se usan los conteos de
`preview` (ceros si no los das), así que puedes diseñar con números creíbles y
el resultado es siempre el mismo.

El código de la sesión es fijo para cada proyecto y el QR es contenido normal
de la escena: se ve igual en la previsualización, en un vídeo exportado o en
un paquete `.gaanim`, y los teléfonos lo escanean una vez por presentación. La
clave que permite abrir preguntas y leer votos se guarda solo en tu equipo.
Para fijar el código, por ejemplo si varias personas presentan el mismo
proyecto, usa la variable `GAANIM_POLL_SESSION`.

Al presentar un paquete `.gaanim` los votos también llegan en vivo, pero el
paquete reproduce lo grabado. Se redibujan con los datos reales las barras de
`poll.bar`, los apodos de la clasificación y las lecturas que muestran
directamente un valor de la encuesta, como
`scene.viz.readout(poll.votes(0))` o `poll.percent(0)`. Lo que pase por un
`computed` con Python muestra los valores de `preview`. El reproductor web
todavía no recibe votos.

== Modo competencia <modo-competencia>

`scene.quiz` es una encuesta con respuesta correcta, al estilo de Kahoot. Los
teléfonos se unen al juego con un apodo, ven una cuenta atrás, responden una
sola vez y una respuesta correcta gana más puntos cuanto antes llega: todos
los puntos al instante y la mitad en el último segundo. `quiz.reveal()` marca
el punto en que la presentación revela la respuesta: cada teléfono muestra si
acertó, cuántos puntos ganó y su puesto.

```python
quiz = scene.quiz("¿Cuál es la derivada de x²?", ["x", "2x", "x²/2", "2"],
                  correct=1, time=20, preview=[3, 17, 5, 2])
clock = scene.viz.readout(quiz.remaining(), format=".0f")   # la cuenta atrás
scene.stop()        # el público responde
quiz.reveal()       # al avanzar, los teléfonos ven el resultado
scene.stop()

board = scene.leaderboard(preview=[("Ana", 2890), ("Beto", 2410), ("Carla", 1995)])
for rank, x in [(1, -4), (0, 0), (2, 4)]:
    board.bar(rank, length=4, thickness=2.6, direction="up").fill(GOLD).move_to(x, -1.2)
    board.name(rank, size=0.55, align="center").move_to(x, 1.9)
    scene.viz.readout(board.points(rank), format=".0f", suffix=" pts").move_to(x, 1.3)
```

Como las encuestas, el cuestionario y la clasificación solo dan datos: el
podio, la lista o las tarjetas los diseñas tú. `quiz.remaining()` cuenta hacia
atrás a lo largo de la línea de tiempo en la previsualización y sigue el reloj
del relay al presentar. `board.name(i)` es texto en vivo con el apodo del
puesto `i`; `board.points(i)` y `board.players()` son `Parameter`.

En la vista del presentador aparece un panel con los teléfonos conectados, los
jugadores y sus puntos, un botón para quitar a un jugador (su teléfono ya no
puede volver a unirse) y otro para empezar el juego de nuevo, que borra votos
y jugadores. Los apodos admiten de 2 a 20 letras, números o espacios y no se
repiten en la sesión.

== El relay

Los votos viajan a través de un *relay*, un pequeño servicio web que despliegas
tú, gratis, en tu propia cuenta de Cloudflare. Como teléfonos y presentación
solo se conectan hacia fuera, por HTTPS, funciona en redes universitarias que
aíslan a los dispositivos entre sí y con datos móviles. La página de votación
está pensada para el teléfono: respuestas grandes con color, letra y forma,
tema claro u oscuro y español o inglés según el teléfono.

```bash
gaanim relay init            # escribe el relay en ./gaanim-relay
cd gaanim-relay
npx wrangler login           # necesita Node.js y una cuenta gratuita
npx wrangler deploy          # imprime https://gaanim-relay.<tú>.workers.dev
gaanim relay use https://gaanim-relay.<tú>.workers.dev
```

`gaanim relay` sin argumentos muestra el relay en uso. Un proyecto puede usar
otro con `[polls] relay` en su
#link("/referencia/gaanim-toml/")[`gaanim.toml`], y la variable
`GAANIM_POLL_RELAY` tiene prioridad sobre ambos. Sin relay, `scene.poll`
avisa y el QR no lleva a ninguna parte. La dirección queda grabada en el QR,
así que si cambias de relay vuelve a exportar.

Los votos son anónimos: el teléfono guarda un identificador al azar y el relay
no pide nombres ni cuentas. El relay borra la sesión y sus votos doce horas
después de su última actividad. Si pierde la conexión, la presentación sigue
y la terminal avisa mientras reintenta.

= Revisar sin pausas

Para revisar una animación de corrido en el editor, activa *Continuous* junto
a los controles de reproducción. El ajuste dura toda la sesión y sobrevive a
las recargas, pero el modo presentación sigue respetando `scene.stop(...)`.
Los saltos, las capturas y la exportación también ignoran las pausas.
