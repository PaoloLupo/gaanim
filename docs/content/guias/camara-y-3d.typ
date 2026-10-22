#import "../../components/section.typ": docs-chapter
#import "../../components/tutorial.typ": experimental

#show: docs-chapter.with(
  title: "Cámara y 3D",
  description: "Encuadres, zoom, seguimiento y cámaras secundarias en 2D; primitivas, luces y cámara en perspectiva en 3D",
  route: "/guias/camara-y-3d/",
)

#experimental[
  La cámara 2D es estable. Todo lo 3D de esta página (primitivas, materiales,
  luces, cámara en perspectiva, ejes y superficies 3D) es
  experimental: tiene errores conocidos y puede cambiar entre versiones.
  Úsalo para explorar, no para producción.
]

En esta guía aprenderás a dirigir la mirada del espectador: acercarte a un
detalle, volver al plano general, seguir a un objeto en movimiento, sacudir
la imagen y mostrar un detalle ampliado con una cámara secundaria. Al final
verás cómo montar una escena 3D sencilla y moverte alrededor de ella.

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
mientras dura la animación, y al terminar la cámara se queda donde lo dejó.
`lag` añade un retraso suave, en segundos:

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

== Cámaras secundarias

`scene.camera` mueve toda la imagen. Para enseñar un detalle ampliado sin
perder el plano general, convierte una figura en una pantalla con
`pantalla.camera_view(marco)`: la pantalla muestra lo que ve una segunda
cámara que encuadra `marco`. Es el _picture-in-picture_ de un vídeo o la lupa
sobre un mapa.

```python
# output: preview.webp
from gaanim import CYAN, GOLD, WHITE, Scene

scene = Scene(frame=(16, 9), background="#0f172a")
for i in range(60):
    scene.geometry.dot(0.03).fill(CYAN if i % 2 else GOLD).move_to(-6.5 + 0.1 * i, 0.3 * (i % 5) - 0.6)
scene.text("detalle", role="caption").scale_to(0.2).move_to(-4.5, -1.2)
scene.text("letra pequeña bajo la lupa", role="caption").scale_to(0.3).move_to(-4, 1.8)

marco = scene.geometry.rect(1.6, 0.9).no_fill().stroke(GOLD, 0.03).move_to(-4.5, -0.9)
pantalla = scene.geometry.rounded_rect(6.4, 3.6, 0.2).stroke(GOLD, 0.05).move_to(3.5, 0)
pantalla.camera_view(marco)

lupa = scene.geometry.circle(0.7).stroke(WHITE, 0.04).move_to(-6, 1.8)
foco = scene.geometry.circle(0.25).no_fill().no_stroke().move_to(-6, 1.8)
lupa.camera_view(foco)

scene.play([
    marco.animate.move_to(-2, 0).scale_to(0.5).duration(1.5),
    lupa.animate.move_to(-2, 1.8).duration(1.5),
    foco.animate.move_to(-2, 1.8).duration(1.5),
])
scene.render()
```

- El marco es la cámara: moverlo desplaza la vista, reducirlo acerca y
  girarlo gira la vista en sentido contrario. Se anima con `animate` como
  cualquier objeto.
- El aumento es el tamaño de la pantalla entre el del marco: una pantalla de
  6,4 de ancho con un marco de 1,6 amplía cuatro veces. `fit="cover"` llena la
  pantalla aunque las proporciones no coincidan y `fit="stretch"` deforma el
  marco hasta encajarlo.
- La pantalla dibuja su relleno, encima la vista recortada a su contorno y
  encima su trazo. Cualquier figura cerrada sirve: un círculo hace de lupa.
- La vista no muestra la pantalla ni el marco. Oculta más objetos con
  `exclude=[...]`; si el marco solo debe hacer de cámara, quítale el trazo.
- El fondo de la vista es el de la escena. Cámbialo con `background=` (un
  color o un `Brush`), o pasa `background=None` para que se vea el relleno de
  la pantalla.
- La cámara ve la escena aunque la región quede fuera de la cámara
  principal, así que puedes mostrar a la vez dos zonas alejadas.

=== Zoom, paneo y pop-out

`camera_view` devuelve un `CameraView` con el mismo vocabulario que
`scene.camera`: `pan_to`, `zoom_to` y `rotate_to`, al instante o con
`animate`. Con `zoom=` la vista tiene su propio aumento y el tamaño del marco
deja de contar; sin marco, la cámara mira a `center` a través de un marco
invisible. `animate.zoom_to` cambia ese zoom a velocidad percibida constante,
como el de la cámara principal.

`animate.pop_out()` hace brotar la pantalla de la región que ve su cámara:
empieza encogida sobre esa región, mostrando la escena a tamaño real, y crece
hasta su sitio; si es su entrada, hasta entonces está oculta.
`animate.pop_in()` la devuelve y la oculta al llegar.

```python
# continue
vista = scene.geometry.rounded_rect(5, 2.8, 0.2).stroke(WHITE, 0.05).move_to(-3.5, -2.3)
vista = vista.camera_view(center=(-4.5, -0.9), zoom=3)
scene.play([vista.animate.pop_out().duration(0.8)])
scene.play([vista.animate.pan_to(-1, 0).duration(1.0), vista.animate.zoom_to(6).duration(1.0)])
scene.play([vista.animate.pop_in().duration(0.6)])
```

=== Insets con conectores

`scene.camera.inset(objetivo, zoom=, at=)` monta de una vez la pantalla, un
marco sobre el objetivo que recuadra lo que muestra y dos conectores entre
ambos. `at` coloca la pantalla en una esquina o un borde del fotograma, o en
un punto. Con `follow=True` el marco persigue a un objeto en movimiento, y
`view.zoom` es un valor reactivo que puedes mostrar con un readout.

```python
# output: preview.webp
from gaanim import CYAN, Anchor, Easing, Scene

scene = Scene(frame=(16, 9), background="#0f172a")
for x in range(-7, 8):
    for y in range(-4, 2):
        scene.geometry.dot(0.04).fill("#334155").move_to(x, y)
bola = scene.geometry.circle(0.12).fill(CYAN).move_to(-6, -3)
view = scene.camera.inset(bola, zoom=5, at=Anchor.TOP_RIGHT, follow=True)
scene.viz.readout(view.zoom, format=".0f", prefix="×").move_to(4.5, 0.4)

scene.play([view.animate.pop_out().duration(0.8)])
scene.play([bola.animate.move_to(5, -2).duration(2.0).easing(Easing.SMOOTH)])
scene.play([view.animate.zoom_to(2).duration(1.0)])
scene.render()
```

Con `fixed=True` la pantalla se queda fija en la imagen aunque
`scene.camera` se desplace o acerque, como cualquier objeto con `.hud()`.

=== Capas: la lupa de rayos X

`drawable.view_layer("nombre")` saca un objeto de la cámara principal: solo
lo muestran las vistas que listan esa capa en `layers=`. Así una lupa puede
revelar lo que hay debajo de una superficie.

```python
# output: preview.webp
from gaanim import GOLD, WHITE, Scene

scene = Scene(frame=(16, 9), background="#0f172a")
scene.geometry.rounded_rect(9, 4, 0.6).fill("#475569").no_stroke()
for x in (-3, -1, 1, 3):
    scene.geometry.rect(0.3, 3).fill(GOLD).no_stroke().move_to(x, 0).view_layer("rayos_x")

lupa = scene.geometry.circle(1.3).stroke(WHITE, 0.06).move_to(-4, 0)
vista = lupa.camera_view(center=(-4, 0), zoom=1.2, layers=["rayos_x"])
scene.play([
    lupa.animate.move_to(4, 0).duration(3.0),
    vista.animate.pan_to(4, 0).duration(3.0),
])
scene.render()
```

=== De pantalla completa a un lado

Una demostración que ocupa toda la imagen puede pasar a una pantalla en una
esquina sin detenerse, como en los vídeos de 3Blue1Brown. Pon la
demostración en una capa y haz que la muestre una pantalla tan grande como el
fotograma, que la ve entera; después mueve y escala la pantalla.

```python
# output: preview.webp
import math
from gaanim import GOLD, WHITE, Easing, Scene

scene = Scene(frame=(16, 9), background="#0f172a")
circulo = scene.geometry.circle(3.2).no_fill().stroke(WHITE, 0.03)
brazo = scene.geometry.line(0, 0, 3.2, 0).stroke(GOLD, 0.06)
giro = scene.geometry.group(brazo, origin=(0, 0))
scene.geometry.group(circulo, giro).view_layer("demo")

publico = [
    scene.character(name=nombre, size=2.4).move_to(x, -2.9).hidden()
    for nombre, x in (("ana", -5.5), ("beto", -2.5), ("caro", 0.5))
]
marco = scene.geometry.rect(16, 9).no_fill().no_stroke()
pantalla = scene.geometry.rect(16, 9).no_fill().stroke(WHITE, 0)
pantalla.camera_view(marco, layers=["demo"], exclude=publico)

scene.launch(giro.animate.rotate_by(math.tau * 2).duration(8).easing(Easing.LINEAR))
scene.wait(1)
scene.play([
    pantalla.animate.scale_to(0.5).move_to(-3.6, 2.0),
    pantalla.animate.stroke_width(0.04),
    *(persona.animate.fade_in() for persona in publico),
], duration=1.5)
scene.wait(4)
scene.render()
```

- La capa saca la demostración de la cámara principal; sin ella, al encogerse
  la pantalla quedaría detrás una copia a tamaño completo.
- Mientras la pantalla cubre el fotograma se ve igual que la escena, porque
  su marco tiene el mismo tamaño. El aumento es la pantalla entre el marco:
  al escalarla a la mitad, la vista se encoge con ella y lo que sobresale
  queda recortado.
- `scene.launch` arranca la animación sin mover el cursor, así que sigue
  durante los `play` siguientes. Lo mismo vale para los updaters y para las
  animaciones de la demostración que pongas en esos `play`.
- La vista ve todo el fotograma, también lo que pongas encima, como los
  personajes: `exclude=` los deja fuera. Solo admite objetos que ya existen,
  así que créalos antes, ocultos con `.hidden()`.
- La pantalla empieza sin borde, con un trazo de ancho 0, y
  `animate.stroke_width` lo hace aparecer al encogerse. Para volver a
  pantalla completa, anímala de vuelta, por ejemplo con
  `animate.place(scene.layout.frame, fit="contain")`.
- Escalar un grupo con la demostración, como se hace en Manim, también
  funciona, pero no recorta, y lo que depende de coordenadas del mundo
  (updaters como `orbit`, seguimientos o contenido 3D) no se mueve con el
  grupo.

El ejemplo `camera/picture_in_picture` lleva la pantalla a varias zonas del
layout con `animate.place(zona, fit="contain")` y la devuelve a pantalla
completa.

=== Instantáneas

`vista.snapshot()` congela lo que muestra la vista en el cursor: devuelve una
copia de la pantalla que guarda esa imagen aunque se mueva o se escale,
mientras la vista sigue. La copia empieza oculta y entra con su primera
animación, así que basta con llevarla a su sitio.

```python
# output: preview.webp
from gaanim import CYAN, WHITE, Scene

scene = Scene(frame=(16, 9), background="#0f172a")
for x in range(-7, 0):
    for y in range(-3, 4):
        scene.geometry.dot(0.04).fill("#334155").move_to(x, y)
bola = scene.geometry.circle(0.12).fill(CYAN).move_to(-6, -2)
pantalla = scene.geometry.rect(5, 2.8).stroke(WHITE, 0.05).move_to(3.5, 1.8)
vista = pantalla.camera_view(center=(-6, -2), zoom=3)
vista.follow(bola)
scene.play([bola.animate.move_to(-3, 1).duration(1.5)])
foto = vista.snapshot()
scene.play([
    foto.animate.move_to(3.5, -2.3).duration(1.0),
    foto.animate.scale_to(0.6).duration(1.0),
    bola.animate.move_to(-5, 2).duration(1.5),
])
scene.render()
```

Para congelarla, Gaanim construye la escena otra vez y la lleva al instante
de cada instantánea, así que una escena con instantáneas tarda algo más en
cargar y exportar. Se congela todo lo que mostraba la vista: el contenido 3D,
dibujado con la cámara principal de ese instante y con el detalle que tenía
la vista, los efectos de sombreador (que leen el tiempo de ese instante), los
mates y el vidrio, y lo que mostraban en su propia vista las pantallas que
aparecían en ella.

= Escenas 3D

Gaanim puede mezclar objetos 3D con el resto de la escena. Empieza con una
composición estática: luz, objetos y una cámara que los vea bien. Cuando sea
legible, anima la cámara.

```python
from gaanim import CYAN, GOLD, Material3D, Scene

scene = Scene(frame=(16, 9), background="#0f172a")
scene.geometry.lighting_3d("studio", intensity=1.0)

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

- `scene.geometry` crea primitivas 3D: `cube`, `sphere`, `cylinder`, `cone`,
  `plane` y `torus`. `arrow_3d`, `tube`, `line_3d` y `flat_3d` construyen
  flechas, tubos, líneas con grosor y láminas a partir de puntos, de objetos
  o de un dibujo 2D (ver más abajo).
- `mesh(vertices, triangles, colors=)` crea una malla con los vértices y
  triángulos que le des, y `revolve(profile)` el sólido que barre un perfil
  `(r, y)` al girar alrededor del eje y. Ambos conservan el origen de sus
  coordenadas, así que una pieza modelada desde su pivote gira sobre él.
- `Material3D.matte(color)`, `Material3D.metal(color)` y
  `Material3D.emissive(color, strength)` son puntos de partida; el constructor
  `Material3D(color, roughness=, metallic=)` permite ajustarlos.
- `Material3D.matte(color).outlined(ink)` añade un contorno de ilustración:
  la silueta y los pliegues de una malla cerrada (y los bordes de una
  abierta, como un plano) se dibujan en ese color, del ancho que se pida en
  unidades de la escena.
- `scene.geometry.lighting_3d("studio")` ilumina las primitivas con material
  al estilo de Gaanim: luz del cielo y del suelo, que aclara las caras que
  miran arriba y oscurece las que miran abajo; una luz principal suave desde
  arriba a la izquierda, y un leve brillo en los bordes de la silueta. El
  color del material se mantiene legible: una cara iluminada muestra más o
  menos su color, y una en sombra, la mitad. Los materiales pulidos (poca
  rugosidad) tienen un reflejo blanco y los metálicos reflejan el cielo.
- `scene.geometry.light_3d()` añade una luz puntual (o `"directional"`, como
  el sol) que se coloca con `move_to_3d` y se anima como cualquier objeto.
  Mientras hay luces, sustituyen a la luz principal fija, cada una en su
  color e intensidad; `fade_in` y `fade_out` la encienden y la apagan. Con
  `lighting_3d("none")` solo alumbran ellas.
- Las mallas opacas proyectan sombra de la luz principal, o de las cuatro
  primeras luces de `light_3d`, sobre lo que recibe luz: un cubo sobre un
  plano deja su sombra en el plano. El borde de la sombra es nítido donde el
  objeto toca la superficie y se suaviza a medida que se separan, como bajo
  el sol o una lámpara.
  `lighting_3d(shadows=False)` las quita.
  `"none"` deja solo la emisión del material.
- Las caras planas se pintan de un solo color y las curvas con degradados
  suaves.
- Los drawables tienen versiones 3D de sus transformaciones: `move_to_3d`,
  `shift_by_3d`, `rotate_by_3d(eje, radianes)` y `scale_by_3d`, tanto
  inmediatas como bajo `.animate`.
- Un sólido rígido se arma con `group(..., origin=(x, y, z))`: el grupo gira
  alrededor de ese punto y `move_to_3d` lo coloca, así que una peonza modelada
  desde su punta se inclina sobre ella. Para moverlo con una simulación,
  `drive_from_samples(tiempos, cuaterniones, "orientation")` lo orienta en
  cada instante sin llamar a Python en cada fotograma.

Los ejes son los de la escena 2D: `x` hacia la derecha, `y` hacia arriba y `z`
hacia el espectador.

=== Transformar una malla en otra

`animate.transform_to` también transforma una malla 3D en otra: una esfera
que se infla en un cubo, o que abre sus polos en el agujero de un toro.

```python
# show-code: true
from gaanim import BLUE, GOLD, Material3D, Scene

scene = Scene(frame=(16, 9))
tilt = (0.45, -0.6, 0)
ball = scene.geometry.sphere(1.7, material=Material3D.matte(BLUE)).rotate_to_3d(*tilt)
box = scene.geometry.cube(2.8, material=Material3D.matte(GOLD)).rotate_to_3d(*tilt)
ring = scene.geometry.torus(1.7, 0.65, material=Material3D.matte(BLUE)).rotate_to_3d(0.9, 0, 0)
scene.play([ball.animate.transform_to(box).duration(1.5)])
scene.play([ball.animate.transform_to(ring).duration(1.5)])
# output: preview.webp
scene.render()
```

- `ball.animate.transform_to(box)` lleva la malla de `ball` a la de `box`
  mientras `ball` va a donde está `box`, con su giro, su tamaño y su
  material. `box` es solo una plantilla y sigue oculta;
  `replacement_transform_to` la muestra al final en lugar de `ball`.
- Las dos mallas se muestrean en una misma rejilla de 129 × 97 puntos: por
  sus coordenadas de textura cuando cubren la malla una sola vez (esfera,
  toro, plano, perfil de `revolve`) y, si no, con rayos desde el centro de
  su caja (cubo, cilindro con tapas, `mesh`, texto extruido). Los puntos se
  emparejan como menos se mueven: un jarrón de `revolve` se redondea en una
  esfera a lo largo de su perfil, y la esfera se convierte en el cubo punto
  a punto, a lo largo de cada rayo. Una malla abierta con material, como un
  plano, oculta las caras que dan la espalda a la cámara mientras se curva,
  igual que quieta.
- Antes y después del morph cada malla se dibuja tal cual, así que un cubo
  conserva sus aristas; durante el morph, sus aristas se ven como un bisel
  estrecho. Una segunda transformación parte de la última malla:
  la esfera que se hizo cubo se hace toro desde el cubo.
- Una malla solo se transforma en otra malla 3D con la misma iluminación:
  las dos con material o ninguna. Lo demás lanza `ValueError`.
- Mientras dura el morph, la malla cambia en cada fotograma y se sube de
  nuevo a la GPU; el paquete `.gaanim` guarda las dos rejillas una vez y, en
  cada fotograma, solo cuánto avanza entre ellas. Antes y después no cuesta
  nada.

== Modelos glTF

`scene.media.gltf("modelo.glb")` importa un modelo hecho en Blender u otra
herramienta: un grupo por nodo, en su pose de reposo, con una malla iluminada
por primitiva, su material y su textura de color. El modelo llega en las
unidades del archivo y con su origen en el de la escena; escálalo con
`scale_to` y coloca su origen con `move_to`. `part(nombre)` alcanza un nodo,
que se anima como cualquier objeto.

`animations()` lista las animaciones del archivo con su duración, y
`animation(nombre, ...)` reproduce una en `scene.play`: una vez, en bucle
(`loop=True`), más lenta o más rápida (`speed`), al revés (`reverse`) o desde
un instante (`start_time`). Con `transition=0.4`, la animación funde en 0,4 s
desde la anterior, que sigue corriendo mientras tanto, como al pasar de andar
a correr. Con `scene.launch`, una animación sigue en marcha mientras se
programan otras: el modelo camina mientras se desplaza, y su siguiente
animación toma el relevo, como en los mezcladores de animación de los motores
de juegos.

```python
from gaanim import Scene

scene = Scene(frame=(16, 9), background="#0f172a")
wolf = scene.media.gltf("assets/Wolf.glb").scale_to(1.7).move_to(0, -2)
wolf.rotate_to_3d(0.2, 0.9, 0)

scene.play(wolf.animation("Idle_Eating"))
scene.play(wolf.animation("Walk", loop=True, transition=0.4), duration=3)
scene.play(wolf.animation("Run", loop=True, transition=0.3), duration=2)
scene.render()
```

Los esqueletos se deforman en la GPU: la malla se sube una vez y en cada
fotograma solo cambian las matrices de sus huesos, así que un personaje
animado cuesta lo mismo que una malla quieta y un paquete `.gaanim` guarda
unas decenas de matrices por fotograma. Los morph targets, las shape keys de
Blender, igual: sus desplazamientos se suben una vez y en cada fotograma solo
cambian sus pesos, que mueven las animaciones del archivo,
`morph("Squash", 0.4)` y `animate.morph("Squash", 1.0)` (ver el ejemplo
`3d/morph_targets_3d`). La pose sale del tiempo, de modo que
los saltos de la línea de tiempo y las capturas de `--diff` son exactos. Las
escenas sin modelos no cargan nada de esto: sus mallas se dibujan como
siempre. El detalle de lo que se importa está en
#link("/referencia/medios/#modelos-gltf")[Medios].

== Vectores, recorridos y texto en el espacio

Algunas formas se definen mejor con puntos del mundo que con medidas:

- `arrow_3d(start, end)` dibuja un vector como una flecha sólida, con el
  astil y la punta proporcionales a su longitud (`shaft_radius`,
  `head_radius` y `head_length` los fijan).
- `tube(points, radius)` sigue una polilínea con un tubo iluminado, como una
  trayectoria o un muelle. La sección avanza sin retorcerse; `closed=True`
  cierra el lazo y `caps=False` deja los extremos abiertos.
- `line_3d(start, end, thickness)` une dos extremos con una línea sólida
  que conserva su grosor. Si un extremo es un objeto que se mueve, la línea
  lo sigue en cada fotograma, como el enlace entre dos átomos;
  `create()` la hace crecer desde `start`.
- `flat_3d(source)` convierte el relleno de un texto, una ecuación o una
  forma en una lámina plana de dos caras. A diferencia de `billboard`, que
  mantiene un objeto 2D de frente a la cámara, la lámina es parte de la
  escena: se coloca con `move_to_3d` y `rotate_to_3d`, recibe luz y gira con
  la cámara.

Las flechas y los tubos quedan en el centro de sus puntos, así que
`move_to_3d` y los giros parten de ahí; una `line_3d` la colocan sus
extremos, así que se mueve moviéndolos. Todas son mallas cerradas que
proyectan sombra.

```python
import math
from gaanim import GOLD, RED, WHITE, Material3D, Scene

scene = Scene(frame=(16, 9), background="#0f172a")
floor = scene.geometry.plane(12, 8).move_to_3d(0, -1, 0)
path = [(2 * math.cos(t / 10), t / 40 - 0.9, 2 * math.sin(t / 10)) for t in range(64)]
trail = scene.geometry.tube(path, 0.06, material=Material3D.metal(GOLD))
force = scene.geometry.arrow_3d((0, -1, 0), (0, 1.5, 0), material=Material3D.matte(RED))
label = scene.geometry.flat_3d(scene.text("F", size=0.8), material=Material3D.matte(WHITE))
label.move_to_3d(0.6, 1.2, 0)

scene.camera.perspective(fov_y=0.7)
scene.camera.look_at(eye=(0, 3, 9), target=(0, 0, 0))
scene.play([trail.animate.create(), force.animate.create(), label.animate.fade_in()])
scene.play([scene.camera.animate.orbit(delta_yaw=1.2, delta_pitch=0.0).duration(2)])
scene.render()
```

== Nubes de puntos y copias de una malla

Para miles de puntos, cubos o flechas iguales no hacen falta miles de
objetos: `points_3d` e `instances` crean un solo objeto que dibuja la misma
malla en muchos sitios. La malla se sube una vez a la GPU y todas las copias
se dibujan juntas, así que una nube de diez mil puntos cuesta casi lo mismo
que una esfera.

```python
# show-code: true
import math
from gaanim import BLUE, GOLD, Material3D, Scene

scene = Scene(frame=(16, 9))
n = 1500
turn = math.pi * (3 - math.sqrt(5))
ball = []
for k in range(n):
    y = 1 - 2 * (k + 0.5) / n
    r = math.sqrt(1 - y * y)
    ball.append((2.2 * r * math.cos(k * turn), 2.2 * y, 2.2 * r * math.sin(k * turn)))
disk = [(2.8 * math.sqrt(k / n) * math.cos(k * turn), 0, 2.8 * math.sqrt(k / n) * math.sin(k * turn)) for k in range(n)]
cloud = scene.geometry.points_3d(ball, radius=0.04, colors=[GOLD if y > 0 else BLUE for _, y, _ in ball])
cloud.move_to_3d(-3.5, 0, 0).rotate_to_3d(0.3, 0, 0)
target = scene.geometry.points_3d(disk, radius=0.04).move_to_3d(-3.5, 0, 0).rotate_to_3d(0.5, 0, 0)

cube = scene.geometry.cube(0.4, material=Material3D.matte(BLUE))
cells = [(x / 2, y / 2, z / 2) for x in range(-3, 4) for y in range(-3, 4) for z in range(-3, 4)]
cells.sort(key=lambda p: p[0] ** 2 + p[1] ** 2 + p[2] ** 2)
voxels = scene.geometry.instances(cube, cells, scales=[0.4 + 0.6 * (y + 1.5) / 3 for _, y, _ in cells])
voxels.move_to_3d(3.5, 0, 0).rotate_to_3d(0.5, -0.6, 0)

scene.play([cloud.animate.create(), voxels.animate.create()], duration=2)
scene.play([cloud.animate.transform_to(target), voxels.animate.rotate_by_3d("y", 1.2)], duration=2)
# output: preview.webp
scene.render()
```

- `points_3d(positions, radius)` pone una esfera iluminada en cada punto;
  `colors` da a cada una su color en lugar del del material.
- `instances(mesh, positions)` copia la malla de una primitiva (un cubo, una
  flecha, un toro, un `revolve`) con su material; `scales`, `rotations` y
  `colors` dan a cada copia su escala, su giro y su color. La primitiva
  queda oculta como plantilla, salvo con `keep_mesh=True`.
- Las posiciones están en las coordenadas del objeto: `move_to_3d`,
  `rotate_to_3d` y `scale` mueven, giran y escalan el conjunto, y la
  opacidad, el material y el glow son de todas las copias.
- `create()` hace crecer las copias una tras otra, en el orden de la lista,
  y `uncreate()` las encoge en el mismo orden. Ordenar las posiciones elige
  cómo aparecen: arriba, los vóxeles crecen desde el centro.
- `animate.transform_to(otra)` lleva cada copia a la que ocupa su lugar en
  la otra lista, con su escala, su giro y su color, mientras el objeto va a
  donde está `otra`. Las dos necesitan tantas copias; si copian mallas
  distintas, la malla también se transforma.
- Las copias proyectan y reciben sombra como cualquier malla. Un objeto de
  copias translúcido se ordena con los demás como un todo, no copia a copia.
- Un paquete `.gaanim` guarda cada conjunto de copias una vez y en cada
  fotograma solo cuánto han crecido; durante una transformación guarda cada
  fotograma como su cambio respecto al anterior.

== La cámara en perspectiva

- `perspective(fov_y)` activa la proyección en perspectiva; `fov_y` es el
  campo de visión vertical en radianes.
- `look_at(eye=, target=, up=)` coloca la cámara en `eye` mirando a `target`.
- `animate.orbit(delta_yaw=, delta_pitch=)` gira alrededor del objetivo.
  Con `dolly=` además se acerca (menor que `1`) o se aleja mientras gira:
  `orbit` y `dolly` no se pueden reproducir a la vez, porque mueven la misma
  pose de la cámara.
- `animate.dolly(factor)` se acerca (menor que `1`) o se aleja (mayor que
  `1`).
- `orthographic()` vuelve a la proyección ortográfica y `reset()` al encuadre
  2D inicial.

=== Seguir un objeto en 3D

`animate.follow(objeto)` también funciona con una cámara en perspectiva o en
ángulo: la cámara se mueve con el objeto sin girar, a la misma distancia
sobre su línea de visión, y lo mantiene en el centro aunque suba, baje o se
aleje. Su objetivo se mueve con ella, así que un `orbit` posterior gira
alrededor del punto seguido. `offset` admite una profundidad, `(x, y, z)`.
A diferencia de `bind_3d(target=)`, que gira la cámara desde un ojo fijo,
aquí el ojo viaja.

```python
# output: preview.webp
from gaanim import GOLD, Material3D, Scene

scene = Scene(frame=(16, 9), background="#0f172a")
scene.camera.look_at(eye=(7, 5, 10), target=(0, -1, 0))
scene.camera.perspective(0.75)
scene.geometry.plane(11, 7, material=Material3D.matte("#334155")).move_to_3d(0, -1, 0)
bola = scene.geometry.sphere(0.8, material=Material3D.matte(GOLD)).move_to_3d(-3, 0, 2)
scene.play([
    bola.animate.move_to_3d(3, 1.5, -2).duration(3.0),
    scene.camera.animate.follow(bola, lag=0.3).duration(3.0),
])
scene.render()
```

Con `follow(bola, turn=True)` la cámara se queda donde está y gira para
mantener la esfera en el centro, como sobre un trípode, y termina mirándola.
`animate.look_at(None, bola)` también gira la cámara sin moverla, pero como
una transición: llega a mirar la esfera al final de la animación, aunque se
haya movido, y la siguiente animación de cámara parte de ahí.

```python
# continue
scene.play([
    bola.animate.move_to_3d(-3, 0.5, -1).duration(3.0),
    scene.camera.animate.follow(bola, turn=True).duration(3.0),
])
```

Las cámaras secundarias siguen al objeto donde la cámara principal lo
dibuja: con `scene.camera.inset(bola, follow=True)` o `vista.follow(bola)`,
el marco se queda sobre la esfera y la pantalla la muestra ampliada.

=== Vistas con cámara 3D propia

Una vista de cámara amplía lo que dibuja la cámara principal, desde su mismo
ángulo. `pantalla.camera_view_3d(eye, target)` le da en cambio una cámara 3D
propia: la pantalla muestra el contenido 3D de la escena desde `eye`,
mirando a `target`, como lo mostraría la cámara principal desde allí. El ojo
y el objetivo pueden ser objetos: se leen donde están en cada fotograma, así
que una vista cenital puede seguir a la esfera mientras rueda.

```python
# continue
arriba = scene.geometry.rounded_rect(4.8, 2.7, 0.15).hud().move_to(5, 2.6)
vista = arriba.camera_view_3d(eye=(0, 12, 0.5), target=bola, fov_y=0.6)
scene.play([bola.animate.move_to_3d(2, 0, 2).duration(3.0)])
```

La vista muestra primitivas, superficies, líneas 3D, modelos y objetos
colocados en 3D; los objetos 2D del canvas quedan fuera. `vista.snapshot()`
la congela como a cualquier otra.

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

Como en las bibliotecas de gráficos, los ejes siguen a la cámara: cada plano
de la rejilla va a la cara de la caja que queda detrás de la superficie, y
cada eje, con sus números, a una arista del contorno de la caja. Al orbitar,
cambian de lado cuando la cámara cruza el plano que pasa por el centro de la
caja. Con `sides="fixed"` se quedan en la esquina más baja de la caja.

== Campos escalares: isosuperficies y volúmenes

Un campo escalar da un valor a cada punto del espacio: una densidad, una
temperatura, un potencial. `scene.geometry.isosurface(campo, nivel, ...)`
dibuja la superficie donde el campo vale `nivel`, y
`scene.geometry.volume(campo, ...)` el campo entero, como una nube que
brilla y absorbe la luz. Los dos muestrean `campo(x, y, z)` en una rejilla
de `resolution` puntos por lado sobre la caja de `x_range`, `y_range` y
`z_range` (con `vectorized=True`, una sola llamada con arrays de numpy), o
toman un array 3D de valores indexado `[x][y][z]`:

```python
import math
from gaanim import Material3D, Scene

scene = Scene(frame=(16, 9))
blob = lambda x, y, z: x * x + 0.5 * y * y + z * z
shell = scene.geometry.isosurface(blob, 1.0, x_range=(-1.5, 1.5), y_range=(-2, 2), z_range=(-1.5, 1.5), material=Material3D.matte("#60a5fa"))
cloud = scene.geometry.volume(lambda x, y, z: math.exp(-3 * blob(x, y, z)), color_map="magma", density=6.0).move_to(4, 0)
scene.play([shell.animate.fade_in(), cloud.animate.fade_in()])
scene.render()
```

- La isosuperficie sale suave a cualquier resolución (_surface nets_: un
  vértice por celda que cruza, y normales del gradiente del campo) y mira
  hacia fuera: hacia el lado del nivel donde quedan la mayoría de las
  paredes de la caja. Donde la caja la corta queda abierta, y se ilumina
  por las dos caras. Es una `Primitive3D`: su material se anima, y
  `animate.transform_to` la transforma en otra isosuperficie, por ejemplo la
  del mismo campo a otro nivel.
- En el volumen, cada valor, situado en `value_range` (por defecto del menor
  al mayor), brilla con el color que le da `color_map` y es tan opaco como
  dice `opacity`, cuyos valores se reparten por el rango: `(0, 1)` oculta
  los valores bajos y `(0, 0, 1)` todo menos la mitad alta. Un valor
  opaco absorbe `density` de la luz por unidad de escena.
- Cada píxel recorre el volumen a lo largo de su rayo desde el ojo hasta lo
  opaco, así que un sólido dentro de la nube se ve a través de la parte que
  queda delante. El volumen se dibuja después de lo opaco y antes de lo
  translúcido: una superficie translúcida delante de la nube se mezcla
  encima, y una que está detrás se ve delante de ella.
- Varias isosuperficies del mismo campo, una dentro de otra y translúcidas,
  se ven en su orden con `transparency_3d("order_independent")`.

Ver el ejemplo `3d/fields_3d`.

== Cómo se dibuja el 3D

Gaanim rasteriza el 3D en la GPU. Cada malla y cada línea se sube una vez,
cuando cambia, y en cada fotograma se dibuja con un búfer de profundidad y
antialiasing MSAA 4x. El resultado es una imagen que Vello compone debajo del
contenido 2D. Por eso el coste de un fotograma depende de cuántos objetos 3D
hay, no de cuántos triángulos tienen: superficies densas y curvas de miles de
puntos se reproducen a la frecuencia de la pantalla.

- Las primitivas con `Material3D` reciben luz, calculada en cada píxel, y
  ocultan sus caras traseras, salvo las de un material glTF de dos caras.
  Las mallas de un modelo glTF multiplican su color por su textura, recortan
  o mezclan su alfa como dice su material y se deforman con los huesos de su
  esqueleto en el sombreador de vértices. Las mallas sin material (superficies, gráficos
  3D) no reciben luz y se ven por las dos caras; las líneas 3D tampoco
  reciben luz y tienen extremos redondeados. `surface` y
  `parametric_surface` aceptan `material=` y entonces se iluminan por las
  dos caras, con sus colores, y proyectan y reciben sombras.
- Una línea 3D mide en pantalla el ancho de su trazo, `.stroke(color, ancho)`
  en unidades de la escena, a cualquier profundidad; sin trazo, 2 px de una
  salida de 1080 px de alto, la misma parte del encuadre a cualquier tamaño.
  En una línea con trazo, `animate.stroke_width(...)` anima el ancho.
- Las superficies y las líneas que se cruzan se cortan donde corresponde, y
  la geometría que atraviesa el plano cercano de la cámara se recorta.
- `glow(...)` y `animate.glow(...)` dan un halo a mallas, superficies, líneas,
  modelos glTF y objetos con `.in_3d()`. El pase 3D dibuja la silueta visible
  del objeto con el color del halo, la difumina como el glow 2D y la pone a
  su alrededor, debajo del objeto; las partes que otro objeto tapa no
  brillan. `shadow(...)` hace lo mismo con la silueta desplazada, como una
  sombra paralela 2D. `blur(...)` dibuja el objeto aparte, desenfoca su
  color y lo pone sobre lo que tiene detrás; lo que tiene delante tapa la
  sombra y el desenfoque. Solo los fotogramas con alguno de estos efectos
  construyen sus pipelines.
- Un objeto translúcido (con opacidad, o con colores con transparencia) se
  ve como una capa: su superficie más cercana se mezcla una vez sobre lo que
  tiene detrás. Los objetos translúcidos se dibujan de atrás hacia delante,
  ordenados por su centro, así que dos que se cruzan se mezclan en el mismo
  orden a los dos lados del cruce.
- Con `scene.camera.depth_of_field(...)`, la imagen 3D se desenfoca al
  final, con brillos y sombras, según la profundidad de cada píxel: primero a
  lo ancho y luego a lo alto, y cada píxel se extiende sobre sus vecinos con
  su propio desenfoque. Lo que queda detrás no se extiende sobre lo que está
  delante más de lo que este se desenfoca, así que un borde nítido delante
  de un fondo desenfocado conserva su borde. Solo los fotogramas
  desenfocados construyen sus pipelines. Ver el ejemplo
  `3d/depth_of_field_3d`.
- Con `scene.geometry.transparency_3d("order_independent")`, cada
  superficie translúcida se mezcla en el orden en que está en cada píxel
  (_depth peeling_): el pase 3D separa, de la más cercana a la más lejana,
  hasta `layers` superficies por píxel (4 por defecto) y las mezcla en ese
  orden. Los objetos que se cruzan, los anidados y los pliegues de una misma
  superficie se ven en su orden, y una malla de dos caras deja ver su lado
  lejano. Cada capa dibuja otra vez el contenido translúcido; solo los
  fotogramas con algo translúcido construyen sus pipelines. Ver el ejemplo
  `3d/transparency_3d`.
- Una línea sobre una superficie en el mismo plano, como la rejilla del
  suelo bajo un gráfico, queda debajo de la superficie.
- Las sombras salen de mapas de sombra: uno por luz direccional, uno en
  perspectiva por foco (`light_3d("spot")`, un poco más ancho que su cono) y
  seis (un cubo) por luz puntual, que cubren todas las mallas. Proyectan sombra
  las mallas opacas y los objetos 2D con `.in_3d()`, donde son al menos
  medio opacos; no las mallas translúcidas, las líneas ni lo que brilla
  (`Material3D.emissive`), que puede contener la luz que lo hace brillar.
  Un sólido no se da sombra en la cara que aparta de la luz, y tampoco una
  lámina (una superficie con material, o una malla abierta): ahí solo la
  ilumina la luz que rodea el borde, y la sombra empieza más allá.
- Todo el contenido 3D se dibuja debajo del 2D: textos y HUD quedan
  siempre encima. Las excepciones viven en el mundo 3D y las dibuja el pase
  3D, tapadas píxel a píxel por las mallas y las líneas que tienen delante:
  un objeto 2D con `.in_3d()`, inclinado por la perspectiva, y las etiquetas
  con `.billboard()` y los números y títulos de los ejes 3D, de frente a la
  cámara y del tamaño de su texto. Una etiqueta se acerca a la cámara la
  mitad de su lado mayor, así que una superficie en la que se apoya no la
  corta. Un número o un título que se ve a través de la caja de sus ejes
  queda justo delante de las caras del fondo: su rejilla no lo cruza, pero
  el gráfico de la caja y lo que está delante de ella lo tapan. Las imágenes
  y las escenas Lottie de estos objetos también las dibuja el pase 3D: una
  imagen con sus píxeles y una escena Lottie con los contornos que rellena y
  traza. Solo un objeto con tiza se dibuja encima del 3D y, si es una
  etiqueta, se desvanece entera mientras una malla opaca queda entre su
  punto y la cámara.

El 3D aparece en la vista previa, las exportaciones, las capturas, los
paquetes `.gaanim` (también en el reproductor web) y las vistas previas de
Presenter View, y admite desenfoque de movimiento y postprocesado.

= Consejos

- Valida primero una escena quieta; añade el movimiento de cámara al final.
- Los objetos 3D usan las mismas unidades lógicas que el resto de la escena;
  mide tamaños y distancias de cámara con la misma escala.

Las firmas completas de la cámara están en la
#link("/referencia/scene/")[referencia de la escena].
