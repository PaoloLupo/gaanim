"""A bar chart race: interpolated keyframes, smoothed overtakes and a rolling ticker."""

import os

from gaanim import GRAY, WHITE, Scene


# Visitors per year (thousands); a city missing from a year counts as zero.
frames = {
    2016: {"Lima": 820, "Cusco": 610, "Arequipa": 380, "Puno": 150, "Piura": 240, "Iquitos": 90},
    2017: {"Lima": 860, "Cusco": 700, "Arequipa": 420, "Puno": 260, "Piura": 250, "Iquitos": 130},
    2018: {"Lima": 880, "Cusco": 910, "Arequipa": 470, "Puno": 390, "Piura": 300, "Iquitos": 210},
    2019: {"Lima": 930, "Cusco": 1040, "Arequipa": 520, "Puno": 560, "Piura": 330, "Iquitos": 340},
    2020: {"Lima": 410, "Cusco": 300, "Arequipa": 210, "Puno": 230, "Piura": 190, "Iquitos": 180},
    2021: {"Lima": 640, "Cusco": 520, "Arequipa": 380, "Puno": 330, "Piura": 420, "Iquitos": 260},
    2022: {"Lima": 990, "Cusco": 980, "Arequipa": 610, "Puno": 450, "Piura": 520, "Iquitos": 600},
    2023: {"Lima": 1120, "Cusco": 1210, "Arequipa": 700, "Puno": 520, "Piura": 560, "Iquitos": 790},
}

scene = Scene(frame=(16, 9), background="#0f172a")
scene.text("Visitantes por ciudad (miles)", role="title").fill(WHITE).move_to(0, 3.7)
scene.text("top=5 · rank_smoothing=0.3").fill(GRAY).scale_by(0.5).move_to(0, 3.05)

race = scene.viz.bar_race(
    frames, top=5, rank_smoothing=0.3, value_format="{:,.0f}", width=13, height=5.6,
).move_to(0, -0.6)

scene.play([race.animate.play().duration(14)])
scene.wait(0.6)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    # The first keyframe, Cusco overtaking Lima, the 2020 drop, Iquitos
    # climbing through the ranks, and the final standings.
    scene.snapshots(snapshots, [0.0, 3.5, 8.0, 11.2, 14.4])
else:
    scene.render()
