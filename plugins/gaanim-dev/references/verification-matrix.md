# Change-aware verification matrix

Start with the narrowest relevant check and expand after it passes.

| Change | Focused checks | Completion checks |
|---|---|---|
| Internal crate | `cargo test -p <crate>` | `just check` |
| Rust public facade | focused crate test | `just check`, `just clippy` |
| Python binding or stub | focused Rust test | `just python-develop`, `just validate-python-api` |
| Typst documentation | relevant example inspection | `just docs` |
| Renderer, timeline, scene, objects, layout, or text | focused unit test and example | selected visual diff |
| Repository-wide or release-sensitive | focused checks first | format check, workspace tests, Clippy all targets, Python contract, docs |

Profiles exposed by `scripts/verify.py`:

- `fast`: docs-only builds docs; otherwise format-check, test changed crates,
  and run `just check`.
- `api`: run `fast`, Clippy, Python development install/API validation, and
  docs.
- `visual`: build the snapshot runner and compare selected or inferred
  examples.
- `full`: check formatting, test the workspace, lint all targets, rebuild and
  validate the Python extension, build docs, and run the repository audit.

Run `python plugins/gaanim-dev/scripts/verify.py <profile> --dry-run` before a
long validation when the scope is uncertain. A skipped visual suite is not a
pass; the command returns a distinct nonzero status if visual execution is not
available.
