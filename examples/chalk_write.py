"""Chalk brush for write() and Drawable.chalk, with its edge cases.

Each case is labeled on the board. The checks of invalid input run before the
scene and print their result at the bottom. Set GAANIM_SNAPSHOTS to capture
exact seeks for visual regression.
"""

import math
import os

from gaanim import BLUE, GOLD, WHITE, Brush, Scene

BOARD = "#1f3a2e"
CHALK = "#f5f5f0"
scene = Scene(frame=(16, 9), background=BOARD)


def label(text, x, y):
    return scene.text(text).fill("#9fb8aa").scale_to(0.32).move_to(x, y)


# -- Invalid input is rejected, valid limits are accepted -------------------
probe = scene.geometry.circle(0.1).move_to(-20, 0)
checks = []


def expect(name, error, operation):
    """Record whether `operation` raises `error`, or succeeds when `error` is None."""
    try:
        operation()
    except Exception as raised:
        checks.append((name, error is not None and isinstance(raised, error)))
    else:
        checks.append((name, error is None))


expect("brush desconocido", ValueError, lambda: probe.animate.write(brush="crayon"))
expect("roughness < 0", ValueError, lambda: probe.chalk(roughness=-0.01))
expect("roughness NaN", ValueError, lambda: probe.chalk(roughness=math.nan))
expect("roughness inf", ValueError, lambda: probe.chalk(roughness=math.inf))
expect("seed negativa", OverflowError, lambda: probe.chalk(seed=-1))
expect("seed 2^64-1", None, lambda: probe.chalk(seed=2**64 - 1))
expect("roughness 0", None, lambda: probe.chalk(roughness=0))

# -- 1. A formula written in chalk -----------------------------------------
formula = scene.text.equation("E = m c^2").fill(CHALK).scale_to(2.0).move_to(-3.2, 3.0)
label("write(brush=\"chalk\", seed=3)", -3.2, 2.0)
pen = scene.text.equation("E = m c^2").fill(CHALK).scale_to(2.0).move_to(3.6, 3.0)
label("write() con pluma, para comparar", 3.6, 2.0)

# -- 2. Same seed draws the same chalk; another seed draws another ---------
twin_a = scene.geometry.circle(0.55).no_fill().stroke(GOLD, 0.06).move_to(-6.6, 0.4)
twin_b = scene.geometry.circle(0.55).no_fill().stroke(GOLD, 0.06).move_to(-5.2, 0.4)
other = scene.geometry.circle(0.55).no_fill().stroke(GOLD, 0.06).move_to(-3.8, 0.4)
label("seed 1   seed 1   seed 2", -5.2, -0.5)

# -- 3. Roughness limits: grain only, default, exaggerated ------------------
smooth = scene.geometry.rect(1.1, 0.8).no_fill().stroke(CHALK, 0.05).move_to(-1.6, 0.4)
default = scene.geometry.rect(1.1, 0.8).no_fill().stroke(CHALK, 0.05).move_to(-0.2, 0.4)
rough = scene.geometry.rect(1.1, 0.8).no_fill().stroke(CHALK, 0.05).move_to(1.2, 0.4)
smooth.chalk(seed=4, roughness=0)
default.chalk(seed=4)
rough.chalk(seed=4, roughness=0.05)
label("roughness 0   0.01   0.05", -0.2, -0.5)

# -- 4. A gradient fill keeps its paint under the grain --------------------
gradient = Brush.linear([BLUE, GOLD], start=(-0.7, 0), end=(0.7, 0))
blob = scene.geometry.star(5, 0.7, 0.32).fill(gradient).no_stroke().move_to(3.5, 0.4)
blob.chalk(seed=5)
label("relleno degradado", 3.5, -0.5)

# -- 5. A group: every member is chalk --------------------------------------
tri = scene.geometry.polygon([(-0.5, -0.4), (0.5, -0.4), (0, 0.45)]).no_fill().stroke(CHALK, 0.05)
dot = scene.geometry.circle(0.15).fill(GOLD).move_to(0, -0.05)
group = scene.geometry.group([tri, dot]).move_to(6.2, 0.4).chalk(seed=6)
label("grupo .chalk()", 6.2, -0.5)

# -- 6. Plain text whose write comes after the first play ------------------
late = scene.text("Pizarra").fill(CHALK).scale_to(1.6).move_to(-4.8, -2.4)
label("texto escrito tras el primer play", -4.8, -3.3)

# -- 7. Scaling up keeps the grain and tremor in scene units ----------------
grow = scene.geometry.circle(0.3).no_fill().stroke(CHALK, 0.05).move_to(0.0, -2.4)
grow.chalk(seed=7)
label("escala ×3: el grano no crece", 0.0, -3.6)

# -- 8. Chalk with a stroke that scales with the object --------------------
face = scene.geometry.circle(0.5).no_fill().stroke(CHALK, 0.04).move_to(4.8, -2.4)
face.scale_stroke_with_object().chalk(seed=8)
label("scale_stroke_with_object + escala", 4.8, -3.6)

ok = sum(passed for _, passed in checks)
failed = [name for name, passed in checks if not passed]
summary = f"validaciones {ok}/{len(checks)}" + ("" if not failed else ": falla " + ", ".join(failed))
status = scene.text(summary).fill("#86efac" if not failed else "#fca5a5").scale_to(0.34).move_to(0, -4.15)

# -- Timeline ----------------------------------------------------------------
scene.play([
    formula.animate.write(brush="chalk", seed=3).duration(1.6),
    pen.animate.write().duration(1.6),
])
scene.play([
    twin_a.animate.write(brush="chalk", seed=1).duration(0.8),
    twin_b.animate.write(brush="chalk", seed=1).duration(0.8),
    other.animate.write(brush="chalk", seed=2).duration(0.8),
    smooth.animate.write().duration(0.8),
    default.animate.write().duration(0.8),
    rough.animate.write().duration(0.8),
    blob.animate.write().duration(0.8),
    group.animate.write().duration(0.8),
])
scene.play([
    late.animate.write(brush="chalk", seed=9).duration(1.0),
    grow.animate.write().duration(0.6),
    face.animate.write().duration(0.6),
])
scene.play([
    grow.animate.scale_to(3.0).duration(1.0),
    face.animate.scale_to(1.8).duration(1.0),
])
scene.play([status.animate.fade_in().duration(0.3)])
scene.wait(0.5)
scene.play([other.animate.unwrite().duration(0.6)])

snapshot_dir = os.environ.get("GAANIM_SNAPSHOTS")
if snapshot_dir:
    count = scene.snapshots(snapshot_dir, [0.8, 1.6, 2.0, 2.4, 3.4, 4.4, 5.3, 5.8])
    print(f"[gaanim-diff] captured {count} exact seeks in {snapshot_dir}")

scene.render()
