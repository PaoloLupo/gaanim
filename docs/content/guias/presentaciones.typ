#import "../../components/section.typ": docs-chapter

#show: docs-chapter.with(
  title: "Presentaciones",
  description: "Crea, ensaya y presenta diapositivas con notas y pausas",
  route: "/guias/presentaciones/",
)

En esta guía aprenderás a preparar una charla con Gaanim: diapositivas con
plantillas reutilizables, notas del orador, pausas donde esperar a que hables,
ensayos de una sola parte y la vista del presentador en un segundo monitor.

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
- Cada `scene.stop("nombre")` dentro de una diapositiva es un *paso*: el
  punto donde la reproducción espera a que avances. Un paso escrito mientras
  la diapositiva aún entra con su transición espera a que termine.
- `segment.bind(...)` rellena los huecos de la plantilla con tus objetos.

`scene.slides.brand(...)` configura para toda la presentación el logo, el pie,
la numeración y la línea separadora. Las plantillas `title_slide`, `lecture`,
`comparison` y `credits` devuelven un layout adaptable y puedes reemplazarlas
por funciones de Python propias (ver #link("/guias/layout/")[Layout]).

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
- *Speaker notes:* tus notas, con tamaño ajustable mediante `A−`/`A+`.
- *Barra inferior:* primer paso, paso anterior, reproducir/pausa, paso
  siguiente, último paso, `Overview`, `Black` y `White`, el progreso de las
  vistas previas y la lista de atajos (icono de teclado).

== El tiempo de la charla <tiempo-de-la-charla>

Dile a Gaanim cuánto quieres que dure la charla, entera o por diapositivas, y
Presenter View te dirá cuánto queda y si vas adelantado o retrasado:

```python
scene.segment("Portada", template=title_slide, target=30)    # segundos
scene.segment("Resultados", Transition.slide(0.5), target=120)
```

```bash
gaanim --present --duration 20m mi-charla
```

- `--duration` fija la duración de la charla entera: `20m`, `1h30m`, `90s`,
  `20:00` o un número de minutos. Sin ella, la duración es la suma de los
  objetivos (`target=`), si todas las diapositivas tienen uno.
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

== Atajos de teclado

#table(
  columns: 2,
  table.header[*Tecla*][*Acción*],
  [`→`, `Enter` o clic en la audiencia], [avanzar al siguiente paso],
  [`←` o `Backspace`], [volver al paso anterior],
  [`Space`], [pausar o reanudar la animación en curso; nunca salta de paso],
  [`Home` / `End`], [ir al primer o al último paso],
  [`O`], [vista general: busca por nombre de diapositiva, paso o notas; `Enter` salta a la primera coincidencia],
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

Para repartir la presentación, subirla a una plataforma de cursos o tener una
copia que no necesite Gaanim, expórtala a PDF: una página por cada pausa, tal
como la muestra el Presenter, y otra para el final. Con `--notes`, tras cada
diapositiva va una página con sus notas.

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
