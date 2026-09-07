"""Native media API E2E checks: framing snapshots, finite video, and audio gaps.

Build the current host first, then run with
`just dev-exec python tests/validate_media_api.py`.
"""

from array import array
import json
import math
import os
from pathlib import Path
import subprocess


ROOT = Path(__file__).resolve().parents[1]
OUTPUT = ROOT / "target/media-api-validation"
HOST = ROOT / "target/debug" / ("gaanim-core.exe" if os.name == "nt" else "gaanim-core")
SCENE = ROOT / "tests/media_api_contract.py"


def run(args, *, env=None):
    result = subprocess.run(args, cwd=ROOT, env=env, capture_output=True, timeout=300)
    if result.returncode:
        raise AssertionError(result.stdout.decode(errors="replace") + result.stderr.decode(errors="replace"))
    return result.stdout


def rgb(path, time=None, size="1:1"):
    args = ["ffmpeg", "-v", "error"]
    if time is not None:
        args += ["-ss", str(time)]
    return run(args + ["-i", str(path), "-vf", f"scale={size}:flags=area", "-frames:v", "1", "-f", "rawvideo", "-pix_fmt", "rgb24", "pipe:1"])


def rms(path, start):
    samples = array("f")
    samples.frombytes(run([
        "ffmpeg", "-v", "error", "-ss", str(start), "-i", str(path),
        "-t", "0.1", "-vn", "-ac", "1", "-ar", "48000", "-f", "f32le", "pipe:1",
    ]))
    assert samples
    return math.sqrt(sum(x * x for x in samples) / len(samples))


def main():
    OUTPUT.mkdir(parents=True, exist_ok=True)
    report = {}
    for kind in ["mp4", "webm"]:
        artifact = OUTPUT / f"segments.{kind}"
        args = [str(HOST), "export", str(SCENE), "--output", str(artifact), "--quality", "draft", "--width", "320", "--height", "180"]
        if kind == "mp4":
            args += ["--encoder", "libx264"]
        run(args)
        probe = json.loads(run(["ffprobe", "-v", "error", "-show_streams", "-show_format", "-of", "json", str(artifact)]))
        assert {s["codec_type"] for s in probe["streams"]} >= {"video", "audio"}
        assert abs(float(probe["format"]["duration"]) - 2.9) < 0.2
        for time, channel in [(0.7, 0), (1.1, 0), (1.7, 2), (2.7, 2)]:
            pixel = rgb(artifact, time)
            assert len(pixel) == 3 and pixel[channel] > 180, (time, list(pixel))
            assert all(pixel[i] < 40 for i in range(3) if i != channel), (time, list(pixel))
        levels = {str(t): rms(artifact, t) for t in [0.2, 0.6, 1.1, 1.7, 2.6]}
        for t in [0.2, 1.1, 2.6]:
            assert levels[str(t)] < 0.003, (t, levels)
        for t in [0.6, 1.7]:
            assert levels[str(t)] > 0.02, (t, levels)
        report[kind] = {"duration": probe["format"]["duration"], "audio_rms": levels}

    snapshots = OUTPUT / "crop"
    environment = dict(os.environ, GAANIM_MEDIA_CROP="1", GAANIM_SNAPSHOTS=str(snapshots))
    run([str(HOST), "--diff", "--example", str(SCENE), "--current", str(snapshots), "--capture-only", "--no-gui"], env=environment)
    frames = sorted(snapshots.glob("*.png"))
    assert len(frames) >= 3, "crop snapshots were not captured"
    initial = [p for p in frames if "t_0_000000" in p.name]
    final = [p for p in frames if "t_1_800000" in p.name]
    resized = [p for p in frames if "t_1_600000" in p.name]
    assert initial and final, [p.name for p in frames]
    pixels = rgb(initial[0], size="2:2")
    assert pixels[0] > 200 and pixels[5] > 200 and pixels[7] > 200, list(pixels)
    pixel = rgb(final[0])
    assert pixel[0] > 230 and pixel[1] > 230 and pixel[2] < 20, list(pixel)
    assert resized
    pixel = rgb(resized[0])
    assert 50 < pixel[0] < 80 and 50 < pixel[1] < 80 and pixel[2] < 20, list(pixel)
    if len(initial) > 1:
        assert rgb(initial[0], size="32:18") == rgb(initial[-1], size="32:18")
    report["crop_snapshots"] = [str(p.relative_to(ROOT)) for p in frames]
    (OUTPUT / "report.json").write_text(json.dumps(report, indent=2), encoding="utf-8")
    print("Media API contracts, MP4/WebM frames/audio gaps, and crop snapshots passed.")


if __name__ == "__main__":
    main()
