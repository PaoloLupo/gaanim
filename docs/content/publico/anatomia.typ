#import "../../components/section.typ": docs-chapter

#show: docs-chapter.with(
  title: "Anatomía de un juego",
  description: "Cuestionarios, revelación, clasificación, sala de espera, equipos y el ensayo",
  route: "/publico/anatomia/",
)

En esta guía verás las piezas con las que se arma un juego: una sala de
espera mientras el público entra, preguntas con cuenta atrás, el momento en
que revelas la respuesta, la clasificación y los equipos. Cada pieza te da
datos y momentos; cómo se ven lo decides tú.

Un juego típico sigue este ritmo, una diapositiva por paso:

+ *Sala de espera*: el QR y los apodos de quienes van entrando
  (`scene.audience`).
+ *Pregunta*: `scene.quiz` abre la pregunta y el reloj empieza a correr.
+ *Pausa*: `scene.stop(until=...)` espera a que respondan, o a que se acabe
  el tiempo, y avanza sola.
+ *Revelación*: `quiz.reveal()` muestra en cada teléfono si acertó y cuántos
  puntos ganó; en la pantalla, tú resaltas la correcta.
+ *Clasificación*: `scene.leaderboard` ordena a los jugadores para un podio,
  una carrera o una lista.

= Cuestionarios <cuestionarios>

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

`quiz.remaining()` sigue el reloj del relay al presentar; en la
previsualización llega a 0 en la pausa donde la presentación espera las
respuestas. `quiz.correct` es el índice de la respuesta correcta (o la lista,
en selección múltiple), para que resaltes la ficha que corresponde.

= La clasificación <clasificacion>

`scene.leaderboard()` da a los jugadores de mejor a peor. Cada método recibe
un puesto, 0 para el primero:

- `board.name(i)`: el apodo como texto en vivo, con `size`, `weight`, `font`
  y `align` para alinearlo por la izquierda, el centro o la derecha.
- `board.points(i)`, `board.correct(i)`, `board.answered(i)`,
  `board.streak(i)` (aciertos seguidos) y `board.players()`: `Parameter`.
- `board.bar(i, ...)`: una barra que mide los puntos del puesto `i` frente a
  los del primero.

Un puesto sin jugador muestra un apodo vacío, 0 puntos y una barra vacía, así
que puedes dibujar un podio de tres aunque jueguen dos. Todo sigue a los
resultados en vivo, también al presentar un `.gaanim`.

= Avanzar por sí sola <avanzar-sola>

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

= Sala de espera <sala-de-espera>

`scene.audience` da los jugadores en el orden en que entran, para llenar una
sala de espera mientras el público escanea el QR. Una escena que lo usa pide
el apodo en cuanto el teléfono abre la página, en vez de esperar al primer
cuestionario.

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
scene.stop("sala", until=audience.at_least(5))
```

`audience.name(i)` es el apodo de quien entró en el puesto `i`, vacío mientras
nadie lo ocupa. `audience.joined(i)` vale 1 cuando el puesto está ocupado y
`audience.age(i)` cuenta los segundos desde que entró, hasta 60, para animar
su llegada. Si quitas a un jugador, los siguientes suben un puesto. Para que
entren como personajes que corren, saltan o se estrellan contra la pantalla,
usa una #link("/publico/zonas-vivas/")[zona viva].

= Equipos <equipos>

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
>>>import math
>>>from gaanim.live import pose
def battle(p):
    side = -1 if p.team == 0 else 1         # cada equipo en su lado
    x = side * (2 + p.team_index // 4 * 1.2)
    y = -2 + p.team_index % 4 * 1.1
    if p.team_rank == 0:                      # el que va ganando salta
        y += 0.5 * abs(math.sin(3 * p.t))
    return pose(x, y, flip=side > 0, express="winner" if p.team_rank == 0 else None, loop=True)
```

= Las respuestas de cada jugador <respuestas-por-jugador>

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
`p.correct` y `p.streak`.

= Ensayo <ensayo>

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
  algo mejor o peor que el resto, así que la clasificación se reparte. Con
  equipos, acepta un valor por equipo para ensayar una batalla despareja.
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
en cada exportación. Diseña con un ensayo parecido a tu público: con
`scene.rehearsal(40)` verás si tu sala de espera aguanta un curso grande.
