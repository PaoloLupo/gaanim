"""Timeline-synchronized MP4 playback with trim, loop, speed, and audio."""

import os

from gaanim import Scene, WHITE


scene = Scene(frame=(16, 9), background="#0f172a")
source = os.environ.get("GAANIM_VIDEO", "assets/clip.mp4")

video = scene.media.video(
    source,
    width=12,
    height=6.75,
    fit="contain",
    offset=0,
    duration=4.0,
    loop=True,
    speed=1.0,
    audio=True,
    volume=0.8,
)
label = scene.text("MP4 · seek · loop · audio", role="caption").fill(WHITE).move_to(0, -3.916667)

scene.play([video, video.animate.fade_in().duration(0.4), label.animate.write().duration(0.4)])
scene.wait(7.6)
scene.render()
