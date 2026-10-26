"""Sankey diagrams: flows between nodes drawn as ribbons as wide as their values, in each alignment, then a change of data.

``scene.viz.sankey(flows)`` reads a source, a target and a value per row,
lays the nodes out in layers and relaxes them so ribbons run as straight as
they can. Each label is the code that builds the diagram under it.
"""

import os

from gaanim import Scene, Zones

FADE = 0.6
HOLD = 1.2
MOVE = 1.4

ENERGY = {
    "source": ["coal", "gas", "solar", "wind", "gas", "power", "power", "power"],
    "target": ["power", "power", "power", "power", "homes", "homes", "industry", "transport"],
    "value": [30, 20, 8, 6, 12, 24, 30, 10],
}
CLEANER = dict(ENERGY, value=[8, 16, 26, 18, 10, 24, 34, 10])

scene = Scene()


def panels(labels, columns):
    """A padded grid of stages, each under its code label; return the stages."""
    rows = (len(labels) + columns - 1) // columns
    cells = scene.layout.zones(Zones.grid(rows=rows, columns=columns, gap="64px", padding="page_padding"))
    stages = []
    for cell, label in zip(cells, labels):
        strip, stage = cell.split(Zones.rows(["84px", "1fr"], gap="12px"))
        scene.text(label, role="code").place(strip, anchor="bottom")
        stages.append(stage)
    return stages


# align decides where nodes without a flow in or out go: "justify" sends the
# nodes no flow leaves to the last layer, "left" keeps every node in the
# layer its longest chain reaches, "right" pushes nodes as far as they go.
scene.segment("alignment")
labels = ['sankey(ENERGY, align="justify")', 'sankey(ENERGY, align="left")', 'sankey(ENERGY, align="right")', "sankey(ENERGY, node_width=0.1,\n       node_gap=0.6,\n       flow_opacity=0.8)"]
stages = panels(labels, columns=2)
diagrams = [
    scene.viz.sankey(ENERGY, within=stages[0], align="justify"),
    scene.viz.sankey(ENERGY, within=stages[1], align="left"),
    scene.viz.sankey(ENERGY, within=stages[2], align="right"),
    scene.viz.sankey(ENERGY, within=stages[3], node_width=0.1, node_gap=0.6, flow_opacity=0.8),
]
scene.play([diagram.animate.fade_in().duration(FADE) for diagram in diagrams])
scene.wait(HOLD)

# animate.to moves every bar, label and ribbon both versions have; the
# colors stay with their nodes.
scene.segment("change")
(stage,) = panels(["diagram.animate.to(CLEANER)"], columns=1)
diagram = scene.viz.sankey(ENERGY, within=stage)
scene.play([diagram.animate.fade_in().duration(FADE)])
scene.wait(0.4)
scene.play([diagram.animate.to(CLEANER).duration(MOVE)])
scene.wait(HOLD)

FIRST = FADE + HOLD
SEEKS = [FADE / 2, FIRST - 0.2, FIRST + FADE + 0.4 + MOVE / 2, FIRST + FADE + 0.4 + MOVE + HOLD - 0.2]

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(snapshots, SEEKS)
else:
    scene.render()
