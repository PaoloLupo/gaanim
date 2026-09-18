"""Regenerate the small, original dotLottie fixtures (standard library only)."""
import copy
import json
from pathlib import Path
import struct
import zipfile
import zlib

ROOT = Path(__file__).resolve().parents[1]


def composition(color, size=70):
    return {
        "v": "5.7.4", "w": 200, "h": 160, "fr": 30, "ip": 0, "op": 60,
        "assets": [{"id": "pixel", "w": 1, "h": 1, "u": "../i/", "p": "pixel.png", "e": 0}],
        "markers": [{"cm": "half", "tm": 0, "dr": 30}],
        "layers": [{
            "ty": 4, "ind": 1, "ip": 0, "op": 60, "st": 0,
            "ks": {"p": {"a": 0, "k": [100, 80]}, "a": {"a": 0, "k": [0, 0]},
                   "s": {"a": 0, "k": [100, 100]}, "o": {"a": 0, "k": 100},
                   "r": {"a": 1, "k": [{"t": 0, "s": [0], "e": [180], "o": {"x": [0], "y": [0]}, "i": {"x": [1], "y": [1]}}, {"t": 60, "s": [180]}]}},
            "shapes": [{"ty": "rc", "p": {"a": 0, "k": [0, 0]}, "s": {"a": 0, "k": [size, size]}, "r": {"a": 0, "k": 12}},
                       {"ty": "fl", "c": {"sid": "accent", "a": 0, "k": color}, "o": {"a": 0, "k": 100}, "r": 1}]
        }, {"ty": 2, "ind": 2, "refId": "pixel", "ip": 0, "op": 60, "st": 0,
            "ks": {"p": {"a": 0, "k": [20, 20]}, "a": {"a": 0, "k": [0, 0]}, "s": {"a": 0, "k": [1200, 1200]}, "o": {"a": 0, "k": 100}, "r": {"a": 0, "k": 0}}}]
    }


def pixel_png():
    def chunk(name, data):
        return struct.pack(">I", len(data)) + name + data + struct.pack(">I", zlib.crc32(name + data))
    return b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", 1, 1, 8, 6, 0, 0, 0)) + chunk(b"IDAT", zlib.compress(b"\x00\xff\xff\xff\xff")) + chunk(b"IEND", b"")


def archive(path, entries):
    path.parent.mkdir(parents=True, exist_ok=True)
    with zipfile.ZipFile(path, "w") as output:
        for name, value in sorted(entries.items()):
            entry = zipfile.ZipInfo(name, (2026, 1, 1, 0, 0, 0))
            entry.compress_type = zipfile.ZIP_DEFLATED
            output.writestr(entry, value if isinstance(value, bytes) else json.dumps(value, separators=(",", ":")).encode())


def main():
    idle = composition([0.1, 0.65, 1])
    active = composition([1, 0.35, 0.15], 100)
    machine = {
        "initial": "idle", "inputs": [{"type": "Boolean", "name": "active", "value": False}, {"type": "Event", "name": "reset"}],
        "states": [
            {"name": "idle", "type": "PlaybackState", "animation": "idle", "loop": True,
             "transitions": [{"type": "Transition", "toState": "active", "guards": [{"type": "Boolean", "inputName": "active", "conditionType": "Equal", "compareTo": True}]}]},
            {"name": "active", "type": "PlaybackState", "animation": "active", "loop": True, "mode": "Reverse",
             "transitions": [{"type": "Transition", "toState": "done", "guards": [{"type": "Event", "inputName": "reset"}]}]},
            {"name": "done", "type": "PlaybackState", "animation": "idle", "autoplay": False, "final": True}
        ]
    }
    archive(ROOT / "examples/assets/dotlottie_demo.lottie", {
        "manifest.json": {"version": "2", "animations": [{"id": "idle"}, {"id": "active"}], "themes": [{"id": "gold"}], "stateMachines": [{"id": "main"}], "initial": {"animation": "idle"}},
        "a/idle.json": idle, "a/active.json": active, "i/pixel.png": pixel_png(), "s/main.json": machine,
        "t/gold.json": {"rules": [{"id": "accent", "type": "Color", "value": [1, 0.75, 0.15]}]}
    })
    v1 = copy.deepcopy(idle)
    v1["assets"][0]["u"] = "../images/"
    archive(ROOT / "tests/assets/dotlottie_v1.lottie", {"manifest.json": {"version": "1.0", "animations": [{"id": "idle"}], "activeAnimationId": "idle"}, "animations/idle.json": v1, "images/pixel.png": pixel_png()})


if __name__ == "__main__":
    main()
