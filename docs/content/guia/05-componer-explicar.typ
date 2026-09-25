#import "../../components/section.typ": docs-chapter
#import "../../components/tutorial.typ": checkpoint, idea

#docs-chapter(
  title: "Componer y explicar",
  description: "Texto matemático, Layout y una estructura que guía la mirada",
  route: "/guia/componer-explicar/",
)[

= Añadir una explicación, no decorar

La mitad izquierda ya contiene el sistema circular. Reservaremos la derecha
para explicar qué significa. Esta separación es una decisión narrativa: a la
izquierda ocurre el fenómeno; a la derecha lo nombramos.

== Texto matemático con Typst

Gaanim compone matemáticas dentro de `scene.text`. Añade dos objetos:

```python
>>>from gaanim import BLUE, WHITE, YELLOW, Color, Scene
>>>BACKGROUND = Color(15, 23, 42)
>>>PRIMARY = BLUE
>>>ACCENT = YELLOW
>>>MUTED = Color(148, 163, 184)
>>>scene = Scene(frame=(16, 9), background=BACKGROUND, margin=0.6)
>>>title = scene.text("Movimiento circular", role="title").fill(WHITE).move_to(0, 3.25)
>>>caption = scene.text("Un punto, un radio constante", role="subtitle").fill(MUTED).move_to(0, 2.69)
>>>orbit = scene.geometry.circle(1.5).stroke(PRIMARY, 0.05).no_fill().move_to(-4, 0)
>>>radius = scene.geometry.line(-4, 0, -2.5, 0).stroke(MUTED, 0.025)
>>>point = scene.geometry.dot(0.125).fill(ACCENT).move_to(-2.5, 0)
>>>system = scene.geometry.group([orbit, radius, point])
>>>from gaanim import stagger
>>>scene.play(stagger(
>>>    title.animate.write().duration(0.8),
>>>    caption.animate.fade_in().duration(0.6),
>>>    each=0.12,
>>>))
>>>scene.play(stagger(
>>>    orbit.animate.create().duration(1.0),
>>>    radius.animate.create().duration(0.7),
>>>    point.animate.fade_in().duration(0.35),
>>>    each=0.15,
>>>))
>>>scene.wait(0.8)
>>>from gaanim import Easing
>>>scene.play([system.animate.shift_by(0.5, 0).duration(0.6).easing(Easing.SMOOTH)])
>>>scene.play([system.animate.shift_by(-0.5, 0).duration(0.6).easing(Easing.SMOOTH)])
formula = scene.text("$y(t) = r sin(omega t)$", role="subtitle").fill(WHITE)
explanation = scene.text(
    "La altura del punto se convertirá en una curva.",
    role="body",
).fill(MUTED)
```

La cadena conserva el código Python, mientras que el contenido entre `$...$`
se interpreta como matemáticas de Typst.

== De coordenadas sueltas a Layout

Podríamos asignar una coordenada distinta a cada texto, pero perderíamos la
relación entre ellos. Una columna expresa que forman un panel:

```python
# continue
panel = scene.layout.column(
    [formula, explanation],
    gap=0.225,
    align="start",
)
panel.move_to(3.5, 1.875)
```

Layout posee la posición de sus hijos. Después de crear la columna no llames
`at()` sobre `formula` o `explanation`; mueve el contenedor `panel` o usa sus
reglas de configuración.

== Layout local y layout de página

Aquí usamos un layout local: organiza un pequeño grupo y luego colocamos el
grupo como una unidad. Para una página completa se puede usar `within="safe"`,
`width="fill"` y `height="fill"`:

```python
# continue
# Ilustrativo: no lo añadas a main.py.
content = scene.text("Contenido de la página", role="body")
page = scene.layout.stack(
    [content],
    within="safe",
    width="fill",
    height="fill",
    align="center",
)
```

No necesitamos convertir toda la escena ahora. El círculo usa coordenadas
porque su geometría depende de un centro y un radio exactos; el panel usa
Layout porque su geometría depende del contenido.

#idea[
Usa coordenadas para explicar geometría. Usa Layout para organizar contenido.
La buena composición no consiste en elegir uno de los dos, sino en asignar a
cada sistema la responsabilidad adecuada.
]

== Introducir el panel en la timeline

Añade después de la entrada del sistema:

```python
# continue
from gaanim import stagger
scene.play(stagger(
    formula.animate.write().duration(0.8),
    explanation.animate.fade_in().duration(0.6),
    each=0.1,
))
scene.wait(0.8)
```

El espectador ya conoce el círculo cuando aparece la fórmula. El texto no
compite con la primera revelación.

#checkpoint[
El panel debe permanecer a la derecha y sus dos líneas deben conservar una
separación consistente. Si Layout informa ownership errors, busca una llamada
posicional aplicada a uno de sus hijos después de crear la columna.
]

== Siguiente paso

La escena ya plantea una promesa: la altura del punto formará una curva. En el
próximo capítulo haremos que el punto se mueva y que el radio lo siga.
]
