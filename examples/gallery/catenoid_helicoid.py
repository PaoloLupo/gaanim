"""A helicoid that bends into a catenoid without stretching, its curvature painted on the material and two curves keeping their lengths.

The associate family X(θ) = cos θ · H + sin θ · C bends the helicoid H into
the catenoid C through minimal surfaces that all share one metric,
ds² = cosh² v (du² + dv²). Gauss's Theorema Egregium says curvature is
intrinsic: the color, K = −sech⁴ v, stays on the same points of the surface
while its shape changes completely, and the two marked curves, the line
u = 0.9 and the circle v = 0.55, keep their lengths. Every point of the
family shares one normal, so each curve is drawn just off both sides of the
sheet. The surface sweeps in along u, bends while the camera circles it, and
goes on to the mirror image of the helicoid.
"""

import math
import os

from gaanim import Axis, Brush, ColorMap, Easing, Material3D, Scene, Zones

U = (-math.pi, math.pi)
V = (-1.25, 1.25)
LINE_U = 0.9
CIRCLE_V = 0.55
OFFSET = 0.03

scene = Scene(theme="gaanim")
violet = scene.canvas.color("accent")
lilac = scene.canvas.color("chart")
gold = scene.canvas.color("title")
paper = scene.canvas.color("foreground")
ink = scene.canvas.color("background")
muted = scene.canvas.color("muted")
geometry = scene.geometry
geometry.lighting_3d("studio", intensity=1.1)
curvature = ColorMap.from_colors([violet, lilac, gold])


def sech4(v):
    """|K| = sech⁴ v, the same at every θ."""
    return 1 / math.cosh(v) ** 4


def family(u, v, theta):
    """The associate family: θ = 0 is the helicoid, θ = π/2 the catenoid."""
    c, s = math.cos(theta), math.sin(theta)
    return (
        c * math.sinh(v) * math.sin(u) + s * math.cosh(v) * math.cos(u),
        -c * math.sinh(v) * math.cos(u) + s * math.cosh(v) * math.sin(u),
        u * c + v * s,
    )


def normal(u, v):
    """The unit normal at (u, v), shared by every surface of the family."""
    return math.cos(u) / math.cosh(v), math.sin(u) / math.cosh(v), -math.tanh(v)


def lifted(u, v, theta, side):
    """The point of the surface at (u, v), moved OFFSET along the normal to one ``side``."""
    point, n = family(u, v, theta), normal(u, v)
    return tuple(p + side * OFFSET * k for p, k in zip(point, n))


def length(points):
    """Length of the polyline through ``points``."""
    return sum(math.dist(a, b) for a, b in zip(points, points[1:]))


space = scene.viz.cartesian_3d(
    Axis.linear(-2, 2),
    Axis.linear(-2, 2),
    Axis.linear(-3.5, 3.5),
    size=(3.6, 3.6, 6.3),
    grid=False,
    axes=False,
    ticks=False,
    numbers=False,
    labels=False,
)

# theta bends the surface; reach sweeps it in along u, from U[0] to U[1].
theta = scene.viz.parameter(0.0)
reach = scene.viz.parameter(0.0)
sheet = space.parametric_surface(
    lambda u, v, t, r: family(U[0] + (u - U[0]) * r, v, t),
    U,
    V,
    resolution=(121, 49),
    inputs=[theta, reach],
    color=lambda u, v: sech4(v),
    color_map=curvature,
    color_range=(sech4(V[1]), 1),
    material=Material3D(roughness=0.45),
)

# The line u = 0.9 in the theme's paper and the circle v = 0.55 in its ink.
curves = [
    space.parametric(lambda v, t, side=side: lifted(LINE_U, v, t, side), V, samples=160, inputs=[theta]).stroke(paper, 0.035)
    for side in (1, -1)
] + [
    space.parametric(lambda u, t, side=side: lifted(u, CIRCLE_V, t, side), U, samples=240, inputs=[theta]).stroke(ink, 0.035)
    for side in (1, -1)
]

def line_length(t):
    return length([family(LINE_U, V[0] + (V[1] - V[0]) * i / 159, t) for i in range(160)])


def circle_length(t):
    return length([family(U[0] + (U[1] - U[0]) * i / 239, CIRCLE_V, t) for i in range(240)])


# The heads-up display: what stays on the left, the family and its readouts
# on the right, a closing line along the bottom.
left, _, right = scene.layout.zones(Zones.columns(["1fr", "1.5fr", "1fr"], padding="page_padding"))
left_top, left_bar, left_bottom = left.split(Zones.rows(["1fr", "1.4fr", "1fr"], gap="24px"))
rows = right.split(Zones.rows(["1fr", 0.6, 0.6, 0.6, "1.8fr"], gap="12px"))
bottom = scene.layout.zones(Zones.rows(["8fr", "1fr"], padding="page_padding"))[1]

gauss = scene.text.equation('K = -"sech"^4 v', size=0.55).hud().place(left_top, anchor="bottom_left")
metric = scene.text.equation("d s^2 = cosh^2 v (d u^2 + d v^2)", size=0.45).hud().place(left_bottom, anchor="top_left")

BAR = (0.28, min(2.4, left_bar.height))
bar = geometry.rect(*BAR).no_stroke().fill(
    Brush.linear([violet, lilac, gold], start=(0, -BAR[1] / 2), end=(0, BAR[1] / 2))
).hud().place(left_bar, anchor="left")
bar_top = scene.text.equation("K = -1", size=0.38, color=muted).hud().place(bar, anchor="top_right", offset=(0.3, 0))
bar_bottom = scene.text.equation("K = -0.08", size=0.38, color=muted).hud().place(bar, anchor="bottom_right", offset=(0.3, 0))
scale = [bar, bar_top, bar_bottom]

formula = scene.text.equation("X_theta = cos theta H + sin theta C", size=0.5).hud().place(rows[0], anchor="bottom_left")
angle = scene.viz.readout(theta, label="$theta$", format=".2f", font_size=0.45).hud().place(rows[1], anchor="left")
line_readout = scene.viz.readout(line_length, inputs=[theta], label="$ell$", format=".3f", font_size=0.42, color=paper)
circle_readout = scene.viz.readout(circle_length, inputs=[theta], label="$ell$", format=".3f", font_size=0.42, color=lilac)
line_readout.hud().place(rows[2], anchor="left")
circle_readout.hud().place(rows[3], anchor="left")
closing = scene.text("Same metric, same curvature, different shape.", size=0.42, color=muted).hud().place(bottom)

DISTANCE = 15.0
scene.camera.perspective(fov_y=0.62)
pitch, yaw = math.radians(18), math.radians(-140)
eye = (
    DISTANCE * math.cos(pitch) * math.cos(yaw),
    DISTANCE * math.cos(pitch) * math.sin(yaw),
    DISTANCE * math.sin(pitch),
)
scene.camera.look_at(eye=eye, target=(0, 0, 0), up=(0, 0, 1))

# 0–3 s: the helicoid sweeps in along u.
scene.play(reach.animate.set(1), duration=3.0, easing=Easing.SMOOTH)
# 3–5 s: what stays: the curvature and the metric.
scene.play([item.animate.fade_in() for item in (gauss, metric, *scale)], duration=2.0)
# 5–6.5 s: the curves, the family and their readouts.
scene.play([item.animate.fade_in() for item in (*curves, formula, angle, line_readout, circle_readout)], duration=1.5)


def orbit(turn, rise=0.0):
    """A steady turn of the camera, as an ambient rotation."""
    return scene.camera.animate.orbit(delta_yaw=turn, delta_pitch=rise).easing(Easing.LINEAR)


# 6.5–15.5 s: bend the helicoid into the catenoid while the camera circles.
scene.play([theta.animate.set(math.pi / 2).easing(Easing.SMOOTH), orbit(1.08)], duration=9.0)
scene.play(orbit(0.12), duration=1.0)
# The camera rises and closes in.
scene.play([orbit(0.3, math.radians(12)).easing(Easing.SMOOTH), scene.camera.animate.perspective(fov_y=0.55)], duration=2.5)
scene.play(orbit(0.06), duration=0.5)
# And on, to the mirror image of the helicoid.
scene.play([theta.animate.set(math.pi).easing(Easing.SMOOTH), orbit(0.72)], duration=6.0)
scene.play([closing.animate.fade_in(), orbit(0.14)], duration=1.2)
scene.play(orbit(0.31), duration=2.6)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    # Sweeping in, the helicoid with its display, bending, the catenoid,
    # risen and closer, and the mirror image.
    scene.snapshots(snapshots, [1.5, 6.5, 11.0, 16.0, 18.5, 22.0, 26.0])
else:
    scene.render()
