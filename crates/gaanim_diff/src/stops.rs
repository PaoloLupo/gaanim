//! Capture every authored `scene.stop(...)` without script cooperation.

use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

use gaanim_api::canvas::{SceneModel, SegmentManifest};
use serde::{Deserialize, Serialize};

use crate::{DiffError, Result, SnapshotManifest, capture_canvas_as};

pub const STOPS_FILE: &str = "stops.json";

/// `stops.json`: which authored stop each captured snapshot shows.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StopsManifest {
    pub schema_version: u32,
    /// Number of stops authored in the scene, captured or not.
    pub total: usize,
    pub stops: Vec<StopEntry>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StopEntry {
    /// 1-based position among all stops, in timeline order.
    pub index: usize,
    /// Snapshot id in `manifest.json`; stable while the stop keeps its index.
    pub id: String,
    pub time_seconds: f64,
    pub segment: String,
    pub name: Option<String>,
    pub file: String,
}

#[derive(Debug, Clone)]
pub struct StopCapture {
    pub manifest: SnapshotManifest,
    pub stops: StopsManifest,
}

/// Parse a 1-based stop selection such as `12,30` or `3-7,9`.
pub fn parse_stop_selection(spec: &str) -> Result<Vec<usize>> {
    let invalid = |part: &str| {
        DiffError::InvalidInput(format!(
            "invalid stop `{part}`: use 1-based numbers or ranges, e.g. 12,30 or 3-7"
        ))
    };
    let number = |part: &str| match part.trim().parse::<usize>() {
        Ok(value) if value > 0 => Ok(value),
        _ => Err(invalid(part)),
    };
    let mut selected = BTreeSet::new();
    for part in spec.split(',') {
        let part = part.trim();
        if part.is_empty() {
            return Err(invalid(part));
        }
        match part.split_once('-') {
            Some((start, end)) => {
                let (start, end) = (number(start)?, number(end)?);
                if start > end {
                    return Err(invalid(part));
                }
                selected.extend(start..=end);
            }
            None => {
                selected.insert(number(part)?);
            }
        }
    }
    Ok(selected.into_iter().collect())
}

/// `(segment, stop name, time)` for every stop, in timeline order.
fn authored_stops(manifest: &SegmentManifest) -> Vec<(String, Option<String>, f64)> {
    manifest
        .segments
        .iter()
        .flat_map(|segment| {
            segment
                .stops
                .iter()
                .map(|stop| (segment.name.clone(), stop.name.clone(), stop.time))
        })
        .collect()
}

/// Number authored stops from 1 and keep the selected ones (all when `None`).
fn select_stops(
    authored: Vec<(String, Option<String>, f64)>,
    selection: Option<&[usize]>,
) -> Result<(usize, Vec<StopEntry>)> {
    let all: Vec<StopEntry> = authored
        .into_iter()
        .enumerate()
        .map(|(position, (segment, name, time_seconds))| {
            let index = position + 1;
            let id = format!("stop_{index:04}");
            StopEntry {
                index,
                file: format!("{id}.png"),
                id,
                time_seconds,
                segment,
                name,
            }
        })
        .collect();
    let total = all.len();
    if total == 0 {
        return Err(DiffError::InvalidInput(
            "the scene has no stops; add scene.stop() where the presentation pauses".to_string(),
        ));
    }
    let Some(selection) = selection else {
        return Ok((total, all));
    };
    if let Some(missing) = selection
        .iter()
        .find(|index| **index == 0 || **index > total)
    {
        return Err(DiffError::InvalidInput(format!(
            "stop {missing} does not exist; the scene has {total} stop(s)"
        )));
    }
    let selected = all
        .into_iter()
        .filter(|stop| selection.contains(&stop.index))
        .collect();
    Ok((total, selected))
}

/// Global 1-based numbers of the stops inside the segments chosen by
/// `selection`, intersected with an explicit `--stops` list when given.
pub fn stops_in_selection(
    manifest: &SegmentManifest,
    selection: &gaanim_timeline::selection::SegmentSelection,
    stops: Option<&[usize]>,
) -> Result<Vec<usize>> {
    let segments: Vec<(&str, f64, f64, usize)> = manifest
        .segments
        .iter()
        .map(|segment| {
            (
                segment.name.as_str(),
                segment.start_time,
                segment.end_time,
                segment.stops.len(),
            )
        })
        .collect();
    select_stop_numbers(&segments, selection, stops)
}

/// `stops_in_selection` over `(name, start, end, stop count)` segments.
fn select_stop_numbers(
    segments: &[(&str, f64, f64, usize)],
    selection: &gaanim_timeline::selection::SegmentSelection,
    stops: Option<&[usize]>,
) -> Result<Vec<usize>> {
    let resolved = selection
        .resolve(
            segments
                .iter()
                .map(|(name, start, end, _)| (*name, *start, *end)),
        )
        .map_err(DiffError::InvalidInput)?;
    let mut number = 0;
    let mut selected = Vec::new();
    for (index, (_, _, _, count)) in segments.iter().enumerate() {
        for _ in 0..*count {
            number += 1;
            if resolved.segments.contains(&index)
                && stops.is_none_or(|stops| stops.contains(&number))
            {
                selected.push(number);
            }
        }
    }
    if selected.is_empty() {
        return Err(DiffError::InvalidInput(
            "the selected segments have no stops to capture".to_string(),
        ));
    }
    Ok(selected)
}

/// Capture the frame shown at each selected stop and write `stops.json`.
///
/// Each stop is captured at its exact time, the frame the presenter shows
/// while paused there: preceding animations are finished and a terminal stop
/// keeps the completed segment on screen.
pub fn capture_stops(
    canvas: SceneModel,
    output_dir: impl AsRef<Path>,
    selection: Option<&[usize]>,
) -> Result<StopCapture> {
    let output_dir = output_dir.as_ref();
    let (total, stops) = select_stops(authored_stops(&canvas.segment_manifest()), selection)?;
    let times: Vec<f64> = stops.iter().map(|stop| stop.time_seconds).collect();
    let ids: Vec<String> = stops.iter().map(|stop| stop.id.clone()).collect();
    let manifest = capture_canvas_as(canvas, output_dir, &times, &ids)?;
    let stops = StopsManifest {
        schema_version: 1,
        total,
        stops,
    };
    fs::write(
        output_dir.join(STOPS_FILE),
        serde_json::to_vec_pretty(&stops)?,
    )?;
    Ok(StopCapture { manifest, stops })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn deck() -> Vec<(String, Option<String>, f64)> {
        [
            ("intro", Some("title"), 1.0),
            ("intro", None, 2.5),
            ("results", Some("chart"), 4.0),
        ]
        .into_iter()
        .map(|(segment, name, time)| (segment.to_string(), name.map(str::to_string), time))
        .collect()
    }

    #[test]
    fn section_selection_keeps_global_stop_numbers() {
        let segments = [
            ("intro", 0.0, 1.0, 1),
            ("results · 1 · 1 · A", 1.0, 2.0, 2),
            ("close", 2.0, 3.0, 1),
        ];
        let selection = gaanim_timeline::selection::SegmentSelection {
            sections: vec!["results".into()],
            from: None,
        };
        assert_eq!(
            select_stop_numbers(&segments, &selection, None).unwrap(),
            vec![2, 3]
        );
        assert_eq!(
            select_stop_numbers(&segments, &selection, Some(&[3, 4])).unwrap(),
            vec![3]
        );
        let from = gaanim_timeline::selection::SegmentSelection {
            sections: Vec::new(),
            from: Some("RESULTS".into()),
        };
        assert_eq!(
            select_stop_numbers(&segments, &from, None).unwrap(),
            vec![2, 3, 4]
        );
        assert!(select_stop_numbers(&segments, &selection, Some(&[1])).is_err());
    }

    #[test]
    fn parses_numbers_and_ranges_sorted_without_duplicates() {
        assert_eq!(parse_stop_selection("12,30").unwrap(), vec![12, 30]);
        assert_eq!(
            parse_stop_selection(" 7 , 3-5,4").unwrap(),
            vec![3, 4, 5, 7]
        );
        assert_eq!(parse_stop_selection("2-2").unwrap(), vec![2]);
    }

    #[test]
    fn rejects_zero_empty_and_reversed_selections() {
        for spec in ["", "0", "1,,2", "5-3", "a", "1-", "-2"] {
            assert!(parse_stop_selection(spec).is_err(), "{spec}");
        }
    }

    #[test]
    fn numbers_stops_across_segments_in_timeline_order() {
        let (total, stops) = select_stops(deck(), None).unwrap();

        assert_eq!(total, 3);
        let summary: Vec<_> = stops
            .iter()
            .map(|stop| {
                (
                    stop.index,
                    stop.id.as_str(),
                    stop.segment.as_str(),
                    stop.time_seconds,
                )
            })
            .collect();
        assert_eq!(
            summary,
            [
                (1, "stop_0001", "intro", 1.0),
                (2, "stop_0002", "intro", 2.5),
                (3, "stop_0003", "results", 4.0),
            ]
        );
        assert_eq!(stops[2].name.as_deref(), Some("chart"));
        assert_eq!(stops[2].file, "stop_0003.png");
    }

    #[test]
    fn selection_keeps_ids_of_the_full_numbering() {
        let (total, stops) = select_stops(deck(), Some(&[3])).unwrap();

        assert_eq!(total, 3);
        assert_eq!(stops.len(), 1);
        assert_eq!(stops[0].id, "stop_0003");
    }

    #[test]
    fn rejects_scenes_without_stops_and_out_of_range_selections() {
        assert!(select_stops(Vec::new(), None).is_err());
        let error = select_stops(deck(), Some(&[4])).unwrap_err().to_string();
        assert!(error.contains("stop 4 does not exist"), "{error}");
        assert!(error.contains("3 stop(s)"), "{error}");
    }
}
