"""Images: fitting a raster image to a frame, cropping it, panning and zooming inside it, sampling quality, and points on its pixels.

Every image sits in an outline of the frame it is given, so the fit modes
show what they keep and what they cut. Each crop is shown twice: the whole
image with the crop window on it, and the cropped image in its frame.
Animating the crop pans and zooms inside the same frame, and points and
regions on source pixels stay on their detail while it does.
"""

import os

from gaanim import Scene, Zones

BANNER = "assets/banner.png"  # 1280 x 640 pixels
BADGE = "assets/badge.png"  # 512 x 512 pixels of pixel art

scene = Scene()
accent = scene.canvas.color("accent")
rule = scene.canvas.color("rule")


def code(text, zone, anchor="left"):
    return scene.text(text, role="code").place(zone, anchor=anchor)


def outline(center, width, height):
    """Outline a frame of ``width`` x ``height`` around ``center``."""
    scene.geometry.rect(width, height).no_fill().stroke(rule, 0.03).move_to(*center)


# fit decides how the image meets its frame: contain keeps it whole (with
# empty bands), cover fills the frame and cuts what overflows, stretch
# distorts it. Given only a width or only a height, the other follows the
# image's proportions, which source_width and source_height also give.
scene.segment("fit")
W, H = 3.6, 2.7
page = scene.layout.zones(Zones.rows(["1fr", "10fr"], gap="24px", padding="page_padding"))
code(f"scene.media.image(BANNER).frame({W}, {H}, fit=...)", page[0])
cells = page[1].split(Zones.grid(rows=2, columns=3, row_gap="36px", column_gap="48px"))
stages = []
for cell, text in zip(cells, ['fit="contain"', 'fit="cover"', 'fit="stretch"', f"width={W}", "height=2.0", None]):
    stage, caption = cell.split(Zones.rows(["5fr", "1fr"], gap="12px"))
    if text:
        code(text, caption, "top")
    stages.append(stage)
for stage, fit in zip(stages, ["contain", "cover", "stretch"]):
    outline(stage.center, W, H)
    scene.media.image(BANNER).frame(W, H, fit=fit).move_to(*stage.center)
outline(stages[3].center, W, H)
scene.media.image(BANNER, width=W).move_to(*stages[3].center)
outline(stages[4].center, W, H)
banner = scene.media.image(BANNER, height=2.0).move_to(*stages[4].center)
# The source size of the banner beside it, whatever its frame.
code(f"source_width == {banner.source_width}\nsource_height == {banner.source_height}", stages[5], "left")
scene.wait(1.0)


def crop_rows(header, entries):
    """One row per entry: its code, the whole badge with the crop window, and the cropped badge.

    ``entries`` are (label, window, build) where ``window`` is the crop in
    fractions (x, y, width, height from the top-left) and ``build(size)``
    returns the cropped image. Returns each row's window and thumbnail
    geometry, for animating the window.
    """
    page = scene.layout.zones(Zones.rows(["1fr", "10fr"], gap="24px", padding="page_padding"))
    code(header, page[0])
    grid = page[1].split(Zones.grid(rows=len(entries), columns=["7fr", "2fr", "2fr"], row_gap="24px",
                                    column_gap="48px"))
    rows = []
    for index, (text, window, build) in enumerate(entries):
        label, whole, cropped = grid[3 * index], grid[3 * index + 1], grid[3 * index + 2]
        code(text, label, "right")
        size = min(whole.width, whole.height)
        scene.media.image(BADGE, width=size, height=size).move_to(*whole.center)
        outline(cropped.center, size, size)
        build(size).move_to(*cropped.center)
        rows.append((frame_window(whole.center, size, window), whole.center, size))
    return rows


def frame_window(center, size, window):
    """The crop ``window`` (fractions from the top-left, y down) drawn over an image of ``size``."""
    x, y, width, height = window
    left, top = center[0] - size / 2, center[1] + size / 2
    rect = scene.geometry.rect(size * width, size * height).no_fill().stroke(accent, 0.05)
    return rect.move_to(left + size * (x + width / 2), top - size * (y + height / 2))


# crop selects source pixels, from the top-left corner with y down, or
# fractions of the image with normalized=True. The frame stays the same.
scene.segment("crop")
crop_rows("scene.media.image(BADGE, width=size, height=size, crop=...)", [
    ("crop=(0, 0, 256, 256)", (0, 0, 0.5, 0.5),
     lambda size: scene.media.image(BADGE, width=size, height=size, crop=(0, 0, 256, 256))),
    ("crop=(256, 128, 256, 256)", (0.5, 0.25, 0.5, 0.5),
     lambda size: scene.media.image(BADGE, width=size, height=size, crop=(256, 128, 256, 256))),
    (".crop(0.25, 0.25, 0.5, 0.5, normalized=True)", (0.25, 0.25, 0.5, 0.5),
     lambda size: scene.media.image(BADGE, width=size, height=size).crop(0.25, 0.25, 0.5, 0.5, normalized=True)),
])
scene.wait(1.0)

# Animating the crop moves its window from the whole image: a zoom, a pan
# between two crops, and a zoom into a detail.
scene.segment("pan_zoom")
TARGETS = [(0.5, 0, 0.5, 0.5), (0, 0.5, 0.5, 0.5), (0.4, 0.25, 0.125, 0.125)]
images = []


def uncropped(size):
    images.append(scene.media.image(BADGE, width=size, height=size))
    return images[-1]


rows = crop_rows("image = scene.media.image(BADGE, width=size, height=size)", [
    (f"image.animate.crop({x}, {y}, {w}, {h},\n                   normalized=True)", (0, 0, 1, 1), uncropped)
    for x, y, w, h in TARGETS
])
scene.wait(0.4)
moves = []
for image, (window, center, size), (x, y, w, h) in zip(images, rows, TARGETS):
    moves.append(image.animate.crop(x, y, w, h, normalized=True))
    left, top = center[0] - size / 2, center[1] + size / 2
    moves.append(window.animate.move_to(left + size * (x + w / 2), top - size * (y + h / 2)))
    moves.append(window.animate.scale_to_3d(w, h, 1))
scene.play(moves, duration=1.6)
scene.wait(0.6)

# quality picks how pixels are sampled when an image is drawn larger than
# its source: nearest pixel (low), bilinear (medium, the default) or
# bicubic (high). Here 24 x 24 pixels of the badge fill each frame.
scene.segment("quality")
S = 3.6
page = scene.layout.zones(Zones.rows(["1fr", "10fr"], gap="24px", padding="page_padding"))
code(f"scene.media.image(BADGE, width={S}, height={S}, crop=(244, 120, 24, 24), quality=...)", page[0])
cells = page[1].split(Zones.columns(3, gap="48px"))
for cell, quality in zip(cells, ["low", "medium", "high"]):
    stage, caption = cell.split(Zones.rows(["5fr", "1fr"], gap="12px"))
    code(f'quality="{quality}"', caption, "top")
    scene.media.image(BADGE, width=S, height=S, crop=(244, 120, 24, 24), quality=quality).move_to(*stage.center)
scene.wait(1.0)

# point_ref and region sit on source pixels, read through the frame and the
# current crop every frame: while the crop zooms in, the circles around the
# regions and the connector to the point stay on their detail, not on the
# frame, as they would follow a moving drawable. A notation is hand-drawn
# by default; roughness=0 and a single pass draw clean geometry.
pixels_start = scene.cursor
scene.segment("pixels")
page = scene.layout.zones(Zones.rows(["1fr", "2fr", "1fr", "1fr", "6fr"], gap="12px", padding="page_padding"))
code("image.region(205, 128, 64, 64).annotate.circle()", page[0])
code("image.region(280, 200, 48, 48).annotate.circle(\n    roughness=0, passes=1)", page[1])
code("image.animate.crop(154, 77, 205, 205)", page[2])
label = code("image.point_ref(237, 160)", page[3])
stage = page[4]
size = min(stage.width, stage.height)
image = scene.media.image(BADGE, width=size, height=size).move_to(*stage.center)
outline(stage.center, size, size)
rough = image.region(205, 128, 64, 64).annotate.circle()
clean = image.region(280, 200, 48, 48).annotate.circle(roughness=0, passes=1)
scene.geometry.connector(label.anchor_point("bottom", offset=(0.0, -0.1)), image.point_ref(237, 160))
scene.play([rough.animate.create().duration(0.6), clean.animate.create().duration(0.6)])
scene.play([image.animate.crop(154, 77, 205, 205)], duration=2.0)
scene.wait(0.5)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    # The fits, the crops, the pan and zoom before, during and after, the
    # qualities, and the region and point through the zoom.
    scene.snapshots(snapshots, [0.5, 1.5, 2.2, 3.0, 4.2, 4.8] + [pixels_start + t for t in (0.8, 1.6, 2.9)])
else:
    scene.render()
