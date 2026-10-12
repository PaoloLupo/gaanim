"""Transmissions: gears and their options, gear trains, racks and pinions, cams with followers and contact.

``scene.mechanics.gear``, ``rack`` and ``cam_profile`` draw editorial
silhouettes; the motion between them is a visual coupling, not a solver.
``bind_rotation_from`` turns a gear from another with a ratio and a phase,
``bind_translation_from_rotation`` slides a rack as a pinion turns, and a cam
bound to an angle with ``rotate_to`` lifts a follower placed by ``computed``.
Each label is the code that builds the variant.
"""

import math
import os

from gaanim import Direction, Easing, Scene, Zones, computed

HOLD = 0.6
SPIN = 3.0

scene = Scene()
fg = scene.canvas.color("foreground")
accent = scene.canvas.color("accent")
muted = scene.canvas.color("muted")
panel = scene.canvas.color("panel")


def cells(rows, columns, label_height=0.5):
    """A padded grid of cells, each split into a label strip and a stage."""
    grid = scene.layout.zones(Zones.grid(rows=rows, columns=columns, gap="48px", padding="page_padding"))
    return [zone.split(Zones.rows([label_height, "1fr"], gap="16px")) for zone in grid]


def legend(zone, lines):
    """Set code ``lines`` as one left-aligned block at the left of ``zone``."""
    scene.text("\n".join(lines), role="code", text_align="left", line_spacing=1.6).place(zone, anchor="left")


def radius(teeth):
    """Gears that mesh share a tooth pitch, so their radii follow their tooth counts."""
    return 0.07 * teeth


def mesh_phase(source_teeth, teeth, angle):
    """The phase that puts a tooth of the source in a gap of a gear at ``angle`` from it."""
    return angle + math.pi + (source_teeth * angle - 2.5 * math.pi) / teeth


# gear(radius, teeth) draws a toothed rim and a bore. The teeth add an eighth
# of the radius outside it; bore_radius and color are options.
scene.segment("gears")
GEARS = [
    ("gear(1.1, 12)", lambda: scene.mechanics.gear(1.1, 12)),
    ("gear(1.1, 24)", lambda: scene.mechanics.gear(1.1, 24)),
    ("gear(1.1, 48)", lambda: scene.mechanics.gear(1.1, 48)),
    ("gear(1.1, 24,\n     bore_radius=0.6)", lambda: scene.mechanics.gear(1.1, 24, bore_radius=0.6)),
    ("gear(1.1, 24,\n     color=accent)", lambda: scene.mechanics.gear(1.1, 24, color=accent)),
    ("gear(0.55, 12)", lambda: scene.mechanics.gear(0.55, 12)),
]
gears = []
for (label_zone, stage), (label, build) in zip(cells(2, 3, label_height=0.9), GEARS):
    scene.text(label, role="code", text_align="left").place(label_zone, anchor="bottom")
    gears.append(build().move_to(*stage.center))
scene.play([gear.animate.create() for gear in gears], duration=SPIN / 2)
scene.wait(HOLD)

# bind_rotation_from copies the source's rotation times ratio, plus phase.
# Meshing gears turn the other way at the ratio of their tooth counts, and
# the phase sets a tooth of one in a gap of the other.
scene.segment("gear_train")
page = scene.layout.zones(Zones.rows([2.0, "1fr"], gap="16px", padding="page_padding"))
TEETH = {"a": 24, "b": 12, "c": 36}
ratio_b, phase_b = -TEETH["a"] / TEETH["b"], mesh_phase(TEETH["a"], TEETH["b"], 0.0)
ratio_c, phase_c = -TEETH["b"] / TEETH["c"], mesh_phase(TEETH["b"], TEETH["c"], 0.0)
legend(page[0], [
    "a.animate.rotate_by(math.tau)",
    f"b.bind_rotation_from(a, ratio={ratio_b:g}, phase={phase_b:.3f})",
    f"c.bind_rotation_from(b, ratio={ratio_c:.3f}, phase={phase_c:.3f})",
])
stage = page[1]
spans = {name: radius(teeth) * 1.065 for name, teeth in TEETH.items()}
width = 2 * sum(spans.values())
x = stage.center[0] - width / 2
centers = {}
for name in "abc":
    centers[name] = (x + spans[name], stage.center[1])
    x += 2 * spans[name]
a = scene.mechanics.gear(radius(TEETH["a"]), TEETH["a"], bore_radius=0.32, color=accent).move_to(*centers["a"])
b = scene.mechanics.gear(radius(TEETH["b"]), TEETH["b"], bore_radius=0.32).move_to(*centers["b"])
c = scene.mechanics.gear(radius(TEETH["c"]), TEETH["c"], bore_radius=0.32).move_to(*centers["c"])
b.bind_rotation_from(a, ratio=ratio_b, phase=phase_b)
c.bind_rotation_from(b, ratio=ratio_c, phase=phase_c)
names = [scene.text(name, role="code").move_to(*centers[name]) for name in "abc"]
scene.play([gear.animate.fade_in() for gear in (a, b, c, *names)], duration=0.4)
scene.play([a.animate.rotate_by(math.tau).duration(2 * SPIN).easing(Easing.LINEAR)])
scene.wait(HOLD)

# bind_translation_from_rotation slides a drawable along an axis by the
# source's turn times scale: at the pitch radius r, rack and pinion roll
# together. The pinion turns one way and then back.
scene.segment("rack_and_pinion")
page = scene.layout.zones(Zones.columns(2, gap="32px", padding="page_padding"))
legend(page[0], [
    "pinion.animate.rotate_by(0.9)",
    "low.bind_translation_from_rotation(",
    "    pinion, axis=Direction.RIGHT, scale=r)",
    "side.bind_translation_from_rotation(",
    "    pinion, axis=Direction.UP, scale=r)",
])
TEETH_PINION, RACK_TEETH, TURN = 24, 9, 0.9
pitch_radius = radius(TEETH_PINION) * 1.065
pitch = math.tau * pitch_radius / TEETH_PINION
height = 1.25 * pitch
gx, gy = page[1].center[0] + 0.3, page[1].center[1] - 0.7
# A gap of the pinion faces down and right, where a tooth of each rack sits;
# the racks start two teeth back so that they stay in the frame.
pinion = scene.mechanics.gear(radius(TEETH_PINION), TEETH_PINION, bore_radius=0.32, color=accent)
pinion.move_to(gx, gy).rotate_to(-math.pi / 2 - 7 * math.pi / (4 * TEETH_PINION))
# A rack's tips lie 0.725 of its height from its centre.
tips = radius(TEETH_PINION) + 0.01 + 0.725 * height
low = scene.mechanics.rack(RACK_TEETH * pitch, RACK_TEETH).move_to(gx - 2 * pitch, gy - tips)
side = scene.mechanics.rack(RACK_TEETH * pitch, RACK_TEETH).rotate_to(math.pi / 2).move_to(gx + tips, gy + 2 * pitch)
low.bind_translation_from_rotation(pinion, axis=Direction.RIGHT, scale=pitch_radius)
side.bind_translation_from_rotation(pinion, axis=Direction.UP, scale=pitch_radius)
scene.play([part.animate.fade_in() for part in (pinion, low, side)], duration=0.4)
scene.play([pinion.animate.rotate_by(TURN).duration(SPIN / 2).easing(Easing.SMOOTH)])
scene.play([pinion.animate.rotate_by(-TURN).duration(SPIN / 2).easing(Easing.SMOOTH)])
scene.wait(HOLD)

# cam_profile takes (angle, radius) samples. Each cam turns with rotate_to(turn)
# and its follower rests on the profile straight above the shaft, through a
# prismatic guide.
scene.segment("cams")
SAMPLES = 120
ANGLES = [math.tau * i / SAMPLES for i in range(SAMPLES)]


def eccentric(angle):
    """A disc of radius 1 whose centre sits 0.4 off the shaft."""
    return 0.4 * math.cos(angle) + math.sqrt(1 - (0.4 * math.sin(angle)) ** 2)


def lobes(angle):
    """Two lobes: the follower rises and falls twice per turn."""
    return 0.95 + 0.35 * math.cos(2 * angle)


def heart(angle):
    """A rise and fall at constant speed."""
    return 0.7 + 0.7 * (1 - abs(angle - math.pi) / math.pi)


CAMS = [("cam_profile(ECCENTRIC)", eccentric), ("cam_profile(LOBES)", lobes), ("cam_profile(HEART)", heart)]
SCALE = 1.4
turn = scene.viz.parameter(0.0)
parts = []
for (label_zone, stage), (label, profile) in zip(cells(1, 3), CAMS):
    scene.text(label, role="code").place(label_zone)
    cx, cy = stage.center[0], stage.bottom + 2.2
    cam = scene.mechanics.cam_profile([(angle, SCALE * profile(angle)) for angle in ANGLES], bore_radius=0.2)
    cam.move_to(cx, cy).rotate_to(turn)
    # The profile under the follower is the one a quarter turn ahead of the cam's angle.
    lift = computed(lambda t, f=profile: SCALE * f((math.pi / 2 - t) % math.tau), inputs=[turn])
    rod = scene.geometry.rounded_rect(0.3, 2.4, 0.08).fill(panel).stroke(fg, 0.05)
    rod.move_to(cx, computed(lambda r, y=cy: y + r + 1.2, inputs=[lift]))
    guide = scene.mechanics.joint_at((cx, cy + 2.8), kind="prismatic", axis=Direction.UP, size=0.9)
    parts += [cam, rod, guide]
scene.play([part.animate.fade_in() for part in parts], duration=0.4)
scene.play([turn.animate.set(math.tau).duration(2 * SPIN).easing(Easing.LINEAR)])
scene.wait(HOLD)

# contact_on_curve marks a point on a curve with its tangent and normal, at
# a fraction t of the way round it; the lengths are options.
scene.segment("contact")
OUTLINE = [(2.2 * math.cos(angle) * lobes(angle), 2.2 * math.sin(angle) * lobes(angle)) for angle in ANGLES]
CONTACTS = [
    ("contact_on_curve(outline, t)", {}),
    (
        "contact_on_curve(outline, t,\n    tangent_length=2.4,\n    normal_length=1.6)",
        {"tangent_length": 2.4, "normal_length": 1.6},
    ),
]
t = scene.viz.parameter(0.0)
parts = []
for (label_zone, stage), (label, options) in zip(cells(1, 2, label_height=1.6), CONTACTS):
    scene.text(label, role="code", text_align="left").place(label_zone, anchor="bottom")
    outline = scene.geometry.polygon([(stage.center[0] + px, stage.center[1] + py) for px, py in OUTLINE])
    outline.no_fill().stroke(muted, 0.04)
    parts += [outline, scene.mechanics.contact_on_curve(outline, t, **options)]
scene.play([part.animate.fade_in() for part in parts], duration=0.4)
scene.play([t.animate.set(1.0).duration(2 * SPIN).easing(Easing.LINEAR)])
scene.wait(HOLD)

# Gears being drawn and drawn; the train at a third and two thirds of a
# turn; the racks out and back; the cams and the contact point at a
# quarter, a half and most of the way round.
gears_end = SPIN / 2 + HOLD
train_end = gears_end + 0.4 + 2 * SPIN + HOLD
racks_end = train_end + 0.4 + SPIN + HOLD
cams_end = racks_end + 0.4 + 2 * SPIN + HOLD
if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(
        snapshots,
        [SPIN / 4, gears_end - 0.1]
        + [gears_end + 0.4 + 2 * SPIN * f for f in (0.33, 0.66)]
        + [train_end + 0.4 + SPIN * f for f in (0.5, 1.0)]
        + [racks_end + 0.4 + 2 * SPIN * f for f in (0.25, 0.5, 0.8)]
        + [cams_end + 0.4 + 2 * SPIN * f for f in (0.25, 0.6)],
    )
else:
    scene.render()
