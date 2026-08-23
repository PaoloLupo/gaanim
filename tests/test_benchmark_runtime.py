from __future__ import annotations

import importlib.util
import json
from pathlib import Path
import tempfile
import unittest


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
            executable=Path("gaanim-core"),
            scene=Path("scene.py"),
            artifact_dir=Path("target/performance/seek"),
        )

        self.assertIn("--capture-only", command)
        self.assertNotIn("--bless", command)

    def test_capture_artifact_validation_requires_the_expected_frame_count(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            artifact_dir = Path(temporary)
            (artifact_dir / "manifest.json").write_text(
                json.dumps({"snapshots": [{"id": "a"}, {"id": "b"}]}),
                encoding="utf-8",
            )

            benchmark_runtime.validate_artifacts("seek", artifact_dir, 2)
            with self.assertRaises(benchmark_runtime.BenchmarkFailure):
                benchmark_runtime.validate_artifacts("preview", artifact_dir, 3)

    def test_export_artifact_validation_reads_phase_timings(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            artifact_dir = Path(temporary)
            (artifact_dir / "benchmark.mp4").write_bytes(b"video")
            (artifact_dir / "command.log").write_text(
                "GAANIM_EXPORT_TIMINGS render_gpu_ms=10.5 encoder_wait_ms=2.0 "
                "encode_active_ms=8.25 finalize_ms=1.0 total_ms=15.0\n",
                encoding="utf-8",
            )

            report = benchmark_runtime.validate_artifacts("export", artifact_dir, 1)

            self.assertEqual(report["phase_timings_ms"]["encode_active_ms"], 8.25)


if __name__ == "__main__":
    unittest.main()
