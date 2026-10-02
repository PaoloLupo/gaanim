#import "../../components/section.typ": docs-chapter

#show: docs-chapter.with(
  title: "Tu primera encuesta",
  description: "Despliega el relay, abre una encuesta y preséntala con teléfonos reales",
  route: "/publico/primera-encuesta/",
)

En esta guía pondrás una encuesta en una diapositiva y la presentarás con
teléfonos reales. Lo haces en tres pasos: despliegas el relay una sola vez,
escribes la encuesta y presentas.

= 1. Despliega el relay <relay>

Los votos viajan a través de un *relay*, un pequeño servicio web que despliegas
tú, gratis, en tu propia cuenta de Cloudflare. Como teléfonos y presentación
solo se conectan hacia fuera, por HTTPS, funciona en redes universitarias que
aíslan a los dispositivos entre sí y con datos móviles. La página de votación
está pensada para el teléfono: respuestas grandes con color, letra y forma,
tema claro u oscuro y español o inglés según el teléfono.

La forma más corta es el botón
#link("https://deploy.workers.cloudflare.com/?url=https://github.com/PaoloLupo/gaanim/tree/main/crates/gaanim_project/relay")[Deploy to Cloudflare]:
copia el relay a un repositorio tuyo y lo publica desde ahí. También puedes
desplegarlo desde tu equipo, con Node.js instalado:

```bash
gaanim relay init            # escribe el relay en ./gaanim-relay
cd gaanim-relay
npx wrangler login           # una cuenta gratuita de Cloudflare
npx wrangler deploy          # imprime https://gaanim-relay.<tú>.workers.dev
```

Después dile a Gaanim dónde está:

```bash
gaanim relay use https://gaanim-relay.<tú>.workers.dev
```

`gaanim relay` sin argumentos muestra el relay en uso y si habla el mismo
protocolo que tu versión de Gaanim; al presentar, Gaanim avisa si no. Para
actualizarlo, vuelve a escribirlo sobre su carpeta con
`gaanim relay init --force gaanim-relay` y despliégalo otra vez.

Un proyecto puede usar otro relay con `[polls] relay` en su
#link("/referencia/gaanim-toml/")[`gaanim.toml`], y la variable
`GAANIM_POLL_RELAY` tiene prioridad sobre ambos. Sin relay, `scene.poll`
avisa y el QR no lleva a ninguna parte. La dirección queda grabada en el QR,
así que si cambias de relay vuelve a exportar.

= 2. Escribe la encuesta <encuesta>

`scene.poll` hace una pregunta al público y te da los datos para que tú
decidas cómo presentarla:

```python
# output: preview.webp
# show-code: true
from gaanim import RED, WHITE, Scene

scene = Scene(frame=(16, 9))
poll = scene.poll("¿Qué curva crece más rápido?", ["x²", "2ˣ", "x log x"], rehearse=[3, 7, 2])

card = scene.geometry.rounded_rect(4.2, 4.2, 0.25).fill(WHITE).no_stroke().move_to(-5, 0)
qr = poll.qr(3.6).move_to(-5, 0)                           # el QR, un drawable más
code = scene.text(poll.code, size=0.55).move_to(-5, -2.7)  # por si no pueden escanear

for i, answer in enumerate(poll.options):
    y = 1.2 - 1.4 * i
    scene.text(answer, size=0.5, text_box="cap").move_to(-1.6, y)
    poll.bar(i, length=6, thickness=0.6, radius=0.3).fill(RED).no_stroke().move_to(3.2, y)
    scene.viz.readout(poll.votes(i), format=".0f").move_to(6.8, y)

scene.wait(3)
scene.stop()
scene.render()
```

La encuesta no dibuja nada por sí misma. Te da:

- `poll.qr(tamaño)`: el código QR como drawable, relleno de negro. Ponlo sobre
  un fondo claro con algo de margen para que los teléfonos lo lean.
- `poll.code` y `poll.url`: el código de seis caracteres y la dirección de
  votación, para mostrarlos como texto.
- `poll.votes(i)`, `poll.share(i)` (de 0 a 1), `poll.percent(i)` (de 0 a 100)
  y `poll.total()`: `Parameter` que siguen los votos. Úsalos como cualquier
  parámetro: en `readout`, en `computed`, en puntos o en líneas reactivas
  (ver #link("/guias/reactividad/")[Reactividad]).
- `poll.bar(i, ...)`: una barra lista cuya longitud sigue a la respuesta `i`.
  Con `direction` crece hacia la derecha, la izquierda, arriba o abajo; con
  `scale="leader"` (por defecto) la respuesta que va ganando llena su barra, y
  con `"total"` cada barra mide su porcentaje. Sus límites son los de la barra
  completa, así que el layout no se mueve al llegar votos.
- `poll.icon(i, tamaño)` y `poll.color(i)`: la forma y el color que la
  respuesta `i` tiene en el teléfono, para que la pantalla y el teléfono se
  parezcan.
- `poll.close()`: deja de recibir votos en ese punto. Sin él, la encuesta se
  cierra al terminar el segmento donde se abrió. Los valores conservan el
  último conteo, así que un paso posterior puede comentar el resultado.

Cada teléfono cuenta una vez y puede cambiar su voto mientras la pregunta
sigue abierta. Si vuelves a una encuesta, conserva sus votos. `rehearse` da
al #link("/publico/anatomia/#ensayo")[ensayo] un peso por respuesta (`[3, 7, 2]`
hace la segunda más popular), para que la vista previa se parezca a lo que
esperas.

El código de la sesión es fijo para cada proyecto y el QR es contenido normal
de la escena: se ve igual en la previsualización, en un vídeo exportado o en
un paquete `.gaanim`, y los teléfonos lo escanean una vez por presentación. La
clave que permite abrir preguntas y leer votos se guarda solo en tu equipo.
Para fijar el código, por ejemplo si varias personas presentan el mismo
proyecto, usa la variable `GAANIM_POLL_SESSION`.

= 3. Presenta <presentar>

```bash
gaanim --present main.py
```

Mientras la presentación está dentro de la encuesta, los votos llegan en vivo
y las barras crecen. Prueba antes con dos o tres teléfonos tuyos: escanea el
QR, vota y mira cómo cambia la pantalla. Para seguir, lee
#link("/publico/presentar/")[Presentar con público], que cuenta qué ves en la
vista del presentador y qué hacer si algo falla.

Los votos son anónimos: el teléfono guarda un identificador al azar y el relay
no pide nombres ni cuentas. El relay borra la sesión y sus votos doce horas
después de su última actividad.
