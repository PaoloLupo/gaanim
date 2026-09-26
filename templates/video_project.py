"""Tu escena. Guarda el archivo y la vista previa se actualiza sola."""

from gaanim import Scene

scene = Scene()

title = scene.text("Hola, Gaanim", role="title").move_to(0, 2)
circle = scene.geometry.circle(1.5).move_to(0, -1)

scene.play([title.animate.write(), circle.animate.create()])
scene.wait(1)

scene.render()
