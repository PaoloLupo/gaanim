"""A leaderboard reordered with a keyed magic move, then carried into the next
segment by a keyed magic-move transition.

Each row is a group named after its player. Rows with the same name move to
their new rank and morph their bar, label and score; a player who drops out
fades away and a newcomer fades in.
"""

import os

from gaanim import BLACK, BLUE, CORAL, CYAN, GOLD, GREEN, WHITE, Scene, Transition, magic_move


scene = Scene(frame=(16, 9), background=BLACK)
COLORS = {"Ana": BLUE, "Bo": GOLD, "Cy": CORAL, "Dee": GREEN, "Eli": CYAN}


def board(rows, top=2.0):
    """One named row group per player, ranked from the top."""
    groups = []
    for rank, (player, score) in enumerate(rows):
        y = top - 1.1 * rank
        width = score / 10
        bar = scene.geometry.rect(width, 0.7).fill(COLORS[player]).move_to(-3.5 + width / 2, y)
        label = scene.text(player).fill(WHITE).scale_to(0.6).move_to(-5.2, y)
        value = scene.text(str(score)).fill(WHITE).scale_to(0.6).move_to(-3.0 + width, y)
        groups.append(scene.geometry.group([bar, label, value]).named(player))
    return scene.geometry.group(groups)


scene.segment("Semana 1")
title = scene.text("Clasificación", role="title").fill(WHITE).move_to(0, 3.4)
week_1 = board([("Ana", 72), ("Bo", 64), ("Cy", 51), ("Dee", 40)])
week_2 = board([("Bo", 88), ("Ana", 75), ("Eli", 58), ("Dee", 46)])

scene.play([title.animate.fade_in().duration(0.4)])
scene.wait(0.6)
# Bo overtakes Ana, Cy drops out and Eli enters; keys are the row names.
scene.play([magic_move(week_1, week_2, key="name", unmatched="fade").duration(1.0)])
scene.wait(0.8)

# Across the cut, rows with the same name travel to their new place.
scene.segment("Semana 3", transition=Transition.magic_move(0.8, key="name"))
board([("Eli", 91), ("Bo", 90), ("Dee", 70), ("Ana", 69)])
scene.wait(1.2)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    # Before the move, mid-move, reordered, mid-transition, and week 3.
    scene.snapshots(snapshots, [0.8, 1.5, 2.4, 3.2, 4.4])
else:
    scene.render()
