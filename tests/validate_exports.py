"""Export and inspect every supported Gaanim output format.

This is an executable E2E smoke test. It intentionally invokes the Gaanim
runtime rather than importing the authoring wheel with plain Python.
"""

from __future__ import annotations

import argparse
from fractions import Fraction
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
from typing import Any


VIDEO_FORMATS = {
    "mp4": {"video": "h264", "audio": "aac"},
    "webm": {"video": "vp9", "audio": "opus"},
    "webp": {"video": "webp_anim"},
    "gif": {"video": "gif"},
}
ALL_FORMATS = (*VIDEO_FORMATS, "png")
WIDTH = 320
HEIGHT = 180
MIN_DURATION = 0.45
MAX_DURATION = 1.0
COMMAND_TIMEOUT_SECONDS = 120


class SmokeFailure(RuntimeError):
    """An exported artifact did not satisfy its public contract."""


def run(command: list[str], *, cwd: Path, env: dict[str, str] | None = None) -> str:
    try:
        result = subprocess.run(
            command,
            cwd=cwd,
            env=env,
            capture_output=True,
            text=True,
            check=False,
            timeout=COMMAND_TIMEOUT_SECONDS,
        )
    except subprocess.TimeoutExpired as error:
        rendered = subprocess.list2cmdline(command)
        raise SmokeFailure(
            f"Command timed out after {COMMAND_TIMEOUT_SECONDS}s: {rendered}"
        ) from error
    if result.returncode != 0:
        rendered = subprocess.list2cmdline(command)
        raise SmokeFailure(
            f"Command failed ({result.returncode}): {rendered}\n"
            f"stdout:\n{result.stdout}\nstderr:\n{result.stderr}"
        )
    return result.stdout


def ffprobe(path: Path, *, cwd: Path) -> dict[str, Any]:
    output = run(
        [
            "ffprobe",
            "-v",
            "error",
            "-count_frames",
            "-show_entries",
            "stream=index,codec_type,codec_name,width,height,nb_read_frames,r_frame_rate",
            "-show_entries",
            "format=duration",
            "-of",
            "json",
            str(path),
        ],
        cwd=cwd,
    )
    return json.loads(output)


def require_stream(
    probe: dict[str, Any], codec_type: str, codec_name: str, artifact: Path
) -> dict[str, Any]:
    matches = [
        stream
        for stream in probe.get("streams", [])
        if stream.get("codec_type") == codec_type
    ]
    if not matches:
        raise SmokeFailure(f"{artifact} has no {codec_type} stream")
    stream = matches[0]
    if stream.get("codec_name") != codec_name:
        raise SmokeFailure(
            f"{artifact} uses {stream.get('codec_name')} instead of {codec_name}"
        )
    return stream


def validate_dimensions(stream: dict[str, Any], artifact: Path) -> None:
    actual = (stream.get("width"), stream.get("height"))
    if actual != (WIDTH, HEIGHT):
        raise SmokeFailure(f"{artifact} has dimensions {actual}, expected {(WIDTH, HEIGHT)}")


def validate_duration(probe: dict[str, Any], artifact: Path) -> None:
    raw = probe.get("format", {}).get("duration")
    if raw not in (None, "N/A"):
        duration = float(raw)
    else:
        video = next(
            (
                stream
                for stream in probe.get("streams", [])
                if stream.get("codec_type") == "video"
            ),
            None,
        )
        frames = video and video.get("nb_read_frames")
        rate = video and video.get("r_frame_rate")
        if not frames or frames == "N/A" or not rate or rate in {"0/0", "N/A"}:
            raise SmokeFailure(f"{artifact} does not report a duration or frame rate")
        duration = int(frames) / float(Fraction(rate))
    if not MIN_DURATION <= duration <= MAX_DURATION:
        raise SmokeFailure(
            f"{artifact} duration is {duration:.6f}s, expected "
            f"{MIN_DURATION:.2f}..{MAX_DURATION:.2f}s"
        )


def generate_audio_fixture(output: Path, *, cwd: Path) -> None:
    run(
        [
            "ffmpeg",
            "-y",
            "-v",
            "error",
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=440:sample_rate=48000:duration=0.6",
            "-c:a",
            "pcm_s16le",
            str(output),
        ],
        cwd=cwd,
    )


def export_format(
    executable: Path,
    scene: Path,
    output_dir: Path,
    format_name: str,
    audio_fixture: Path,
    *,
    repo: Path,
) -> dict[str, Any]:
    artifact = output_dir / f"smoke.{format_name}"
    if format_name == "png":
        for stale_frame in output_dir.glob("smoke_*.png"):
            stale_frame.unlink()
    else:
        artifact.unlink(missing_ok=True)
    environment = os.environ.copy()
    if format_name in {"mp4", "webm"}:
        environment["GAANIM_EXPORT_SMOKE_AUDIO"] = str(audio_fixture)
    else:
        environment.pop("GAANIM_EXPORT_SMOKE_AUDIO", None)

    run(
        [
            str(executable),
            "export",
            str(scene),
            "--output",
            str(artifact),
            "--quality",
            "draft",
        ],
        cwd=repo,
        env=environment,
    )

    if format_name == "png":
        frames = sorted(output_dir.glob("smoke_*.png"))
        if len(frames) < 2:
            raise SmokeFailure(f"PNG sequence contains only {len(frames)} frame(s)")
        probe = ffprobe(frames[0], cwd=repo)
        stream = require_stream(probe, "video", "png", frames[0])
        validate_dimensions(stream, frames[0])
        return {"format": format_name, "frames": len(frames), "first": str(frames[0])}

    if not artifact.is_file() or artifact.stat().st_size == 0:
        raise SmokeFailure(f"Export did not create a non-empty {artifact}")
    probe = ffprobe(artifact, cwd=repo)
    expected = VIDEO_FORMATS[format_name]
    video = require_stream(probe, "video", expected["video"], artifact)
    validate_dimensions(video, artifact)
    validate_duration(probe, artifact)
    if audio_codec := expected.get("audio"):
        require_stream(probe, "audio", audio_codec, artifact)
    return {
        "format": format_name,
        "artifact": str(artifact),
        "bytes": artifact.stat().st_size,
        "probe": probe,
    }


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--executable", type=Path)
    parser.add_argument("--scene", type=Path, default=Path("examples/export_smoke.py"))
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--formats", nargs="+", choices=ALL_FORMATS, default=ALL_FORMATS)
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    repo = Path(__file__).resolve().parents[1]
    executable = (
        args.executable.resolve()
        if args.executable
        else repo
        / "target"
        / "debug"
        / ("gaanim-core.exe" if os.name == "nt" else "gaanim-core")
    )
    scene = (repo / args.scene).resolve() if not args.scene.is_absolute() else args.scene
    output_dir = args.output.resolve()

    if not executable.is_file():
        print(f"Gaanim executable does not exist: {executable}", file=sys.stderr)
        return 2
    for dependency in ("ffmpeg", "ffprobe"):
        if shutil.which(dependency) is None:
            print(f"Required export dependency is unavailable: {dependency}", file=sys.stderr)
            return 2

    output_dir.mkdir(parents=True, exist_ok=True)
    audio_fixture = output_dir / "tone.wav"
    try:
        generate_audio_fixture(audio_fixture, cwd=repo)
        reports = [
            export_format(
                executable,
                scene,
                output_dir,
                format_name,
                audio_fixture,
                repo=repo,
            )
            for format_name in args.formats
        ]
    except (OSError, ValueError, json.JSONDecodeError, SmokeFailure) as error:
        print(f"Export smoke failed: {error}", file=sys.stderr)
        return 1

    report_path = output_dir / "export-smoke-report.json"
    report_path.write_text(json.dumps(reports, indent=2), encoding="utf-8")
    print(f"Export smoke passed for {', '.join(args.formats)}: {report_path}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
