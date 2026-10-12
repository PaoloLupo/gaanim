"""Bar race: ranked bars that grow, overtake each other and leave the top, with a rolling ticker.

``scene.viz.bar_race`` takes one ``{name: value}`` table per keyframe and
interpolates between them; the whole race is a function of one keyframe
position, so seeking shows the same frame as playing. The first segment
runs a race, the next ones compare options side by side, and the last one
drives the position by hand.
"""

import os

from gaanim import Scene, Zones

RACE = 8.0
HOLD = 0.6
# The first three segments each run a race and hold.
STEP = RACE + HOLD

scene = Scene()
accent = scene.canvas.color("accent")
success = scene.canvas.color("success")
warning = scene.canvas.color("warning")
danger = scene.canvas.color("danger")
chart = scene.canvas.color("chart")
muted = scene.canvas.color("muted")

# Visitors per year, in thousands; a city missing from a year counts as zero.
FRAMES = {
    2016: {"Lima": 820, "Cusco": 610, "Arequipa": 380, "Puno": 150, "Piura": 240, "Iquitos": 90},
    2017: {"Lima": 860, "Cusco": 700, "Arequipa": 420, "Puno": 260, "Piura": 250, "Iquitos": 130},
    2018: {"Lima": 880, "Cusco": 910, "Arequipa": 470, "Puno": 390, "Piura": 300, "Iquitos": 210},
    2019: {"Lima": 930, "Cusco": 1040, "Arequipa": 520, "Puno": 560, "Piura": 330, "Iquitos": 340},
    2020: {"Lima": 410, "Cusco": 300, "Arequipa": 210, "Puno": 230, "Piura": 190, "Iquitos": 180},
    2021: {"Lima": 640, "Cusco": 520, "Arequipa": 380, "Puno": 330, "Piura": 420, "Iquitos": 260},
    2022: {"Lima": 990, "Cusco": 980, "Arequipa": 610, "Puno": 450, "Piura": 520, "Iquitos": 600},
    2023: {"Lima": 1120, "Cusco": 1210, "Arequipa": 700, "Puno": 520, "Piura": 560, "Iquitos": 790},
}


def labelled(zone, label, **options):
    """A race in ``zone`` below its code label.

    The race's width covers names and bars; the numbers past the longest
    bar take the last eighth of the zone.
    """
    top, body = zone.split(Zones.rows(["2fr", "11fr"], gap="24px"))
    scene.text(label, role="code").place(top, anchor="bottom_left")
    width = body.width * 0.875
    race = scene.viz.bar_race(FRAMES, width=width, height=body.height * 0.95, **options)
    return race.move_to(body.left + width / 2, body.center[1])


def compare(left, right):
    """Run two races side by side; ``left`` and ``right`` are (label, options) pairs."""
    columns = scene.layout.zones(Zones.columns(2, gap="96px", padding="page_padding"))
    races = [labelled(zone, label, **options) for zone, (label, options) in zip(columns, [left, right])]
    scene.play([race.animate.play().duration(RACE) for race in races])
    scene.wait(HOLD)


# top= keeps the leaders, value_format= formats the numbers rolling at the
# end of each bar, and the ticker rolls through the keyframe labels.
scene.segment("race")
frame = scene.layout.zones(Zones.rows(1, padding="page_padding"))[0]
race = labelled(frame, 'scene.viz.bar_race(FRAMES, top=5, value_format="{:,.0f}k")', top=5, value_format="{:,.0f}k")
scene.play([race.animate.play().duration(RACE)])
scene.wait(HOLD)

# rank_smoothing is how many keyframes a swap between two bars takes; with
# 0 they trade places at once. The side-by-side races share smaller text:
# SIDE = dict(top=4, label_width=1.9, font_size=0.36, ticker_size=0.6).
scene.segment("rank_smoothing")
SIDE = {"top": 4, "label_width": 1.9, "font_size": 0.36, "ticker_size": 0.6}
compare(
    ("bar_race(FRAMES, **SIDE,\n         rank_smoothing=0)", {**SIDE, "rank_smoothing": 0}),
    ("bar_race(FRAMES, **SIDE,\n         rank_smoothing=1.5)", {**SIDE, "rank_smoothing": 1.5}),
)

# colors= maps names to colors (or lists colors to cycle); bar_gap= is the
# empty share of each slot and ticker=False hides the keyframe label.
scene.segment("style")
COLORS = {"Lima": accent, "Cusco": success, "Arequipa": warning, "Puno": danger, "Piura": chart, "Iquitos": muted}
compare(
    ("bar_race(FRAMES, **SIDE,\n         colors=COLORS)", {**SIDE, "colors": COLORS}),
    (
        'bar_race(FRAMES, **SIDE,\n         bar_gap=0.5, ticker=False,\n         value_format="${:,.1f}")',
        {**SIDE, "bar_gap": 0.5, "ticker": False, "value_format": "${:,.1f}"},
    ),
)

# animate.to(position) runs to any keyframe position, fractional or
# backwards, and set(position) jumps there. bar(name) is one row (name, bar
# and number) that moves between slots: here it takes the accent.
scene.segment("position")
frame = scene.layout.zones(Zones.rows(["1fr", "8fr"], padding="page_padding"))
race = scene.viz.bar_race(FRAMES, top=6, width=frame[1].width, height=frame[1].height * 0.95)
race.move_to(*frame[1].center)


def highlight():
    race.bar("Iquitos").fill(accent)
    return []


shown = None
for label, action in [
    ("race.animate.to(3)", lambda: [race.animate.to(3).duration(2)]),
    ("race.animate.to(1)", lambda: [race.animate.to(1).duration(1.5)]),
    ('race.bar("Iquitos").fill(accent)', highlight),
    ("race.animate.to(7)", lambda: [race.animate.to(7).duration(3)]),
]:
    text = scene.text(label, role="code").place(frame[0], anchor="bottom_left").hidden()
    swap = [text.animate.fade_in().duration(0.4)] + ([shown.animate.fade_out().duration(0.4)] if shown else [])
    scene.play(swap + action())
    shown = text
scene.wait(HOLD)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    # The race at its start, overtakes, the 2020 drop and its end; the two
    # comparisons mid-race and at the end; the position steps.
    scene.snapshots(
        snapshots,
        [0.2, 2.6, 4.6, 8.2]
        + [STEP * index + t for index in (1, 2) for t in (2.6, 8.2)]
        + [STEP * 3 + t for t in (2.0, 3.5, 4.0, 7.3)],
    )
else:
    scene.render()
