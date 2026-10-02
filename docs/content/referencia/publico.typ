#import "../../components/section.typ": docs-chapter
#import "../../components/api.typ": api-entry

#show: docs-chapter.with(
  title: "Público",
  description: "Poll, Leaderboard, Teams, Audience, Character, LiveZone, Condition, gaanim.live y las preguntas",
  route: "/referencia/publico/",
)

= Público

Los objetos que dan a la escena lo que hace el público: las encuestas y
cuestionarios, la clasificación, los equipos, la sala, los personajes y las
zonas vivas. Ninguno dibuja nada por sí mismo: dan `Parameter`, textos en
vivo, barras y condiciones para que diseñes el juego. Fuera de una
presentación en vivo siguen al ensayo (`scene.rehearsal`); al presentar, al
relay.

Los métodos de `Scene` que los crean (`poll`, `quiz`, `question`,
`leaderboard`, `teams`, `audience`, `rehearsal`, `character` y `live_zone`)
están en #link("/referencia/scene/")[Escena]. Las guías están en
#link("/publico/")[Público en vivo].

== Encuestas y cuestionarios

#api-entry(
  name: "Poll",
  kind: "class",
  desc: [Una encuesta de `scene.poll` o un cuestionario de `scene.quiz`, o lo que abre `scene.question`. Sus valores siguen los votos en vivo al presentar y el ensayo en todo lo demás. Al presentar un paquete `.gaanim`, las barras y las lecturas que muestran directamente uno de sus `Parameter` siguen los votos en vivo; lo que pasa por un `computed` muestra lo grabado.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
poll = scene.poll("¿Té o café?", ["Té", "Café"], rehearse=[2, 3])
scene.viz.readout(poll.percent(1), format=".0f", suffix=" % café")
```
]

#api-entry(
  name: "Poll.id / question / options",
  kind: "property",
  desc: [El identificador estable de la pregunta en el relay, el texto de la pregunta y sus respuestas, en el orden dado.],
  none,
)

#api-entry(
  name: "Poll.code / url / qr",
  kind: "method",
  desc: [`code` es el código de seis caracteres de la sesión y `url` la dirección que abre el QR. `qr(size=3.0)` devuelve el QR como un drawable de `size` unidades de lado, relleno de negro: ponlo sobre un fondo claro con margen. Un tamaño no positivo lanza `ValueError`.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
>>>poll = scene.poll("¿Té o café?", ["Té", "Café"])
scene.geometry.rounded_rect(3.4, 3.4, 0.2).fill(WHITE).no_stroke().move_to(-5, 0)
poll.qr(3.0).move_to(-5, 0)
scene.text(poll.code, size=0.5).move_to(-5, -2.2)
```
]

#api-entry(
  name: "Poll.votes / share / percent / total",
  kind: "method",
  desc: [`Parameter` que siguen los votos: `votes(i)` los de la respuesta `i` (0 para la primera), `share(i)` su fracción de 0 a 1, `percent(i)` lo mismo de 0 a 100 y `total()` todos los votos. En un cuestionario `total()` cuenta a los jugadores que respondieron; en una encuesta de selección múltiple, `votes(i)` cuenta cada respuesta elegida y `total()` y `share(i)` cuentan teléfonos. Sin votos, `share` y `percent` valen 0. Una respuesta que la pregunta no tiene lanza `ValueError`.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
>>>poll = scene.poll("¿Té o café?", ["Té", "Café"])
scene.viz.readout(poll.votes(0), format=".0f", prefix="Té: ")
scene.viz.readout(poll.total(), format=".0f", suffix=" votos").move_to(0, -1)
```
]

#api-entry(
  name: "Poll.bar",
  kind: "method",
  desc: [Una barra cuya longitud sigue los votos de `answer`. Sus límites son la caja completa de `length` por `thickness`, centrada en su posición, así que el layout no se mueve al llegar votos; crece desde el borde opuesto a `direction` (`"right"`, `"left"`, `"up"`, `"down"`). Con `scale="leader"` la respuesta que va ganando llena su barra; con `"total"` cada barra mide su parte del total. Se estiliza y anima como cualquier drawable (`create` la hace crecer) y sigue los votos en vivo también en un `.gaanim`. Una respuesta, dirección o escala desconocida, una longitud o grosor no positivos o un radio negativo lanzan `ValueError`.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
>>>poll = scene.poll("¿Té o café?", ["Té", "Café"])
poll.bar(0, length=5, thickness=0.4, radius=0.2).fill(RED).no_stroke().move_to(1, 1)
```
]

#api-entry(
  name: "Poll.icon / color",
  kind: "method",
  desc: [`icon(answer, size=0.6)` es la forma que la respuesta tiene en los teléfonos (triángulo, rombo, círculo, cuadrado, estrella o hexágono), de `size` unidades de alto, centrada y rellena de su color. `color(answer)` es ese color, `#rrggbb`. En un cuestionario los teléfonos ordenan y colorean las respuestas a su manera, así que solo en una encuesta la pantalla coincide con el teléfono. Una respuesta desconocida o un tamaño no positivo lanzan `ValueError`.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
>>>poll = scene.poll("¿Té o café?", ["Té", "Café"])
for i, answer in enumerate(poll.options):
    scene.geometry.rounded_rect(6, 1.4, 0.3).fill(poll.color(i)).no_stroke().move_to(0, 1 - 1.8 * i)
    poll.icon(i, 0.6).fill(WHITE).move_to(-2.2, 1 - 1.8 * i)
```
]

#api-entry(
  name: "Poll.answered / time_up",
  kind: "method",
  desc: [Condiciones para `scene.stop(until=...)`. `answered(at_least=n)` se cumple con al menos `n` respuestas; `answered(share=f)` cuando respondió esa parte del público: los jugadores en un cuestionario y los teléfonos conectados en una encuesta; nunca se cumple sin nadie. `time_up()` se cumple cuando se acaba el tiempo del cuestionario en el reloj del relay. `answered` sin exactamente uno de sus argumentos o con una parte fuera de (0, 1\], y `time_up` en una encuesta, lanzan `ValueError`.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
quiz = scene.quiz("¿2 + 2?", ["3", "4"], correct=1)
scene.stop(until=quiz.answered(share=0.9) | quiz.time_up())
```
]

#api-entry(
  name: "Poll.remaining",
  kind: "method",
  desc: [Los segundos que quedan para responder un cuestionario, como `Parameter`. En la previsualización y la exportación baja con el ensayo y llega a 0 en la pausa donde la presentación espera las respuestas; al presentar sigue el reloj del relay. En una encuesta lanza `ValueError`.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
>>>quiz = scene.quiz("¿2 + 2?", ["3", "4"], correct=1)
clock = scene.viz.readout(quiz.remaining(), format=".0f", suffix=" s").move_to(6, 3.5)
```
]

#api-entry(
  name: "Poll.reveal",
  kind: "method",
  desc: [Revela la respuesta de un cuestionario en el cursor: cuando la presentación pasa por este punto, cada teléfono muestra la respuesta correcta y al jugador si acertó, los puntos que ganó y su puesto; el cuestionario ya no acepta respuestas. Escrito justo después de un `scene.stop()`, espera mientras la presentación descansa en la pausa y revela al avanzar. En una encuesta, o una segunda vez, lanza `ValueError`.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
quiz = scene.quiz("¿2 + 2?", ["3", "4"], correct=1)
scene.stop()
quiz.reveal()
```
]

#api-entry(
  name: "Poll.revealed",
  kind: "method",
  returns: (type: "Parameter", desc: []),
  desc: [Vale 0 hasta que el cuestionario revela su respuesta y 1 desde entonces. Mostrar barras o porcentajes durante la pregunta influye en las respuestas: con este valor la escena decide qué hacer hasta el `reveal()`, ocultarlos, taparlos o mostrar «?». El cambio es un momento de la línea de tiempo, igual en la vista previa, en la exportación y en vivo, donde la presentación revela al llegar a él. Se pide antes del `reveal()`; después, o en una encuesta, lanza `ValueError`.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
quiz = scene.quiz("¿2 + 2?", ["3", "4"], correct=1)
shown = quiz.revealed()
for i in range(2):
    scene.viz.readout(quiz.percent(i), format=".0f", suffix="%").move_to(4 * i - 2, 0).opacity(shown)
scene.wait(2)
scene.stop()
quiz.reveal()
```
]

#api-entry(
  name: "Poll.close",
  kind: "method",
  desc: [Deja de recibir votos en el cursor, en vez de al final del segmento donde se abrió. Los valores conservan el último conteo. Si ya estaba cerrada lanza `ValueError`.],
  none,
)

#api-entry(
  name: "Poll.is_quiz / correct / multiple / time / has_image",
  kind: "property",
  desc: [`is_quiz` dice si es un cuestionario; `correct` es el índice de la respuesta correcta, la lista de ellas en selección múltiple o `None` en una encuesta; `multiple` si se pueden elegir varias; `time` los segundos de un cuestionario (`None` en una encuesta); `has_image` si los teléfonos muestran una imagen sobre la pregunta.],
  none,
)

== Clasificación

#api-entry(
  name: "Leaderboard",
  kind: "class",
  desc: [La clasificación de `scene.leaderboard()`: los jugadores de los cuestionarios, de mejor a peor. Cada método recibe un puesto, 0 para el primero. Un puesto sin jugador muestra un apodo vacío, 0 puntos y una barra vacía. Todo sigue a los resultados en vivo, también en un `.gaanim`, que guarda los glifos de los apodos.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
board = scene.leaderboard()
for rank in range(3):
    board.name(rank, size=0.5).move_to(-3, 1 - rank)
    scene.viz.readout(board.points(rank), format=".0f").move_to(3, 1 - rank)
```
]

#api-entry(
  name: "Leaderboard.name",
  kind: "method",
  desc: [El apodo del puesto `rank` como texto en vivo. `size` y `font` toman el texto del tema por defecto; `align` coloca el borde izquierdo, el centro o el borde derecho del texto en la posición del drawable, para que apodos de cualquier largo queden alineados. Se rellena con el color de primer plano del tema; cámbialo con `fill`. Un `align` desconocido o un tamaño no positivo lanzan `ValueError`.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
>>>board = scene.leaderboard()
board.name(0, size=0.6, align="left").fill(GOLD).move_to(-3, 1)
```
]

#api-entry(
  name: "Leaderboard.points / players / correct / answered / streak",
  kind: "method",
  desc: [`Parameter` del juego hasta ahora: `points(rank)` los puntos del puesto, `correct(rank)` sus aciertos, `answered(rank)` las preguntas que respondió, `streak(rank)` sus aciertos seguidos hasta la última revelada y `players()` cuántos jugadores hay.],
  none,
)

#api-entry(
  name: "Leaderboard.responded / chose / right / earned / answer_time",
  kind: "method",
  desc: [`Parameter` de una pregunta para el jugador del puesto `rank`: `responded(rank, poll)` vale 1 cuando respondió; `chose(rank, poll, answer)` 1 si eligió esa respuesta (también en selección múltiple); `right(rank, quiz)` 1 si acertó; `earned(rank, quiz)` los puntos que ganó y `answer_time(rank, quiz)` los segundos que tardó, 0 sin respuesta. `right` en una encuesta lanza `ValueError`.],
  none,
)

#api-entry(
  name: "Leaderboard.bar",
  kind: "method",
  desc: [Una barra que mide los puntos del puesto `rank` frente a los del primero. Sus límites son la caja completa y crece desde el borde opuesto a `direction`, como `Poll.bar`, con los mismos errores.],
  none,
)

== Equipos

#api-entry(
  name: "Teams",
  kind: "class",
  desc: [Los equipos de `scene.teams(...)`. Cada método recibe el índice de un equipo, en el orden de los nombres. Fuera de una presentación siguen al ensayo, que reparte a sus jugadores igual que el relay. En una zona viva cada jugador sabe `p.team`, `p.team_index`, `p.team_count`, `p.team_score` y `p.team_rank`.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
teams = scene.teams(["Rojo", "Azul"], colors=["#ff4f8b", "#2fb8ff"])
for team, x in [(0, -4), (1, 4)]:
    scene.text(teams.names[team], size=0.6).fill(teams.colors[team]).move_to(x, 2)
    scene.viz.readout(teams.score(team), format=".0f", suffix=" pts").move_to(x, 1)
```
]

#api-entry(
  name: "Teams.names / colors / choose",
  kind: "property",
  desc: [Los nombres de los equipos, sus colores `#rrggbb` como los muestran los teléfonos, y si cada jugador elige su equipo en el teléfono. `len(teams)` es cuántos equipos hay.],
  none,
)

#api-entry(
  name: "Teams.score / players / average / leader",
  kind: "method",
  desc: [`Parameter` de cada equipo: `score(team)` suma los puntos de sus jugadores, `players(team)` cuenta sus jugadores y `average(team)` da los puntos por jugador (0 sin jugadores), más justo cuando los equipos quedan desparejos. `leader()` es el índice del equipo que va ganando, el primero en un empate. Un equipo que el juego no tiene lanza `ValueError`.],
  none,
)

#api-entry(
  name: "Teams.bar",
  kind: "method",
  desc: [Una barra que mide los puntos del equipo frente a los del que va ganando, como `Poll.bar`, con los mismos errores.],
  none,
)

== Sala

#api-entry(
  name: "Audience",
  kind: "class",
  desc: [El público de `scene.audience()`: los jugadores en el orden en que entraron, cada uno en un puesto, 0 para el primero. Un puesto vacío muestra un apodo vacío y `joined` y `age` en 0. Si quitas a un jugador, los siguientes suben un puesto. Una escena que lo usa pide el apodo en cuanto el teléfono abre la página.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
audience = scene.audience()
for slot in range(8):
    audience.name(slot, size=0.4).move_to(-6 + slot * 1.7, -2)
scene.viz.readout(audience.count(), format=".0f", suffix=" en la sala")
```
]

#api-entry(
  name: "Audience.code / url / qr",
  kind: "method",
  desc: [El código de la sesión, la dirección para unirse y `qr(size=3.0)`, el QR de esa dirección, como en `Poll`.],
  none,
)

#api-entry(
  name: "Audience.name",
  kind: "method",
  desc: [El apodo del puesto `slot` como texto en vivo, centrado por defecto; acepta `size`, `weight`, `font` y `align` como `Leaderboard.name`.],
  none,
)

#api-entry(
  name: "Audience.count / joined / age / at_least",
  kind: "method",
  desc: [`count()` cuántos jugadores entraron; `joined(slot)` vale 1 cuando el puesto está ocupado; `age(slot)` cuenta los segundos desde que entró, hasta 60, para animar su llegada. `at_least(n)` es una condición para `scene.stop(until=...)` que se cumple con al menos `n` jugadores.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
>>>audience = scene.audience()
pop = computed(lambda a: min(a / 0.4, 1), inputs=[audience.age(0)])
audience.name(0, size=0.5).scale_to(pop)
scene.stop("sala", until=audience.at_least(5))
```
]

#api-entry(
  name: "Audience.score / correct / answered / streak",
  kind: "method",
  desc: [Los mismos datos del juego que `Leaderboard`, para el jugador de un puesto de la sala en vez de uno de la clasificación.],
  none,
)

#api-entry(
  name: "Audience.responded / chose / right / earned / answer_time",
  kind: "method",
  desc: [Los mismos datos de una pregunta que `Leaderboard`, para el jugador de un puesto de la sala.],
  none,
)

== Personajes y zonas vivas

#api-entry(
  name: "Character",
  kind: "class",
  desc: [Un personaje como los que arma el público, de `scene.character(...)`: respira, parpadea y hace expresiones igual que en el teléfono. Se coloca, escala y anima como cualquier drawable.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
hero = scene.character([1, 0, 4, 2, 1], name="Ana").move_to(-3, 0)
hero.express("winner", loop=True)
scene.wait(2)
```
]

#api-entry(
  name: "Character.express / expressions / avatar",
  kind: "method",
  desc: [`express(expression=None, loop=False)` hace una expresión desde el cursor: una vez, o con `loop=True` hasta la siguiente; `None` vuelve a su cara. Las expresiones, que da `Character.expressions()`, son `happy`, `sad`, `hurt`, `winner` y `surprised`; los lentes se quedan puestos. `avatar` son sus partes: cuerpo, color, ojos, boca y extra. Una expresión desconocida lanza `ValueError`.],
  none,
)

#api-entry(
  name: "LiveZone",
  kind: "class",
  desc: [Una parte de la escena donde juega el público, de `scene.live_zone(...)`. Al presentar, cada jugador llega como su personaje y el comportamiento lo coloca en cada cuadro; la vista previa y la exportación reproducen el ensayo, siempre igual. El comportamiento se compila dentro de la escena, así que un `.gaanim` lo ejecuta sin Python. Los personajes se dibujan sobre el resto, recortados a los límites de la zona.],
  none,
)

#api-entry(
  name: "LiveZone.close / instructions",
  kind: "method",
  desc: [`close()` cierra la zona en el cursor en vez de al final del segmento; si ya estaba cerrada lanza `ValueError`. `instructions` es cuántas instrucciones ejecuta el comportamiento compilado por jugador.],
  none,
)

#api-entry(
  name: "Condition",
  kind: "class",
  desc: [Lo que el público debe hacer para que una pausa avance sola con `scene.stop(until=...)`. Se crea con `poll.answered(at_least=)` o `poll.answered(share=)`, `quiz.time_up()` y `audience.at_least(n)`, y se combina con `|` (cualquiera) y `&` (todas). Ver #link("/publico/anatomia/#avanzar-sola")[Avanzar por sí sola].],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
quiz = scene.quiz("¿2 + 2?", ["3", "4"], correct=1)
scene.stop(until=quiz.answered(share=0.8) | quiz.time_up())
```
]

== gaanim.live <gaanim-live>

El módulo `gaanim.live` tiene lo que usan los comportamientos de una zona
viva. Ver la guía #link("/publico/zonas-vivas/")[Zonas vivas] para el
subconjunto de Python que se compila.

=== pose

`pose(x, y, *, rotation=0, scale=1, sx=1, sy=1, lean=0, look_x=None,
look_y=None, show_name=True, flip=False, visible=True, express=None,
since=None, loop=False)` dice dónde está un personaje y cómo se ve.

#table(
  columns: (auto, 1fr),
  [*Argumento*], [*Qué hace*],
  [`x`, `y`], [Dónde están sus pies, en unidades de la escena.],
  [`rotation`], [Lo gira sobre su centro, en radianes, en sentido antihorario.],
  [`scale`], [Multiplica el tamaño de la zona.],
  [`sx`, `sy`], [Lo estiran a lo ancho y a lo largo desde los pies, para aplastar y estirar (`impact` y `anticipate` los dan).],
  [`lean`], [Lo inclina desde los pies.],
  [`look_x`, `look_y`], [Hacia dónde miran sus ojos, de -1 a 1 (derecha y arriba); sin ellos mira hacia donde se mueve.],
  [`show_name`], [`False` oculta el apodo en una zona que los dibuja.],
  [`flip`], [Lo refleja de izquierda a derecha.],
  [`visible`], [`False` lo oculta.],
  [`express`], [Una expresión: `"happy"`, `"sad"`, `"hurt"`, `"winner"` o `"surprised"`.],
  [`loop`], [Repite la expresión hasta otra.],
  [`since`], [El `p.t` en que empezó la expresión, para las que siguen a un momento que calcula el comportamiento, como aterrizar; sin él, empieza cuando aparece.],
)

=== El jugador

El comportamiento recibe al jugador `p` con estos campos:

#table(
  columns: (auto, 1fr),
  [*Campo*], [*Qué es*],
  [`t`, `time`], [Segundos desde que llegó a la zona, y desde que se abrió.],
  [`joined`, `index`, `count`], [Cuándo llegó, su orden de llegada desde 0 y cuántos hay.],
  [`rank`, `score`, `leader`], [Su puesto (0 el primero), sus puntos y los del primero.],
  [`previous_rank`, `rank_since`], [Su puesto anterior y los segundos desde que cambió.],
  [`previous_score`, `score_since`], [Sus puntos anteriores y los segundos desde que cambiaron.],
  [`team`, `team_index`, `team_count`], [Su equipo, su orden de llegada en él y cuántos son.],
  [`team_score`, `team_rank`], [Los puntos de su equipo y su puesto (0 el que va ganando).],
  [`answer`, `answer_mask`], [La respuesta que eligió en la pregunta abierta o la última (-1 si ninguna) y todas las que eligió, un bit cada una.],
  [`answer_time`, `answer_points`], [Lo que tardó en responder y lo que ganó.],
  [`answers`, `correct`, `streak`], [Preguntas respondidas, acertadas y aciertos seguidos en el juego.],
  [`random(k)`], [Un número entre 0 y 1 que depende solo del jugador y de `k`.],
  [`state.<nombre>`], [Los números que la zona guarda para él.],
)

`gaanim.live.Player` crea un jugador de muestra con esos campos, para probar
un comportamiento en Python: `fila(Player(index=2, t=0.5))`.

=== Estado y ayudas

- `state(**valores)`: los siguientes números guardados, como los devuelve una
  función `update`; los que no nombra conservan su valor.
- `STEP`: cada cuánto corre `update`, 1/60 s. `MAX_STATE`: cuántos números
  puede guardar una zona, 8.
- `clamp(x, low=0, high=1)`, `lerp(a, b, u)` y
  `progress(t, start=0, duration=1)`.
- Curvas de 0 a 1: `smoothstep(u)`, `ease_in(u)`, `ease_out(u)`,
  `ease_in_out(u)` y `ease_out_back(u, overshoot=1.7)`, que se pasa y vuelve.
- Movimiento: `spring(t, frequency=2, damping=0.3)`, un resorte de 0 a 1;
  `wobble(t, amount=1, frequency=3, decay=4)`, una sacudida que se apaga;
  `impact(t, amount=0.35, ...)` y `anticipate(t, at, amount=0.25,
  duration=0.3)`, que devuelven `(sx, sy)` para aplastar al aterrizar y
  agacharse antes de saltar; `ballistic(t, x, y, vx, vy, gravity=9.8)`, la
  posición de un tiro parabólico, y `landing(vy, drop, gravity=9.8)`, cuándo
  cae.
- `BehaviorError`: lo que lanza `scene.live_zone` cuando el comportamiento usa
  algo que no se compila, señalando la línea.

== Preguntas en archivos <preguntas>

`gaanim.load_questions(path)` lee las preguntas de un `.md` (o `.txt`) o un
`.csv`, con la ruta relativa al script que lo llama, y devuelve una lista de
`Question`. Un archivo con una pregunta sin respuestas, más de 6 respuestas,
una clave desconocida o un valor que no es un número lanza `QuestionError`
con el archivo y la línea. El formato está en
#link("/publico/preguntas/#preguntas-en-un-archivo")[Preguntas en un archivo].

#table(
  columns: (auto, 1fr),
  [*Campo de `Question`*], [*Qué es*],
  [`text`, `options`], [La pregunta y sus respuestas.],
  [`correct`], [Los índices de las correctas, desde 0; vacío en una encuesta.],
  [`is_quiz`, `multiple`], [Si es un cuestionario y si acepta varias respuestas.],
  [`time`, `points`], [Segundos para responder (20) y puntos máximos (1000).],
  [`image`], [La ruta absoluta de la imagen, o `None`.],
  [`rehearse`], [Cómo responde el ensayo: la parte que acierta o un peso por respuesta.],
  [`notes`], [Las notas del orador.],
)

`scene.question(q)` abre la pregunta como cuestionario o encuesta según tenga
respuestas correctas.
