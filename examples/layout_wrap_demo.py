"""Responsive wrapped row."""
from gaanim import BLUE, GOLD, Scene

scene = Scene(960, 540, background="#0f172a", margin=36)
cards = [scene.rounded_rect(180, 100, 12).fill(BLUE) for _ in range(6)]
row = scene.row(cards, within="safe", width="fill", gap=24, padding=24, wrap=True, align="center")
page = scene.column([scene.title("Wrapped cards").fill(GOLD), row], within="safe", width="fill", height="fill", gap=24, align="stretch")
scene.play([page.fade_in().duration(0.5)])

