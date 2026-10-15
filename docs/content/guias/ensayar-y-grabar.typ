#import "../../components/section.typ": docs-chapter

#show: docs-chapter.with(
  title: "Ensayar y grabar una charla",
  description: "Del guion a una charla que cabe en su tiempo: practica contra un objetivo, graba tus tiempos y tu voz, y conviértelo en vídeo o en presentación",
  route: "/guias/ensayar-y-grabar/",
  nav: "Ensayar y grabar",
)

En esta guía preparas una charla corta de principio a fin con el panel
*Script & narration* del editor: le pones una duración, la practicas
diapositiva a diapositiva hasta que cabe, grabas tus tiempos y, al final,
la conviertes en un vídeo con tu voz o la presentas en directo. Todo con un
proyecto de ejemplo que puedes copiar.

El flujo es un ciclo:

+ *Escribir* lo que vas a decir en el guion.
+ *Practicar* contra un objetivo, sin grabar nada.
+ *Recortar* donde te pasas: el texto, o el objetivo de esa diapositiva.
+ *Grabar* tus tiempos (y, si quieres, tu voz para escucharte).
+ *Producir*: un vídeo con la voz grabada, o la presentación en directo.

= El proyecto de ejemplo <ejemplo>

Una charla de cinco minutos sobre el ciclo del agua: una portada y cuatro
ideas, cada una con dos stops (el título y la idea). Crea una carpeta con
estos tres archivos, o empieza con `gaanim init` y sustituye su `main.py`.

`gaanim.toml`:

```toml
name = "ciclo-del-agua"
kind = "slides"
entry = "main.py"
```

`main.py`:

```python
from gaanim import Scene, lecture, title_slide

scene = Scene(frame=(16, 9), margin=0.9)
scene.canvas.set_theme("presentation")
scene.slides.brand(footer="EL CICLO DEL AGUA", slide_numbers=True, rule=True)
scene.slides.target(5 * 60)                     # la charla entera: 5 minutos


def idea(name, heading_text, lines, target=None):
    """Una diapositiva con un título y una línea por stop."""
    slide = scene.segment(name, template=lecture, target=target)
    heading = scene.text(heading_text, role="title")
    body = scene.text("\n".join(lines), size=0.42)
    slide.bind(title=heading, body=body)
    scene.play([heading.animate.fade_in().duration(0.4)])
    scene.stop("titulo")
    scene.play([body.animate.write().duration(1.2)])
    scene.stop("idea")


cover = scene.segment("Portada", template=title_slide, target=20)
title = scene.text("El ciclo del agua", role="title")
subtitle = scene.text("Cinco minutos, cuatro ideas", role="subtitle")
cover.bind(title=title, subtitle=subtitle)
scene.play([title.animate.write().duration(0.8), subtitle.animate.fade_in().duration(0.6)])
scene.stop("portada")

idea("Evaporacion", "Evaporación", ["El sol calienta mares y lagos.", "El agua sube como vapor."])
idea("Condensacion", "Condensación", ["El vapor se enfría al subir.", "Se forman las nubes."])
idea("Precipitacion", "Precipitación", ["Las gotas crecen y pesan.", "Cae lluvia, nieve o granizo."])
idea("Cierre", "Y vuelta a empezar", ["El agua llega a ríos y mares.", "El ciclo no se detiene."], target=30)

scene.render()
```

`assets/narration/script.md`: lo que dices en cada diapositiva,
con una línea `[stop nombre]` donde avanzas (ver
#link("/guias/presentaciones/#guion")[El guion de la charla]).

```markdown
# El ciclo del agua

## Portada
Buenas tardes. En cinco minutos les cuento cómo el agua da la vuelta al
planeta una y otra vez.
[stop portada]

## Evaporacion
Todo empieza con el sol.
[stop titulo]
El sol calienta los mares, los lagos y hasta los charcos, y una parte del agua
sube al aire convertida en vapor, sin que la veamos.
[stop idea]

## Condensacion
Ese vapor no se queda arriba para siempre.
[stop titulo]
Al subir, el aire se enfría; el vapor se condensa en gotas diminutas que,
juntas, forman las nubes que vemos pasar.
[stop idea]

## Precipitacion
Las nubes, entonces, empiezan a pesar.
[stop titulo]
Las gotas chocan, se unen y crecen hasta que el aire ya no las sostiene, y
caen como lluvia, nieve o granizo según la temperatura.
[stop idea]

## Cierre
Y aquí el ciclo se cierra.
[stop titulo]
El agua corre por ríos y acuíferos hasta el mar, y el sol vuelve a empezar.
Gracias.
[stop idea]
```

No hace falta escribir el guion a mano: con el panel abierto, el icono del
libro de su cabecera lo crea con una sección por diapositiva y sus marcas, y
lo abre en tu editor. `gaanim check ciclo-del-agua` confirma que guion y
diapositivas coinciden (`Ready to present`).

= Abrir el panel

Abre el proyecto en el editor (`gaanim ciclo-del-agua`, o desde Home) y
pulsa el micrófono de la barra de reproducción, o busca *Script & narration*
en la paleta de comandos (`Ctrl+Shift+P`). El panel se acopla a la derecha y
la escena se encoge a su lado.

#figure(
  image("../../assets/narracion/panel.webp", alt: "El editor con el panel Script & narration acoplado a la derecha: objetivo de 1:00, tomas y última práctica, historial de prácticas, mapa de la charla, guion de la diapositiva actual, lista de diapositivas y controles de grabación"),
  caption: [El panel con las cinco diapositivas grabadas y un objetivo de 1:00:
    tres diapositivas se pasan de su parte (en ámbar).],
)

De arriba abajo: el objetivo, cómo van las tomas y la última práctica, el
mapa de la charla, el guion de la diapositiva en pantalla, la lista de
diapositivas y, abajo, los controles para grabar o practicar. Cada parte se
explica en #link("/guias/presentaciones/#panel-narracion")[El panel Script &
narration].

= Fijar el objetivo

El objetivo es lo que quieres que dure la charla. El ejemplo lo pone en el
código con `scene.slides.target(5 * 60)`, y el panel lo muestra en *TARGET*
con `from scene.slides.target`. Para probar otra duración sin tocar el
código, escríbela en el campo (`4:00`, `4m`, `1h10m`) y pulsa `Enter`: pasa a
`set here`, y Presenter View planifica con ella también. Vaciarla vuelve a la
del código.

Con el objetivo, cada diapositiva recibe su parte (*Share*). La portada y el
cierre tienen `target=` propios (20 y 30 segundos); lo que queda se reparte
entre las otras tres según cuánto dice su guion. La tarjeta *SCRIPT* avisa si
el texto de una diapositiva, leído a tu ritmo, no cabe en su parte
(`Script 0:08 too long`): antes de practicar ya sabes dónde sobra texto.

= Practicar

Practicar es presentar la charla cronometrada sin grabar nada. En los
controles de abajo, elige *Practice* y pulsa *Practise from the start* (o
*From here*, para empezar en la diapositiva en pantalla). Tras una cuenta
atrás de tres segundos, presenta en voz alta:

#table(
  columns: 2,
  [*Tecla*], [*Qué hace*],
  [`→` o `Espacio`], [Avanza al siguiente stop.],
  [`Enter`], [Termina y guarda la práctica.],
  [`Esc`], [La descarta.],
)

#figure(
  image("../../assets/narracion/practicando.webp", alt: "Una práctica en curso: el prompter muestra el guion y una línea de estado con PRACTICE, el nivel del micrófono, 1:26 ahead, 0:39 / 5:00 y 0:20 of 1:20 en Condensacion"),
  caption: [Practicando: el prompter lee el guion y la línea de estado dice
    cómo vas.],
)

El prompter muestra el tramo del guion que toca decir, con las palabras
iluminándose a tu ritmo, y debajo:

- `PRACTICE` y el tiempo desde que empezaste, con el nivel del micrófono.
- `1:26 ahead` o `0:12 behind`: cuánto vas adelantado o retrasado frente a
  las partes del objetivo. Las diapositivas ya dejadas cuentan enteras y la
  actual solo cuando se pasa de la suya.
- `0:39 / 5:00`: el reloj frente al objetivo.
- `0:20 of 1:20 Condensacion`: el tiempo en la diapositiva frente a su parte.
- A la derecha, `▸ idea`: el stop donde toca avanzar.

= Leer el resultado

Al terminar, el panel lo resume (`Practice: 1:07 of 5:00 · 3:53 to spare`),
*LAST PRACTICE* da el total frente al objetivo y *PRACTICE RUNS* añade una
columna por práctica, con una línea dorada en el objetivo: de un vistazo ves
si cada ensayo se acerca.

#figure(
  image("../../assets/narracion/lista.webp", width: 45%, alt: "El panel con la lista en Last practice y el filtro Over: cuatro diapositivas pasadas de su parte, Evaporacion en rojo"),
  caption: [La última práctica, filtrada por las diapositivas que se pasan.],
)

Pon la lista en *Last practice* y pulsa *Over*: quedan solo las diapositivas
que se pasaron de su parte, en ámbar si se pasan un poco (un 10 %, al menos
10 segundos) y en rojo si se pasan más. Para cada una tienes dos salidas:

- *Decir menos*: *Open in the editor* abre el guion en su sección; recorta el
  texto y guarda (el editor actualiza las notas al momento).
- *Darle más tiempo*: súbele el `target=` en el código, y las demás
  diapositivas se reparten lo que queda.

El botón de cada fila practica desde esa diapositiva, para repetir solo el
tramo que corregiste. Repite hasta que *LAST PRACTICE* quede por debajo del
objetivo.

= Grabar los tiempos y la voz

Cuando la charla cabe, grábala. En los controles, elige *Record*:

- *Test level* abre el micrófono y muestra su nivel sin grabar: habla como
  en la charla y busca que los picos caigan en la franja violeta del
  medidor, entre -18 y -6 dB.
- *From the start* presenta desde la primera diapositiva y *Record from here*
  desde la que está en pantalla. Cada diapositiva que recorres entera guarda
  su tiempo y el de cada tramo entre stops.
- *This slide* vuelve a grabar solo la diapositiva en pantalla; el botón de
  grabar de un stop, solo ese tramo.
- *Dictate into the script* escribe en el guion lo que dices, con Scribe
  (ver #link("/guias/presentaciones/#dictar")[Dictar el guion con Scribe]).
- *Keep the audio* guarda tu voz de cada tramo para escucharla con el botón
  de reproducir de cada fila.

#figure(
  image("../../assets/narracion/grabando.webp", alt: "Grabando una diapositiva: el prompter muestra el guion y TIMING, el medidor de nivel, talk ≈ 0:45 of 1:00 y el tiempo en Condensacion"),
  caption: [Grabando: `talk ≈ 0:45 of 1:00` estima lo que duraría la charla
    con lo grabado hasta ahora.],
)

Al grabar, la línea de estado dice `TIMING` y `talk ≈ 0:45 of 1:00`: lo que
duraría la charla con esta grabación y los tiempos guardados del resto. Si el
micrófono suena bajo o satura, el prompter se enmarca y lo avisa.

Los tiempos y el audio de *Keep the audio* son para ti: se guardan en la
carpeta de datos de Gaanim, un archivo WAV por tramo, nunca en el proyecto,
y no acaban en una exportación.

= Del ensayo al vídeo

Para que tu voz forme parte del vídeo, grábala como una toma en vivo: añade
`scene.live_take("charla")` antes de la primera diapositiva.

```python
>>>from gaanim import Scene, title_slide
>>>scene = Scene(frame=(16, 9))
scene.live_take("charla")
cover = scene.segment("Portada", template=title_slide, target=20)
>>>scene.wait(1)
scene.stop("portada")
>>>scene.render()
```

Con la escena recargada, el panel muestra al final la tarjeta *LIVE TAKE*.
*Record live take* presenta la escena desde el principio mientras grabas,
igual que en la práctica: `→` o `Espacio` avanzan, `Enter` termina. La toma
se guarda en `narration/charla.wav` del proyecto y cada stop pasa a durar lo
que tardaste en él, así que la escena se reproduce de corrido con tu voz.
Después exporta:

```bash
gaanim export ciclo-del-agua -o ciclo-del-agua.mp4
```

Practica antes de grabar la toma: una toma en vivo se graba siempre entera,
y el ciclo de practicar y recortar es más rápido que repetirla. Si prefieres
grabar bloque a bloque, usa voz en off (ver
#link("/referencia/audio/#narracion")[Narración]).

= Del ensayo a la presentación

Para presentar en directo, el objetivo y las prácticas te acompañan:

```bash
gaanim --present ciclo-del-agua
```

Presenter View (ver #link("/guias/presentaciones/#tiempo-de-la-charla")[El
tiempo de la charla]) cuenta hacia atrás el objetivo y dice si vas
adelantado o retrasado, con el mismo plan que fijaste en el panel. Cada
presentación de más de 30 segundos cuenta como una práctica más, y aparece
en *PRACTICE RUNS* la próxima vez que abras el panel. Para repartir la charla,
`gaanim export ciclo-del-agua -o charla.gaanim` la guarda en un solo archivo
que se presenta sin Python, y `-o charla.pdf --notes` da las diapositivas con
tu guion (ver #link("/guias/compartir/")[Compartir]).
