#import "../../components/section.typ": docs-chapter

#show: docs-chapter.with(
  title: "Zonas vivas",
  description: "El público entra a la escena como sus personajes y tú decides cómo se mueven, con funciones normales de Python",
  route: "/guias/zonas-vivas/",
)

En esta guía aprenderás a poner al público dentro de tu escena: mientras
presentas, cada persona que se une desde su teléfono aparece como el personaje
que armó y tú decides cómo se mueve con una función de Python. Sirve para una
sala de espera, una carrera con los puntajes, un podio o una batalla entre
equipos.

```python
>>>import math
>>>from gaanim import Scene
from gaanim.live import pose, smoothstep

scene = Scene(frame=(16, 9))
audience = scene.audience()

def fila(p):
    llegada = smoothstep(min(p.t / 0.8, 1))      # entra en 0,8 s
    x = -6 + p.index * 1.5
    return pose(x, -2 + 4 * (1 - llegada), express="happy")

scene.live_zone(audience, fila, names=True)
scene.wait(6)
```

`scene.live_zone(audience, comportamiento)` abre una zona en el cursor que
dura hasta `zone.close()` o el final del segmento. El comportamiento es una
función normal de Python que recibe al jugador `p` y devuelve
`pose(x, y, ...)`: dónde están sus pies y cómo se ve. Gaanim la compila al
crear la escena, así que un `.gaanim` exportado la ejecuta sin Python. Fuera
de una presentación en vivo, la zona reproduce el
#link("/guias/presentaciones/#ensayo")[ensayo], el público inventado de la
escena.

= El jugador

`p` trae lo que el motor sabe del jugador en cada cuadro:

- `p.t`: segundos desde que llegó a la zona; `p.time`, desde que se abrió.
- `p.joined`: cuándo llegó; `p.index`: su orden de llegada, desde 0;
  `p.count`: cuántos hay.
- `p.rank` (0 para el que va primero), `p.score`, `p.leader` (el puntaje del
  primero), `p.previous_rank`, `p.rank_since`, `p.previous_score` y
  `p.score_since`, para animar los cambios de puesto.
- `p.team`, `p.team_index`, `p.team_count`, `p.team_score` y `p.team_rank`
  en un juego con #link("/guias/presentaciones/#equipos")[equipos].
- `p.random(k)`: un número entre 0 y 1 que depende solo del jugador y de
  `k`, el mismo en cada cuadro y en cada equipo.
- `p.state.<nombre>`: los números que la zona guarda para él (ver
  #link("#estado")[Estado entre cuadros]).

`pose` acepta `rotation`, `scale`, `sx` y `sy` (aplastar y estirar), `lean`,
`look_x` y `look_y` (hacia dónde miran los ojos), `flip`, `visible`,
`show_name` y `express` (`"happy"`, `"sad"`, `"hurt"`, `"winner"`,
`"surprised"`) con `loop` y `since`.

`gaanim.live` trae ayudas para animar con los principios de la animación:
`lerp`, `clamp`, `progress`, `smoothstep`, `ease_in`, `ease_out`,
`ease_in_out`, `ease_out_back`, `spring`, `wobble`, `impact` (el rebote al
aterrizar), `anticipate` (agacharse antes de saltar), `ballistic` y
`landing` (tiros parabólicos). Además, la zona estira a cada personaje en la
dirección en que se mueve, lo inclina al correr, balancea orejas y sombreros
y mueve sus ojos hacia donde va; `squash`, `lean`, `follow` y `look` en
`live_zone` regulan cuánto, y 0 lo apaga.

= Estado entre cuadros <estado>

Un comportamiento es una función del tiempo: no recuerda nada del cuadro
anterior. Cuando necesitas memoria por jugador (vidas, energía que se carga,
peldaños subidos), declárala con `state` y calcula los siguientes valores con
`update`:

```python
>>>from gaanim import Scene
from gaanim.live import STEP, pose, state

scene = Scene(frame=(16, 9))
audience = scene.audience()

def cargar(p):                                   # cada 1/60 s
    energia = p.state.energia + 0.7 * STEP
    if energia >= 1:
        return state(energia=0, peldanos=p.state.peldanos + 1)
    return state(energia=energia)                # peldanos sigue igual

def subir(p):
    return pose(-6 + p.index * 2, -3.5 + 1.2 * p.state.peldanos + 0.4 * p.state.energia)

scene.live_zone(audience, subir, state={"energia": 0.0, "peldanos": 0}, update=cargar)
scene.wait(6)
```

`state` da los números (hasta 8) y dónde empieza cada uno cuando el jugador
llega. `update(p)` corre cada `STEP` segundos (1/60) para cada jugador y
devuelve `state(...)` con los que cambian; los que no nombra conservan su
valor. El comportamiento y la actualización los leen como `p.state.energia`.
La zona los calcula paso a paso desde que se abre, así que saltar en la línea
de tiempo y exportar dan siempre lo mismo.

= Diferencias con Python <diferencias-con-python>

El comportamiento se escribe en Python, pero no se ejecuta con Python: Gaanim
lo traduce a instrucciones que corren en Rust, en la presentación, en el
reproductor y en la web. Por eso acepta un subconjunto del lenguaje, y algunas
cosas se comportan distinto.

== Lo que puedes usar

- Números, `True`/`False`, `None` y las cadenas de las expresiones.
- `+ - * / // % **` con las reglas de Python (`-7 // 2` es `-4`, `-7 % 2`
  es `1`, `round` redondea las mitades al par), comparaciones encadenadas
  (`0 < x < 1`), `and`, `or`, `not`, `in` sobre tuplas e `is None`.
- `if`/`elif`/`else`, el condicional `a if c else b`, asignaciones normales,
  aumentadas (`x += 1`) y con desempaquetado (`x, y = ...`), `return`.
- `for i in range(N)` con un `N` fijo (se desenrolla, hasta 256 vueltas).
- Tuplas (una lista escrita entre corchetes se trata como tupla), `len`,
  `sum`, `min`, `max`, `abs`, `round`, `int`, `float`, `bool` y todo el
  módulo `math`.
- Tus propias funciones, definidas a nivel de módulo, con parámetros
  normales y valores por defecto (puedes llamarlas pasando argumentos por
  nombre); y constantes del módulo.

== Lo que no

- `while`, `break`, `continue`, recursión, `try`, `with`, clases, `global`.
- Modificar colecciones (`append`, asignar a `xs[i]`), diccionarios,
  conjuntos, comprensiones, `lambda` y f-strings.
- Parámetros solo por nombre (`def f(a, *, b)`), `*args` y `**kwargs`.
- Otras bibliotecas (`numpy`, `random`...): usa `math` y `p.random(k)`.
- `p.name` y otras cosas del jugador que no estén en la lista de arriba.

Si escribes algo que no se puede compilar, `scene.live_zone` lanza
`gaanim.live.BehaviorError` señalando la línea y explicando qué usar.

== Lo que se comporta distinto

- *Las constantes se congelan al compilar.* El comportamiento lee el valor
  que tenían las variables del módulo al llamar a `scene.live_zone`; cambiarlas
  después no tiene efecto.
- *Las dos ramas de un `if` se calculan* y se elige una. No hay efectos
  secundarios que puedan notarse, pero una operación inválida en la rama que
  no se elige tampoco falla.
- *Los errores son números.* Donde Python lanzaría un error (dividir por
  cero, `math.sqrt(-1)`, `math.log(0)`), el comportamiento obtiene `inf` o
  `nan`, como en coma flotante; una pose con `nan` no se dibuja bien, así que
  protege esos casos con `max` o un `if`.
- *Todos los números son `float`.* `int(x)` trunca, pero el resultado sigue
  siendo un número de coma flotante: los enteros son exactos hasta 2#super[53].
- *El tiempo es discreto para lo que recuerda.* Las expresiones que empiezan
  y el estado avanzan en pasos de 1/60 s desde que se abre la zona; la pose,
  en cambio, se calcula para el instante exacto de cada cuadro.

Los mismos casos se comprueban en las pruebas del proyecto: cada
comportamiento de ejemplo se ejecuta en CPython y en Rust con jugadores de
muestra y ambos deben dar las mismas poses.
