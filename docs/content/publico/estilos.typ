#import "../../components/section.typ": docs-chapter

#show: docs-chapter.with(
  title: "Recetario de estilos",
  description: "Escenas completas para copiar: fichas, encuestas, sala de espera, carreras, batallas y podios",
  route: "/publico/estilos/",
)

Cada receta es una escena completa que puedes guardar como `main.py`,
presentar tal cual y cambiar a tu gusto. Todas usan el
#link("/publico/anatomia/#ensayo")[ensayo] para que la vista previa muestre
un juego en marcha; al presentar, los mismos dibujos siguen al público real.

Las recetas se combinan: una trivia completa puede empezar con la sala de
espera, seguir con fichas de colores en cada pregunta, pasar por una batalla
de equipos y terminar en el podio. Usa un segmento por diapositiva
(`scene.segment`) y una pausa donde el presentador hable.

= Fichas de colores <fichas>

El estilo clásico: cuatro fichas con el color y la forma de cada respuesta en
el teléfono, una cuenta atrás y una pausa que avanza sola cuando todos
respondieron. Al revelar, las fichas se encogen a una fila con
`magic_move`, aparecen los votos y la correcta brilla.

```python
# output: preview.webp
# show-code: true
from gaanim import Scene, magic_move

INK, WHITE, GOLD = "#1c0b3f", "#fff8ff", "#ffd23f"
scene = Scene(frame=(16, 9), background=INK)
scene.rehearsal(16, seed=4)
quiz = scene.quiz("¿Qué planeta es el más caliente?", ["Mercurio", "Venus", "Marte", "Júpiter"],
                  correct=1, time=20, rehearse=0.45)


def tile(i, width, height, size):
    """Una respuesta con su color y su forma, con nombre para magic_move."""
    box = scene.geometry.rounded_rect(width, height, height / 4).fill(quiz.color(i)).no_stroke()
    x = -width / 2 + height / 2
    badge = scene.geometry.circle(height * 0.32).fill(WHITE).no_stroke().move_to(x, 0)
    icon = quiz.icon(i, height * 0.36).move_to(x, 0)
    label = scene.text(quiz.options[i], size=size, color=WHITE, weight=700, text_box="cap")
    label.move_to(height * 0.35, 0)
    return scene.geometry.group([box, badge, icon, label]).named(f"respuesta{i}")


title = scene.text(quiz.question, size=0.6, color=WHITE, weight=700).move_to(0, 3.4)
clock = scene.viz.readout(quiz.remaining(), format=".0f", color=GOLD, font_size=0.7).move_to(7, 3.4)
tiles = [tile(i, 7.4, 1.8, 0.55).move_to(-3.9 + i % 2 * 7.8, 0.9 - i // 2 * 2.2) for i in range(4)]
scene.play([title.animate.fade_in(), clock.animate.fade_in()]
           + [t.animate.grow_from_center().duration(0.4).delay(0.08 * i).settle()
              for i, t in enumerate(tiles)])
scene.stop("respondiendo", until=quiz.answered(share=1.0) | quiz.time_up())
quiz.reveal()

# Las fichas se encogen a una fila y aparecen los votos.
COLUMNS = [-5.7, -1.9, 1.9, 5.7]
row = [tile(i, 3.6, 0.9, 0.3).move_to(COLUMNS[i], 0.6) for i in range(4)]
before, after = scene.geometry.group(tiles), scene.geometry.group(row)
scene.play([magic_move(before, after).duration(0.8), clock.animate.fade_out()])
reveal = []
for i, x in enumerate(COLUMNS):
    bar = quiz.bar(i, length=2.8, thickness=0.16, radius=0.08, scale="total")
    bar.fill(WHITE).no_stroke().move_to(x - 0.3, -0.2)
    votes = scene.viz.readout(quiz.votes(i), format=".0f", color=WHITE, font_size=0.3)
    reveal += [bar.animate.create(), votes.move_to(x + 1.45, -0.2).animate.fade_in()]
    if i == quiz.correct:
        reveal.append(row[i].animate.glow(quiz.color(i), radius=0.4, intensity=1.3))
    else:
        reveal.append(row[i].animate.opacity(0.3))
scene.play(reveal)
scene.wait(2)
scene.stop("respuesta")
scene.render()
```

Los teléfonos ordenan y colorean las respuestas de un cuestionario a su manera
para que nadie copie al vecino, así que en un cuestionario la ficha de la
pantalla no coincide con la del teléfono; en una encuesta sí.

= Columnas de una encuesta <columnas>

Una encuesta sin respuesta correcta: columnas que crecen con el porcentaje de
cada respuesta y el total de teléfonos que votaron.

```python
# output: preview.webp
# show-code: true
from gaanim import Scene

INK, WHITE, MUTED = "#0f172a", "#f8fafc", "#94a3b8"
scene = Scene(frame=(16, 9), background=INK)
scene.rehearsal(30, seed=2)
poll = scene.poll("¿Cómo prefieres estudiar?", ["Solo", "En grupo", "Con música", "En silencio"],
                  rehearse=[2, 5, 4, 1])

scene.text(poll.question, size=0.6, color=WHITE, weight=700).move_to(0, 3.5)
for i, answer in enumerate(poll.options):
    x = -5.4 + 3.6 * i
    poll.bar(i, length=5, thickness=2.4, direction="up", radius=0.2) \
        .fill(poll.color(i)).no_stroke().move_to(x, -0.4)
    scene.viz.readout(poll.percent(i), format=".0f", suffix=" %", color=WHITE,
                      font_size=0.45).move_to(x - 0.5, 2.6)
    poll.icon(i, 0.4).move_to(x - 0.9, -3.3)
    scene.text(answer, size=0.36, color=WHITE, text_box="cap").move_to(x + 0.3, -3.3)
scene.viz.readout(poll.total(), format=".0f", prefix="votaron ", color=MUTED,
                  font_size=0.3).move_to(0, -4)
scene.wait(4)
scene.stop("votando")
scene.render()
```

Con el valor por defecto de `scale`, `"leader"`, la respuesta que va ganando
llena su columna y las demás miden en proporción, lo que se lee bien aunque
voten pocos; con `scale="total"` cada columna mide su porcentaje. Una lectura
(`readout`) se ancla por la izquierda para no saltar cuando cambian los
dígitos, así que para centrarla bajo algo córrela un poco a la izquierda. `poll.percent(i)` sigue en vivo también al
presentar un `.gaanim`.

= Sala de espera con personajes <sala>

Mientras el público escanea el QR, cada jugador cae desde arriba como el
personaje que armó en su teléfono y rebota al aterrizar. La pausa avanza sola
cuando hay al menos 8.

```python
# output: preview.webp
# show-code: true
from gaanim import Scene
from gaanim.live import ease_in, impact, lerp, pose

INK, WHITE, GOLD = "#1c0b3f", "#fff8ff", "#ffd23f"
scene = Scene(frame=(16, 9), background=INK)
scene.rehearsal(14, seed=5, arrive=5)
audience = scene.audience()

scene.geometry.rounded_rect(3.6, 3.6, 0.3).fill(WHITE).no_stroke().move_to(-5.6, 0.6)
audience.qr(3.1).fill(INK).move_to(-5.6, 0.6)
scene.text(audience.code, size=0.7, color=GOLD, weight=800).move_to(-5.6, -1.8)
scene.viz.readout(audience.count(), format=".0f", suffix=" en la sala", color=WHITE,
                  font_size=0.35).move_to(-6.8, -2.6)


def llegar(p):
    x = -1.8 + p.index % 6 * 1.7
    y = 1.4 - p.index // 6 * 2.0
    caida = ease_in(min(p.t / 0.5, 1))       # cae medio segundo
    sx, sy = impact(p.t - 0.5)               # y se aplasta al tocar el suelo
    return pose(x, lerp(6, y, caida), sx=sx, sy=sy, express="happy", since=0.5)


scene.live_zone(audience, llegar, size=1.1, names=True, name_size=0.22)
scene.wait(7)
scene.stop("sala", until=audience.at_least(8))
scene.render()
```

`p.index` es el orden de llegada y `p.t` los segundos desde que llegó, así que
cada uno hace su entrada a su tiempo. Con `names=True` la zona dibuja el apodo
bajo los pies. Para más movimientos, mira
#link("/publico/zonas-vivas/")[Zonas vivas].

= Cada uno corre a su respuesta <correr>

Después de revelar, cada personaje corre bajo la respuesta que eligió: los
que acertaron saltan, los demás se lamentan y quienes no respondieron se
quedan en una esquina.

```python
# output: preview.webp
# show-code: true
import math

from gaanim import Scene
from gaanim.live import ease_out_back, impact, lerp, pose, progress, smoothstep

INK, WHITE = "#1c0b3f", "#fff8ff"
COLUMNS = (-4.8, 0, 4.8)
scene = Scene(frame=(16, 9), background=INK)
scene.rehearsal(18, seed=3)
audience = scene.audience()
quiz = scene.quiz("¿Cuántos huesos tiene un adulto?", ["186", "206", "306"], correct=1,
                  rehearse=0.55)
for i, x in enumerate(COLUMNS):
    scene.geometry.rounded_rect(4.2, 1.1, 0.25).fill(quiz.color(i)).no_stroke().move_to(x, 2.9)
    scene.text(quiz.options[i], size=0.55, color=WHITE, weight=700, text_box="cap").move_to(x, 2.9)
scene.wait(1)
scene.stop("respondiendo")
quiz.reveal()


def correr(p):
    if p.answer < 0:                                     # no respondió
        return pose(-7.2 + 0.4 * p.index, -4, scale=0.6, express="sad", loop=True)
    x = COLUMNS[0] + (COLUMNS[1] - COLUMNS[0]) * p.answer + 3 * (p.random(1) - 0.5)
    y = 0.8 - 3.8 * p.random(2)
    salida = 0.4 * p.random(3)
    carrera = progress(p.t, salida, 0.8)
    if carrera < 1:                                      # corre desde abajo
        return pose(lerp(16 * p.random(4) - 8, x, ease_out_back(carrera)),
                    lerp(-6, y, smoothstep(carrera)) + math.sin(math.pi * carrera),
                    express="surprised", since=0)
    llego = p.t - salida - 0.8
    if p.answer_points > 0:                              # acertó: salta sin parar
        beat = (llego + p.random(5)) % 0.8 / 0.8
        return pose(x, y + 1.6 * beat * (1 - beat), express="winner", loop=True)
    sx, sy = impact(llego, amount=0.4)
    return pose(x, y, sx=sx, sy=sy, express="sad", since=salida + 0.8)


scene.live_zone(audience, correr, size=0.95)
scene.wait(4)
scene.stop("respuesta")
scene.render()
```

`p.answer` es la respuesta que eligió (-1 si ninguna) y `p.answer_points` lo
que ganó con ella; en selección múltiple, `p.answer_mask` tiene un bit por
respuesta elegida. `p.random(k)` da a cada jugador un número propio y fijo,
para que no corran todos igual.

= Batalla de equipos <batalla>

Dos equipos tiran de una cuerda: el que va ganando la arrastra hacia su lado.
Las barras de arriba comparan el promedio de puntos por jugador, más justo si
los equipos quedaron desparejos.

```python
# output: preview.webp
# show-code: true
import math

from gaanim import Scene
from gaanim.live import ease_out_back, pose

INK, MUTED = "#1a1033", "#cbb8ff"
scene = Scene(frame=(16, 9), background=INK)
teams = scene.teams(["Rojo", "Azul"], colors=["#ff4f8b", "#2fb8ff"])
scene.rehearsal(16, seed=2, skill=[0.85, 0.4])
audience = scene.audience()

# Aquí van tus preguntas; el ensayo las responde y suma los puntos.
for question, options, correct in [("¿7 × 8?", ["54", "56", "64"], 1),
                                   ("¿Capital de Perú?", ["Lima", "Cusco", "Puno"], 0)]:
    quiz = scene.quiz(question, options, correct=correct)
    scene.wait(0.5)
    scene.stop()
    quiz.reveal()

scene.segment("Batalla")


def side(p):
    return -1 if p.team == 0 else 1


def tirar(p):
    x = side(p) * (1.6 + p.team_index % 5 * 0.95)
    y = -0.6 - p.team_index // 5 * 1.25
    adelante = p.team_rank == 0
    arrastre = 1.4 * ease_out_back(min(p.time / 1.2, 1)) * (1 if adelante else -1)
    tiron = 0.15 * math.sin(2 * math.pi * (p.t * 1.4 + p.team_index * 0.13))
    return pose(x + side(p) * (arrastre + tiron), y, lean=side(p) * 0.25, flip=side(p) > 0,
                express="winner" if adelante else "hurt", loop=True)


scene.geometry.rect(15, 0.12).fill("#c89b6d").no_stroke().move_to(0, -0.2)
for team, x in [(0, -4.5), (1, 4.5)]:
    scene.text(teams.names[team], size=0.7, color=teams.colors[team], weight=800).move_to(x, 3.4)
    teams.bar(team, length=5.5, thickness=0.35, radius=0.17,
              direction="left" if team == 0 else "right") \
        .fill(teams.colors[team]).no_stroke().move_to(x, 2.5)
    scene.viz.readout(teams.average(team), format=".0f", suffix=" pts por jugador", color=MUTED,
                      font_size=0.28).move_to(x - 1.3, 1.9)
scene.live_zone(audience, tirar, size=0.9, lean=0.0)
scene.wait(5)
scene.stop("batalla")
scene.render()
```

`p.team_rank` es 0 para el equipo que va ganando y `p.team_index` el orden de
llegada dentro de su equipo, que sirve para formar filas en cada lado. Con
`choose=True` en `scene.teams`, cada uno elige su equipo en el teléfono.

= Carrera de puntajes <carrera>

Los cinco primeros corren por carriles: avanzan hasta su puntaje, cambian de
carril cuando alguien los adelanta y celebran una racha de aciertos. El
puntaje que muestran sube poco a poco gracias al estado entre cuadros.

```python
# output: preview.webp
# show-code: true
import math

from gaanim import Scene
from gaanim.live import STEP, ease_out_back, lerp, pose, spring, state

INK, SEA, MUTED, GOLD = "#1c0b3f", "#4a2a8c", "#cbb8ff", "#ffd23f"
scene = Scene(frame=(16, 9), background=INK)
scene.rehearsal(12, seed=8)
audience = scene.audience()
board = scene.leaderboard()

for question, options, correct in [("¿Símbolo del oro?", ["Ag", "Au", "Or"], 1),
                                   ("¿Año de la llegada a la Luna?", ["1965", "1969", "1972"], 1),
                                   ("¿Qué significa PDF?", ["Portable Document Format",
                                                           "Printed Data File"], 0)]:
    quiz = scene.quiz(question, options, correct=correct)
    scene.wait(0.5)
    scene.stop()
    quiz.reveal()

scene.segment("Carrera")
LANES, START, LENGTH = 5, -5.2, 10.4


def lane(rank):
    return 2.3 - 1.3 * rank


def contar(p):                         # el puntaje mostrado persigue al real
    return state(mostrado=p.state.mostrado + (p.score - p.state.mostrado) * min(1, 2.5 * STEP))


def correr(p):
    y = lerp(lane(p.previous_rank), lane(p.rank), ease_out_back(p.rank_since / 0.6))
    meta = START + LENGTH * p.state.mostrado / max(p.leader, 1)
    x = lerp(START - 1.2, meta, spring(p.t, frequency=0.9, damping=0.45))
    paso = abs(math.sin(2 * math.pi * 1.6 * p.t))
    animo = "winner" if p.streak >= 3 else ("happy" if p.rank == 0 else None)
    return pose(x, y + 0.16 * paso, express=animo, loop=True, visible=p.rank < LANES)


scene.text("Así va la carrera", size=0.6, color="#fff8ff", weight=700).move_to(0, 3.7)
for rank in range(LANES):
    y = lane(rank)
    scene.geometry.rect(LENGTH + 0.6, 0.07).fill(SEA).no_stroke().move_to(START + LENGTH / 2, y - 0.04)
    board.name(rank, size=0.28, weight=700).fill(MUTED).move_to(-6.9, y + 0.35)
    scene.viz.readout(board.points(rank), format=".0f", color=GOLD, font_size=0.28).move_to(6.2, y + 0.35)
scene.live_zone(audience, correr, size=0.85, state={"mostrado": 0.0}, update=contar)
scene.wait(5)
scene.stop("carrera")
scene.render()
```

`p.rank`, `p.previous_rank` y `p.rank_since` (segundos desde el último cambio
de puesto) animan los adelantamientos; `p.leader` es el puntaje del primero,
para medir la pista. `state` y `update` guardan un número por jugador entre
cuadros (ver #link("/publico/zonas-vivas/#estado")[Estado entre cuadros]).

= Podio <podio>

Los tres primeros suben a su escalón y el ganador salta de alegría mientras
llueve confeti; el resto aplaude desde abajo.

```python
# output: preview.webp
# show-code: true
import math

from gaanim import Scene
from gaanim.live import anticipate, impact, pose

INK, WHITE, GOLD = "#1c0b3f", "#fff8ff", "#ffd23f"
scene = Scene(frame=(16, 9), background=INK)
scene.rehearsal(10, seed=6)
audience = scene.audience()
board = scene.leaderboard()
for question, options, correct in [("¿Planeta con anillos?", ["Marte", "Saturno"], 1),
                                   ("¿Autor de «La noche estrellada»?", ["Van Gogh", "Monet"], 0)]:
    quiz = scene.quiz(question, options, correct=correct)
    scene.wait(0.5)
    scene.stop()
    quiz.reveal()

scene.segment("Podio")
PLACES = ((0.0, 0.2, 3.6), (-3.6, -0.8, 2.6), (3.6, -1.4, 2.0))   # x, arriba, alto
MEDALS = (GOLD, "#d7dbe8", "#e39b5f")


def celebrar(p):
    if p.rank < 3:
        x, y, _ = PLACES[p.rank]
        if p.rank > 0:
            return pose(x, y, scale=1.3, lean=0.08 * math.sin(2 * math.pi * p.t / 1.3), look_y=0.6)
        beat = p.t % 1.3                                 # agacharse, saltar, aterrizar
        sx, sy = anticipate(beat, at=0.35, amount=0.3, duration=0.35)
        alto = 0.0
        if 0.35 <= beat < 0.9:
            u = (beat - 0.35) / 0.55
            alto = 4.4 * u * (1 - u)
        elif beat >= 0.9:
            sx, sy = impact(beat - 0.9, amount=0.35)
        return pose(x, y + alto, sx=sx, sy=sy, scale=1.4, express="winner", loop=True)
    x = -6.3 + (p.rank - 3) % 8 * 1.8
    return pose(x, -4.0, scale=0.7, look_x=-x / 4, look_y=1)


heading = scene.text("¡Campeones!", size=0.8, color=WHITE, weight=800).move_to(0, 3.5)
steps = []
for rank, (x, top, height) in enumerate(PLACES):
    block = scene.geometry.rect(2.8, height).fill(MEDALS[rank]).no_stroke().move_to(x, top - height / 2)
    name = board.name(rank, size=0.36, weight=700, align="center").fill(INK).move_to(x, top - 0.5)
    points = scene.viz.readout(board.points(rank), format=".0f", suffix=" pts", color=INK,
                               font_size=0.3).move_to(x - 0.6, top - 1.0)
    steps.append(scene.geometry.group([block, name, points]).shift_by(0, -8))
scene.play([step.animate.shift_by(0, 8).duration(0.6).delay(0.2 * (2 - rank)).settle(overshoot=0.1)
            for rank, step in enumerate(steps)])
scene.live_zone(audience, celebrar, size=1.1)
confetti = scene.fx.confetti((0, -4.6), count=0, seed=21, spread=1.2, speed=(8, 12))
scene.play([confetti.animate.burst(110).duration(1.2), heading.animate.indicate().duration(0.6)])
scene.wait(2)
scene.stop("podio")
scene.render()
```

Al terminar la presentación, los teléfonos también muestran el podio y una
despedida.

= Más ideas <ideas>

- *Sonidos*: cualquier animación lleva un sonido con `.sound(ruta)`, y
  `Transition.magic_move(..., sound=)` suena al cambiar de diapositiva. Los
  paquetes de efectos de Kenney son de dominio público (CC0).
- *Un dato entre preguntas*: después de revelar, una
  `scene.viz.bar_race` cuenta la historia detrás de la respuesta.
- *Un anfitrión*: `scene.character(...)` pone en la escena un personaje como
  los del público, que celebra con `express("winner")` cuando quieras.
- *Transiciones que continúan*: nombra las piezas que se repiten en cada
  diapositiva (el logo, el marcador de equipos) con `.named()` y usa
  `Transition.magic_move` en `scene.segment` para que se muevan en lugar de
  aparecer de nuevo.
- *Partidas de verdad*: el ejemplo `examples/la_gran_trivia/` del repositorio
  junta todas estas recetas en una trivia completa con preguntas en Markdown,
  sonidos e imágenes.
