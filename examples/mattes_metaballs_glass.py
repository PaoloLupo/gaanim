"""Track mattes, metaballs, glass and liquid glass.

Top left, drops of a lava lamp melt into each other as they drift
(`scene.geometry.metaballs`). Top right, stripes show only inside the
letters of a title (an alpha matte) and a gradient sweeping across reveals
a block (a luma matte), with a star cut out of a disc (inverted alpha).
Below, over colored stripes and a caption, a frosted glass card slides
by, two drops of liquid glass melt into one lens that bends and splits
the stripes, and a disc only blurs what is behind it.
The checks of edge cases run before the scene and print their result at
the bottom. Set GAANIM_SNAPSHOTS to capture exact seeks for visual
regression.
"""

import os

from gaanim import BLUE, CORAL, GOLD, WHITE, Brush, Scene, Text

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


def fresh():
    scene = Scene(frame=(16, 9))
    return scene, scene.geometry.circle(1)


def foreign_ball():
    scene, ball = fresh()
    scene.geometry.metaballs([ball, Scene(frame=(16, 9)).geometry.circle(1)])


def foreign_matte():
    _, shape = fresh()
    shape.matte(Scene(frame=(16, 9)).geometry.circle(1))


expect("metaballs sin bolas", ValueError, lambda: fresh()[0].geometry.metaballs([]))
expect("threshold 0", ValueError, lambda: fresh()[0].geometry.metaballs([fresh()[1]], threshold=0))
expect("smoothness negativa", ValueError,
       lambda: (lambda s, b: s.geometry.metaballs([b], smoothness=-1))(*fresh()))
expect("bola de otra escena", ValueError, foreign_ball)
expect("modo de mate desconocido", ValueError,
       lambda: (lambda s, b: b.matte(s.geometry.square(1), "chroma"))(*fresh()))
expect("mate de sí mismo", ValueError, lambda: (lambda s, b: b.matte(b))(*fresh()))
expect("mate de otra escena", ValueError, foreign_matte)
expect("matte(None) lo quita", None,
       lambda: (lambda s, b: b.matte(s.geometry.square(1)).matte(None))(*fresh()))
expect("vidrio con blur negativo", ValueError, lambda: fresh()[1].glass(blur=-0.1))
expect("edge mayor que 1", ValueError, lambda: fresh()[1].glass(edge=1.5))
expect("dispersion mayor que 1", ValueError, lambda: fresh()[1].liquid_glass(dispersion=2))
expect("bevel negativo", ValueError, lambda: fresh()[1].liquid_glass(bevel=-0.1))
expect("blur infinito", ValueError, lambda: fresh()[1].glass(blur=float("inf")))
expect("liquid_glass sobre metaballs", None,
       lambda: (lambda s, b: s.geometry.metaballs([b]).liquid_glass())(*fresh()))
expect("glass sin blur ni refracción", None,
       lambda: fresh()[1].glass(blur=0, refraction=0, edge=0))
expect("un Text sigue siendo Text", None,
       lambda: isinstance(Scene(frame=(16, 9)).text("a").backdrop_blur(0.2).no_glass(), Text) or 1 / 0)

# -- The scene ---------------------------------------------------------------
scene = Scene(frame=(16, 9), background="#0b1020")
label = lambda text, x, y: scene.text(text).fill("#94a3b8").scale_to(0.26).move_to(x, y)

# 1. A lava lamp: drops that melt together as they pass.
drops = [scene.geometry.circle(r).move_to(x, y)
         for x, y, r in ((-6.0, 2.2, 0.55), (-4.6, 3.3, 0.7), (-3.2, 2.0, 0.45), (-5.2, 1.2, 0.4))]
lava = scene.geometry.metaballs(drops, smoothness=0.7).fill(CORAL).stroke(GOLD, 0.03)
label("geometry.metaballs", -4.6, 0.55)

# 2. Stripes inside the letters, a gradient wipe and a star cut out of a disc.
title = scene.text("MATE").fill(WHITE).scale_to(1.6).move_to(1.2, 3.0)
stripes = scene.geometry.group([
    scene.geometry.rect(0.32, 2.0).fill(color).no_stroke().move_to(-1.2 + 0.32 * i, 3.0)
    for i, color in zip(range(16), [GOLD, CORAL, "#22d3ee", "#a78bfa"] * 4)
])
stripes.matte(title)
label('matte(title): "alpha"', 1.2, 2.1)

block = scene.geometry.rect(3.0, 1.2).fill("#22d3ee").no_stroke().move_to(5.6, 3.0)
wipe = scene.geometry.rect(6.0, 1.2).fill(Brush.linear(["#000000", "#ffffff", "#ffffff"], start=(-3, 0), end=(3, 0)))
wipe.move_to(1.1, 3.0)
block.matte(wipe, "luma")
label('"luma": revelado suave', 5.6, 2.1)

disc = scene.geometry.circle(0.7).fill(GOLD).no_stroke().move_to(1.2, 1.0)
star = scene.geometry.star(5, 0.5, 0.22).fill(WHITE).move_to(1.2, 1.0)
disc.matte(star, "alpha_inverted")
label('"alpha_inverted"', 1.2, 0.05)

# 3. Glass over stripes and a caption: frosted, liquid, and a plain blur.
for i in range(14):
    scene.geometry.rect(0.5, 3.6).fill([BLUE, GOLD, CORAL, "#22d3ee"][i % 4]).no_stroke().move_to(-7.15 + 1.1 * i, -2.2)
scene.text("lo que hay detrás").fill(WHITE).scale_to(0.55).move_to(-1.5, -3.1)
card = scene.geometry.rounded_rect(3.6, 2.2, 0.4).fill("#ffffff1a").stroke("#ffffff55", 0.03).move_to(-5.0, -2.0)
card.glass(blur=0.22, refraction=0.14, edge=0.35)
beads = [scene.geometry.circle(0.85).move_to(0.2, -1.9), scene.geometry.circle(0.65).move_to(3.3, -2.3)]
lens = scene.geometry.metaballs(beads, smoothness=0.9).fill("#ffffff10").liquid_glass()
panel = scene.geometry.circle(0.9).fill("#ffffff10").move_to(5.9, -2.0).backdrop_blur(0.3)
label("glass(), liquid_glass() sobre metaballs y backdrop_blur()", 0, -0.15)

ok = sum(passed for _, passed in checks)
failed = [name for name, passed in checks if not passed]
summary = f"validaciones {ok}/{len(checks)}" + ("" if not failed else ": falla " + ", ".join(failed))
status = scene.text(summary).fill("#86efac" if not failed else "#fca5a5").scale_to(0.3).move_to(5.6, -4.1)

# -- Timeline ----------------------------------------------------------------
scene.play([
    drops[0].animate.move_to(-4.4, 2.4).duration(2.0),
    drops[2].animate.move_to(-5.0, 2.9).duration(2.0),
    drops[3].animate.move_to(-3.6, 1.5).duration(2.0),
    wipe.animate.move_to(6.2, 3.0).duration(2.0),
    card.animate.move_to(-2.4, -2.2).duration(2.0),
    beads[1].animate.move_to(1.8, -2.1).duration(2.0),
    title.animate.move_to(1.4, 3.0).duration(2.0),
])
scene.wait(0.5)

snapshot_dir = os.environ.get("GAANIM_SNAPSHOTS")
if snapshot_dir:
    count = scene.snapshots(snapshot_dir, [0.0, 1.0, 2.5])
    print(f"[gaanim-diff] captured {count} exact seeks in {snapshot_dir}")

scene.render()
