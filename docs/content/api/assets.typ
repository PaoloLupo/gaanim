#import "../../components/section.typ": docs-chapter

#show: docs-chapter.with(
  title: "Recursos",
  description: "Rutas portables de imágenes, SVG, Lottie y glTF, manifiestos y precarga",
  route: "/api/assets/",
  code-langs: (),
)

= Recursos

Define un directorio de recursos por escena para que las rutas relativas de
imágenes, SVG, Lottie JSON y glTF sigan siendo portables al mover el proyecto o renderizarlo
desde otro directorio de trabajo.

```python
from gaanim import Scene

scene = Scene()
scene.assets.assets_dir("assets")

logo = scene.media.svg("logo.svg")
cover = scene.media.image("cover.png")
robot = scene.media.gltf("robot.glb")
pulse = scene.media.lottie("pulse.json")
```

Las rutas absolutas también funcionan y tienen prioridad sobre `assets_dir`.

== Calidad de imágenes y vídeo

`scene.media.image` y `scene.media.video` aceptan `quality="low"`, `"medium"` (el valor
predeterminado) o `"high"`. La calidad alta solicita muestreo bicúbico de
Vello 0.9, especialmente útil para fotos, capturas y vídeo que se escalan o
rotan; tiene un coste de GPU mayor que la calidad media.

```python
hero = scene.media.image("cover.png", width=8, quality="high")
clip = scene.media.video("intro.mp4", height=4.5, quality="high", audio=False)
scene.play([clip])
```

Los valores de calidad no válidos, las dimensiones no positivas y los crops
fuera de la imagen producen `ValueError`.

Los SVG importados pueden usarse como máscaras y operandos de booleanas porque
Gaanim conserva sus paths vectoriales. Las etiquetas SVG `<mask>` basadas en
alfa o luminancia no se convierten a composición ráster; usa una silueta
vectorial explícita para ese caso.

== Manifiesto del proyecto

Crea un proyecto completo desde la CLI:

```text
gaanim init video my-video
gaanim init slides my-deck
```

Cada proyecto contiene `main.py`, `gaanim.toml`, `pyproject.toml`,
`.python-version` (3.14), `assets/`, `exports/`, un README y un `.gitignore`.
El proyecto Python parte de `uv init --bare --python 3.14` y declara `gaanim`
como dependencia en `pyproject.toml`. El manifiesto generado es:

```toml
name = "my-deck"
kind = "slides"
entry = "main.py"
assets_dir = "assets"
output_dir = "exports"
```

Después, cárgalo antes de crear objetos dibujables:

```python
scene = Scene()
scene.assets.load_project()  # lee gaanim.toml junto a este script de Python
```

La CLI acepta el script de entrada o el directorio del proyecto (`gaanim my-deck`,
`gaanim check my-deck`). El directorio de recursos se resuelve respecto al
manifiesto, no respecto al directorio de trabajo del proceso. Sin argumentos,
`load_project()` busca el manifiesto junto al script que hizo la llamada, aunque
Gaanim se haya iniciado desde otro lugar. `load_project("path/to/gaanim.toml")`
acepta una ruta explícita al manifiesto.

== Precarga

Usa `preload` para validar archivos ráster, SVG, Lottie JSON y glTF antes de
reproducir la escena. Las imágenes ráster se decodifican en la misma caché que
usa `scene.media.image`; las composiciones Lottie se analizan en la caché que usa
`scene.media.lottie`.

```python
scene.assets.preload(["logo.svg", "cover.png", "pulse.json"])
```

Los errores identifican el recurso que no pudo resolverse o decodificarse. La
escena consume actualmente `assets_dir`; `name`, `kind`, `entry` y `output_dir`
describen el flujo del proyecto y reservan espacio para futuros perfiles de
exportación.

== Actualización de archivos modificados

En el editor no hace falta: guardar un asset del proyecto vacía las cachés y
recarga la escena completa. Fuera del editor, cuando un recurso ráster o Lottie
cambia en disco sin reiniciar el proceso, limpia las cachés antes de reconstruir
los objetos afectados:

```python
scene.assets.reload_assets()
cover = scene.media.image("cover.png")
pulse = scene.media.lottie("pulse.json")
```

Los archivos SVG vuelven a analizarse cada vez que `scene.media.svg(...)` crea un
objeto dibujable.

Los metadatos glTF se guardan por ruta canónica y fecha de modificación.
`reload_assets()` también limpia esta caché; el editor elimina la instancia
nativa anterior y todos sus descendientes antes de reconstruirla.

== Lottie JSON y dotLottie

`MediaLibrary.lottie` acepta archivos Lottie en formato JSON y paquetes `.lottie` v1/v2 y los compone directamente en
Vello mediante Velato. Los parámetros `width`, `height` y `fit` siguen la misma
semántica que en imágenes; `offset`, `duration`, `loop` y `speed` controlan el
intervalo reproducido. Activa el clip con `scene.play([clip])`.
La reproducción sigue el tiempo absoluto de la escena, incluso al declararlo
antes de otras pausas o segmentos y al saltar hacia atrás en el timeline.

Puedes introducir el primer fotograma seleccionado con `fade_in`, `write` o
`create` antes de reproducir el clip. `write` y `create` revelan los contornos
vectoriales en paralelo y después incorporan sus rellenos originales. Las
imágenes ráster aparecen durante la fase de relleno. La introducción no inicia
por sí sola la animación interna del JSON:

```python
from gaanim import Scene, sequence

scene = Scene()
clip = scene.media.lottie("pulse.json")
scene.play(sequence(clip.animate.create().duration(1.0), clip))
```

El soporte inicial prioriza formas vectoriales, transformaciones, rellenos,
trazos y máscaras. Las capas sólidas se dibujan con su color y tamaño tanto en
la composición principal como dentro de precomposiciones. Los assets de imagen
externos exportados por Cavalry también funcionan en ambos niveles y se
resuelven respecto al archivo JSON, incluido su subdirectorio `images/`.
Sólidos e imágenes conservan el orden de capa, transformaciones, opacidad,
tiempo local de la precomposición y máscaras simples.
Los rellenos y trazos con gradientes lineales o radiales conservan sus color
stops y opacity stops independientes, tanto estáticos como animados.
Texto, imágenes embebidas y efectos todavía pueden omitirse o aproximarse.
Consulta `clip.warnings` para detectar esas diferencias.
Cuando una transformación de capa o grupo omite su posición o escala, se usa
una traslación de cero o una escala del 100 %, también dentro de precomposiciones.
Las posiciones separadas X/Y de una capa sí se reproducen; si Velato encuentra
otra construcción que no puede convertir de forma segura, la carga devuelve un
error en vez de abortar el proceso.

=== Paquetes `.lottie`

Los paquetes se leen en memoria, incluidas sus imágenes PNG/JPEG/WebP; no se
extraen al disco ni se buscan recursos externos al ZIP. `scene.assets.preload` y
`scene.assets.reload_assets` también admiten estos paquetes. La caché comparte
recursos, pero cada clip mantiene sus propias entradas y operaciones.

`animation_id` selecciona una animación y `state_machine_id` una máquina;
son mutuamente excluyentes. Sin selección explícita se utiliza el contenido
inicial del manifiesto y, en su ausencia, la primera animación. `theme_id`
selecciona el tema inicial. Para reproducción simple se usan las opciones de
Gaanim; no se aplican los controles de reproducción del manifiesto v1.
`clip.animation_ids`, `clip.theme_ids` y
`clip.state_machine_ids` enumeran los identificadores disponibles.

```python
clip = scene.media.lottie("button.lottie", state_machine_id="main", width=4)
clip.set_input("active", False)
scene.play([clip])
scene.wait(1)
clip.set_input("active", True).set_theme("gold")
scene.wait(1)
clip.fire_event("reset").set_theme(None)
scene.wait(1)
```

`set_theme(id)`, `set_input(name, value)` y `fire_event(name)` devuelven el mismo
clip y registran la operación en el cursor sin avanzar el tiempo. Antes de
activar el clip, tema y entradas configuran el estado inicial; los eventos
requieren activación. Las operaciones simultáneas mantienen su orden de
declaración y se reproducen al buscar cualquier instante o exportar.

Activar una máquina no alarga la escena: usa `scene.wait` u otras animaciones.
El estado controla velocidad, dirección, bucles, segmento por marcador y fondo;
no se aceptan valores globales de `offset`, `duration`, `loop` o `speed`
distintos de sus valores predeterminados. Un estado final detiene las
transiciones. Un marcador ausente utiliza el intervalo completo de la animación.

Esta versión admite `PlaybackState`, `GlobalState`, transiciones inmediatas y
condiciones numéricas, booleanas, de cadena y evento. Los temas admiten valores
estáticos de color, escalar, posición, vector y gradiente. No admite acciones,
interacciones de ratón, entradas automáticas reservadas, transiciones suavizadas,
temas con keyframes/expresiones ni sustituciones temáticas de imágenes o texto.
Estas funciones provocan `ValueError` cuando se selecciona el tema o máquina
que las contiene; otras animaciones del paquete siguen disponibles. Los ciclos
de transiciones instantáneas también se rechazan. Se conservan las limitaciones
visuales y `warnings` del importador JSON, junto con `Write` y `Create`.

Ejemplo ejecutable: `examples/dotlottie_demo.py`.

== Modelos 3D glTF

`MediaLibrary.gltf(path, *, scene=None) -> Drawable` importa archivos locales glTF 2.0
con extensión `.gltf` o `.glb`. `scene` acepta el nombre de una escena, un índice
basado en cero o `None` para usar la escena predeterminada del archivo.

```python
model = scene.media.gltf("robot.glb", scene="Presentation")
arm = model.part("Robot/Rig/Arm")

print(model.parts())       # tupla de selectores estables
print(model.animations())  # nombres de acciones de Blender
```

El nombre corto de un nodo solo está disponible cuando es único. Una ruta
jerárquica desambigua los nombres repetidos; las rutas completas duplicadas
reciben el sufijo estable `#<node-index>`. Los errores de búsqueda enumeran los
selectores candidatos.

Gaanim conserva las unidades exportadas, la orientación, la jerarquía de nodos,
los materiales PBR metallic-roughness, normales, UV, texturas, skins, huesos y
morph targets. Una unidad glTF equivale a una unidad del mundo de Gaanim; los
modelos no se centran ni escalan automáticamente. Las cámaras y luces importadas
se eliminan en favor de la cámara de Gaanim y su iluminación neutra
predeterminada. Las extensiones glTF no compatibles y los buffers o texturas
externos ausentes producen un error que incluye la ruta de origen.

La carga visual usa el importador glTF nativo de Bevy. Gaanim crea un contenedor
estable por nodo: las transformaciones manuales del contenedor se componen sobre
la transformación creada en Blender y sobre la animación esquelética o morph,
sin sobrescribirlas. Los materiales se clonan por instancia para que una
animación de opacidad no pueda modificar otra importación del mismo archivo.

== SVG avanzado

`scene.media.svg(...)` conserva el documento como geometría vectorial. El importador
resuelve:

- grupos anidados, CSS, transformaciones, `viewBox` y `<use>`;
- rellenos y trazos sólidos, lineales y radiales, incluidos los modos de
  extensión del gradiente;
- geometría `clipPath` aplicada sin rasterizar el documento;
- texto SVG convertido en contornos mediante las fuentes instaladas;
- filtros habituales `feGaussianBlur` y `feDropShadow` mediante los efectos
  vectoriales retenidos de Gaanim.

Los grosores aplicados al drawable raíz con `stroke(...)` se expresan en
unidades lógicas de escena y no se reducen al escalar la geometría fuente del
SVG para colocarla en el frame.

Los grupos, trayectorias y textos con nombre siguen siendo direccionables:

```python
diagram = scene.media.svg("architecture.svg")
diagram.part("database").animate.indicate().duration(0.6)
diagram.part("caption").animate.opacity(0.5)
```

La importación prioriza deliberadamente los vectores. Todavía no conserva
patrones de pintura, máscaras de luminancia o alfa, imágenes ráster incrustadas
ni grafos arbitrarios de filtros SVG. Para obtener contornos de texto portables,
instala la fuente solicitada en cada máquina de renderizado o convierte el texto
en trayectorias dentro del SVG de origen.


Las imágenes devuelven `Image` y los videos `Video`, ambos compatibles con
`Drawable`. Consulta la API de medios para `frame`, `crop`, `quality`, metadatos
de origen y `Video.segment`. Estas operaciones usan archivos locales y respetan
`assets_dir`; no descargan URLs ni aceptan arrays o bytes.
