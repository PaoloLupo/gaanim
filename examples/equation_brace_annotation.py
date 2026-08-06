"""Semantic brace labels and annotations attached to equation tags."""

import os

from gaanim import BLACK, CORAL, GOLD, GRAY, WHITE, Scene


scene = Scene(1280, 720, background=BLACK)

title = scene.title("Explicar los términos de una ecuación").fill(WHITE).at(0, 250)
formula = scene.equation(
    "E = m c^2",
    tags={
        "energy": "E",
        "mass": "m",
        "light_speed": "c^2",
    },
).at(0, 60)
formula.tag("mass").fill(GOLD)
formula.tag("light_speed").fill(CORAL)

caption = scene.text("La llave nombra la masa").fill(GRAY).at(0, -220)

scene.play([title.write(), formula.write(), caption.fade_in()])
scene.wait(0.4)

# La llave se calcula con el límite real del tag ``mass``.
scene.brace_label(formula, "mass", "masa", duration=0.65)
scene.wait(0.5)

caption.fade_out(duration=0.2)
caption = scene.text("La línea sigue al término anotado").fill(GRAY).at(0, -220)
scene.play([caption.fade_in()])

# La línea comienza en c² y conserva ese extremo si el término se desplaza.
scene.annotate_tag(
    formula,
    "light_speed",
    "velocidad de la luz",
    offset=(175, 95),
    duration=0.65,
)
scene.wait(1.0)

snapshot_dir = os.environ.get("GAANIM_SNAPSHOTS")
if snapshot_dir:
    scene.snapshots(snapshot_dir, [0.0, 1.4, 2.2, 4.0, 4.5])

scene.render()
