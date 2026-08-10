"""Fixed, auto, and fractional grid tracks."""
from gaanim import BLUE, GOLD, WHITE, Scene

scene = Scene(960, 540, background="#111827", margin=36)
grid = scene.grid([scene.text("auto").fill(GOLD), scene.rect(120, 90).fill(BLUE), scene.text("2fr").fill(WHITE)], columns=["auto", 180, "2fr"], rows=["1fr"], gap=24, width="fill", height="fill", align="center")
page = scene.column([scene.title("Grid tracks").fill(GOLD), scene.item(grid, grow=1)], within="safe", width="fill", height="fill", padding=28, gap=24, align="stretch")
scene.play([page.fade_in().duration(0.5)])

