# Visual regression workflow

Use the per-example layout:

`tests/visual/<example-path-without-.py>/{baseline,current,report}`.

Only `baseline/` is an approved fixture. `current/` and `report/` are local
outputs. A visual example must call `scene.snapshots(...)` conditionally when
`GAANIM_SNAPSHOTS` is present.

## Compare

Build the editor snapshot runner, then execute:

```text
target/debug/gaanim.exe --diff --example examples/<name>.py --no-gui
```

On failure, inspect `tests/visual/<name>/report/index.html` and its JSON report.
Do not tune pixel thresholds until the semantic difference is understood.

## Approve

Never run `--bless` from inference, routine testing, or an attempt to make CI
green. Require an explicit user request approving the exact example and visual
change. `scripts/verify.py` requires both `--bless` and `--allow-bless` as a
deliberate safety interlock.

After approval, review the newly tracked baseline images and manifest. Do not
commit `current/` or `report/`.

The repository currently runs visual regression on Windows in CI. On a host
where the runner cannot be built or executed, report the suite as skipped and
run the remaining checks; never call it successful.
