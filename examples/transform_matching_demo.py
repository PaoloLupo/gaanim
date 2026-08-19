"""Example: TransformMatching — improved over Manim's TransformMatchingShapes/Tex.

Demonstrates:
  - shape auto-matching (geometry + position, Hungarian + shape hash)
  - tex auto-matching (character key + LCS order preservation)
  - handling of mismatched counts (fade for surplus)
"""

import os

from gaanim import BLACK, BLUE, GOLD, GREEN, RED, WHITE, Scene, Transition

scene = Scene(1920, 1080, background=BLACK)

# Segment 1: shape matching with mismatched counts and duplicates
scene.segment("shape_matching")
title = scene.text("TransformMatchingShapes (mejorado)", role="title").fill(WHITE).at(0, 420)
subtitle = scene.text("Geometría + posición + color · Hungarian + shape-hash · sobrantes Fade").fill(WHITE).at(0, 350).scaled(0.55)
title.write().duration(0.8).smooth()
subtitle.write().duration(0.8).smooth()

# Source: 4 shapes at left — includes duplicate squares to test hash handling
c1 = scene.circle(55.0).fill(BLUE).stroke(WHITE, 3.0).at(-420, 120)
s1 = scene.square(90.0).fill(RED).stroke(WHITE, 3.0).at(-260, 120)
s2 = scene.square(90.0).fill(GOLD).stroke(WHITE, 3.0).at(-100, 120)
tri = scene.regular_polygon(3, 60).fill(GREEN).stroke(WHITE, 3.0).at(-260, -40)
src_group = scene.group([c1, s1, s2, tri])
src_group.create().duration(0.9).smooth()
scene.wait(0.3)

# Target: 5 shapes at right — reordered + extra star (unmatched)
c2 = scene.circle(55.0).fill(GOLD).stroke(WHITE, 3.0).at(260, 120)
s3 = scene.square(90.0).fill(BLUE).stroke(WHITE, 3.0).at(420, 120)
star = scene.star(5, 60, 30).fill(RED).stroke(WHITE, 3.0).at(420, -40)
tri2 = scene.regular_polygon(3, 60).fill(RED).stroke(WHITE, 3.0).at(100, 120)
rect = scene.rect(110, 70).fill(WHITE).stroke(WHITE, 3.0).at(260, -40)
dst_group = scene.group([tri2, s3, c2, rect, star])

# Auto-match by shape: squares should pair despite reordering, star/rect fade in, one gold square fades out
scene.transform_matching_shapes(src_group, dst_group, duration=2.2)
scene.wait(0.8)

# Segment 2: tex matching — order-preserving LCS
scene.segment("tex_matching", Transition.cross_fade(0.4))
scene.wait(0.2)
caption = scene.text("TransformMatchingTex: LCS + geometría · preserva orden").fill(WHITE).at(0, 420).scaled(0.62)
caption.fade_in().duration(0.6).smooth()
scene.wait(0.2)

src_text = scene.text("ABCD", role="title").fill(BLUE).at(0, 80).scaled(1.2)
src_text.write().duration(0.7).smooth()
scene.wait(0.3)

dst_text = scene.text("BADC", role="title").fill(GREEN).at(0, 80).scaled(1.2)
# Scrambled order BADC shares all letters but order differs — LCS should keep longest order-preserving subset (mismo centro)
scene.play([src_text.morph_to(dst_text, duration=2.0)])
scene.wait(0.8)

# Segment 3: equation tex matching — real math
scene.segment("equation_matching", Transition.cross_fade(0.4))
scene.wait(0.2)
eq_title = scene.text("Ecuaciones: mantiene 'm' y reordena, sobrantes Fade").fill(WHITE).at(0, 380).scaled(0.6)
eq_title.fade_in().duration(0.5).smooth()

e1 = scene.equation("E = m c").fill(WHITE).at(0, 80).scaled(1.3)
e1.write().duration(0.7).smooth()
scene.wait(0.3)

e2 = scene.equation("p = m v^2").fill(GOLD).at(0, 80).scaled(1.3)
# Tex matching on equations: 'm' matches by key, other glyphs fade (mismo y, no debajo)
scene.play([e1.morph_to(e2, duration=1.6)])
scene.wait(1.0)

finale = scene.text("Mejoras: winding preservado, spring-continuo, coste combinado, greedy >64", role="subtitle").fill(GREEN).at(0, -420).scaled(0.9)
finale.write().duration(0.8).smooth()
scene.wait(1.0)

# Snapshots for visual regression (gaanim --diff)
snapshot_dir = os.environ.get("GAANIM_SNAPSHOTS")
if snapshot_dir:
    count = scene.snapshots(
        snapshot_dir,
        [
            0.0,
            1.2,
            2.4,
            3.8,
            5.0,
            6.2,
            7.0,
            8.3,
            9.2,
            11.0,
            12.5,
            14.0,
        ],
    )
    print(f"[gaanim-diff] captured {count} exact seeks in {snapshot_dir}")

if __name__ == "__main__":
    scene.render()
