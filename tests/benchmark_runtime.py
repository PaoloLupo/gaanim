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
POLL_SECONDS = 0.025
EXPORT_TIMING_PREFIX = "GAANIM_EXPORT_TIMINGS "
EXPORT_PHASES = (
    "render_gpu_ms",
    "encoder_wait_ms",
    "encode_active_ms",
    "finalize_ms",
    "total_ms",
)


class BenchmarkFailure(RuntimeError):
    """A benchmark command or configuration was invalid."""


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
            "--no-gui",
        ]
    if scenario == "export":
        return [
            str(executable),
            "export",
            str(scene),
            "--output",
            str(artifact_dir / "benchmark.mp4"),
            "--quality",
            "draft",
        ]
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


def parse_export_timings(log_path: Path) -> dict[str, float]:
    try:
        lines = log_path.read_text(encoding="utf-8", errors="replace").splitlines()
        marker = next(line for line in reversed(lines) if line.startswith(EXPORT_TIMING_PREFIX))
        fields = dict(part.split("=", 1) for part in marker.split()[1:])
        timings = {phase: float(fields[phase]) for phase in EXPORT_PHASES}
        if any(not math.isfinite(value) or value < 0 for value in timings.values()):
            raise ValueError("phase timings must be finite and non-negative")
        return timings
    except (OSError, KeyError, StopIteration, ValueError) as error:
        raise BenchmarkFailure(
            f"export did not report valid phase timings in {log_path}"
        ) from error


def validate_artifacts(
    scenario: str, artifact_dir: Path, frames: int
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
    elif scenario == "export":
        artifact = artifact_dir / "benchmark.mp4"
        if not artifact.is_file() or artifact.stat().st_size == 0:
            raise BenchmarkFailure(f"export did not produce a non-empty {artifact}")
        return {"phase_timings_ms": parse_export_timings(artifact_dir / "command.log")}
    return None


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--executable", type=Path)
    parser.add_argument(
        "--scene", type=Path, default=Path("examples/performance_benchmark.py")
    )
    parser.add_argument(
        "--budgets", type=Path, default=Path("tests/performance/budgets.json")
    )
    parser.add_argument("--output", type=Path, default=Path("target/performance"))
    parser.add_argument("--profile", choices=("smoke", "standard"), default="smoke")
    parser.add_argument("--scenarios", nargs="+", choices=SCENARIOS, default=SCENARIOS)
    parser.add_argument("--enforce", action="store_true")
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    repo = Path(__file__).resolve().parents[1]
    executable = args.executable or repo / "target" / "release" / (
        "gaanim-core.exe" if os.name == "nt" else "gaanim-core"
    )
    executable = executable.resolve()
    scene = (repo / args.scene).resolve() if not args.scene.is_absolute() else args.scene
    budgets_path = (
        (repo / args.budgets).resolve() if not args.budgets.is_absolute() else args.budgets
    )
    output_dir = (
        (repo / args.output).resolve() if not args.output.is_absolute() else args.output
    )

    if not executable.is_file():
        print(f"Gaanim executable does not exist: {executable}", file=sys.stderr)
        return 2
    if not scene.is_file():
        print(f"Benchmark scene does not exist: {scene}", file=sys.stderr)
        return 2

    try:
        configuration = json.loads(budgets_path.read_text(encoding="utf-8"))
        if configuration.get("schema_version") != 1:
            raise BenchmarkFailure("unsupported performance budget schema")
        profile_config = configuration["profiles"][args.profile]
    except (OSError, KeyError, TypeError, json.JSONDecodeError, BenchmarkFailure) as error:
        print(f"Invalid benchmark configuration: {error}", file=sys.stderr)
        return 2

    output_dir.mkdir(parents=True, exist_ok=True)
    results: dict[str, Any] = {}
    any_violations = False
    try:
        for scenario in args.scenarios:
            scenario_config = profile_config["scenarios"][scenario]
            samples = int(scenario_config["samples"])
            warmups = int(profile_config["warmups"])
            frames = int(scenario_config["frames"])
            timings = []
            process_timings = []
            peak_rss_values = []
            memory_scope = "unavailable"
            export_phase_samples = {phase: [] for phase in EXPORT_PHASES}

            for sample_index in range(warmups + samples):
                is_warmup = sample_index < warmups
                run_index = sample_index if is_warmup else sample_index - warmups
                run_kind = "warmup" if is_warmup else "sample"
                artifact_dir = output_dir / "artifacts" / scenario / f"{run_kind}-{run_index:02}"
                environment = os.environ.copy()
                environment["GAANIM_BENCHMARK_SCENARIO"] = scenario
                environment["GAANIM_BENCHMARK_FRAMES"] = str(frames)
                command = scenario_command(
                    scenario,
                    executable=executable,
                    scene=scene,
                    artifact_dir=artifact_dir,
                )
                elapsed_ms, peak_rss_mb, sampled_scope = run_sample(
                    command,
                    cwd=repo,
                    environment=environment,
                    timeout_seconds=float(scenario_config["timeout_seconds"]),
                    log_path=artifact_dir / "command.log",
                )
                artifact_report = validate_artifacts(scenario, artifact_dir, frames)
                memory_scope = sampled_scope
                if is_warmup:
                    continue
                if scenario == "export" and artifact_report is not None:
                    for phase, value in artifact_report["phase_timings_ms"].items():
                        export_phase_samples[phase].append(float(value))
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
            result: dict[str, Any] = {
                "samples": samples,
                "warmups": warmups,
                "frames_per_sample": frames if scenario != "reload" else None,
                "timings_ms": [round(value, 3) for value in timings],
                "process_timings_ms": [round(value, 3) for value in process_timings]
                if scenario == "reload"
                else None,
                "p50_ms": round(p50_ms, 3),
                "p95_ms": round(p95_ms, 3),
                "peak_rss_mb": round(max(peak_rss_values), 3)
                if peak_rss_values
                else None,
                "memory_scope": memory_scope,
                "fps_at_p50": round(frames / (p50_ms / 1000.0), 3)
                if scenario != "reload"
                else None,
                "fps_at_p95": round(frames / (p95_ms / 1000.0), 3)
                if scenario != "reload"
                else None,
                "budget": scenario_config["budget"],
            }
            if scenario == "export":
                result["phases"] = {
                    phase: {
                        "timings_ms": [round(value, 3) for value in values],
                        "p50_ms": round(percentile(values, 0.50), 3),
                        "p95_ms": round(percentile(values, 0.95), 3),
                    }
                    for phase, values in export_phase_samples.items()
                }
            result["violations"] = budget_violations(result, result["budget"])
            result["status"] = "warning" if result["violations"] else "pass"
            any_violations |= bool(result["violations"])
            results[scenario] = result
            throughput = (
                f" fps@p95={result['fps_at_p95']:.2f}"
                if result["fps_at_p95"] is not None
                else ""
            )
            print(
                f"{scenario}: p50={result['p50_ms']:.1f}ms "
                f"p95={result['p95_ms']:.1f}ms "
                f"RSS={result['peak_rss_mb'] or 0:.1f}MiB{throughput} "
                f"[{result['status']}]"
            )
    except (OSError, KeyError, TypeError, ValueError, BenchmarkFailure) as error:
        print(f"Runtime benchmark failed: {error}", file=sys.stderr)
        return 1

    report = {
        "schema_version": 1,
        "generated_at": datetime.now(timezone.utc).isoformat(),
        "profile": args.profile,
        "platform": {
            "system": platform.system(),
            "release": platform.release(),
            "machine": platform.machine(),
            "python": platform.python_version(),
        },
        "executable": str(executable),
        "scene": str(scene),
        "enforced": args.enforce,
        "scenarios": results,
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
