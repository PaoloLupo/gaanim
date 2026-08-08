"""A real rigged glTF character with deterministic action cross-fades."""

import os

from gaanim import BLACK, CYAN, GOLD, GRAY, WHITE, Scene


scene = Scene(1920, 1080, background=BLACK)
scene.assets_dir("examples/assets")

fox = scene.gltf("Fox.glb").at_3d(2.0, -1.0, 0.0).scaled_3d(0.040, 0.040, 0.04)

# Keep the example honest if the downloaded asset is ever replaced.
expected_actions = {"Survey", "Walk", "Run"}
missing_actions = expected_actions.difference(fox.animations())
if missing_actions:
    raise RuntimeError(f"Fox.glb is missing glTF actions: {sorted(missing_actions)}")

title = (
    scene.text("glb character animation")
    .fill(WHITE)
    .hud()
    .at(0, 500)
)
caption = (
    scene.text("Khronos Fox  ·  Survey  →  Walk  →  Run  ·  cross-fade")
    .fill(GRAY)
    .hud()
    .at(0, 450)
)
survey_label = scene.text("SURVEY").fill(CYAN).hud().at(-505, -300)
walk_label = scene.text("WALK").fill(GOLD).hud().at(-35, -300)
run_label = scene.text("RUN").fill(WHITE).hud().at(410, -300)

scene.camera.perspective(fov_y=1, near=0.1, far=500.0, duration=0.0)
scene.camera.look_at(eye=(3.8, 2.4, 5.8), target=(0.0, 0.15, 0.0), duration=0.0)

# Survey is deliberately short: it establishes the character before the
# locomotion cycles begin. Each transition overlaps the previous action.
scene.play([
    fox.animation("Survey", duration=2.4),
    title.write(duration=0.8),
    caption.fade_in(0.5),
    survey_label.fade_in(0.4),
])
scene.play([
    fox.animation("Walk", duration=3.6, loop=True, transition=0.45),
    survey_label.fade_out(0.25),
    walk_label.fade_in(0.35),
])
scene.camera.orbit(delta_yaw=0.32, delta_pitch=0.04, duration=3.6)
scene.play([
    fox.animation("Run", duration=3.0, loop=True, transition=0.45),
    walk_label.fade_out(0.25),
    run_label.fade_in(0.35),
])
scene.camera.orbit(delta_yaw=-0.18, delta_pitch=-0.02, duration=3.0)
scene.play([
    fox.animation("Survey", duration=1.8, reverse=True, transition=0.45),
    run_label.fade_out(0.25),
    survey_label.fade_in(0.35),
])


if os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(os.environ["GAANIM_SNAPSHOTS"], [0.0, 1.2, 2.8, 4.6, 7.0, 9.0, 10.8])

scene.render()
