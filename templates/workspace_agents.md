# AGENTS.md

This is a Gaanim workspace: several projects (videos, talks) in one
repository that share Python code, assets and a Python environment. The
`[workspace]` table of `gaanim.toml` says where the projects are and what
they share; each project is a folder with its own `gaanim.toml` and
`main.py`.

## Layout

- `gaanim.toml`: `members` lists where the projects are (`*` stands for any
  part of a folder name); `[workspace.shared]` names the shared folders.
- `src/`: Python shared by every project; a member imports it as any
  package, and Gaanim reloads it when it changes.
- `assets/`: images, SVG, fonts and sounds every project can load by name.
- Each member keeps its own `assets/` and writes its renders to its own
  `exports/`; its own files win over the shared ones.

## Documentation

Consult the documentation of the installed Gaanim version before writing or
changing scenes; do not rely on memory.

- The `gaanim` package in `.venv` ships the docs as Typst sources in
  `gaanim/_docs/` (`.venv/Lib/site-packages/gaanim/_docs` on Windows,
  `.venv/lib/python3.*/site-packages/gaanim/_docs` elsewhere).
  `index.typ` lists the reading order; `referencia/*.typ` holds signatures,
  parameters, defaults, and errors; `guias/proyectos.typ` explains
  workspaces.
- The `.pyi` stubs next to `_docs` are authoritative for signatures.
- If the `gaanim-docs` agent skill is installed, use it to search these docs.
- If `.venv` is missing, run `uv sync` first.

## Workflow

- `import gaanim` fails in plain Python by design. Do not run `python main.py`.
- Check every member: `gaanim check .` (or `--members "videos/*"`).
- New member: `gaanim init video videos/<name>` or
  `gaanim init slides talks/<name>`, from this folder; a folder `members`
  does not list is not part of the workspace.
- Preview a member: `gaanim videos/<name>`; `gaanim .` opens the workspace
  in Home.
- Export a member to its `exports/`: `gaanim export videos/<name>`; every
  member: `gaanim export .`, or `gaanim export . -o renders/{member}.mp4`.
- Put code more than one member uses in `src/`, and assets more than one
  member uses in `assets/`.
