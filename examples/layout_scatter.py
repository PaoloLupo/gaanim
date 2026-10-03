"""scene.layout.scatter: formulas around a character without covering it.

The left side scatters 24 formulas around the character's parts; the right
side scatters cards in a zone around an avoided zone. The checks of edge
cases run before the scene and print their result at the bottom. Set
GAANIM_SNAPSHOTS to capture exact seeks for visual regression.
"""

import os
from pathlib import Path

from gaanim import GOLD, WHITE, Scene, Zones, stagger

SVG = str(Path(__file__).resolve().parent.parent / "docs/fixtures/assets/personaje.svg")
FORMULAS = [
    "E = m c^2", "a^2 + b^2 = c^2", "e^(i pi) + 1 = 0", "F = m a", "pi r^2",
    "sqrt(2)", "integral_0^1 x dif x", "sum_(n=1)^oo 1/n^2", "d/(d x) sin x",
    "lim_(x -> 0) (sin x)/x", "nabla dot E", "p V = n R T", "Delta x", "x!",
    "log_2 8 = 3", "(a + b)^2", "v = d / t", "phi", "f'(x)", "binom(n, k)",
    "cos^2 + sin^2 = 1", "lambda", "oo", "1 + 1 = 2",
]


def boxes_apart(drawables, gap):
    boxes = [d.bounds() for d in drawables]
    for i, a in enumerate(boxes):
        for b in boxes[i + 1:]:
            dx = max(0.0, max(a.left, b.left) - min(a.right, b.right))
            dy = max(0.0, max(a.bottom, b.bottom) - min(a.top, b.top))
            if (dx * dx + dy * dy) ** 0.5 < gap - 1e-6:
                return False
    return True


# -- Edge cases, each on a fresh scene ---------------------------------------
checks = []


def expect(name, error, operation):
    """Record whether `operation` raises `error`, or succeeds when `error` is None."""
    try:
        operation()
    except Exception as raised:
        checks.append((name, error is not None and isinstance(raised, error)))
    else:
        checks.append((name, error is None))


def fresh(count=6, size=0.6):
    probe = Scene(frame=(16, 9))
    return probe, [probe.geometry.rect(size, size) for _ in range(count)]


def layout_of(seed):
    probe, items = fresh()
    probe.layout.scatter(items, seed=seed)
    return [item.bounds().center for item in items]


def too_large():
    probe = Scene(frame=(16, 9))
    probe.layout.scatter([probe.geometry.rect(30, 1)])


def no_room_moves_nothing():
    probe, items = fresh(count=60, size=1.5)
    before = [item.bounds().center for item in items]
    try:
        probe.layout.scatter(items, gap=0.2)
    except ValueError:
        if [item.bounds().center for item in items] != before:
            raise AssertionError("a failed scatter moved items")
        raise


def foreign_item():
    probe, items = fresh()
    other = Scene(frame=(16, 9))
    probe.layout.scatter(items + [other.geometry.rect(1, 1)])


expect("mismo seed, mismo layout", None, lambda: layout_of(4) == layout_of(4) or 1 / 0)
expect("otro seed, otro layout", None, lambda: layout_of(4) != layout_of(5) or 1 / 0)
expect("item mayor que la región", ValueError, too_large)
expect("sin sitio: error y nada se mueve", ValueError, no_room_moves_nothing)


def scatter_fresh(**options):
    probe, items = fresh()
    probe.layout.scatter(items, **options)


expect("gap negativo", ValueError, lambda: scatter_fresh(gap=-0.1))
expect("item de otra escena", ValueError, foreign_item)
expect("un str no es lista", TypeError, lambda: fresh()[0].layout.scatter("abc"))
expect("lista vacía", None, lambda: fresh()[0].layout.scatter([]))
expect("gap 0 y gap en px", None, lambda: (scatter_fresh(gap=0), scatter_fresh(gap="12px")))

# -- The board ---------------------------------------------------------------
scene = Scene(frame=(16, 9), background="#0f172a")
rows = scene.layout.zones(Zones.rows(["1fr", 0.5], gap=0.1), within=scene.layout.frame.inset(0.3))
board, footer = rows[0], rows[1]
stage, side = board.split(Zones.columns(["2fr", "1fr"], gap=0.4))

# Left: a character, and formulas everywhere except on its parts.
persona = scene.media.svg(SVG).scale_to(2.0).move_to(*stage.center)
parts = [persona.part(name) for name in ("cabeza", "torso", "piernas", "brazo-izq", "brazo-der")]
formulas = [scene.text.equation(f).fill(WHITE).scale_to(0.75) for f in FORMULAS]
scene.layout.scatter(formulas, stage, avoid=parts, gap=0.18, seed=1)

# Right: cards in a zone, around a zone they must leave free.
side_frame = scene.geometry.rect(side.width, side.height).no_fill().stroke("#334155", 0.03).move_to(*side.center)
hole = side.inset(1.2)
hole_frame = scene.geometry.rect(hole.width, hole.height).no_fill().stroke("#f87171", 0.03).move_to(*hole.center)
cards = [
    scene.geometry.rect(0.5 + 0.25 * (i % 3), 0.4).fill(GOLD).no_stroke().opacity(0.85)
    for i in range(10)
]
scene.layout.scatter(cards, side, avoid=[hole], gap=0.12, seed=2)

checks.append(("fórmulas separadas ≥ gap", boxes_apart(formulas, 0.18)))
checks.append(("tarjetas separadas ≥ gap", boxes_apart(cards, 0.12)))

ok = sum(passed for _, passed in checks)
failed = [name for name, passed in checks if not passed]
summary = f"validaciones {ok}/{len(checks)}" + ("" if not failed else ": falla " + ", ".join(failed))
status = (
    scene.text(summary).fill("#86efac" if not failed else "#fca5a5").scale_to(0.34)
    .move_to(stage.center[0], footer.center[1])
)
caption = scene.text("zona roja: avoid=[zona]").fill("#f87171").scale_to(0.3).move_to(side.center[0], footer.center[1])

# -- Timeline ----------------------------------------------------------------
scene.play([persona.animate.fade_in().duration(0.5), side_frame.animate.fade_in().duration(0.5)])
scene.play(stagger(*[f.animate.write().duration(0.8) for f in formulas], each=0.04))
scene.play([hole_frame.animate.create().duration(0.4), caption.animate.fade_in().duration(0.4)])
scene.play(stagger(*[c.animate.fade_in().duration(0.4) for c in cards], each=0.05))
scene.play([status.animate.fade_in().duration(0.3)])
scene.wait(0.5)

snapshot_dir = os.environ.get("GAANIM_SNAPSHOTS")
if snapshot_dir:
    count = scene.snapshots(snapshot_dir, [0.5, 1.4, 3.0, 4.0, 4.6])
    print(f"[gaanim-diff] captured {count} exact seeks in {snapshot_dir}")

scene.render()
