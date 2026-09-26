#import "../../components/section.typ": docs-chapter

#show: docs-chapter.with(
  title: "Movimiento",
  description: "Easings, springs, repetición, stagger, ruido y trayectorias para que cada movimiento tenga intención",
  route: "/guias/movimiento/",
)

= Movimiento con intención

En esta guía aprendes a decidir *cómo* se mueve algo, no solo adónde va:
elegir un easing por lo que comunica, usar springs sin pensar en física,
repetir y hacer bucles, convertir una rejilla en una onda, dar vida a objetos
quietos con ruido determinista y mover objetos por trayectorias.

Todo lo que ves aquí es una función pura del tiempo: la vista previa, un seek
a cualquier instante y la exportación producen el mismo fotograma.

```python
from gaanim import CYAN, Easing, Scene, stagger

scene = Scene(frame=(16, 9), background="#0f172a")
dots = [
    scene.geometry.dot(0.22).fill(CYAN).move_to(x - 4.5, y - 1.5)
    for x in range(10)
    for y in range(4)
]
scene.play(stagger(
    *[d.animate.grow_from_center().duration(0.6).easing(Easing.BOUNCY) for d in dots],
    total=0.8,
    origin="center",
))
scene.wait(0.2)
scene.render()
# output: preview.webp
```

Una onda desde el centro con un spring con rebote: dos decisiones de
movimiento, dos argumentos.

== Elegir un easing

El easing decide cómo se reparte el recorrido dentro de la duración. Si no
indicas ninguno, las animaciones de propiedades (`move_to`, `scale_to`,
`fill`…) usan `Easing.SMOOTH`, que acelera al salir y frena al llegar.

#table(
  columns: (1.3fr, 2fr),
  inset: 7pt,
  [*Easing*], [*Cuándo usarlo*],
  [`Easing.SMOOTH`], [Mover algo entre dos posiciones de reposo. Es el tono editorial por defecto.],
  [`Easing.ease_out(curva)`], [Entradas: el objeto llega rápido y se asienta. Se siente responsivo.],
  [`Easing.ease_in(curva)`], [Salidas: el objeto arranca despacio y se va acelerando.],
  [`Easing.ease_in_out(curva)`], [Como `SMOOTH`, con la intensidad de la curva elegida.],
  [`Easing.LINEAR`], [Velocidad constante: giros continuos, barras de progreso, recorridos uniformes.],
  [`Easing.THERE_AND_BACK`], [Ir y volver en una sola animación, por ejemplo un pulso.],
  [`Easing.steps(n)`], [Saltos discretos: relojes, contadores, animación "a tirones".],
)

`EasingCurve` fija la intensidad: `QUADRATIC` es suave, `CUBIC` es el punto
medio habitual, `QUARTIC`, `QUINTIC` y `EXPONENTIAL` son cada vez más
marcadas, y `SINE` y `CIRCULAR` dan variantes más orgánicas.

```python
from gaanim import CORAL, CYAN, Easing, EasingCurve, Scene

scene = Scene(frame=(16, 9), background="#0f172a")
entrada = scene.geometry.circle(0.5).fill(CYAN).move_to(-9, 1.2)
salida = scene.geometry.circle(0.5).fill(CORAL).move_to(0, -1.2)
scene.play([
    entrada.animate.move_to(0, 1.2).duration(0.8).easing(Easing.ease_out(EasingCurve.CUBIC)),
    salida.animate.move_to(9, -1.2).duration(0.8).easing(Easing.ease_in(EasingCurve.CUBIC)),
])
scene.render()
```

La regla práctica: *ease-out* para lo que entra, *ease-in* para lo que sale y
*ease-in-out* para lo que cambia de sitio dentro del cuadro.

== Easings con carácter

Las fábricas paramétricas añaden personalidad. Todas son deterministas y
validan sus argumentos: un valor fuera de dominio lanza `ValueError`.

```python
from gaanim import CORAL, CYAN, GRAY, Easing, Scene

scene = Scene(frame=(16, 9), background="#0f172a")
filas = [
    ("back(2.5)", Easing.back(2.5)),
    ("elastic(1.2, 0.35)", Easing.elastic(1.2, 0.35)),
    ("bounce(0.6)", Easing.bounce(0.6)),
    ("slow_mo()", Easing.slow_mo()),
    ("rough(seed=4)", Easing.rough(0.8, points=16, seed=4)),
    ('steps(6)', Easing.steps(6)),
]
anims = []
for i, (nombre, easing) in enumerate(filas):
    y = 2.5 - i * 1.0
    scene.text(nombre).fill(GRAY).move_to(-5, y)
    punto = scene.geometry.dot(0.15).fill(CORAL if i % 2 else CYAN).move_to(-2, y)
    anims.append(punto.animate.move_to(5, y).duration(1.4).easing(easing))
scene.play(anims)
scene.wait(0.2)
scene.render()
# output: preview.webp
```

- `back(overshoot)`: se pasa del destino y vuelve. Ideal para tarjetas y
  botones que "encajan".
- `elastic(amplitude, period)`: oscila como una goma. Úsalo con moderación.
- `bounce(strength)`: rebota al llegar; `strength=0` es un cúbico.
- `slow_mo(linear_ratio, power)`: rápido, cámara lenta y rápido otra vez. Sirve
  para detener la mirada en el centro del recorrido.
- `rough(strength, points, seed)`: parpadeo y temblor deterministas, para
  fallos técnicos o luces que titilan.
- `squish(easing, start, end)`: aplica un easing solo dentro de una fracción
  del clip y mantiene los extremos fuera de ella.
- `from_svg("M0,0 C0.3,0 0.2,1.2 1,1")`: una curva dibujada en cualquier
  editor, con `x` como tiempo e `y` como progreso.
- `back`, `elastic` y `bounce` aceptan `mode="in"`, `"out"` o `"in_out"`.

== Springs sin física

Un spring perceptual se describe por cómo se ve: `bounce` es el sobrepaso
máximo, de `0` (sin rebote) a casi `1`, y el resorte se asienta dentro de la
duración de la animación. `duration` controla el ritmo y `bounce` el carácter.

```python
from gaanim import GOLD, Easing, Scene

scene = Scene(frame=(16, 9), background="#0f172a")
logo = scene.geometry.star(5, 1.2, 0.5).fill(GOLD).scale_by(0.4)
scene.play(logo.animate.scale_to(1.0).duration(0.6).easing(Easing.spring(bounce=0.35)))
scene.render()
```

Los presets con nombre cubren los casos habituales sin ajustar nada:

#table(
  columns: (1fr, 2fr),
  inset: 7pt,
  [*Preset*], [*Carácter*],
  [`Easing.SMOOTH_SPRING`], [Amortiguamiento crítico: llega lo antes posible sin pasarse.],
  [`Easing.GENTLE`], [Suave y pausado, con un sobrepaso apenas visible.],
  [`Easing.QUICK`], [Enérgico, con un sobrepaso pequeño que se asienta pronto.],
  [`Easing.SNAPPY`], [Rápido y nítido, con un 20 % de sobrepaso.],
  [`Easing.BOUNCY`], [Juguetón, con un 45 % de sobrepaso y rebotes visibles.],
)

Si necesitas el modelo físico, `Easing.spring(stiffness=180, damping=14,
mass=1.0, velocity=2.0)` sigue disponible. No mezcles `bounce` con parámetros
físicos: la combinación lanza `ValueError`. Todos los springs terminan
exactamente en el destino.

== Tu propia curva con Easing.custom

`Easing.custom(función, samples=256)` convierte cualquier función de Python en
un easing. La función se muestrea una sola vez al crear el easing, así que el
render nunca vuelve a Python. Los valores deben ser finitos y estar en
`[-1, 2]`.

```python
import math
from gaanim import CYAN, Easing, Scene

scene = Scene(frame=(16, 9), background="#0f172a")
pelota = scene.geometry.circle(0.35).fill(CYAN).move_to(0, 3)
salto = Easing.custom(lambda t: 1 - abs(math.cos(3 * math.pi * t)) * (1 - t) ** 2)
scene.play(pelota.animate.move_to(0, -2.5).duration(1.2).easing(salto))
scene.render()
```

Usa `custom` cuando la curva sale de una fórmula. Si la tienes dibujada, usa
`from_svg`.

== Repetir, alternar y hacer bucles

`anim.repeat(count, yoyo=False, delay=0)` reproduce una animación varias
veces: `duration` y `easing` describen un ciclo. Con `yoyo=True` los ciclos
alternan de sentido, así que un número par termina donde empezó.
`anim.loop(mode, until=segundos)` repite tantos ciclos completos como quepan:
`"cycle"` reinicia, `"pingpong"` alterna y `"offset"` acumula, útil para
giros continuos. `Composition.repeat(count)` repite un grupo completo.

```python
import math
from gaanim import CORAL, CYAN, GOLD, Scene, parallel

scene = Scene(frame=(16, 9), background="#0f172a")
spinner = scene.geometry.square(1.2).fill(CYAN).move_to(-4.5, 0)
badge = scene.geometry.circle(0.7).fill(GOLD).move_to(-1, 0)
flecha = scene.geometry.arrow(1.5, 0, 3, 0).fill(CORAL)
a = scene.geometry.dot(0.2).fill(CYAN).move_to(5, 0.5)
b = scene.geometry.dot(0.2).fill(GOLD).move_to(5, -0.5)
scene.play([
    spinner.animate.rotate_by(math.pi / 2).duration(0.5).repeat(4),
    badge.animate.scale_to(1.2).duration(0.4).repeat(4, yoyo=True, delay=0.1),
    flecha.animate.shift_by(0.8, 0).duration(0.5).loop("pingpong", until=2.0),
    parallel(a.animate.shift_by(0, 0.6), b.animate.shift_by(0, -0.6)).repeat(2),
])
scene.render()
```

La duración total (`count · duration + (count − 1) · delay`) entra en la
línea de tiempo, que sigue siendo finita: los bucles se pueden inspeccionar,
recortar y exportar como cualquier otra animación.

== Stagger: de una lista a una coreografía

`stagger(*anims, each=0.1)` retrasa cada animación por su índice. Con `origin`
el retraso crece con la distancia a un origen, y la lista se convierte en una
onda:

- `"start"` y `"end"`: desde el primer o el último objeto.
- `"center"`: desde el centro del conjunto hacia fuera.
- `"edges"`: desde los bordes hacia dentro.
- `"random"`: orden aleatorio fijado por `seed`.
- `(x, y)`: desde un punto de la escena, por ejemplo donde "cae" algo.

`each` es el retraso por paso de separación; `total` fija en su lugar la
duración de toda la onda, que es más fácil de ajustar al ritmo de la escena.
`grid="auto"` (por defecto) mide distancias con las posiciones declaradas y
`grid=(filas, columnas)` usa celdas. `easing` da forma a la propia onda.

```python
from gaanim import CYAN, GOLD, Easing, EasingCurve, Scene, distribute, stagger

scene = Scene(frame=(16, 9), background="#0f172a")
dots = [scene.geometry.dot(0.2).fill(CYAN).move_to(c - 5.5, 3 - r) for r in range(7) for c in range(12)]
scene.play(stagger(
    *[d.animate.grow_from_center().duration(0.4) for d in dots],
    total=0.8, origin="center", easing=Easing.ease_in(EasingCurve.QUADRATIC),
))
scene.play(stagger(*[d.animate.fill(GOLD).duration(0.3) for d in dots], total=0.8, origin=(-5.5, -3.0)))
scene.play(stagger(*[d.animate.opacity(0.3).duration(0.3) for d in dots], total=0.8, origin="random", seed=7))
tamaños = distribute(dots, 1.0, 0.5, origin="edges")
scene.play([d.animate.scale_to(s).duration(0.5) for d, s in zip(dots, tamaños)])
scene.render()
```

`distribute(items, low, high, origin=...)` usa el mismo orden que `stagger`,
pero reparte *valores* (tamaños, opacidades, colores) en lugar de tiempos. Los
objetos cuya posición depende de un layout vuelven al orden por índice.

== Coreografía con etiquetas

Cuando una secuencia crece, calcular retrasos a mano se vuelve frágil.
`label(nombre)` marca un instante dentro de un `sequence`, y
`Composition.insert(anim, at=...)` coloca animaciones relativas a él:
`"golpe+0.15"`, `"<"` (junto con el anterior), `">-0.1"` (antes de que
termine el anterior) o `"-=0.2"` (solapando el final actual).

```python
from gaanim import CYAN, GOLD, GRAY, WHITE, Scene, label, sequence

scene = Scene(frame=(16, 9), background="#0f172a")
titulo = scene.text("Etiquetas", role="title").fill(WHITE).move_to(0, 2.4)
subtitulo = scene.text("tiempos relativos").fill(GRAY).move_to(0, 1.4)
logo = scene.geometry.circle(0.9).fill(GOLD).move_to(0, -1)
anillo = scene.geometry.circle(1.3).no_fill().stroke(CYAN, 0.08).move_to(0, -1)
intro = (
    sequence(
        titulo.animate.write().duration(0.8),
        label("golpe"),
        subtitulo.animate.fade_in().duration(0.4),
    )
    .insert(logo.animate.grow_from_center().duration(0.6), at="golpe+0.15")
    .insert(anillo.animate.create().duration(0.6), at="<")
)
print(intro.schedule().labels)
scene.play(intro)
scene.marker("clímax")
scene.render()
```

`scene.marker(nombre)` nombra un instante de la línea de tiempo global. El
editor lo muestra en la barra de reproducción y `gaanim export --from clímax`
lo acepta en lugar de segundos.

== Aleatoriedad reproducible y ruido

`scene.random(seed)` devuelve un generador con semilla (`uniform`, `gauss`,
`integer`, `choice`, `shuffle`). La misma semilla da los mismos valores en
cualquier plataforma, así que puedes esparcir objetos sin perder
reproducibilidad.

`scene.noise(...)` es otra cosa: una señal suave que cambia con el tiempo de la
línea de tiempo, evaluada en Rust en cada fotograma. Se conecta con
`computed(...)` a cualquier propiedad reactiva.

```python
from gaanim import CYAN, GOLD, GRAY, WHITE, Scene, computed

scene = Scene(frame=(16, 9), background="#0f172a")
rng = scene.random(seed=42)
for _ in range(60):
    scene.geometry.dot(rng.uniform(0.02, 0.07)).fill(rng.choice([GRAY, WHITE, CYAN])).move_to(
        rng.uniform(-7.5, 7.5), rng.uniform(-4.2, 4.2)
    )
deriva = scene.noise(frequency=0.8, amplitude=1.0, octaves=3, seed=5)
hoja = scene.geometry.rect(2.4, 0.5).fill(GOLD)
hoja.rotate_to(computed(lambda v: 0.6 * v, inputs=[deriva]))
scene.wait(2.0)
scene.render()
```

`frequency` controla lo rápido que cambia, `amplitude` el rango
(`center ± amplitude`) y `octaves` (1 a 8) añade detalle fino.

== Dar vida: wiggle y osciladores

Para que un objeto quieto respire sin programar cada movimiento, añade un
updater procedural. Ambos se *suman* a las animaciones del objeto en lugar de
reemplazarlas, y `remove_updater()` los termina.

- `Updater.wiggle(position, rotation, scale, frequency, octaves, seed)`:
  temblor orgánico con ruido. Las amplitudes están en unidades de escena,
  radianes y fracción de escala.
- `Updater.oscillate(canal, waveform, frequency, low, high, phase)`: un valor
  periódico sobre `"x"`, `"y"`, `"rotation"`, `"scale"` u `"opacity"`, con
  onda `"sine"`, `"square"`, `"triangle"` o `"saw"`.

```python
from gaanim import CORAL, CYAN, GOLD, Scene, Updater

scene = Scene(frame=(16, 9), background="#0f172a")
logo = scene.geometry.star(5, 1.0, 0.45).fill(GOLD).move_to(-4, 0.5)
luz = scene.geometry.circle(0.8).fill(CYAN).move_to(0, 0.5)
boya = scene.geometry.square(1.0).fill(CORAL).move_to(4, 0.5)
logo.add_updater(Updater.wiggle(position=0.2, rotation=0.2, frequency=1.5, seed=1))
luz.add_updater(Updater.oscillate("opacity", frequency=0.8, low=0.3, high=1.0))
boya.add_updater(Updater.oscillate("y", waveform="triangle", frequency=1.0, low=-0.4, high=0.4))
scene.wait(0.5)
scene.play(logo.animate.move_to(-4, -1).duration(1.0))
scene.render()
```

No confundas `Updater.wiggle` con `animate.wiggle()`: el segundo es un énfasis
breve para señalar un error, no una capa continua.

== Trayectorias y arcos

`animate.move_along(camino)` recorre el contorno de cualquier drawable. Con
`orient=True` el objeto gira con la tangente, así un avión apunta hacia donde
vuela; `rotate_offset` corrige la orientación de tu dibujo y `start`/`end`
eligen el tramo recorrido. Para un recorrido a velocidad constante usa
`Easing.LINEAR`.

Un `move_to` en línea recta puede verse mecánico. `.path_arc(ángulo)` lo curva
en un arco circular que gira `ángulo` radianes (positivo es antihorario).

```python
import math
from gaanim import CYAN, GOLD, GRAY, Scene

scene = Scene(frame=(16, 9), background="#0f172a")
ruta = scene.geometry.polyline([(x / 10, 1.2 * math.sin(x / 10 * 1.3) + 1) for x in range(-65, 66)])
ruta.no_fill().stroke(GRAY, 0.03)
avion = scene.geometry.polygon([(0.45, 0), (-0.3, 0.25), (-0.15, 0), (-0.3, -0.25)]).fill(CYAN)
avion.move_to(-6.5, 1)
pelota = scene.geometry.dot(0.22).fill(GOLD).move_to(-4, -2.5)
scene.play([
    avion.animate.move_along(ruta, orient=True).duration(2.0),
    pelota.animate.move_to(4, -2.5).path_arc(-math.pi / 2).duration(2.0),
])
scene.render()
# output: preview.webp
```

`path_arc` curva el destino de un `move_to` o un `shift_by` de la misma
animación; sin una de esas traslaciones lanza `ValueError`.

== Trim: dibujar trazos por partes

`drawable.trim(start, end, offset)` muestra solo la ventana `[start, end]` del
trazo, en fracciones de longitud. `animate.trim(...)` la anima, y con eso
salen los revelados de line art más habituales:

- Dibujar: empieza en `trim(end=0)` y anima a `end=1`.
- Desde el centro: empieza en `trim(start=0.5, end=0.5)` y abre a `0` y `1`.
- Segmento viajero: una ventana corta y un `offset` animado, que da la vuelta
  en caminos cerrados.
- `mode="sequential"`: los subtrazos aparecen uno tras otro en lugar de a la
  vez.

```python
from gaanim import CORAL, CYAN, GOLD, WHITE, Scene

scene = Scene(frame=(16, 9), background="#0f172a")
anillo = scene.geometry.circle(1.2).no_fill().stroke(CYAN, 0.12).move_to(-5, 0)
anillo.trim(start=0.5, end=0.5)
orbita = scene.geometry.circle(1.2).no_fill().stroke(GOLD, 0.12).move_to(-1.7, 0)
orbita.trim(start=0.0, end=0.15)
guiones = scene.geometry.dashed_line(0.6, 0, 3.4, 0, dash_length=0.3, gap_length=0.2).stroke(CORAL, 0.1)
guiones.trim(end=0.0, mode="sequential")
estrella = scene.geometry.star(5, 1.2, 0.5).no_fill().stroke(WHITE, 0.08).move_to(5.3, 0)
estrella.trim(end=0.0)
scene.play([
    anillo.animate.trim(start=0.0, end=1.0),
    orbita.animate.trim(offset=1.0),
    guiones.animate.trim(end=1.0),
    estrella.animate.trim(end=1.0),
])
scene.wait(0.2)
scene.render()
# output: preview.webp
```

`create`, `write`, `trim` y `show_passing_flash` comparten el mismo canal en
un drawable: no los solapes en el mismo `play`; encadénalos con `sequence`.

== Referencia

- #link("/referencia/animations/")[Animaciones]: `Easing`, `stagger`,
  `distribute`, `repeat`, `loop`, `label`, `move_along`, `path_arc` y `trim`.
- #link("/referencia/scene/")[Scene]: `scene.random`, `scene.noise`,
  `Updater.wiggle`, `Updater.oscillate` y `scene.marker`.
- #link("/referencia/geometria/")[Geometría]: los drawables que sirven de
  trayectoria.
