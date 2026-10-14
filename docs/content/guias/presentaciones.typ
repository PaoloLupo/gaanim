#import "../../components/section.typ": docs-chapter

#show: docs-chapter.with(
  title: "Presentaciones",
  description: "Crea, ensaya y presenta diapositivas con notas y pausas",
  route: "/guias/presentaciones/",
)

En esta guía aprenderás a preparar una charla con Gaanim: diapositivas con
plantillas reutilizables, notas del orador, pausas donde esperar a que hables,
el guion de la charla en un archivo aparte, su tiempo grabado diapositiva a
diapositiva, ensayos con un teleprompter y la vista del presentador en un
segundo monitor.

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

También puedes ejecutar `gaanim` sin argumentos, elegir *New presentation*. Si aún no tienes Gaanim listo, sigue
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
- En la barra de tiempo del editor, un cuadrado en la esquina de cada
  diapositiva dice cómo va su guion: verde si todos sus tramos tienen texto,
  escrito o dictado, y ámbar si solo algunos. Sin cuadrado, está pendiente.

Gaanim escribe el guion por ti y lo mantiene al día con el código. En el
editor, abre *Script & narration* (el botón de micrófono de la barra de
reproducción, o *More controls*) y pulsa *Create script*: el archivo nace con
una sección por diapositiva, con sus `notes=` como punto de partida, y una
marca `[stop nombre]` por cada stop. Cuando el código gana diapositivas o
stops, el botón pasa a *Open and complete*: añade las secciones nuevas detrás
de la diapositiva anterior y las marcas que faltan al final de cada sección,
sin tocar lo que escribiste.

== Grabar la charla y ajustar sus tiempos <grabar-charla>

Graba la charla presentándola en el editor y Gaanim apunta cuánto tardas en
cada diapositiva, suma el total y lo compara con lo que quieres que dure.
Después vuelves a grabar solo las diapositivas que se pasan hasta que la
charla cabe. Funciona aunque el guion esté vacío: si además dictas con
Scribe, la misma grabación lo escribe (ver #link(<dictar>)[Dictar el guion
con Scribe]).

En *Script & narration*, la tarjeta *TIMING* tiene una fila por diapositiva
con el tiempo grabado, su objetivo y un botón, y arriba el total frente a la
duración prevista. `▸` despliega los stops de una diapositiva, cada uno con su
tiempo y su botón, y un clic en el nombre de una diapositiva o de un stop
lleva el cabezal allí.

+ Planifica la charla con `scene.slides.target(...)` o con un `target=` en
  cada diapositiva (ver #link(<tiempo-de-la-charla>)[El tiempo de la
  charla]). Sin plan también se graban los tiempos, pero no hay con qué
  compararlos.
+ Pulsa *Record the talk*. Tras la cuenta atrás, presenta hablando desde la
  primera diapositiva: `→` o `Espacio` avanzan en cada stop, `Enter` termina y
  `Esc` lo descarta todo.
+ Cada diapositiva que recorres entera guarda su tiempo, desde que entras
  hasta que sales, y el de cada tramo entre stops. La diapositiva en la que
  pulsas `Enter` cuenta si ya estabas en su último stop.
+ Mira la tarjeta: cada tiempo va en verde si cabe en su objetivo, en ámbar si
  se pasa un poco (un 10 %, al menos 10 segundos) y en rojo si se pasa más.
  Debajo, Gaanim dice cuánto te sobra (`Fits the plan with 0:08 to spare`),
  cuánto te pasas o, si faltan diapositivas por grabar, cuánto tiempo les
  queda.
+ *Re-record* vuelve a presentar solo esa diapositiva: la grabación termina
  sola cuando sales de ella y su tiempo sustituye al anterior. En una
  diapositiva sin tiempo, *Record* presenta desde ahí hasta que pulsas
  `Enter`.
+ Para ajustar menos, despliega la diapositiva y pulsa *Re-record* en un
  stop: la grabación arranca justo al dejar el stop anterior (o al entrar en
  la diapositiva, para el primero) y termina sola cuando avanzas en ese stop.
  Su tiempo sustituye al del tramo, y el de la diapositiva se recalcula.

Mientras grabas, el subtítulo del editor (ver #link(<prompter>)[El
prompter]) muestra, además del guion, el reloj de la grabación, el nivel del
micrófono, el tiempo que llevas en la diapositiva frente a su objetivo y
`talk ≈ 3:40 of 4:00`: lo que duraría la charla con lo que llevas grabado y
los tiempos guardados del resto de diapositivas.

Los tiempos se guardan en la carpeta de datos de Gaanim, junto a los ensayos
de ese archivo, no en el proyecto. Si la escena tiene una toma en vivo
(`scene.live_take`), se graba siempre entera: las filas no se regraban por
separado.

== Dictar el guion con Scribe <dictar>

En lugar de escribir el guion, puedes dictarlo mientras grabas la charla:
Scribe Live, el servicio de transcripción de Zoom, convierte tu voz en texto y
cada frase va a la diapositiva y al tramo que estabas presentando.

+ Consigue un token de Scribe en Zoom Platform Studio (*View JWT Token*). Elige
  la caducidad más corta que te sirva: si se filtrara, deja de valer antes.
+ En *Script & narration*, abre *Zoom Scribe (dictation)*, pega el token y
  pulsa *Save in Windows* (ver #link(<token-scribe>)[El token]).
+ En la tarjeta *TIMING*, deja marcado *Write what I say into the script
  (Zoom Scribe)*, que se activa solo en cuanto hay un token, y graba la charla
  o una diapositiva (ver #link(<grabar-charla>)[Grabar la charla]).
+ Al terminar con `Enter`, el cabezal se queda donde llegaste. Si Scribe se
  cortó antes (un error, el límite de minutos o dos minutos sin voz), se queda
  donde se cortó, para grabar de nuevo lo que falta; `Esc` lo descarta todo y
  vuelve a donde empezaste.
+ Gaanim espera las últimas frases (como mucho 12 segundos; `Esc` deja de
  esperar) y las escribe en el guion: lo que dices en un tramo pasa a ser su
  texto. Si el tramo ya tenía texto, lo sustituye y deja el anterior justo
  debajo en un comentario `<!-- previous: … -->`, que el guion ignora; al
  regrabar de nuevo, ese comentario se cambia por el del último texto, así
  que nunca se acumulan. Tus propios comentarios se quedan donde están.
+ Al regrabar una diapositiva o un stop, solo se escribe en ellos: lo que
  digas mientras avanzas fuera no cae en el tramo siguiente.

Durante el dictado, el subtítulo del editor muestra el guion del tramo que
toca decir; si el tramo aún no tiene guion, muestra en vivo lo que Scribe va
escribiendo, frase a frase. Gaanim mide además tu ritmo al hablar (palabras
por minuto) para que el prompter ilumine el guion a tu velocidad. Si la escena
tiene una toma en vivo, se dicta con sus paradas originales y la toma grabada
no cambia; para grabarla y dictar a la vez, marca *Also dictate the script
with Scribe* junto a *Record live take*.

- Idiomas: `es-ES`, `en-US`, `pt-BR`, `pt-PT`, `fr-FR`, `de-DE`, `it-IT`,
  `ja-JP`, `zh-CN`, `ar-SA` y `ar-AE`; el español de Scribe es `es-ES`. Se
  elige en los ajustes del panel o, para todo el proyecto, con
  #link("/referencia/gaanim-toml/#language")[`language`] en `gaanim.toml`
  (*Pin in gaanim.toml*), que manda sobre el de los ajustes.
- Scribe asigna cada frase completa al tramo en el que empezó a decirse: haz
  una pausa breve al avanzar para que la frase siguiente caiga en su tramo.
- *Send the names of slides and stops as vocabulary* ayuda a
  Scribe con nombres propios y términos; está desactivado, porque envía esos
  nombres a Zoom.

Scribe Live cobra por minuto de audio recibido, así que Gaanim lo limita: una
sola sesión a la vez, sin reconexiones automáticas si falla, y la sesión se
corta sola al llegar a los *Max minutes per session* de los ajustes (60 por
defecto, nunca más de los 120 de Zoom), tras dos minutos sin voz o si el
micrófono deja de enviar audio, aunque el editor esté en segundo plano.

=== El token <token-scribe>

El token de Scribe es de tu cuenta de Zoom, no del proyecto, y Gaanim nunca lo
guarda en la carpeta del proyecto: no puede acabar en un repositorio. Gaanim
lo busca en este orden:

+ *Pegado en el panel:* vive solo en la memoria de esta sesión del editor.
+ *Guardado en Windows* (recomendado): el Administrador de credenciales de
  Windows lo cifra para tu usuario, como `gaanim/scribe-token`. Puedes verlo o
  borrarlo allí, o con *Remove saved token*.
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
  ajustan el tamaño y *Prompter* muestra el tramo que toca decir como un
  subtítulo (ver #link(<prompter>)[El prompter]).
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

Para ajustar la charla a esa duración antes de presentarla, grábala en el
editor diapositiva a diapositiva (ver #link(<grabar-charla>)[Grabar la charla
y ajustar sus tiempos]).

Cada presentación que dura más de 30 segundos se guarda como un ensayo, con
el tiempo de cada diapositiva, en la carpeta de datos de Gaanim. En el
siguiente, Presenter View añade `last rehearsal 1:20`: lo que tardaste en esa
diapositiva la vez anterior. Se guardan los 20 últimos ensayos de cada
archivo, del script o del paquete `.gaanim`, que también conserva los
objetivos.

== El prompter <prompter>

El prompter muestra lo que toca decir como un subtítulo: el tramo del guion
entre un stop y el siguiente, sobre una franja opaca, con sus palabras
iluminándose al ritmo al que deberías ir diciéndolas. Es el mismo en
Presenter View y en el editor, con el mismo tamaño de texto.

- *En el editor:* `Shift+T` o *More controls → Prompter · Shift+T* lo abren
  abajo, sobre la vista previa y por encima de la barra de reproducción. No
  tiene barra ni botones: el clic derecho cambia el tamaño del texto, dice a
  qué ritmo lee y lo cierra. Mientras grabas o dictas, el mismo subtítulo
  muestra también la grabación (ver #link(<grabar-charla>)[Grabar la
  charla]).
- *En Presenter View:* *Prompter* (o la tecla `T`) convierte la columna de
  notas en el subtítulo, que ocupa toda la columna; `A+` y `A−` cambian el
  tamaño. La columna se ensancha y la miniatura de *Up Next* se reduce para
  dejarle sitio. *Notes* o `T` vuelven a las notas de la diapositiva.

Cómo se lee:

- Lo ya dicho se apaga y lo que viene queda en blanco. Cuando el tramo no cabe
  en dos líneas, el texto sube con suavidad para dejar a la vista la línea que
  toca.
- El avance sigue a la línea de tiempo: cuenta el trozo del tramo que ya
  recorrió el cabezal más el tiempo que la diapositiva lleva esperando en su
  stop. Si retrocedes, las palabras vuelven atrás con él.
- El ritmo sale de tus grabaciones: el tiempo que tardaste en ese tramo
  cuando grabaste la charla, si lo grabaste; si no, tu ritmo medido al
  dictar con Scribe; y si nunca dictaste, 150 palabras por minuto.
- A la derecha, `▶ grafico` dice dónde avanzas: el nombre del stop o, al
  final de la diapositiva, el de la siguiente. Late cuando la diapositiva ya
  espera en ese stop y el tramo está dicho: toca avanzar.
- Debajo, a la izquierda, el nombre de la diapositiva y, si la grabaste, su
  tiempo frente a su objetivo.

Para ensayar la charla entera contra su duración, presenta con el prompter
abierto: Presenter View cuenta el tiempo de cada diapositiva y guarda el
ensayo (ver #link(<tiempo-de-la-charla>)[El tiempo de la charla]).

```bash
gaanim --present --duration 20m mi-charla
```

Para grabarte presentando, llama a `scene.live_take()` al principio del script
y pulsa *Record live take* en *Script & narration*. El prompter del editor
sigue la grabación, y la toma guarda cuánto esperaste en cada stop: la
reproducción y el vídeo exportado siguen tu ritmo (ver
#link("/referencia/audio/#narracion")[Narración]).

== Atajos de teclado

#table(
  columns: 2,
  table.header[*Tecla*][*Acción*],
  [`→`, `Enter` o clic en la audiencia], [avanzar al siguiente paso],
  [`←` o `Backspace`], [volver al paso anterior],
  [`Space`], [pausar o reanudar la animación en curso; nunca salta de paso],
  [`Home` / `End`], [ir al primer o al último paso],
  [`O`], [vista general: busca por nombre de diapositiva, paso o notas; `Enter` salta a la primera coincidencia],
  [`T`], [mostrar el tramo que toca decir como subtítulo en el prompter, o volver a las notas de la diapositiva; en el editor, `Shift+T` abre o cierra el subtítulo del prompter],
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

También funciona con un paquete `.gaanim` y desde el diálogo *Export* del
editor; la #link("/referencia/cli/#diapositivas-pdf")[referencia de la línea
de comandos] explica las opciones.

= Revisar sin pausas

Para revisar una animación de corrido en el editor, activa *Continuous* junto
a los controles de reproducción. El ajuste dura toda la sesión y sobrevive a
las recargas, pero el modo presentación sigue respetando `scene.stop(...)`.
Los saltos, las capturas y la exportación a vídeo también ignoran las pausas.
