"""Backgrounds: the scene's background, solid and gradient segment backgrounds, living backgrounds and WGSL shaders.

``scene.canvas.background`` paints behind every segment; a segment's own
``background=`` replaces it while that segment plays, and a segment without
one goes back to it. A background is a color, a ``Brush`` or a
``Background``: the living ones drift with the timeline, and a shader is
drawn by your own WGSL. Each panel holds the code that sets the background
around it.
"""

import os
from pathlib import Path

from gaanim import Background, Brush, ColorMap, Scene, Zones

SHOW = 0.4
HOLD = 1.8

scene = Scene()
background = scene.canvas.color("background")
panel = scene.canvas.color("panel")
accent = scene.canvas.color("accent")
success = scene.canvas.color("success")
# Palettes for the backgrounds, from two color maps: four blues from dark
# to light, and a sunset from dark red to yellow.
NIGHT = ColorMap("lapaz").colors(4)
DUSK = ColorMap("lajolla").reversed().colors(4)[1:]
# The frame is 16 x 9 scene units around the origin; brushes on a background
# use scene coordinates.
HALF_WIDTH = scene.canvas.frame_width / 2


def show(code, moves=()):
    """Fade in a panel at the bottom of the frame holding ``code``, then hold it while ``moves`` play."""
    _, strip = scene.layout.zones(Zones.rows(["4fr", "1fr"], padding="page_padding"))
    text = scene.text(code, role="code", markup=False)
    box = scene.layout.box(text, padding=("16px", "28px"), background=panel, radius="12px")
    box.place(strip, anchor="bottom")
    scene.play([box.animate.fade_in()], duration=SHOW)
    if moves:
        scene.play(list(moves), duration=HOLD)
    else:
        scene.wait(HOLD)


# The scene background shows in every segment without a background of its
# own; without one, it is the theme's.
scene.canvas.background = Background.dot_grid(0.5, radius=0.035, background=background)
scene.segment("scene")
show("scene.canvas.background = Background.dot_grid(\n    0.5, radius=0.035, background=background)")

# A color, or a gradient brush laid out in scene coordinates: x runs from
# -8 to 8 across the frame and y from -4.5 to 4.5. Background(paint) wraps
# either one the same way.
scene.segment("color", background=NIGHT[0])
show('scene.segment("color", background=NIGHT[0])')

scene.segment("linear", background=Brush.linear(NIGHT, start=(-HALF_WIDTH, 0), end=(HALF_WIDTH, 0)))
show('scene.segment("linear",\n    background=Brush.linear(NIGHT, start=(-8, 0), end=(8, 0)))')

scene.segment("radial", background=Brush.radial(DUSK + [background], center=(0, -4.5), radius=9))
show('scene.segment("radial", background=Brush.radial(\n    DUSK + [background], center=(0, -4.5), radius=9))')

# Living backgrounds move with the timeline, so every seek of a frame looks
# the same. speed scales the motion (0 holds it) and seed picks another layout.
scene.segment("mesh_gradient", background=Background.mesh_gradient(NIGHT[:2] + DUSK, speed=0.4, seed=4))
show("Background.mesh_gradient(NIGHT[:2] + DUSK, speed=0.4, seed=4)")

scene.segment("noise_gradient", background=Background.noise_gradient(DUSK, scale=1.6, speed=0.15, seed=1))
show("Background.noise_gradient(DUSK, scale=1.6, speed=0.15, seed=1)")

scene.segment("noise_scale", background=Background.noise_gradient(NIGHT, scale=4, speed=0.15, seed=2))
show("Background.noise_gradient(NIGHT, scale=4, speed=0.15, seed=2)")

# One curtain of light per color, over a night sky that sky= can change.
scene.segment("aurora", background=Background.aurora([success, NIGHT[3], accent], speed=0.4, seed=2))
show("Background.aurora([success, NIGHT[3], accent], speed=0.4, seed=2)")

scene.segment("aurora_sky", background=Background.aurora([success, NIGHT[3]], sky=DUSK[0], speed=0.4, seed=5))
show("Background.aurora([success, NIGHT[3]], sky=DUSK[0], speed=0.4, seed=5)")

# Dots every spacing units; the color keeps its alpha (DOTS is the accent
# at 160 of 255) and drift scrolls the grid in units per second.
DOTS = (accent.r, accent.g, accent.b, 160)
scene.segment(
    "dot_grid", background=Background.dot_grid(0.6, radius=0.08, color=DOTS, background=NIGHT[0], drift=(0.4, 0.2))
)
show("Background.dot_grid(0.6, radius=0.08, color=DOTS,\n    background=NIGHT[0], drift=(0.4, 0.2))")

# A shader defines gaanim_background(uv, resolution, time): uv runs from the
# top left (0, 0) to the bottom right (1, 1) and time is the timeline's, so
# seeks and exports agree. fallback paints outside the frame.
RIBBONS = """
fn gaanim_background(uv: vec2<f32>, resolution: vec2<f32>, time: f32) -> vec4<f32> {
    let q = (uv - vec2<f32>(0.5)) * vec2<f32>(resolution.x / resolution.y, 1.0);
    let warped = q + 0.08 * vec2<f32>(sin(q.y * 4.2 + time * 0.6), sin(q.x * 3.6 - time * 0.5));
    let folds = 0.5 + 0.5 * sin(warped.y * 9.0 + 1.5 * sin(warped.x * 3.3 - time * 0.4));
    let ribbon = smoothstep(0.55, 0.95, folds) * smoothstep(0.1, 0.8, length(q));
    let color = vec3<f32>(0.03, 0.04, 0.07) + vec3<f32>(0.2, 0.35, 0.55) * ribbon;
    return vec4<f32>(color, 1.0);
}
"""
scene.segment("shader", background=Background.shader(RIBBONS, fallback=NIGHT[0]))
show("Background.shader(RIBBONS, fallback=NIGHT[0])")

# uniforms hand the shader values it reads from gaanim_uniforms, evaluated
# every frame: here a parameter that brightens the rings as it animates.
RINGS = """
fn gaanim_background(uv: vec2<f32>, resolution: vec2<f32>, time: f32) -> vec4<f32> {
    let p = (uv - vec2<f32>(0.5)) * gaanim_frame_size(resolution);
    let d = length(p);
    let rings = 0.5 + 0.5 * cos(d * 3.0 - time * 2.0);
    let glow = exp(-d * 0.3) * gaanim_uniforms.level;
    let color = vec3<f32>(0.07) + vec3<f32>(0.95, 0.65, 0.25) * rings * glow;
    return vec4<f32>(color, 1.0);
}
"""
level = scene.viz.parameter(0.0)
scene.segment("uniforms", background=Background.shader(RINGS, uniforms={"level": level}, fallback=background))
show('Background.shader(RINGS, uniforms={"level": level})\nlevel.animate.set(1.0)', [level.animate.set(1.0)])

# A Path loads the WGSL from a file.
scene.segment("shader_file", background=Background.shader(Path("assets/graph_paper.wgsl"), fallback=background))
show('Background.shader(Path("assets/graph_paper.wgsl"))')

# A segment without background= shows the scene background again.
scene.segment("scene_again")
show('scene.segment("scene_again")')

STEP = SHOW + HOLD
# Every segment settled; the ones that move also early on, to show them change.
MOVING = (4, 7, 9, 10, 11, 12)
SEEKS = [STEP * index + t for index in range(14) for t in ((0.5, 2.0) if index in MOVING else (2.0,))]
if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(snapshots, SEEKS)
else:
    scene.render()
