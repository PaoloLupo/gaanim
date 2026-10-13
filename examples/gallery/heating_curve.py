"""The heating curve of water: temperature against heat added, traced as a thermometer fills, through melting and boiling.

A Parameter holds the heat added so far. The pen's position, the readouts
and the thermometer's fill level are all computed from it by interpolating
five measured points, the pen traces the curve as it goes, and the phases
are shaded bands named as the pen reaches them.
"""

import os

from gaanim import Axis, Easing, Scene, Zones, computed

scene = Scene(theme="gaanim")
violet = scene.canvas.color("accent")
gold = scene.canvas.color("title")
lilac = scene.canvas.color("chart")
paper = scene.canvas.color("foreground")
muted = scene.canvas.color("muted")
mist = scene.canvas.color("rule")
geometry = scene.geometry

# Heat in kJ for one kilogram of ice at -40 °C, and its temperature.
HEAT = [0, 84, 418, 837, 1100]
TEMPERATURE = [-40, 0, 0, 100, 100]
PHASES = [("ice", 0, 84), ("melting", 84, 418), ("water", 418, 837), ("boiling", 837, 1100)]


def temperature(q):
    """Temperature after adding q kJ, interpolated between the data points."""
    for (q0, t0), (q1, t1) in zip(zip(HEAT, TEMPERATURE), zip(HEAT[1:], TEMPERATURE[1:])):
        if q <= q1:
            return t0 + (t1 - t0) * (q - q0) / (q1 - q0)
    return TEMPERATURE[-1]


stage, side = scene.layout.zones(Zones.columns(["5fr", "1fr"], gap="48px", padding="page_padding"))
X0, X1, Y0, Y1 = 0, 1100, -60, 120
width, height = stage.width * 0.86, stage.height * 0.8
plane = scene.viz.cartesian_2d(
    Axis.linear(X0, X1).ticks(200).crossing("min"),
    Axis.linear(Y0, Y1).ticks(20).crossing("min"),
    width=width,
    height=height,
).move_to(*stage.center)


def at(x, y):
    cx, cy = stage.center
    return cx + (x - (X0 + X1) / 2) * width / (X1 - X0), cy + (y - (Y0 + Y1) / 2) * height / (Y1 - Y0)


bands, names = [], []
for index, (name, q0, q1) in enumerate(PHASES):
    (l, b), (r, t) = at(q0, Y0), at(q1, Y1)
    color = violet if index % 2 == 0 else lilac
    bands.append(geometry.rect(r - l, t - b).move_to((l + r) / 2, (b + t) / 2).fill(color).opacity(0.12).no_stroke()
                 .hidden())
    names.append(scene.text(name, role="code", color=muted).move_to((l + r) / 2, t - 0.3).hidden())

heat = scene.viz.parameter(0.0)
pen = geometry.dot(0.14).fill(gold).glow(gold, radius=0.22).follow(geometry.point_ref(
    computed(lambda q: at(q, 0)[0], inputs=[heat]), computed(lambda q: at(0, temperature(q))[1], inputs=[heat])
))
curve = geometry.traced_path(pen).stroke(gold, 0.06)

# The thermometer: a tube whose fill level is the temperature on the axis,
# with the heat and the temperature read out below it.
top_zone, bottom_zone = side.split(Zones.rows(["3fr", "1fr"]))
heat_zone, temperature_zone = bottom_zone.split(Zones.rows(2))
tube_height = top_zone.height * 0.85
tube = geometry.rounded_rect(0.5, tube_height, 0.25).no_fill().stroke(paper, 0.04).move_to(*top_zone.center)
bore = geometry.rounded_rect(0.36, tube_height - 0.14, 0.18).no_fill().no_stroke().move_to(*top_zone.center)
level = geometry.fill_level(
    bore, violet, computed(lambda q: (temperature(q) - Y0) / (Y1 - Y0), inputs=[heat]), keep_outline=False
)
readouts = [
    scene.viz.readout(heat, format=".0f", suffix=" kJ", font_size=0.45, color=muted).place(heat_zone, anchor="center"),
    scene.viz.readout(computed(temperature, inputs=[heat]), format=".0f", suffix=" °C", font_size=0.5, color=gold)
    .place(temperature_zone, anchor="center"),
]

scene.play(plane.animate.create(), duration=1.0)
scene.play([tube.animate.create(), level.animate.fade_in(), pen.animate.grow_from_center(),
            *(readout.animate.fade_in() for readout in readouts)], duration=0.6)
scene.play(curve.animate.fade_in(), duration=0.01)
# The pen traces the curve, phase by phase.
for band, name, (_, q0, q1) in zip(bands, names, PHASES):
    scene.play([band.animate.fade_in(), name.animate.fade_in()], duration=0.3)
    scene.play([heat.animate.set(q1)], duration=1.6 * (q1 - q0) / 400 + 0.4, easing=Easing.SMOOTH)
scene.wait(1.2)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    # Warming ice, melting, warming water, boiling, and the whole curve.
    scene.snapshots(snapshots, [2.2, 3.6, 5.4, 7.2, 8.4, 9.6])
else:
    scene.render()
