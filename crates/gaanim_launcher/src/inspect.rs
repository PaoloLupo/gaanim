//! `gaanim inspect`: the timeline of a scene with the script line of every
//! part, as a table or as JSON for editors.

use std::path::{Path, PathBuf};

use gaanim_api::canvas::{IndexedSpan, TimelineIndex};
use gaanim_core::console::{self, ScriptLocation};
use serde_json::{Value, json};

/// Version of the JSON `gaanim inspect --json` prints; it grows only by
/// adding fields.
pub const SCHEMA: u32 = 1;

#[derive(Debug, Clone, PartialEq)]
struct InspectArgs {
    script: PathBuf,
    json: bool,
}

fn parse_inspect_args(args: &[String]) -> Result<InspectArgs, String> {
    let mut script = None;
    let mut json = false;
    for arg in args {
        match arg.as_str() {
            "--json" => json = true,
            value if value.starts_with('-') => return Err(format!("unknown option `{value}`")),
            value if script.is_none() => script = Some(PathBuf::from(value)),
            value => return Err(format!("unexpected argument `{value}`")),
        }
    }
    Ok(InspectArgs {
        script: script.ok_or_else(|| "missing <SCRIPT_OR_PROJECT>".to_string())?,
        json,
    })
}

/// Run `gaanim inspect` when it is the command.
pub fn dispatch_inspect_mode() -> bool {
    let mut args = std::env::args().skip(1);
    if args.next().as_deref() != Some("inspect") {
        return false;
    }
    let args: Vec<String> = args.collect();
    let parsed = parse_inspect_args(&args).unwrap_or_else(|error| {
        console::error("inspect", error);
        console::hint("Run `gaanim inspect --help` for usage.");
        std::process::exit(2);
    });
    if gaanim_editor::bundle_player::is_bundle_path(&parsed.script) {
        console::error(
            "inspect",
            "inspect reads a script or project; a bundle keeps no script lines",
        );
        std::process::exit(2);
    }
    let script = gaanim_project::resolve_entry(&parsed.script).unwrap_or_else(|error| {
        console::error("inspect", error);
        std::process::exit(2);
    });
    // Absolute and written as the script lines are, which editors match.
    let script = std::path::absolute(&script)
        .map(|script| console::plain_path(&script))
        .unwrap_or(script);
    let python = crate::python::runtime(&script).unwrap_or_else(|error| {
        console::error("inspect", error);
        console::hint(crate::python::install_hint());
        std::process::exit(2);
    });
    // The lines are what this command is for.
    gaanim_api::canvas::track_script_lines(true);
    let canvas = (python.load_script_canvas)(&script).unwrap_or_else(|error| {
        console::error("inspect", format!("could not load project: {error}"));
        std::process::exit(2);
    });
    let index = canvas.timeline_index();
    if parsed.json {
        println!("{}", index_json(&script, &index));
    } else {
        print!("{}", index_table(&script, &index));
    }
    true
}

/// A location's `file` and `line` fields, null when the line is unknown.
fn location_fields(location: Option<&ScriptLocation>) -> (Value, Value) {
    match location {
        Some(location) => (json!(location.file.to_string_lossy()), json!(location.line)),
        None => (Value::Null, Value::Null),
    }
}

fn span_json(span: &IndexedSpan) -> Value {
    let (file, line) = location_fields(span.location.as_ref());
    let mut value = json!({
        "start": span.start,
        "end": span.end,
        "file": file,
        "line": line,
    });
    if let Some(key) = &span.key {
        value["key"] = json!(key);
    }
    value
}

/// The JSON `gaanim inspect --json` prints for `index`, read from `script`.
fn index_json(script: &Path, index: &TimelineIndex) -> Value {
    let segments: Vec<Value> = index
        .segments
        .iter()
        .map(|segment| {
            let (file, line) = location_fields(segment.location.as_ref());
            let stops: Vec<Value> = segment
                .stops
                .iter()
                .map(|stop| {
                    let (file, line) = location_fields(stop.location.as_ref());
                    json!({ "name": stop.name, "time": stop.time, "file": file, "line": line })
                })
                .collect();
            json!({
                "name": segment.name,
                "section": segment.section,
                "start": segment.start,
                "end": segment.end,
                "file": file,
                "line": line,
                "stops": stops,
            })
        })
        .collect();
    // Each Section once, from its first segment's start to its last's end.
    let mut sections: Vec<(String, f64, f64)> = Vec::new();
    for segment in &index.segments {
        let Some(key) = &segment.section else {
            continue;
        };
        match sections.iter_mut().find(|(known, _, _)| known == key) {
            Some(section) => section.2 = segment.end,
            None => sections.push((key.clone(), segment.start, segment.end)),
        }
    }
    let markers: Vec<Value> = index
        .markers
        .iter()
        .map(|marker| {
            let (file, line) = location_fields(marker.location.as_ref());
            json!({ "name": marker.name, "time": marker.time, "file": file, "line": line })
        })
        .collect();
    json!({
        "schema": SCHEMA,
        "script": script.to_string_lossy(),
        "duration": index.duration,
        "segments": segments,
        "sections": sections
            .iter()
            .map(|(key, start, end)| json!({ "key": key, "start": start, "end": end }))
            .collect::<Vec<_>>(),
        "markers": markers,
        "voiceovers": index.voiceovers.iter().map(span_json).collect::<Vec<_>>(),
        "live_take": index.live_take.as_ref().map(span_json),
        "plays": index.plays.iter().map(span_json).collect::<Vec<_>>(),
    })
}

/// `file:line` relative to the script's folder, or empty.
fn place(script: &Path, location: Option<&ScriptLocation>) -> String {
    let Some(location) = location else {
        return String::new();
    };
    let folder = script.parent().unwrap_or(Path::new(""));
    let file = location.file.strip_prefix(folder).unwrap_or(&location.file);
    format!("{}:{}", file.display(), location.line)
}

/// The table `gaanim inspect` prints for `index`: one row per segment,
/// stop, marker and take, in timeline order.
fn index_table(script: &Path, index: &TimelineIndex) -> String {
    let stops: usize = index
        .segments
        .iter()
        .map(|segment| segment.stops.len())
        .sum();
    let mut rows: Vec<(f64, usize, String, String)> = Vec::new();
    for segment in &index.segments {
        rows.push((
            segment.start,
            0,
            format!(
                "{:>7.2}–{:<7.2} {}",
                segment.start, segment.end, segment.name
            ),
            place(script, segment.location.as_ref()),
        ));
        for stop in &segment.stops {
            rows.push((
                stop.time,
                1,
                format!(
                    "{:>7.2}           stop {}",
                    stop.time,
                    stop.name.as_deref().unwrap_or("·")
                ),
                place(script, stop.location.as_ref()),
            ));
        }
    }
    for marker in &index.markers {
        rows.push((
            marker.time,
            2,
            format!("{:>7.2}           marker {}", marker.time, marker.name),
            place(script, marker.location.as_ref()),
        ));
    }
    for (label, take) in index
        .voiceovers
        .iter()
        .map(|take| ("voiceover", take))
        .chain(index.live_take.iter().map(|take| ("live take", take)))
    {
        rows.push((
            take.start,
            3,
            format!(
                "{:>7.2}–{:<7.2}   {label} {}",
                take.start,
                take.end,
                take.key.as_deref().unwrap_or_default()
            ),
            place(script, take.location.as_ref()),
        ));
    }
    rows.sort_by(|left, right| left.0.total_cmp(&right.0).then(left.1.cmp(&right.1)));
    let width = rows
        .iter()
        .map(|(_, _, label, _)| label.chars().count())
        .max()
        .unwrap_or(0);
    let mut out = format!(
        "{} · {} segments · {} stops · {} markers · {:.2} s\n",
        console::display_path(script),
        index.segments.len(),
        stops,
        index.markers.len(),
        index.duration
    );
    for (_, _, label, place) in rows {
        let padding = width - label.chars().count();
        out.push_str(format!("{label}{}  {place}", " ".repeat(padding)).trim_end());
        out.push('\n');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use gaanim_api::canvas::{IndexedMarker, IndexedSegment, IndexedStop};

    fn index() -> TimelineIndex {
        TimelineIndex {
            duration: 6.0,
            segments: vec![
                IndexedSegment {
                    name: "Portada".into(),
                    section: None,
                    start: 0.0,
                    end: 2.0,
                    location: Some(ScriptLocation::new("/talk/main.py", 4)),
                    stops: vec![IndexedStop {
                        name: Some("titulo".into()),
                        time: 1.0,
                        location: Some(ScriptLocation::new("/talk/main.py", 6)),
                    }],
                },
                IndexedSegment {
                    name: "results · 1 · 1 · Datos".into(),
                    section: Some("results".into()),
                    start: 2.0,
                    end: 4.0,
                    location: None,
                    stops: Vec::new(),
                },
                IndexedSegment {
                    name: "results · 1 · 2 · Curvas".into(),
                    section: Some("results".into()),
                    start: 4.0,
                    end: 6.0,
                    location: None,
                    stops: Vec::new(),
                },
            ],
            markers: vec![IndexedMarker {
                name: "climax".into(),
                time: 3.0,
                location: None,
            }],
            voiceovers: vec![IndexedSpan {
                key: Some("intro".into()),
                start: 0.0,
                end: 1.5,
                location: Some(ScriptLocation::new("/talk/main.py", 5)),
            }],
            live_take: None,
            plays: vec![IndexedSpan {
                key: None,
                start: 0.0,
                end: 1.0,
                location: Some(ScriptLocation::new("/talk/main.py", 5)),
            }],
        }
    }

    #[test]
    fn parses_inspect_arguments() {
        let args = |list: &[&str]| list.iter().map(|arg| arg.to_string()).collect::<Vec<_>>();
        assert_eq!(
            parse_inspect_args(&args(&[".", "--json"])),
            Ok(InspectArgs {
                script: PathBuf::from("."),
                json: true
            })
        );
        assert!(parse_inspect_args(&args(&[])).is_err());
        assert!(parse_inspect_args(&args(&[".", "--yaml"])).is_err());
        assert!(parse_inspect_args(&args(&[".", "other"])).is_err());
    }

    #[test]
    fn the_json_lists_every_part_with_its_line() {
        let value = index_json(Path::new("/talk/main.py"), &index());
        assert_eq!(value["schema"], 1);
        assert_eq!(value["duration"], 6.0);
        let portada = &value["segments"][0];
        assert_eq!(portada["name"], "Portada");
        assert_eq!(portada["line"], 4);
        assert_eq!(portada["stops"][0]["name"], "titulo");
        assert_eq!(portada["stops"][0]["line"], 6);
        // Unknown lines are null, not missing.
        assert!(value["segments"][1]["file"].is_null());
        assert_eq!(value["segments"][1]["section"], "results");
        assert_eq!(
            value["sections"],
            json!([{ "key": "results", "start": 2.0, "end": 6.0 }])
        );
        assert_eq!(value["markers"][0]["time"], 3.0);
        assert_eq!(value["voiceovers"][0]["key"], "intro");
        assert!(value["live_take"].is_null());
        assert_eq!(value["plays"][0]["end"], 1.0);
        assert!(value["plays"][0].get("key").is_none());
    }

    #[test]
    fn the_table_lists_parts_in_timeline_order() {
        let table = index_table(Path::new("/talk/main.py"), &index());
        let lines: Vec<&str> = table.lines().collect();
        assert!(lines[0].ends_with("3 segments · 1 stops · 1 markers · 6.00 s"));
        assert!(lines[1].contains("Portada") && lines[1].ends_with("main.py:4"));
        assert!(lines[2].contains("voiceover intro") && lines[2].ends_with("main.py:5"));
        assert!(lines[3].contains("stop titulo") && lines[3].ends_with("main.py:6"));
        assert!(lines[4].contains("results · 1 · 1 · Datos"));
        assert!(lines[5].contains("marker climax"));
    }
}
