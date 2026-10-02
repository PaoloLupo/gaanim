#import "../../components/section.typ": docs-chapter

#show: docs-chapter.with(
  title: "Presentar con público",
  description: "La vista del presentador, partidas nuevas, paquetes .gaanim y qué hacer si algo falla",
  route: "/publico/presentar/",
)

En esta guía verás qué pasa cuando presentas una escena con público: qué te
muestra la vista del presentador, cómo empezar una partida nueva, qué sigue en
vivo en un paquete `.gaanim` y cómo resolver los problemas más comunes.

= Antes de la charla <antes>

+ Comprueba el relay con `gaanim relay`: debe decir `up to date`.
+ Revisa la escena con `gaanim check` y recórrela con la vista previa, donde
  juega el ensayo.
+ Presenta una vez con dos o tres teléfonos tuyos, con datos móviles y con la
  red del lugar si puedes: escanea el QR, únete, responde y revela.
+ Antes de empezar, una partida nueva (`R` dos veces) deja la sesión limpia.

= La vista del presentador <vista>

La vista del presentador tiene una sección *Audience* con el código de la
sesión, los teléfonos conectados y los jugadores con sus puntos. Desde ahí
puedes quitar a un jugador (su teléfono ya no puede volver a unirse) o empezar
una partida nueva con *New game*, que borra votos, respuestas y jugadores y
pide una segunda pulsación para confirmar. Pulsar `R` dos veces seguidas hace
lo mismo desde cualquier ventana de la presentación, y
`gaanim relay reset [ruta]` lo hace desde la terminal.

En una pausa con `until=`, la vista del presentador muestra cuánto falta para
que avance sola; siempre puedes avanzar antes.

Al terminar la presentación los teléfonos muestran el podio y una despedida,
y la siguiente presentación empieza una partida nueva. Si la presentación se
cae a mitad de la charla, al volver a abrirla la partida sigue donde estaba.
Si pierde la conexión con el relay, la presentación sigue y la terminal avisa
mientras reintenta.

= Guardar los resultados <resultados>

Cada presentación con público guarda sola los resultados de la partida, para
que puedas revisarlos o poner notas después. Van a `resultados/` dentro de la
carpeta de salida del proyecto (`exports/` por defecto), o junto al paquete
`.gaanim` que presentas, en una carpeta por partida con la fecha y hora en
que empezó:

```text
exports/resultados/2026-10-01_15-30/
├── jugadores.csv     un jugador por fila: puesto, puntos, aciertos y cada respuesta
├── respuestas.csv    una fila por respuesta: pregunta, respuesta, si acertó, puntos y segundos
└── preguntas.csv     una fila por pregunta: cuántos respondieron, cuántos acertaron y los votos
```

Los archivos se actualizan mientras llegan respuestas, a lo sumo cada cinco
segundos, y una vez más al terminar la presentación, así que una presentación
que se cierra de golpe también los deja. Son CSV en UTF-8, que Excel, Google
Sheets y LibreOffice abren con las tildes bien. En la vista del presentador, la
sección *Audience* dice cuándo se guardaron por última vez, tiene *Save now*
para guardarlos al instante y *Open folder* para abrir la carpeta. Si tienes
un archivo abierto en Excel mientras presentas, Excel no deja reemplazarlo:
la vista del presentador lo avisa y se guarda en cuanto lo cierras.

Si la presentación se cerró antes de guardarlos, el relay conserva la partida
doce horas y la descargas con:

```bash
gaanim relay results            # en la carpeta del proyecto
```

== Saber quién es quién <roster>

Los apodos son libres. Para que cada fila tenga el código o el nombre real del
alumno, pídelo al unirse con `scene.roster`:

```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
scene.roster("Código de alumno")    # o "Nombre y apellido", "Correo"…
```

El teléfono muestra ese campo junto al apodo y no deja unirse sin él (con
`required=False` es opcional). El dato nunca llega a la pantalla ni a los
demás teléfonos: solo aparece, como una columna más, en los resultados
guardados.

= Los teléfonos <telefonos>

La página de votación funciona en cualquier navegador moderno, sin instalar
nada. Muestra la pregunta en cuanto la presentación llega a ella, una cuenta
atrás en los cuestionarios y, al revelar, si el jugador acertó, sus puntos y
su puesto. Para que nadie copie, cada teléfono ordena las respuestas de un
cuestionario a su manera y no muestra el puntaje hasta la revelación.

Los apodos admiten de 2 a 20 letras, números o espacios y no se repiten en la
sesión. Si el teléfono se bloquea o pierde la señal, vuelve a la pregunta
abierta al regresar. En redes que bloquean WebSockets, la página pregunta por
HTTP cada pocos segundos.

= Paquetes .gaanim <paquetes>

Un paquete `.gaanim` exportado se presenta sin Python y también recibe votos
en vivo, pero el paquete reproduce lo que se grabó al exportar. Siguen al
público real:

- las barras de `poll.bar`, `board.bar` y `teams.bar`;
- los apodos de `board.name` y `audience.name`, cuyos glifos guarda el
  paquete;
- las lecturas que muestran directamente un valor del público, como
  `scene.viz.readout(poll.votes(0))`, `poll.percent(0)` o
  `teams.score(1)`;
- las zonas vivas, cuyos comportamientos se compilan dentro del paquete;
- las pausas con `until=`.

Lo que pase por un `computed` de Python muestra lo que se grabó con el
ensayo: si quieres que algo siga en vivo en un paquete, usa directamente el
`Parameter`.

== En el reproductor web <web>

El #link("/guias/compartir/#compartir-un-enlace")[reproductor web] también
presenta un paquete con encuestas: al presentar, la página de la presentación
se conecta al relay que se grabó en el paquete y los teléfonos votan como con
la aplicación. El Presenter View muestra los teléfonos y los jugadores, su
diapositiva sigue los votos de la sala como la de la audiencia, y
desde él puedes quitar a un jugador o empezar una partida nueva. Los
resultados no se guardan solos en una carpeta: *Download results* descarga
un ZIP con `jugadores.csv`, `respuestas.csv` y `preguntas.csv`.

Necesita un relay de Gaanim 0.8 o posterior: si el tuyo es anterior,
actualízalo con `gaanim relay init --force <su carpeta>` y
`npx wrangler deploy`. Los votos de la sesión anterior no se pierden.

El primer navegador o computadora que presenta una sesión se queda con ella:
el relay solo acepta su clave. Si la sesión ya la presentó la aplicación en tu
computadora, pásale su clave al navegador con un enlace:

```bash
gaanim relay key mi-charla.gaanim --src https://tu-sitio.com/mi-charla.gaanim
```

El enlace que imprime termina en `#clave=…`. La página la guarda y la quita
de la barra de direcciones. Quien tenga la clave controla las encuestas de la
sesión: no la compartas con el público.

= Privacidad y límites <privacidad>

Los votos son anónimos: el teléfono guarda un identificador al azar y el relay
no pide nombres ni cuentas; un cuestionario solo pide un apodo, y lo que pida
`scene.roster`. Los resultados guardados tienen los apodos, ese dato y las
respuestas de cada jugador: trátalos como las notas de tu curso. Los votos
de quien no se unió con un apodo solo cuentan en los totales. Lo que cada
teléfono votó lo recuerda el relay, no el teléfono, así que una partida nueva
empieza limpia en todos. El relay borra la sesión y sus votos doce horas
después de su última actividad.

Para que nadie con el código pueda inflar una sesión, una partida admite 500
jugadores y una encuesta 1000 teléfonos, y cada teléfono puede enviar unos
pocos mensajes por segundo. Una sala de unos cientos de teléfonos entra con
holgura en el plan gratuito de Cloudflare.

= Si algo falla <si-algo-falla>

#table(
  columns: (auto, 1fr),
  [*Síntoma*], [*Qué hacer*],
  [El QR no lleva a ninguna parte],
  [La escena se creó sin relay. Configúralo con `gaanim relay use <URL>` y vuelve a cargar o exportar: la dirección queda grabada en el QR.],
  [La terminal dice que el relay está desactualizado],
  [Escríbelo de nuevo con `gaanim relay init --force <carpeta>` y despliégalo otra vez con `npx wrangler deploy`.],
  [Los teléfonos no ven la pregunta],
  [Comprueba que la presentación esté dentro de la encuesta (entre que se abre y su cierre) y que la terminal no avise de un relay inaccesible.],
  [Un teléfono dice que el apodo está ocupado],
  [Alguien ya lo usa en esta sesión; los apodos no distinguen mayúsculas.],
  [Quedaron jugadores de un ensayo anterior],
  [Empieza una partida nueva con `R` dos veces o `gaanim relay reset`.],
  [La vista previa no se parece a la sala],
  [Ajusta `scene.rehearsal`: cuántos jugadores, qué tan bien responden (`skill`) y qué tan rápido (`speed`).],
  [Probar contra un relay local va lento en Windows],
  [Usa `http://127.0.0.1:8787` en vez de `localhost`, que primero intenta IPv6.],
)
