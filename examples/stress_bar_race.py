"""Stress test for scene.viz.bar_race: nine small races probing edge cases.

Every race with five keyframes runs for 8 s, so keyframe k lands at t = 2k and
odd seconds fall halfway between keyframes (mid-overtake). The bottom line
lists which invalid inputs raised as expected; a regression that stops raising
changes that text and therefore the baseline.
"""

import os

from gaanim import GRAY, Scene


scene = Scene(frame=(16, 9), background="#0f172a")

DURATION = 8.0
WIDTH, HEIGHT = 4.6, 2.0
COLUMNS = (-5.3, 0.0, 5.3)
ROWS = (2.6, -0.2, -3.0)
YEARS = (2019, 2020, 2021, 2022, 2023)


def panel(column, row, caption, frames, **options):
    """Place one race in the 3x3 grid with a caption above it."""
    x, y = COLUMNS[column], ROWS[row]
    scene.text(caption, markup=False).fill(GRAY).scale_by(0.4).move_to(x, y + HEIGHT / 2 + 0.25)
    return scene.viz.bar_race(frames, width=WIDTH, height=HEIGHT, **options).move_to(x, y)


def by_year(*rows):
    return dict(zip(YEARS, rows))


races = [
    # 1. Ties: every bar equal at 2019 and 2021, so the listed order must win.
    panel(0, 0, "empates (orden listado)", by_year(
        {"A": 10, "B": 10, "C": 10, "D": 10},
        {"A": 20, "B": 30, "C": 30, "D": 5},
        {"A": 40, "B": 40, "C": 40, "D": 40},
        {"A": 50, "B": 45, "C": 60, "D": 70},
        {"A": 80, "B": 80, "C": 20, "D": 80},
    ), top=4),
    # 2. All-zero keyframes: the full bar length falls back to 1, bars vanish.
    panel(1, 0, "fotogramas todo cero", by_year(
        {"X": 0, "Y": 0, "Z": 0},
        {"X": 5, "Y": 3, "Z": 0},
        {"X": 0, "Y": 0, "Z": 0},
        {"X": 0, "Y": 8, "Z": 8},
        {"X": 9, "Y": 2, "Z": 4},
    ), top=3),
    # 3. A single keyframe with a text label and a tie: play() has nothing to do.
    panel(2, 0, "un solo fotograma", {
        "Único": {"Solo": 42, "Par": 42, "Tercero": 7},
    }, top=3),
    # 4. Names appear and disappear (missing -> 0), 7 names for top=3, text ticker.
    panel(0, 1, "nombres que entran y salen · top=3", {
        "Q1": {"a": 50, "b": 40, "c": 30},
        "Q2": {"b": 60, "d": 55, "e": 20},
        "Q3": {"a": 70, "e": 65, "f": 64, "g": 10},
        "Q4": {"g": 90, "c": 80, "b": 10},
        "Q5": {"d": 100, "f": 99, "a": 98},
    }, top=3),
    # 5. top=1 and instant swaps: only the leader shows, with a tie at 2021.
    panel(1, 1, "top=1 · rank_smoothing=0", by_year(
        {"P": 10, "Q": 9},
        {"P": 9, "Q": 10},
        {"P": 12, "Q": 12},
        {"P": 5, "Q": 20, "R": 30},
        {"P": 2, "Q": 3, "R": 1},
    ), top=1, rank_smoothing=0.0),
    # 6. Maximum smoothing with a zig-zag that crosses at every midpoint.
    panel(2, 1, "rank_smoothing=4 · cruces constantes", by_year(
        {"N1": 40, "N2": 30, "N3": 20, "N4": 10},
        {"N1": 10, "N2": 20, "N3": 30, "N4": 40},
        {"N1": 40, "N2": 30, "N3": 20, "N4": 10},
        {"N1": 10, "N2": 20, "N3": 30, "N4": 40},
        {"N1": 40, "N2": 30, "N3": 20, "N4": 10},
    ), top=4, rank_smoothing=4.0),
    # 7. Negative, tiny and 1e9 values with prefix, suffix and two decimals.
    panel(0, 2, "negativos y 1e9 · prefijo, sufijo y 2 decimales", by_year(
        {"Mega": 1e9, "Neg": -5e8, "Cero": 0, "Micro": 0.004},
        {"Mega": 9.5e8, "Neg": -1e8, "Cero": 0, "Micro": 0.5},
        {"Mega": 2e8, "Neg": 3e8, "Cero": 0, "Micro": 1.25},
        {"Mega": -1e9, "Neg": 6e8, "Cero": 0, "Micro": 999.999},
        {"Mega": 1e9, "Neg": 1e9, "Cero": 0, "Micro": 0},
    ), top=4, value_format="${:,.2f} M"),
    # 8. {:,d}, long names, a wide name column and a partial color dict.
    panel(1, 2, "formato ,d · nombres largos · colores dict", by_year(
        {"Organización Internacional de Pruebas": 1200, "Corto": 900, "Mediano Nombre": 400},
        {"Organización Internacional de Pruebas": 1500, "Corto": 1600, "Mediano Nombre": 800},
        {"Organización Internacional de Pruebas": 1700, "Corto": 1650, "Mediano Nombre": 2100},
        {"Organización Internacional de Pruebas": 3000, "Corto": 1650, "Mediano Nombre": 2900},
        {"Organización Internacional de Pruebas": 3100, "Corto": 12345, "Mediano Nombre": 3000},
    ), top=3, value_format="{:,d}", label_width=2.2,
        colors={"Corto": "#f43f5e", "Mediano Nombre": "#22d3ee"}),
    # 9. A two-color list that repeats, "_" grouping, suffix, no ticker.
    panel(2, 2, "colores que se repiten · sin ticker", by_year(
        {"v": 1000.5, "w": 2000.25, "x": 3000, "y": 4000, "z": 5000},
        {"v": 5000, "w": 4000, "x": 3000, "y": 2000, "z": 1000},
        {"v": 2500, "w": 2500, "x": 2500, "y": 2500, "z": 2500},
        {"v": 12000, "w": 0, "x": 11999.9, "y": 3, "z": 7},
        {"v": 0, "w": 12000, "x": 1, "y": 11999.9, "z": 7},
    ), top=5, rank_smoothing=1.0, value_format="{:_.1f}%",
        colors=["#ef4444", "#22c55e"], ticker=False, label_color="#fbbf24"),
]

# Invalid inputs, checked while the scene is built. None of them spawns anything.
# Names avoid braces and dollars so the summary text stays plain.
good = by_year(*({"a": 1, "b": 2} for _ in YEARS))
invalid = [
    ("fmt e", ValueError, good, {"value_format": "{:e}"}),
    ("fmt .7f", ValueError, good, {"value_format": "{:.7f}"}),
    ("fmt dos campos", ValueError, good, {"value_format": "{}{}"}),
    ("top=0", ValueError, good, {"top": 0}),
    ("vacío", ValueError, {}, {}),
    ("smoothing=5", ValueError, good, {"rank_smoothing": 5.0}),
    ("nan", ValueError, {2020: {"a": float("nan")}}, {}),
    ("1e20", ValueError, {2020: {"a": 1e20}}, {}),
    ("bar_gap=1", ValueError, good, {"bar_gap": 1.0}),
    ("width<0", ValueError, good, {"width": -1.0}),
    ("frames=[1,2]", TypeError, [1, 2], {}),
    ("fila no mapa", TypeError, {2020: [1, 2]}, {}),
]
raised, missed = [], []
for name, error, frames, options in invalid:
    try:
        scene.viz.bar_race(frames, **options)
    except error:
        raised.append(name)
    else:
        missed.append(name)
summary = f"errores esperados {len(raised)}/{len(invalid)}: " + ", ".join(raised)
if missed:
    summary += " · NO LANZARON: " + ", ".join(missed)
scene.text(summary, markup=False).fill(GRAY).scale_by(0.35).move_to(0, -4.3)

scene.play([race.animate.play().duration(DURATION) for race in races])
scene.wait(0.5)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    # Start; a quarter keyframe in; keyframes 1-4 (t = 2, 4, 6, 8); midpoints
    # where panels 4-6 swap ranks (t = 3, 5, 7); a fraction mid-transition
    # (3.37, 7.9); and the settled end after the wait.
    scene.snapshots(snapshots, [0.0, 0.5, 2.0, 3.0, 3.37, 4.0, 5.0, 6.0, 7.0, 7.9, 8.0, 8.5])
else:
    scene.render()
