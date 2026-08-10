"""The same tree adapts to a vertical viewport."""
from gaanim import BLUE, GOLD, Scene, vertical_short

scene = Scene(720, 1280, background="#0f172a", margin=48)
page = scene.template(vertical_short, title=scene.title("Vertical").fill(GOLD), body=scene.circle(180).fill(BLUE), caption=scene.text("9:16 · no at()"))
scene.play([page.fade_in().duration(0.5)])

