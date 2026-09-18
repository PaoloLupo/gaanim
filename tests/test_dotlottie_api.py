"""Run in the native host: --validate-python-api tests/test_dotlottie_api.py."""
from pathlib import Path
from gaanim import Scene

ROOT = Path(__file__).resolve().parents[1]
PATH = str(ROOT / "examples/assets/dotlottie_demo.lottie")


def fails(error_type, operation):
    try:
        operation()
    except error_type:
        return
    raise AssertionError(f"expected {error_type.__name__}")


scene = Scene()
scene.assets.preload([PATH])
clip = scene.media.lottie(PATH, state_machine_id="main", width=4)
assert clip.animation_ids == ["idle", "active"]
assert clip.theme_ids == ["gold"]
assert clip.state_machine_ids == ["main"]
assert clip.set_theme("gold") is clip
assert clip.set_input("active", False) is clip
fails(ValueError, lambda: clip.fire_event("reset"))
fails(ValueError, lambda: clip.set_input("active", 1))
fails(ValueError, lambda: clip.set_input("missing", True))
fails(ValueError, lambda: clip.set_theme("missing"))
fails(ValueError, lambda: scene.media.lottie(PATH, animation_id="idle", state_machine_id="main"))
fails(ValueError, lambda: scene.media.lottie(PATH, state_machine_id="main", speed=2))
scene.play([clip])
scene.wait(0.5)
assert clip.set_input("active", True) is clip
assert clip.fire_event("reset") is clip
assert clip.set_theme(None) is clip
scene.wait(0.5)
assert scene.media.lottie(str(ROOT / "tests/assets/dotlottie_v1.lottie")).animation_ids == ["idle"]
print("dotLottie Python API checks passed")
