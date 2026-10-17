"""Eight paths of the Lorenz system that start a ten-thousandth apart, travel as one curve, then split and each weave the butterfly its own way.

In 1963 Edward Lorenz cut a model of convection down to three equations,
x' = sigma (y - x), y' = x (rho - z) - y, z' = x y - beta z, and found that
it never settles and never repeats: a point winds around one lobe, then the
other, in an order no one can predict. The difference between two paths
doubles about every three quarters of a unit of time. The paths are
integrated with fourth-order Runge-Kutta (sigma = 10, rho = 28,
beta = 8/3) and drawn as they grow while the camera swings slowly around
the attractor.
"""

import math
import os

from gaanim import ColorMap, Easing, Material3D, Scene, Zones

SIGMA, RHO, BETA = 10.0, 28.0, 8.0 / 3.0
DT, STEPS = 0.004, 6500  # 26 units of time
SETTLE = 3000  # steps run before the paths start, so that they start on the attractor
PATHS = 8
GROW = 22.0  # seconds the paths take to grow
SPIN = 0.05  # radians per second the camera turns around the attractor


def lorenz(x, y, z):
    return SIGMA * (y - x), x * (RHO - z) - y, x * y - BETA * z


def step(p):
    """One step of fourth-order Runge-Kutta."""
    a = lorenz(*p)
    b = lorenz(*(p[i] + DT / 2 * a[i] for i in range(3)))
    c = lorenz(*(p[i] + DT / 2 * b[i] for i in range(3)))
    d = lorenz(*(p[i] + DT * c[i] for i in range(3)))
    return tuple(p[i] + DT / 6 * (a[i] + 2 * b[i] + 2 * c[i] + d[i]) for i in range(3))


def integrate(start):
    """The path from ``start``, STEPS + 1 points."""
    path = [start]
    for _ in range(STEPS):
        path.append(step(path[-1]))
    return path


scene = Scene(theme="gaanim")
violet = scene.canvas.color("accent")
lilac = scene.canvas.color("chart")
gold = scene.canvas.color("title")
frame, = scene.layout.zones(Zones.rows(1))

# The attractor is about 50 units tall around z = 25; SCALE fits it in the frame.
SCALE = 0.115 * frame.height / 8
CENTER = (0.0, 0.0, 25.0)


def to_screen(point):
    return tuple((point[i] - CENTER[i]) * SCALE for i in range(3))


settled = (0.0, 1.0, 1.05)
for _ in range(SETTLE):
    settled = step(settled)
paths = [[to_screen(p) for p in integrate((settled[0] + 1e-4 * k, settled[1], settled[2]))] for k in range(PATHS)]
colors = ColorMap.from_colors([violet, lilac, gold]).colors(PATHS)

# create() reveals a 3D line one segment at a time, so a linear reveal of one segment
# per step keeps each curve's tip on its head.
curves, heads = [], []
for path, color in zip(paths, colors):
    curves.append(scene.geometry.polyline_3d(path, color).stroke(color, 0.022).opacity(0.85))
    heads.append(scene.geometry.sphere(0.07, material=Material3D.emissive(color)).move_to_3d(*path[0]).opacity(0.0))

equations = scene.text.equation(
    r" dot(x) &= sigma (y - x) \ dot(y) &= x (rho - z) - y \ dot(z) &= x y - beta z ",
    size=0.5,
).hud()
equations.place(scene.layout.zones(Zones.rows(1, padding="page_padding"))[0], anchor="top_left")
equations.opacity(0.0)

# The camera looks down 22 degrees onto the attractor, from just short of the -y side,
# and swings slowly through the view that shows the two lobes side by side.
PITCH, YAW, DISTANCE = math.radians(22), math.radians(-88), 40
scene.camera.perspective(fov_y=2 * math.atan(frame.height / 2 / DISTANCE) / 1.05)
scene.camera.look_at(
    eye=(DISTANCE * math.cos(PITCH) * math.cos(YAW), DISTANCE * math.cos(PITCH) * math.sin(YAW), DISTANCE * math.sin(PITCH)),
    target=(0, 0, 0),
    up=(0, 0, 1),
)


def turn(duration):
    return scene.camera.animate.orbit(delta_yaw=SPIN * duration, delta_pitch=0.0)


scene.play([turn(1.5), equations.animate.write()] + [head.animate.opacity(1.0) for head in heads],
           duration=1.5, easing=Easing.LINEAR)
times = [GROW * k / STEPS for k in range(STEPS + 1)]
for head, path in zip(heads, paths):
    head.drive_from_samples(times, [(x, y) for x, y, _ in path], "xy", absolute=True)
    head.drive_from_samples(times, [z for _, _, z in path], "z", absolute=True)
scene.play([turn(GROW)] + [curve.animate.create() for curve in curves],
           duration=GROW, easing=Easing.LINEAR)
scene.play([turn(1.0)] + [head.animate.fade_out() for head in heads], duration=1.0, easing=Easing.LINEAR)
scene.play(turn(2.5), duration=2.5, easing=Easing.LINEAR)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    # The equations, the paths still as one, splitting, apart, and the whole butterfly.
    scene.snapshots(snapshots, [1.4, 6.0, 11.0, 17.0, 26.5])
else:
    scene.render()
