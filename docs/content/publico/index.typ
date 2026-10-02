#import "../../components/section.typ": docs-chapter

#show: docs-chapter.with(
  title: "Público en vivo",
  description: "Encuestas, cuestionarios, equipos y personajes del público dentro de tu escena",
  route: "/publico/",
  nav: "Introducción",
)

#let card(href, eyebrow, title, body) = html.a(href: href, class: "home-card", {
  html.span(class: "home-card-eyebrow", eyebrow)
  html.span(class: "home-card-title", title)
  html.span(class: "home-card-body", body)
})

Mientras presentas, el público puede responder desde su teléfono: votar en
una encuesta, competir en un cuestionario al estilo de Kahoot, jugar por
equipos y hasta entrar a la escena como el personaje que armó. Escanean un QR,
escriben un apodo y la escena reacciona a lo que hacen.

```python
# output: preview.webp
# show-code: true
from gaanim import GOLD, WHITE, Scene

scene = Scene(frame=(16, 9), background="#1c0b3f")
poll = scene.poll("¿Qué tema dominas más?", ["Ciencia", "Historia", "Arte"],
                  rehearse=[4, 3, 2])

scene.text(poll.question, size=0.6, color=WHITE, weight=700).move_to(0, 3)
for i, answer in enumerate(poll.options):
    y = 1.2 - 1.5 * i
    poll.icon(i, 0.55).move_to(-6.3, y)
    scene.text(answer, size=0.5, color=WHITE, text_box="cap").move_to(-4.2, y)
    bar = poll.bar(i, length=7, thickness=0.6, radius=0.3).fill(poll.color(i)).no_stroke()
    bar.move_to(1.6, y)
    scene.viz.readout(poll.votes(i), format=".0f", color=GOLD, font_size=0.45).move_to(5.9, y)
scene.wait(4)
scene.stop()
scene.render()
```

En la vista previa vota un público inventado, el
#link("/publico/anatomia/#ensayo")[ensayo], así que puedes diseñar todo el
juego sin teléfonos. Al presentar, las mismas barras siguen los votos reales.

= Cómo funciona

Hay cuatro piezas:

- *La escena*, que escribes en Python como cualquier otra. Abre preguntas
  con `scene.poll` y `scene.quiz` y decide cómo se ven.
- *El relay*, un pequeño servicio web gratuito que despliegas una vez en tu
  cuenta de Cloudflare. Lleva las preguntas a los teléfonos y los votos a la
  presentación.
- *Los teléfonos* del público, que abren la página del relay con el QR. No
  instalan nada ni crean cuentas.
- *El ensayo*, un público inventado que juega la escena en la vista previa,
  al exportar y en las capturas, siempre igual.

Al presentar con `gaanim --present`, Gaanim abre cada pregunta en el relay
cuando llegas a ella y lee los resultados mientras el público responde. Un
paquete `.gaanim` exportado también recibe votos en vivo, sin Python.

= Datos, no diseño

Gaanim no trae una pantalla de Kahoot ya hecha. Te da los datos y piezas
para dibujarlos, y el diseño es tuyo:

- *Números en vivo*: votos, porcentajes, respuestas, segundos que quedan,
  puntos de cada jugador y de cada equipo. Son `Parameter`, así que los usas
  en lecturas, en `computed` o para mover y escalar cualquier cosa.
- *Piezas listas*: el QR, barras que siguen a una respuesta, los apodos como
  texto en vivo, la forma y el color que cada respuesta tiene en el teléfono.
- *Momentos*: dónde se abre y se cierra una pregunta, dónde se revela la
  respuesta y cuándo una pausa avanza sola porque el público ya respondió.
- *Personajes*: con una zona viva, cada jugador entra a la escena como su
  personaje y una función de Python decide cómo se mueve.

Con esas piezas puedes hacer un Kahoot clásico, una carrera, una batalla
entre equipos, un podio o algo que todavía no existe. El
#link("/publico/estilos/")[recetario de estilos] tiene varios para copiar.

= Por dónde seguir

#html.div(class: "home-cards", {
  card("primera-encuesta/", "Empezar", "Tu primera encuesta", [Despliega el relay, abre una encuesta y preséntala con teléfonos reales.])
  card("anatomia/", "Juegos", "Anatomía de un juego", [Cuestionarios, revelación, clasificación, sala de espera, equipos y ensayo.])
  card("preguntas/", "Contenido", "Preguntas", [Imágenes, selección múltiple y preguntas escritas en Markdown o CSV.])
  card("zonas-vivas/", "Personajes", "Zonas vivas", [El público entra a la escena y tú decides cómo se mueve, en Python.])
  card("estilos/", "Diseño", "Recetario de estilos", [Escenas completas para copiar: fichas, carreras, batallas y podios.])
  card("presentar/", "En vivo", "Presentar con público", [La vista del presentador, partidas nuevas, paquetes `.gaanim` y problemas comunes.])
  card("../referencia/publico/","Referencia", "API del público", [`Poll`, `Leaderboard`, `Teams`, `Audience`, `LiveZone`, `gaanim.live` y preguntas.])
})
