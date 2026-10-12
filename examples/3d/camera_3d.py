"""3D camera: aiming, orbiting, dollying, lenses, saved poses and bindings of the scene camera.

Every segment starts from the same perspective pose over a few lit
primitives on a floor and plays one camera move. The label, pinned to the
frame with ``hud()`` so the camera never moves it, is the code of the move.
"""

import os

from gaanim import Material3D, Scene, Zones

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

# Before and after each move. Segments last one move, except the states
# segment (two moves) and the bind_3d segment (two moves of the sphere).
one = 2 * HOLD + MOVE
states = 5 * one
bind = states + HOLD + 2 * (MOVE + HOLD)
last = bind + 2 * (HOLD + MOVE)
seeks = [start + t for start in (0, one, 2 * one, 3 * one, 4 * one, last, last + one) for t in (0.3, one - 0.2)]
seeks += [states + 0.3, states + HOLD + MOVE + 0.2, bind - 0.2]
seeks += [bind + HOLD + MOVE, last - 0.2, last + one + HOLD + MOVE / 2]
if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(snapshots, sorted(seeks))
else:
    scene.render()
