#import "../../components/section.typ": docs-chapter
#import "../../components/tutorial.typ": checkpoint, idea

#docs-chapter(
  title: "Primera escena",
  description: "El lienzo, las coordenadas y el primer fotograma del proyecto",
  route: "/tutorial/primera-escena/",
)[

= Objetivo

Al terminar este capítulo, `main.py` mostrará un fotograma quieto: el título,
un círculo a la izquierda, un punto amarillo en su borde derecho y el radio que
los une. Todavía no habrá movimiento.

Abre `main.py` y borra su contenido: vamos a escribir la escena desde cero.

= Cambios

== El lienzo

```python
from gaanim import CYAN, WHITE, YELLOW, Scene

scene = Scene(frame=(16, 9), background="#0f172a", margin=0.6)

CENTER = (-4.5, -1.0)
R = 1.5
```

`Scene(frame=(16, 9), ...)` crea un lienzo horizontal de 16 unidades de ancho y
9 de alto. El origen `(0, 0)` está en el centro: X crece hacia la derecha e Y
hacia arriba, así que X va de `-8` a `8` e Y de `-4.5` a `4.5`. `margin=0.6`
reserva un borde de seguridad de 0.6 unidades para que nada quede pegado al
límite del fotograma.

Estas unidades son lógicas y no dependen de la resolución: la misma escena se
exporta a 1280 × 720 o a 3840 × 2160 sin cambiar el código. Radios, grosores de
trazo y tamaños de texto usan la misma unidad.

`CENTER` y `R` guardan el centro y el radio del círculo. Toda la geometría del
tutorial se calculará a partir de ellos: si cambias uno, todo lo demás lo
acompaña.

== El título

```python
# continue
title = scene.text("Del círculo al seno", role="title")
title.fill(WHITE).move_to(0, 3.4)
```

`scene.text` crea texto vectorial. El rol `"title"` elige el tamaño de un
título; `fill` le da color y `move_to` coloca su centro cerca del borde
superior.

La variable `title` no contiene una imagen ni unas coordenadas: es un _handle_,
una referencia al objeto que ya vive en la escena. Con él seguiremos
modificándolo y, más adelante, animándolo.

== El círculo y el punto

```python
# continue
orbit = scene.geometry.circle(R)
orbit.stroke(CYAN, 0.05).no_fill().move_to(*CENTER)

point = scene.geometry.dot(0.125)
point.fill(YELLOW).move_to(CENTER[0] + R, CENTER[1])
```

`scene.geometry` agrupa las fábricas de formas. El círculo solo tiene trazo
(`stroke` con color y grosor, y `no_fill`) porque representa una trayectoria.
`move_to(*CENTER)` desempaqueta la tupla en sus dos coordenadas.

El punto se coloca a `R` unidades a la derecha del centro, es decir, justo
sobre el borde del círculo.

#idea[
Nombra los objetos por su papel, no por su aspecto. `orbit` sigue siendo un buen
nombre aunque en el próximo capítulo cambie de color; `cyan_circle` dejaría de
serlo.
]

== El radio

```python
# continue
radius = scene.geometry.line(CENTER, point).stroke(WHITE, 0.025)
```

`scene.geometry.line` acepta coordenadas u objetos como extremos. Aquí el
origen es el punto fijo `CENTER`, pero el final es el handle `point`: la línea
termina siempre donde esté el punto. Hoy eso no se nota, porque el punto está
quieto; en el capítulo 6, cuando el punto gire, el radio lo seguirá sin tocar
esta línea.

== Mantener el fotograma

Termina el archivo con:

```python
# continue
scene.wait(1)
scene.render()
```

`scene.render()` entrega la escena al editor y debe ser siempre la última
línea. `scene.wait(1)` mantiene el fotograma durante un segundo: una escena sin
duración no tiene nada que exportar. En el capítulo 4 sustituiremos esa espera
por animaciones.

= Archivo completo

```python
# output: preview.webp
from gaanim import CYAN, WHITE, YELLOW, Scene

scene = Scene(frame=(16, 9), background="#0f172a", margin=0.6)

CENTER = (-4.5, -1.0)
R = 1.5

title = scene.text("Del círculo al seno", role="title")
title.fill(WHITE).move_to(0, 3.4)

orbit = scene.geometry.circle(R)
orbit.stroke(CYAN, 0.05).no_fill().move_to(*CENTER)

point = scene.geometry.dot(0.125)
point.fill(YELLOW).move_to(CENTER[0] + R, CENTER[1])

radius = scene.geometry.line(CENTER, point).stroke(WHITE, 0.025)

scene.wait(1)
scene.render()
```

#checkpoint[
Al guardar, el editor muestra el título arriba, un círculo cian a la izquierda
y un radio blanco horizontal que termina en el punto amarillo, sobre el borde
derecho del círculo. Si el punto no toca el círculo, revisa que su posición use
`CENTER[0] + R`.
]

En el siguiente capítulo, #link("/tutorial/objetos-estilo/")[Objetos y estilo],
convertiremos este boceto en una composición con jerarquía visual.
]
