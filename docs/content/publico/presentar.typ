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
`Parameter`. El reproductor web todavía no recibe votos.

= Privacidad y límites <privacidad>

Los votos son anónimos: el teléfono guarda un identificador al azar y el relay
no pide nombres ni cuentas; un cuestionario solo pide un apodo. Lo que cada
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
