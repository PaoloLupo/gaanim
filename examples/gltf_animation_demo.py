"""A real rigged glTF character with deterministic action cross-fades."""

import os

from gaanim import BLACK, CYAN, GOLD, GRAY, WHITE, Scene

scene = Scene(frame=(16, 9), background=BLACK)
scene.assets.assets_dir("examples/assets")

fox = scene.media.gltf("Fox.glb").move_to_3d(2.0, -1.0, 0.0).scale_to_3d(0.040, 0.040, 0.04)

# Keep the example honest if the downloaded asset is ever replaced.
expected_actions = {"Survey", "Walk", "Run"}
missing_actions = expected_actions.difference(fox.animations())
if missing_actions:
    raise RuntimeError(f"Fox.glb is missing glTF actions: {sorted(missing_actions)}")

title = (
    scene.text("glb character animation")
    .fill(WHITE)
    .hud()
    .move_to(0, 4.166667)
)
caption = (
    scene.text("Khronos Fox  ·  Survey  →  Walk  →  Run  ·  cross-fade")
    .fill(GRAY)
    .hud()
    .move_to(0, 3.75)
)
survey_label = scene.text("SURVEY").fill(CYAN).hud().move_to(-4.208333, -2.5)
walk_label = scene.text("WALK").fill(GOLD).hud().move_to(-0.291667, -2.5)
run_label = scene.text("RUN").fill(WHITE).hud().move_to(3.416667, -2.5)


# Survey is deliberately short: it establishes the character before the
# locomotion cycles begin. Each transition overlaps the previous action.
scene.camera.perspective(fov_y=1, near=0.1, far=500.0)
scene.camera.look_at(eye=(3.8, 2.4, 5.8), target=(0.0, 0.15, 0.0))
scene.play([
    fox.animation("Survey", duration=2.4),
    title.animate.write().duration(0.8),
    caption.animate.fade_in().duration(0.5),
    survey_label.animate.fade_in().duration(0.4),
])
scene.play([
    fox.animation("Walk", duration=3.6, loop=True, transition=0.45),
    survey_label.animate.fade_out().duration(0.25),
    walk_label.animate.fade_in().duration(0.35),
])
scene.play([
    fox.animation("Run", duration=3.0, loop=True, transition=0.45),
    scene.camera.animate.orbit(delta_yaw=0.32, delta_pitch=0.04).duration(3.6),
    walk_label.animate.fade_out().duration(0.25),
    run_label.animate.fade_in().duration(0.35),
])
scene.play([
    fox.animation("Survey", duration=1.8, reverse=True, transition=0.45),
    scene.camera.animate.orbit(delta_yaw=-0.18, delta_pitch=-0.02).duration(3.0),
    run_label.animate.fade_out().duration(0.25),
    survey_label.animate.fade_in().duration(0.35),
])


if os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(os.environ["GAANIM_SNAPSHOTS"], [0.0, 1.2, 2.8, 4.6, 7.0, 9.0, 10.8])

scene.render()
