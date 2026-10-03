"""Fixes of 0.9.0 for imported SVGs, groups and effects, with their edge cases.

Top: `insignia.svg` puts every part under a transform and an offset view box.
The gradients keep their colors, the clipped window shows, the ring's stroke
is centered on its outline, and the SVG fades in as one layer (the pupil
stays under the eyelid). Next to it, `copy()` duplicates it with its parts.
Middle: a text and an overlapping group share one shadow, and a translucent
halo with `echo` rests without stacking its copies. Bottom: a head moves
with `keyframes(offset=...)` from wherever it is drawn, and `quantity` writes
compound units without uneven gaps. At the end the camera pulls back past
the 16 x 9 frame and the background keeps filling the view. The checks of
edge cases run before the scene and print their result at the bottom. Set
GAANIM_SNAPSHOTS to capture exact seeks for visual regression.
"""

import os
from pathlib import Path

from gaanim import GOLD, WHITE, Background, Scene, Text, quantity

ASSETS = Path(__file__).resolve().parent.parent / "docs/fixtures/assets"
INSIGNIA = str(ASSETS / "insignia.svg")
PERSONAJE = str(ASSETS / "personaje.svg")

# -- Edge cases, each on a fresh scene ---------------------------------------
checks = []


def expect(name, error, operation):
    """Record whether `operation` raises `error`, or succeeds when `error` is None."""
    try:
        operation()
    except Exception as raised:
        checks.append((name, error is not None and isinstance(raised, error)))
    else:
        checks.append((name, error is None))


def probe():
    return Scene(frame=(16, 9))


def keyframes_with(**channels):
    dot = probe().geometry.circle(0.3)
    dot.animate.keyframes([0, 0.5, 1], **channels)


def copied_parts_are_its_own():
    scene = probe()
    original = scene.media.svg(INSIGNIA)
    copy = original.copy()
    before = original.part("anillo").bounds().center
    copy.part("anillo").shift_by(2, 0)
    if copy.parts() != original.parts() or original.part("anillo").bounds().center != before:
        raise AssertionError("the copy shares parts with the original")


def relative_to_the_script():
    # Not in the working directory: found next to this script.
    probe().media.svg("../docs/fixtures/assets/insignia.svg")


expect("position y offset juntos", ValueError,
       lambda: keyframes_with(position=[None, (1, 0), None], offset=[None, (1, 0), None]))
expect("offset con 2 valores para 3 tiempos", ValueError,
       lambda: keyframes_with(offset=[None, (1, 0)]))
expect("offset vacío de None", None, lambda: keyframes_with(offset=[None, None, None]))
expect("copy() con partes propias", None, copied_parts_are_its_own)
expect("copy() de un Text es Text", None,
       lambda: isinstance(probe().text("hola").copy(), Text) or 1 / 0)
expect("ruta relativa al script", None, relative_to_the_script)
expect("SVG inexistente", RuntimeError, lambda: probe().media.svg("no/existe.svg"))
expect("quantity sin hueco en /", None,
       lambda: "#h(0pt)" in quantity(1600, "kg/m^3") or 1 / 0)

# -- The scene ---------------------------------------------------------------
scene = Scene(
    frame=(16, 9),
    background=Background.mesh_gradient(["#0f172a", "#1e3a8a", "#312e81", "#0f172a"], speed=0.0, seed=3),
)
label = lambda text, x, y: scene.text(text).fill("#cbd5e1").scale_to(0.26).move_to(x, y)

# Top: the SVG under transforms, and its copy.
insignia = scene.media.svg(INSIGNIA).scale_to(1.1).move_to(-4.6, 2.3)
label("SVG bajo transform y viewBox; se funde como una capa", -4.6, 0.45)
copia = insignia.copy().move_to(1.6, 2.3)
copia.part("medalla").opacity(0.45)
copia.part("anillo").fill(GOLD)
label("copy(): sus partes son propias", 1.6, 0.45)

# Top right: compound units.
units = scene.text.equation("rho =", quantity(1600, "kg/m^3")).fill(WHITE).scale_to(0.55).move_to(6.0, 2.9)
bag = scene.text.equation("m =", quantity(42.5, "kg/bolsa")).fill(WHITE).scale_to(0.55).move_to(6.0, 2.1)
label("quantity: kg/m³ sin huecos", 6.0, 1.45)

# Middle: one shadow for a text and for an overlapping group.
title = scene.text("Sombra única").fill(WHITE).scale_to(0.7).move_to(-4.6, -0.6)
title.shadow("#000000c0", x=0.1, y=-0.1, blur=0.1)
coins = scene.geometry.group([
    scene.geometry.circle(0.45).fill(GOLD).no_stroke().move_to(-1.0 + 0.45 * i, -0.6) for i in range(4)
]).shadow("#000000c0", x=0.12, y=-0.12, blur=0.12)
label("una sombra por texto y por grupo", -2.8, -1.45)

# Middle right: a translucent halo with echo, beside one without.
halo = scene.geometry.circle(0.55).fill("#ffffff55").no_stroke().move_to(2.6, -0.6).echo(8, decay=0.72)
plain = scene.geometry.circle(0.55).fill("#ffffff55").no_stroke().move_to(6.2, -0.6)
label("echo en reposo = sin echo", 4.4, -1.45)

# Bottom: keyframes by offset from where the head is drawn.
persona = scene.media.svg(PERSONAJE).scale_to(0.55).move_to(-6.2, -2.9)
head = persona.part("cabeza")
rest = head.bounds().center
marker = scene.geometry.circle(0.06).fill("#f87171").no_stroke().move_to(*rest)
label("keyframes(offset=...) desde donde está", -3.4, -3.6)

ok = sum(passed for _, passed in checks)
failed = [name for name, passed in checks if not passed]
summary = f"validaciones {ok}/{len(checks)}" + ("" if not failed else ": falla " + ", ".join(failed))
status = scene.text(summary).fill("#86efac" if not failed else "#fca5a5").scale_to(0.3).move_to(4.6, -3.6)

# -- Timeline ----------------------------------------------------------------
scene.play([insignia.animate.fade_in().duration(1.0), copia.animate.fade_in().duration(1.0)])
scene.play([
    halo.animate.move_to(4.4, -0.6).duration(1.0),
    head.animate.keyframes([0, 0.5, 1], offset=[None, (1.2, 0.3), None]).duration(1.0),
])
scene.wait(0.5)
scene.play([scene.camera.animate.to(scene.camera.state_2d(center=(0.0, 0.9), zoom=0.82)).duration(1.0)])
scene.wait(0.5)

snapshot_dir = os.environ.get("GAANIM_SNAPSHOTS")
if snapshot_dir:
    count = scene.snapshots(snapshot_dir, [0.5, 1.0, 1.5, 2.4, 3.9])
    print(f"[gaanim-diff] captured {count} exact seeks in {snapshot_dir}")

scene.render()
