#import "../../components/section.typ": docs-chapter

#show: docs-chapter.with(
  title: "Solución de problemas",
  description: "Qué hacer cuando una escena no se abre, no se ve como esperas o no se exporta",
  route: "/apendices/solucion-de-problemas/",
)

Los problemas más habituales, agrupados por el momento en que aparecen. Para
cada uno se indica la causa y qué hacer. Antes de nada, ejecuta
`gaanim check <script-o-proyecto>`: ejecuta la escena sin ventana y muestra el
error de Python con su línea.

= Instalación y Python

Los errores al arrancar (`Python 3.14 was not found`, una versión de Python
incorrecta, `python3.dll` no encontrado, `authoring environment not ready`)
están explicados en la sección de problemas de
#link("/empezar/instalacion/")[Instalación].

- *`python main.py` falla:* es lo esperado. El paquete de Python solo contiene los tipos para tu editor; las
  escenas se ejecutan siempre con `gaanim main.py` o `gaanim .`.
- *Mi editor no autocompleta la API:* configura como intérprete el `.venv`
  del proyecto, donde `gaanim init` instala el paquete de autoría.

= Abrir la escena

- *`gaanim check: could not load project`:* revisa que `entry` en
  `gaanim.toml` sea una ruta relativa a un archivo que exista dentro del
  proyecto.
- *`TypeError` al crear `Scene(1920, 1080)` o `Scene(1280, 720)`:* las
  escenas ya no se definen en píxeles. Usa `Scene(frame=(16, 9))` y elige la
  resolución al exportar (ver #link("/apendices/novedades/")[Novedades]).
- *`AttributeError` con `scene.circle`, `scene.equation` y similares:* las
  fábricas viven en capacidades: `scene.geometry.circle`,
  `scene.text.equation`… La tabla de equivalencias está en
  #link("/apendices/novedades/")[Novedades].
- *No se encuentra una imagen, un SVG o un modelo:* las rutas relativas se
  resuelven desde el directorio de trabajo salvo que cargues el proyecto con
  `scene.assets.load_project()` (lee el `gaanim.toml` junto al script) o fijes
  una carpeta absoluta con
  `scene.assets.assets_dir(str(Path(__file__).parent / "assets"))`.
- *`script did not submit a scene`:* el script debe terminar llamando a
  `scene.render()`.

= La escena no se ve como esperas

- *Un objeto no aparece:* las líneas reactivas, los trazos
  (`traced_path`), los objetos con `follow` y los puntos sobre curvas empiezan
  ocultos; añade su `fade_in` o `create` a un `scene.play`.
- *Una animación no hace nada:* `.animate` solo describe el cambio. Pásalo a
  `scene.play([...])`.
- *Dos animaciones del mismo objeto chocan:* no puedes animar el mismo canal
  (por ejemplo, la posición) dos veces a la vez. El error nombra el objeto y
  las dos animaciones; combínalas en una o ponlas en secuencia.
- *Los objetos son enormes o diminutos:* las medidas están en unidades
  lógicas, no en píxeles. El fotograma mide 16 × 9; un círculo de radio `1`
  ocupa una octava parte del ancho. Un SVG se importa a 100 px por unidad
  (uno de 300 px mide 3 unidades); ajústalo con `scale_to`.
- *Todo sale en blanco:* una escena creada con `Scene(theme=None)` (o tras
  `scene.canvas.set_theme(None)`) no tiene tema: el fondo es blanco y el texto,
  las formas y los ejes sin color propio también son blancos. Quita
  `theme=None` para volver al tema `technical` predeterminado, elige otro tema
  o pon un fondo que contraste; `gaanim check` lo avisa.
- *Mis objetos tienen otro color del que esperaba:* las escenas usan por
  defecto el tema `technical`, que rellena las formas sin color con el acento
  y pinta texto y ejes en gris claro. Un `.fill(...)`, `.stroke(...)` o
  `color=` explícito siempre gana al tema.
- *Un texto o una tarjeta se salen de su sitio dentro de un layout:* no uses
  `move_to()` en objetos dentro de un layout; expresa la intención con
  `align`, `offset` o una restricción (ver #link("/guias/layout/")[Layout]).
- *El movimiento cambia al saltar a otro instante:* alguna función de
  `computed`, `readout` o `add_updater_fn` guarda estado o lee el reloj.
  Consulta las reglas de #link("/guias/reactividad/")[Reactividad].
- *Una lectura numérica muestra `invalid`:* su función lanzó una excepción o
  devolvió un valor no finito en ese instante.

= Recarga y vista previa

- *Guardé y no cambió nada:* comprueba que el archivo está dentro del
  proyecto y no en una carpeta ignorada (`exports`, `snapshots`, `target`,
  carpetas ocultas o de entornos). Los paquetes instalados fuera del proyecto
  no se recargan hasta reiniciar Gaanim.
- *La recarga tarda más de lo normal:* los segmentos con funciones de Python
  (updaters, funciones reactivas, easings propios) se recompilan siempre.
  Para comparar con una recarga completa, arranca con `GAANIM_INCREMENTAL=0`.
- *Necesito los mensajes del motor:* Gaanim oculta los avisos de Bevy, wgpu,
  winit y el audio. Para verlos, arranca con `RUST_LOG=info` (o
  `RUST_LOG=wgpu=warn` para un solo módulo). `NO_COLOR=1` quita los colores;
  el logo solo aparece cuando la salida es una terminal.
- *En la vista del presentador no veo los objetos 3D:* sus vistas previas
  solo dibujan las capas 2D; la audiencia sí los ve. La API 3D es
  experimental (ver #link("/guias/camara-y-3d/")[Cámara y 3D]).

= Exportar

- *La exportación dice que `ffmpeg` no está en el `PATH`:* instala FFmpeg y
  añádelo al `PATH`, o exporta una secuencia PNG
  (`--output frames/frame.png`), que no lo necesita.
- *Error por la proporción de la salida:* si `--width` y `--height` no
  tienen la proporción de la escena, añade `--fit contain` (con bandas) o
  `--fit cover` (recortando).
- *El video se detiene en las pausas o no lo hace:* la exportación ignora
  siempre `scene.stop(...)` y produce un video continuo; solo el modo
  presentación espera en ellas.
- *`--from` o `--to` fallan:* los valores deben ser segundos no negativos o
  nombres de `scene.marker(...)`, y `--to` debe ser mayor que `--from`.
- *Un fotograma no se pudo dibujar:* en escenas muy densas, Gaanim reintenta
  con más memoria y, si no basta, falla indicando el fotograma. Prueba una
  resolución menor o reduce la cantidad de objetos translúcidos.
- *Transparencia:* `--transparent` solo funciona con WebM, WebP y PNG, y la
  escena necesita un fondo transparente.

= Capturas y comparación

- *`--capture-stops` dice que no hay nada que comparar:* la versión aprobada
  no tiene `stops.json`. Aprueba una captura de pausas con
  `--capture-stops --bless`.
- *Todas las capturas cambian por diferencias mínimas:* ajusta
  `--pixel-threshold` y `--max-changed-ratio` (ver
  #link("/guias/capturas-y-comparacion/")[Capturas y comparación visual]).

Si tu problema no aparece aquí, abre un aviso en el repositorio de Gaanim con
el mensaje de error completo y el script mínimo que lo reproduce.
