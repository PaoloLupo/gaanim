"""Math coloring composed with Layout v2."""
from gaanim import GOLD, WHITE, Scene

scene = Scene(960, 540, background="#0f172a", margin=40)
equation = scene.text.equation("E = m c^2").fill(WHITE)
page = scene.layout.column([scene.text("Semantic math", role="title").fill(GOLD), scene.layout.item(equation, grow=1, align="center")], within="safe", width="fill", height="fill", padding=36, gap=28, align="stretch")
scene.play([page.animate.fade_in().duration(0.5)])
