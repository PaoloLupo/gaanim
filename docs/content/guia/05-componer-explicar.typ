#import "../../components/section.typ": docs-chapter
#import "../../components/tutorial.typ": checkpoint, idea

#docs-chapter(
  title: "Componer y explicar",
  description: "Texto matemático, Layout y una estructura que guía la mirada",
  route: "/guia/componer-explicar/",
  updated: datetime.today().display(),
  code-langs: (),
)[

= Añadir una explicación, no decorar

La mitad izquierda ya contiene el sistema circular. Reservaremos la derecha
para explicar qué significa. Esta separación es una decisión narrativa: a la
izquierda ocurre el fenómeno; a la derecha lo nombramos.

== Texto matemático con Typst

Gaanim compone matemáticas dentro de `scene.text`. Añade dos objetos:

```python
formula = scene.text("$x(t) = r cos(omega t)$", role="subtitle").fill(WHITE)
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
panel = scene.layout.column(
    [formula, explanation],
    gap=18,
    align="start",
)
panel.move_to(280, 150)
```

Layout posee la posición de sus hijos. Después de crear la columna no llames
`at()` sobre `formula` o `explanation`; mueve el contenedor `panel` o usa sus
reglas de configuración.

== Layout local y layout de página

Aquí usamos un layout local: organiza un pequeño grupo y luego colocamos el
grupo como una unidad. Para una página completa se puede usar `within="safe"`,
`width="fill"` y `height="fill"`:

```python
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
