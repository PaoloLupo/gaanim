"""3D camera: aiming, orbiting, dollying, lenses, saved poses, bindings, follows, snapshots and views of their own.

Every segment starts from the same perspective pose over a few lit
primitives on a floor and plays one camera move. The label, pinned to the
frame with ``hud()`` so the camera never moves it, is the code of the move.
"""

import os

from gaanim import Anchor, Material3D, Scene, Zones

HOLD = 0.5
MOVE = 1.6

scene = Scene()
accent = scene.canvas.color("accent")
success = scene.canvas.color("success")
danger = scene.canvas.color("danger")
chart = scene.canvas.color("chart")
rule = scene.canvas.color("rule")

# A camera state is a reusable pose: where the eye is, what it looks at and
# its vertical field of view in radians. to() cuts to it; animate.to() moves.
HOME = scene.camera.state_3d(eye=(7, 5, 10), target=(0, -1, 0), fov_y=0.75)
OVERHEAD = scene.camera.state_3d(eye=(0, 13, 5), target=(0, -0.5, 0), fov_y=0.6)


def stage():
    """A floor with a cube, a sphere, a cone and a cylinder; returns the sphere."""
    scene.geometry.plane(11, 7, material=Material3D.matte(rule)).move_to_3d(0, -1, 0)
    scene.geometry.cube(2, material=Material3D.matte(accent)).move_to_3d(-3, 0, -0.5)
    scene.geometry.cone(1.1, 2.6, material=Material3D.matte(danger)).move_to_3d(2.6, 0.3, -1.2)
    scene.geometry.cylinder(0.7, 1.6, material=Material3D.metal(chart)).move_to_3d(3.2, -0.2, 1.8)
    return scene.geometry.sphere(1.1, material=Material3D.matte(success)).move_to_3d(-0.2, 0.1, 1.2)


def shot(name, label):
    """Start segment ``name`` at the home pose, with its stage and its label; returns the sphere."""
    scene.segment(name)
    scene.camera.to(HOME)
    band = scene.layout.zones(Zones.rows(["1fr", 0.5], padding="page_padding"))[1]
    scene.text(label, role="code", text_align="left").hud().place(band, anchor="left")
    sphere = stage()
    scene.wait(HOLD)
    return sphere


def move(animation):
    """Play one camera move, then hold where it ends."""
    scene.play([animation.duration(MOVE)])
    scene.wait(HOLD)


# Aim the camera from a new eye at a new target.
shot("look_at", "scene.camera.animate.look_at(eye=(0, 9, 9), target=(0, -1, 0))")
move(scene.camera.animate.look_at(eye=(0, 9, 9), target=(0, -1, 0)))

# Circle the target: yaw turns about the vertical, pitch raises or lowers the eye.
shot("orbit", "scene.camera.animate.orbit(delta_yaw=1.2, delta_pitch=0.25)")
move(scene.camera.animate.orbit(delta_yaw=1.2, delta_pitch=0.25))

# Move the eye along its line of sight: below 1 closer, above 1 farther.
shot("dolly", "scene.camera.animate.dolly(0.55)")
move(scene.camera.animate.dolly(0.55))

# Change the lens: a narrower field of view zooms in without moving the eye.
shot("perspective", "scene.camera.animate.perspective(0.35)")
move(scene.camera.animate.perspective(0.35))

# Dolly in while the lens widens, so the target keeps its size and the depth
# stretches: the vertigo shot. Above 1 the eye backs away and depth flattens.
shot("dolly_zoom", "scene.camera.animate.dolly_zoom(0.4)")
move(scene.camera.animate.dolly_zoom(0.4))

# Move to a stored state, then back to a pose saved under a name.
shot("states", 'scene.camera.save("home")\nscene.camera.animate.to(OVERHEAD)\nscene.camera.animate.restore("home")')
scene.camera.save("home")
move(scene.camera.animate.to(OVERHEAD))
move(scene.camera.animate.restore("home"))

# Bind the camera to drawables and reactive values until the binding is
# disabled: here its target follows the sphere and its field of view a
# parameter.
ball = shot("bind_3d", "follow = scene.camera.bind_3d(target=ball, fov_y=lens)")
lens = scene.viz.parameter(0.75)
follow = scene.camera.bind_3d(target=ball, fov_y=lens)
scene.play([ball.animate.move_to_3d(-3.5, 0.1, 2.5), lens.animate.set(0.5)], duration=MOVE)
scene.play([ball.animate.move_to_3d(3.5, 0.1, 2.5)], duration=MOVE)
scene.wait(HOLD)
follow.disable()

# Switch to a parallel projection at once; an orbit then turns the same view
# without perspective.
shot("orthographic", "scene.camera.orthographic()\nscene.camera.animate.orbit(delta_yaw=1.2, delta_pitch=0)")
scene.camera.orthographic()
move(scene.camera.animate.orbit(delta_yaw=1.2, delta_pitch=0))

# Back to the default 2D camera: orthographic, looking down the z axis.
shot("reset", "scene.camera.animate.reset()")
move(scene.camera.animate.reset())

# Follow the sphere as it rises and moves away: the camera moves with it,
# without turning, and keeps it at the center as it framed the target; with
# a lag it catches up. Unlike bind_3d, the eye moves too.
ball = shot("follow", "scene.camera.animate.follow(ball, lag=0.3)")
scene.play(
    [
        scene.camera.animate.follow(ball, lag=0.3).duration(2 * MOVE),
        ball.animate.move_to_3d(3, 1.5, -2).duration(2 * MOVE),
    ]
)
scene.wait(HOLD)

# A second camera follows the sphere too: the inset's frame stays where the
# main camera draws it, and the screen shows it enlarged.
ball = shot("inset", "scene.camera.inset(ball, zoom=3, follow=True)")
view = scene.camera.inset(ball, zoom=3, at=Anchor.TOP_RIGHT, follow=True)
scene.play([view.animate.pop_out().duration(MOVE)])
scene.play([ball.animate.move_to_3d(3, 1.5, -2).duration(2 * MOVE)])
scene.wait(HOLD)

# view.snapshot() keeps what the inset shows at one instant, the 3D content
# drawn as the main camera drew it then, while the inset goes on following
# the sphere.
ball = shot("snapshot", "photo = view.snapshot()")
view = scene.camera.inset(ball, zoom=2, at=Anchor.TOP_RIGHT, follow=True)
page = scene.layout.zones(Zones.rows(["1fr", 0.5], padding="page_padding"))[0]
scene.play([view.animate.pop_out().duration(MOVE)])
photo = view.snapshot()
scene.play(
    [
        photo.animate.scale_to(0.5).duration(MOVE),
        photo.animate.place(page, anchor="bottom_right").duration(MOVE),
        ball.animate.move_to_3d(3, 1.5, -2).duration(2 * MOVE),
    ]
)
scene.wait(HOLD)

# With turn=True the camera stays where it is and turns to keep the sphere
# at the center, as on a tripod, and ends looking at it.
ball = shot("turn", "scene.camera.animate.follow(ball, turn=True)")
scene.play(
    [
        scene.camera.animate.follow(ball, turn=True).duration(2 * MOVE),
        ball.animate.move_to_3d(3, 1.5, -2).duration(2 * MOVE),
    ]
)
scene.wait(HOLD)

# A view with a 3D camera of its own shows the stage from another angle:
# from overhead, keeping the sphere at its center as it moves.
ball = shot("view_3d", "view = screen.camera_view_3d(eye=(0, 12, 0.5), target=ball)")
corner = scene.layout.zones(Zones.grid(rows=2, columns=2, padding="page_padding"))[1]
top = scene.geometry.rounded_rect(4.8, 2.7, 0.15).stroke(accent, 0.04).hud().place(corner, anchor="top_right")
top.camera_view_3d(eye=(0, 12, 0.5), target=ball, fov_y=0.6, up=(0, 0, -1))
scene.play([ball.animate.move_to_3d(3, 1.5, -2).duration(2 * MOVE)])
scene.wait(HOLD)

# Before and after each move. Segments last one move, except the states
# segment (two moves) and the bind_3d segment (two moves of the sphere).
one = 2 * HOLD + MOVE
states = 5 * one
bind = states + HOLD + 2 * (MOVE + HOLD)
last = bind + 2 * (HOLD + MOVE)
seeks = [start + t for start in (0, one, 2 * one, 3 * one, 4 * one, last, last + one) for t in (0.3, one - 0.2)]
seeks += [states + 0.3, states + HOLD + MOVE + 0.2, bind - 0.2]
seeks += [bind + HOLD + MOVE, last - 0.2, last + one + HOLD + MOVE / 2]
# Halfway through the follow and once it holds; the inset as it opens,
# halfway through the sphere's move and at its end.
follow = last + 2 * one
inset = follow + 2 * (HOLD + MOVE)
seeks += [follow + HOLD + MOVE, follow + HOLD + 2 * MOVE + HOLD / 2]
seeks += [inset + HOLD + MOVE / 2, inset + HOLD + 2 * MOVE, inset + HOLD + 3 * MOVE + HOLD / 2]
# The snapshot as it leaves the inset, and both once the sphere has moved.
snapshot = inset + 2 * HOLD + 3 * MOVE
seeks += [snapshot + HOLD + MOVE + MOVE / 2, snapshot + HOLD + 3 * MOVE + HOLD / 2]
# Halfway through the turning follow and once it holds.
turn = snapshot + 2 * HOLD + 3 * MOVE
seeks += [turn + HOLD + MOVE, turn + HOLD + 2 * MOVE + HOLD / 2]
# The view from overhead as the sphere starts and once it has moved.
view_3d = turn + 2 * HOLD + 2 * MOVE
seeks += [view_3d + HOLD + MOVE / 4, view_3d + HOLD + 2 * MOVE + HOLD / 2]
if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(snapshots, sorted(seeks))
else:
    scene.render()
