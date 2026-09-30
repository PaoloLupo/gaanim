//! Live zones: parts of a scene where the audience's characters play while
//! a presentation takes votes (`scene.live_zone`).
//!
//! A scene authors a zone's behavior as a plain Python function, compiled
//! when the scene is authored into a [`Program`]: data that travels in a
//! `.gaanim` bundle and runs here, in Rust, so a presented bundle plays the
//! zone without Python. The engine gives the behavior facts about each
//! player (see [`program::Input`]); the behavior decides where the
//! character stands and which expression it plays.
//!
//! While presenting, a zone the timeline is in runs on the wall clock: each
//! player who joins arrives in it as their own character. Everywhere else
//! (previews, exports, snapshots) it replays its preview players as a pure
//! function of the timeline's time, so seeks and exports stay exact. Either
//! way it draws into [`LiveOverlay`], which the renderer draws above the
//! scene and bundle recordings leave out.

pub mod program;
pub mod run;
pub mod spec;

use std::collections::HashMap;

use bevy::prelude::{Res, ResMut, Resource, Time};
use bevy::time::Real;
use gaanim_core::kurbo::{BezPath, Rect};
use gaanim_core::peniko::Brush;

use crate::polls::PollResults;
use crate::updaters::PlaybackState;
pub use program::{Inputs, Pose, Program, ProgramError};
pub use run::{Player, STEP, ZoneRun};
pub use spec::LiveZone;

/// The live zones of the scene, as compiled or read from a bundle.
#[derive(Resource, Debug, Clone, Default, PartialEq)]
pub struct LiveZones(pub Vec<LiveZone>);

/// What live zones draw this frame, in scene units, above the scene.
#[derive(Resource, Debug, Clone, Default)]
pub struct LiveOverlay {
    pub groups: Vec<OverlayGroup>,
}

/// Paths drawn clipped to `clip`, back to front.
#[derive(Debug, Clone)]
pub struct OverlayGroup {
    pub clip: Rect,
    pub paths: Vec<(BezPath, Brush)>,
}

/// The runs of the zones: live ones by zone id, and the preview last
/// replayed, kept while the timeline moves forward.
#[derive(Resource, Default)]
pub struct LiveRuns {
    live: HashMap<String, ZoneRun>,
    previews: HashMap<String, ZoneRun>,
    /// Whether the runs are live, so switching drops the other kind.
    was_live: bool,
}

/// Longest wall-clock step a live zone takes in one frame, so a stall does
/// not throw characters across the zone.
const MAX_FRAME: f64 = 0.1;

fn zone_contains(zone: &LiveZone, time: f64) -> bool {
    time >= zone.open - 1e-6 && time <= zone.close + 1e-6
}

/// A live player: with the character the phone made, or one read from the
/// nickname.
fn player(results: &PollResults, name: &std::sync::Arc<str>) -> Player {
    match results.avatars.get(name) {
        Some(character) => Player {
            name: name.clone(),
            character: *character,
        },
        None => Player::named(name),
    }
}

/// Run the zones the timeline is in and draw them into [`LiveOverlay`].
pub fn live_zone_system(
    zones: Option<Res<LiveZones>>,
    results: Option<Res<PollResults>>,
    playback: Option<Res<PlaybackState>>,
    time: Option<Res<Time<Real>>>,
    mut runs: bevy::prelude::Local<LiveRuns>,
    mut overlay: ResMut<LiveOverlay>,
) {
    let empty = PollResults::default();
    let results = results.as_deref().unwrap_or(&empty);
    let now = playback.map_or(0.0, |playback| playback.current_time);
    let live = results.live;
    if live != runs.was_live {
        runs.live.clear();
        runs.previews.clear();
        runs.was_live = live;
    }
    let zones = zones.as_deref().map_or(&[][..], |zones| zones.0.as_slice());
    if zones.is_empty() && overlay.groups.is_empty() {
        return;
    }
    let dt = time
        .map_or(0.0, |time| time.delta_secs_f64())
        .min(MAX_FRAME);
    let mut groups = Vec::new();
    for zone in zones.iter().filter(|zone| zone_contains(zone, now)) {
        let run = if live {
            let run = runs.live.entry(zone.id.clone()).or_default();
            let joined = run.clock;
            for (name, _) in &results.audience {
                run.arrive(player(results, name), joined);
            }
            let scores: HashMap<&str, f64> = results
                .leaderboard
                .iter()
                .map(|(name, score)| (&**name, *score as f64))
                .collect();
            run.standings(|name| scores.get(name).copied().unwrap_or(0.0));
            let target = run.clock + dt;
            while run.next_step() <= target {
                run.step(zone);
            }
            run.settle(target);
            run
        } else {
            // A pure function of the timeline's time: replay from the
            // zone's opening, reusing the last replay while time moves on.
            let clock = (now - zone.open).max(0.0);
            let run = runs.previews.entry(zone.id.clone()).or_default();
            if run.clock > clock + 1e-9 {
                *run = ZoneRun::default();
            }
            let every = zone.preview_every.max(STEP);
            let count = zone.preview.len();
            // Preview players keep the order they are listed in.
            let preview_score = |name: &str| {
                zone.preview
                    .iter()
                    .position(|other| other == name)
                    .map_or(0.0, |rank| ((count - rank) * 100) as f64)
            };
            let arrive = |run: &mut ZoneRun, until: f64| {
                for (index, name) in zone.preview.iter().enumerate() {
                    let joined = index as f64 * every;
                    if joined <= until + 1e-9 && !run.has(name) {
                        run.arrive(Player::named(name), joined);
                    }
                }
                run.standings(preview_score);
            };
            while run.next_step() <= clock + 1e-9 {
                let next = run.next_step();
                arrive(run, next);
                run.step(zone);
            }
            run.settle(clock);
            arrive(run, clock);
            run
        };
        let [x0, y0, x1, y1] = zone.bounds;
        groups.push(OverlayGroup {
            clip: Rect::new(x0, y0, x1, y1),
            paths: run.draw(zone),
        });
    }
    overlay.groups = groups;
}
