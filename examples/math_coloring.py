"""Math coloring composed with Layout v2."""
from gaanim import GOLD, WHITE, Scene

scene = Scene(frame=(16, 9), background="#0f172a", margin=0.666667)
equation = scene.text.equation("E = m c^2").fill(WHITE)
page = scene.layout.column([scene.text("Semantic math", role="title").fill(GOLD), scene.layout.item(equation, grow=1, align="center")], within="safe", width="fill", height="fill", padding=0.6, gap=0.466667, align="stretch")
scene.play([page.animate.fade_in().duration(0.5)])
