"""Capture the rolling counter contract and verify exact non-monotonic replay.

Build the current host, then run: just dev-exec python tests/validate_rolling_number.py
The Python runtime DLL directory must be on PATH on Windows.
"""
import hashlib
import json
import os
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parents[1]
OUTPUT = ROOT / "target/rolling-number-validation"
HOST = ROOT / "target/debug" / ("gaanim-core.exe" if os.name == "nt" else "gaanim-core")


def main():
    env = dict(os.environ, GAANIM_SNAPSHOTS=str(OUTPUT))
    subprocess.run([
        str(HOST), "--diff", "--example", str(ROOT / "tests/rolling_number_api_contract.py"),
        "--current", str(OUTPUT), "--capture-only", "--no-gui",
    ], cwd=ROOT, env=env, check=True, timeout=180)
    frames = sorted(OUTPUT.glob("seek_*.png"))
    assert len(frames) == 8, [path.name for path in frames]
    hashes = [hashlib.sha256(path.read_bytes()).hexdigest() for path in frames]
    for first, repeated in [(0, 6), (1, 5), (3, 7)]:
        assert hashes[first] == hashes[repeated], f"replay differs at {frames[first].name}"
    assert len(set(hashes[:5])) == 5, "start, fractional roll, end, cut, and translation must differ"
    report = {"frames": [path.name for path in frames], "sha256": hashes, "exact_replays": 3}
    (OUTPUT / "validation.json").write_text(json.dumps(report, indent=2), encoding="utf-8")
    print("Rolling number API and 3 exact reverse-seek comparisons passed.")


if __name__ == "__main__":
    main()
