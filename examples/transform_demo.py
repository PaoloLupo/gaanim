"""Example: Transform and ReplacementTransform.

Manual smoke test for:
  - shape -> shape morph
  - shape -> text replacement
  - text -> text morph
  - text -> math morph
  - math -> math morph
"""

import os

from gaanim import Easing, Anchor, BLACK, BLUE, GREEN, RED, WHITE, Scene, Transition

c = Scene(1920, 1080, background=BLACK)


# Segment 1: shape -> shape
c.segment("shape_to_shape")

title = c.text("Transform Demo", role="title").fill(WHITE).move_to(0.0, 260.0, anchor=Anchor.CENTER)
subtitle = (
    c.text(
        "shape -> shape, shape -> text, text -> math, math -> math",
        role="subtitle",
    )
    .fill(WHITE)
    .move_to(0.0, 200.0, anchor=Anchor.CENTER)
)
c.play([title.animate.write().duration(1.0).easing(Easing.SMOOTH)])
c.play([subtitle.animate.write().duration(1.0).easing(Easing.SMOOTH)])

circle = c.geometry.circle(95.0).fill(BLUE).stroke(WHITE, 4.0).move_to(-260.0, -20.0)
c.play([circle.animate.create().duration(0.9).easing(Easing.SMOOTH)])
c.wait(0.3)

diamond = (
    c.geometry.rect(180.0, 180.0).fill(RED).stroke(WHITE, 4.0).move_to(260.0, 20.0).rotate_to(0.785398)
)
c.play([circle.animate.transform_to(diamond).duration(2.0).easing(Easing.spring(stiffness=90.0, damping=12.0))])
c.wait(0.8)


# Segment 2: shape -> text
c.segment("shape_to_text", Transition.cross_fade(0.5))
c.wait(1.0)

headline = c.text("Morphing into text", role="title").fill(GREEN).move_to(0.0, 100.0, anchor=Anchor.CENTER)
c.play([circle.animate.replacement_transform_to(headline).duration(2.1).easing(Easing.spring(stiffness=90.0, damping=12.0))])
c.wait(0.5)
c.play([headline.animate.indicate().duration(0.9)])
c.wait(0.7)


# Segment 3: text hierarchy morphs
# Keep this in the same segment to avoid transition flicker while exercising
# text/text and text/math conversions.

phrase = c.text("Energy", role="title").fill(WHITE).move_to(-260.0, 40.0, anchor=Anchor.CENTER)
c.play([phrase.animate.write().duration(0.8).easing(Easing.SMOOTH)])
c.wait(0.3)

target_text = c.text("Momentum", role="title").fill(BLUE).move_to(240.0, -20.0, anchor=Anchor.CENTER).scale_to(1.1)
c.play([phrase.animate.transform_to(target_text).duration(2.2).easing(Easing.spring(stiffness=90.0, damping=12.0))])
c.wait(0.8)

target_math = c.text("$p = m v$").fill(GREEN).move_to(0.0, 10.0, anchor=Anchor.CENTER).scale_to(1.2)
c.play([phrase.animate.transform_to(target_math).duration(2.2).easing(Easing.spring(stiffness=90.0, damping=12.0))])
c.wait(0.8)

alt_math = c.text("$E = m c^2$").fill(BLUE).move_to(0.0, 10.0, anchor=Anchor.CENTER).scale_to(1.2)
c.play([phrase.animate.replacement_transform_to(alt_math).duration(2.2).easing(Easing.spring(stiffness=90.0, damping=12.0))])
c.wait(0.8)

finale = (
    c.text("Transform pipeline with text and math hierarchies", role="subtitle")
    .fill(GREEN)
    .move_to(0.0, -240.0, anchor=Anchor.CENTER)
)
c.play([finale.animate.write().duration(0.9).easing(Easing.SMOOTH)])
c.play([alt_math.animate.indicate().duration(0.9)])
c.wait(1.8)


# Exact seeks cover the circular source, the "o" in Momentum, and every
# transform endpoint. They are captured only by `gaanim --diff`.
snapshot_dir = os.environ.get("GAANIM_SNAPSHOTS")
if snapshot_dir:
    count = c.snapshots(
        snapshot_dir,
        [
            0.0,
            2.45,
            3.7,
            4.5,
            5.2,
            7.7,
            8.5,
            9.1,
            12.9,
            13.7,
            14.5,
            16.4,
            17.5,
            19.4,
            20.5,
            22.6,
            24.0,
        ],
    )
    print(f"[gaanim-diff] captured {count} exact seeks in {snapshot_dir}")


if __name__ == "__main__":
    c.render()
