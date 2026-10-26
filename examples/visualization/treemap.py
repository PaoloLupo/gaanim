"""Treemaps: rows nested into rectangles whose areas follow their values, then a change of data.

``scene.viz.treemap(data, path=..., value=...)`` lays out one rectangle per
row with the squarified algorithm, nested by the ``path`` columns. Each
label is the code that builds the treemap under it.
"""

import os

from gaanim import Scene, Zones

FADE = 0.6
HOLD = 1.2
MOVE = 1.4

GDP = {
    "region": ["Asia", "Asia", "Asia", "Europe", "Europe", "Europe", "America", "America"],
    "country": ["China", "Japan", "India", "Germany", "France", "Italy", "USA", "Brazil"],
    "gdp": [18.5, 4.2, 3.9, 4.5, 3.1, 2.3, 27.4, 2.2],
}
LATER = dict(GDP, gdp=[21.0, 4.0, 5.6, 4.6, 3.2, 2.3, 29.2, 2.6])

scene = Scene()


def panels(labels):
    """A padded row of stages, each under its code label; return the stages."""
    cells = scene.layout.zones(Zones.columns(len(labels), gap="72px", padding="page_padding"))
    stages = []
    for cell, label in zip(cells, labels):
        strip, stage = cell.split(Zones.rows(["88px", "1fr"], gap="16px"))
        scene.text(label, role="code").place(strip, anchor="bottom")
        stages.append(stage)
    return stages


# One column gives a flat treemap; two nest the countries in their regions.
scene.segment("nesting")
flat, nested = panels(['treemap(GDP, path="country",\n        value="gdp")', 'treemap(GDP, value="gdp",\n        path=["region", "country"])'])
maps = [
    scene.viz.treemap(GDP, path="country", value="gdp", within=flat),
    scene.viz.treemap(GDP, path=["region", "country"], value="gdp", within=nested),
]
scene.play([treemap.animate.fade_in().duration(FADE) for treemap in maps])
scene.wait(HOLD)

# gap and padding space the cells; labels=False leaves the names out.
scene.segment("spacing")
loose, bare = panels(['treemap(..., gap=0.12, padding=0.16)', "treemap(..., labels=False)"])
maps = [
    scene.viz.treemap(GDP, path=["region", "country"], value="gdp", within=loose, gap=0.12, padding=0.16),
    scene.viz.treemap(GDP, path=["region", "country"], value="gdp", within=bare, labels=False),
]
scene.play([treemap.animate.fade_in().duration(FADE) for treemap in maps])
scene.wait(HOLD)

# animate.to keeps every cell whose key both years have: it slides and
# resizes, and the colors stay with their regions.
scene.segment("change")
(stage,) = panels(["treemap.animate.to(LATER)"])
treemap = scene.viz.treemap(GDP, path=["region", "country"], value="gdp", within=stage)
scene.play([treemap.animate.fade_in().duration(FADE)])
scene.wait(0.4)
scene.play([treemap.animate.to(LATER).duration(MOVE)])
scene.wait(HOLD)

SEGMENT = FADE + HOLD
SEEKS = [FADE / 2, SEGMENT - 0.2, SEGMENT + SEGMENT - 0.2]
SEEKS += [2 * SEGMENT + FADE + 0.4 + MOVE / 2, 2 * SEGMENT + FADE + 0.4 + MOVE + HOLD - 0.2]

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(snapshots, SEEKS)
else:
    scene.render()
