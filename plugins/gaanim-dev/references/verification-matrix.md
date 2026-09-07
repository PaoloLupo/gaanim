# Change-aware verification matrix

Run focused and completion checks for the changed layers. After they pass,
expand only when new evidence or the requested scope requires it.

| Change | Focused checks | Completion checks |
|---|---|---|
| Agent instructions, skill prose, plugin metadata | diff and reference review; skill/plugin validators | plugin audit; no runtime build |
| Internal crate | `just test-package <crate>` with relevant target/filter | affected consumers only when integration risk warrants it |
| Rust public facade | focused dev crate test | focused consumer checks/lints; workspace only with concrete cross-workspace risk |
| Python binding or stub | focused Rust test | `just wheel`, `just validate-python-api` through the executable |
| Typst documentation | relevant example inspection | `just docs` |
| Renderer, timeline, scene, objects, layout, or text | focused unit test and example | selected visual diff |
| Runtime hot path or performance harness | Python harness tests and `just benchmark smoke` | comparable `standard` run when warranted |
| Repository-wide or release-sensitive | focused checks first | format check, workspace tests, Clippy all targets, Python contract, docs |

Profiles exposed by `scripts/verify.py`:

- `fast`: builds docs for docs changes without other non-plugin changes; for Rust
  changes, format-checks and tests changed Rust crates in one `just dev test`
  invocation, with no trailing workspace check; for plugin
  changes, runs plugin utility tests and the audit. Agent prose alone adds no build.
- `api`: run `fast`, Clippy, authoring-wheel/API validation, and
  docs.
- `visual`: build the snapshot runner and compare selected or inferred
  examples.
- `performance`: run the native runtime benchmark with `smoke` by default; pass
  `--benchmark-profile standard` for the 300-frame comparable profile.
- `full`: check formatting, test the workspace, lint all targets, rebuild the
  authoring wheel, validate the embedded module, build docs, smoke the
  performance harness, and run the audit.

Run `python plugins/gaanim-dev/scripts/verify.py <profile> --dry-run` before a
long validation when the scope is uncertain. A skipped visual suite is not a
pass; the command returns a distinct nonzero status if visual execution is not
available.

## Reuse compilation work

Gaanim builds are expensive. Plan required checks together after edits stabilize.
Inspect recipe dependencies and the dry-run output before selecting a profile;
`api` and `full` include broad work and are not mandatory for every public edit.
Use the table's affected-layer checks directly when a profile adds unrelated work.
Prefer the checkout's scripts over an older installed plugin copy.

Use `just check-package`, `just test-package`, or `just dev` for normal agent
validation. The dev helper consistently enables `dev-dynamic` for selected
Bevy consumers. Do not switch between these recipes and raw static Cargo
commands. Batch packages in one invocation and stop after relevant checks pass;
do not add workspace checks/tests/builds merely as a completion ritual.
Dynamic linking saves repeated link work, not compilation of changed sources
or the separate artifact requirements of check/test/build. Release recipes and
runtime benchmarks intentionally keep static linking. Never enable all features
for distribution, since that would also select `dev-dynamic`.

Run already-current dev executables and subprocess harnesses with
`just dev-exec <command> ...` so Bevy and Rust shared libraries can be found.
This command only sets library search paths and runs the command; it does not
build or establish that the executable is current.

Keep the existing Cargo target directory, toolchain, features, profile, flags,
and configured PyO3 interpreter stable. Never clear `target/`, use `cargo clean`,
or create an isolated build cache as a troubleshooting default. Coordinate agents
so only one Cargo invocation uses a shared target directory at a time; leave
Cargo's internal job count at its default unless the user requests a limit.

Reuse a successful check when no relevant input changed, and an existing binary
when its matching build is known to be current. File existence alone is not
freshness evidence. If freshness is uncertain, use the supported Cargo/just
command so Cargo can reuse valid cached artifacts and rebuild what changed.
Do not assume a successful `check` produced a runnable binary or that debug
artifacts satisfy release benchmarks. A missing artifact does not justify a
workspace build when the required package and target can be built directly.
