"""Maps: projections, graticules, GeoJSON features, polygons, lines and places.

A map lays longitude and latitude on a rectangle of the scene and draws
ordinary drawables there. The first segment compares the two projections
through their graticules; the next ones label meridians and parallels,
draw countries from a GeoJSON file with a line and places on them, and
cut holes and add islands to polygons that the map's frame clips.
"""

import os

from gaanim import Scene, Zones

DRAW = 1.0
STEP = 0.6
HOLD = 0.8

scene = Scene()
accent = scene.colors.accent
muted = scene.colors.muted
panel = scene.colors.panel
foreground = scene.colors.foreground

# The central Andes, from 1° N to 19° S: the latitudes fix the scale and
# center_lon picks the meridian in the middle of the region.
ANDES = (-19, 1)

# An approximate trace of the Peru-Chile trench, from north to south.
TRENCH = [
    (-81.2, 1.0), (-81.5, -1.5), (-81.8, -3.5), (-82.0, -5.0), (-81.6, -7.0),
    (-80.6, -8.8), (-79.3, -10.7), (-78.2, -12.3), (-76.9, -14.0), (-75.6, -15.6),
    (-73.4, -17.1), (-71.6, -18.4), (-71.2, -19.0),
]
# Each city's label sits on one side of its dot, away from the coast and the
# map's edges.
CITIES = [
    ("Quito", -78.47, -0.18, "left", (0.12, 0)),
    ("Lima", -77.03, -12.05, "right", (-0.12, 0)),
    ("La Paz", -68.15, -16.5, "top", (0, -0.1)),
]


def cells(count, columns=2, bands=("1fr", "5fr")):
    """A padded grid of ``count`` cells, each split into a label band and a body."""
    rows = -(-count // columns)
    grid = scene.layout.zones(
        Zones.grid(rows=rows, columns=columns, column_gap="96px", row_gap="48px", padding="page_padding")
    )
    return [zone.split(Zones.rows(list(bands))) for zone in list(grid)[:count]]


# Mercator keeps the shapes of small areas and stretches them toward the
# poles; equirectangular spaces the parallels evenly. Both use one scale for
# longitude and latitude, so the maps keep their proportions.
scene.segment("projections")
graticules = []
for (top, body), projection in zip(cells(2, bands=("1fr", "8fr")), ["mercator", "equirectangular"]):
    scene.text(f'projection="{projection}"', role="code").place(top, anchor="bottom")
    atlas = scene.viz.map(lat=(-70, 70), lon=(-100, -20), projection=projection, within=body)
    atlas.frame.stroke(muted, 0.01)
    graticules.append(atlas.graticule(10, every=20))
scene.play([graticule.animate.create() for graticule in graticules], duration=DRAW)
scene.wait(HOLD)

# graticule() draws meridians and parallels every step degrees, or a
# (longitude, latitude) pair of steps, and labels those at multiples of
# every on the sides labels names; hemispheres gives the letters.
scene.segment("graticules")
variants = [
    ("graticule()", {}),
    ("graticule(5, every=(10, 5))", {"step": 5, "every": (10, 5)}),
    ('graticule(5, labels="top,right")', {"step": 5, "labels": "top,right"}),
    ('graticule(5, hemispheres="NSEO")', {"step": 5, "hemispheres": "NSEO"}),
]
drawings = []
for (top, body), (label, options) in zip(cells(4), variants):
    scene.text(label, role="code").place(top, anchor="bottom")
    atlas = scene.viz.map(lat=ANDES, center_lon=-72, within=body)
    drawings.append(atlas.geojson("andes.geojson"))
    drawings.append(atlas.graticule(**options))
scene.play([drawing.animate.create() for drawing in drawings], duration=DRAW)
scene.wait(HOLD)

# geojson() draws each feature of a file as a drawable, found by the
# property key= names; polyline() draws a line through longitudes and
# latitudes, and place() puts any object at one.
scene.segment("features")
left, right = scene.layout.zones(Zones.columns(2, gap="72px", padding="page_padding"))
atlas = scene.viz.map(lat=ANDES, center_lon=-75, within=left)
atlas.frame.fill(panel)
grid = atlas.graticule(5, every=(10, 5))
countries = atlas.geojson("andes.geojson", key="name")
trench = atlas.polyline(TRENCH).stroke(accent, 0.03).hidden()
places = []
for name, lon, lat, anchor, offset in CITIES:
    places.append(atlas.place(scene.geometry.dot(0.07).fill(foreground).hidden(), lon, lat))
    label = scene.text(name, role="label").hidden()
    places.append(atlas.place(label, lon, lat, anchor=anchor, offset=offset))
notes = [
    scene.text(text, role="code").place(row, anchor="left").hidden()
    for row, text in zip(
        right.split(Zones.rows(4)),
        [
            'atlas.geojson(file, key="name")',
            'countries["Peru"].fill(accent)',
            "atlas.polyline(TRENCH)",
            'atlas.place(label, lon, lat,\n    anchor="right")',
        ],
    )
]
scene.play([atlas.frame.animate.fade_in(), grid.animate.create(), countries.animate.create(), notes[0].animate.fade_in()], duration=DRAW)
scene.play([countries["Peru"].animate.fill(accent), notes[1].animate.fade_in()], duration=STEP)
scene.play([trench.animate.create(), notes[2].animate.fade_in()], duration=DRAW)
scene.play([place.animate.fade_in() for place in places] + [notes[3].animate.fade_in()], duration=STEP)
scene.wait(HOLD)

# polygon() takes one ring or several: a later ring inside the first cuts a
# hole and one apart from it adds an island. Drawings that reach past the
# frame are clipped to it; no_clip() lets one overflow.
scene.segment("polygons")
# The outline reaches past the maps' western and southern edges.
outline = [(-85, -4), (-70, -4), (-70, -21), (-85, -21)]
lake = [(-78, -12), (-74, -12), (-74, -15), (-78, -15)]
island = [(-67.5, -6), (-64, -6), (-64, -9.5), (-67.5, -9.5)]
shapes = []
for (top, body), (label, clipped) in zip(
    cells(2, bands=("1fr", "6fr")),
    [("polygon([outline, lake, island])", True), ("polygon(...).no_clip()", False)],
):
    scene.text(label, role="code").place(top, anchor="bottom")
    atlas = scene.viz.map(lat=ANDES, center_lon=-72, within=body.inset("48px"))
    atlas.frame.stroke(muted, 0.01)
    shape = atlas.polygon([outline, lake, island]).fill(accent).opacity(0.6)
    shapes.append(shape if clipped else shape.no_clip())
scene.play([shape.animate.create() for shape in shapes], duration=DRAW)
scene.wait(HOLD)

# Each segment once drawn, and the features segment as it builds.
FEATURES = 2 * (DRAW + HOLD)
POLYGONS = FEATURES + 2 * DRAW + 2 * STEP + HOLD
SEEKS = [
    DRAW + 0.4,
    DRAW + HOLD + DRAW + 0.4,
    FEATURES + DRAW + STEP + 0.2,
    FEATURES + 2 * DRAW + 2 * STEP + 0.4,
    POLYGONS + DRAW + 0.4,
]

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(snapshots, SEEKS)
else:
    scene.render()
