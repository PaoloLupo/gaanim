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
//! (previews, exports, snapshots) it replays the scene's rehearsal
//! ([`crate::rehearsal`]) as a pure function of the timeline's time, so
//! seeks and exports stay exact. Either
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
use crate::rehearsal::Rehearsal;
use crate::updaters::PlaybackState;
pub use program::{Inputs, MAX_STATE, Pose, Program, ProgramError};
pub use run::{Player, STEP, ZoneRun};
pub use spec::{LiveZone, Motion, NameGlyph, ZoneNames, ZoneState};

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
    let named = Player::named(name);
    Player {
        character: results
            .avatars
            .get(name)
            .copied()
            .unwrap_or(named.character),
        team: results.player_teams.get(name).copied().unwrap_or(0),
        stats: results.stats.get(name).copied().unwrap_or_default(),
        answer: results
            .latest
            .as_ref()
            .and_then(|poll| results.answers.get(poll))
            .and_then(|answers| answers.get(name))
            .copied(),
        ..named
    }
}

/// Replay the rehearsal's players in `zone` up to the timeline's `now`: a
/// pure function of time, reusing `run` while time moves forward. Players
/// arrive as they join, or as the zone opens for those already in the room.
pub fn replay_rehearsal(zone: &LiveZone, now: f64, run: &mut ZoneRun, rehearsal: &Rehearsal) {
    let clock = (now - zone.open).max(0.0);
    if run.clock > clock + 1e-9 {
        *run = ZoneRun::default();
    }
    let arrive = |run: &mut ZoneRun, until: f64| {
        let time = zone.open + until;
        for (index, player) in rehearsal.players.iter().enumerate() {
            if player.joined <= time + 1e-9 && !run.has(&player.name) {
                run.arrive(
                    rehearsal.player(index),
                    (player.joined - zone.open).max(0.0),
                );
            }
        }
        let scores = rehearsal.scores(time);
        let index_of = |name: &str| {
            rehearsal
                .players
                .iter()
                .position(|player| player.name == name)
        };
        run.standings(|name| index_of(name).map_or(0.0, |index| scores[index].0 as f64));
        let stats = rehearsal.stats_at(time);
        let latest = rehearsal.latest_at(time);
        run.facts(|name| match index_of(name) {
            Some(index) => (
                stats[index],
                latest.and_then(|poll| rehearsal.answer_of(poll, index, time)),
            ),
            None => Default::default(),
        });
    };
    while run.next_step() <= clock + 1e-9 {
        let next = run.next_step();
        arrive(run, next);
        run.step(zone);
    }
    run.settle(clock);
    arrive(run, clock);
}

/// Replays of the rehearsal in zones, for drawing them over recorded frames
/// (a bundle exported to video) as the scene draws them.
#[derive(Default)]
pub struct PreviewReplay {
    runs: HashMap<String, ZoneRun>,
}

impl PreviewReplay {
    /// What `zones` draw at the timeline's `time`, with `rehearsal`'s
    /// players; nothing without one.
    pub fn overlay(
        &mut self,
        zones: &[LiveZone],
        rehearsal: Option<&Rehearsal>,
        time: f64,
    ) -> LiveOverlay {
        let Some(rehearsal) = rehearsal else {
            return LiveOverlay::default();
        };
        let groups = zones
            .iter()
            .filter(|zone| zone_contains(zone, time))
            .map(|zone| {
                let run = self.runs.entry(zone.id.clone()).or_default();
                replay_rehearsal(zone, time, run, rehearsal);
                let [x0, y0, x1, y1] = zone.bounds;
                OverlayGroup {
                    clip: Rect::new(x0, y0, x1, y1),
                    paths: run.draw(zone),
                }
            })
            .collect();
        LiveOverlay { groups }
    }
}

/// Run the zones the timeline is in and draw them into [`LiveOverlay`].
pub fn live_zone_system(
    zones: Option<Res<LiveZones>>,
    results: Option<Res<PollResults>>,
    rehearsal: Option<Res<Rehearsal>>,
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
        } else if let Some(rehearsal) = rehearsal.as_deref() {
            let run = runs.previews.entry(zone.id.clone()).or_default();
            replay_rehearsal(zone, now, run, rehearsal);
            run
        } else {
            continue;
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
    fn a_rehearsed_zone_ranks_teams_by_their_points() {
        use crate::rehearsal::{Lean, PlannedPoll, Rehearsal, RehearsalSpec, RehearsalTeams};
        // x is the player's team rank, y its team.
        let json = r#"{"version":[1,1],"code":[{"input":"team_rank"},{"input":"team"},{"const":"1.0"},{"const":"0.0"}],
            "pose":{"x":0,"y":1,"rotation":3,"scale":2,"sx":2,"sy":2,"lean":3,
                    "look_x":3,"look_y":3,"show_name":3,"flip":3,"visible":2,"express":3,"since":3,"loop":3}}"#;
        let zone = LiveZone {
            id: "z".into(),
            open: 10.0,
            close: 20.0,
            stop_at_open: false,
            stop_at_close: false,
            bounds: [-8.0, -4.5, 8.0, 4.5],
            size: 1.0,
            behavior: Program::from_json(json).unwrap(),
            motion: Motion::default(),
            names: None,
            state: None,
        };
        let spec = RehearsalSpec {
            names: RehearsalSpec::names(19),
            team_skill: vec![0.9, 0.1],
            ..Default::default()
        };
        let polls = [PlannedPoll {
            id: "q".into(),
            answers: 4,
            open: 0.0,
            due: 5.0,
            quiz: Some((1 << 1, 20, 1000)),
            multiple: false,
            lean: Lean::Right(0.6),
        }];
        let teams = Some(RehearsalTeams {
            count: 2,
            choose: false,
        });
        let rehearsal = Rehearsal::plan(&spec, None, &polls, teams);
        let results = rehearsal.results_at(12.0);
        assert!(results.teams[0].score > results.teams[1].score);
        let mut run = ZoneRun::default();
        replay_rehearsal(&zone, 12.0, &mut run, &rehearsal);
        for (name, pose) in run.poses(&zone) {
            let expected = if pose.y == 0.0 { 0.0 } else { 1.0 };
            assert_eq!(pose.x, expected, "{name} in team {}", pose.y);
        }
    }

    #[test]
    fn kept_numbers_step_with_the_zone_and_replay_the_same() {
        use crate::rehearsal::{Rehearsal, RehearsalSpec};
        // Compiled by gaanim.live: count(p) returns state(steps=p.state.steps + 1)
        // and show(p) poses at (p.state.steps, p.state.start).
        let update = r#"{"version":[1,2],"name":"count","strings":[],"code":[{"state":0},{"const":"1.0"},{"add":[0,1]},{"state":1}],"next":[2,3]}"#;
        let behavior = r#"{"version":[1,2],"name":"show","strings":[],"code":[{"state":0},{"state":1},{"const":"0.0"},{"const":"1.0"},{"const":"-1.0"},{"const":"nan"}],"pose":{"x":0,"y":1,"rotation":2,"scale":3,"sx":3,"sy":3,"lean":2,"look_x":5,"look_y":5,"show_name":3,"flip":2,"visible":3,"express":4,"since":5,"loop":2}}"#;
        let zone = LiveZone {
            id: "z".into(),
            open: 0.0,
            close: 10.0,
            stop_at_open: false,
            stop_at_close: false,
            bounds: [-8.0, -4.5, 8.0, 4.5],
            size: 1.0,
            behavior: Program::from_json(behavior).unwrap(),
            motion: Motion::default(),
            names: None,
            state: Some(ZoneState {
                names: vec!["steps".into(), "start".into()],
                initial: vec![0.0, 7.0],
                update: Program::from_json(update).unwrap(),
            }),
        };
        let spec = RehearsalSpec {
            names: RehearsalSpec::names(1),
            ..Default::default()
        };
        // One player, there since long before the zone opens.
        let rehearsal = Rehearsal::plan(&spec, None, &[], None);
        let pose_at = |run: &mut ZoneRun, time: f64| {
            replay_rehearsal(&zone, time, run, &rehearsal);
            run.poses(&zone)[0].1
        };
        let mut going = ZoneRun::default();
        let half = pose_at(&mut going, 0.5);
        // A step every sixtieth of a second; the start kept as given.
        assert!((half.x - 30.0).abs() <= 1.0, "{}", half.x);
        assert_eq!(half.y, 7.0);
        let later = pose_at(&mut going, 1.0);
        assert!((later.x - 60.0).abs() <= 1.0, "{}", later.x);
        // Seeking replays from the opening and lands on the same numbers.
        assert_eq!(pose_at(&mut ZoneRun::default(), 1.0).x, later.x);
        assert_eq!(pose_at(&mut going, 0.5).x, half.x);
    }

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
            behavior: Program::from_json(json).unwrap(),
            motion: Motion::default(),
            names: None,
            state: None,
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
