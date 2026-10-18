"""Three identical spinning tops under glass, tilted alike and let go three ways: none of them falls, each precesses and nods between two circles.

A symmetric top pinned at its tip keeps its spin p_psi, its vertical angular
momentum p_phi and its energy E, so its tilt theta moves in the effective
potential V(theta) = (p_phi - p_psi cos theta)^2 / (2 I_1 sin^2 theta)
+ M g l cos theta and turns back wherever V reaches E. Three tops tilted 32
degrees and spun at 2.2 turns a second are let go at rest, pushed back and
pushed forward; the knob on each spindle writes cusps, loops or waves on its
glass globe between the two circles where theta turns. The moments of inertia
come from the drawn parts, the motion is integrated with fourth-order
Runge-Kutta, and every part is placed from scene time, so seeks are exact.
"""

import math
import os

from gaanim import Axis, Easing, Material3D, Scene, Zones, computed

TILT = math.radians(32)  # every top starts here, with theta' = 0
SPIN = 2.2 * math.tau  # omega_3, in radians a second
MGL = 4.979  # g times the height of the center of mass: the top let go at rest draws four cusps a turn
PUSHES = (0.0, -1.362, 0.4)  # the initial phi' in units of the slow steady precession
RATE, SUBSTEPS = 60, 4  # samples a second, and integration steps per sample
CONE_H, CONE_R, DISK_Y, DISK_R, DISK_T = 0.75, 0.32, 0.9, 0.95, 0.22
PEN, SPINDLE_R, KNOB_R = 1.8, 0.05, 0.08  # the knob at the end of the spindle writes on the glass
GLASS_R = 1.9
FACING = -math.pi / 2  # each top first leans to the left
SPOTS = [(-4.7, 0.45, 0.0), (0.0, 0.45, 0.0), (4.7, 0.45, 0.0)]  # the pinned tips
RELEASES = (1.6, 15.2, 15.2)
END = 33.0


def moments():
    """I_1 about a transverse axis through the tip and I_3 about the spin axis, per unit mass,
    of the drawn top: a cone from the tip, a disk, a spindle and its knob."""
    cone = math.pi * CONE_R**2 * CONE_H / 3
    disk = math.pi * DISK_R**2 * DISK_T
    spindle = math.pi * SPINDLE_R**2 * (PEN - DISK_Y)
    knob = 4 / 3 * math.pi * KNOB_R**3
    i3 = 0.3 * cone * CONE_R**2 + 0.5 * disk * DISK_R**2 + 0.5 * spindle * SPINDLE_R**2 + 0.4 * knob * KNOB_R**2
    i1 = cone * (3 / 20 * CONE_R**2 + 3 / 5 * CONE_H**2)
    i1 += disk * ((3 * DISK_R**2 + DISK_T**2) / 12 + DISK_Y**2)
    middle, length = (PEN + DISK_Y) / 2, PEN - DISK_Y
    i1 += spindle * ((3 * SPINDLE_R**2 + length**2) / 12 + middle**2)
    i1 += knob * (0.4 * KNOB_R**2 + PEN**2)
    mass = cone + disk + spindle + knob
    return i1 / mass, i3 / mass


I1, I3 = moments()
P_PSI = I3 * SPIN


def steady_rate():
    """The slow steady precession at TILT, the smaller root of I_1 cos(theta) phi'^2 - p_psi phi' + M g l = 0."""
    a = I1 * math.cos(TILT)
    return (P_PSI - math.sqrt(P_PSI**2 - 4 * a * MGL)) / (2 * a)


def simulate(push, duration):
    """(theta, phi, psi) at RATE samples a second for a top let go at TILT with phi' = push, and its p_phi."""
    p_phi = I1 * push * math.sin(TILT) ** 2 + P_PSI * math.cos(TILT)

    def rates(theta, theta_rate):
        s, c = math.sin(theta), math.cos(theta)
        phi_rate = (p_phi - P_PSI * c) / (I1 * s * s)
        nod = (phi_rate * phi_rate * c - P_PSI / I1 * phi_rate + MGL / I1) * s
        return theta_rate, nod, phi_rate, P_PSI / I3 - phi_rate * c

    h = 1 / (RATE * SUBSTEPS)
    state = (TILT, 0.0, 0.0, 0.0)
    samples = [(TILT, 0.0, 0.0)]
    for step in range(1, round(duration * RATE) * SUBSTEPS + 1):
        k1 = rates(state[0], state[1])
        k2 = rates(state[0] + h / 2 * k1[0], state[1] + h / 2 * k1[1])
        k3 = rates(state[0] + h / 2 * k2[0], state[1] + h / 2 * k2[1])
        k4 = rates(state[0] + h * k3[0], state[1] + h * k3[1])
        state = tuple(s + h / 6 * (a + 2 * b + 2 * c + d) for s, a, b, c, d in zip(state, k1, k2, k3, k4))
        if step % SUBSTEPS == 0:
            samples.append((state[0], state[2], state[3]))
    return samples, p_phi


def potential(theta, p_phi):
    return (p_phi - P_PSI * math.cos(theta)) ** 2 / (2 * I1 * math.sin(theta) ** 2) + MGL * math.cos(theta)


def axis_of(theta, phi):
    """The unit spin axis in the scene, y up: phi turns about y, theta tilts away from it."""
    return (math.sin(theta) * math.sin(phi), math.cos(theta), math.sin(theta) * math.cos(phi))


rate = steady_rate()
motions = [simulate(push * rate, END - release) for push, release in zip(PUSHES, RELEASES)]


def pose(k, t):
    """(theta, phi, psi) of top k at scene time t: spinning in place at TILT until it is let go."""
    after = t - RELEASES[k]
    if after <= 0:
        return TILT, FACING, SPIN * t
    samples = motions[k][0]
    i = min(int(after * RATE), len(samples) - 2)
    f = min(after * RATE - i, 1.0)
    a, b = samples[i], samples[i + 1]
    theta, phi, psi = (x + (y - x) * f for x, y in zip(a, b))
    return theta, phi + FACING, psi + SPIN * RELEASES[k]


def bounds(k):
    """The two angles where top k's tilt turns back."""
    thetas = [theta for theta, _, _ in motions[k][0]]
    return min(thetas), max(thetas)


scene = Scene(theme="gaanim")
gold = scene.canvas.color("title")
violet = scene.canvas.color("accent")
lilac = scene.canvas.color("chart")
paper = scene.canvas.color("foreground")
muted = scene.canvas.color("muted")
rule = scene.canvas.color("rule")
panel = scene.canvas.color("panel")
geometry = scene.geometry
geometry.lighting_3d("studio", intensity=1.1)
COLORS = [gold, violet, lilac]
steel = Material3D(paper, roughness=0.25, metallic=0.9)


def top(k):
    """Top k standing upright on its tip, inside three nested groups that turn about the tip:
    by phi about y, tilting by theta about x, and spinning by psi about y."""
    x, y, z = SPOTS[k]
    color = COLORS[k]
    cone = geometry.cone(CONE_R, CONE_H, segments=48, material=steel).rotate_to_3d(math.pi, 0, 0)
    cone.move_to_3d(x, y + CONE_H / 2, z)
    disk = geometry.cylinder(DISK_R, DISK_T, segments=72, material=Material3D(color, roughness=0.35))
    disk.move_to_3d(x, y + DISK_Y, z)
    beads = [
        geometry.sphere(0.07, material=Material3D.matte(paper)).move_to_3d(
            x + 0.8 * DISK_R * math.cos(a), y + DISK_Y + DISK_T / 2, z + 0.8 * DISK_R * math.sin(a)
        )
        for a in (0, math.tau / 3, 2 * math.tau / 3)
    ]
    spindle = geometry.cylinder(SPINDLE_R, PEN - DISK_Y, segments=16, material=steel)
    spindle.move_to_3d(x, y + (PEN + DISK_Y) / 2, z)
    knob = geometry.sphere(KNOB_R, material=Material3D.emissive(color, strength=1.5)).move_to_3d(x, y + PEN, z)
    theta, phi, psi = (computed(lambda t, j=j: pose(k, t)[j], inputs=[scene.time]) for j in range(3))
    spin = geometry.group(cone, disk, *beads, spindle, knob).with_pivot_3d(x, y, z).rotate_to_3d(0, psi, 0)
    tilt = geometry.group(spin).with_pivot_3d(x, y, z).rotate_to_3d(theta, 0, 0)
    return geometry.group(tilt).with_pivot_3d(x, y, z).rotate_to_3d(0, phi, 0)


def jar(k):
    """The stand, the post the tip sits on and the glass globe around top k."""
    x, y, z = SPOTS[k]
    base = y - GLASS_R - 0.1
    plinth = geometry.cylinder(1.3, 0.3, segments=72, material=Material3D(panel, roughness=0.6)).move_to_3d(x, base, z)
    post = geometry.cylinder(0.05, GLASS_R, material=steel).move_to_3d(x, y - GLASS_R / 2, z)
    cup = geometry.sphere(0.09, material=steel).move_to_3d(x, y, z)
    glass = geometry.sphere(GLASS_R, segments=72, rings=36, material=Material3D(paper, roughness=0.05))
    glass.move_to_3d(x, y, z).opacity(0.1)
    return [plinth, post, cup, glass]


def on_glass(k, theta, phi, lift=0.02):
    x, y, z = SPOTS[k]
    ax, ay, az = axis_of(theta, phi)
    radius = GLASS_R + lift
    return (x + radius * ax, y + radius * ay, z + radius * az)


def circles(k):
    """The two circles on the glass between which top k's tilt nods."""
    return [
        geometry.polyline_3d([on_glass(k, theta, math.tau * i / 120) for i in range(121)], muted).stroke(muted, 0.012)
        for theta in bounds(k)
    ]


def trace(k):
    """The path the knob of top k writes on the glass after it is let go, sampled 30 times a second.

    It starts trimmed to nothing. A 3D line trims by segments, and its samples are evenly spaced
    in time, so trimming it open at a steady rate keeps its end on the knob."""
    samples = motions[k][0][::2]
    return geometry.polyline_3d([on_glass(k, theta, phi + FACING) for theta, phi, _ in samples], COLORS[k]).stroke(
        COLORS[k], 0.028
    ).trim(end=0.0)


stands = [jar(k) for k in range(3)]
tops = [top(k) for k in range(3)]
rings = [circles(k) for k in range(3)]
traces = [trace(k) for k in range(3)]
for k in range(3):
    for part in [*stands[k], tops[k]]:
        part.opacity(0.0)

# The heads-up display of the first top: its effective potential, shifted so that its energy
# is the zero line. The tilt lives where V - E is below the line; the dot rides the line at
# the current tilt, and the drop to the curve is the energy left for nodding, I_1 theta'^2 / 2.
left, right = scene.layout.zones(Zones.columns(["1.15fr", "1fr"], gap="48px", padding="page_padding"))
formula_zone, plot_zone, readout_zone = right.split(Zones.rows([1.2, "1fr", 0.6], gap="24px"))
p_phi = motions[0][1]
energy = potential(TILT, p_phi)
low, high = bounds(0)
TH0, TH1, U0, U1 = 26.0, 54.0, -0.22, 0.42
plane = scene.viz.cartesian_2d(
    Axis.linear(TH0, TH1).ticks(4).label("θ"),
    Axis.linear(U0, U1).ticks(0.2),
    width=plot_zone.width * 0.95,
    height=plot_zone.height * 0.92,
    grid=False,
    y_numbers=False,
)
plane.move_to(*plot_zone.center)
axes = plane.drawable()
axes.hud().opacity(0.0)
well = plane.plot(lambda d: potential(math.radians(d), p_phi) - energy, (TH0 + 2, TH1)).stroke(paper, 0.04)
level = geometry.tracking_line(plane.data_to_scene(TH0, 0), plane.data_to_scene(TH1, 0)).stroke(gold, 0.03)
tilt = computed(lambda t: math.degrees(pose(0, t)[0]), inputs=[scene.time])
drop = computed(lambda d: potential(math.radians(d), p_phi) - energy, inputs=[tilt])
rider = geometry.dot(0.11).fill(gold).follow(plane.point_ref(tilt, 0.0))
kinetic = geometry.tracking_line(plane.point_ref(tilt, 0.0), plane.point_ref(tilt, drop)).stroke(gold, 0.035)
turns = [
    geometry.tracking_line(plane.data_to_scene(math.degrees(b), U0), plane.data_to_scene(math.degrees(b), 0)).stroke(
        muted, 0.02
    )
    for b in (low, high)
]
names = [
    scene.text.equation(name, size=0.4, color=muted).follow(plane.data_to_scene(math.degrees(b), U0), offset=(0, 0.3))
    for name, b in (("theta_1", low), ("theta_2", high))
]
formula = scene.text.equation(
    "V(theta) = (p_phi - p_psi cos theta)^2 / (2 I_1 sin^2 theta) + M g l cos theta", size=0.42
).place(formula_zone, anchor="bottom_left")
e_label = scene.text.equation("E", size=0.42, color=gold).follow(plane.data_to_scene(TH0, 0), offset=(-0.3, 0))
readout = scene.viz.readout(tilt, label="$theta$", format=".1f", suffix="°", font_size=0.45, color=gold)
readout.place(readout_zone, anchor="left")
hud = [well, level, rider, kinetic, *turns, *names, formula, e_label, readout]
for item in hud:
    item.hud().opacity(0.0)

# The angles of the two circles, written beside the first globe.
tags = [
    scene.text.equation(name, size=0.4, color=muted).move_to_3d(*on_glass(0, b, math.pi / 2, lift=0.3)).billboard()
    for name, b in (("theta_1", low), ("theta_2", high))
]
captions = []
for k, (what, how) in enumerate((("cusps", "let go at rest"), ("loops", "pushed back"), ("waves", "pushed forward"))):
    x, y, z = SPOTS[k]
    # The note sits below and in front of the word, so the two stay apart seen from the front or from above.
    word = scene.text(what, size=0.5, color=COLORS[k]).move_to_3d(x, y - GLASS_R - 0.6, z + 1.5).billboard()
    note = scene.text(how, size=0.32, color=muted).move_to_3d(x, y - GLASS_R - 0.95, z + 1.95).billboard()
    captions += [word, note]
closing = scene.text("None of them falls: each tip stays between its two circles.", size=0.42, color=paper).hud()
closing.place(scene.layout.zones(Zones.rows(["8fr", "1fr"], padding="page_padding"))[1])
for item in (*tags, *captions, closing, *(c for pair in rings for c in pair)):
    item.opacity(0.0)

# The camera looks at the first globe from in front and a little above, with the globe on the
# left of the frame and the display on the right.
scene.camera.perspective(fov_y=0.6)
scene.camera.look_at(eye=(-1.4, 3.6, 12.5), target=(-1.4, 0.0, 0.0))

# 0-1.6 s: the first top spins in place, tilted; then it is let go.
scene.play([part.animate.fade_in() for part in [*stands[0], tops[0]]], duration=1.0)
scene.wait_until(seconds=RELEASES[0])
scene.launch(traces[0].animate.trim(end=1.0).duration(END - RELEASES[0]).easing(Easing.LINEAR))
# 1.6-8 s: its effective potential, and the tilt nodding in its well.
scene.play([axes.animate.fade_in(), formula.animate.write()], duration=1.2)
scene.play([well.animate.create(), level.animate.fade_in(), e_label.animate.fade_in()], duration=1.2)
scene.play([rider.animate.fade_in(), kinetic.animate.fade_in(), readout.animate.fade_in()], duration=0.8)
scene.wait(1.2)
scene.play(
    [*(item.animate.fade_in() for item in (*turns, *names, *tags)), *(c.animate.fade_in() for c in rings[0])],
    duration=1.0,
)
scene.wait_until(seconds=12.0)

# 12-15.2 s: two more tops, spun and tilted alike, and the camera backs away to see all three.
scene.play([*(item.animate.fade_out() for item in (*hud, axes, *tags))], duration=0.8)
scene.play(
    [
        scene.camera.animate.look_at(eye=(0, 3.4, 15.5), target=(0, 0.0, 0)),
        *(part.animate.fade_in() for k in (1, 2) for part in [*stands[k], tops[k]]),
    ],
    duration=2.2,
    easing=Easing.SMOOTH,
)
scene.wait_until(seconds=RELEASES[1])
# 15.2 s: they are let go, one pushed back and one pushed forward.
for k in (1, 2):
    scene.launch(traces[k].animate.trim(end=1.0).duration(END - RELEASES[k]).easing(Easing.LINEAR))
# The camera drifts slowly around the globes while they write.
scene.launch(scene.camera.animate.orbit(delta_yaw=0.1, delta_pitch=0.0).duration(22.0 - scene.cursor).easing(Easing.LINEAR))
scene.play([caption.animate.fade_in() for caption in captions], duration=1.2)
scene.wait(2.5)
scene.play([c.animate.fade_in() for k in (1, 2) for c in rings[k]], duration=1.0)
scene.wait_until(seconds=22.0)
# 22-25 s: from higher up, where cusps, loops and waves are easiest to tell apart.
scene.play(scene.camera.animate.orbit(delta_yaw=-0.1, delta_pitch=0.6), duration=3.0, easing=Easing.SMOOTH)
scene.wait_until(seconds=28.0)
scene.play(closing.animate.fade_in(), duration=1.2)
scene.wait_until(seconds=END)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    # Spinning in place, the well and its turning points, the three tops let go, and from above.
    scene.snapshots(snapshots, [1.0, 9.0, 13.6, 19.0, 26.0, 32.0])
else:
    scene.render()
