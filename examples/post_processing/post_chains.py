"""Post-process chains, the scene default and its segment overrides, and custom WGSL passes with uniforms.

The first two segments chain the same two passes in both orders, the next
three inherit, replace and switch off the scene's post-process, and the last
three run a WGSL pass of our own whose uniform is a number, an animated
Parameter and seeded noise. Each label is the code that sets the segment's
post-process.
"""

import os

from gaanim import Brush, PostProcess, Scene, Zones

HOLD = 1.0

# gaanim_post receives the frame position (uv, 0..1 from the top left), its
# size in output pixels and the timeline time; gaanim_scene(uv) samples what
# was drawn and gaanim_uniforms holds the uniforms, as f32 fields.
WAVE = """
fn gaanim_post(uv: vec2<f32>, resolution: vec2<f32>, time: f32) -> vec4<f32> {
    let shift = sin(uv.y * 12.0 - time * 4.0) * gaanim_uniforms.amount;
    return gaanim_scene(uv + vec2<f32>(shift, 0.0));
}
"""

scene = Scene()
background = scene.colors.background
foreground = scene.colors.foreground
page = scene.layout.zones(Zones.rows(["5fr", "1fr"], gap="24px", padding="page_padding"))

# The scene's post-process applies to every segment that does not set its own.
scene.canvas.post = PostProcess.vignette(1.0, softness=0.3)


def test_card(zone):
    """Fill ``zone`` with four colored tiles, each holding a dark shape, above a gray ramp."""
    tiles, ramp = zone.split(Zones.rows(["4fr", "1fr"], gap="24px"))
    cells = tiles.split(Zones.columns(4, gap="24px"))
    roles = ["accent", "success", "danger", "chart"]
    shapes = [
        lambda: scene.geometry.circle(1.0),
        lambda: scene.geometry.regular_polygon(3, 1.0),
        lambda: scene.geometry.star(5, 1.0, 0.45),
        lambda: scene.geometry.annulus(1.0, 0.55),
    ]
    for cell, role, shape in zip(cells, roles, shapes):
        scene.geometry.rounded_rect(cell.width, cell.height, 0.2).fill(scene.colors[role]).place(cell)
        shape().fill(background).place(cell.inset("48px"), fit="contain")
    (left, y), (right, _) = ramp.point("left"), ramp.point("right")
    gray = Brush.linear([background, foreground], start=(left, y), end=(right, y))
    scene.geometry.rounded_rect(ramp.width, ramp.height, 0.2).fill(gray).place(ramp)


def page_with(name, lines, **options):
    """Start segment ``name`` with ``options``, draw the test card and label it with ``lines`` of code."""
    start = scene.cursor
    scene.segment(name, **options)
    test_card(page[0])
    for line, zone in zip(lines, page[1].split(Zones.rows(len(lines)))):
        scene.text(line, role="code").place(zone, anchor="left")
    return start


# A list chains passes in order, each one sampling what the previous one
# wrote: aberration then gray leaves gray fringes, gray then aberration
# splits color out of gray tiles.
starts = []
starts.append(page_with("aberration first", [
    "post=[PostProcess.chromatic_aberration(0.015),",
    "      PostProcess.color_grade(saturation=0.0)]",
], post=[PostProcess.chromatic_aberration(0.015), PostProcess.color_grade(saturation=0.0)]))
scene.wait(HOLD)
starts.append(page_with("gray first", [
    "post=[PostProcess.color_grade(saturation=0.0),",
    "      PostProcess.chromatic_aberration(0.015)]",
], post=[PostProcess.color_grade(saturation=0.0), PostProcess.chromatic_aberration(0.015)]))
scene.wait(HOLD)

# Without post= a segment inherits scene.canvas.post; post= replaces it while
# the segment is active, and post=False (or an empty list) draws it plain.
DEFAULT = "scene.canvas.post = PostProcess.vignette(1.0, softness=0.3)"
starts.append(page_with("inherit", [DEFAULT, 'scene.segment("inherit")']))
scene.wait(HOLD)
starts.append(page_with("override", [DEFAULT, 'scene.segment("override", post=PostProcess.crt())'],
                        post=PostProcess.crt()))
scene.wait(HOLD)
starts.append(page_with("plain", [DEFAULT, 'scene.segment("plain", post=False)'], post=False))
scene.wait(HOLD)

# Uniforms are numbers, or a Parameter, Variable, Computed or scene.time of
# this scene, read every frame.
starts.append(page_with("number", ['PostProcess.shader(WAVE, uniforms={"amount": 0.02})'],
                        post=PostProcess.shader(WAVE, uniforms={"amount": 0.02})))
scene.wait(HOLD)
swell = scene.viz.parameter(0.0)
starts.append(page_with("parameter", [
    'PostProcess.shader(WAVE, uniforms={"amount": swell})',
    "swell.animate.set(0.04).repeat(2, yoyo=True)",
], post=PostProcess.shader(WAVE, uniforms={"amount": swell})))
scene.play([swell.animate.set(0.04).duration(HOLD / 2).repeat(2, yoyo=True)])
noise = scene.noise(frequency=2.0, amplitude=0.03, seed=4)
starts.append(page_with("noise", [
    'PostProcess.shader(WAVE, uniforms={"amount": noise})',
    "noise = scene.noise(frequency=2.0, amplitude=0.03, seed=4)",
], post=PostProcess.shader(WAVE, uniforms={"amount": noise})))
scene.wait(HOLD)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    # The middle of every segment, which is the Parameter's peak in its
    # segment, the Parameter on its way up, and the noise at two moments.
    scene.snapshots(snapshots, sorted([start + HOLD / 2 for start in starts] + [starts[6] + HOLD / 4, starts[7] + 0.9]))
else:
    scene.render()
