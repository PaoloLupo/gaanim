"""Safe-frame root layout (replaces legacy regions)."""
from gaanim import GOLD, GRAY, WHITE, Scene

scene = Scene(960, 540, background="#0f172a", margin=40)
page = scene.column(
    [scene.text("Safe frame", role="title").fill(GOLD), scene.item(scene.text("Responsive content").fill(WHITE), grow=1), scene.text("within='safe'").fill(GRAY)],
    within="safe", width="fill", height="fill", padding=32, gap=24, align="stretch", justify="between",
)
scene.play([page.fade_in().duration(0.5)])
scene.render()
