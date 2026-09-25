#import "../../components/section.typ": docs-chapter
#import "../../components/tutorial.typ": checkpoint, idea

#docs-chapter(
  title: "Componer y explicar",
  description: "Texto matemático, un panel con Layout y un reparto claro del espacio",
  route: "/tutorial/componer-explicar/",
)[

= Objetivo

La izquierda ya contiene el círculo. Al terminar este capítulo, arriba a la
derecha aparecerá un panel con la fórmula $y = r sin(theta)$ y una frase que
anuncia lo que ocurrirá: la altura del punto dibujará una onda. El panel se
escribirá justo después de la entrada del círculo.

El espacio queda repartido con intención: a la izquierda ocurre el fenómeno,
arriba a la derecha lo nombramos y debajo del panel queda sitio libre para la
onda del capítulo 7.

= Cambios

== Texto matemático

Añade al final de la zona `# Texto`:

```python
>>># Contexto mínimo para validar los fragmentos de esta página.
>>>from gaanim import WHITE, Color, Scene, stagger
>>>MUTED = Color(148, 163, 184)
>>>scene = Scene(frame=(16, 9))

formula = scene.text("$y = r sin(theta)$", role="subtitle").fill(WHITE)
explanation = scene.text("La altura del punto dibuja la onda.", role="body")
explanation.fill(MUTED)
```

`scene.text` compone con Typst lo que va entre `$...$`: `sin` se escribe en
redonda, `theta` se convierte en θ y las variables van en cursiva. El resto de
la cadena es texto normal.

Estos dos textos no llevan `move_to`: su posición la decidirá el panel.

== Un panel con Layout

Justo debajo, agrupa los dos textos en una columna:

```python
# continue
panel = scene.layout.column([formula, explanation], gap=0.2, width=6.5)
panel.move_to(3.5, 1.6)
```

`scene.layout.column` apila sus hijos de arriba abajo, separados por `gap`
unidades y alineados a la izquierda. `width=6.5` fija el ancho del panel: si la
frase no cupiera, se partiría en varias líneas dentro de ese ancho. Después
movemos el panel entero; sus hijos lo acompañan.

A partir de aquí, el panel es el dueño de la posición de `formula` y
`explanation`. No llames a `move_to` sobre ellos: Gaanim lo rechaza con un
`LayoutOwnershipError`. Para recolocarlos, mueve `panel`.

#idea[
Usa coordenadas para la geometría y Layout para el contenido. El círculo
depende de un centro y un radio exactos, así que usa coordenadas. El panel
depende de lo que miden sus textos, así que usa Layout: si cambias la frase, la
columna se recalcula sola.
]

== La entrada del panel

En la zona `# Línea de tiempo`, añade una tercera entrada antes de
`scene.wait(1)`:

```python
# continue
scene.play(stagger(
    formula.animate.write().duration(0.8),
    explanation.animate.write().duration(0.8),
    each=0.2,
))
```

El panel ocupa del segundo 2 al 3, cuando el espectador ya conoce el círculo:
la explicación llega después del fenómeno y no compite con él.

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

formula = scene.text("$y = r sin(theta)$", role="subtitle").fill(WHITE)
explanation = scene.text("La altura del punto dibuja la onda.", role="body")
explanation.fill(MUTED)
panel = scene.layout.column([formula, explanation], gap=0.2, width=6.5)
panel.move_to(3.5, 1.6)

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
scene.play(stagger(
    formula.animate.write().duration(0.8),
    explanation.animate.write().duration(0.8),
    each=0.2,
))
scene.wait(1)
scene.render()
```

#checkpoint[
La vista previa dura 4 segundos. Entre el segundo 2 y el 3 se escriben la
fórmula y la frase, alineadas a la izquierda y en una sola línea cada una,
arriba a la derecha y por debajo del subtítulo. Bajo el panel queda un espacio
vacío: lo ocupará la onda.
]

En el siguiente capítulo, #link("/tutorial/reactividad/")[Dar vida a la
escena], el punto empezará a girar y el radio lo seguirá.
]
