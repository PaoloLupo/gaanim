"""Example: Transform and ReplacementTransform.

Manual smoke test for:
  - shape -> shape morph
  - shape -> text replacement
  - text -> text morph
  - text -> math morph
  - math -> math morph
"""

import os

from gaanim import Anchor, BLACK, BLUE, GREEN, RED, WHITE, Scene, Transition

c = Scene(1920, 1080, background=BLACK)


# Segment 1: shape -> shape
c.segment("shape_to_shape")

title = c.text("Transform Demo", role="title").fill(WHITE).at(0.0, 260.0, anchor=Anchor.CENTER)
subtitle = (
    c.text(
        "shape -> shape, shape -> text, text -> math, math -> math",
        role="subtitle",
    )
    .fill(WHITE)
    .at(0.0, 200.0, anchor=Anchor.CENTER)
)
title.write().duration(1.0).smooth()
subtitle.write().duration(1.0).smooth()

circle = c.circle(95.0).fill(BLUE).stroke(WHITE, 4.0).at(-260.0, -20.0)
circle.create().duration(0.9).smooth()
c.wait(0.3)

diamond = (
    c.rect(180.0, 180.0).fill(RED).stroke(WHITE, 4.0).at(260.0, 20.0).rotated(0.785398)
)
circle.transform(diamond).duration(2.0).spring()
c.wait(0.8)


# Segment 2: shape -> text
c.segment("shape_to_text", Transition.cross_fade(0.5))
c.wait(1.0)

headline = c.text("Morphing into text", role="title").fill(GREEN).at(0.0, 100.0, anchor=Anchor.CENTER)
circle.replacement_transform(headline).duration(2.1).spring()
c.wait(0.5)
headline.indicate().duration(0.9)
c.wait(0.7)


# Segment 3: text hierarchy morphs
# Keep this in the same segment to avoid transition flicker while exercising
# text/text and text/math conversions.

phrase = c.text("Energy", role="title").fill(WHITE).at(-260.0, 40.0, anchor=Anchor.CENTER)
phrase.write().duration(0.8).smooth()
c.wait(0.3)

target_text = c.text("Momentum", role="title").fill(BLUE).at(240.0, -20.0, anchor=Anchor.CENTER).scaled(1.1)
phrase.transform(target_text).duration(2.2).spring()
c.wait(0.8)

target_math = c.text("$p = m v$").fill(GREEN).at(0.0, 10.0, anchor=Anchor.CENTER).scaled(1.2)
phrase.transform(target_math).duration(2.2).spring()
c.wait(0.8)

alt_math = c.text("$E = m c^2$").fill(BLUE).at(0.0, 10.0, anchor=Anchor.CENTER).scaled(1.2)
phrase.replacement_transform(alt_math).duration(2.2).spring()
c.wait(0.8)

finale = (
    c.text("Transform pipeline with text and math hierarchies", role="subtitle")
    .fill(GREEN)
    .at(0.0, -240.0, anchor=Anchor.CENTER)
)
finale.write().duration(0.9).smooth()
alt_math.indicate().duration(0.9)
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
