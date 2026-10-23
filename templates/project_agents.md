# AGENTS.md

This is a Gaanim project: a Python scene script (`main.py`, per `gaanim.toml`)
executed by the Gaanim application, which owns the native runtime.

## Documentation

Consult the documentation of the installed Gaanim version before writing or
changing scenes; do not rely on memory or Manim conventions.

- The `gaanim` package in `.venv` ships the docs as Typst sources in
  `gaanim/_docs/` (`.venv/Lib/site-packages/gaanim/_docs` on Windows,
  `.venv/lib/python3.*/site-packages/gaanim/_docs` elsewhere).
  `index.typ` lists the reading order; `referencia/*.typ` holds signatures,
  parameters, defaults, and errors; `guias/` explains tasks and `ejemplos/`
  holds runnable scenes.
- The `.pyi` stubs next to `_docs` are authoritative for signatures.
- If the `gaanim-docs` agent skill is installed, use it to locate, index, and
  search these docs.
- If `.venv` is missing, run `uv sync` first.

## Workflow

- `import gaanim` fails in plain Python by design. Do not run `python main.py`.
- Validate: `gaanim check .`
- Preview: `gaanim .`
- Export video: `gaanim export . --output exports/video.mp4 --quality production`
- Present slides: `gaanim --present --monitor 1 .`
- Put source media in `assets/`; generated output goes to `exports/`.

## Look at the result

`gaanim check .` only proves that the script runs, and `gaanim .` opens a
window you cannot see. Before calling a scene done, capture frames and look
at them:

- With `scene.stop()` (slides): `gaanim --diff --example . --capture-stops
  --capture-only --current snapshots` writes `snapshots/stop_0001.png`, one
  image per stop.
- Without stops (videos): end the script with

  ```python
  import os

  if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
      scene.snapshots(snapshots, [1.0, 3.5, scene.cursor])
  else:
      scene.render()
  ```

  choosing the seconds where each step has finished (`scene.cursor` is the
  last frame), then run `gaanim --diff --example . --capture-only
  --current snapshots` for `snapshots/seek_*.png`. `manifest.json` there
  lists the time of each image.
- Open every PNG as an image and compare it with what was asked: layout,
  overlaps, clipped or unreadable text, colors, what is visible when. Fix
  the script and capture again until it matches.
- `snapshots/` is gitignored. Do not pass `--bless` unless the user asks: it
  replaces the approved images in `tests/visual/`.
- If the `gaanim` executable is unavailable or you cannot read images, say
  that the result was not checked visually.
