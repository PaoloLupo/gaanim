"""Tu presentación. Cada segmento es una diapositiva; stop() espera a que avances."""

from gaanim import Scene

scene = Scene(theme="presentation")

scene.segment("Portada", notes="Presenta el tema.")
title = scene.text("Mis slides", role="title")
scene.play([title.animate.write()])
scene.stop("portada")

scene.segment("Idea", notes="Una sola idea; revela el detalle al avanzar.")
scene.text("Una idea por diapositiva", role="title").move_to(0, 2.5)
detail = scene.text("Avanza para revelar el detalle")
scene.stop("idea")
scene.play([detail.animate.fade_in()])
scene.stop("detalle")

scene.render()
