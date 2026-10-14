//! Where in the script each part of the timeline was authored, with its
//! times: segments, stops, markers, voiceovers, the live take and every
//! `play`. `gaanim inspect` prints it, and an editor follows the cursor
//! with it.
//!
//! Lines are recorded only while [`track_script_lines`] is on, like the
//! inspector's; without it the index still has every time.
//!
//! [`track_script_lines`]: super::track_script_lines

use std::collections::HashMap;
use std::path::Path;

use gaanim_core::console::ScriptLocation;

use super::SceneModel;
use super::segment::SegmentId;

/// The script lines of the timeline's parts, kept out of the operations
/// like the drawables' lines: moving a line must not change what a reload
/// reuses.
#[derive(Debug, Default)]
pub(crate) struct AuthoredTimeline {
    pub segments: HashMap<SegmentId, ScriptLocation>,
    /// By segment and the stop's position among the segment's stops.
    pub stops: HashMap<(SegmentId, usize), ScriptLocation>,
    pub markers: HashMap<String, ScriptLocation>,
    /// Voiceovers and the live take, by key.
    pub takes: HashMap<String, ScriptLocation>,
    /// Each `play`: its segment, its start and end in the segment, and its
    /// line.
    pub plays: Vec<(SegmentId, f64, f64, ScriptLocation)>,
}

/// The script line of the authoring call being made, when lines are
/// tracked.
pub(crate) fn authoring_site() -> Option<ScriptLocation> {
    if super::profile::tracks_script_lines() {
        super::profile::call_site()
    } else {
        None
    }
}

/// A stop of an [`IndexedSegment`].
#[derive(Debug, Clone, PartialEq)]
pub struct IndexedStop {
    pub name: Option<String>,
    pub time: f64,
    pub location: Option<ScriptLocation>,
}

/// A segment and its stops.
#[derive(Debug, Clone, PartialEq)]
pub struct IndexedSegment {
    pub name: String,
    /// The key of the `Section` that generated it, if one did.
    pub section: Option<String>,
    pub start: f64,
    pub end: f64,
    pub location: Option<ScriptLocation>,
    pub stops: Vec<IndexedStop>,
}

/// A named instant: a `scene.marker`.
#[derive(Debug, Clone, PartialEq)]
pub struct IndexedMarker {
    pub name: String,
    pub time: f64,
    pub location: Option<ScriptLocation>,
}

/// A span of the timeline authored by one call: a voiceover, the live take
/// or a `play`. `key` names a take; a `play` has none.
#[derive(Debug, Clone, PartialEq)]
pub struct IndexedSpan {
    pub key: Option<String>,
    pub start: f64,
    pub end: f64,
    pub location: Option<ScriptLocation>,
}

/// The timeline of a scene with the script line of each part, in timeline
/// order.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TimelineIndex {
    pub duration: f64,
    pub segments: Vec<IndexedSegment>,
    pub markers: Vec<IndexedMarker>,
    pub voiceovers: Vec<IndexedSpan>,
    pub live_take: Option<IndexedSpan>,
    pub plays: Vec<IndexedSpan>,
}

impl TimelineIndex {
    /// The instant the line `line` of `file` stands for: the part authored
    /// on it, or else the closest line above it in the same file that
    /// authored one. A segment stands for its start, a stop or marker for
    /// its time, a take for its start, and a `play` for its end, where what
    /// it animates has arrived. Several parts on one line take the earliest.
    pub fn instant_at(&self, file: &Path, line: u32) -> Option<f64> {
        let file = gaanim_core::console::plain_path(file);
        let mut best: Option<(u32, f64)> = None;
        let mut consider = |location: &Option<ScriptLocation>, time: f64| {
            let Some(location) = location else {
                return;
            };
            if location.line > line || *location.file != *file {
                return;
            }
            best = match best {
                Some((best_line, best_time))
                    if best_line > location.line
                        || (best_line == location.line && best_time <= time) =>
                {
                    Some((best_line, best_time))
                }
                _ => Some((location.line, time)),
            };
        };
        for segment in &self.segments {
            consider(&segment.location, segment.start);
            for stop in &segment.stops {
                consider(&stop.location, stop.time);
            }
        }
        for marker in &self.markers {
            consider(&marker.location, marker.time);
        }
        for take in self.voiceovers.iter().chain(&self.live_take) {
            consider(&take.location, take.start);
        }
        for play in &self.plays {
            consider(&play.location, play.end);
        }
        best.map(|(_, time)| time)
    }
}

impl SceneModel {
    /// Keep the script line starting the voiceover or live take `key`.
    pub(crate) fn record_take_site(&self, key: &str) {
        if let Some(site) = authoring_site() {
            self.state
                .lock()
                .expect("canvas state poisoned")
                .authored_timeline
                .takes
                .insert(key.to_owned(), site);
        }
    }

    /// The timeline's segments, stops, markers, takes and plays with their
    /// times and, while lines are tracked, the script line of each.
    pub fn timeline_index(&self) -> TimelineIndex {
        let manifest = self.segment_manifest();
        let markers = self.markers();
        let narration = self.narration_manifest();
        let state = self.state.lock().expect("canvas state poisoned");
        let authored = &state.authored_timeline;
        let starts: HashMap<SegmentId, f64> = manifest
            .segments
            .iter()
            .map(|segment| (segment.id, segment.start_time))
            .collect();
        let segments = manifest
            .segments
            .iter()
            .map(|segment| IndexedSegment {
                name: segment.name.clone(),
                section: gaanim_timeline::selection::section_key(&segment.name).map(str::to_owned),
                start: segment.start_time,
                end: segment.end_time,
                location: authored.segments.get(&segment.id).cloned(),
                stops: segment
                    .stops
                    .iter()
                    .enumerate()
                    .map(|(index, stop)| IndexedStop {
                        name: stop.name.clone(),
                        time: stop.time,
                        location: authored.stops.get(&(segment.id, index)).cloned(),
                    })
                    .collect(),
            })
            .collect();
        let markers = markers
            .into_iter()
            .map(|marker| IndexedMarker {
                location: authored.markers.get(&marker.name).cloned(),
                name: marker.name,
                time: marker.time,
            })
            .collect();
        let take = |key: &str, start: f64, length: Option<f64>| IndexedSpan {
            key: Some(key.to_owned()),
            start,
            end: start + length.unwrap_or(0.0),
            location: authored.takes.get(key).cloned(),
        };
        let voiceovers = narration
            .voiceovers
            .iter()
            .map(|voice| take(&voice.key, voice.start_time, Some(voice.duration)))
            .collect();
        let live_take = narration
            .live
            .as_ref()
            .map(|live| take(&live.key, live.start_time, live.duration));
        let mut plays: Vec<IndexedSpan> = authored
            .plays
            .iter()
            .filter_map(|(segment, start, end, location)| {
                let offset = starts.get(segment)?;
                Some(IndexedSpan {
                    key: None,
                    start: offset + start,
                    end: offset + end,
                    location: Some(location.clone()),
                })
            })
            .collect();
        plays.sort_by(|left, right| left.start.total_cmp(&right.start));
        TimelineIndex {
            duration: manifest.duration(),
            segments,
            markers,
            voiceovers,
            live_take,
            plays,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(file: &str, line: u32) -> Option<ScriptLocation> {
        Some(ScriptLocation::new(file, line))
    }

    #[test]
    fn a_line_stands_for_the_part_authored_on_or_above_it() {
        let index = TimelineIndex {
            duration: 10.0,
            segments: vec![
                IndexedSegment {
                    name: "Portada".into(),
                    section: None,
                    start: 0.0,
                    end: 4.0,
                    location: at("main.py", 3),
                    stops: vec![IndexedStop {
                        name: None,
                        time: 2.0,
                        location: at("main.py", 6),
                    }],
                },
                IndexedSegment {
                    name: "Datos".into(),
                    section: None,
                    start: 4.0,
                    end: 10.0,
                    location: at("main.py", 10),
                    stops: Vec::new(),
                },
            ],
            markers: vec![IndexedMarker {
                name: "climax".into(),
                time: 7.0,
                location: at("main.py", 14),
            }],
            voiceovers: Vec::new(),
            live_take: None,
            plays: vec![
                IndexedSpan {
                    key: None,
                    start: 0.0,
                    end: 1.5,
                    location: at("main.py", 5),
                },
                IndexedSpan {
                    key: None,
                    start: 4.0,
                    end: 6.0,
                    location: at("helpers.py", 5),
                },
            ],
        };
        let file = Path::new("main.py");
        assert_eq!(index.instant_at(file, 3), Some(0.0));
        // A play stands for its end; a line between parts for the one above.
        assert_eq!(index.instant_at(file, 5), Some(1.5));
        assert_eq!(index.instant_at(file, 6), Some(2.0));
        assert_eq!(index.instant_at(file, 8), Some(2.0));
        assert_eq!(index.instant_at(file, 12), Some(4.0));
        assert_eq!(index.instant_at(file, 99), Some(7.0));
        // Above every part, or in a file without one, there is none.
        assert_eq!(index.instant_at(file, 1), None);
        assert_eq!(index.instant_at(Path::new("other.py"), 5), None);
        assert_eq!(index.instant_at(Path::new("helpers.py"), 5), Some(6.0));
    }

    #[test]
    fn the_index_holds_absolute_times_and_section_keys() {
        let mut scene = SceneModel::new(1280, 720);
        scene.segment("Portada", None).unwrap();
        scene.wait(1.0);
        scene.stop(Some("titulo".into())).unwrap();
        scene.wait(1.0);
        scene.segment("results · 1 · 1 · Datos", None).unwrap();
        scene.wait(0.5);
        scene.marker("climax").unwrap();
        scene.wait(0.5);
        scene.stop(None).unwrap();

        let index = scene.timeline_index();
        assert_eq!(index.duration, 3.0);
        let segments: Vec<_> = index
            .segments
            .iter()
            .map(|segment| {
                (
                    segment.name.as_str(),
                    segment.section.as_deref(),
                    segment.start,
                    segment.end,
                )
            })
            .collect();
        assert_eq!(
            segments,
            [
                ("Portada", None, 0.0, 2.0),
                ("results · 1 · 1 · Datos", Some("results"), 2.0, 3.0),
            ]
        );
        let stops: Vec<_> = index
            .segments
            .iter()
            .flat_map(|segment| &segment.stops)
            .map(|stop| (stop.name.as_deref(), stop.time))
            .collect();
        assert_eq!(stops, [(Some("titulo"), 1.0), (None, 3.0)]);
        assert_eq!(index.markers[0].name, "climax");
        assert_eq!(index.markers[0].time, 2.5);
        // Without tracked lines, nothing carries a location.
        assert!(
            index
                .segments
                .iter()
                .all(|segment| segment.location.is_none())
        );
        assert!(index.plays.is_empty());
    }
}
