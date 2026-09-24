---
name: gaanim-docs
description: Consult the Gaanim documentation that matches the version installed in the current project before writing or explaining Gaanim scenes. Use in any project that uses Gaanim (a gaanim.toml, `from gaanim import`, or `gaanim` in pyproject.toml) when authoring or editing scenes, slides, animations, text, layout, visualization, audio, assets, themes, or export; when answering questions about the Gaanim Python API; or when a Gaanim error needs its documented contract.
---

# Consult Gaanim documentation

Gaanim evolves quickly; do not rely on memory or Manim conventions. The
authoring package installed in the project ships its documentation as Typst
sources under `gaanim/_docs`, versioned with the API it describes.

## Locate the docs

Run the bundled script with any Python 3.10+, from the project directory:

```bash
python <SKILL_DIR>/scripts/gaanim_docs.py locate
python <SKILL_DIR>/scripts/gaanim_docs.py index
python <SKILL_DIR>/scripts/gaanim_docs.py search "Scene\.segment|segment\("
```

`<SKILL_DIR>` is the directory containing this `SKILL.md`. Use `--project DIR`
when the Gaanim project is not the cwd. `locate` prints the docs `root`, the
installed `version`, and the `stubs` directory. If nothing is found, the
project has no environment yet: suggest `uv sync` (or opening the project once
in the Gaanim application) rather than guessing the API.

## Workflow

1. Run `index` once per task to see pages in reading order with titles.
   Choose pages by intent:
   - `api/*.typ`: exact signatures, parameters, defaults, returns, errors.
   - `manual/*.typ` and `guia/*.typ`: concepts, the timeline model, idioms.
   - `examples/*.typ` and `examples/*.py`: complete runnable scenes.
   - `guides/*.typ`: projects, slides, layout, visual regression, migration.
2. Read the relevant files directly from `root`, or `search` for a name.
   In Typst sources, `#api-entry(name:, signature:, params:, returns:, desc:)`
   is one API contract; fenced `python` blocks are runnable examples; `#...`
   calls outside those blocks are presentation markup. Prose is in Spanish.
3. Confirm every name you use against the `.pyi` stubs in `stubs`
   (`gaanim_core.pyi`, `colors.pyi`, `templates.pyi`, ...). When docs and stubs
   disagree on a signature, follow the stubs and mention the discrepancy.
4. Write code in the documented style: build everything from a `Scene`
   (`scene.geometry`, `scene.text`, `scene.layout`, `scene.viz`, ...), use
   fluent setters for immediate state and `.animate` for timeline changes, and
   end the script with `scene.render()`.
5. Validate with the Gaanim application, not plain Python: the `gaanim`
   package deliberately refuses to import outside the application. Use
   `gaanim check .` for a project (or `gaanim check path/to/script.py`), and
   `gaanim .` to preview. If the `gaanim` executable is unavailable, say the
   scene was not executed.

Cite the page (`api/scene.typ`, ...) and docs version behind non-obvious API
choices. Never edit files under the installed `_docs` directory.
