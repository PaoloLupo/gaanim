#import "../../components/section.typ": docs-chapter

#show: docs-chapter.with(
  title: "Audio",
  description: "Timeline-aligned tracks mixed directly into video exports",
  route: "/api/audio/",
  code-langs: (),
  updated: datetime.today().display(),
)

= Audio

Declare audio on the scene. Relative paths use `scene.assets_dir(...)`, just
like images and SVG files. On MP4 and WebM export, gaanim sends the tracks to
FFmpeg, aligns them to the scene timeline, mixes them, and muxes the result
with the rendered video.

```python
from gaanim import Scene

scene = Scene()
scene.assets_dir("assets")

scene.audio("music.ogg", volume=0.35)
scene.wait(1.5)
scene.audio("pop.wav", duration=0.4, volume=0.8, fade_in=0.02)

scene.export("lesson.mp4")
```

`start` is optional. When omitted, the source starts at the current timeline
cursor; use `start=...` to place it at an absolute scene time. `duration`
trims a source, which also makes a fade-out deterministic.

```python
scene.audio(
    "narration.m4a",
    start=3.0,
    duration=7.5,
    volume=0.9,
    fade_in=0.15,
    fade_out=0.25,
)
```

Audio is currently export-only: the interactive preview does not yet play or
scrub audio. MP4 uses AAC; WebM uses Opus. Image sequences, GIF, and animated
WebP reject audio tracks because those formats do not carry an audio stream.
