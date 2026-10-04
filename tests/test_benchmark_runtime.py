from __future__ import annotations

import importlib.util
import json
from pathlib import Path
import tempfile
import unittest
from unittest import mock


MODULE_PATH = Path(__file__).with_name("benchmark_runtime.py")
SPEC = importlib.util.spec_from_file_location("benchmark_runtime", MODULE_PATH)
assert SPEC and SPEC.loader
benchmark_runtime = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(benchmark_runtime)


class RuntimeBenchmarkTests(unittest.TestCase):
    def test_percentile_interpolates_small_sample_sets(self) -> None:
        self.assertEqual(benchmark_runtime.percentile([7.0], 0.95), 7.0)
        self.assertEqual(benchmark_runtime.percentile([1.0, 3.0], 0.50), 2.0)
        self.assertAlmostEqual(
            benchmark_runtime.percentile([10.0, 20.0, 30.0], 0.95), 29.0
        )

    def test_budget_reports_latency_memory_and_throughput(self) -> None:
        result = {"p95_ms": 120.0, "peak_rss_mb": 512.0, "fps_at_p95": 8.0}
        budget = {"p95_ms": 100.0, "peak_rss_mb": 500.0, "min_fps": 10.0}

        violations = benchmark_runtime.budget_violations(result, budget)

        self.assertEqual(len(violations), 3)
        self.assertTrue(any("p95" in violation for violation in violations))
        self.assertTrue(any("RSS" in violation for violation in violations))
        self.assertTrue(any("throughput" in violation for violation in violations))

    def test_capture_scenarios_use_the_non_baseline_cli(self) -> None:
        command = benchmark_runtime.scenario_command(
            "seek",
            executable=Path("gaanim"),
            scene=Path("scene.py"),
            artifact_dir=Path("target/performance/seek"),
        )

        self.assertIn("--capture-only", command)
        self.assertNotIn("--bless", command)

    def test_export_scenario_forwards_the_requested_encoder(self) -> None:
        command = benchmark_runtime.scenario_command(
            "export",
            executable=Path("gaanim"),
            scene=Path("scene.py"),
            artifact_dir=Path("target/performance/export"),
            encoder="nvenc",
        )

        self.assertEqual(command[-2:], ["--encoder", "nvenc"])

    def test_capture_artifact_validation_requires_the_expected_frame_count(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            artifact_dir = Path(temporary)
            (artifact_dir / "manifest.json").write_text(
                json.dumps({"snapshots": [{"id": "a"}, {"id": "b"}]}),
                encoding="utf-8",
            )
            (artifact_dir / "command.log").write_text(
                "GAANIM_CAPTURE_TIMINGS setup_ms=1 timeline_update_ms=2 "
                "scene_compile_ms=3 render_readback_ms=4 capture_total_ms=10\n"
                "GAANIM_PNG_TIMINGS png_encode_ms=5\n",
                encoding="utf-8",
            )

            benchmark_runtime.validate_artifacts("seek", artifact_dir, 2)
            with self.assertRaises(benchmark_runtime.BenchmarkFailure):
                benchmark_runtime.validate_artifacts("preview", artifact_dir, 3)

    def test_windows_child_path_contains_the_python_runtime(self) -> None:
        with mock.patch.object(benchmark_runtime.os, "name", "nt"), mock.patch.object(
            benchmark_runtime.sys, "base_prefix", r"C:\Python"
        ), mock.patch.dict(benchmark_runtime.os.environ, {"PATH": r"C:\Tools"}, clear=True):
            environment = benchmark_runtime.child_environment()

        self.assertEqual(environment["PATH"].split(benchmark_runtime.os.pathsep)[0], r"C:\Python")

    def test_export_artifact_validation_reads_phase_timings(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            artifact_dir = Path(temporary)
            (artifact_dir / "benchmark.mp4").write_bytes(b"video")
            (artifact_dir / "command.log").write_text(
                "GAANIM_EXPORT_TIMINGS encoder=h264_vaapi render_gpu_ms=10.5 encoder_wait_ms=2.0 "
                "encode_active_ms=8.25 finalize_ms=1.0 total_ms=15.0\n",
                encoding="utf-8",
            )

            report = benchmark_runtime.validate_artifacts("export", artifact_dir, 1)

            self.assertEqual(report["encoder"], "h264_vaapi")
            self.assertEqual(report["phase_timings_ms"]["encode_active_ms"], 8.25)

    def test_export_formats_pick_the_extension_and_drop_inapplicable_options(self) -> None:
        def command(export_format: str) -> list[str]:
            return benchmark_runtime.scenario_command(
                "export",
                executable=Path("gaanim"),
                scene=Path("scene.py"),
                artifact_dir=Path("out"),
                encoder="nvenc",
                export_format=export_format,
            )

        webm = command("webm")
        self.assertIn(str(Path("out") / "benchmark.webm"), webm)
        self.assertNotIn("--encoder", webm)
        self.assertIn("--quality", webm)
        bundle = command("gaanim")
        self.assertNotIn("--quality", bundle)
        self.assertNotIn("--encoder", bundle)

    def test_png_sequence_export_accepts_numbered_files_without_timings(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            artifact_dir = Path(temporary)
            (artifact_dir / "benchmark_0001.png").write_bytes(b"png")
            (artifact_dir / "command.log").write_text("", encoding="utf-8")

            report = benchmark_runtime.validate_artifacts("export", artifact_dir, 1, "png")

            self.assertIsNone(report["phase_timings_ms"])
            with self.assertRaises(benchmark_runtime.BenchmarkFailure):
                benchmark_runtime.validate_artifacts("export", artifact_dir, 1, "mp4")

    def test_suites_fill_defaults_and_reject_unknown_scenarios(self) -> None:
        configuration = {
            "suites": {
                "gpu": {"scenes": ["a.py", {"scene": "b.py", "scenarios": ["seek"], "scales": [1, 4], "budgets": False}]},
                "bad": {"scenes": [{"scene": "c.py", "scenarios": ["render"]}]},
            }
        }

        first, second = benchmark_runtime.suite_entries(configuration, "gpu")

        self.assertEqual(first["scenarios"], benchmark_runtime.SCENARIOS)
        self.assertEqual(first["scales"], [1])
        self.assertTrue(first["budgets"])
        self.assertEqual(second["scenarios"], ("seek",))
        self.assertEqual(second["scales"], [1, 4])
        self.assertFalse(second["budgets"])
        for suite in ("bad", "missing"):
            with self.assertRaises(benchmark_runtime.BenchmarkFailure):
                benchmark_runtime.suite_entries(configuration, suite)

    def test_baseline_comparison_flags_only_slowdowns_past_the_limit(self) -> None:
        baseline = benchmark_runtime.baseline_p50s(
            {
                "schema_version": 2,
                "export_format": "webm",
                "runs": [
                    {"scene": "examples/heavy.py", "scale": 2, "scenarios": {"export": {"p50_ms": 100.0}}}
                ],
            }
        )

        self.assertEqual(baseline, {"heavy@2:export/webm": 100.0})
        change, violation = benchmark_runtime.regression_violation(105.0, 100.0, 0.10)
        self.assertAlmostEqual(change, 0.05)
        self.assertIsNone(violation)
        change, violation = benchmark_runtime.regression_violation(125.0, 100.0, 0.10)
        self.assertIsNotNone(violation)
        _, violation = benchmark_runtime.regression_violation(50.0, 100.0, 0.10)
        self.assertIsNone(violation)
        with self.assertRaises(benchmark_runtime.BenchmarkFailure):
            benchmark_runtime.baseline_p50s({"schema_version": 1, "scenarios": {}})


if __name__ == "__main__":
    unittest.main()
