#import "../../components/section.typ": docs-chapter

#show: docs-chapter.with(
  title: "Presentaciones",
  description: "Crea, ensaya y presenta diapositivas con notas y pausas",
  route: "/guias/presentaciones/",
)

En esta guía aprenderás a preparar una charla con Gaanim: diapositivas con
plantillas reutilizables, notas del orador, pausas donde esperar a que hables,
el guion de la charla en un archivo aparte, ensayos con un teleprompter y la
vista del presentador en un segundo monitor.

```python
# output: preview.webp
from gaanim import Scene, lecture, title_slide

scene = Scene(frame=(16, 9), margin=0.9)
scene.canvas.set_theme("presentation")
scene.slides.brand(footer="MI CHARLA", slide_numbers=True, rule=True)

cover = scene.segment("Portada", template=title_slide, notes="Presenta el tema.")
title = scene.text("Una idea clara", role="title")
subtitle = scene.text("Diapositivas reutilizables", role="subtitle")
cover.bind(title=title, subtitle=subtitle)
scene.play([title.animate.write().duration(0.8), subtitle.animate.fade_in().duration(0.6)])
scene.stop("portada")

content = scene.segment("Contenido", template=lecture, notes="Desarrolla la idea.")
heading = scene.text("Contenido", role="title")
body = scene.text("Una sola idea por diapositiva.", size=0.42)
content.bind(title=heading, body=body)
scene.play([heading.animate.fade_in().duration(0.4)])
scene.stop("titulo")
scene.play([body.animate.write().duration(0.8)])
scene.stop("mensaje")
scene.render()
```

La vista previa se reproduce de corrido porque, fuera del modo presentación,
las pausas no detienen nada. Al presentar, Gaanim espera en cada
`scene.stop(...)` hasta que avanzas.

= Crear el proyecto

```bash
gaanim init slides mi-charla
gaanim mi-charla
```

También puedes ejecutar `gaanim` sin argumentos, elegir *Nuevo proyecto* y
seleccionar *Slides*. Si aún no tienes Gaanim listo, sigue
#link("/empezar/instalacion/")[Instalación]. El proyecto de ejemplo no incluye
colores, fuentes ni contenido de ninguna institución: la identidad visual es
tuya.

= Diapositivas, pasos y notas

Una presentación de Gaanim es una escena normal organizada así:

- Cada `scene.segment("nombre", ...)` es una *diapositiva*; acepta una
  plantilla (`template=`) y notas (`notes=`). Su nombre y sus notas aparecen en la vista del presentador.
  Las notas también pueden vivir en un archivo aparte (ver
  #link(<guion>)[El guion de la charla]).
- Cada `scene.stop("nombre")` dentro de una diapositiva es un *paso*: el
  punto donde la reproducción espera a que avances. Un paso escrito mientras
  la diapositiva aún entra con su transición espera a que termine.
- `segment.bind(...)` rellena los huecos de la plantilla con tus objetos.

`scene.slides.brand(...)` configura para toda la presentación el logo, el pie,
la numeración y la línea separadora. Las plantillas `title_slide`, `lecture`,
`comparison` y `credits` devuelven un layout adaptable y puedes reemplazarlas
por funciones de Python propias (ver #link("/guias/layout/")[Layout]).

= El guion de la charla <guion>

Lo que vas a decir puede vivir fuera del código, en un archivo Markdown que
escribes con cualquier editor: una sección `## nombre` por diapositiva, con el
nombre de su `scene.segment(...)`, y una línea `[stop]` donde avanzas.

```markdown
# Mi charla

## Portada
Buenos días. Hoy les cuento cómo crecieron las ventas.
[stop titulo]

## Resultados
Primero miren el gráfico de ventas.
[stop grafico]
Como ven, crece cada trimestre.
[stop]
Y la conclusión es clara.
```

```python
>>>from gaanim import Scene
>>>scene = Scene(frame=(16, 9))
scene.segment("Portada")
scene.wait(1)
scene.stop("titulo")
scene.segment("Resultados")
scene.wait(1)
scene.stop("grafico")
scene.wait(1)
scene.stop()
```

- El texto antes de la primera marca es lo que dices mientras la diapositiva
  espera en su primer stop: al terminarlo, avanzas. El texto antes de la
  segunda va en el segundo stop, y así. Lo que sigue a la última marca va
  después del último stop.
- `[stop grafico]` nombra el stop. `gaanim check` avisa si no coincide con el
  de `scene.stop("grafico")`, si una diapositiva tiene más o menos marcas que
  stops y si una sección no corresponde a ninguna diapositiva ni bloque de voz.
- El archivo es `narration/script.md`, dentro de la carpeta de assets o junto
  al script si la escena no tiene; `scene.narration_script("guion.md")` elige
  otro. Es el mismo guion de la
  #link("/referencia/audio/#narracion")[narración]: una sección puede ser
  también el texto de un `scene.voiceover(...)`.
- La sección de una diapositiva manda sobre su `notes=`; sin sección, se usan
  las notas del código, que también admiten líneas `[stop]`. Los encabezados
  se comparan sin distinguir mayúsculas, los `###` son notas para ti que no se
  muestran y los comentarios HTML se ignoran.
- Al guardar el archivo, el editor actualiza las notas sin volver a compilar
  la escena.

Gaanim escribe el guion por ti y lo mantiene al día con el código. En el
editor, abre *Guion y narración* (el botón de micrófono de la barra de
reproducción, o *Más controles*) y pulsa *Crear guion*: el archivo nace con
una sección por diapositiva, con sus `notes=` como punto de partida, y una
marca `[stop nombre]` por cada stop. Cuando el código gana diapositivas o
stops, el botón pasa a *Abrir y completar*: añade las secciones nuevas detrás
de la diapositiva anterior y las marcas que faltan al final de cada sección,
sin tocar lo que escribiste.

== Dictar el guion con Scribe <dictar>

En lugar de escribir el guion, puedes dictarlo presentando: Scribe Live, el
servicio de transcripción de Zoom, convierte tu voz en texto mientras avanzas
y cada frase va a la diapositiva y al tramo que estabas presentando.

+ Consigue un token de Scribe en Zoom Platform Studio (*View JWT Token*). Elige
  la caducidad más corta que te sirva: si se filtrara, deja de valer antes.
+ En *Guion y narración*, abre *Scribe de Zoom (dictado)*, pega el token y
  pulsa *Guardar en Windows* (ver #link(<token-scribe>)[El token]).
+ Lleva el cabezal a la diapositiva desde la que quieres dictar y pulsa
  *Dictar con Scribe*. Tras la cuenta atrás, presenta hablando: `→` o
  `Espacio` avanzan en cada stop, `Enter` termina y `Esc` descarta. Al
  terminar con `Enter`, el cabezal se queda donde llegaste: el siguiente
  *Dictar con Scribe* sigue desde ese stop, así que puedes dictar la charla
  por partes.
+ Al terminar, Gaanim espera las últimas frases (como mucho 12 segundos;
  `Esc` deja de esperar) y las escribe en el guion. Un tramo vacío recibe lo
  dictado; uno que ya tenía texto lo conserva y lo dictado se añade a su lado
  como comentario (`<!-- dictado: … -->`), que el guion ignora.

Durante el dictado, el teleprompter muestra el guion de la diapositiva, el
estado de Scribe y la última frase que oyó. Si la escena tiene una toma en vivo
(`scene.live_take`), se dicta con sus paradas originales y la toma grabada no
cambia; para grabarla y dictar a la vez, marca *Dictar también el guion con
Scribe* junto a *Grabar toma en vivo*.

- Idiomas: `es-ES`, `en-US`, `pt-BR`, `pt-PT`, `fr-FR`, `de-DE`, `it-IT`,
  `ja-JP`, `zh-CN`, `ar-SA` y `ar-AE`; el español de Scribe es `es-ES`. Se
  elige en los ajustes del panel o, para todo el proyecto, con
  #link("/referencia/gaanim-toml/#language")[`language`] en `gaanim.toml`
  (*Fijar en gaanim.toml*), que manda sobre el de los ajustes.
- Scribe asigna cada frase completa al tramo en el que empezó a decirse: haz
  una pausa breve al avanzar para que la frase siguiente caiga en su tramo.
- *Enviar los nombres de diapositivas y stops como vocabulario* ayuda a
  Scribe con nombres propios y términos; está desactivado, porque envía esos
  nombres a Zoom.

Scribe Live cobra por minuto de audio recibido, así que Gaanim lo limita: una
sola sesión a la vez, sin reconexiones automáticas si falla, y la sesión se
corta sola al llegar a los *Minutos máximos por sesión* de los ajustes (60 por
defecto, nunca más de los 120 de Zoom), tras dos minutos sin voz o si el
micrófono deja de enviar audio, aunque el editor esté en segundo plano.

=== El token <token-scribe>

El token de Scribe es de tu cuenta de Zoom, no del proyecto, y Gaanim nunca lo
guarda en la carpeta del proyecto: no puede acabar en un repositorio. Gaanim
lo busca en este orden:

+ *Pegado en el panel:* vive solo en la memoria de esta sesión del editor.
+ *Guardado en Windows* (recomendado): el Administrador de credenciales de
  Windows lo cifra para tu usuario, como `gaanim/scribe-token`. Puedes verlo o
  borrarlo allí, o con *Quitar el guardado*.
+ *La variable de entorno `GAANIM_SCRIBE_TOKEN`* de tu usuario. Es cómoda en
  otros sistemas, pero cualquier programa que ejecutes la puede leer, incluido
  el código Python de las escenas.

No lo escribas en el script, en `gaanim.toml` ni en un archivo `.env` del
proyecto. Gaanim no lo muestra en el panel, en los mensajes ni en la consola,
y solo lo envía en la cabecera de la conexión cifrada con Zoom. Si crees que
se filtró, genera otra clave en Zoom Platform Studio: la anterior deja de
valer.

= Comprobar y presentar

```bash
gaanim check mi-charla --strict
gaanim --present --monitor 1 --duration 20m mi-charla
```

`gaanim check` revisa segmentos, notas, paradas y el formato 16:9. Los índices
de monitor empiezan en cero. La exportación ignora las paradas y genera un
video continuo.

== Ensayar una parte

```bash
gaanim mi-charla --sections portada,contenido
gaanim --present mi-charla --from contenido
```

`--sections` reproduce solo los segmentos indicados y `--from` empieza en uno
y sigue hasta el final; si los combinas, `--from` recorta la lista. Cada
nombre selecciona el segmento con ese nombre exacto, todos los segmentos de
una `Section` con esa clave o el paso de una `Section` con ese nombre de
`SectionStep` (por ejemplo `--sections "Problemática · país sísmico"`). Un
nombre con comas se pasa solo o con sus comas escritas como `\,`, sin
distinguir mayúsculas. Un nombre desconocido se muestra como error con las
opciones disponibles, y entonces se reproduce todo.

El script se ejecuta completo: los objetos que persisten, la cámara y el tema
llegan a la primera diapositiva elegida exactamente como en la ejecución
entera, porque la selección *salta* a ese segmento en lugar de omitir código.
La reproducción y la navegación solo visitan lo elegido; al terminar un tramo
se salta al siguiente. Con `gaanim --diff ... --capture-stops` las mismas
opciones limitan las pausas capturadas (ver
#link("/guias/capturas-y-comparacion/")[Capturas y comparación visual]).

= La vista del presentador

Con `--present`, la audiencia ve la diapositiva a pantalla completa y tú ves
*Presenter View*, la vista del presentador. Se lee de arriba abajo:

- *Encabezado:* estado (`PLAYING`, `PAUSED` o `END`), `Slide n of N`, la hora,
  el tiempo transcurrido con un botón `↺` para reiniciarlo y una barra con un
  bloque por diapositiva y marcas de pasos; un clic salta a ese punto. Si la
  charla tiene una duración prevista, también `LEFT` y `PACE` (ver
  #link(<tiempo-de-la-charla>)[El tiempo de la charla]).
- *Now on screen:* nombre de la diapositiva, `Step k of K · nombre`, el
  tiempo que llevas en ella si tiene objetivo o un ensayo anterior, y una
  vista grande de lo que ve la audiencia, en vivo: con los votos y los
  jugadores de la sala cuando hay público. Si la pantalla de la audiencia está
  en negro o en blanco, la vista lo indica. Debajo, unos botones permiten
  saltar al inicio de la diapositiva o a cualquiera de sus pasos.
- *Up Next:* una miniatura de la siguiente pausa, indicando si es otro paso
  de la misma diapositiva o la siguiente diapositiva. Las miniaturas muestran
  el ensayo, no los votos de la sala.
- *Speaker notes:* las notas de la diapositiva cortadas en sus `[stop]`, con
  el tramo que toca decir resaltado y `▶ ADVANCE` donde avanzas. `A−`/`A+`
  ajustan el tamaño y *Prompter* muestra el guion entero (ver
  #link(<prompter>)[El prompter]).
- *Barra inferior:* primer paso, paso anterior, reproducir/pausa, paso
  siguiente, último paso, `Overview`, `Black` y `White`, el progreso de las
  vistas previas y la lista de atajos (icono de teclado).

== El tiempo de la charla <tiempo-de-la-charla>

Dile a Gaanim cuánto quieres que dure la charla, entera o por diapositivas, y
Presenter View te dirá cuánto queda y si vas adelantado o retrasado:

```python
>>>from gaanim import Scene, Transition, title_slide
>>>scene = Scene(frame=(16, 9))
scene.slides.target(20 * 60)                                 # la charla, en segundos
scene.segment("Portada", template=title_slide, target=30)    # cada diapositiva
scene.segment("Resultados", Transition.slide(0.5, "left"), target=120)
```

La duración de la charla entera también se fija al presentar, sin tocar el
script: en Presenter View, con el botón que la muestra junto a `LEFT` (o
`Plan time` si aún no hay ninguna), eliges `10m` a `60m`, escribes otra
(`25m`, `1h10m`, `25:00`) o la quitas; o desde la línea de comandos:

```bash
gaanim --present --duration 20m mi-charla
```

- `--duration` acepta `20m`, `1h30m`, `90s`, `20:00` o un número de minutos.
- Si hay varias, manda la que fijes en Presenter View durante la sesión,
  luego `--duration`, luego la del script y por último la que fijaste en la
  vista la vez anterior para ese archivo, que Gaanim recuerda. Sin ninguna,
  la duración es la suma de los objetivos (`target=`), si todas las
  diapositivas tienen uno. El popup del botón dice de dónde viene.
- `LEFT` cuenta hacia atrás lo que queda de esa duración desde que empezó la
  presentación (el `↺` del tiempo transcurrido también la reinicia). Pasa a
  ámbar en el último 10 % y a rojo, con signo menos, cuando te pasas.
- `PACE` aparece si alguna diapositiva tiene objetivo. Suma lo que cada
  diapositiva ya dejada tardó de más o de menos, y lo que la actual ya se
  pasó: `+1:05` es un minuto y cinco segundos de retraso, `-0:20` veinte
  segundos de adelanto. Va en verde si vas a tiempo o adelantado, en ámbar
  con un retraso de hasta un 10 % de la duración prevista (30 segundos como
  mínimo) y en rojo con más.
- Bajo el nombre de la diapositiva, `0:45 of 1:30 on this slide` compara el
  tiempo que llevas en ella con su objetivo; en rojo, cuando lo superas.

Cada presentación que dura más de 30 segundos se guarda como un ensayo, con
el tiempo de cada diapositiva, en la carpeta de datos de Gaanim. En el
siguiente, Presenter View añade `last rehearsal 1:20`: lo que tardaste en esa
diapositiva la vez anterior. Se guardan los 20 últimos ensayos de cada
archivo, del script o del paquete `.gaanim`, que también conserva los
objetivos.

== El prompter <prompter>

Para ensayar, o para leer durante la charla, *Prompter* (o la tecla `T`)
convierte la columna de notas en un teleprompter de televisión con el guion
entero: todas las diapositivas seguidas, con su número y su nombre. La columna
se ensancha y la miniatura de *Up Next* se reduce para dejarle sitio.

- El tramo que toca decir va resaltado, con una barra a la izquierda; lo ya
  dicho se apaga y lo que viene queda en gris.
- Donde avanzas aparece `▶ ADVANCE` con el nombre del stop, en ámbar. Late
  cuando la diapositiva ya espera en ese stop: al terminar el tramo, toca
  avanzar. `≈0:12` es lo que tarda en leerse el tramo, a unas 150 palabras por
  minuto.
- El prompter se desplaza solo hasta el tramo actual y lo deja en el tercio
  superior. Puedes desplazarte a mano; al avanzar, vuelve.
- Una diapositiva sin guion muestra igualmente sus stops, y una marca de más,
  sin stop, aparece en rojo.
- *Notes* o `T` vuelven a las notas de la diapositiva actual.

Para ensayar la charla entera contra su duración, presenta con el prompter
abierto: Presenter View cuenta el tiempo de cada diapositiva y guarda el
ensayo (ver #link(<tiempo-de-la-charla>)[El tiempo de la charla]).

```bash
gaanim --present --duration 20m mi-charla
```

Para grabarte presentando, llama a `scene.live_take()` al principio del script
y pulsa *Grabar toma en vivo* en *Guion y narración*. El teleprompter de la
grabación muestra el guion de cada diapositiva con `▶ AVANZA` en cada stop, y
la toma guarda cuánto esperaste en cada uno: la reproducción y el vídeo
exportado siguen tu ritmo (ver #link("/referencia/audio/#narracion")[Narración]).

== Atajos de teclado

#table(
  columns: 2,
  table.header[*Tecla*][*Acción*],
  [`→`, `Enter` o clic en la audiencia], [avanzar al siguiente paso],
  [`←` o `Backspace`], [volver al paso anterior],
  [`Space`], [pausar o reanudar la animación en curso; nunca salta de paso],
  [`Home` / `End`], [ir al primer o al último paso],
  [`O`], [vista general: busca por nombre de diapositiva, paso o notas; `Enter` salta a la primera coincidencia],
  [`T`], [mostrar el guion entero como teleprompter, o volver a las notas de la diapositiva],
  [`B` / `W`], [poner la pantalla de la audiencia en negro o en blanco],
  [`P`], [volver a abrir la vista del presentador si la cerraste],
  [`Esc`], [cerrar la vista general o el negro/blanco; después, salir del modo presentación],
)

Saltar a una diapositiva muestra su estado inicial aunque la anterior termine
con una pausa en el mismo instante.

== Pantalla de la audiencia

Al llevar el cursor a la parte inferior de la pantalla completa aparece una
barra compacta con primer paso, paso anterior, reproducir/pausa, paso
siguiente, último paso, el nombre y número de la diapositiva y el progreso. Se
oculta al retirar el cursor o al pasar a la vista del presentador. Sus botones
y los atajos hacen lo mismo, así que un clic no avanza dos veces.

La pantalla completa, tanto en el editor como al presentar, ajusta el lienzo
al monitor sin deformarlo y rellena en negro las franjas sobrantes, sea cual
sea el fondo de la escena. En el editor, `Esc` o `F11` salen de la pantalla
completa.

== Vistas previas de las diapositivas

Las vistas previas se generan en segundo plano sin afectar a la presentación:
primero la diapositiva actual y la siguiente, luego el resto. Tras una recarga
en caliente, las anteriores siguen visibles, marcadas como
`Updating preview…`, hasta que llega su reemplazo. Se conservan al reabrir la
vista del presentador, se adaptan al tamaño y la densidad de píxeles de la
ventana y solo se regeneran al agrandarla. Si el renderizado falla, la barra
ofrece `Retry` y conserva las vistas ya generadas.

Las vistas previas incluyen el contenido 3D, igual que la pantalla de la
audiencia.

= Encuestas a la audiencia <encuestas-a-la-audiencia>

Una presentación puede hacer preguntas al público: encuestas, cuestionarios
al estilo de Kahoot con clasificación, juegos por equipos y personajes del
público dentro de la escena. Todo eso está en
#link("/publico/")[Público en vivo]: empieza por
#link("/publico/primera-encuesta/")[Tu primera encuesta].

= Repartir las diapositivas en PDF

Para repartir la presentación o tener una copia que no necesite Gaanim,
expórtala a PDF: una página por cada pausa, tal como la muestra el Presenter,
y otra para el final. Con `--notes`, tras cada diapositiva va una página con
sus notas, donde cada `[stop]` se imprime como una flecha (`→ grafico`).

```bash
gaanim export mi-charla --output mi-charla.pdf --notes
```

También funciona con un paquete `.gaanim` y desde el diálogo *Exportar* del
editor; la #link("/referencia/cli/#diapositivas-pdf")[referencia de la línea
de comandos] explica las opciones.

= Revisar sin pausas

Para revisar una animación de corrido en el editor, activa *Continuous* junto
a los controles de reproducción. El ajuste dura toda la sesión y sobrevive a
las recargas, pero el modo presentación sigue respetando `scene.stop(...)`.
Los saltos, las capturas y la exportación a vídeo también ignoran las pausas.
