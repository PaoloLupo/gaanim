#import "../../components/section.typ": docs-chapter

#show: docs-chapter.with(
  title: "Tipografía cinética",
  description: "Escritura por unidades, animador de rango, revelados con máscara, máquina de escribir, scramble y resaltado",
  route: "/guias/tipografia-cinetica/",
  nav: "Tipografía cinética",
)

= Texto que se mueve con intención

En esta guía animas texto por grafemas, palabras o líneas: revelados
editoriales detrás de una máscara, olas que recorren un titular, una terminal
que escribe sola, un código que se descifra y un resaltador que subraya una
idea.

Todas estas animaciones trabajan sobre el `Text` ya compuesto: el párrafo no
se vuelve a maquetar en cada fotograma y un seek a cualquier instante coincide
con la reproducción.

```python
from gaanim import WHITE, Scene

scene = Scene(frame=(16, 9), background="#0f172a")
titular = scene.text("Cada línea sube\ndesde su máscara", role="title").fill(WHITE).scale_by(1.6)
scene.play(titular.animate.reveal(by="line", style="slide_up", stagger=0.15).duration(1.0))
scene.wait(0.3)
scene.render()
# output: preview.webp
```

== Las unidades: grafema, palabra, línea y parte

Todas las técnicas de esta guía eligen una unidad con `by`:

- `"grapheme"`: cada carácter visible, incluidos los compuestos como "é".
- `"word"`: palabras según los límites de Unicode; la puntuación se une a su
  vecina.
- `"line"`: líneas explícitas, separadas con `\n` en el texto.
- `"part"`: las partes semánticas que declaras con `part(...)`.

Son las mismas unidades que `text.graphemes`, `text.words`, `text.lines` y
`text.parts`, así que lo que seleccionas y lo que animas coincide siempre.

== Escribir por unidades

`text.animate.write()` traza los glifos uno tras otro. Con `by` los glifos de
una misma unidad empiezan juntos, con `order` eliges qué unidad va primero
(`"forward"`, `"reverse"`, `"center"` o `"random"`) y `stagger` es el desfase
entre unidades consecutivas.

```python
from gaanim import GOLD, WHITE, Scene, part

scene = Scene(frame=(16, 9), background="#0f172a")
frase = scene.text("Escribir desde el centro hacia fuera").fill(WHITE).move_to(0, 1.5)
ecuacion = scene.text("$", part("e", "E"), " = ", part("m", "m"), " ", part("c", "c^2"), "$").fill(GOLD)
ecuacion.move_to(0, -1)
scene.play(frase.animate.write(by="word", order="center", stagger=0.08).duration(1.2))
scene.play(ecuacion.animate.write(by="part", stagger=0.1).duration(1.0))
scene.render()
```

`write` es la opción natural para ecuaciones y trazos de pluma. Para texto de
lectura, un revelado suele ser más limpio.

== Revelados con máscara

`text.animate.reveal(style, by="line", stagger=0.06)` es el movimiento
editorial por excelencia: cada unidad entra escalonada. Los estilos:

#table(
  columns: (1fr, 2.2fr),
  inset: 7pt,
  [*Estilo*], [*Cuándo usarlo*],
  [`"slide_up"`], [Titulares y citas: cada unidad sube desde detrás de una máscara recortada a su fila. Es el predeterminado.],
  [`"slide_down"`], [Lo mismo, cayendo desde arriba; combina con entradas desde el borde superior.],
  [`"fade"`], [Texto largo o secundario, cuando el movimiento distraería.],
  [`"scale"`], [Palabras sueltas o etiquetas que "aparecen" con energía.],
  [`"blur"`], [Un tono suave y cinematográfico.],
)

La máscara es vectorial, así que el recorte funciona también al exportar a
SVG. Con `mask=False` los deslizamientos recorren menos distancia y entran con
un fundido. La duración cubre toda la cascada: si el `stagger` no cabe, se
comprime. `conceal` es la salida simétrica.

```python
from gaanim import GOLD, WHITE, Scene

scene = Scene(frame=(16, 9), background="#0f172a")
lineas = scene.text("Tres líneas\nque suben\ny se van").fill(WHITE).move_to(-4, 0)
palabras = scene.text("palabra por palabra").fill(GOLD).move_to(3.5, 1)
suave = scene.text("con desenfoque").fill(WHITE).move_to(3.5, -1)
scene.play([
    lineas.animate.reveal(by="line", style="slide_up", stagger=0.15).duration(1.2),
    palabras.animate.reveal(by="word", style="slide_down", stagger=0.12).duration(1.2),
    suave.animate.reveal(by="word", style="blur", stagger=0.1).duration(1.2),
])
scene.wait(0.4)
scene.play(lineas.animate.conceal(by="line", style="slide_up", stagger=0.15).duration(1.0))
scene.render()
```

Sobre una selección (`ecuacion["rhs"].animate.reveal("from_below")`),
`reveal` hace aparecer solo esos glifos con `"fade"`, `"wipe"` o
`"from_below"`; lo verás en la sección «Marcar y anotar», más abajo.

== El animador de rango

`reveal`, `conceal` y `blur_in` son presets de una primitiva más general: el
animador de rango, al estilo de los Text Animators de After Effects.

1. `text.animator(by, shape, order, seed)` crea el selector.
2. `.set(...)` define el estado "fuera": `offset`, `opacity`, `scale`,
   `rotation`, `blur`, `tracking` y `color`.
3. `.animate.sweep()` recorre el rango de `0` (antes de la primera unidad) a
   `1` (después de la última).

`shape` decide qué pasa en cada unidad. `"square"`, `"ramp"`, `"smooth"`,
`"ease_in"` y `"ease_out"` llevan cada unidad del estado "fuera" al reposo:
son entradas. `"triangle"` y `"round"` suben al estado "fuera" y vuelven: son
olas que pasan.

```python
from gaanim import CYAN, GOLD, WHITE, Scene

scene = Scene(frame=(16, 9), background="#0f172a")
cascada = scene.text("Tipografía cinética", role="title").fill(WHITE).scale_by(1.6).move_to(0, 1.2)
entrada = cascada.animator(by="grapheme", shape="smooth")
entrada.set(offset=(0, -0.5), opacity=0.0, scale=0.6, rotation=0.3)
ola_texto = scene.text("una ola que pasa").fill(CYAN).scale_by(2.0).move_to(0, -1.2)
ola = ola_texto.animator(by="grapheme", shape="round", order="center").set(offset=(0, 0.35), color=GOLD)
scene.play([
    entrada.animate.sweep().duration(1.2),
    ola.animate.sweep().duration(1.2),
])
scene.wait(0.2)
scene.render()
# output: preview.webp
```

`sweep(1, 0)` recorre el rango al revés y `stagger` fija el retraso entre
unidades en segundos. Usa el animador cuando ningún preset describe lo que
buscas; si uno lo hace, el preset es más corto y más fácil de leer.

== Máquina de escribir

`text.animate.typewriter(cps=18, cursor="▍")` escribe el texto a un ritmo de
pulsaciones por segundo, con una pequeña irregularidad (`jitter`) fijada por
`seed` para que parezca humano. El cursor sigue la última letra y parpadea
cuando termina (`blink`). `backspace(n)` borra desde el final y `retype(texto)`
conserva el prefijo común y escribe el resto.

```python
from gaanim import CYAN, Scene

scene = Scene(frame=(16, 9), background="#0f172a")
prompt = scene.text("> gaanim render", role="code").fill(CYAN).scale_by(2).move_to(-3, 0)
scene.play(prompt.animate.typewriter(cps=18, cursor="▍", seed=7))
scene.wait(0.5)
scene.render()
# output: preview.webp
```

Sin duración explícita, cada animación dura hasta su última pulsación. Usa el
rol `code`: su fuente monoespaciada mantiene alineados el cursor y los
caracteres.

```python
# continue
scene.play(prompt.animate.backspace(6))
scene.play(prompt.animate.retype("> gaanim export --from clímax"))
scene.render()
```

== Scramble: descifrar un texto

`text.animate.scramble(charset, reveal_delay, speed, seed)` hace ciclar cada
carácter por glifos al azar antes de fijarlo, de izquierda a derecha. Da un
tono técnico a títulos, códigos y rótulos. `charset` acepta `"upper"`,
`"lower"`, `"digits"`, `"hex"`, `"symbols"` o una cadena literal como `"01"`.
`scramble_to(texto)` descifra hacia un texto nuevo.

```python
from gaanim import GOLD, Scene

scene = Scene(frame=(16, 9), background="#0f172a")
rotulo = scene.text("ACCESO CONCEDIDO", role="title").fill(GOLD).scale_by(1.6)
scene.play(rotulo.animate.scramble(charset="upper", reveal_delay=0.3, speed=20, seed=1).duration(1.2))
scene.wait(0.3)
scene.render()
# output: preview.webp
```

El ancho del texto final se reserva desde el primer fotograma, así que nada
salta mientras los caracteres cambian.

== Blur-in y tracking

`blur_in(sigma, by, stagger)` hace entrar cada unidad desde un desenfoque
transparente. `tracking(valor)` añade espacio entre glifos sin recomponer el
párrafo; animarlo hasta `0` "cierra" un título abierto, un recurso clásico de
créditos y cabeceras.

```python
from gaanim import WHITE, Scene

scene = Scene(frame=(16, 9), background="#0f172a")
enfoque = scene.text("ENFOQUE", role="title").fill(WHITE)
enfoque.tracking(0.5)
scene.play([
    enfoque.animate.blur_in(sigma=0.3, by="grapheme", stagger=0.05).duration(1.4),
    enfoque.animate.tracking(0.0).duration(1.4),
])
scene.render()
```

`tracking` y un `reveal` que desliza escriben el mismo canal de cada glifo:
`scene.play` rechaza esa combinación. Combina `tracking` con animaciones que
no mueven glifos, como `blur_in`.

== Marcar y anotar

Para señalar una idea dentro de un texto que ya está en pantalla, anima una
selección:

- `marker(color, skew, opacity)`: una banda de resaltador que crece desde la
  izquierda detrás de cada línea seleccionada. `selection.marker(...)` la
  coloca sin animación.
- `reveal("from_below")`: hace aparecer solo un término de una ecuación.
- `brace("etiqueta")` y `annotate("texto", offset=(x, y))`: una llave o una
  línea guía con etiqueta, que se quedan en la escena.

```python
from gaanim import CORAL, GOLD, GREEN, Scene, part

INK = "#1f2937"
scene = Scene(frame=(16, 9), background="#f8f5ee")
cita = scene.text("Lo que no se mide no se puede mejorar", color=INK, size=0.6).move_to(0, 2)
scene.play(cita.words[5:8].animate.marker().duration(1.0))
scene.play(cita.words[0:2].animate.marker(GREEN, skew=-0.04, opacity=0.4).duration(0.6))

ecuacion = scene.text("$", part("e", "E"), " = ", part("m", "m"), " ", part("c", "c^2"), "$").fill(INK)
ecuacion.move_to(0, -1)
ecuacion["m"].fill(GOLD)
ecuacion["c"].fill(CORAL)
scene.play(ecuacion["c"].animate.reveal("from_below").duration(0.6))
scene.play(ecuacion["m"].animate.brace("masa").duration(0.7))
scene.play(ecuacion["c"].animate.annotate("velocidad de la luz", offset=(2.5, 1.0)).duration(0.7))
scene.render()
```

Las bandas se dibujan detrás de los glifos pero encima de lo que creaste antes
que el texto. Con `blend="multiply"` la banda se multiplica con el fondo o la
tarjeta que tiene debajo, como tinta sobre papel: realza sobre fondos claros y
desaparece sobre negro.

== Referencia

- #link("/referencia/text/")[Texto]: unidades y selecciones, `write`,
  `reveal`, `conceal`, `blur_in`, `tracking`, `Text.animator` y el
  resaltador.
- #link("/referencia/animations/")[Animaciones]: `typewriter`, `backspace`,
  `retype`, `scramble` y `scramble_to`.
