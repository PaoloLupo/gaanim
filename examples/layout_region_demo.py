"""Safe-frame root layout (replaces legacy regions)."""
from gaanim import GOLD, GRAY, WHITE, Scene

scene = Scene(frame=(16, 9), background="#0f172a", margin=0.666667)
page = scene.layout.column(
    [scene.text("Safe frame", role="title").fill(GOLD), scene.layout.item(scene.text("Responsive content").fill(WHITE), grow=1), scene.text("within='safe'").fill(GRAY)],
    within="safe", width="fill", height="fill", padding=0.533333, gap=0.4, align="stretch", justify="between",
)
scene.play([page.animate.fade_in().duration(0.5)])
scene.render()
