"""Layout v2 fit modes without manual coordinates."""
from gaanim import BLUE, GOLD, Scene

scene = Scene(960, 540, background="#0f172a", margin=36)
formula = scene.column(
    [scene.title("Fit modes").fill(GOLD), scene.item(scene.circle(110).fill(BLUE), fit="contain", grow=1)],
    within="safe", width="fill", height="fill", padding=32, gap=28, align="center",
)
scene.play([formula.fade_in().duration(0.5)])
scene.wait(0.3)

