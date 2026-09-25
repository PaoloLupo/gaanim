#import "../../components/section.typ": docs-chapter
#import "../../components/tutorial.typ": experimental

#show: docs-chapter.with(
  title: "Cámara y 3D",
  description: "Encuadres, zoom y seguimiento con la cámara 2D; primitivas, luces y cámara en perspectiva en 3D",
  route: "/guias/camara-y-3d/",
)

#experimental[
  La cámara 2D es estable. Todo lo 3D de esta página (primitivas, materiales,
  luces, cámara en perspectiva, ejes y superficies 3D y modelos glTF) es
  experimental: tiene errores conocidos y puede cambiar entre versiones.
  Úsalo para explorar, no para producción.
]

En esta guía aprenderás a dirigir la mirada del espectador: acercarte a un
detalle, volver al plano general, seguir a un objeto en movimiento y sacudir
la imagen. Al final verás cómo montar una escena 3D sencilla y moverte
alrededor de ella.

```python
# output: preview.webp
from gaanim import CYAN, GREEN, WHITE, Easing, Scene

scene = Scene(frame=(16, 9), background="#0f172a")
left = scene.geometry.circle(0.8).fill(CYAN).move_to(-4, 0)
right = scene.geometry.square(1.4).fill(GREEN).move_to(4, 0)
label = scene.text("Detalle", role="caption").fill(WHITE).move_to(4, -1.2)

scene.play([
    left.animate.grow_from_center().duration(0.4),
    right.animate.grow_from_center().duration(0.4),
    label.animate.fade_in().duration(0.4),
])
scene.camera.save("general")
scene.play([scene.camera.animate.frame_to([right, label], margin=0.5).duration(1.0).easing(Easing.SMOOTH)])
scene.play([scene.camera.animate.restore("general").duration(0.8)])
scene.play([scene.camera.animate.shake(trauma=0.6, seed=1)])
scene.render()
```

= La cámara 2D

`scene.camera` controla qué parte de la escena se ve. Trabaja en las mismas
unidades lógicas que los objetos (un fotograma de 16 × 9), así que un zoom de
`2` muestra la mitad de ancho y de alto.

Cada operación existe en dos formas:

- `scene.camera.zoom_to(2)` aplica el cambio al instante, en el cursor actual.
- `scene.camera.animate.zoom_to(2)` devuelve una animación que pasas a
  `scene.play([...])` con su duración y su easing.

== Mover, acercar y girar

```python
>>>from gaanim import CYAN, Scene
>>>scene = Scene(frame=(16, 9), background="#0f172a")
>>>target = scene.geometry.circle(0.5).fill(CYAN).move_to(3, 1)
scene.play([scene.camera.animate.pan_to(3, 1).duration(0.8)])
scene.play([scene.camera.animate.zoom_to(2.5).duration(1.0)])
scene.play([scene.camera.animate.rotate_to(0.2).duration(0.6)])
scene.play([scene.camera.animate.reset().duration(0.8)])
```

- `pan_to(x, y)` centra la vista en un punto; también acepta un drawable.
- `zoom_to(z)` acerca con valores mayores que `1`. Por defecto interpola de
  forma exponencial para que un zoom grande se perciba a velocidad constante;
  `interpolation="linear"` recupera la interpolación lineal.
- `rotate_to(ángulo)` gira la vista, en radianes.
- `reset()` vuelve al encuadre inicial.

== Encuadrar objetos

`frame_to(objetos, margin=)` calcula el paneo y el zoom necesarios para que
los objetos quepan en pantalla con un margen. Es la forma más cómoda de
acercarse a un detalle sin calcular coordenadas:

```python
# continue
scene.play([scene.camera.animate.frame_to([target], margin=0.6).duration(1.0)])
```

Con `dynamic=True` el encuadre se recalcula mientras los objetos se mueven.

== Guardar y recuperar encuadres

`scene.camera.save("nombre")` guarda el encuadre actual y
`scene.camera.animate.restore("nombre")` vuelve a él. Para definir un encuadre
sin aplicarlo, usa `scene.camera.state_2d(...)` con `center`, `zoom` y `rotation`, y
anímalo con `scene.camera.animate.to(estado)`:

```python
# continue
overview = scene.camera.state_2d(center=(0, 0), zoom=1.0)
detail = scene.camera.state_2d(center=(3, 1), zoom=3.0)
scene.play([scene.camera.animate.to(detail).duration(1.0)])
scene.play([scene.camera.animate.to(overview).duration(1.0)])
```

== Seguir a un objeto

`scene.camera.animate.follow(objeto, lag=)` mantiene el objeto en el centro
mientras dura la animación. `lag` añade un retraso suave, en segundos:

```python
# output: preview.webp
from gaanim import GOLD, WHITE, Easing, Scene

scene = Scene(frame=(16, 9), background="#0f172a")
for x in range(-7, 8):
    for y in range(-4, 5):
        scene.geometry.dot(0.05).fill(WHITE).move_to(x, y)
car = scene.geometry.rect(0.8, 0.4).fill(GOLD).move_to(-6, -2)

scene.play([
    car.animate.move_to(6, 2).duration(2.0).easing(Easing.SMOOTH),
    scene.camera.animate.follow(car, lag=0.2).duration(2.0),
])
scene.render()
```

Para un vínculo que dure varias animaciones, `scene.camera.bind_2d(center=,
zoom=, rotation=)` conecta la cámara a extremos o parámetros y devuelve un
`CameraConstraint` que puedes desactivar con `disable()`:

```python
# continue
zoom = scene.viz.parameter(1.0)
constraint = scene.camera.bind_2d(center=car, zoom=zoom)
scene.play([zoom.animate.set(2.0).duration(0.8)])
constraint.disable()
```

Consulta #link("/guias/reactividad/")[Reactividad] para crear esos parámetros
y extremos.

== Sacudir la imagen

`shake()` produce un temblor determinista que termina en reposo. `trauma`
(entre `0` y `1`) controla la intensidad, `decay` cuánto tarda en apagarse y
`seed` fija el patrón:

```python
# continue
scene.play([scene.camera.animate.shake(trauma=0.8, decay=1.5, seed=3)])
```

= Escenas 3D

Gaanim puede mezclar objetos 3D con el resto de la escena. Empieza con una
composición estática: luz, objetos y una cámara que los vea bien. Cuando sea
legible, anima la cámara.

```python
from gaanim import CYAN, GOLD, Material3D, Scene

scene = Scene(frame=(16, 9), background="#0f172a")
scene.geometry.lighting_3d("studio", intensity=1.0, shadows=True)

cube = scene.geometry.cube(2.0, material=Material3D.matte(CYAN)).move_to_3d(-1.8, 0, 0)
sphere = scene.geometry.sphere(1.1, material=Material3D.metal(GOLD)).move_to_3d(1.8, 0, 0)

scene.camera.perspective(fov_y=0.7)
scene.camera.look_at(eye=(0, 3, 10), target=(0, 0, 0))

scene.play([cube.animate.create().duration(0.5), sphere.animate.create().duration(0.5)])
scene.play([scene.camera.animate.orbit(delta_yaw=0.8, delta_pitch=0.3).duration(1.2)])
scene.play([scene.camera.animate.dolly(0.8).duration(0.6)])
scene.render()
```

== Objetos, materiales y luz

- `scene.geometry` crea primitivas 3D: `cube`, `sphere`, `cylinder`, `cone` y
  `plane`.
- `Material3D.matte(color)`, `Material3D.metal(color)` y
  `Material3D.emissive(color, strength)` son puntos de partida; el constructor
  `Material3D(color, roughness=, metallic=)` permite ajustarlos.
- `scene.geometry.lighting_3d("studio")` añade una iluminación de estudio con
  sombras opcionales.
- Los drawables tienen versiones 3D de sus transformaciones: `move_to_3d`,
  `shift_by_3d`, `rotate_by_3d(eje, radianes)` y `scale_by_3d`, tanto
  inmediatas como bajo `.animate`.

Los ejes son los de la escena 2D: `x` hacia la derecha, `y` hacia arriba y `z`
hacia el espectador.

== La cámara en perspectiva

- `perspective(fov_y)` activa la proyección en perspectiva; `fov_y` es el
  campo de visión vertical en radianes.
- `look_at(eye=, target=, up=)` coloca la cámara en `eye` mirando a `target`.
- `animate.orbit(delta_yaw=, delta_pitch=)` gira alrededor del objetivo.
- `animate.dolly(factor)` se acerca (menor que `1`) o se aleja (mayor que
  `1`).
- `orthographic()` vuelve a la proyección ortográfica y `reset()` al encuadre
  2D inicial.

== Ejes y superficies 3D

`scene.viz.cartesian_3d` crea unos ejes 3D sobre los que puedes dibujar
superficies, curvas paramétricas y campos. Una superficie puede depender de
parámetros, como cualquier objeto reactivo:

```python
import math
from gaanim import Axis, Scene

scene = Scene(frame=(16, 9), background="#0f172a")
space = scene.viz.cartesian_3d(
    Axis.linear(-3, 3).ticks(1),
    Axis.linear(-3, 3).ticks(1),
    Axis.linear(-1, 1).ticks(0.5),
    size=(6, 6, 3),
)
phase = scene.viz.parameter(0.0)
surface = space.surface(lambda x, y, p: math.sin(math.hypot(x, y) - p), inputs=[phase])

scene.camera.perspective(fov_y=0.8)
scene.camera.look_at(eye=(7, -9, 6), target=(0, 0, 0), up=(0, 0, 1))
scene.play([space.animate.create().duration(0.6), surface.animate.fade_in().duration(0.6)])
scene.play([phase.animate.set(2 * math.pi).duration(2.0)])
scene.render()
```

La coordenada `z` de los datos apunta hacia el espectador, así que para ver la
superficie como un relieve se usa `up=(0, 0, 1)` en `look_at`.

== Modelos glTF

`scene.media.gltf("modelo.glb")` importa un modelo glTF 2.0 como un drawable
más. Consulta #link("/referencia/assets/")[Recursos] para las rutas y las
escenas internas del archivo.

= Consejos

- Valida primero una escena quieta; añade el movimiento de cámara al final.
- Los objetos 3D usan las mismas unidades lógicas que el resto de la escena;
  mide tamaños y distancias de cámara con la misma escala.
- En Presenter View, las vistas previas del orador solo dibujan las capas 2D;
  la pantalla de la audiencia sí muestra los objetos 3D.

Las firmas completas de la cámara están en la
#link("/referencia/scene/")[referencia de la escena].
