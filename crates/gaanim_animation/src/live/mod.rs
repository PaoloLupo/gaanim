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
pub use spec::{LiveZone, Motion, NameGlyph, ZoneNames};

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

/// Whether the timeline at `time` is in the zone. Its ends follow segment
/// boundaries: at `open` the zone shows unless a stop rests there (holding
/// what came before), and at `close` only if one does (holding the zone).
pub fn zone_contains(zone: &LiveZone, time: f64) -> bool {
    const EPSILON: f64 = 1e-5;
    if (time - zone.close).abs() <= EPSILON {
        return zone.stop_at_close;
    }
    if (time - zone.open).abs() <= EPSILON {
        return !zone.stop_at_open;
    }
    time > zone.open && time < zone.close
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
            let roster: Vec<Player> = results
                .audience
                .iter()
                .map(|(name, _)| player(results, name))
                .collect();
            run.follow(&roster);
            let joined = run.clock;
            for player in roster {
                run.arrive(player, joined);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_stop_at_a_shared_boundary_holds_the_outgoing_zone() {
        let json = r#"{"version":[1,0],"code":[{"const":"0.0"}],
            "pose":{"x":0,"y":0,"rotation":0,"scale":0,"sx":0,"sy":0,"lean":0,
                    "look_x":0,"look_y":0,"show_name":0,"flip":0,"visible":0,"express":0,"since":0,"loop":0}}"#;
        let zone = |open: f64, close: f64| LiveZone {
            id: "z".into(),
            open,
            close,
            stop_at_open: false,
            stop_at_close: false,
            bounds: [0.0, 0.0, 1.0, 1.0],
            size: 1.0,
            preview: Vec::new(),
            preview_every: 1.0,
            behavior: Program::from_json(json).unwrap(),
            motion: Motion::default(),
            names: None,
        };
        // Two segments meeting at 8, the first ending in a stop.
        let mut sala = zone(0.0, 8.0);
        sala.stop_at_close = true;
        let mut carrera = zone(8.0, 12.0);
        carrera.stop_at_open = true;
        assert!(zone_contains(&sala, 8.0) && !zone_contains(&carrera, 8.0));
        assert!(!zone_contains(&sala, 8.01) && zone_contains(&carrera, 8.01));
        // Without a stop the incoming zone owns the boundary.
        let (sala, carrera) = (zone(0.0, 8.0), zone(8.0, 12.0));
        assert!(!zone_contains(&sala, 8.0) && zone_contains(&carrera, 8.0));
        assert!(zone_contains(&sala, 7.99) && zone_contains(&sala, 0.0));
    }
}
