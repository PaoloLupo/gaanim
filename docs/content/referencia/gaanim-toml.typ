#import "../../components/section.typ": docs-chapter

#show: docs-chapter.with(
  title: "Manifiesto gaanim.toml",
  description: "Campos, valores predeterminados y validación del manifiesto de proyecto",
  route: "/referencia/gaanim-toml/",
  nav: "gaanim.toml",
)

= Manifiesto `gaanim.toml`

Una carpeta es un proyecto de Gaanim cuando contiene un `gaanim.toml`. El
manifiesto dice qué script ejecutar y dónde están los recursos y las
salidas. `gaanim init` lo genera así:

```toml
name = "mi-charla"
kind = "slides"
entry = "main.py"
assets_dir = "assets"
output_dir = "exports"
```

== Campos

#table(
  columns: (auto, auto, auto, 1fr),
  inset: 7pt,
  [*Campo*], [*Tipo*], [*Predeterminado*], [*Significado*],
  [`kind`], [cadena], [obligatorio], [Tipo de proyecto: `"video"` o `"slides"`.],
  [`entry`], [ruta], [obligatorio], [Script que ejecutan `gaanim <carpeta>`, `check` y `export`.],
  [`name`], [cadena], [nombre de la carpeta], [Nombre que muestran el Inicio y el título de la ventana.],
  [`assets_dir`], [ruta], [`"assets"`], [Carpeta de recursos, relativa al manifiesto. La lee `scene.assets.load_project()`.],
  [`output_dir`], [ruta], [`"exports"`], [Carpeta donde el diálogo de exportación del editor propone guardar, y donde van las capturas del fotograma.],
  [`language`], [cadena], [ninguno], [Idioma en que se habla la charla, como etiqueta BCP 47 (`"es-ES"`, `"es"`). Lo usa el dictado del guion.],
  [`polls.relay`], [URL], [ninguno], [Relay de las encuestas de este proyecto, en una tabla `[polls]`.],
)

Las claves desconocidas se ignoran. Las que parecen credenciales, porque su
nombre contiene `token`, `secret`, `password` o `api_key`, también, pero
`gaanim check` y el panel *Script & narration* avisan de ellas: el manifiesto
viaja con el proyecto, también a un repositorio, así que los tokens no van
ahí (ver #link("/guias/presentaciones/#token-scribe")[El token]).

=== `kind`

`"video"` y `"slides"` distinguen los dos tipos de proyecto en el Inicio y en
la plantilla de `gaanim init`. No cambian cómo se ejecuta ni cómo se exporta la
escena: `gaanim check` aplica las comprobaciones de presentación cuando la
escena usa segmentos, sea cual sea su `kind`. Los valores antiguos
`"presentation"` y `"thesis"` producen un error que pide cambiarlos por
`"slides"`; cualquier otro valor, un error con los tipos disponibles.

=== `entry`

Ruta relativa a la carpeta del proyecto. Debe apuntar a un archivo que exista
y no puede ser absoluta ni salir de la carpeta con `..`. Así, `gaanim .`,
`gaanim check .` y `gaanim export . --output …` no necesitan la ruta del script.

=== `assets_dir`

El editor y la CLI no aplican `assets_dir` por su cuenta: la escena lo carga al
llamar a `scene.assets.load_project()`, que busca el `gaanim.toml` en la
carpeta del script que la llama y, si no está, en sus carpetas superiores, como
hace `git` con `.git`: un módulo dentro de un paquete del proyecto encuentra el
manifiesto del proyecto. Después, las rutas relativas de `scene.media.image`, `svg`, `lottie`,
`video` y demás se resuelven desde esa carpeta, sin depender del directorio de
trabajo desde el que se lanzó Gaanim.

```python
from gaanim import Scene

scene = Scene()
scene.assets.load_project()             # lee assets_dir del gaanim.toml del proyecto
logo = scene.media.image("logo.webp")   # es assets/logo.webp
```

`load_project` lee solo la clave `assets_dir` de nivel superior, escrita como
cadena entre comillas; si el manifiesto no la tiene, usa `"assets"`, igual que
la CLI y el editor. En un #link(<workspace>)[workspace], sin argumento, carga
el proyecto del script que se ejecuta, así que un módulo compartido que la
llama no cambia los recursos de la escena al `gaanim.toml` del workspace.
Consulta
#link("/referencia/assets/#api-assetmanager-load-project")[`AssetManager.load_project`].

=== `output_dir`

Carpeta relativa al proyecto, o absoluta. El diálogo de exportación del editor
propone guardar ahí, con el nombre del script o, si es un `main.py`, el de
su carpeta (`<output_dir>/mi-video.mp4`), y las capturas del fotograma actual
van a `<output_dir>/captures/`. `gaanim export` sin `--output` también
escribe ahí, con el mismo nombre; con `--output`, en esa ruta, relativa al
directorio actual.

=== `language` <language>

```toml
language = "es-ES"
```

El idioma en que se habla la charla del proyecto, como etiqueta BCP 47:
`"es-ES"`, `"es"`, `"es-PE"`, `"en-US"`… Al
#link("/guias/presentaciones/#dictar")[dictar el guion con Scribe] manda sobre
el idioma elegido en el editor, así que la charla se dicta igual en cualquier
equipo. Scribe usa la misma etiqueta si la tiene y, si no, la primera de ese
idioma: `"es"` y `"es-PE"` se dictan como `es-ES`, y `"pt"` como `pt-BR`. Si
Scribe no transcribe ese idioma, se usa el del editor y el panel lo avisa.

En *Settings → Dictation*, con el proyecto abierto en el editor, *Pin in gaanim.toml* escribe el
idioma elegido y *Unpin* lo quita; el resto del archivo, comentarios
incluidos, queda como estaba.

=== `[polls]`

```toml
[polls]
relay = "https://gaanim-relay.<tú>.workers.dev"
```

Relay al que apuntan los códigos QR de las encuestas (`scene.poll`) de este
proyecto, en lugar del guardado con `gaanim relay use`. `GAANIM_POLL_RELAY` tiene prioridad
sobre ambos. Ver
#link("/publico/primera-encuesta/")[Tu primera encuesta].

== `[workspace]` <workspace>

Un workspace reúne varios proyectos en un repositorio: comparten código de
Python, recursos y entorno, y cada uno se abre, se comprueba y se exporta por
separado. Es el `gaanim.toml` de la carpeta que los contiene, con una tabla
`[workspace]`. Ningún nombre de carpeta es especial: la estructura y los
nombres son tuyos, y el workspace los describe.

```toml
# channel/gaanim.toml
name = "channel"

[workspace]
members = [
  { path = "season-1/*", group = "Season 1" },
  { path = "season-2/*", group = "Season 2" },
  "events/*/talk",
  "trailer",
]
exclude = ["season-2/draft"]

[workspace.shared]
python = ["common"]
assets = ["media", "brand"]
```

`gaanim init workspace` crea uno con `members = ["videos/*", "talks/*"]`,
`src/` y `assets/`, y un primer miembro en `videos/intro` (ver
#link("/guias/proyectos/#workspaces")[Varios proyectos en un repositorio]).

#table(
  columns: (auto, auto, auto, 1fr),
  inset: 7pt,
  [*Campo*], [*Tipo*], [*Predeterminado*], [*Significado*],
  [`members`], [lista], [obligatorio], [Dónde están los proyectos: rutas relativas al workspace, a cualquier profundidad. `*` vale por cualquier parte de un nombre de carpeta en ese tramo (`season-1/*`, `events/*/talk`, `ep-*`). Una entrada puede ser una tabla `{ path, group }`.],
  [`exclude`], [lista], [ninguna], [Rutas o patrones que `members` no debe incluir.],
  [`shared.python`], [lista], [`["src"]` si existe], [Carpetas que entran en `sys.path` de cada miembro, después de las suyas.],
  [`shared.assets`], [lista], [`["assets"]` si existe], [Carpetas donde cada miembro busca una ruta relativa que no está en las suyas.],
  [`cover`], [ruta], [`cover.png` (o `.jpg`, `.webp`) si existe], [Imagen PNG, JPEG o WebP con la que el Inicio muestra el workspace. Sin ella, el Inicio compone las portadas de hasta cuatro miembros.],
)

- *Miembros:* una carpeta que encaja en `members` es miembro si tiene su
  propio `gaanim.toml`; las demás, como una carpeta de guiones, se ignoran.
  Un miembro es un proyecto normal: su `kind`, `entry`, `assets_dir`,
  `output_dir`, `language` y `[polls]` siguen siendo suyos. Lo decide solo el
  workspace: un proyecto anidado que `members` no incluye sigue siendo
  independiente.
- *Grupos:* `group` es el nombre con el que se agrupan sus miembros; sin él,
  el grupo es la carpeta que contiene al miembro.
- *Carpetas compartidas:* pueden llamarse como quieras y estar a cualquier
  profundidad, pero dentro del workspace; una que no existe es un error.
  `src` y `assets` son solo los valores por defecto, los mismos nombres que
  usa un proyecto suelto.
- *El manifiesto del workspace* puede no tener `kind` ni `entry`. Si los
  tiene, también es un proyecto, que se abre como cualquier otro.

Lo que hereda cada miembro:

- `sys.path` es `[miembro/src, carpeta del script, miembro, <shared.python>…,
  workspace]`: lo del miembro va primero y gana si hay nombres repetidos.
  Guardar un módulo de una carpeta compartida recarga el miembro abierto,
  como sus propios módulos.
- La vista previa vigila el miembro, las carpetas compartidas y el
  `gaanim.toml` del workspace, no el repositorio entero: guardar un archivo
  de otro miembro no recarga el abierto.
- Una ruta relativa de un recurso que no está en las carpetas del miembro se
  busca en `shared.assets`, en su orden. También las fuentes, los fondos de
  shader, las transiciones y los postprocesos.
- El entorno de Python: el `.venv` del miembro si tiene uno; si no, el del
  workspace, que Gaanim crea en su raíz.

== Cómo se encuentra el proyecto

- `gaanim <carpeta>` exige un `gaanim.toml` en esa carpeta. Si es un
  workspace sin `entry`, abre el Inicio en la página del workspace, con sus
  miembros.
- `gaanim <script.py>` ejecuta ese script y busca un `gaanim.toml` en su
  carpeta y en hasta cinco carpetas superiores para mostrar el nombre del
  proyecto y proponer la carpeta de exportación. Un manifiesto que solo fija
  `assets_dir` para `load_project`, sin `kind`, `entry` ni `[workspace]`, no
  es un proyecto y se salta.
- El workspace de un proyecto es el primer `gaanim.toml` con `[workspace]`
  por encima de él cuyos `members` lo incluyen. La búsqueda se detiene en la
  carpeta que contiene `.git` o en la raíz del disco.

Un manifiesto que no es TOML válido, sin `kind` o `entry`, o con valores
inválidos detiene `gaanim`, `check` y `export` con un mensaje que incluye la
ruta del manifiesto y el código de salida `2`, tanto al abrir una carpeta
como al abrir un script de un proyecto con ese manifiesto.
