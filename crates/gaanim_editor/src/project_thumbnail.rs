//! The cover Home shows for a project: the frame at `scene.thumbnail(t)`,
//! or an automatic instant, rendered on a worker thread once the scene is
//! ready and saved in Gaanim's data folder (see
//! [`gaanim_project::project_thumbnail_path`]). Reloads refresh it at most
//! every few minutes, so editing never waits on it.

use std::path::PathBuf;
use std::time::Duration;

use bevy::platform::time::Instant;
use bevy::prelude::*;
use crossbeam_channel::{Receiver, TryRecvError};
use gaanim_timeline::timeline::Timeline;

use crate::PresentationMode;
use crate::export::{ProjectPaths, StashedReplay};
use crate::frame_capture::{Source, render_frame};

/// Longest side of a cover, in pixels: twice a Home card, for sharp
/// displays.
const LONG_EDGE: u32 = 480;

/// A scene that changed waits this long before its cover is rendered again.
const REFRESH: Duration = Duration::from_secs(5 * 60);

/// The cover being rendered, and the scene it last showed.
#[derive(Resource, Default)]
pub(crate) struct ProjectThumbnail {
    job: Option<Receiver<Result<(), String>>>,
    /// The replay the last cover was rendered from.
    revision: Option<u64>,
    rendered_at: Option<Instant>,
}

pub(crate) fn project_thumbnail_system(
    mut state: ResMut<ProjectThumbnail>,
    stash: Res<StashedReplay>,
    paths: Option<Res<ProjectPaths>>,
    timeline: Res<Timeline>,
    presentation: Res<PresentationMode>,
) {
    if crate::WEB {
        return;
    }
    if let Some(job) = &state.job {
        match job.try_recv() {
            Ok(result) => {
                if let Err(error) = result {
                    gaanim_core::console::warn("thumbnail", error);
                }
                state.job = None;
            }
            Err(TryRecvError::Disconnected) => state.job = None,
            Err(TryRecvError::Empty) => return,
        }
    }
    // A presentation keeps the GPU to itself.
    if presentation.active || state.revision == Some(stash.revision) {
        return;
    }
    if state
        .rendered_at
        .is_some_and(|rendered| rendered.elapsed() < REFRESH)
    {
        return;
    }
    let (Some(paths), Some(canvas)) = (paths, &stash.canvas) else {
        return;
    };
    let Some(path) = gaanim_project::project_thumbnail_path(&paths.project_dir) else {
        return;
    };
    let times = match canvas.thumbnail_time() {
        Some(at) => vec![at],
        None => cover_times(&timeline),
    };
    let canvas = canvas.clone();
    state.revision = Some(stash.revision);
    state.rendered_at = Some(Instant::now());
    let (sender, receiver) = crossbeam_channel::bounded(1);
    let spawned = std::thread::Builder::new()
        .name("gaanim-project-thumbnail".to_owned())
        .spawn(move || {
            let _ = sender.send(render_cover(canvas, &times, &path));
        });
    if spawned.is_ok() {
        state.job = Some(receiver);
    }
}

/// Render each of `times` beside the cover's file, keep the one that shows
/// the most, and move it in, so Home never reads half a PNG.
fn render_cover(
    canvas: gaanim_api::canvas::SceneModel,
    times: &[f64],
    path: &std::path::Path,
) -> Result<(), String> {
    let mut best: Option<(usize, PathBuf)> = None;
    for (index, &at) in times.iter().enumerate() {
        let candidate: PathBuf = path.with_extension(format!("{index}.tmp.png"));
        render_frame(
            Source::Scene(Box::new(canvas.clone())),
            at,
            &candidate,
            Some(LONG_EDGE),
        )?;
        // A single instant needs no judging.
        let shown = if times.len() == 1 {
            0
        } else {
            std::fs::read(&candidate)
                .ok()
                .and_then(|png| gaanim_thumbnail::scaled(&png, 96).ok())
                .map(|image| content(image.as_raw()))
                .unwrap_or(0)
        };
        match &best {
            Some((most, _)) if *most >= shown => {
                let _ = std::fs::remove_file(&candidate);
            }
            _ => {
                if let Some((_, previous)) = best.replace((shown, candidate)) {
                    let _ = std::fs::remove_file(previous);
                }
            }
        }
    }
    let (_, chosen) = best.ok_or("no instant to render the thumbnail at")?;
    std::fs::rename(&chosen, path)
        .map_err(|error| format!("could not save the project's thumbnail: {error}"))
}

/// How much of an RGBA image is not its background: the pixels that differ
/// clearly from the top-left one.
fn content(rgba: &[u8]) -> usize {
    let Some(background) = rgba.get(..4) else {
        return 0;
    };
    rgba.chunks_exact(4)
        .filter(|pixel| {
            pixel
                .iter()
                .zip(background)
                .take(3)
                .any(|(channel, back)| channel.abs_diff(*back) > 24)
        })
        .count()
}

/// The instants a cover may show when the script does not choose one: the
/// first stop, as a bundle's cover, where a talk's first slide is
/// complete; without stops, a few instants through the first segment (and
/// its end, when more follow), of which the one that shows the most wins,
/// as a bundle picks the fullest frame of its first segment.
fn cover_times(timeline: &Timeline) -> Vec<f64> {
    let first_stop = timeline
        .segments
        .iter()
        .flat_map(|segment| segment.stops.iter().map(|stop| stop.time))
        .filter(|time| time.is_finite())
        .min_by(f64::total_cmp);
    if let Some(stop) = first_stop {
        return vec![stop];
    }
    let Some(first) = timeline.segments.first() else {
        return vec![0.0];
    };
    let span = first.end_time - first.start_time;
    let mut times: Vec<f64> = [0.35, 0.6, 0.85]
        .iter()
        .map(|share| first.start_time + span * share)
        .collect();
    if timeline.segments.len() > 1 {
        times.push(first.end_time);
    }
    times
}

#[cfg(test)]
mod tests {
    use super::*;
    use gaanim_timeline::timeline::{SegmentMetadata, SegmentStop};

    fn segment(id: u32, start_time: f64, end_time: f64, stops: &[f64]) -> SegmentMetadata {
        SegmentMetadata {
            id,
            name: format!("s{id}"),
            notes: None,
            target: None,
            start_time,
            end_time,
            stops: stops
                .iter()
                .map(|&time| SegmentStop {
                    name: None,
                    time,
                    ambient: None,
                })
                .collect(),
        }
    }

    #[test]
    fn the_automatic_cover_is_the_first_stop_else_instants_of_the_first_segment() {
        let mut timeline = Timeline::default();
        assert_eq!(cover_times(&timeline), [0.0]);
        timeline.segments = vec![segment(0, 0.0, 8.0, &[])];
        assert_eq!(cover_times(&timeline), [2.8, 4.8, 6.8]);
        timeline.segments = vec![segment(0, 0.0, 4.0, &[]), segment(1, 4.0, 9.0, &[])];
        assert_eq!(cover_times(&timeline), [1.4, 2.4, 3.4, 4.0]);
        timeline.segments = vec![segment(0, 0.0, 3.0, &[1.5]), segment(1, 3.0, 9.0, &[])];
        assert_eq!(cover_times(&timeline), [1.5]);
        // A talk whose first slide does not stop rests first on a later one.
        timeline.segments = vec![segment(0, 0.0, 3.0, &[]), segment(1, 3.0, 9.0, &[4.0])];
        assert_eq!(cover_times(&timeline), [4.0]);
    }

    #[test]
    fn content_counts_what_differs_from_the_background() {
        let dark = [10, 10, 10, 255];
        let ink = [240, 200, 60, 255];
        let empty: Vec<u8> = dark.repeat(16);
        assert_eq!(content(&empty), 0);
        let mut drawn = empty.clone();
        drawn[20..24].copy_from_slice(&ink);
        drawn[40..44].copy_from_slice(&ink);
        assert_eq!(content(&drawn), 2);
        // Noise below the threshold is background.
        let mut faint = empty;
        faint[20..24].copy_from_slice(&[20, 20, 20, 255]);
        assert_eq!(content(&faint), 0);
    }
}
