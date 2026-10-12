"""Mechanisms: bars and joints between moving points, a slider-crank and a four-bar linkage.

``bar_between`` and ``joint_at`` follow their endpoints every frame, so a
mechanism is a few reactive points driven by one angle: ``polar_point`` puts
the crank pin on a circle, ``computed`` solves the other joints from the same
angle, and ``move_to`` and ``rotate_to`` bind rigid bodies to those values.
Labels are the code that builds each part.
"""

import math
import os

from gaanim import Direction, Easing, Scene, Zones, computed

HOLD = 0.6
TURN = 4.0

scene = Scene()
fg = scene.canvas.color("foreground")
accent = scene.canvas.color("accent")
muted = scene.canvas.color("muted")
panel = scene.canvas.color("panel")


def legend(zone, lines):
    """Set code ``lines`` as one left-aligned block at the left of ``zone``."""
    scene.text("\n".join(lines), role="code", text_align="left", line_spacing=1.6).place(zone, anchor="left")


def turn(theta):
    """Turn ``theta`` through one revolution at a steady speed, then hold."""
    scene.play([theta.animate.set(math.tau).duration(TURN).easing(Easing.LINEAR)])
    scene.wait(HOLD)


# bar_between draws a round-capped bar between two endpoints (give it a color
# and width with stroke), and joint_at draws a joint that follows one; both
# track b as it rises and falls. Rows with joints add them to a bar like the
# first row's.
scene.segment("bars_and_joints")
ROWS = [
    "bar_between(a, b).stroke(fg, 0.12)",
    "bar_between(a, b).stroke(accent, 0.3)",
    "joint_at(a), joint_at(b)",
    "joint_at(b, size=0.8, color=accent)",
    'joint_at(b, kind="prismatic", size=0.8,\n         axis=Direction.UP)',
]
grid = scene.layout.zones(Zones.grid(rows=len(ROWS), columns=["5fr", "4fr"], column_gap="64px", padding="page_padding"))
lift = scene.viz.parameter(0.0)
parts = []
for index, label in enumerate(ROWS):
    scene.text(label, role="code", text_align="left").place(grid[2 * index], anchor="right")
    stage = grid[2 * index + 1].inset((0, 0.4))
    (left, y), (right, _) = stage.point("left"), stage.point("right")
    a = (left, y)
    b = scene.geometry.offset_point((right, y), 0, lift)
    width, color = (0.3, accent) if index == 1 else (0.12, fg)
    parts.append(scene.mechanics.bar_between(a, b).stroke(color, width))
    if index == 2:
        parts += [scene.mechanics.joint_at(a), scene.mechanics.joint_at(b)]
    elif index == 3:
        parts.append(scene.mechanics.joint_at(b, size=0.8, color=accent))
    elif index == 4:
        # The prismatic joint slides along a guide drawn through b's travel.
        parts.append(scene.geometry.line(right, y - 0.75, right, y + 0.75).stroke(muted, 0.03))
        parts.append(scene.mechanics.joint_at(b, kind="prismatic", size=0.8, axis=Direction.UP))
scene.play([part.animate.fade_in() for part in parts], duration=0.4)
for height in (0.6, -0.6, 0.0):
    scene.play([lift.animate.set(height).duration(1.0).easing(Easing.SMOOTH)])
scene.wait(HOLD)

# A slider-crank: the crank pin turns on a circle and the slider sits where the
# connecting rod of length L reaches the rail. The wheel turns with rotate_to.
scene.segment("slider_crank")
page = scene.layout.zones(Zones.rows([1.9, "1fr"], gap="16px", padding="page_padding"))
legend(page[0], [
    "tip = polar_point(o, R, theta)",
    "x = computed(slider_x, inputs=[theta])",
    "block.move_to(x, y)    wheel.rotate_to(theta)",
])
stage = page[1]
R, L = 1.8, 5.4
# Centre the span from the wheel's left edge to the block's furthest reach.
ox, oy = stage.center[0] - (L + 0.45) / 2, stage.center[1] + 0.6
theta = scene.viz.parameter(0.0)
o = (ox, oy)


def slider_x(angle):
    """Where a rod of length L from the crank pin meets the rail through o."""
    return ox + R * math.cos(angle) + math.sqrt(L**2 - (R * math.sin(angle)) ** 2)


tip = scene.geometry.polar_point(o, R, theta)
x = computed(slider_x, inputs=[theta])
pin = scene.geometry.point_ref(x, oy)
rail = scene.mechanics.fixed_support((ox + L, oy - 0.45), size=0.9, ground_length=2 * R + 1.8)
wheel = scene.geometry.circle(R + 0.3).fill(panel).stroke(muted, 0.04).move_to(ox, oy)
spokes = [scene.geometry.line(ox - R - 0.3, oy, ox + R + 0.3, oy), scene.geometry.line(ox, oy - R - 0.3, ox, oy + R + 0.3)]
hub = scene.geometry.group([wheel, *[spoke.stroke(muted, 0.04) for spoke in spokes]])
hub.rotate_to(theta)
block = scene.geometry.rounded_rect(1.5, 0.9, 0.1).fill(panel).stroke(fg, 0.05).move_to(x, oy)
mechanism = [
    scene.mechanics.pin_support(o, size=0.9, ground_length=1.6),
    scene.mechanics.bar_between(o, tip).stroke(accent, 0.2),
    scene.mechanics.bar_between(tip, pin).stroke(fg, 0.14),
    scene.mechanics.joint_at(tip, size=0.5),
    scene.mechanics.joint_at(pin, size=0.5),
    scene.mechanics.angle_between(o, Direction.RIGHT, tip, radius=0.7, sweep="ccw", label="$theta$", label_gap=0.25, arrowheads="end"),
    scene.mechanics.dimension_between(o, pin, R + 0.6, side="below", label="$x$", show_value=True, label_gap=0.15),
]
scene.play([hub.animate.fade_in(), rail.animate.create(), block.animate.fade_in()] + [part.animate.fade_in() for part in mechanism], duration=0.5)
turn(theta)

# A four-bar linkage: the rocker pin is where circles about the crank pin and
# the right pivot cross, and a pen on the coupler draws its coupler curve.
scene.segment("four_bar")
page = scene.layout.zones(Zones.columns([6.0, "1fr"], gap="32px", padding="page_padding"))
legend(page[0], [
    "b = polar_point(a, r, theta)",
    "c = point_ref(cx, cy)",
    "pen = dot().move_to(px, py)",
    "traced_path(pen)",
])
stage = page[1]
CRANK, COUPLER, ROCKER, GROUND = 1.7, 5.1, 4.0, 5.7
ax, ay = stage.left + CRANK + 0.2, stage.bottom + 2.4
dx = ax + GROUND
theta = scene.viz.parameter(0.0)


def joints(angle):
    """Crank pin, rocker pin and coupler pen of the linkage at ``angle``."""
    bx, by = ax + CRANK * math.cos(angle), ay + CRANK * math.sin(angle)
    ex, ey = dx - bx, ay - by
    span = math.hypot(ex, ey)
    along = (COUPLER**2 - ROCKER**2 + span**2) / (2 * span)
    across = math.sqrt(max(COUPLER**2 - along**2, 0.0))
    cx = bx + (along * ex - across * ey) / span
    cy = by + (along * ey + across * ex) / span
    # The pen sits above the middle of the coupler, like a triangular plate.
    ux, uy = (cx - bx) / COUPLER, (cy - by) / COUPLER
    return (cx, cy), (bx + (cx - bx) * 0.5 - uy * 1.5, by + (cy - by) * 0.5 + ux * 1.5)


cx, cy = (computed(lambda angle, i=i: joints(angle)[0][i], inputs=[theta]) for i in (0, 1))
px, py = (computed(lambda angle, i=i: joints(angle)[1][i], inputs=[theta]) for i in (0, 1))
a, d = (ax, ay), (dx, ay)
b = scene.geometry.polar_point(a, CRANK, theta)
c = scene.geometry.point_ref(cx, cy)
pen = scene.geometry.dot(0.12).fill(accent).move_to(px, py)
trail = scene.geometry.traced_path(pen).no_fill().stroke(accent, 0.04)
linkage = [
    scene.mechanics.pin_support(a, size=0.9, ground_length=1.6),
    scene.mechanics.pin_support(d, size=0.9, ground_length=1.6),
    scene.mechanics.bar_between(a, b).stroke(accent, 0.2),
    scene.mechanics.bar_between(b, c).stroke(fg, 0.14),
    scene.mechanics.bar_between(b, pen).stroke(fg, 0.08),
    scene.mechanics.bar_between(c, pen).stroke(fg, 0.08),
    scene.mechanics.bar_between(d, c).stroke(fg, 0.14),
    scene.mechanics.joint_at(b, size=0.5),
    scene.mechanics.joint_at(c, size=0.5),
]
scene.play([part.animate.fade_in() for part in linkage + [trail]] + [pen.animate.fade_in()], duration=0.5)
turn(theta)

# The bars rising, falling and level; then each mechanism at three points of
# its turn, the four-bar's last one with its coupler curve closed.
slider_start = 0.4 + 3.0 + HOLD
four_bar_start = slider_start + 0.5 + TURN + HOLD
if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(
        snapshots,
        [1.4, 2.4, 3.6]
        + [slider_start + 0.5 + TURN * f for f in (0.15, 0.5, 0.8)]
        + [four_bar_start + 0.5 + TURN * f for f in (0.3, 0.6, 1.0)],
    )
else:
    scene.render()
