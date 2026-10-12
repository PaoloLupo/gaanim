"""Entrances and exits: every built-in way to bring a drawable in and take it out.

Each row plays one animation of ``drawable.animate`` and is labelled with the
call. Entrances keep the drawable hidden until they start; exits leave it
hidden when they end. One segment per family: fades, strokes, growth, exits.
"""

import os

from gaanim import Direction, Scene, Zones

DURATION = 1.2
HOLD = 0.5

scene = Scene()
foreground = scene.canvas.color("foreground")
rule = scene.canvas.color("rule")


def card(zone):
    """An outlined card in the middle of ``zone``."""
    return scene.geometry.rounded_rect(1.8, 0.56, 0.1).stroke(foreground, 0.03).move_to(*zone.center)


def star(zone):
    """An outlined star in the middle of ``zone``."""
    return scene.geometry.star(5, 0.36, 0.16).stroke(foreground, 0.03).move_to(*zone.center)


def word(zone):
    """A word of text in the middle of ``zone``."""
    return scene.text("Hello").move_to(*zone.center)


def arrow(zone):
    """An arrow that bends across the middle of ``zone``."""
    (x, y), half = zone.center, 1.2
    return scene.geometry.curved_arrow(x - half, y - 0.15, x + half, y - 0.15, 0.8)


def trio(zone):
    """Three dots grouped in the middle of ``zone``."""
    x, y = zone.center
    return scene.geometry.group([scene.geometry.circle(0.2).move_to(x + dx, y) for dx in (-0.7, 0, 0.7)])


def rows(count):
    """``count`` rows of a label column and a stage column, padded from the frame."""
    return scene.layout.zones(Zones.grid(rows=count, columns=["4fr", "3fr"], column_gap="48px", padding="page_padding"))


def table(entries, hold_first=False):
    """Play one animation per row, its code to the left of the drawable it animates.

    ``entries`` are (label, make, animation): ``make(zone)`` builds the drawable
    in the row's stage and ``animation(drawable.animate)`` returns its
    animation. With ``hold_first`` the drawables show for a moment first, as exits need.
    """
    grid = rows(len(entries))
    moves = []
    for index, (label, make, animation) in enumerate(entries):
        scene.text(label, role="code").place(grid[2 * index], anchor="right")
        moves.append(animation(make(grid[2 * index + 1]).animate).duration(DURATION))
    if hold_first:
        scene.wait(HOLD)
    scene.play(moves)
    scene.wait(HOLD)


# Fades change opacity; fade_in_from also slides in from a side, 0.48 units by default.
scene.segment("fades")
table([
    ("fade_in()", card, lambda a: a.fade_in()),
    ("fade_in_from(Direction.UP)", card, lambda a: a.fade_in_from(Direction.UP)),
    ("fade_in_from(Direction.DOWN)", card, lambda a: a.fade_in_from(Direction.DOWN)),
    ("fade_in_from(Direction.LEFT)", card, lambda a: a.fade_in_from(Direction.LEFT)),
    ("fade_in_from(Direction.RIGHT)", card, lambda a: a.fade_in_from(Direction.RIGHT)),
    ("fade_in_from(Direction.UP_LEFT)", card, lambda a: a.fade_in_from(Direction.UP_LEFT)),
    ("fade_in_from(Direction.DOWN_RIGHT)", card, lambda a: a.fade_in_from(Direction.DOWN_RIGHT)),
    ("fade_in_from(Direction.LEFT, 2.0)", card, lambda a: a.fade_in_from(Direction.LEFT, 2.0)),
])

# Stroke entrances trace the outline, then fill it. stroke_width sets the
# traced width, with_pen_tip rides a dot on the drawing end, and lag_ratio
# staggers the members of a group.
scene.segment("strokes")
table([
    ("create()", star, lambda a: a.create()),
    ("create().stroke_width(0.08)", star, lambda a: a.create().stroke_width(0.08)),
    ("create().with_pen_tip()", star, lambda a: a.create().with_pen_tip()),
    ("draw_border_then_fill()", star, lambda a: a.draw_border_then_fill()),
    ("write()", word, lambda a: a.write()),
    ('write(brush="chalk", seed=3)', word, lambda a: a.write(brush="chalk", seed=3)),
    ("create().lag_ratio(0.0)", trio, lambda a: a.create().lag_ratio(0.0)),
    ("create().lag_ratio(1.0)", trio, lambda a: a.create().lag_ratio(1.0)),
])

# Growth scales up from zero about a fixed point: the center, a scene point,
# an edge or corner of the bounds, or an arrow's tail.
scene.segment("growth")
# The pinned point of grow_from_point: the left end of its row's stage.
px, py = rows(7)[5].point("left")
scene.geometry.dot(0.05).fill(rule).move_to(px, py)
table([
    ("grow_from_center()", card, lambda a: a.grow_from_center()),
    ("spin_in_from_nothing()", star, lambda a: a.spin_in_from_nothing()),
    (f"grow_from_point({px:.2f}, {py:.2f})", card, lambda a: a.grow_from_point(px, py)),
    ("grow_from_edge(Direction.DOWN)", card, lambda a: a.grow_from_edge(Direction.DOWN)),
    ("grow_from_edge(Direction.LEFT)", card, lambda a: a.grow_from_edge(Direction.LEFT)),
    ("grow_from_edge(Direction.UP_RIGHT)", card, lambda a: a.grow_from_edge(Direction.UP_RIGHT)),
    ("grow_arrow()", arrow, lambda a: a.grow_arrow()),
])

# Exits leave the drawable hidden: fade_out, the reverse of create and write,
# and a shrink to the center.
scene.segment("exits")
table([
    ("fade_out()", card, lambda a: a.fade_out()),
    ("uncreate()", star, lambda a: a.uncreate()),
    ("uncreate().stroke_width(0.08)", star, lambda a: a.uncreate().stroke_width(0.08)),
    ("unwrite()", word, lambda a: a.unwrite()),
    ("shrink_to_center()", card, lambda a: a.shrink_to_center()),
], hold_first=True)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    # Early, mid-way and settled in the three entrance segments, then the
    # exits held, mid-way and gone.
    scene.snapshots(
        snapshots,
        [(DURATION + HOLD) * index + t for index in range(3) for t in (0.3, 0.7, 1.4)]
        + [3 * (DURATION + HOLD) + t for t in (0.3, 1.0, 1.4, 2.0)],
    )
else:
    scene.render()
