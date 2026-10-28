"""Graphs and trees: nodes and edges laid out in each layout, drawn with and without direction, then changes of structure.

``scene.viz.graph(nodes, edges)`` places every node with a deterministic
layout and joins them with reactive lines or arrows that follow the nodes.
Each label is the code that builds the graph under it.
"""

import os

from gaanim import Scene, Zones

FADE = 0.6
HOLD = 1.2
MOVE = 1.0

NODES = ["a", "b", "c", "d", "e", "f", "g"]
EDGES = [("a", "b"), ("a", "c"), ("b", "d"), ("b", "e"), ("c", "f"), ("c", "g"), ("e", "f")]
TEAM = ["CEO", "CTO", "CFO", "Dev", "QA", "Ops"]
TEAM_EDGES = [("CEO", "CTO"), ("CEO", "CFO"), ("CTO", "Dev"), ("CTO", "QA"), ("CFO", "Ops")]
REORG = ["CEO", "CTO", "COO", "Dev", "Data", "QA", "Ops"]
REORG_EDGES = [("CEO", "CTO"), ("CEO", "COO"), ("CTO", "Dev"), ("CTO", "Data"), ("COO", "QA"), ("COO", "Ops")]

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


def side_by_side(builds):
    """Two graphs of NODES and EDGES side by side, each under its code; return them."""
    stages = panels([f"graph(NODES, EDGES,\n  {options})" for options in builds], columns=2)
    return [scene.viz.graph(NODES, EDGES, within=stage, **builds[options]) for options, stage in zip(builds, stages)]


# Hierarchies: a tidy tree and Sugiyama layers, both going down.
scene.segment("hierarchies")
graphs = side_by_side({'layout="tree"': {"layout": "tree"}, 'layout="layered"': {"layout": "layered"}})
scene.play([graph.animate.fade_in().duration(FADE) for graph in graphs])
scene.wait(HOLD)

# Networks: a circle from the top, clockwise, and a seeded force layout.
scene.segment("networks")
graphs = side_by_side({'layout="circular"': {"layout": "circular"}, 'layout="force", seed=3': {"layout": "force", "seed": 3}})
scene.play([graph.animate.fade_in().duration(FADE) for graph in graphs])
scene.wait(HOLD)

# directed=False draws lines instead of arrows; labels=False leaves the
# names out, and node_radius sizes the nodes.
scene.segment("options")
graphs = side_by_side({"directed=False": {"directed": False}, "labels=False, node_radius=0.15": {"labels": False, "node_radius": 0.15}})
scene.play([graph.animate.fade_in().duration(FADE) for graph in graphs])
scene.wait(HOLD)

# Every change lays the graph out again: kept nodes move, new parts fade in
# and removed ones fade out, while the edges follow their nodes.
scene.segment("changes")
left, right = panels(['tree.animate.add_node("h", ...)\ntree.animate.remove_node("g")', 'flow.animate.add_edge("d", "g")\nflow.animate.remove_edge("a", "c")'], columns=2)
tree = scene.viz.graph(NODES, EDGES[:-1], layout="tree", within=left)
flow = scene.viz.graph(NODES, EDGES, within=right)
scene.play([tree.animate.fade_in().duration(FADE), flow.animate.fade_in().duration(FADE)])
scene.wait(0.4)
scene.play([tree.animate.add_node("h", edges=[("d", "h")]), flow.animate.add_edge("d", "g")], duration=MOVE)
scene.wait(0.4)
scene.play([tree.animate.remove_node("g"), flow.animate.remove_edge("a", "c")], duration=MOVE)
scene.play([flow.edge("d", "g").animate.fill(scene.colors.success).duration(0.5)])
scene.wait(HOLD)

# animate.to changes the whole structure at once: the nodes both structures
# have move, and the others fade out and in.
scene.segment("restructure")
(stage,) = panels(["org.animate.to(REORG, REORG_EDGES)"], columns=1)
org = scene.viz.graph(TEAM, TEAM_EDGES, layout="tree", within=stage)
scene.play([org.animate.fade_in().duration(FADE)])
scene.wait(0.4)
scene.play([org.animate.to(REORG, REORG_EDGES, duration=MOVE)])
scene.wait(HOLD)

SEGMENT = FADE + HOLD
CHANGES = 3 * SEGMENT + FADE + 0.4
REORG_START = CHANGES + 2 * MOVE + 0.4 + 0.5 + HOLD + FADE + 0.4
SEEKS = [FADE / 2, SEGMENT - 0.2, 2 * SEGMENT - 0.2, 3 * SEGMENT - 0.2]
SEEKS += [CHANGES + MOVE / 2, CHANGES + MOVE + 0.3, CHANGES + 1.5 * MOVE + 0.4, CHANGES + 2 * MOVE + 0.4 + 0.5 + HOLD - 0.2]
SEEKS += [REORG_START + MOVE / 2, REORG_START + MOVE + HOLD - 0.2]

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(snapshots, SEEKS)
else:
    scene.render()
