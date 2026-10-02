"""Write the differential cases for live behaviors.

Compiles the behaviors in ``behaviors.py`` with ``gaanim.live``, runs each
as plain CPython on sample players, and writes the programs with the poses
Python returned to ``crates/gaanim_animation/src/live/cases.json``. The Rust
test ``compiled_behaviors_pose_like_python`` evaluates the programs and
compares, so the compiler and the evaluator agree with Python.

Run with plain Python (no Gaanim host needed):

    python tests/live/generate_cases.py
"""

from __future__ import annotations

import importlib.util
import json
import math
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
OUTPUT = ROOT / "crates" / "gaanim_animation" / "src" / "live" / "cases.json"


def load(name: str, path: Path):
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    sys.modules[name] = module
    spec.loader.exec_module(module)
    return module


live = load("gaanim_live", ROOT / "crates" / "gaanim_python" / "gaanim" / "live.py")
sys.modules["gaanim.live"] = live
behaviors = load("live_behaviors", Path(__file__).with_name("behaviors.py"))


def number(value) -> str:
    return repr(float(value))


def players():
    names = ["Ana", "Beto", "Caro", "Dani", "Eli", "Fede", "Gabi", "Hugo"]
    for index, name in enumerate(names):
        for t in (0.0, 0.05, 0.4, 0.9, 1.3, 2.0, 3.7, 7.25):
            rank = (index * 3 + int(t)) % len(names)
            previous = (rank + index) % len(names)
            score = float((len(names) - rank) * 137 + index)
            yield live.Player(
                name=name,
                t=t,
                time=t + index * 0.6,
                joined=index * 0.6,
                index=index,
                count=len(names),
                rank=rank,
                score=score,
                leader=float(len(names) * 137 + 7),
                previous_rank=previous,
                rank_since=t / 2,
                previous_score=score - 100 * (index % 3),
                score_since=t / 3,
                team=index % 2,
                team_index=index // 2,
                team_count=len(names) // 2,
                team_score=float(500 + 137 * (index % 2)),
                team_rank=1 - index % 2,
            )


def kept_players():
    """The sample players, each keeping some numbers."""
    for index, player in enumerate(players()):
        player.state = live.State(
            energy=(index * 0.37) % 1.0,
            hops=float(index % 4),
            best=float(index * 113 % 1000),
        )
        yield player


def inputs_of(player) -> dict:
    inputs = {name: number(getattr(player, name)) for name in live.PLAYER_FIELDS}
    inputs["seed"] = player.seed
    if player.state.__dict__:
        inputs["state"] = [number(value) for value in player.state.__dict__.values()]
    return inputs


def main() -> None:
    programs = []
    kept = [(function, behaviors.KEPT) for function in behaviors.KEPT_BEHAVIORS]
    for function, state in [(function, ()) for function in behaviors.BEHAVIORS] + kept:
        program = json.loads(live.compile_behavior(function, state))
        cases = []
        for player in kept_players() if state else players():
            result = function(player)
            inputs = inputs_of(player)
            cases.append(
                {
                    "inputs": inputs,
                    "pose": {
                        "x": number(result.x),
                        "y": number(result.y),
                        "rotation": number(result.rotation),
                        "scale": number(result.scale),
                        "sx": number(result.sx),
                        "sy": number(result.sy),
                        "lean": number(result.lean),
                        "look": None
                        if result.look_x is None or result.look_y is None
                        else [number(result.look_x), number(result.look_y)],
                        "show_name": bool(result.show_name),
                        "flip": bool(result.flip),
                        "visible": bool(result.visible),
                        "express": result.express,
                        "since": None if result.since is None else number(result.since),
                        "loop": bool(result.loop),
                    },
                }
            )
        programs.append({"program": program, "cases": cases})
    for function in behaviors.KEPT_UPDATES:
        program = json.loads(live.compile_update(function, behaviors.KEPT))
        cases = []
        for player in kept_players():
            result = function(player)
            # The numbers the update leaves out keep their value.
            after = {**player.state.__dict__, **result.__dict__}
            cases.append(
                {
                    "inputs": inputs_of(player),
                    "next": [number(after[name]) for name in behaviors.KEPT],
                }
            )
        programs.append({"program": program, "cases": cases})
    OUTPUT.write_text(json.dumps(programs, indent=1) + "\n", encoding="utf-8")
    total = sum(len(entry["cases"]) for entry in programs)
    print(f"wrote {len(programs)} behaviors, {total} cases to {OUTPUT.relative_to(ROOT)}")


if __name__ == "__main__":
    main()
