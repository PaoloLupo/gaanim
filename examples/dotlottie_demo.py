"""Vector dotLottie themes and state transitions authored on the timeline."""
import os
from pathlib import Path

from gaanim import Scene

scene = Scene(background="#0b1020")
path = str(Path(__file__).resolve().parent / "assets/dotlottie_demo.lottie")
scene.assets.preload([path])
clip = scene.media.lottie(path, state_machine_id="main", width=7)
scene.play([clip.animate.write().duration(0.75)])
scene.play([clip])
scene.wait(0.75)
clip.set_input("active", True)
scene.wait(0.75)
clip.set_theme("gold")
scene.wait(0.75)
clip.fire_event("reset")
scene.wait(0.5)
clip.set_theme(None)
scene.wait(0.5)

if snapshot_dir := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(snapshot_dir, [0.0, 0.4, 1.0, 1.75, 2.5, 3.25, 3.75])

scene.render()
