#import "../../components/section.typ": docs-chapter
#import "../../components/tutorial.typ": checkpoint, idea

#docs-chapter(
  title: "Objetos y estilo",
  description: "Una paleta con propósito, rellenos, trazos, texto secundario y orden de dibujo",
  route: "/tutorial/objetos-estilo/",
)[

= Objetivo

El primer fotograma funciona, pero todos sus elementos compiten por la atención.
Al terminar este capítulo tendrá una jerarquía clara: la órbita define el
espacio, el punto amarillo es el foco, el radio queda en segundo plano y un
subtítulo gris explica la escena. Seguirá sin haber movimiento.

= Cambios

== Una paleta con propósito

Sustituye el import y la línea de `Scene` por:

```python
from gaanim import WHITE, YELLOW, Color, Scene

# Paleta
BACKGROUND = Color(15, 23, 42)
PRIMARY = Color(96, 165, 250)
ACCENT = YELLOW
MUTED = Color(148, 163, 184)

scene = Scene(frame=(16, 9), background=BACKGROUND, margin=0.6)

CENTER = (-4.5, -1.0)
R = 1.5
```

`Color(r, g, b)` crea un color a partir de sus componentes de 0 a 255; el
fondo es el mismo azul noche de antes. Los nombres dicen para qué sirve cada
color, no cómo es: `PRIMARY` para la geometría principal, `ACCENT` para lo que
debe mirar el espectador y `MUTED` para lo secundario. Si mañana cambias la
identidad visual, solo tocas la paleta.

`CYAN` ya no se usa, así que desaparece del import.

== Texto principal y secundario

En la zona `# Texto`, añade un subtítulo debajo del título:

```python
# continue
# Texto
title = scene.text("Del círculo al seno", role="title")
title.fill(WHITE).move_to(0, 3.4)
caption = scene.text("Un punto que gira a radio constante", role="subtitle")
caption.fill(MUTED).move_to(0, 2.8)
```

Los roles conectan el texto con la tipografía del tema: `"subtitle"` es más
pequeño que `"title"`. El color `MUTED` termina de marcarlo como información
secundaria.

== Relleno, trazo y color

Reescribe la geometría con la paleta. Ahora cada objeto cabe en una sola
cadena de llamadas:

```python
# continue
# Círculo
orbit = scene.geometry.circle(R).stroke(PRIMARY, 0.05).no_fill().move_to(*CENTER)
point = scene.geometry.dot(0.125).fill(ACCENT).move_to(CENTER[0] + R, CENTER[1])
radius = scene.geometry.line(CENTER, point).stroke(MUTED, 0.025)
```

Una forma vectorial puede tener relleno (`fill`), trazo (`stroke`) o ambos. La
órbita solo tiene trazo porque es una trayectoria; el punto solo tiene relleno
porque es una masa. El radio usa la mitad de grosor que la órbita y un color
apagado: está para explicar, no para protagonizar.

== El orden de dibujo

Si te fijas en el capítulo anterior, el extremo del radio se dibujaba encima
del punto. Por defecto, lo que se crea después queda encima, y `radius` tiene
que crearse después de `point` porque lo usa como extremo. Súbelo de capa:

```python
# continue
point.z_index(1)
```

`z_index` fija la capa de dibujo: los valores más altos se dibujan encima. Con
`1`, el punto queda por delante del radio y de la órbita, que siguen en la capa
`0`.

#idea[
El estilo también es contenido. Un color de acento y un trazo más fino ya le
dicen al espectador dónde mirar antes de que ocurra nada.
]

= Archivo completo

```python
# output: preview.webp
from gaanim import WHITE, YELLOW, Color, Scene

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

scene.wait(1)
scene.render()
```

#checkpoint[
La órbita es azul claro, el radio gris y más fino, y el punto amarillo se ve
entero, por encima del extremo del radio. Bajo el título aparece el subtítulo
«Un punto que gira a radio constante» en gris.
]

En el siguiente capítulo, #link("/tutorial/animar-tiempo/")[Animar el tiempo],
haremos que cada elemento entre en escena en su momento.
]
