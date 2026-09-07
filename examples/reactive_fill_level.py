"""One parameter drives a clipped fill and its numeric readout."""
import os

from gaanim import Scene, computed

scene = Scene(frame=(16, 9))
scene.canvas.set_theme("paper")
amount = scene.viz.parameter(0)
outline = scene.geometry.rect(4, 4).no_fill().stroke("#626878", 0.025)
fraction = computed(lambda value: value / 100, inputs=[amount])
fill = scene.geometry.fill_level(outline, "#e26d5c", fraction, keep_outline=False)
number = scene.viz.readout(amount, format=".0f", suffix="%", font_size=0.7)
scene.play([outline.animate.fade_in(), number.animate.fade_in()], duration=0.2)
scene.play(amount.animate.set(75), duration=1.0)
scene.wait(0.3)
fill.set_fill_level(0.25)
scene.wait(0.5)
fill.set_fill_level(fraction)
scene.wait(0.5)
if path := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(path, [0.7, 1.2, 1.7, 2.2, 0.7])
scene.render()
