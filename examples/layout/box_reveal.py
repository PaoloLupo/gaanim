"""Box pieces: reveal a box piece by piece, and find, restyle or animate its pieces.

A box's pieces are its backgrounds, texts, rules and other objects at any
depth, in draw order. ``reveal`` brings them in with the usual slide policy,
``cascade`` and ``stagger`` with any animation, and ``find``, ``find_all``
and ``each`` pick pieces to change. Every card is the same box, and each
label is the call that animates it.
"""

import os

from gaanim import Box, BoxStyle, Direction, Scene, Text, Zones, stagger

scene = Scene()
L = scene.layout
accent = scene.colors.accent
header = scene.colors.header
rule = scene.colors.rule
seeks = []

chip = BoxStyle(padding=("2px", "24px"), border=rule, radius="full", font_size="48px")


def columns(count, split):
    """The frame cut into ``count`` rows of (label zone, example zone)."""
    grid = L.zones(Zones.grid(rows=count, columns=list(split), column_gap="48px", padding="page_padding"))
    return [(grid[2 * index], grid[2 * index + 1]) for index in range(count)]


def card(width):
    """A title, an accent rule and a row of chips, on a rounded panel ``width`` wide."""
    return L.column(
        L.box("Title", font_size="64px", weight=700),
        L.box(height="6px", width="fill", background=accent),
        L.row(*[L.box(name, style=chip) for name in ("Boxes", "Rules", "Text")], gap="12px"),
        padding=("12px", "24px"), gap="12px", background=header, border=rule, radius="16px", width=width,
    )


def table(variants, split, visible=False):
    """Lay out one card per (label, animate) row and run every ``animate(card)`` at once.

    Each card is as wide as the example column. The labels fade in first;
    with ``visible`` the cards do too, so the animations start from a
    finished card.
    """
    labels, cards = [], []
    for (label_zone, zone), (label, _) in zip(columns(len(variants), split), variants):
        code = scene.text(label, role="code", text_align="left")
        labels.append(code.place(label_zone, anchor="right"))
        cards.append(card(zone.width).place(zone, anchor="left"))
    shown = labels + (cards if visible else [])
    scene.play(stagger(*[item.animate.fade_in().duration(0.4) for item in shown], each=0.05))
    start = scene.cursor
    animations = [animate(box) for box, (_, animate) in zip(cards, variants)]
    # each() changes the card at once and returns it: there is nothing to play.
    scene.play([item for item in animations if not isinstance(item, Box)])
    scene.wait(0.6)
    # Early, midway and settled.
    seeks.extend([start + 0.3, start + 0.7, scene.cursor - 0.1])


# reveal() fades backgrounds in, slides texts in from direction and grows
# rules (boxes with no children that draw) from rules_from.
scene.segment("reveal")
table([
    ("card.reveal()", lambda card: card.reveal()),
    ("card.reveal(direction=Direction.LEFT)", lambda card: card.reveal(direction=Direction.LEFT)),
    ('card.reveal(rules="fade", each=0.2)', lambda card: card.reveal(rules="fade", each=0.2)),
    ("card.reveal(rules_from=Direction.RIGHT)", lambda card: card.reveal(rules_from=Direction.RIGHT)),
], ("3fr", "2fr"))

# cascade() runs one animation of the .animate proxy on every piece, with
# the timing options of stagger; walk() lists the pieces for a stagger of
# your own.
scene.segment("cascade")
table([
    ("card.cascade().fade_in()", lambda card: card.cascade().fade_in()),
    ("card.cascade(each=0.2).grow_from_center()", lambda card: card.cascade(each=0.2).grow_from_center()),
    ("card.cascade().fade_in_from(Direction.UP)", lambda card: card.cascade().fade_in_from(Direction.UP)),
], ("5fr", "3fr"))

scene.segment("cascade_more")
table([
    ('card.cascade(origin="end").fade_in()', lambda card: card.cascade(origin="end").fade_in()),
    ("stagger(*[p.animate.fade_in()\n          for p in card.walk()])",
     lambda card: stagger(*[piece.animate.fade_in() for piece in card.walk()])),
], ("5fr", "3fr"))

# find() and find_all() pick pieces by type, text or a where= test, each()
# applies an immediate setter to the pieces that match, and stagger() makes
# one animation per piece (None skips it).
scene.segment("pieces")
table([
    ("card.each(lambda p: p.fill(accent), type=Text)",
     lambda card: card.each(lambda piece: piece.fill(accent), type=Text)),
    ('card.find(text="Title").animate.scale_by(1.3)', lambda card: card.find(text="Title").animate.scale_by(1.3)),
    ("card.stagger(lambda p: p.animate.indicate())", lambda card: card.stagger(lambda piece: piece.animate.indicate())),
    ("card.stagger(\n    lambda p: p.animate.indicate()\n    if isinstance(p, Box) else None,\n    boxes=True)",
     lambda card: card.stagger(lambda piece: piece.animate.indicate() if isinstance(piece, Box) else None,
                               boxes=True)),
], ("2fr", "1fr"), visible=True)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(snapshots, seeks)
else:
    scene.render()
