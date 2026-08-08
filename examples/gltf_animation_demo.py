"""Native glTF PBR, Blender Actions, manual node transforms, and Vello overlay."""

import os

from gaanim import BLACK, GOLD, WHITE, Scene


scene = Scene(1280, 720, background=BLACK)
scene.assets_dir("examples/assets")

model = scene.gltf("gltf_animation_fixture.gltf", scene="Presentation")
arm = model.part("Robot/Rig/Arm")

title = (
    scene.text("glTF 3D + timeline determinista")
    .fill(WHITE)
    .hud()
    .at(0, 300)
)
caption = (
    scene.text("Action cross-fade + wrapper manual + overlay Vello")
    .fill(GOLD)
    .hud()
    .at(0, 265)
)

scene.camera.perspective(fov_y=0.785, near=0.1, far=100.0, duration=0.0)
scene.camera.look_at(eye=(3.5, 2.5, 5.0), target=(0.0, 0.4, 0.0), duration=0.0)

scene.play([model.animation("Walk"), title.write(duration=0.8)])
scene.play([
    model.animation("Wave", duration=2.0, loop=True, transition=0.35),
    arm.rotate_by_3d("y", 0.6).duration(2.0),
    caption.fade_in(0.5),
])
scene.play([model.animation("Walk", start_time=0.5, reverse=True)])

if os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(os.environ["GAANIM_SNAPSHOTS"], [0.0, 0.5, 1.2, 2.5, 3.8])

scene.render()
