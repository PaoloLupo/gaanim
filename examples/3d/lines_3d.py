"""3D lines: polylines through 3D points and paths traced behind moving objects.

``scene.geometry.polyline_3d`` draws a line strip through world points in
one color, one color per point or a colormap; ``traced_path_3d`` records
where a moving drawable has been. A distant perspective camera looks at
every line from the front right with z pointing up.
"""

import math
import os

from gaanim import ColorMap, Material3D, Scene, Zones

DRAW = 1.4
HOLD = 0.8
TRACE = 3.0

scene = Scene()
accent = scene.canvas.color("accent")
danger = scene.canvas.color("danger")
foreground = scene.canvas.color("foreground")

# The camera looks at the origin from AZIMUTH radians around z and ELEVATION
# radians above the floor, with z up. From DISTANCE away, a field of view as
# tall as the frame draws the plane through the origin that faces the camera
# at the layout's scale, so on_screen() finds the 3D point under a zone.
AZIMUTH, ELEVATION, DISTANCE = 0.65, 0.45, 40
RIGHT = (math.cos(AZIMUTH), math.sin(AZIMUTH), 0)
ABOVE = (-math.sin(AZIMUTH) * math.sin(ELEVATION), math.cos(AZIMUTH) * math.sin(ELEVATION), math.cos(ELEVATION))
TOWARD_EYE = (math.sin(AZIMUTH) * math.cos(ELEVATION), -math.cos(AZIMUTH) * math.cos(ELEVATION), math.sin(ELEVATION))
frame = scene.layout.zones(Zones.rows(1))[0]
scene.camera.perspective(fov_y=2 * math.atan(frame.height / 2 / DISTANCE))
scene.camera.look_at(eye=tuple(DISTANCE * c for c in TOWARD_EYE), target=(0, 0, 0), up=(0, 0, 1))


def on_screen(zone):
    """The 3D point the camera draws at the center of ``zone``."""
    x, y = zone.center
    return tuple(x * right + y * above for right, above in zip(RIGHT, ABOVE))


def helix(center, radius, height, t):
    """The point at ``t`` (0 to 1) of a three-turn helix around ``center``, rising along z."""
    cx, cy, cz = center
    angle = 3 * math.tau * t
    return cx + radius * math.cos(angle), cy + radius * math.sin(angle), cz + height * (t - 0.5)


def columns(variants, build):
    """One column per (label, value) variant, its label below; ``build(value, zone)`` draws in it."""
    zones = scene.layout.zones(Zones.columns(len(variants), gap="48px", padding="page_padding"))
    drawn = []
    for zone, (label, value) in zip(zones, variants):
        art, caption = zone.split(Zones.rows(["1fr", 0.5]))
        scene.text(label, role="code").place(caption)
        drawn.append(build(value, art))
    return drawn


# Each helix is a list of 3D points: points = [helix(center, ...) for t in ...].
def strip(zone):
    center = on_screen(zone)
    return [helix(center, zone.width * 0.3, zone.height * 0.75, k / 120) for k in range(121)]


# A uniform color, or one color per point: here a ramp from a ColorMap.
scene.segment("colors")
lines = columns(
    [
        ("polyline_3d(points, accent)", lambda points: scene.geometry.polyline_3d(points, accent)),
        (
            "polyline_3d(points, colors=ramp)",
            lambda points: scene.geometry.polyline_3d(
                points, colors=ColorMap.from_colors([accent, danger]).colors(len(points))
            ),
        ),
    ],
    lambda line, zone: line(strip(zone)),
)
scene.play([line.animate.create() for line in lines], duration=DRAW)
scene.wait(HOLD)

# Or a colormap from the start of the line to its end:
# scene.geometry.polyline_3d(points, colormap="inferno").
scene.segment("colormaps")
lines = columns(
    [('colormap="inferno"', "inferno"), ('colormap="viridis"', "viridis"), ('colormap="plasma"', "plasma")],
    lambda colormap, zone: scene.geometry.polyline_3d(strip(zone), colormap=colormap),
)
scene.play([line.animate.create() for line in lines], duration=DRAW)
scene.wait(HOLD)


# traced_path_3d follows a drawable's 3D position as it moves, here a small
# sphere that an updater drives along a helix: each label is the options of
# scene.geometry.traced_path_3d(pen, ...).
def traced(options, zone):
    center = on_screen(zone)
    pen = scene.geometry.sphere(0.12, material=Material3D.emissive(foreground)).move_to_3d(
        *helix(center, zone.width * 0.3, zone.height * 0.75, 0)
    )
    pen.add_updater_fn(lambda position, dt, elapsed: helix(center, zone.width * 0.3, zone.height * 0.75, elapsed / TRACE))
    return scene.geometry.traced_path_3d(pen, **options)


scene.segment("traces")
trails = columns(
    [
        ("traced_path_3d(pen)", {}),
        ('colormap="viridis"', {"colormap": "viridis"}),
        ('colormap="plasma"', {"colormap": "plasma"}),
    ],
    traced,
)
scene.play([trail.animate.fade_in() for trail in trails], duration=0.1)
scene.wait(TRACE)

# Samples older than dissipating_time seconds fade away, max_points keeps
# only the newest samples, and min_distance skips samples closer than that
# to the last one.
scene.segment("samples")
trails = columns(
    [
        ("dissipating_time=0.8", {"dissipating_time": 0.8}),
        ("max_points=40", {"max_points": 40}),
        ("min_distance=0.6", {"min_distance": 0.6}),
    ],
    traced,
)
scene.play([trail.animate.fade_in() for trail in trails], duration=0.1)
scene.wait(TRACE)

segment = DRAW + HOLD
trace = 0.1 + TRACE
seeks = [0.7, segment - 0.2, segment + 0.7, 2 * segment - 0.2]
seeks += [2 * segment + t for t in (1.2, trace - 0.2)] + [2 * segment + trace + t for t in (1.2, trace - 0.2)]
if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(snapshots, seeks)
else:
    scene.render()
