"""Editorial titles: title cards, section headers, banners and lower thirds from scene.slides.

Title cards open a talk, section headers start a part of it, and banners and
lower thirds anchor themselves to the edges of the safe area. Each component
measures its own text; each label is the code that builds the component
under or beside it.
"""

import os

from gaanim import Scene, Zones, stagger

scene = Scene()
S = scene.slides
accent = scene.canvas.color("accent")
chart = scene.canvas.color("chart")
dark = scene.canvas.color("background")
muted = scene.canvas.color("muted")
success = scene.canvas.color("success")
warning = scene.canvas.color("warning")
seeks = []

TITLE, SUBTITLE = "Vector motion", "A short explanation"


def code(label):
    return scene.text(label, role="code", markup=False)


def show(*objects, hold=1.0):
    """Fade the objects in one after another, hold, and note the seeks."""
    start = scene.cursor
    scene.play(stagger(*[item.animate.fade_in().duration(0.4) for item in objects], each=0.06))
    scene.wait(hold)
    # Mid-entrance and settled.
    seeks.extend([start + 0.3, scene.cursor - 0.1])


def labelled(zone, label, component, zoom=1.0):
    """The code at the top of ``zone`` and the component, ``zoom`` times its size, centered under it."""
    top, rest = zone.split(Zones.rows(["64px", "1fr"]))
    return [code(label).place(top), component.scale_by(zoom).place(rest)]


def column(variants, zoom=1.0):
    """One (label, component) pair per row, the label above its component."""
    rows = scene.layout.zones(Zones.rows(len(variants), gap="24px", padding="page_padding"))
    shown = []
    for zone, (label, component) in zip(rows, variants):
        shown += labelled(zone, label, component, zoom)
    show(*shown)


# A title card centers a title over an accent rule (width= sets the rule's
# length) and an optional subtitle; color= and accent= replace the theme's
# foreground and accent. Title cards are shown at 1.6 times their size.
scene.segment("title_cards")
cells = scene.layout.zones(Zones.grid(rows=2, columns=2, gap="48px", padding="page_padding"))
show(
    *labelled(cells[0], "title_card(TITLE, SUBTITLE)", S.title_card(TITLE, SUBTITLE), zoom=1.6),
    *labelled(cells[1], "title_card(..., width=4)", S.title_card(TITLE, SUBTITLE, width=4), zoom=1.6),
    *labelled(cells[2], "title_card(..., accent=success)", S.title_card(TITLE, SUBTITLE, accent=success), zoom=1.6),
    *labelled(cells[3], "title_card(..., color=muted)", S.title_card(TITLE, SUBTITLE, color=muted), zoom=1.6),
)

# A section header stacks an optional kicker, the title and an optional
# subtitle on a panel; rule=True underlines them. Headers are shown at 1.2
# times their size.
HEADER = dict(kicker="02", subtitle="How we measured")
scene.segment("section_headers")
column([
    ('section_header("Method")', S.section_header("Method")),
    ('section_header("Method", kicker="02", subtitle="How we measured")',
     S.section_header("Method", **HEADER)),
    ("section_header(..., rule=True)", S.section_header("Method", **HEADER, rule=True)),
], zoom=1.2)

# align= aligns every slot, and width=, padding=, gap= and radius= size the
# panel.
scene.segment("header_layout")
column([
    ('section_header(..., align="center")', S.section_header("Method", **HEADER, align="center")),
    ('section_header(..., align="right")', S.section_header("Method", **HEADER, align="right")),
    ("section_header(..., width=5, padding=(0.5, 0.3), gap=0.2, radius=0.4)",
     S.section_header("Method", **HEADER, width=5, padding=(0.5, 0.3), gap=0.2, radius=0.4)),
], zoom=1.2)

# Like every editorial component, headers take a variant and an
# appearance, or explicit colors.
scene.segment("header_styles")
column([
    ('section_header(..., variant="accent")', S.section_header("Method", **HEADER, variant="accent")),
    ('section_header(..., variant="success", appearance="solid")',
     S.section_header("Method", **HEADER, variant="success", appearance="solid")),
    ("section_header(..., color=accent, background=dark, border=chart)",
     S.section_header("Method", **HEADER, color=accent, background=dark, border=chart)),
], zoom=1.2)


def edges(top, bottom):
    """A banner at each safe edge, each with its code on the side facing the middle."""
    (top_label, top_banner), (bottom_label, bottom_banner) = top, bottom
    frame = scene.layout.frame
    below = frame.inset((frame.top - top_banner.bounds().bottom + 0.3, 0, 0, 0))
    above = frame.inset((0, 0, bottom_banner.bounds().top - frame.bottom + 0.3, 0))
    show(top_banner, code(top_label).place(below, anchor="top"),
         bottom_banner, code(bottom_label).place(above, anchor="bottom"))


# A banner anchors itself to the top or bottom of the safe area; without
# width= it spans it, less margin= on each side, and its height follows its
# text.
scene.segment("banners")
edges(
    ("banner(TITLE, SUBTITLE)", S.banner(TITLE, SUBTITLE)),
    ('banner(TITLE, position="bottom", variant="success", appearance="solid")',
     S.banner(TITLE, position="bottom", variant="success", appearance="solid")),
)

scene.segment("banner_sizes")
edges(
    ('banner(TITLE, width=8, padding=(0.6, 0.3), variant="accent")',
     S.banner(TITLE, width=8, padding=(0.6, 0.3), variant="accent")),
    ('banner(TITLE, SUBTITLE, position="bottom", margin=1.5, radius=0)',
     S.banner(TITLE, SUBTITLE, position="bottom", margin=1.5, radius=0)),
)


def corners(left, right):
    """A lower third in each bottom corner, with the code of each above them on its side."""
    (left_label, left_third), (right_label, right_third) = left, right
    frame = scene.layout.frame
    a, b = left_third.bounds(), right_third.bounds()
    # Two lines above the taller lower third, from the outer edge of one to the other's.
    band = frame.inset((0, frame.right - b.right, max(a.top, b.top) - frame.bottom + 0.3, a.left - frame.left))
    lines = band.split(Zones.rows(["1fr", "72px", "72px"]))
    show(left_third, code(left_label).place(lines[2], anchor="left"),
         right_third, code(right_label).place(lines[1], anchor="right"))


# A lower third anchors itself to a bottom corner (side=) with an optional
# kicker over its title and subtitle.
NAME, ROLE = "Ada Lovelace", "Mathematician and writer"
scene.segment("lower_thirds")
corners(
    ('lower_third(NAME, ROLE, kicker="SPEAKER")', S.lower_third(NAME, ROLE, kicker="SPEAKER")),
    ('lower_third(NAME, side="right", variant="accent")', S.lower_third(NAME, side="right", variant="accent")),
)

scene.segment("lower_third_styles")
corners(
    ('lower_third(NAME, ROLE, width=4, margin=0.8)', S.lower_third(NAME, ROLE, width=4, margin=0.8)),
    ('lower_third(NAME, side="right", variant="accent", appearance="solid")',
     S.lower_third(NAME, side="right", variant="accent", appearance="solid")),
)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(snapshots, seeks)
else:
    scene.render()
