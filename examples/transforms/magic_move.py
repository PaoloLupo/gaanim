"""Magic move: carry the members two states share from one to the other, by key.

``magic_move(before, after)`` pairs the members of two drawables that share a
key, morphing their place, size, color and shape, and fades the rest out and
in; ``after`` takes over when it ends. Keys are names by default, SVG ids or
whatever a function returns. The last segments reorder a leaderboard and
carry it across a cut with ``Transition.magic_move``. Each label is the call
that moves its band.
"""

import os

from gaanim import Scene, Transition, Zones, magic_move

MOVE = 1.4
HOLD = 0.8

scene = Scene()
accent = scene.canvas.color("accent")
success = scene.canvas.color("success")
warning = scene.canvas.color("warning")
danger = scene.canvas.color("danger")
chart = scene.canvas.color("chart")
rule = scene.canvas.color("rule")
geometry = scene.geometry

SHAPES = {
    "circle": lambda: geometry.circle(0.5),
    "square": lambda: geometry.square(0.9),
    "triangle": lambda: geometry.regular_polygon(3, 0.6),
    "star": lambda: geometry.star(5, 0.6, 0.26),
}


def bands(labels):
    """One band per label across the frame, the label above a wide stage; return the stages."""
    stages = []
    for band, label in zip(scene.layout.zones(Zones.rows(len(labels), gap="24px", padding="page_padding")), labels):
        top, stage = band.split(Zones.rows(["40px", "1fr"]))
        scene.text(label, role="code").place(top, anchor="left")
        stages.append(stage)
    return stages


def halves(stage):
    """The left and right halves of ``stage``: where ``before`` is and where ``after`` is."""
    left, right = stage.split(Zones.columns(2, gap="96px"))
    return left, right


def state(zone, members, prefix="", outline=False):
    """A group of named shapes spread across ``zone``.

    ``members`` are (name, shape, color); each shape is named ``prefix + name``.
    With ``outline`` the shapes are only outlined, to mark where ``after`` is.
    """
    shapes = []
    for cell, (name, kind, color) in zip(zone.split(Zones.columns(len(members))), members):
        shape = SHAPES[kind]().move_to(*cell.center)
        shapes.append((shape.no_fill().stroke(rule, 0.03) if outline else shape.fill(color)).named(prefix + name))
    return geometry.group(shapes)


def states(stage, before, after, prefixes=("", "")):
    """``before`` on the left half of ``stage`` and ``after`` on the right, over its outline."""
    left, right = halves(stage)
    state(right, after, outline=True)
    return state(left, before, prefixes[0]), state(right, after, prefixes[1])


def character(zone, scale=1.6):
    """The character SVG, its parts named by their ids, in the middle of ``zone``."""
    return scene.media.svg("assets/character.svg").scale_to(scale).move_to(*zone.center)


def restyled(zone):
    """The character larger, with its torso and legs recolored and its head raised."""
    person = character(zone, 2.0)
    person.part("torso").fill(success)
    person.part("legs").fill(danger)
    person.part("head").shift_by(0, 0.35)
    return person


def stem(member):
    """The key of a draft or final member: its name without the prefix, or None to look inside."""
    return member.name and member.name.split("-")[1]


BEFORE = [("a", "circle", accent), ("b", "square", success), ("c", "triangle", danger)]
AFTER = [("c", "square", danger), ("a", "star", accent), ("b", "circle", warning)]

# key="name" (the default) pairs members named alike with Drawable.named.
# Names that differ pair nothing, so everything fades; a key function
# returns the key of each member (None looks inside it).
scene.segment("keys")
stages = bands([
    "magic_move(before, after)",
    "magic_move(draft, final)",
    "magic_move(draft, final, key=stem)",
])
before, after = states(stages[0], BEFORE, AFTER)
draft, final = states(stages[1], BEFORE, AFTER, ("draft-", "final-"))
keyed_draft, keyed_final = states(stages[2], BEFORE, AFTER, ("draft-", "final-"))
scene.wait(0.3)
scene.play(
    [
        magic_move(before, after, duration=MOVE),
        magic_move(draft, final, duration=MOVE),
        magic_move(keyed_draft, keyed_final, key=stem, duration=MOVE),
    ]
)
scene.wait(HOLD)

# key="id" pairs the elements of imported SVGs by their id, and each keyed
# member moves as a whole: the parts of a character take their new size,
# place and colors.
scene.segment("svg_ids")
(stage,) = bands(['magic_move(person, dressed, key="id")'])
left, right = halves(stage)
person = character(left)
restyled(right).opacity(0.15)
dressed = restyled(right)
scene.wait(0.3)
scene.play([magic_move(person, dressed, key="id", duration=MOVE)])
scene.wait(HOLD)

# Members without a partner fade out and in during the move, or with
# unmatched="cut" vanish at its start and appear at its end. magic_move
# lasts its own duration (1 s by default), whatever play's default is.
scene.segment("unmatched")
stages = bands([
    'magic_move(before, after, unmatched="fade")',
    'magic_move(before, after, unmatched="cut")',
])
LEAVING = [("a", "circle", accent), ("b", "square", success), ("c", "triangle", danger)]
ENTERING = [("b", "square", success), ("c", "triangle", danger), ("d", "star", warning)]
faded = states(stages[0], LEAVING, ENTERING)
cut = states(stages[1], LEAVING, ENTERING)
scene.wait(0.3)
scene.play([magic_move(*faded, unmatched="fade", duration=MOVE), magic_move(*cut, unmatched="cut", duration=MOVE)])
scene.wait(HOLD)


# A leaderboard: each row is a group named after its player, so a player
# keeps their row while it moves to a new rank and its bar, name and score
# morph. Whoever drops out fades away and a newcomer fades in.
COLORS = {"Ana": accent, "Bo": success, "Cy": danger, "Dee": warning, "Eli": chart}
LEADER = 100


def board(zone, ranking):
    """One named row per (player, score), ranked from the top of ``zone``."""
    rows = []
    for slot, (player, score) in zip(zone.split(Zones.rows(len(ranking), gap="24px")), ranking):
        name_zone, track = slot.split(Zones.columns(["2fr", "11fr"], gap="24px"))
        width = (track.width - 1.2) * score / LEADER
        x, y = track.point("left")
        bar = geometry.rect(width, slot.height * 0.8).fill(COLORS[player]).move_to(x + width / 2, y)
        name = scene.text(player, size=0.6).place(name_zone, anchor="right")
        value = scene.text(str(score), size=0.6).move_to(x + width + 0.6, y)
        rows.append(geometry.group([bar, name, value]).named(player))
    return geometry.group(rows)


def leaderboard(label, ranking):
    """The label above the board of ``ranking``; return the board."""
    top, rest = scene.layout.zones(Zones.rows(["40px", "1fr"], gap="24px", padding="page_padding"))
    scene.text(label, role="code").place(top, anchor="left")
    return board(rest, ranking)


scene.segment("week_1")
week_1 = leaderboard("magic_move(week_1, week_2)", [("Ana", 72), ("Bo", 64), ("Cy", 51), ("Dee", 40)])
# The second state sits in the same place, hidden until the move.
top, rest = scene.layout.zones(Zones.rows(["40px", "1fr"], gap="24px", padding="page_padding"))
week_2 = board(rest, [("Bo", 88), ("Ana", 75), ("Eli", 58), ("Dee", 46)])
scene.wait(0.4)
scene.play([magic_move(week_1, week_2, duration=MOVE)])
scene.wait(HOLD)

# Across a cut, Transition.magic_move pairs the named objects of both
# segments the same way.
scene.segment("week_3", transition=Transition.magic_move(MOVE, key="name"))
leaderboard(
    'scene.segment("week_3", transition=Transition.magic_move(1.4))',
    [("Eli", 91), ("Bo", 90), ("Dee", 70), ("Ana", 69)],
)
scene.wait(1.6)

KEYS_END = 0.3 + MOVE + HOLD
IDS_END = KEYS_END + 0.3 + MOVE + HOLD
UNMATCHED_END = IDS_END + 0.3 + MOVE + HOLD
WEEK_2_END = UNMATCHED_END + 0.4 + MOVE + HOLD
if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    # Each segment before, midway and after its move; the cut midway and settled.
    scene.snapshots(
        snapshots,
        [0.2, 0.3 + MOVE / 2, KEYS_END - 0.1]
        + [KEYS_END + 0.2, KEYS_END + 0.3 + MOVE / 2, IDS_END - 0.1]
        + [UNMATCHED_END - MOVE - HOLD + t for t in (0.35, 0.7, 1.05)] + [UNMATCHED_END - 0.1]
        + [UNMATCHED_END + 0.2, UNMATCHED_END + 0.4 + MOVE / 2, WEEK_2_END - 0.1]
        + [WEEK_2_END + 0.4, WEEK_2_END + 0.8, WEEK_2_END + 1.5],
    )
else:
    scene.render()
