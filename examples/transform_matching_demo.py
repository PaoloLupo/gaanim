"""Example: TransformMatching — improved over Manim's TransformMatchingShapes/Tex.

Demonstrates:
  - shape auto-matching (geometry + position, Hungarian + shape hash)
  - tex auto-matching (character key + LCS order preservation)
  - handling of mismatched counts (fade for surplus)
"""

import os

from gaanim import Easing, BLACK, BLUE, GOLD, GREEN, RED, WHITE, Scene, Transition

scene = Scene(frame=(16, 9), background=BLACK)

# Segment 1: shape matching with mismatched counts and duplicates
scene.segment("shape_matching")
title = scene.text("TransformMatchingShapes (mejorado)", role="title").fill(WHITE).move_to(0, 3.5)
subtitle = scene.text("Geometría + posición + color · Hungarian + shape-hash · sobrantes Fade").fill(WHITE).move_to(0, 2.916667).scale_to(0.55)
scene.play([title.animate.write().duration(0.8).easing(Easing.SMOOTH)])
scene.play([subtitle.animate.write().duration(0.8).easing(Easing.SMOOTH)])

# Source: 4 shapes at left — includes duplicate squares to test hash handling
c1 = scene.geometry.circle(0.458333).fill(BLUE).stroke(WHITE, 0.025).move_to(-3.5, 1)
s1 = scene.geometry.square(0.75).fill(RED).stroke(WHITE, 0.025).move_to(-2.166667, 1)
s2 = scene.geometry.square(0.75).fill(GOLD).stroke(WHITE, 0.025).move_to(-0.833333, 1)
tri = scene.geometry.regular_polygon(3, 0.5).fill(GREEN).stroke(WHITE, 0.025).move_to(-2.166667, -0.333333)
src_group = scene.geometry.group([c1, s1, s2, tri])
scene.play([src_group.animate.create().duration(0.9).easing(Easing.SMOOTH)])
scene.wait(0.3)

# Target: 5 shapes at right — reordered + extra star (unmatched)
c2 = scene.geometry.circle(0.458333).fill(GOLD).stroke(WHITE, 0.025).move_to(2.166667, 1)
s3 = scene.geometry.square(0.75).fill(BLUE).stroke(WHITE, 0.025).move_to(3.5, 1)
star = scene.geometry.star(5, 0.5, 0.25).fill(RED).stroke(WHITE, 0.025).move_to(3.5, -0.333333)
tri2 = scene.geometry.regular_polygon(3, 0.5).fill(RED).stroke(WHITE, 0.025).move_to(0.833333, 1)
rect = scene.geometry.rect(0.916667, 0.583333).fill(WHITE).stroke(WHITE, 0.025).move_to(2.166667, -0.333333)
dst_group = scene.geometry.group([tri2, s3, c2, rect, star])

# Auto-match by shape: squares should pair despite reordering, star/rect fade in, one gold square fades out
scene.geometry.transform_matching_shapes(src_group, dst_group, duration=2.2)
scene.wait(0.8)

# Segment 2: tex matching — order-preserving LCS
scene.segment("tex_matching", Transition.cross_fade(0.4))
scene.wait(0.2)
caption = scene.text("TransformMatchingTex: LCS + geometría · preserva orden").fill(WHITE).move_to(0, 3.5).scale_to(0.62)
scene.play([caption.animate.fade_in().duration(0.6).easing(Easing.SMOOTH)])
scene.wait(0.2)

src_text = scene.text("ABCD", role="title").fill(BLUE).move_to(0, 0.666667).scale_to(1.2)
scene.play([src_text.animate.write().duration(0.7).easing(Easing.SMOOTH)])
scene.wait(0.3)

dst_text = scene.text("BADC", role="title").fill(GREEN).move_to(0, 0.666667).scale_to(1.2)
# Scrambled order BADC shares all letters but order differs — LCS should keep longest order-preserving subset (mismo centro)
scene.play([src_text.animate.transform_to(dst_text).duration(2.0)])
scene.wait(0.8)

# Segment 3: equation tex matching — real math
scene.segment("equation_matching", Transition.cross_fade(0.4))
scene.wait(0.2)
eq_title = scene.text("Ecuaciones: mantiene 'm' y reordena, sobrantes Fade").fill(WHITE).move_to(0, 3.166667).scale_to(0.6)
scene.play([eq_title.animate.fade_in().duration(0.5).easing(Easing.SMOOTH)])

e1 = scene.text.equation("E = m c").fill(WHITE).move_to(0, 0.666667).scale_to(1.3)
scene.play([e1.animate.write().duration(0.7).easing(Easing.SMOOTH)])
scene.wait(0.3)

e2 = scene.text.equation("p = m v^2").fill(GOLD).move_to(0, 0.666667).scale_to(1.3)
# Tex matching on equations: 'm' matches by key, other glyphs fade (mismo y, no debajo)
scene.play([e1.animate.transform_to(e2).duration(1.6)])
scene.wait(1.0)

finale = scene.text("Mejoras: winding preservado, spring-continuo, coste combinado, greedy >64", role="subtitle").fill(GREEN).move_to(0, -3.5).scale_to(0.9)
scene.play([finale.animate.write().duration(0.8).easing(Easing.SMOOTH)])
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
