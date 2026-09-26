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
