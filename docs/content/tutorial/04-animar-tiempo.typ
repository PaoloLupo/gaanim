#import "../../components/section.typ": docs-chapter
#import "../../components/tutorial.typ": checkpoint, idea

#docs-chapter(
  title: "Animar el tiempo",
  description: "La línea de tiempo: scene.play, duraciones, stagger, easing y pausas",
  route: "/tutorial/animar-tiempo/",
)[

= Objetivo

Hasta ahora todo aparecía de golpe. Al terminar este capítulo la escena
contará su contenido en orden: primero se escribe el título, después se dibuja
la órbita, crece el radio y el punto entra con un pequeño rebote. La animación
durará tres segundos.

= Cambios

== Nuevos imports

Amplía el import:

```python
from gaanim import WHITE, YELLOW, Color, Easing, Scene, stagger
>>># Contexto mínimo para validar los fragmentos de esta página.
>>>scene = Scene(frame=(16, 9))
>>>title = scene.text("Del círculo al seno", role="title")
>>>caption = scene.text("Un punto que gira a radio constante", role="subtitle")
>>>orbit = scene.geometry.circle(1.5)
>>>point = scene.geometry.dot(0.125)
>>>radius = scene.geometry.line((0, 0), point)
```

`stagger` escalona animaciones y `Easing` elige cómo avanzan en el tiempo.

== La entrada del texto

Añade una zona nueva, `# Línea de tiempo`, justo antes de `scene.wait(1)`:

```python
# continue
# Línea de tiempo
scene.play(stagger(
    title.animate.write().duration(0.8),
    caption.animate.fade_in().duration(0.5),
    each=0.5,
))
```

Cada objeto tiene una propiedad `.animate` con el mismo vocabulario que sus
métodos, más las animaciones de entrada y salida. `title.animate.write()` no
cambia nada todavía: devuelve una animación, un objeto `Anim` que describe qué
ocurrirá. `.duration(0.8)` fija cuántos segundos dura.

`scene.play(...)` coloca esas animaciones en la línea de tiempo, a partir del
instante actual. `stagger(..., each=0.5)` las lanza una tras otra con medio
segundo entre inicios: el título empieza en 0 s y el subtítulo en 0.5 s. El
bloque termina cuando acaba su última animación, en el segundo 1.

== La entrada del círculo

Justo debajo, añade la segunda entrada:

```python
# continue
scene.play(stagger(
    orbit.animate.create().duration(0.8),
    radius.animate.create().duration(0.4),
    point.animate.grow_from_center().duration(0.5).easing(Easing.SNAPPY),
    each=0.25,
))
```

Cada llamada a `scene.play` empieza cuando termina la anterior, así que esta
entrada ocupa del segundo 1 al 2. `create()` dibuja el trazo desde su inicio:
la órbita se cierra en 0.8 s y el radio crece desde el centro hacia el punto.

`grow_from_center()` hace crecer el punto desde tamaño cero. Por defecto, las
animaciones aceleran al empezar y frenan al terminar (`Easing.SMOOTH`).
`Easing.SNAPPY` es un muelle: el punto se pasa un poco de su tamaño y vuelve,
lo justo para atraer la mirada.

== Una pausa final

La línea `scene.wait(1)` que ya tenías cambia de sentido: ahora deja un segundo
para mirar el resultado cuando termina la segunda entrada.

#idea[
Un objeto con animación de entrada permanece oculto hasta que esta empieza.
Por eso los objetos se crean todos arriba, en su estado final, y la línea de
tiempo solo decide cuándo aparecen. Gaanim conoce así la escena completa y
puede mostrar cualquier instante exacto al mover la barra de reproducción.
]

= Archivo completo

```python
# output: preview.webp
from gaanim import WHITE, YELLOW, Color, Easing, Scene, stagger

# Paleta
BACKGROUND = Color(15, 23, 42)
PRIMARY = Color(96, 165, 250)
ACCENT = YELLOW
MUTED = Color(148, 163, 184)

scene = Scene(frame=(16, 9), background=BACKGROUND, margin=0.6)

CENTER = (-4.5, -1.0)
R = 1.5

# Texto
title = scene.text("Del círculo al seno", role="title")
title.fill(WHITE).move_to(0, 3.4)
caption = scene.text("Un punto que gira a radio constante", role="subtitle")
caption.fill(MUTED).move_to(0, 2.8)

# Círculo
orbit = scene.geometry.circle(R).stroke(PRIMARY, 0.05).no_fill().move_to(*CENTER)
point = scene.geometry.dot(0.125).fill(ACCENT).move_to(CENTER[0] + R, CENTER[1])
radius = scene.geometry.line(CENTER, point).stroke(MUTED, 0.025)
point.z_index(1)

# Línea de tiempo
scene.play(stagger(
    title.animate.write().duration(0.8),
    caption.animate.fade_in().duration(0.5),
    each=0.5,
))
scene.play(stagger(
    orbit.animate.create().duration(0.8),
    radius.animate.create().duration(0.4),
    point.animate.grow_from_center().duration(0.5).easing(Easing.SNAPPY),
    each=0.25,
))
scene.wait(1)
scene.render()
```

#checkpoint[
La vista previa dura 3 segundos. En el primero se escribe el título y aparece
el subtítulo; en el segundo se dibuja la órbita, crece el radio y el punto
entra con un rebote; en el tercero la escena queda quieta. Arrastra la barra de
reproducción del editor hasta 0.5 s: la órbita todavía no debe verse.
]

En el siguiente capítulo, #link("/tutorial/componer-explicar/")[Componer y
explicar], añadiremos una fórmula y aprenderemos cuándo usar Layout en lugar de
coordenadas.
]
