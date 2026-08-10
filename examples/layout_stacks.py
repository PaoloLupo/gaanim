"""Rows and columns replace group vstack/hstack helpers."""
from gaanim import BLUE, GOLD, WHITE, Scene

scene = Scene(960, 540, background="#0f172a", margin=40)
steps = scene.column([scene.title("Pipeline").fill(GOLD), scene.text("Measure").fill(WHITE), scene.text("Solve").fill(WHITE), scene.text("Place").fill(BLUE)], gap=22, align="start")
page = scene.stack([steps], within="safe", width="fill", height="fill", align="center")
scene.play([page.fade_in().duration(0.5)])

