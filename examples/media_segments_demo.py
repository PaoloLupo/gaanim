"""Local media: fixed framing, animated crop, and finite video segments.

Requires FFmpeg. Generates deterministic local assets in target/media-demo.
Run with `just run media_segments_demo`.
"""

import os
from pathlib import Path
import struct
import subprocess
import zlib

from gaanim import Scene


def make_assets(directory):
    directory = Path(directory)
    directory.mkdir(parents=True, exist_ok=True)
    image = directory / "quadrants.png"
    video = directory / "colors.mp4"
    if not image.exists():
        def chunk(kind, data):
            return struct.pack(">I", len(data)) + kind + data + struct.pack(">I", zlib.crc32(kind + data))

        colors = [(255, 0, 0), (0, 0, 255), (0, 255, 0), (255, 255, 0)]
        pixels = b"".join(
            b"\0" + b"".join(bytes(colors[(y // 90) * 2 + x // 160]) for x in range(320))
            for y in range(180)
        )
        image.write_bytes(
            b"\x89PNG\r\n\x1a\n"
            + chunk(b"IHDR", struct.pack(">IIBBBBB", 320, 180, 8, 2, 0, 0, 0))
            + chunk(b"IDAT", zlib.compress(pixels)) + chunk(b"IEND", b"")
        )
    if not video.exists():
        subprocess.run([
            "ffmpeg", "-v", "error", "-y",
            "-f", "lavfi", "-i", "color=red:s=320x180:r=30:d=1",
            "-f", "lavfi", "-i", "color=blue:s=320x180:r=30:d=1",
            "-f", "lavfi", "-i", "color=green:s=320x180:r=30:d=1",
            "-f", "lavfi", "-i", "sine=frequency=440:sample_rate=48000:duration=3",
            "-filter_complex", "[0:v][1:v][2:v]concat=n=3:v=1:a=0[v]",
            "-map", "[v]", "-map", "3:a", "-c:v", "libx264", "-pix_fmt", "yuv420p",
            "-c:a", "aac", "-t", "3", str(video),
        ], check=True)
    return image, video


def build_scene():
    root = Path(__file__).resolve().parents[1]
    image_path, video_path = make_assets(root / "target" / "media-demo")
    scene = Scene(frame=(16, 9), background="#0f172a")
    image = scene.media.image(str(image_path)).frame(7, 4, fit="cover").move_to(-4, 0)
    video = scene.media.video(str(video_path)).frame(7, 4, fit="cover").move_to(4, 0)
    scene.play([image.animate.crop(0, 0, 0.5, 0.5, normalized=True).duration(1)])
    scene.play([video.segment(start=0, end=1, speed=2)])
    scene.wait(0.5)
    scene.play([
        video.segment(start=1, end=2),
        image.animate.crop(0.5, 0.5, 0.5, 0.5, normalized=True).duration(1),
    ])
    scene.play([video.segment(start=0, end=1, speed=2)])
    scene.wait(0.5)
    return scene


if __name__ == "__main__":
    scene = build_scene()
    if "GAANIM_SNAPSHOTS" in os.environ:
        scene.snapshots(os.environ["GAANIM_SNAPSHOTS"], [0, 0.5, 1, 1.75, 2.5, 3.5])
    scene.render()
