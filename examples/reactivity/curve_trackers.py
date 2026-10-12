"""Curve trackers: a point, tangent, normal and osculating circle that ride a curve, and traced paths.

``point_on_curve``, ``tangent_on_curve``, ``normal_on_curve`` and
``curvature_on_curve`` place a mark at a fraction of a curve's arc length
given by a Parameter (or Variable), recomputed every frame without Python.
They work on polylines, Bézier curves, arcs and polygons. ``traced_path``
records where a drawable has been, whole or fading. Each panel or row is
labelled with its code.
"""

import math
import os

from gaanim import Easing, Scene, Zones, computed

RUN = 4.0
HOLD = 0.5

scene = Scene()
accent = scene.canvas.color("accent")
success = scene.canvas.color("success")
foreground = scene.canvas.color("foreground")
muted = scene.canvas.color("muted")
rule = scene.canvas.color("rule")


def code(text):
    """A code label, its lines aligned left."""
    return scene.text(text, role="code", text_align="left")


def panels(rows, columns, head="0.5"):
    """A rows x columns grid of panels, each split into a label head and a stage."""
    grid = scene.layout.zones(Zones.grid(rows=rows, columns=columns, gap="48px", padding="page_padding"))
    return [zone.split(Zones.rows([float(head), "1fr"], gap="12px")) for zone in grid]


def spiral(stage):
    """An Archimedean spiral, three half turns out from the middle of ``stage``, in scene coordinates."""
    (cx, cy), grow = stage.center, (min(stage.width, stage.height) / 2 - 0.1) / (3 * math.pi)
    angles = [3 * math.pi * index / 240 for index in range(241)]
    points = [(cx + grow * (1 + a) * math.cos(a), cy + grow * (1 + a) * math.sin(a)) for a in angles]
    return scene.geometry.polyline(points).no_fill().stroke(rule, 0.03)


def ride(tracker, duration=RUN):
    """Move ``tracker`` from 0 to 1 along its curve."""
    scene.play(tracker.animate.set(1).duration(duration).easing(Easing.LINEAR))
    scene.wait(HOLD)


# Each tracker takes the curve and a Parameter from 0 (its start) to 1 (its
# end), measured along the arc length. They start hidden: fade them in. Build
# the curve where it is drawn, in scene coordinates: the trackers follow the
# curve's own points.
trackers_start = scene.cursor
scene.segment("trackers")
t = scene.viz.parameter(0.0)
cells = panels(2, 2)
for (head, _), label in zip(cells, [
    "point_on_curve(curve, t)",
    "tangent_on_curve(curve, t, length=1.6)",
    "normal_on_curve(curve, t, length=1.2)",
    "curvature_on_curve(curve, t)",
]):
    code(label).place(head, anchor="center")
curves = [spiral(stage) for _, stage in cells]
marks = [
    scene.geometry.point_on_curve(curves[0], t).scale_to(1.6),
    scene.geometry.tangent_on_curve(curves[1], t, length=1.6).stroke(accent, 0.05),
    scene.geometry.normal_on_curve(curves[2], t, length=1.2).stroke(accent, 0.05),
    scene.geometry.curvature_on_curve(curves[3], t).no_fill().stroke(accent, 0.04),
]
scene.play([mark.animate.fade_in() for mark in marks], duration=0.2)
ride(t)

# A tangent is centered on its point; length sets its size. curvature_on_curve
# estimates the circle from samples a window apart (a fraction of the curve):
# a wider window smooths it. A Variable works as the tracker and shows itself.
options_start = scene.cursor
scene.segment("options")
t = scene.viz.parameter(0.0)
cells = panels(2, 2, head="1.1")
for (head, _), label in zip(cells, [
    "tangent_on_curve(curve, t)",
    "curvature_on_curve(curve, t,\n                   window=0.1)",
    "point_on_curve(curve, t)\ntangent_on_curve(curve, t)\nnormal_on_curve(curve, t)",
    'v = scene.viz.variable(0, label="$v$")\npoint_on_curve(curve, v)',
]):
    code(label).place(head, anchor="center")
curves = [spiral(stage) for _, stage in cells]
v = scene.viz.variable(0, label="$v$", font_size=0.6)
v.move_to(cells[3][1].right - 0.8, cells[3][1].top - 0.3)
marks = [
    scene.geometry.tangent_on_curve(curves[0], t).stroke(accent, 0.05),
    scene.geometry.curvature_on_curve(curves[1], t, window=0.1).no_fill().stroke(accent, 0.04),
    scene.geometry.point_on_curve(curves[2], t).scale_to(1.4),
    scene.geometry.tangent_on_curve(curves[2], t).stroke(accent, 0.05),
    scene.geometry.normal_on_curve(curves[2], t).stroke(success, 0.05),
    scene.geometry.point_on_curve(curves[3], v).scale_to(1.6),
]
scene.play([mark.animate.fade_in() for mark in marks], duration=0.2)
scene.play([t.animate.set(1), v.animate.set(1)], duration=RUN, easing=Easing.LINEAR)
scene.wait(HOLD)

# Any open or closed path works: a sampled polyline, a Bézier curve kept as
# Bézier segments, an arc, or a polygon, whose tangent turns at each corner.
# Each row tracks its curve with point_on_curve and tangent_on_curve.
curves_start = scene.cursor
scene.segment("curves")
grid = scene.layout.zones(Zones.grid(rows=4, columns=["6fr", "5fr"], column_gap="48px", padding="page_padding"))
t = scene.viz.parameter(0.0)
stages = [grid[2 * row + 1].inset((0.1, 0.3)) for row in range(4)]
for row, label in enumerate([
    "curve = scene.geometry.polyline(points)",
    "curve = scene.geometry.bezier(\n    start, [c1, c2], end)",
    "curve = scene.geometry.arc(\n    x, y, radius, start, sweep)",
    "curve = scene.geometry.polygon(points)",
]):
    code(label).place(grid[2 * row], anchor="right")
stage = stages[0]
wave = [
    (stage.left + stage.width * k / 120, stage.center[1] + stage.height / 2 * math.sin(4 * math.pi * k / 120))
    for k in range(121)
]
(left, bottom), (right, top) = (stages[1].left, stages[1].bottom), (stages[1].right, stages[1].top)
(ax, ay), radius = stages[2].center, stages[2].height * 1.2
stage = stages[3]
zigzag = [(stage.left + stage.width * k / 6, stage.bottom if k % 2 == 0 else stage.top) for k in range(7)]
curves = [
    scene.geometry.polyline(wave),
    scene.geometry.bezier((left, bottom), [(left + 2.5, top + 0.6), (right - 2.5, bottom - 0.6)], (right, top)),
    scene.geometry.arc(ax, ay - radius + stages[2].height / 2, radius, math.pi / 2 - 1.0, 2.0),
    scene.geometry.polygon(zigzag + [(stage.right, stage.bottom)]),
]
marks = []
for curve in curves:
    curve.no_fill().stroke(rule, 0.03)
    marks.append(scene.geometry.point_on_curve(curve, t).scale_to(1.4))
    marks.append(scene.geometry.tangent_on_curve(curve, t, length=1.2).stroke(accent, 0.05))
scene.play([mark.animate.fade_in() for mark in marks], duration=0.2)
ride(t)

# traced_path draws where a drawable has been since it was declared, and
# rebuilds itself on a seek. dissipating_time lets samples fade after that
# many seconds, max_points keeps only the latest samples, and min_distance
# skips samples closer than that to the last one.
traces_start = scene.cursor
scene.segment("traces")
grid = scene.layout.zones(Zones.grid(rows=4, columns=["6fr", "5fr"], column_gap="48px", padding="page_padding"))
t = scene.viz.parameter(0.0)
trails = []
for row, (label, options) in enumerate([
    ("traced_path(dot)", {}),
    ("traced_path(dot, dissipating_time=1.5)", {"dissipating_time": 1.5}),
    ("traced_path(dot, max_points=20)", {"max_points": 20}),
    ("traced_path(dot, min_distance=0.5)", {"min_distance": 0.5}),
]):
    code(label).place(grid[2 * row], anchor="right")
    stage = grid[2 * row + 1].inset((0.1, 0.3))
    (x0, y0), width, height = stage.point("left"), stage.width, stage.height / 2
    x = computed(lambda a, x0=x0, width=width: x0 + width * a, inputs=[t])
    y = computed(lambda a, y0=y0, height=height: y0 + height * math.sin(5 * math.pi * a), inputs=[t])
    dot = scene.geometry.dot(0.14).follow(scene.geometry.point_ref(x, y))
    trails.append(dot.animate.fade_in())
    trails.append(scene.geometry.traced_path(dot, **options).stroke(accent, 0.04).animate.fade_in())
scene.play(trails, duration=0.2)
ride(t)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    # Each segment a quarter, half and nearly all the way along.
    scene.snapshots(
        snapshots,
        [start + 0.2 + RUN * fraction for start in (trackers_start, options_start, curves_start, traces_start)
         for fraction in (0.3, 0.6, 0.95)],
    )
else:
    scene.render()
