"""Responsive wrapped row."""
from gaanim import BLUE, GOLD, Scene

scene = Scene(frame=(16, 9), background="#0f172a", margin=0.6)
cards = [scene.geometry.rounded_rect(3, 1.666667, 0.2).fill(BLUE) for _ in range(6)]
row = scene.layout.row(cards, within="safe", width="fill", gap=0.4, padding=0.4, wrap=True, align="center")
page = scene.layout.column([scene.text("Wrapped cards", role="title").fill(GOLD), row], within="safe", width="fill", height="fill", gap=0.4, align="stretch")
scene.play([page.animate.fade_in().duration(0.5)])
