"""Edge cases of keyframes, throw/inertia, speed ramps and path modifiers.

Each row stresses a corner of the API that is easy to get wrong: a
yoyo-repeated keyframe clip, a dead throw played in slow motion, a flick
that snaps behind it, a ramp over a written text and a repeated spin, a
stepped time remap, a modifier added mid-scene on a trimmed path, and an
offset that shrinks a shape away.
"""

import os

from gaanim import BLUE, CORAL, GOLD, GRAY, TEAL, WHITE, Easing, Scene, parallel, sequence


scene = Scene(frame=(16, 9))

# Row 1: keyframes repeated as a yoyo come back to the start; two stops
# with a None start are a plain move.
hopper = scene.geometry.circle(0.25).fill(GOLD).move_to(-6.5, 2.8)
bounce = hopper.animate.keyframes(
    times=[0, 0.5, 1], position=[None, (-5.0, 3.6), (-3.5, 2.8)], spatial="catmull_rom"
).duration(0.6).repeat(2, yoyo=True)
slider = scene.geometry.square(0.4).fill(TEAL).move_to(-2.5, 2.8)
slide = slider.animate.keyframes(times=[0, 1], position=[None, (-0.5, 2.8)]).duration(1.2)

# Row 2: a throw without bounce (restitution 0) stretched to 2 s, and a flick
# to the right whose only notch is behind it.
dead = scene.geometry.circle(0.25).fill(CORAL).move_to(0.5, 3.2)
scene.geometry.line(0.0, 1.8, 3.0, 1.8).stroke(GRAY, 0.02)
thud = dead.animate.throw(velocity=(1.0, 0.0), floor=1.8, restitution=0.0).duration(2.0)
flick = scene.geometry.circle(0.2).fill(BLUE).move_to(5.0, 2.8)
back = flick.animate.inertia(velocity=3.0, friction=4.0, snap=[4.0])

scene.play([bounce, slide, thud, back])

# Row 3: a ramp over a written text (one clip per glyph) and a spin
# repeated three times, in slow motion through the middle.
word = scene.text("ramp").fill(WHITE).move_to(-4.5, 0.5)
spinner = scene.geometry.square(0.6).fill(GOLD).move_to(-1.5, 0.5)
scene.play(
    parallel(word.animate.write().duration(1.0), spinner.animate.rotate_by(1.57).duration(0.4).repeat(3)).speed_ramp(
        {0.3: 1.0, 0.5: 0.2, 0.7: 1.0}
    )
)

# Row 3, right: a stepped remap jumps through its children in 4 beats.
steppers = [scene.geometry.circle(0.15).fill(TEAL).move_to(1.0 + 0.6 * i, 0.5) for i in range(5)]
scene.play(sequence(*(dot.animate.shift_by(0, -1.0).duration(0.2) for dot in steppers)).time_remap(Easing.steps(4)))

# Row 4: a modifier added mid-scene, on a line being trimmed, and an offset
# that shrinks a circle until nothing is left.
wire = scene.geometry.line(-6.5, -2.5, -1.5, -2.5).no_fill().stroke(CORAL, 0.05)
scene.wait(0.2)
teeth = wire.modifiers.zigzag(size=0.2, ridges=8)
coin = scene.geometry.circle(0.6).fill(BLUE).move_to(2.5, -2.5)
shrink = coin.modifiers.offset(0.0)
scene.play([wire.animate.trim(end=0.6).duration(0.6), teeth.animate.size(0.05).duration(0.6), shrink.animate.amount(-0.7).duration(0.6)])
scene.wait(0.2)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(snapshots, [0.3, 0.9, 1.9, 2.4, 3.1, 3.9, 4.4, 4.9, 5.2, 5.6])
else:
    scene.render()
