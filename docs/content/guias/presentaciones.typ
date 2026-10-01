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
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
poll = scene.poll("¿Qué curva crece más rápido?", ["x²", "2ˣ", "x log x"], rehearse=[3, 7, 2])

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
previsualización, la exportación y las capturas vota el
#link("#ensayo")[ensayo], un público inventado; `rehearse` le da un peso por
respuesta (`[3, 7, 2]` hace la segunda más popular).

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
`computed` con Python muestra lo que se grabó. El reproductor web
todavía no recibe votos.

== Modo competencia <modo-competencia>

`scene.quiz` es una encuesta con respuesta correcta, al estilo de Kahoot. Los
teléfonos se unen al juego con un apodo, ven una cuenta atrás, responden una
sola vez y una respuesta correcta gana más puntos cuanto antes llega: todos
los puntos al instante y la mitad en el último segundo. `quiz.reveal()` marca
el punto en que la presentación revela la respuesta: cada teléfono muestra si
acertó, cuántos puntos ganó y su puesto. Justo después de un `scene.stop()`,
espera mientras la presentación descansa en la pausa y revela al avanzar.

```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
quiz = scene.quiz("¿Cuál es la derivada de x²?", ["x", "2x", "x²/2", "2"],
                  correct=1, time=20, rehearse=0.7)
clock = scene.viz.readout(quiz.remaining(), format=".0f")   # la cuenta atrás
scene.stop()        # el público responde
quiz.reveal()       # al avanzar, los teléfonos ven el resultado
scene.wait(2)
scene.stop()

board = scene.leaderboard()
for rank, x in [(1, -4), (0, 0), (2, 4)]:
    board.bar(rank, length=4, thickness=2.6, direction="up").fill(GOLD).move_to(x, -1.2)
    board.name(rank, size=0.55, align="center").move_to(x, 1.9)
    scene.viz.readout(board.points(rank), format=".0f", suffix=" pts").move_to(x, 1.3)
```

Como las encuestas, el cuestionario y la clasificación solo dan datos: el
podio, la lista o las tarjetas los diseñas tú. `quiz.remaining()` sigue el
reloj del relay al presentar; en la previsualización llega a 0 en la pausa
donde la presentación espera las respuestas. `board.name(i)` es texto en vivo con el apodo del
puesto `i`; `board.points(i)` y `board.players()` son `Parameter`.

La vista del presentador tiene una sección *Audience* con el código de la
sesión, los teléfonos conectados y los jugadores con sus puntos. Desde ahí
puedes quitar a un jugador (su teléfono ya no puede volver a unirse) o empezar
una partida nueva con *New game*, que borra votos, respuestas y jugadores y
pide una segunda pulsación para confirmar. Pulsar `R` dos veces seguidas hace
lo mismo desde cualquier ventana de la presentación, y
`gaanim relay reset [ruta]` lo hace desde la terminal. Al terminar la
presentación los teléfonos muestran el podio y una despedida, y la siguiente
presentación empieza una partida nueva; si la presentación se cae a mitad de
la charla, al volver a abrirla la partida sigue donde estaba. Los apodos admiten de 2 a 20 letras, números o espacios y no se
repiten en la sesión.

== Avanzar por sí sola <avanzar-sola>

`scene.stop(until=...)` hace que una pausa avance sola cuando el público
cumple una condición: al presentar, en cuanto se cumple, la presentación
sigue como si hubieras pulsado siguiente.

```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
quiz = scene.quiz("¿Cuál es la derivada de x²?", ["x", "2x", "x²/2", "2"], correct=1)
scene.stop(until=quiz.answered(share=0.8) | quiz.time_up())
quiz.reveal()
```

- `poll.answered(at_least=10)`: al menos 10 respuestas.
- `poll.answered(share=0.8)`: respondió el 80 % del público, los jugadores
  en un cuestionario y los teléfonos conectados en una encuesta.
- `quiz.time_up()`: se acabó el tiempo del cuestionario.
- `audience.at_least(5)`: entraron al menos 5 jugadores.

Se combinan con `|` (cualquiera) y `&` (todas). Solo avanza sola una pausa a
la que llegaste avanzando: si vuelves atrás a una cuya condición ya se
cumple, se queda ahí hasta que pulses siguiente. La vista del presentador
muestra cuánto falta, y en la previsualización y la exportación es una pausa
normal. Funciona igual al presentar un paquete `.gaanim`.

== Sala de espera <sala-de-espera>

`scene.audience` da los jugadores en el orden en que entran, para llenar una
sala de espera mientras el público escanea el QR. Una escena que lo usa pide
el apodo en cuanto el teléfono abre la página, en vez de esperar al primer
cuestionario. Como la clasificación, solo da datos: cómo se acomodan y cómo
entran los decides tú.

```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
audience = scene.audience()
audience.qr(3).move_to(-5, 0)
for slot in range(12):
    x, y = (slot % 4) * 2.2 - 1, 1.5 - slot // 4 * 1.5
    pop = computed(lambda a: min(a / 0.4, 1), inputs=[audience.age(slot)])
    audience.name(slot, size=0.4).move_to(x, y).scale_to(pop)
players = scene.viz.readout(audience.count(), format=".0f", suffix=" jugadores")
```

`audience.name(i)` es el apodo de quien entró en el puesto `i`, vacío mientras
nadie lo ocupa. `audience.joined(i)` vale 1 cuando el puesto está ocupado y
`audience.age(i)` cuenta los segundos desde que entró, hasta 60, para animar
su llegada. En la previsualización entran los jugadores del
#link("#ensayo")[ensayo], uno tras otro. Si quitas a un jugador, los
siguientes suben un puesto.

== Ensayo <ensayo>

Fuera de una presentación en vivo, un público inventado juega la escena: en
la previsualización, la exportación y las capturas los jugadores entran a la
sala, responden cada pregunta, el reloj baja y los puntos suben, como en una
sesión real. Todo sale del mismo grupo, así que los números cuadran en todas
partes: los votos de una pregunta suman los jugadores que había, y la
clasificación suma los puntos que ganaron sus respuestas. Las zonas vivas
también lo usan.

```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
scene.rehearsal(24, seed=3, skill=0.65, speed=0.5)

audience = scene.audience()                  # entran uno tras otro
quiz = scene.quiz("¿Capital de Australia?", ["Sídney", "Canberra", "Perth"],
                  correct=1, rehearse=0.4)   # solo el 40 % acierta
poll = scene.poll("¿Te gustó?", ["Sí", "No"], rehearse=[5, 1])
board = scene.leaderboard()                  # puntos de sus respuestas
```

- `players`: cuántos jugadores (se llaman Ana, Beto, Caro…) o una lista de
  apodos, en el orden en que entran. Sin llamar a `scene.rehearsal` ensayan
  12.
- `skill` (0 a 1): cuántas preguntas aciertan en promedio; cada jugador es
  algo mejor o peor que el resto, así que la clasificación se reparte.
- `speed` (0 a 1): qué tan pronto responden.
- `arrive`: en cuántos segundos entran todos; por defecto, entre
  `scene.audience()` y la primera pausa de la sala.
- `seed`: otro grupo, con otros tiempos, respuestas y personajes.

Por pregunta, `rehearse=` en `scene.quiz` es la parte que acierta, y en
`scene.poll` (o en un cuestionario) un peso por respuesta.

El ensayo sigue el ritmo de la escena: las respuestas de una pregunta llegan
entre que se abre y la pausa donde la presentación esperaría por ellas, así
que la previsualización es una versión acelerada de lo que pasará en vivo. Con
los mismos argumentos el ensayo es siempre el mismo, en cada vista previa y
en cada exportación.

== Equipos <equipos>

`scene.teams` hace que el juego se juegue por equipos, por ejemplo una
batalla entre dos grupos del curso. Cada jugador entra a un equipo: el relay
lo pone en el que tiene menos jugadores, o con `choose=True` el teléfono le
pregunta a cuál quiere unirse (hasta que responda su primera pregunta puede
cambiar). El teléfono muestra su equipo con su color y, al final, qué equipo
ganó. Los puntos de cada jugador suman para su equipo.

```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
teams = scene.teams(["Rojo", "Azul"], colors=["#ff4f8b", "#2fb8ff"])
scene.rehearsal(20, skill=[0.7, 0.5])     # un ensayo con un equipo más fuerte

for team, x in [(0, -4), (1, 4)]:
    scene.text(teams.names[team], size=0.6).move_to(x, 3)
    scene.viz.readout(teams.score(team), format=".0f", suffix=" pts").move_to(x, 2)
    teams.bar(team, length=5, direction="up").fill(teams.colors[team]).move_to(x, -1)
```

Como el resto, solo da datos y la batalla la diseñas tú:

- `teams.score(i)`, `teams.players(i)` y `teams.average(i)` (puntos por
  jugador, más justo si los equipos quedan desparejos) son `Parameter`.
- `teams.leader()` es el índice del equipo que va ganando.
- `teams.bar(i, ...)` es una barra que mide los puntos del equipo frente al
  que va primero.
- En una zona viva, cada jugador sabe su equipo: `p.team`, su orden de
  llegada dentro del equipo `p.team_index`, cuántos son `p.team_count`, los
  puntos del equipo `p.team_score` y su puesto `p.team_rank` (0 para el que
  va ganando). Sin equipos, todos están en el equipo 0.

```python
def battle(p):
    side = -1 if p.team == 0 else 1         # cada equipo en su lado
    x = side * (2 + p.team_index // 4 * 1.2)
    y = -2 + p.team_index % 4 * 1.1
    if p.team_rank == 0:                      # el que va ganando salta
        y += 0.5 * abs(math.sin(3 * p.t))
    return pose(x, y, flip=side > 0, express="winner" if p.team_rank == 0 else None, loop=True)
```

El #link("#ensayo")[ensayo] reparte a sus jugadores igual que el relay, y
`skill` acepta un valor por equipo para ensayar una batalla despareja.

== Tipos de pregunta <tipos-de-pregunta>

Además de una sola respuesta, una pregunta puede llevar una imagen y aceptar
varias respuestas:

```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
# no-run: necesita una imagen del proyecto
quiz = scene.quiz("¿De qué país es esta bandera?", ["Chile", "Perú", "Canadá"],
                  correct=1, image="imagenes/bandera.png")
paises = scene.quiz("¿Qué países están en Sudamérica?", ["Perú", "Chile", "México", "Brasil"],
                    correct=[0, 1, 3])          # selección múltiple
gustos = scene.poll("¿Qué te gustó?", ["Mapas", "Banderas", "El juego"], multiple=True)
```

- `image=` muestra la imagen sobre la pregunta en los teléfonos, reducida a
  1024 píxeles. En la escena la pones tú, con `scene.media.image`.
- Con `correct=[...]` el cuestionario es de selección múltiple: el teléfono
  deja marcar varias respuestas y enviarlas, y solo suma puntos quien marcó
  todas las correctas y ninguna otra.
- `multiple=True` en una encuesta deja votar por varias. `votes(i)` cuenta
  cada respuesta elegida, y `total()` y `share(i)` cuentan a los teléfonos que
  respondieron.
- `poll.icon(i, tamaño)` es la forma que tiene la respuesta `i` en el teléfono
  (triángulo, rombo, círculo, cuadrado, estrella, hexágono), con su color, y
  `poll.color(i)` el color. En un cuestionario los teléfonos ordenan y colorean
  las respuestas a su manera para que no se copien, así que ahí la forma de la
  pantalla no coincide con la del teléfono.

== Preguntas en un archivo <preguntas-en-un-archivo>

Las preguntas se pueden escribir fuera de Python, en un Markdown o en una hoja
de cálculo, y abrirlas con `scene.question`:

```markdown
# ¿De qué país es esta bandera?
![](imagenes/bandera.png)
tiempo: 15

- Chile
- [x] Perú
- Canadá

# ¿Qué países están en Sudamérica?

- [x] Perú
- [x] Chile
- México
- [x] Brasil

# ¿Qué te gustó más?
modo: varias

- Los mapas
- Las banderas
```

Cada `#` empieza una pregunta y cada elemento de la lista es una respuesta;
`[x]` marca las correctas (varias la hacen de selección múltiple) y sin
ninguna es una encuesta. Las líneas `clave: valor` ajustan `tiempo`, `puntos`,
`modo` (`una` o `varias`), `imagen`, `acierta` (la parte que acierta en el
ensayo, como `70%`), `votos` (pesos del ensayo) y `notas`. En una hoja de
cálculo (`.csv`) va una pregunta por fila con las columnas `pregunta`,
`respuesta 1` a `respuesta 6` y `correcta` (los números de las correctas
desde 1, como `2` o `1, 3`), y opcionalmente las mismas claves; la plantilla de
Kahoot sirve tal cual.

```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
# no-run: necesita los archivos de preguntas del proyecto
from gaanim import load_questions

for q in load_questions("preguntas.md"):
    scene.segment(q.text)
    poll = scene.question(q)                  # cuestionario o encuesta
    scene.text(q.text, size=0.6).move_to(0, 3.5)
    for i, respuesta in enumerate(q.options):
        poll.icon(i, 0.5).move_to(-6, 2 - i * 1.2)
        scene.text(respuesta, size=0.45).move_to(-3.5, 2 - i * 1.2)
    scene.stop()
    if q.is_quiz:
        poll.reveal()
```

`load_questions` devuelve objetos `Question` con `text`, `options`, `correct`,
`multiple`, `time`, `points`, `image`, `rehearse` y `notes`, así que el diseño
de cada diapositiva sigue siendo tuyo.

== Las respuestas de cada jugador <respuestas-por-jugador>

Además de los totales, puedes saber qué respondió cada jugador, por su puesto
en la sala (`audience`) o en la clasificación (`board`):

```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
quiz = scene.quiz("¿2 + 2?", ["3", "4"], correct=1)
audience = scene.audience()
board = scene.leaderboard()
eligio = audience.chose(0, quiz, 1)     # 1 si el primero en llegar eligió la 2.ª
acerto = audience.right(0, quiz)        # 1 si acertó
tardo = audience.answer_time(0, quiz)   # segundos que tardó
racha = board.streak(0)                 # aciertos seguidos del primero
```

`responded`, `chose`, `right`, `earned` y `answer_time` hablan de una pregunta;
`correct`, `answered` y `streak` (y `audience.score`) del juego hasta ahora.
Todos son `Parameter`. En las zonas vivas cada personaje lo sabe de sí mismo:
`p.answer`, `p.answer_mask`, `p.answer_time`, `p.answer_points`, `p.answers`,
`p.correct` y `p.streak` (ver #link("/guias/zonas-vivas/")[Zonas vivas]).

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
no pide nombres ni cuentas. Lo que cada teléfono votó lo recuerda el relay, no
el teléfono, así que una partida nueva empieza limpia en todos. El relay borra la sesión y sus votos doce horas
después de su última actividad. Si pierde la conexión, la presentación sigue
y la terminal avisa mientras reintenta.

= Revisar sin pausas

Para revisar una animación de corrido en el editor, activa *Continuous* junto
a los controles de reproducción. El ajuste dura toda la sesión y sobrevive a
las recargas, pero el modo presentación sigue respetando `scene.stop(...)`.
Los saltos, las capturas y la exportación también ignoran las pausas.
