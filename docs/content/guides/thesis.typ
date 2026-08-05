#import "../../components/section.typ": docs-chapter

#show: docs-chapter.with(
  title: "Thesis presentations",
  description: "Create, rehearse, validate, and present a thesis defense",
  route: "/guides/thesis/",
  updated: datetime.today().display(),
)

= Create the starter

The thesis starter contains nine semantic slides, speaker notes, controlled reveals,
charts, an equation, a methodology diagram, visual snapshots, and video backup support.

It starts with `scene.canvas.set_theme("presentation")`: projected-slide contrast,
semantic title/body colors, component panels, chart labels, and data values therefore
stay visually consistent without repeating colors on every object.

On Windows, make the Python runtime available and generate a new project:

```powershell
$pyBase = & .\.venv\Scripts\python.exe -c "import sys; print(sys.base_prefix)"
$env:PATH = "$pyBase;$env:PATH"
target/debug/gaanim.exe init thesis mi_tesis
```

The generated directory contains the entry script, manifest, assets, exports, and its
own README. Gaanim will not update scaffold files unless `--force` is supplied; custom
assets are preserved.

= Edit and preview

```powershell
target/debug/gaanim.exe mi_tesis
```

Saving the Python file triggers hot reload. Replace every placeholder enclosed in
brackets and customize the values used by charts, tables, equations, and notes.

= Present

```powershell
target/debug/gaanim.exe --present --monitor 1 mi_tesis
```

The public output opens full-screen. Presenter View shows the current slide and step,
the next stop, notes, elapsed time, navigation controls, and a searchable overview.

- `Right`, `Enter`, `Space`, or left click: advance.
- `Left` or `Backspace`: previous stop.
- `O`: searchable overview.
- `B`: toggle a black audience screen.
- `W`: toggle a white audience screen.
- `Escape`: leave presentation mode.

Static slide content appears immediately and the same advance starts the next slide's
first animation. Playback pauses at every `slide.step()`.

= Validate and export

Run semantic preflight before rehearsal:

```powershell
target/debug/gaanim.exe check mi_tesis
target/debug/gaanim.exe check mi_tesis --strict
```

It checks slide duration, 16:9 aspect ratio, speaker notes, named reveal steps, and
unresolved placeholders.

```powershell
target/debug/gaanim.exe --diff --example mi_tesis --bless --no-gui
target/debug/gaanim.exe --diff --example mi_tesis --no-gui
```

To produce a 60 FPS production-quality video backup:

```powershell
$env:GAANIM_EXPORT = "exports/mi_tesis_respaldo.mp4"
target/debug/gaanim.exe mi_tesis
Remove-Item Env:GAANIM_EXPORT
```

The detailed rehearsal and day-of checklist is also available in
`docs/thesis-presentations.md`.
