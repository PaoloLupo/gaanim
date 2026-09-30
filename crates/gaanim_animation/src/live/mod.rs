//! Live zones: parts of a scene where the audience's characters play while
//! a presentation takes votes (`scene.live_zone`).
//!
//! A scene authors a zone as data ([`LiveZone`]): its surfaces, launchers,
//! podium places or race, and rules that make characters react. The data
//! travels in a `.gaanim` bundle and runs here, in Rust, so a presented
//! bundle plays the zone without Python.
//!
//! While presenting, a zone the timeline is in runs on the wall clock: each
//! player who joins arrives in it as their own character. Everywhere else
//! (previews, exports, snapshots) it replays its preview players as a pure
//! function of the timeline's time, so seeks and exports stay exact. Either
//! way it draws into [`LiveOverlay`], which the renderer draws above the
//! scene and bundle recordings leave out.

pub mod sim;
pub mod spec;

use std::collections::HashMap;

use bevy::prelude::{Res, ResMut, Resource, Time};
use bevy::time::Real;
use gaanim_core::kurbo::{BezPath, Rect};
use gaanim_core::peniko::Brush;

use crate::polls::PollResults;
use crate::updaters::PlaybackState;
pub use sim::{Player, STEP, ZoneRun};
pub use spec::{Event, Express, Launcher, LiveZone, Place, Race, Rule, Surface, Wave};

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

/// The standings for places and races: live, or the preview players with
/// scores from best to worst.
fn standings(zone: &LiveZone, results: &PollResults, live: bool) -> Vec<(Player, u64)> {
    if live {
        results
            .leaderboard
            .iter()
            .map(|(name, score)| (player(results, name), *score))
            .collect()
    } else {
        let count = zone.preview.len() as u64;
        zone.preview
            .iter()
            .enumerate()
            .map(|(rank, name)| (Player::named(name), (count - rank as u64) * 100))
            .collect()
    }
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
        let standings = standings(zone, results, live);
        let run = if live {
            let run = runs.live.entry(zone.id.clone()).or_default();
            for (name, _) in &results.audience {
                run.arrive(zone, player(results, name));
            }
            let target = run.clock + dt;
            while run.clock + STEP <= target {
                run.step(zone, &standings);
            }
            run
        } else {
            // A pure function of the timeline's time: replay from the
            // zone's opening, reusing the last replay while time moves on.
            let clock = (now - zone.open).max(0.0);
            let run = runs.previews.entry(zone.id.clone()).or_default();
            if run.clock > clock + 1e-9 {
                *run = ZoneRun::default();
            }
            while run.clock + STEP <= clock {
                let arrivals = (run.clock / zone.preview_every.max(STEP)).floor() as usize + 1;
                for name in zone.preview.iter().take(arrivals) {
                    run.arrive(zone, Player::named(name));
                }
                run.step(zone, &standings);
            }
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
