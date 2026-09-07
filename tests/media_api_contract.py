"""Run inside the Gaanim host, including native media API and snapshot checks."""

import os
from pathlib import Path
import runpy

from gaanim import Drawable, Image, Scene, Video, VideoSegment, parallel, sequence

ROOT = Path(__file__).resolve().parents[1]
make_assets = runpy.run_path(str(ROOT / "examples/media_segments_demo.py"))["make_assets"]
image_path, video_path = make_assets(ROOT / "target/media-demo")


def invalid(call):
    try:
        call()
    except ValueError:
        return
    raise AssertionError("expected ValueError")


def contracts():
    scene = Scene(frame=(16, 9))
    image = scene.media.image(str(image_path))
    video = scene.media.video(str(video_path), speed=2, volume=0.5)
    assert isinstance(image, (Image, Drawable))
    assert isinstance(video, (Video, Drawable))
    for obj, cls in [(image, Image), (video, Video)]:
        assert obj.frame(8, 4.5).move_to(0, 0).scale_to(1).rotate_to(0).opacity(1) is obj
        assert obj.crop(0, 0, 1, 1, normalized=True).quality("high") is obj
        assert isinstance(obj, cls)
        assert (obj.source_width, obj.source_height) == (320, 180)
        invalid(lambda: obj.frame(0, 1))
        invalid(lambda: obj.crop(0.5, 0, 1, 1, normalized=True))
        invalid(lambda: obj.animate.crop(float("nan"), 0, 1, 1))
        invalid(lambda: obj.quality("unknown"))
    assert abs(video.source_duration - 3) < 0.05
    assert video.frame_rate == 30
    invalid(lambda: scene.geometry.rect(1, 1).animate.crop(0, 0, 1, 1))
    a = video.segment(start=0.1, end=0.9)
    b = video.segment(start=1, end=2, speed=1)
    assert isinstance(a, VideoSegment)
    invalid(lambda: scene.play([a, b]))
    invalid(lambda: parallel(a).stretch(4))
    invalid(lambda: Scene().play([a]))
    invalid(lambda: scene.play([a, video]))
    scene.play([sequence(a, b)])
    invalid(lambda: scene.play([a]))
    invalid(lambda: scene.play([video]))
    scene.play([video.segment(start=0.1, end=0.9, audio=False)])


contracts()

scene = Scene(frame=(16, 9), background="black")
if os.environ.get("GAANIM_MEDIA_CROP"):
    obj = scene.media.image(str(image_path)).frame(16, 9, fit="cover").quality("low")
    scene.wait(0.5)
    scene.play([obj.animate.crop(0, 0, 0.5, 0.5, normalized=True).duration(1)])
    obj.crop(0.5, 0.5, 0.5, 0.5, normalized=True).frame(8, 4.5).quality("high")
    scene.wait(0.2)
    obj.frame(16, 9)
    scene.wait(0.3)
else:
    obj = scene.media.video(str(video_path)).frame(16, 9)
    scene.wait(0.5)
    scene.play([obj.segment(start=0.1, end=0.9, speed=2)])
    scene.wait(0.5)
    scene.play([obj.segment(start=1, end=2)])
    scene.wait(0.5)
if "GAANIM_SNAPSHOTS" in os.environ:
    times = [1.8, 0.0, 1.0, 1.6, 0.0, 1.8] if os.environ.get("GAANIM_MEDIA_CROP") else [2.7, 0.2, 0.7, 1.1, 1.7, 0.7]
    scene.snapshots(os.environ["GAANIM_SNAPSHOTS"], times)
scene.render()
