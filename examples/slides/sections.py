"""Sections: an agenda, a progress rail and sections of slides that move them.

scene.sections builds navigation for a talk split into sections: an agenda
whose entries are done, current or upcoming, and a progress rail that fills
as the talk moves on. Section and SectionStep open one segment per step and
report each step's progress. Each label is the code that builds or moves
what it describes; the last segments run a small talk with them.
"""

import os

from gaanim import Easing, Scene, Section, SectionStep, TextStyle, Transition, Zones, stagger

scene = Scene()
L = scene.layout
accent = scene.canvas.color("accent")
foreground = scene.canvas.color("foreground")
header = scene.canvas.color("header")
muted = scene.canvas.color("muted")
rule = scene.canvas.color("rule")
success = scene.canvas.color("success")
seeks = []

# A section is a Section, a key used as its own title, or a (key, title) pair.
SECTIONS = [("context", "Context"), ("method", "Method"), ("results", "Results"), ("outlook", "Outlook")]
# Agendas are shown at ZOOM times their size.
ZOOM = 1.4


def code(label):
    return scene.text(label, role="code")


def show(*objects):
    """Fade the objects in one after another and note a seek once they are in."""
    scene.play(stagger(*[item.animate.fade_in().duration(0.4) for item in objects], each=0.06))
    scene.wait(0.3)
    seeks.append(scene.cursor - 0.1)


def steps(zone, changes):
    """For each (label, change): show the label in ``zone``, then play the change."""
    caption = None
    for label, change in changes:
        line = code(label).place(zone).hidden()
        fades = [line.animate.fade_in().duration(0.3)]
        if caption is not None:
            fades.append(caption.animate.fade_out().duration(0.3))
        scene.play(fades)
        start = scene.cursor
        scene.play(change(), duration=0.8)
        # Midway and settled.
        seeks.extend([start + 0.4, scene.cursor + 0.3])
        scene.wait(0.4)
        caption = line


def labelled(zone, label, drawable, anchor="left", top="72px"):
    """The code at the top of ``zone`` and the drawable under it."""
    head, rest = zone.split(Zones.rows([top, "1fr"]))
    return [code(label).place(head, anchor=anchor), drawable.place(rest, anchor=anchor)]


def bar(scene):
    """A marker: a short accent bar."""
    return scene.geometry.rounded_rect(0.06, 0.36, 0.03).fill(accent)


def numbered(scene, entry, state):
    """An agenda item: the entry's number, then its title, in the state's color."""
    color = {"done": muted, "current": accent, "upcoming": rule}[state]
    return L.row(f"{entry.index + 1:02d}", entry.title, gap="16px", color=color)


# An agenda lists the sections; current= marks one, the ones before it are
# done and the rest upcoming. marker= builds a drawable that sits before the
# current entry, item= builds each entry in each state, and styles= restyles
# the default text per state.
scene.segment("agendas")
page = L.zones(Zones.rows(["1fr", "96px"], padding="page_padding"))
cells = page[0].split(Zones.grid(rows=2, columns=2, gap="48px"))
STYLES = {"current": TextStyle(color=success, weight=800), "done": TextStyle(color=muted, italic=True)}
agendas = [
    ('agenda(SECTIONS, "method")', scene.sections.agenda(SECTIONS, "method")),
    ("agenda(..., marker=bar)", scene.sections.agenda(SECTIONS, "method", marker=bar)),
    ("agenda(..., item=numbered)", scene.sections.agenda(SECTIONS, "method", item=numbered)),
    ("agenda(..., styles=STYLES)", scene.sections.agenda(SECTIONS, "method", styles=STYLES)),
]
shown = []
for cell, (label, agenda) in zip(cells, agendas):
    shown += labelled(cell.inset((0, "96px")), label, agenda.root.scale_by(ZOOM))
show(*shown)
# animate.advance() and animate.focus() cross-fade the entries into their new
# states and slide the marker; focus(None) leaves every entry upcoming.
steps(page[1], [
    ("agenda.animate.advance()", lambda: [agenda.animate.advance() for _, agenda in agendas]),
    ('agenda.animate.focus("context")', lambda: [agenda.animate.focus("context") for _, agenda in agendas]),
    ("agenda.animate.focus(None)", lambda: [agenda.animate.focus(None) for _, agenda in agendas]),
])

# direction="row" lines the entries up; pitch= is the distance between their
# left edges, and marker_gap= the marker's distance before the current one.
scene.segment("agenda_row")
page = L.zones(Zones.rows(["1fr", "1fr", "96px"], padding="page_padding"))


def dot(scene):
    """A marker: an accent dot."""
    return scene.geometry.dot(0.06).fill(accent)


row = scene.sections.agenda(SECTIONS, "context", direction="row", pitch=2.4, marker=dot, marker_gap=0.2)
show(*labelled(page[0], 'agenda(SECTIONS, "context", direction="row", pitch=2.4,\n'
                        "       marker=dot, marker_gap=0.2)", row.root.scale_by(ZOOM), anchor="center", top="120px"))
steps(page[2], [
    ('row.animate.focus("results", easing=Easing.SNAPPY)',
     lambda: row.animate.focus("results", easing=Easing.SNAPPY)),
    ("row.animate.advance(-2)", lambda: row.animate.advance(-2)),
])


def rails_segment(name, variants, changes):
    """A segment with one rail per (label, options) row, its code above it, then the changes.

    Each rail is ``scene.sections.progress_rail(SECTIONS, **options)``.
    """
    scene.segment(name)
    page = L.zones(Zones.rows(["1fr", "96px"], padding="page_padding"))
    shown, rails = [], []
    for zone, (label, options) in zip(page[0].split(Zones.rows(len(variants), gap="24px")), variants):
        rail = scene.sections.progress_rail(SECTIONS, **options)
        shown += labelled(zone, label, rail.root)
        rails.append(rail)
    show(*shown)
    steps(page[1], [(label, lambda change=change: [change(rail) for rail in rails]) for label, change in changes])


# A progress rail fills from 0 to 1 (value=). With sections it marks where
# each one starts; segmented=True gives each section its own track, and
# captions=True names them, colored by state. label= titles the rail.
# animate.to() fills it to a fraction, and animate.enter() makes a section
# current with nothing of it filled yet.
rails_segment("rails", [
    ("progress_rail(SECTIONS, value=0.4)", dict(value=0.4)),
    ("progress_rail(SECTIONS, segmented=True, captions=True, value=0.4)",
     dict(segmented=True, captions=True, value=0.4)),
    ('progress_rail(SECTIONS, thickness=0.2, label="Progress", value=0.4)',
     dict(thickness=0.2, label="Progress", value=0.4)),
    ("progress_rail(SECTIONS, fill_color=success, mark_color=foreground)",
     dict(fill_color=success, mark_color=foreground)),
], [
    ("rail.animate.to(0.85)", lambda rail: rail.animate.to(0.85)),
    ('rail.animate.enter("method")', lambda rail: rail.animate.enter("method")),
])


# track=, mark= and caption= build the rail's parts; every track is also the
# mask of its fill. gap= separates the tracks of a segmented rail.
def square(scene, width, height):
    return scene.geometry.rect(width, height).fill(header).stroke(rule, 0.01)


def tick(scene, entry):
    return scene.geometry.line(0, -0.18, 0, 0.18).stroke(foreground, 0.03)


def upper(scene, entry):
    return scene.text(entry.title.upper(), size=0.22, weight=700)


rails_segment("rail_parts", [
    ("progress_rail(SECTIONS, thickness=0.24, track=square, value=0.6)",
     dict(thickness=0.24, track=square, value=0.6)),
    ("progress_rail(SECTIONS, mark=tick, value=0.6)", dict(mark=tick, value=0.6)),
    ("progress_rail(SECTIONS, captions=True, caption=upper, value=0.6)", dict(captions=True, caption=upper, value=0.6)),
    ("progress_rail(SECTIONS, segmented=True, gap=0.5, value=0.6)", dict(segmented=True, gap=0.5, value=0.6)),
], [("rail.animate.to(1.0)", lambda rail: rail.animate.to(1.0))])

# orientation="vertical" stands the rail up and fills it upward; captions sit
# to the right of each section, in caption_style= and caption_colors=.
scene.segment("vertical_rail")
page = L.zones(Zones.rows(["180px", "1fr", "96px"], padding="page_padding"))
CAPTION, COLORS = TextStyle(size=0.36), {"done": foreground}
side = scene.sections.progress_rail(SECTIONS, orientation="vertical", length=5, captions=True, caption_style=CAPTION,
                                    caption_colors=COLORS, thickness=0.16, value=0.3)
show(code('progress_rail(SECTIONS, orientation="vertical", length=5,\n'
          "              captions=True, caption_style=CAPTION,\n"
          "              caption_colors=COLORS, thickness=0.16, value=0.3)").place(page[0]),
     side.root.place(page[1]))
steps(page[2], [("rail.animate.to(0.8)", lambda: side.animate.to(0.8))])


# A Section groups the steps of one part of the talk. build() opens one
# segment per SectionStep, with its transition and notes, calls on_enter with
# the step's SectionProgress and then the step's own build function.
def content(scene):
    scene.wait(0.6)


fade = Transition.cross_fade(0.3)
TALK = [
    Section("context", [SectionStep(name="Problem", build=content, transition=fade, notes="Why it matters.")],
            title="Context"),
    Section("method", [SectionStep(name="Model", build=content, transition=fade),
                       SectionStep(name="Data", build=content, transition=fade, notes="Where the data comes from.")],
            title="Method"),
    Section("results", [SectionStep(name="Findings", build=content, transition=fade)], title="Results"),
]

# A rail pinned to the screen with hud() and kept by persist() stays on
# every segment of the talk.
scene.segment("talk", fade)
frame = L.zones(Zones.rows(["1fr", "120px"], padding="page_padding"))
talk_rail = scene.sections.progress_rail(TALK, segmented=True, captions=True, length=frame[1].width)
talk_rail.root.place(frame[1]).hud()
scene.persist(talk_rail.root)
scene.play(talk_rail.root.animate.fade_in().duration(0.4))


def readout(scene, progress):
    """Each step shows its SectionProgress, then fills the rail to it."""
    lines = [
        ("progress.key", f'"{progress.key}"'),
        ("progress.step.name", f'"{progress.step.name}"'),
        ("progress.index, progress.total", f"{progress.index}, {progress.total}"),
        ("progress.fraction", f"{progress.fraction:.2f}"),
        ("rail.fraction(progress)", f"{talk_rail.fraction(progress):.2f}"),
    ]
    grid = frame[0].split(Zones.grid(rows=len(lines), columns=2, column_gap="48px", padding=("120px", 0)))
    shown = []
    for index, (expression, value) in enumerate(lines):
        shown += [code(expression).place(grid[2 * index], anchor="right"),
                  code(value).fill(accent).place(grid[2 * index + 1], anchor="left")]
    scene.play([stagger(*[item.animate.fade_in().duration(0.3) for item in shown], each=0.04),
                talk_rail.animate.to(progress)], duration=0.6)
    seeks.append(scene.cursor + 0.4)


previous = None
for section in TALK:
    # Each section opens on the agenda, whose marker moves on to it.
    scene.segment(f"agenda · {section.title}", fade)
    agenda = scene.sections.agenda(TALK, previous, marker=bar, item=numbered)
    agenda.root.scale_by(ZOOM).place(frame[0])
    scene.play([agenda.animate.focus(section), talk_rail.animate.enter(section)], duration=0.6)
    scene.wait(0.4)
    seeks.append(scene.cursor - 0.1)
    section.build(scene, on_enter=readout)
    previous = section

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(snapshots, seeks)
else:
    scene.render()
