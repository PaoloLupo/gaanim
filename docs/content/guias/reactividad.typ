#import "../../components/section.typ": docs-chapter
#import "../../components/tutorial.typ": idea

#show: docs-chapter.with(
  title: "Reactividad",
  description: "Objetos que dependen de otros: parámetros, puntos y líneas reactivas, trazos y updaters",
  route: "/guias/reactividad/",
)

En esta guía aprenderás a construir objetos que dependen de otros: un punto
que recorre un círculo, una línea que une dos objetos en movimiento, un número
que muestra un ángulo o una curva que se dibuja sola. Todo sale de una única
magnitud animable, igual que en el #link("/tutorial/circulo-al-seno/")[tutorial].

```python
# output: preview.webp
import math
from gaanim import CYAN, GOLD, WHITE, Easing, Scene, computed

scene = Scene(frame=(16, 9), background="#0f172a")
center = (-5, 0)
radius = 2.0

theta = scene.viz.parameter(0.0)
tip = scene.geometry.polar_point(center, radius, theta)
wave_point = scene.geometry.point_ref(
    computed(lambda angle: -2.2 + 1.3 * angle, inputs=[theta]),
    computed(lambda angle: radius * math.sin(angle), inputs=[theta]),
)

orbit = scene.geometry.circle(radius).no_fill().stroke(WHITE, 0.03).move_to(*center)
arm = scene.geometry.tracking_line(center, tip).stroke(GOLD, 0.05)
dot = scene.geometry.dot(0.12).fill(GOLD).follow(tip)
pen = scene.geometry.dot(0.1).fill(CYAN).follow(wave_point)
bridge = scene.geometry.tracking_line(tip, wave_point).stroke(WHITE, 0.02)
wave = scene.geometry.traced_path(pen).stroke(CYAN, 0.06).no_fill()
value = scene.viz.readout(theta, label="$theta$", format=".2f").move_to(-5, 2.9)

scene.play([orbit.animate.create().duration(0.5)])
scene.play([item.animate.fade_in().duration(0.3) for item in (arm, dot, pen, bridge, wave, value)])
scene.play([theta.animate.set(2 * math.pi).duration(3.0).easing(Easing.LINEAR)])
scene.render()
```

= Una sola fuente de verdad

La escena anterior anima un único valor: `theta`. El punto del círculo, el
brazo, el punto de la onda, la línea que los une, el trazo y el número se
calculan a partir de él en cada fotograma. No hay dos velocidades que puedan
desincronizarse.

Esa es la forma recomendada de pensar la reactividad en Gaanim:

+ Crea un `Parameter` con `scene.viz.parameter(valor)` para la magnitud que
  cambia (un ángulo, un tiempo, una amplitud). Es invisible.
+ Deriva de él puntos, líneas, curvas y números.
+ Anima solo el parámetro con `parameter.animate.set(valor)` dentro de
  `scene.play([...])`.

Como todo es función del parámetro, Gaanim puede saltar a cualquier instante
sin reproducir lo anterior: el editor, la exportación y las capturas muestran
exactamente lo mismo.

#idea[
Si te descubres sincronizando a mano dos animaciones para que «coincidan»,
probablemente te falta un parámetro compartido.
]

= Puntos que dependen de valores

Un `PointRef` es un punto lógico: no se dibuja, pero otros objetos pueden
seguirlo o usarlo como extremo. Las fábricas de `scene.geometry` aceptan en
cada coordenada un número fijo, un `Parameter` o un valor calculado:

- `polar_point(origen, radio, ángulo)`: un punto a cierta distancia y ángulo
  (en radianes) de un origen.
- `point_ref(x, y)`: un punto con coordenadas reactivas.
- `offset_point(origen, dx, dy)`: un punto desplazado respecto de otro que
  puede moverse.
- `point_between(a, b, alpha=0.5)`: un punto entre dos extremos.

Para verlo, haz que un drawable lo siga con `follow`:

```python
# continue
middle = scene.geometry.point_between(center, tip, alpha=0.5)
marker = scene.geometry.dot(0.08).fill(WHITE).follow(middle)
```

Un objeto con `follow` empieza oculto: aparece cuando incluyes su entrada
(`fade_in`, `create`…) en un `scene.play`. Lo mismo vale para las líneas
reactivas y los trazos de las secciones siguientes.

= Valores calculados con `computed`

`computed(función, inputs=[...])` crea un valor derivado de otros. La función
recibe los valores de `inputs` en el mismo orden y devuelve un número:

```python
# continue
height = computed(lambda angle: radius * math.sin(angle), inputs=[theta])
readout = scene.viz.readout(height, label="$y$", format=".2f").move_to(-5, -2.9)
```

Un valor calculado puede usarse donde se acepte un parámetro: coordenadas de
puntos, radios, extremos de líneas, lecturas numéricas o la cámara. También
puede depender de otros valores calculados. Si necesitas el tiempo de la línea
de tiempo, decláralo como entrada: `inputs=[scene.time]`.

`scene.viz.readout` acepta además una función con sus entradas directamente:

```python
# continue
area = scene.viz.readout(lambda r: math.pi * r**2, inputs=[theta], label="$A$")
```

= Líneas y flechas que siguen a sus extremos

`scene.geometry.tracking_line(desde, hasta)` dibuja una línea cuyos extremos se
recalculan en el mismo fotograma. Cada extremo puede ser una tupla fija, un
drawable, un `PointRef` o un punto de anclaje:

```python
# continue
from gaanim import Anchor

box = scene.geometry.rect(1.6, 0.9).no_fill().stroke(WHITE, 0.03).move_to(4.5, 2.5)
pointer = scene.geometry.connector(box.anchor_point(Anchor.BOTTOM), tip)
```

`drawable.anchor_point(Anchor.BOTTOM)` es un extremo pegado al borde inferior
de `box`, que lo acompaña si el objeto se mueve, rota o escala.
`scene.geometry.connector` hace lo mismo que `tracking_line`, pero con punta de
flecha; puede aparecer con `grow_arrow()`.

= Dejar un rastro con `traced_path`

`scene.geometry.traced_path(objeto)` registra la posición de un drawable y la
dibuja como un trazo. Con `dissipating_time=` los puntos más antiguos
desaparecen tras esos segundos:

```python
# continue
comet = scene.geometry.traced_path(dot, dissipating_time=0.8).stroke(GOLD, 0.04).no_fill()
```

El trazo también se reconstruye al saltar a otro instante, así que no depende
de haber reproducido la escena desde el principio.

= Puntos y tangentes sobre una curva

Para recorrer una curva ya dibujada, usa un parámetro entre `0` y `1` con
`point_on_curve` y, si quieres, `tangent_on_curve`:

```python
# continue
t = scene.viz.parameter(0.0)
curve = scene.geometry.polyline([(x / 10, math.sin(x / 10)) for x in range(0, 63)])
runner = scene.geometry.point_on_curve(curve, t)
tangent = scene.geometry.tangent_on_curve(curve, t, length=1.2)
```

`normal_on_curve` y `curvature_on_curve` completan la familia.

= Copiar el movimiento de otro objeto

Cuando una propiedad debe copiar la de otro drawable, usa un *binding*:

- `bind_x_from(fuente)`, `bind_y_from(fuente)` y `bind_position_from(fuente)`
  copian la posición.
- `bind_rotation_from(fuente)` copia la rotación, con `ratio=` y `phase=`
  opcionales; es útil para engranajes.
- `follow(fuente, offset=)` coloca el objeto sobre un extremo, con un
  desplazamiento opcional.

Para relaciones numéricas (una etiqueta que muestra una distancia, un punto a
mitad de camino) prefiere un `PointRef` o `computed`: la dependencia queda
explícita.

= Cuándo usar un updater

Un `Updater` es un comportamiento continuo que se añade a un drawable con
`add_updater` y se retira con `remove_updater`. Úsalo cuando el movimiento no
depende de una magnitud que quieras mostrar o compartir: un logo que respira,
un icono que gira, un temblor orgánico.

```python
# output: preview.webp
from gaanim import CORAL, CYAN, GOLD, Scene, Updater

scene = Scene(frame=(16, 9), background="#0f172a")
gear = scene.geometry.square(1.4).fill(CYAN).move_to(-4, 0)
buoy = scene.geometry.circle(0.7).fill(GOLD)
badge = scene.geometry.circle(0.7).fill(CORAL).move_to(4, 0)

gear.add_updater(Updater.rotate(1.5))
buoy.add_updater(Updater.oscillate("y", frequency=1.0, low=-0.6, high=0.6))
badge.add_updater(Updater.wiggle(position=0.15, rotation=0.1, frequency=3.0, seed=7))

scene.wait(2.0)
gear.remove_updater()
scene.render()
```

Los presets (`orbit`, `rotate`, `bob`, `pulse`, `advance_x`, `wiggle` y
`oscillate`) son funciones del tiempo de la línea de tiempo, así que también
son exactos al saltar a cualquier instante. `wiggle` y `oscillate` se suman a
las animaciones del objeto en lugar de reemplazarlas.

Si el movimiento sí representa una magnitud (una fase, una posición en una
gráfica), vuelve a la primera sección: un `Parameter` es más fácil de mostrar,
compartir y controlar.

= Datos medidos

Para reproducir una serie registrada (un sensor, una simulación externa), usa
`drive_from_samples` con una lista de tiempos, otra de valores y la propiedad
que controlan. Se evalúa sin código Python
por fotograma y es exacto al saltar:

```python
>>>import math
>>>from gaanim import GOLD, Scene
>>>scene = Scene(frame=(16, 9), background="#0f172a")
probe = scene.geometry.dot(0.15).fill(GOLD)
times = [i * 0.05 for i in range(61)]
path = [(math.cos(2 * t), math.sin(3 * t)) for t in times]
probe.drive_from_samples(times, path, "xy", scale=2.5)
```

Los tiempos son relativos al cursor en el que haces la llamada. Un `Parameter`
también tiene `drive_from_samples`, y todo lo que dependa de él seguirá la
serie.

= Simulaciones con estado

Algunas simulaciones, como una pelota que cae y rebota, acumulan estado de un
fotograma al siguiente y no se pueden escribir como una función del tiempo.
Para ellas usa `add_updater_fn` con dos argumentos obligatorios en la práctica:
`reset`, que devuelve el estado al inicial, y `fixed_dt`, un paso de tiempo
fijo. Así Gaanim puede reiniciar la simulación y repetir los pasos al saltar o
exportar:

```python
# output: preview.webp
from gaanim import GOLD, WHITE, Scene

scene = Scene(frame=(16, 9), background="#0f172a")
floor = scene.geometry.line(-3, -3, 3, -3).stroke(WHITE, 0.03)
ball = scene.geometry.circle(0.3).fill(GOLD).move_to(0, 3)

state = {"y": 3.0, "vy": 0.0}

def reset():
    state["y"], state["vy"] = 3.0, 0.0

def step(position, dt, elapsed):
    state["vy"] -= 9.8 * dt
    state["y"] += state["vy"] * dt
    if state["y"] < -2.7:
        state["y"], state["vy"] = -2.7, -0.8 * state["vy"]
    return (position[0], state["y"], position[2])

ball.add_updater_fn(step, reset=reset, fixed_dt=1 / 240)
scene.play([floor.animate.create().duration(0.3)])
scene.wait(2.2)
ball.remove_updater()
scene.render()
```

`step(posición, dt, transcurrido)` devuelve la nueva posición local
`(x, y, z)`. Sin `reset` y `fixed_dt`, usa `add_updater_fn` solo para
comportamientos ligeros que dependan del tiempo transcurrido y no de pasos
anteriores.

= Reglas para que todo sea reproducible

Las funciones que pasas a `computed`, `readout`, `plot` o `add_updater_fn`
deben ser síncronas y puras: con las mismas entradas devuelven lo mismo.
Con el mismo script, las mismas entradas y el mismo entorno de Python, la
reproducción, el rebobinado, los saltos y la exportación producen el mismo
resultado.

- No leas el reloj del sistema ni archivos dentro de esas funciones.
- Si necesitas azar, usa `scene.random(seed)` o pasa la semilla como entrada;
  nunca el módulo `random` global.
- No modifiques variables externas desde un `computed`.

Si una función lanza una excepción o devuelve un valor no finito, Gaanim no
reutiliza el último valor válido: la geometría muestra un hueco, la lectura
numérica muestra `invalid` y los extremos dejan de estar vinculados en ese
fotograma. No se garantiza igualdad bit a bit entre plataformas distintas.

= Errores frecuentes

- *El objeto reactivo no aparece:* las líneas reactivas, los trazos y los
  objetos con `follow` empiezan ocultos. Añade su `fade_in` a un `scene.play`.
- *El movimiento cambia al saltar a otro instante:* hay estado oculto en una
  función. Conviértelo en entrada de `computed` o usa `reset` y `fixed_dt`.
- *Dos objetos se desincronizan:* derívalos del mismo parámetro en lugar de
  animarlos por separado.
- *`NameError` o error de aridad en `computed`:* la función debe aceptar
  exactamente tantos argumentos como elementos tenga `inputs`.

Consulta las firmas completas en la #link("/referencia/scene/")[referencia de
la escena] y en #link("/referencia/visualization/")[Visualización].
