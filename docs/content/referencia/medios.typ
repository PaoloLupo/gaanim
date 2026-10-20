#import "../../components/section.typ": docs-chapter
#import "../../components/api.typ": api-entry

#show: docs-chapter.with(
  title: "Medios",
  description: "Fábricas de scene.media: imágenes, SVG, modelos glTF, video y Lottie",
  route: "/referencia/medios/",
  nav: "Medios",
)

= Medios

`scene.media` carga archivos locales como objetos de la escena: imágenes, SVG,
modelos glTF, video y animaciones Lottie. Todos son `Drawable`, así que se
colocan, escalan y animan como cualquier forma. Una ruta relativa se resuelve
desde la carpeta de recursos de la escena si se fijó con
`scene.assets.load_project()` o `scene.assets.assets_dir(...)`; si no, desde el
directorio de trabajo, la carpeta `assets/` del script y la carpeta del script,
en ese orden (ver #link("/referencia/assets/")[Recursos]); el audio está en
#link("/referencia/audio/")[Audio].

```python
# show-code: true
from gaanim import Scene

scene = Scene(frame=(16, 9), background="#0f172a")
photo = scene.media.image("assets/cover.png").frame(6, 3.4, fit="cover").move_to(-3.5, 0)
robot = scene.media.svg("assets/robot.svg").scale_to(2).move_to(3.5, 0)
scene.play([photo.animate.fade_in(), robot.animate.write()], duration=1.2)
# output: preview.webp
scene.render()
```

== Imágenes

PNG, JPEG y WebP con marco, recorte y calidad de muestreo. La textura decodificada
se comparte entre objetos que usan la misma ruta.

#api-entry(
  name: "MediaLibrary.image",
  kind: "factory",
  params: ((name: "path", type: "str", default: none, desc: [Archivo PNG, JPEG o WebP.]), (name: "width / height", type: "float | None", default: "None", desc: [Tamaño de destino en unidades de escena; sin ellos, la imagen cabe en el área segura.]), (name: "fit", type: "str", default: "\"contain\"", desc: [Con `width` y `height`, como en `Image.frame`: `contain` conserva la imagen entera, `cover` llena el marco recortando el sobrante por igual a ambos lados y `stretch` la deforma.]), (name: "crop", type: "(x, y, w, h) | None", default: "None", desc: [Recorte de la fuente en píxeles, con origen arriba a la izquierda.]), (name: "quality", type: "str", default: "\"medium\"", desc: [Muestreo: `low` (vecino más cercano), `medium` (bilineal) o `high` (bicúbico).])),
  desc: [Crea una `Image` que conserva su tipo al encadenar setters. Ajustes, recortes, medidas o calidades inválidos lanzan `ValueError`.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
logo = scene.media.image("assets/logo.webp", width=3)
banner = scene.media.image("foto.jpg", width=8, height=2, fit="cover")
```
]

#api-entry(
  name: "Image.frame",
  kind: "method",
  desc: [Fija un marco centrado en unidades de escena. `contain` conserva la proporción con bandas transparentes, `cover` llena el marco recortando el sobrante y `stretch` deforma. Después de congelarse la declaración, crea un corte reversible en el cursor.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
foto = scene.media.image("foto.jpg").frame(8, 4.5, fit="cover")
```
]

#api-entry(
  name: "Image.crop",
  kind: "method",
  desc: [Selecciona un rectángulo de la fuente sin mover el marco. El origen es la esquina superior izquierda, con Y hacia abajo; `normalized=True` usa fracciones del tamaño original. `obj.animate.crop(...)` interpola el recorte para hacer paneos o zoom dentro del marco. Rectángulos fuera de la imagen lanzan `ValueError`.],
)[
```python
# show-code: true
from gaanim import Scene
scene = Scene(frame=(16, 9), background="#0f172a")
foto = scene.media.image("foto.jpg").frame(8, 4.5, fit="cover")
scene.play([
    foto.animate.crop(0.25, 0.25, 0.5, 0.5, normalized=True).duration(1.5)
])
# output: preview.webp
scene.render()
```
]

#api-entry(
  name: "Image.point_ref",
  kind: "method",
  signature: "point_ref(x, y, *, normalized=False) -> PointRef",
  params: (
    (name: "x, y", type: "float", default: none, desc: [Píxel de la fuente, desde la esquina superior izquierda con Y hacia abajo, en la resolución del archivo.]),
    (name: "normalized", type: "bool", default: "False", desc: [`True` toma `x` e `y` como fracciones del tamaño original.]),
  ),
  returns: (type: "PointRef", desc: [Un punto que sigue a ese detalle.]),
  desc: [Un punto sobre un detalle de la imagen que se recalcula en cada fotograma a través del marco, el ajuste y el recorte actual: se queda en el mismo detalle mientras `animate.crop` hace un paneo o un zoom (un Ken Burns), y sigue a la imagen y a sus grupos si se mueven, escalan o giran. Sirve donde va un `PointRef`: `geometry.connector`, `line`, `follow`, `camera.pan_to` o `camera.inset(..., follow=True)`. A diferencia de `Drawable.pixel`, que mide con el recorte del momento en que se llama, sigue un recorte posterior o animado. Coordenadas no finitas lanzan `ValueError`.],
  none,
)

#api-entry(
  name: "Image.region",
  kind: "method",
  signature: "region(x, y, width, height, *, normalized=False) -> Drawable",
  params: (
    (name: "x, y, width, height", type: "float", default: none, desc: [Rectángulo de la fuente en píxeles, contado como en `point_ref`.]),
    (name: "normalized", type: "bool", default: "False", desc: [`True` usa fracciones del tamaño original.]),
  ),
  returns: (type: "Drawable", desc: [Un objeto invisible sobre esos píxeles.]),
  desc: [Un objeto que no dibuja nada y se queda sobre un rectángulo de la imagen como `point_ref` sobre su punto, a través de recortes, zooms y transformaciones. Sirve de objetivo para `annotate`, `scene.fx.spotlight`, `geometry.surrounding_rect` y los conectores, como cualquier otro objeto, así que una marca sigue al detalle durante un Ken Burns. Una región que el recorte deja fuera del marco visible queda fuera también. Un rectángulo vacío o valores no finitos lanzan `ValueError`.],
)[
```python
# show-code: true
from gaanim import CORAL, Scene
scene = Scene(frame=(16, 9), background="#0f172a")
foto = scene.media.image("foto.jpg").frame(8, 4.5, fit="cover")
detalle = foto.region(0.55, 0.3, 0.15, 0.2, normalized=True)
aro = detalle.annotate.circle(color=CORAL)
nota = scene.text("detalle", size=0.5).move_to(-5.5, 3)
scene.geometry.connector(nota, foto.point_ref(0.62, 0.4, normalized=True)).fill(CORAL)
scene.play([aro.animate.create().duration(0.6)])
scene.play([foto.animate.crop(0.4, 0.2, 0.4, 0.4, normalized=True).duration(2)])
# output: preview.webp
scene.render()
```
]

#api-entry(
  name: "Image.quality",
  kind: "method",
  desc: [Cambia la calidad de muestreo (`"low"`, `"medium"` o `"high"`) en el cursor.],
  none,
)

#api-entry(
  name: "Image.source_width / source_height",
  kind: "property",
  signature: "source_width: int · source_height: int",
  desc: [Tamaño original en píxeles, independiente del recorte y las transformaciones.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
foto = scene.media.image("foto.jpg")
aspect = foto.source_width / foto.source_height
```
]

== SVG

Ilustraciones vectoriales importadas como una jerarquía real de caminos y grupos,
con acceso a cada parte por su `id`.

#api-entry(
  name: "MediaLibrary.svg",
  kind: "factory",
  desc: [Importa geometría, degradados, transformaciones, `clipPath` y `feGaussianBlur` como caminos vectoriales normales. El documento se importa a 100 píxeles SVG por unidad lógica, la misma equivalencia que usa el resto del motor (el trazo por defecto de 0.03 equivale a 3 px), con el centro del documento en el origen. `scale_to` y `rotate_to` usan como pivote el centro de lo que el SVG dibuja, no el del documento: si el dibujo no está centrado en su lienzo, tras escalarlo el centro del documento ya no queda en el origen; colócalo con `move_to`, que usa el centro visual, o fija el pivote con `with_pivot(0, 0)` antes de escalar. Las partes (`part(id)`) se colocan después de la raíz, así que moverlas no cambia dónde queda el SVG. El tamaño en píxeles es el `width`/`height` del documento, o el `viewBox` si faltan: un SVG de 200 × 200 px mide 2 × 2 unidades en el marco de 16 × 9. Trazos, degradados, `clipPath`, texto y filtros escalan con la geometría. `scale_to(factor)` cambia el tamaño de todo el SVG; los anchos de trazo fluidos aplicados a la raíz o a una parte (`part(id)`), fijos o animados, siguen en unidades lógicas de escena aunque el SVG se escale. Como todo trazo, ninguno sigue una animación de escala posterior. Los detalles de compatibilidad están en #link("/referencia/assets/")[Recursos].],
)[
```python
# show-code: true
from gaanim import BLUE, Scene
scene = Scene(frame=(16, 9), background="#f8fafc")
# robot.svg mide 200 px, es decir 2 unidades: scale_to(2.5) lo deja en 5
robot = scene.media.svg("assets/robot.svg").scale_to(2.5).move_to(0, 0)
robot.part("left-arm").fill(BLUE)
scene.play([robot.animate.write().duration(0.8)])
scene.play([robot.part("head").animate.shift_by(0, 0.4).duration(0.6)])
# output: preview.webp
scene.render()
```
]

#api-entry(
  name: "Drawable.part",
  kind: "method",
  desc: [Devuelve la parte con ese `id` de un SVG, o el nodo con ese nombre de un modelo glTF, como un `Drawable` propio. Los nombres distinguen mayúsculas; un nombre desconocido lanza `KeyError` con la lista de nombres disponibles, y un objeto sin partes, `ValueError`. El estilo de un grupo llega a todos sus caminos.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
robot = scene.media.svg("assets/robot.svg").scale_to(2)
head = robot.part("head").fill(GOLD)
```
]

#api-entry(
  name: "Drawable.parts",
  kind: "method",
  desc: [Tupla con los nombres que acepta `part`.],
)[
```python
# hide-code
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
print(scene.media.svg("assets/robot.svg").parts())
```
]

== Modelos glTF

Modelos 3D en glTF 2.0 (`.gltf` con sus archivos, o `.glb`), con sus
materiales, texturas, esqueletos y animaciones. El modelo se lee al ejecutar el
script y se vuelve contenido 3D normal: un grupo por nodo y una malla
iluminada por primitiva, que dibuja el mismo pase 3D que las primitivas de
#link("/referencia/geometria/")[Geometría]. Los paquetes `.gaanim` guardan lo ya
resuelto, así que el reproductor web y el ligero muestran el modelo sin leer
glTF.

#api-entry(
  name: "MediaLibrary.gltf",
  kind: "factory",
  params: ((name: "path", type: "str", default: none, desc: [Archivo `.gltf` o `.glb` local.]), (name: "scene", type: "str | int | None", default: "None", desc: [Escena del archivo por nombre o índice; por defecto, su escena principal.])),
  desc: [Importa el modelo en las unidades del archivo, con su origen en el origen de la escena: `scale_to` lo lleva a la escala de la escena y `move_to` coloca su origen, que suele estar a los pies. Cada nodo es un `Drawable` en su pose de reposo, alcanzable con `part(nombre)`. Se importan los triángulos, las normales, los colores de vértice, los factores y la textura de color del material metallic-roughness, la emisión, los materiales sin luz (`KHR_materials_unlit`), los modos alfa (opaco, recortado en `alphaCutoff` y mezclado), los materiales de dos caras, los esqueletos de hasta 256 huesos y las animaciones de traslación, rotación y escala de los nodos. Quedan fuera los morph targets, los mapas de normales y de oclusión, las transformaciones de textura, las cámaras y luces del archivo y los contornos de `Material3D.outlined`, que un modelo no dibuja. Las texturas se reducen a 2048 píxeles en su lado mayor. Un archivo que no existe lanza `FileNotFoundError`; una escena desconocida, `ValueError`, y un archivo que no es glTF, `RuntimeError`.],
)[
```python
# show-code: true
from gaanim import Scene
scene = Scene(frame=(16, 9), background="#0f172a")
# El zorro mide unas 80 unidades: scale_to(0.04) lo deja en 3,2
fox = scene.media.gltf("assets/Fox.glb").scale_to(0.04).move_to(0, -2)
fox.rotate_to_3d(0.2, 0.9, 0)
scene.play(fox.animation("Walk", loop=True), duration=2)
# output: preview.webp
scene.render()
```
]

#api-entry(
  name: "Drawable.animations",
  kind: "method",
  signature: "animations() -> dict[str, float]",
  desc: [Las animaciones de un modelo glTF, en el orden del archivo, con su duración en segundos. Vacío para los demás objetos. Recorrerlo da los nombres que acepta `animation`.],
)[
```python
# hide-code
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
print(scene.media.gltf("assets/Fox.glb").animations())
```
]

#api-entry(
  name: "Drawable.animation",
  kind: "method",
  params: ((name: "name", type: "str", default: none, desc: [Nombre de la animación, de `animations()`.]), (name: "duration", type: "float | None", default: "None", desc: [Segundos que dura en la línea de tiempo; por defecto, la duración de `scene.play` o, sin ella, una pasada: de `start_time` al final, o un ciclo con `loop`.]), (name: "speed", type: "float", default: "1.0", desc: [Velocidad de la animación; mayor que cero.]), (name: "loop", type: "bool", default: "False", desc: [Vuelve a empezar al terminar mientras dure.]), (name: "reverse", type: "bool", default: "False", desc: [La reproduce hacia atrás.]), (name: "transition", type: "float", default: "0.0", desc: [Segundos en que funde desde la animación anterior del modelo, que sigue corriendo mientras tanto; la primera funde desde la pose de reposo.]), (name: "start_time", type: "float", default: "0.0", desc: [Segundos dentro de la animación donde empieza.])),
  desc: [Devuelve un `Anim` que reproduce esa animación en `scene.play`. La pose se calcula a partir del tiempo, así que un salto a cualquier instante es exacto y las exportaciones, los paquetes y `--diff` muestran lo mismo que la vista previa. El tiempo de la animación avanza lineal salvo que se le dé `.easing(...)`. Antes de su primera animación el modelo muestra su pose de reposo y, al terminar una, conserva su última pose. La animación fija la traslación, la rotación y la escala de los nodos que mueve; para desplazar, girar o escalar el modelo entero, anima su raíz. Un nombre desconocido lanza `KeyError`; un objeto que no es un modelo glTF, `TypeError`; una velocidad, duración o transición inválidas, `ValueError`.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
fox = scene.media.gltf("assets/Fox.glb").scale_to(0.04).move_to(0, -2)
scene.play(fox.animation("Survey"))
scene.play(fox.animation("Walk", loop=True, transition=0.4), duration=3)
scene.play(fox.animation("Run", loop=True, transition=0.3, speed=1.2), duration=2)
```
]

== Video

Clips MP4 sincronizados con la línea de tiempo, con su audio. Requieren
`ffmpeg` y `ffprobe`.

#api-entry(
  name: "MediaLibrary.video",
  kind: "factory",
  params: ((name: "path", type: "str", default: none, desc: [Archivo MP4 local.]), (name: "width / height / fit / crop / quality", type: "—", default: "—", desc: [Igual que en `image`.]), (name: "offset", type: "float", default: "0.0", desc: [Inicio dentro de la fuente, en segundos.]), (name: "duration", type: "float | None", default: "None", desc: [Duración seleccionada; `None` llega al final.]), (name: "loop", type: "bool", default: "False", desc: [Repite el intervalo seleccionado.]), (name: "speed", type: "float", default: "1.0", desc: [Velocidad positiva; conserva el tono del audio.]), (name: "audio", type: "bool", default: "True", desc: [Reproduce y exporta la primera pista de audio.]), (name: "volume", type: "float", default: "1.0", desc: [Ganancia no negativa.])),
  desc: [Declarar el video no inicia fotogramas ni audio: `scene.play([clip])` fija su inicio absoluto. Un video finito sin bucle aporta su duración al lote; con bucle no alarga la línea de tiempo. Rangos inválidos lanzan `ValueError` y los fallos de medios, `RuntimeError`.],
)[
```python
# show-code: true
from gaanim import Scene
scene = Scene(frame=(16, 9), background="#0f172a")
clip = scene.media.video("clip.mp4", width=9, duration=1.5)
scene.play([clip.animate.fade_in().duration(0.3)])
scene.play([clip])
# output: preview.webp
scene.render()
```
]

#api-entry(
  name: "Video.segment",
  kind: "method",
  params: ((name: "start / end", type: "float", default: none, desc: [Segundos absolutos de la fuente, con `0 <= start < end <= source_duration`.]), (name: "speed / audio / volume", type: "float | bool | None", default: "None", desc: [`None` hereda el valor del video.])),
  desc: [Devuelve un `VideoSegment` inerte hasta pasarlo a `scene.play` o a una composición. Dura `(end - start) / speed` segundos de escena y no hereda `offset`, `duration` ni `loop`. Antes del primer fragmento se ve el póster; entre fragmentos y después del último se mantiene el último fotograma, sin audio. Cada fragmento se programa una sola vez: para repetirlo crea otro. Los solapamientos sobre el mismo video, estirar composiciones con medios y mezclar fragmentos con `scene.play([video])` lanzan `ValueError`.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
video = scene.media.video("clip.mp4").frame(8, 4.5)
scene.play([video.segment(start=2, end=5)])
scene.wait(1)
scene.play([video.segment(start=5, end=8, speed=2)])
```
]

#api-entry(
  name: "Video.frame / crop / quality / point_ref / region",
  kind: "method",
  signature: "frame(width, height, fit=\"contain\") · crop(x, y, width, height, *, normalized=False) · quality(value) -> Self · point_ref(x, y, *, normalized=False) -> PointRef · region(x, y, width, height, *, normalized=False) -> Drawable",
  desc: [Mismo comportamiento que en `Image`; los cambios de fotograma conservan la calidad, y `point_ref` y `region` siguen los píxeles de la fuente en cualquier fotograma.],
  none,
)

#api-entry(
  name: "Video.source_duration / frame_rate / source_width / source_height",
  kind: "property",
  signature: "source_duration: float · frame_rate: float · source_width: int · source_height: int",
  desc: [Duración completa de la fuente en segundos (antes de recortes o cambios de velocidad), fotogramas por segundo y tamaño original en píxeles.],
)[
```python
# hide-code
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
clip = scene.media.video("clip.mp4")
print(clip.source_duration, clip.frame_rate, clip.source_width, clip.source_height)
```
]

== Lottie y dotLottie

Animaciones vectoriales exportadas desde After Effects, Cavalry u otras
herramientas, renderizadas con Velato dentro de la misma escena Vello.

#api-entry(
  name: "MediaLibrary.lottie",
  kind: "factory",
  params: ((name: "path", type: "str", default: none, desc: [Lottie JSON o paquete dotLottie v1/v2 local.]), (name: "animation_id / theme_id / state_machine_id", type: "str | None", default: "None", desc: [Selectores del paquete. Animación y máquina de estados son excluyentes; un JSON no acepta selectores.]), (name: "width / height / fit", type: "—", default: "—", desc: [Igual que en `image`.]), (name: "offset / duration / loop / speed", type: "—", default: "0 / hasta el final / False / 1", desc: [Intervalo reproducido, como en `video`.])),
  desc: [Declarar el clip muestra su primer fotograma; `scene.play([clip])` fija el inicio de la reproducción, que sigue el reloj de la escena también al saltar hacia atrás. `fade_in`, `write` y `create` pueden presentar el primer fotograma antes de reproducirlo. Una máquina de estados no alarga la escena: usa `scene.wait`. Opciones inválidas lanzan `ValueError`; fallos de archivo, JSON o imágenes, `RuntimeError`. Los detalles de compatibilidad están en #link("/referencia/assets/")[Recursos].],
)[
```python
# show-code: true
from gaanim import Scene, sequence
scene = Scene(frame=(16, 9), background="#0f172a")
clip = scene.media.lottie("assets/pulse.json", width=3)
scene.play(sequence(clip.animate.fade_in().duration(0.3), clip))
# output: preview.webp
scene.render()
```
]

#api-entry(
  name: "Lottie.set_input",
  kind: "method",
  desc: [Fija una entrada tipada de la máquina de estados en el cursor (o el estado inicial antes de `play`) y devuelve el clip sin avanzar el tiempo. Entradas desconocidas, tipos incompatibles o números no finitos lanzan `ValueError`; `bool` es distinto de los números.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
clip = scene.media.lottie("assets/button.lottie", state_machine_id="main", width=4)
clip.set_input("active", False)
scene.play([clip])
scene.wait(1)
clip.set_input("active", True)
scene.wait(1)
```
]

#api-entry(
  name: "Lottie.fire_event",
  kind: "method",
  desc: [Dispara un evento declarado en el cursor y devuelve el clip. Requiere que el clip esté activo. Las órdenes simultáneas conservan su orden de declaración y se reproducen igual al buscar y exportar.],
)[
```python
# continue
clip.fire_event("reset")
scene.wait(1)
```
]

#api-entry(
  name: "Lottie.set_theme",
  kind: "method",
  desc: [Aplica un tema estático del paquete en el cursor; `None` restaura el diseño base. Antes de activar el clip fija el tema inicial.],
)[
```python
# continue
clip.set_theme("gold")
scene.wait(1)
```
]

#api-entry(
  name: "Lottie.animation_ids / theme_ids / state_machine_ids",
  kind: "property",
  signature: "animation_ids: list[str] · theme_ids: list[str] · state_machine_ids: list[str]",
  desc: [Identificadores disponibles en el paquete, en el orden del manifiesto; vacíos para un JSON.],
)[
```python
# hide-code
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
package = scene.media.lottie("assets/button.lottie")
print(package.animation_ids, package.theme_ids, package.state_machine_ids)
```
]

#api-entry(
  name: "Lottie.warnings",
  kind: "property",
  desc: [Lista de características de la fuente que Velato omite o aproxima. Revísala si el clip no se ve como en la herramienta de origen. `source_width`, `source_height`, `frame_rate` y `source_duration` describen la composición original.],
  none,
)
