"""Timeline: events on an axis of dates, their labels stacked in lanes that do not overlap.

``scene.viz.timeline`` draws the axis, with scale breaks where the dates
crowd; each ``event`` puts its label in the first free lane of its side and
grows it toward the side where it fits. ``reserve`` keeps dates free for
what is drawn apart. Each label is the code that builds its timeline.
"""

import os

from gaanim import Scene, Zones

scene = Scene()
success = scene.colors.success


def stage(label):
    """The code label at the top and the room for a timeline below it."""
    zones = scene.layout.zones(Zones.rows(["auto", "1fr"], gap="24px", padding="page_padding"))
    scene.text(label, role="code").place(zones[0], anchor="left")
    return zones[1]


def reveal(line):
    """The axis, then every event: dot, stem and label; return when it is all shown."""
    scene.play(line.axis.animate.create().duration(0.8))
    for event in line.events:
        scene.play([part.animate.fade_in().duration(0.3) for part in event.parts])
    return scene.cursor


# Without a side, each label takes the side whose free lane is nearer the
# axis, alternating when both are as near; the break gives the last decade
# almost half the axis.
scene.segment("auto")
room = stage("scene.viz.timeline(1980, 2026, breaks=[(2016, 0.55)])")
line = scene.viz.timeline(1980, 2026, x_range=(room.left, room.right), y=room.center[1], breaks=[(2016, 0.55)])
for year, title, body in [
    (1984, "Prototype", "a bench model"),
    (1991, "First field test", None),
    (1997, "Standard draft", "reviewed by two labs"),
    (2003, "Release 1.0", None),
    (2011, "Large tests", "six full walls"),
    (2017, "Sensors", None),
    (2019, "Image models", "crack detection"),
    (2021, "Automation", None),
    (2023, "Field pilots", "three sites"),
    (2025, "Today", None),
]:
    line.event(year, title, body)
shown = [reveal(line)]
scene.wait(0.6)

# Fixed sides stack their labels in lanes; near the end of the axis a label
# grows to the left, and a reserved span sends the next one to the other side.
scene.segment("sides")
room = stage('line.reserve(2014, 2019, side="above")')
line = scene.viz.timeline(2000, 2026, x_range=(room.left, room.right), y=room.center[1])
for year in (2002, 2004, 2006, 2008):
    line.event(year, f"Step {year - 2000}", "same side", side="above")
line.reserve(2014, 2019, side="above")
line.event(2016, "Reserved above", "so it goes below", color=success)
line.event(2025, "Last", "grows to the left", side="above")
shown.append(reveal(line))
scene.wait(0.6)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    # Each timeline revealed in full.
    scene.snapshots(snapshots, [time + 0.3 for time in shown])
else:
    scene.render()
