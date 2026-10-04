"""Measure end-to-end Gaanim runtime scenarios and compare versioned budgets.

The harness invokes the native executable. It never imports the authoring
wheel as a runtime. Budgets are informational unless ``--enforce`` is passed.
"""

from __future__ import annotations

import argparse
from datetime import datetime, timezone
import json
import math
import os
from pathlib import Path
import platform
import signal
import subprocess
import sys
import time
from typing import Any


SCENARIOS = ("reload", "seek", "preview", "export")
ENCODERS = ("auto", "libx264", "nvenc", "amf", "qsv", "vaapi")
EXPORT_FORMATS = ("mp4", "webm", "webp", "gif", "png", "gaanim")
REPORT_SCHEMA_VERSION = 2
EXPECTED_ENCODERS = {
    "libx264": "libx264",
    "nvenc": "h264_nvenc",
    "amf": "h264_amf",
    "qsv": "h264_qsv",
    "vaapi": "h264_vaapi",
}
POLL_SECONDS = 0.025
EXPORT_TIMING_PREFIX = "GAANIM_EXPORT_TIMINGS "
CAPTURE_TIMING_PREFIX = "GAANIM_CAPTURE_TIMINGS "
PNG_TIMING_PREFIX = "GAANIM_PNG_TIMINGS "
EXPORT_PHASES = (
    "render_gpu_ms",
    "encoder_wait_ms",
    "encode_active_ms",
    "finalize_ms",
    "total_ms",
)
CAPTURE_PHASES = (
    "setup_ms",
    "timeline_update_ms",
    "scene_compile_ms",
    "render_readback_ms",
    "capture_total_ms",
    "png_encode_ms",
)


class BenchmarkFailure(RuntimeError):
    """A benchmark command or configuration was invalid."""


def child_environment() -> dict[str, str]:
    environment = os.environ.copy()
    if os.name == "nt":
        python_base = str(Path(sys.base_prefix))
        entries = environment.get("PATH", "").split(os.pathsep)
        if os.path.normcase(python_base) not in {
            os.path.normcase(entry) for entry in entries if entry
        }:
            environment["PATH"] = os.pathsep.join([python_base, *entries])
    return environment


def percentile(values: list[float], fraction: float) -> float:
    """Return a linearly interpolated percentile for one or more samples."""
    if not values:
        raise ValueError("percentile requires at least one value")
    ordered = sorted(values)
    position = (len(ordered) - 1) * fraction
    lower = math.floor(position)
    upper = math.ceil(position)
    if lower == upper:
        return ordered[lower]
    weight = position - lower
    return ordered[lower] * (1.0 - weight) + ordered[upper] * weight


def linux_process_tree_rss_kib(root_pid: int) -> int | None:
    """Read current RSS for a Linux process and all of its descendants."""
    proc = Path("/proc")
    if not proc.is_dir():
        return None
    parents: dict[int, int] = {}
    rss: dict[int, int] = {}
    for entry in proc.iterdir():
        if not entry.name.isdigit():
            continue
        try:
            fields = {}
            for line in (entry / "status").read_text(encoding="utf-8").splitlines():
                if line.startswith(("PPid:", "VmRSS:")):
                    key, value = line.split(":", 1)
                    fields[key] = value.strip().split()[0]
            pid = int(entry.name)
            parents[pid] = int(fields.get("PPid", "0"))
            rss[pid] = int(fields.get("VmRSS", "0"))
        except (FileNotFoundError, PermissionError, ProcessLookupError, ValueError):
            continue

    descendants = {root_pid}
    changed = True
    while changed:
        changed = False
        for pid, parent in parents.items():
            if parent in descendants and pid not in descendants:
                descendants.add(pid)
                changed = True
    return sum(rss.get(pid, 0) for pid in descendants)


def process_rss_kib(pid: int) -> tuple[int | None, str]:
    if sys.platform.startswith("linux"):
        return linux_process_tree_rss_kib(pid), "process-tree"
    if os.name == "posix":
        result = subprocess.run(
            ["ps", "-o", "rss=", "-p", str(pid)],
            capture_output=True,
            text=True,
            check=False,
        )
        try:
            return int(result.stdout.strip()), "process"
        except ValueError:
            return None, "unavailable"
    return None, "unavailable"


def stop_process(process: subprocess.Popen[bytes]) -> None:
    if process.poll() is not None:
        return
    if os.name == "posix":
        os.killpg(process.pid, signal.SIGKILL)
    else:
        process.kill()


def run_sample(
    command: list[str],
    *,
    cwd: Path,
    environment: dict[str, str],
    timeout_seconds: float,
    log_path: Path,
) -> tuple[float, float | None, str]:
    log_path.parent.mkdir(parents=True, exist_ok=True)
    creation_flags = subprocess.CREATE_NEW_PROCESS_GROUP if os.name == "nt" else 0
    with log_path.open("wb") as log:
        started = time.perf_counter()
        process = subprocess.Popen(
            command,
            cwd=cwd,
            env=environment,
            stdout=log,
            stderr=subprocess.STDOUT,
            start_new_session=os.name == "posix",
            creationflags=creation_flags,
        )
        peak_kib = 0
        memory_scope = "unavailable"
        try:
            while process.poll() is None:
                elapsed = time.perf_counter() - started
                if elapsed > timeout_seconds:
                    stop_process(process)
                    raise BenchmarkFailure(
                        f"command timed out after {timeout_seconds:.0f}s: "
                        f"{subprocess.list2cmdline(command)}"
                    )
                current_kib, sampled_scope = process_rss_kib(process.pid)
                memory_scope = sampled_scope
                if current_kib is not None:
                    peak_kib = max(peak_kib, current_kib)
                time.sleep(POLL_SECONDS)
            return_code = process.wait()
        finally:
            stop_process(process)
        elapsed_ms = (time.perf_counter() - started) * 1000.0

    if return_code != 0:
        output = log_path.read_text(encoding="utf-8", errors="replace")
        raise BenchmarkFailure(
            f"command failed ({return_code}): {subprocess.list2cmdline(command)}\n"
            f"log: {log_path}\n{output[-4000:]}"
        )
    peak_mb = peak_kib / 1024.0 if peak_kib else None
    return elapsed_ms, peak_mb, memory_scope


def scenario_command(
    scenario: str,
    *,
    executable: Path,
    scene: Path,
    artifact_dir: Path,
    encoder: str = "libx264",
    export_format: str = "mp4",
) -> list[str]:
    if scenario == "reload":
        return [
            str(executable),
            "--benchmark-reload",
            str(scene),
            "--output",
            str(artifact_dir / "reload.json"),
        ]
    if scenario in {"seek", "preview"}:
        return [
            str(executable),
            "--diff",
            "--example",
            str(scene),
            "--current",
            str(artifact_dir),
            "--capture-only",
        ]
    if scenario == "export":
        command = [
            str(executable),
            "export",
            str(scene),
            "--output",
            str(artifact_dir / f"benchmark.{export_format}"),
        ]
        # Bundles record lossless frames: quality and encoders do not apply,
        # and only MP4 accepts an explicit encoder.
        if export_format != "gaanim":
            command += ["--quality", "draft"]
        if export_format == "mp4":
            command += ["--encoder", encoder]
        return command
    raise ValueError(f"unknown scenario: {scenario}")


def budget_violations(result: dict[str, Any], budget: dict[str, float]) -> list[str]:
    violations = []
    if result["p95_ms"] > budget["p95_ms"]:
        violations.append(
            f"p95 {result['p95_ms']:.1f}ms exceeds {budget['p95_ms']:.1f}ms"
        )
    peak = result.get("peak_rss_mb")
    if peak is not None and peak > budget["peak_rss_mb"]:
        violations.append(
            f"peak RSS {peak:.1f}MiB exceeds {budget['peak_rss_mb']:.1f}MiB"
        )
    minimum_fps = budget.get("min_fps")
    throughput = result.get("fps_at_p95")
    if minimum_fps is not None and throughput is not None and throughput < minimum_fps:
        violations.append(
            f"throughput {throughput:.2f}fps is below {minimum_fps:.2f}fps"
        )
    return violations


def parse_export_metrics(log_path: Path) -> dict[str, Any]:
    try:
        lines = log_path.read_text(encoding="utf-8", errors="replace").splitlines()
        marker = next(line for line in reversed(lines) if line.startswith(EXPORT_TIMING_PREFIX))
        fields = dict(part.split("=", 1) for part in marker.split()[1:])
        timings = {phase: float(fields[phase]) for phase in EXPORT_PHASES}
        if any(not math.isfinite(value) or value < 0 for value in timings.values()):
            raise ValueError("phase timings must be finite and non-negative")
        encoder = fields["encoder"]
        if not encoder:
            raise ValueError("encoder must not be empty")
        return {"encoder": encoder, "phase_timings_ms": timings}
    except (OSError, KeyError, StopIteration, ValueError) as error:
        raise BenchmarkFailure(
            f"export did not report valid phase timings in {log_path}"
        ) from error


def parse_capture_metrics(log_path: Path) -> dict[str, float]:
    try:
        lines = log_path.read_text(encoding="utf-8", errors="replace").splitlines()
        capture = next(
            line for line in reversed(lines) if line.startswith(CAPTURE_TIMING_PREFIX)
        )
        png = next(line for line in reversed(lines) if line.startswith(PNG_TIMING_PREFIX))
        fields = dict(part.split("=", 1) for part in capture.split()[1:])
        fields.update(dict(part.split("=", 1) for part in png.split()[1:]))
        timings = {phase: float(fields[phase]) for phase in CAPTURE_PHASES}
        if any(not math.isfinite(value) or value < 0 for value in timings.values()):
            raise ValueError("phase timings must be finite and non-negative")
        return timings
    except (OSError, KeyError, StopIteration, ValueError) as error:
        raise BenchmarkFailure(
            f"capture did not report valid phase timings in {log_path}"
        ) from error


def validate_artifacts(
    scenario: str, artifact_dir: Path, frames: int, export_format: str = "mp4"
) -> dict[str, Any] | None:
    if scenario == "reload":
        report_path = artifact_dir / "reload.json"
        try:
            report = json.loads(report_path.read_text(encoding="utf-8"))
            if report.get("schema_version") != 1:
                raise ValueError("unsupported reload report schema")
            for field in ("python_ms", "replay_ms", "total_ms"):
                if not math.isfinite(float(report[field])) or report[field] < 0:
                    raise ValueError(f"invalid {field}")
        except (OSError, KeyError, TypeError, ValueError, json.JSONDecodeError) as error:
            raise BenchmarkFailure(
                f"reload did not produce a valid {report_path}"
            ) from error
        return report
    if scenario in {"seek", "preview"}:
        manifest_path = artifact_dir / "manifest.json"
        try:
            manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
        except (OSError, json.JSONDecodeError) as error:
            raise BenchmarkFailure(
                f"{scenario} did not produce a valid {manifest_path}"
            ) from error
        actual_frames = len(manifest.get("snapshots", []))
        if actual_frames != frames:
            raise BenchmarkFailure(
                f"{scenario} produced {actual_frames} frames; expected {frames}"
            )
        return {"phase_timings_ms": parse_capture_metrics(artifact_dir / "command.log")}
    elif scenario == "export":
        # A PNG sequence writes numbered files next to the requested name.
        outputs = [
            path
            for path in artifact_dir.glob("benchmark*")
            if path.is_file() and path.stat().st_size > 0
        ]
        if not outputs:
            raise BenchmarkFailure(
                f"export did not produce a non-empty benchmark.{export_format} in {artifact_dir}"
            )
        if export_format == "mp4":
            return parse_export_metrics(artifact_dir / "command.log")
        # Other formats report phase timings when their path publishes them.
        try:
            return parse_export_metrics(artifact_dir / "command.log")
        except BenchmarkFailure:
            return {"encoder": None, "phase_timings_ms": None}
    return None


def suite_entries(configuration: dict[str, Any], suite: str) -> list[dict[str, Any]]:
    """Return the scene entries of ``suite`` with their defaults filled in."""
    try:
        entries = configuration["suites"][suite]["scenes"]
    except (KeyError, TypeError) as error:
        raise BenchmarkFailure(f"unknown benchmark suite: {suite}") from error
    resolved = []
    for entry in entries:
        if isinstance(entry, str):
            entry = {"scene": entry}
        scenarios = tuple(entry.get("scenarios", SCENARIOS))
        unknown = set(scenarios) - set(SCENARIOS)
        if unknown:
            raise BenchmarkFailure(f"suite {suite} names unknown scenarios: {sorted(unknown)}")
        resolved.append(
            {
                "scene": str(entry["scene"]),
                "scenarios": scenarios,
                "scales": [int(scale) for scale in entry.get("scales", [1])],
                "budgets": bool(entry.get("budgets", True)),
            }
        )
    return resolved


def run_key(scene: str, scale: int, scenario: str, export_format: str) -> str:
    stem = Path(scene).stem
    suffix = f"/{export_format}" if scenario == "export" else ""
    return f"{stem}@{scale}:{scenario}{suffix}"


def baseline_p50s(report: dict[str, Any]) -> dict[str, float]:
    """Index a schema 2 report by run key, for relative comparison."""
    if report.get("schema_version") != REPORT_SCHEMA_VERSION:
        raise BenchmarkFailure(
            f"baseline report must use schema_version {REPORT_SCHEMA_VERSION}"
        )
    export_format = report.get("export_format", "mp4")
    return {
        run_key(run["scene"], int(run["scale"]), scenario, export_format): float(result["p50_ms"])
        for run in report["runs"]
        for scenario, result in run["scenarios"].items()
    }


def regression_violation(
    current_p50_ms: float, baseline_p50_ms: float, max_regression: float
) -> tuple[float, str | None]:
    """Return the relative change and a violation when it exceeds ``max_regression``."""
    change = (current_p50_ms - baseline_p50_ms) / baseline_p50_ms
    if change > max_regression:
        return change, (
            f"p50 {current_p50_ms:.1f}ms is {change:+.1%} over the baseline "
            f"{baseline_p50_ms:.1f}ms (limit {max_regression:+.0%})"
        )
    return change, None


def measure_scene(
    *,
    repo: Path,
    executable: Path,
    scene: Path,
    scale: int,
    scenarios: tuple[str, ...],
    profile_config: dict[str, Any],
    output_dir: Path,
    encoder: str,
    export_format: str,
    apply_budgets: bool,
    baseline: dict[str, float],
    max_regression: float,
) -> tuple[dict[str, Any], bool]:
    """Measure every scenario of one scene at one scale."""
    results: dict[str, Any] = {}
    any_violations = False
    for scenario in scenarios:
        scenario_config = profile_config["scenarios"][scenario]
        samples = int(scenario_config["samples"])
        warmups = int(profile_config["warmups"])
        frames = int(scenario_config["frames"])
        timings = []
        process_timings = []
        peak_rss_values = []
        memory_scope = "unavailable"
        export_phase_samples = {phase: [] for phase in EXPORT_PHASES}
        capture_phase_samples = {phase: [] for phase in CAPTURE_PHASES}
        export_encoder = None

        for sample_index in range(warmups + samples):
            is_warmup = sample_index < warmups
            run_index = sample_index if is_warmup else sample_index - warmups
            run_kind = "warmup" if is_warmup else "sample"
            artifact_dir = (
                output_dir / "artifacts" / scene.stem / f"scale-{scale}" / scenario
                / f"{run_kind}-{run_index:02}"
            )
            environment = child_environment()
            environment["GAANIM_BENCHMARK_SCENARIO"] = scenario
            environment["GAANIM_BENCHMARK_FRAMES"] = str(frames)
            environment["GAANIM_BENCHMARK_SCALE"] = str(scale)
            if scenario in {"seek", "preview"}:
                environment["GAANIM_CAPTURE_TELEMETRY"] = "1"
            command = scenario_command(
                scenario,
                executable=executable,
                scene=scene,
                artifact_dir=artifact_dir,
                encoder=encoder,
                export_format=export_format,
            )
            elapsed_ms, peak_rss_mb, sampled_scope = run_sample(
                command,
                cwd=repo,
                environment=environment,
                timeout_seconds=float(scenario_config["timeout_seconds"]) * scale,
                log_path=artifact_dir / "command.log",
            )
            artifact_report = validate_artifacts(scenario, artifact_dir, frames, export_format)
            if scenario == "export" and export_format == "mp4" and artifact_report is not None:
                actual_encoder = artifact_report["encoder"]
                expected_encoder = EXPECTED_ENCODERS.get(encoder)
                if expected_encoder is not None and actual_encoder != expected_encoder:
                    raise BenchmarkFailure(
                        f"export requested {encoder} but reported {actual_encoder}"
                    )
            memory_scope = sampled_scope
            if is_warmup:
                continue
            if scenario == "export" and artifact_report is not None:
                sample_encoder = artifact_report["encoder"]
                if export_encoder is not None and sample_encoder != export_encoder:
                    raise BenchmarkFailure(
                        f"export encoder changed between samples: "
                        f"{export_encoder} -> {sample_encoder}"
                    )
                export_encoder = sample_encoder
                for phase, value in (artifact_report["phase_timings_ms"] or {}).items():
                    export_phase_samples[phase].append(float(value))
            if scenario in {"seek", "preview"} and artifact_report is not None:
                for phase, value in artifact_report["phase_timings_ms"].items():
                    capture_phase_samples[phase].append(float(value))
            process_timings.append(elapsed_ms)
            timings.append(
                float(artifact_report["total_ms"])
                if scenario == "reload" and artifact_report is not None
                else elapsed_ms
            )
            if peak_rss_mb is not None:
                peak_rss_values.append(peak_rss_mb)

        p50_ms = percentile(timings, 0.50)
        p95_ms = percentile(timings, 0.95)
        counts_frames = scenario != "reload"
        result: dict[str, Any] = {
            "samples": samples,
            "warmups": warmups,
            "frames_per_sample": frames if counts_frames else None,
            "timings_ms": [round(value, 3) for value in timings],
            "process_timings_ms": [round(value, 3) for value in process_timings]
            if scenario == "reload"
            else None,
            "p50_ms": round(p50_ms, 3),
            "p95_ms": round(p95_ms, 3),
            "ms_per_frame_p50": round(p50_ms / frames, 3) if counts_frames else None,
            "peak_rss_mb": round(max(peak_rss_values), 3)
            if peak_rss_values
            else None,
            "memory_scope": memory_scope,
            "fps_at_p50": round(frames / (p50_ms / 1000.0), 3) if counts_frames else None,
            "fps_at_p95": round(frames / (p95_ms / 1000.0), 3) if counts_frames else None,
            "budget": scenario_config["budget"] if apply_budgets else None,
        }
        phase_samples = (
            export_phase_samples if scenario == "export"
            else capture_phase_samples if scenario in {"seek", "preview"}
            else {}
        )
        if scenario == "export":
            result["requested_encoder"] = encoder if export_format == "mp4" else None
            result["export_format"] = export_format
            result["encoder"] = export_encoder
        if phase_samples and all(phase_samples.values()):
            result["phases"] = {
                phase: {
                    "timings_ms": [round(value, 3) for value in values],
                    "p50_ms": round(percentile(values, 0.50), 3),
                    "p95_ms": round(percentile(values, 0.95), 3),
                }
                for phase, values in phase_samples.items()
            }
        violations = budget_violations(result, result["budget"]) if apply_budgets else []
        key = run_key(str(scene), scale, scenario, export_format)
        if key in baseline:
            change, violation = regression_violation(p50_ms, baseline[key], max_regression)
            result["baseline_p50_ms"] = baseline[key]
            result["change_vs_baseline"] = round(change, 4)
            if violation:
                violations.append(violation)
        result["violations"] = violations
        result["status"] = "warning" if violations else "pass"
        any_violations |= bool(violations)
        results[scenario] = result
        throughput = (
            f" fps@p95={result['fps_at_p95']:.2f} ms/frame={result['ms_per_frame_p50']:.1f}"
            if result["fps_at_p95"] is not None
            else ""
        )
        encoder_note = (
            f" encoder={result['encoder']}"
            if scenario == "export" and result.get("encoder")
            else ""
        )
        comparison = (
            f" vs-baseline={result['change_vs_baseline']:+.1%}"
            if "change_vs_baseline" in result
            else ""
        )
        print(
            f"{scene.stem}@{scale} {scenario}: p50={result['p50_ms']:.1f}ms "
            f"p95={result['p95_ms']:.1f}ms "
            f"RSS={result['peak_rss_mb'] or 0:.1f}MiB{throughput}{encoder_note}{comparison} "
            f"[{result['status']}]"
        )
    return results, any_violations


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--executable", type=Path)
    parser.add_argument(
        "--scene", type=Path, default=Path("examples/performance_benchmark.py")
    )
    parser.add_argument(
        "--suite",
        help="Run the scenes of a suite from the budgets file instead of --scene.",
    )
    parser.add_argument(
        "--scales",
        nargs="+",
        type=int,
        help="GAANIM_BENCHMARK_SCALE values to run (default: the suite's, or 1).",
    )
    parser.add_argument(
        "--budgets", type=Path, default=Path("tests/performance/budgets.json")
    )
    parser.add_argument("--output", type=Path, default=Path("target/performance"))
    parser.add_argument("--profile", choices=("smoke", "standard"), default="smoke")
    parser.add_argument("--scenarios", nargs="+", choices=SCENARIOS)
    parser.add_argument("--encoder", choices=ENCODERS, default="libx264")
    parser.add_argument("--export-format", choices=EXPORT_FORMATS, default="mp4")
    parser.add_argument(
        "--compare",
        type=Path,
        help="A previous runtime-benchmark.json (schema 2) to compare p50 against.",
    )
    parser.add_argument(
        "--max-regression",
        type=float,
        default=0.10,
        help="Relative p50 slowdown against --compare that counts as a violation.",
    )
    parser.add_argument("--enforce", action="store_true")
    return parser.parse_args()


def resolve(repo: Path, path: Path) -> Path:
    return path if path.is_absolute() else (repo / path).resolve()


def main() -> int:
    args = parse_args()
    repo = Path(__file__).resolve().parents[1]
    executable = args.executable or repo / "target" / "release" / (
        "gaanim.exe" if os.name == "nt" else "gaanim"
    )
    executable = executable.resolve()
    budgets_path = resolve(repo, args.budgets)
    output_dir = resolve(repo, args.output)

    if not executable.is_file():
        print(f"Gaanim executable does not exist: {executable}", file=sys.stderr)
        return 2

    try:
        configuration = json.loads(budgets_path.read_text(encoding="utf-8"))
        if configuration.get("schema_version") != 1:
            raise BenchmarkFailure("unsupported performance budget schema")
        profile_config = configuration["profiles"][args.profile]
        if args.suite:
            entries = suite_entries(configuration, args.suite)
        else:
            entries = [
                {"scene": str(args.scene), "scenarios": SCENARIOS, "scales": [1], "budgets": True}
            ]
        baseline = (
            baseline_p50s(json.loads(resolve(repo, args.compare).read_text(encoding="utf-8")))
            if args.compare
            else {}
        )
    except (OSError, KeyError, TypeError, ValueError, json.JSONDecodeError, BenchmarkFailure) as error:
        print(f"Invalid benchmark configuration: {error}", file=sys.stderr)
        return 2

    for entry in entries:
        scene = resolve(repo, Path(entry["scene"]))
        if not scene.is_file():
            print(f"Benchmark scene does not exist: {scene}", file=sys.stderr)
            return 2

    output_dir.mkdir(parents=True, exist_ok=True)
    runs = []
    any_violations = False
    try:
        for entry in entries:
            scene = resolve(repo, Path(entry["scene"]))
            scenarios = tuple(
                scenario for scenario in entry["scenarios"]
                if args.scenarios is None or scenario in args.scenarios
            )
            for scale in args.scales or entry["scales"]:
                if scale < 1:
                    raise BenchmarkFailure(f"scale must be at least 1, got {scale}")
                results, violated = measure_scene(
                    repo=repo,
                    executable=executable,
                    scene=scene,
                    scale=scale,
                    scenarios=scenarios,
                    profile_config=profile_config,
                    output_dir=output_dir,
                    encoder=args.encoder,
                    export_format=args.export_format,
                    apply_budgets=entry["budgets"],
                    baseline=baseline,
                    max_regression=args.max_regression,
                )
                any_violations |= violated
                runs.append(
                    {
                        "scene": entry["scene"],
                        "scale": scale,
                        "scenarios": results,
                    }
                )
    except (OSError, KeyError, TypeError, ValueError, BenchmarkFailure) as error:
        print(f"Runtime benchmark failed: {error}", file=sys.stderr)
        return 1

    report = {
        "schema_version": REPORT_SCHEMA_VERSION,
        "generated_at": datetime.now(timezone.utc).isoformat(),
        "profile": args.profile,
        "suite": args.suite,
        "platform": {
            "system": platform.system(),
            "release": platform.release(),
            "machine": platform.machine(),
            "python": platform.python_version(),
        },
        "executable": str(executable),
        "requested_encoder": args.encoder,
        "export_format": args.export_format,
        "baseline": str(args.compare) if args.compare else None,
        "max_regression": args.max_regression if args.compare else None,
        "enforced": args.enforce,
        "runs": runs,
    }
    report_path = output_dir / "runtime-benchmark.json"
    report_path.write_text(json.dumps(report, indent=2), encoding="utf-8")
    print(f"Performance report: {report_path}")
    if any_violations and args.enforce:
        print("Performance budget exceeded", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
