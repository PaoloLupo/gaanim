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
    let at = canvas
        .thumbnail_time()
        .unwrap_or_else(|| automatic_time(&timeline));
    let source = Source::Scene(Box::new(canvas.clone()));
    state.revision = Some(stash.revision);
    state.rendered_at = Some(Instant::now());
    let (sender, receiver) = crossbeam_channel::bounded(1);
    let spawned = std::thread::Builder::new()
        .name("gaanim-project-thumbnail".to_owned())
        .spawn(move || {
            let _ = sender.send(render_cover(source, at, &path));
        });
    if spawned.is_ok() {
        state.job = Some(receiver);
    }
}

/// Render the cover beside its file and move it in, so Home never reads half
/// a PNG.
fn render_cover(source: Source, at: f64, path: &std::path::Path) -> Result<(), String> {
    let temporary: PathBuf = path.with_extension("tmp.png");
    render_frame(source, at, &temporary, Some(LONG_EDGE))?;
    std::fs::rename(&temporary, path)
        .map_err(|error| format!("could not save the project's thumbnail: {error}"))
}

/// The instant a cover shows when the script does not choose one: the
/// first stop, as a bundle's cover, where a talk's first slide is complete.
/// Without stops a bundle picks the fullest frame of the first segment,
/// which takes rendering all of it; this takes the segment's end instead, or
/// for a video in one segment its middle, since many end on a fade.
fn automatic_time(timeline: &Timeline) -> f64 {
    let first_stop = timeline
        .segments
        .iter()
        .flat_map(|segment| segment.stops.iter().map(|stop| stop.time))
        .filter(|time| time.is_finite())
        .min_by(f64::total_cmp);
    if let Some(stop) = first_stop {
        return stop;
    }
    let Some(first) = timeline.segments.first() else {
        return 0.0;
    };
    if timeline.segments.len() > 1 {
        return first.end_time;
    }
    (first.start_time + first.end_time) / 2.0
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
    fn the_automatic_cover_is_the_first_stop_then_the_first_segment() {
        let mut timeline = Timeline::default();
        assert_eq!(automatic_time(&timeline), 0.0);
        timeline.segments = vec![segment(0, 0.0, 8.0, &[])];
        assert_eq!(automatic_time(&timeline), 4.0);
        timeline.segments = vec![segment(0, 0.0, 3.0, &[]), segment(1, 3.0, 9.0, &[])];
        assert_eq!(automatic_time(&timeline), 3.0);
        timeline.segments = vec![segment(0, 0.0, 3.0, &[1.5]), segment(1, 3.0, 9.0, &[])];
        assert_eq!(automatic_time(&timeline), 1.5);
        // A talk whose first slide does not stop rests first on a later one.
        timeline.segments = vec![segment(0, 0.0, 3.0, &[]), segment(1, 3.0, 9.0, &[4.0])];
        assert_eq!(automatic_time(&timeline), 4.0);
    }
}
