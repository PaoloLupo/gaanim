//! Running a live zone: players arrive, the standings move, and every frame
//! the zone's behavior poses each player's character.
//!
//! The behavior is stateless; what it needs of the past the run keeps for
//! it: when each player arrived and when their rank and score last changed.
//! It also notices when a player's expression changes, so the expression
//! plays from then. That check runs on a fixed grid of [`STEP`]s from the
//! zone's opening, so a replay seeked to a time is the one played to it.

use std::collections::HashSet;
use std::sync::Arc;

use gaanim_core::kurbo::{Affine, BezPath, Point, Vec2};
use gaanim_core::peniko::Brush;
use gaanim_objects::character::{Character, ExpressionPlay, catalog, character_seed, to_scene};

use super::program::{Inputs, Pose};
use super::spec::LiveZone;

/// The grid expressions are checked on, in seconds.
pub const STEP: f64 = 1.0 / 60.0;

/// A player as a zone sees it.
#[derive(Debug, Clone, PartialEq)]
pub struct Player {
    pub name: Arc<str>,
    pub character: Character,
}

impl Player {
    /// A player whose character is read from the nickname, as the relay
    /// gives a phone that did not choose one.
    pub fn named(name: &str) -> Self {
        let character = catalog().character_from_seed(character_seed(name));
        Self {
            name: name.into(),
            character,
        }
    }
}

/// An expression playing since `start` on the zone's clock.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Latch {
    express: usize,
    looped: bool,
    start: f64,
}

/// A player's character in a zone.
#[derive(Debug, Clone)]
struct Actor {
    player: Player,
    seed: u32,
    /// When it arrived, on the zone's clock.
    joined: f64,
    ranked: bool,
    rank: usize,
    previous_rank: usize,
    rank_changed: f64,
    score: f64,
    previous_score: f64,
    score_changed: f64,
    latch: Option<Latch>,
}

/// A zone's state: its clock, its players and what they remember.
#[derive(Debug, Clone, Default)]
pub struct ZoneRun {
    /// Seconds since the zone opened.
    pub clock: f64,
    /// The last grid point expressions were checked at.
    stepped: f64,
    actors: Vec<Actor>,
    arrived: HashSet<Arc<str>>,
    registers: Vec<f64>,
}

impl ZoneRun {
    /// A player arrived at `joined` on the zone's clock; a player arrives
    /// once.
    pub fn arrive(&mut self, player: Player, joined: f64) {
        if !self.arrived.insert(player.name.clone()) {
            return;
        }
        self.actors.push(Actor {
            seed: character_seed(&player.name),
            player,
            joined,
            ranked: false,
            rank: 0,
            previous_rank: 0,
            rank_changed: joined,
            score: 0.0,
            previous_score: 0.0,
            score_changed: joined,
            latch: None,
        });
    }

    /// Whether `name` has arrived.
    pub fn has(&self, name: &str) -> bool {
        self.arrived.contains(name)
    }

    /// How many characters are in the zone.
    pub fn len(&self) -> usize {
        self.actors.len()
    }

    pub fn is_empty(&self) -> bool {
        self.actors.is_empty()
    }

    /// Set the players' scores (missing ones score 0) and rank them, best
    /// first, earlier arrivals first among equals. Changes are dated now.
    pub fn standings(&mut self, score_of: impl Fn(&str) -> f64) {
        let clock = self.clock;
        for actor in &mut self.actors {
            let score = score_of(&actor.player.name);
            if actor.ranked && score != actor.score {
                actor.previous_score = actor.score;
                actor.score_changed = clock;
            } else if !actor.ranked {
                actor.previous_score = score;
            }
            actor.score = score;
        }
        let mut order: Vec<usize> = (0..self.actors.len()).collect();
        order.sort_by(|&a, &b| self.actors[b].score.total_cmp(&self.actors[a].score));
        for (rank, index) in order.into_iter().enumerate() {
            let actor = &mut self.actors[index];
            if !actor.ranked {
                actor.previous_rank = rank;
                actor.ranked = true;
            } else if actor.rank != rank {
                actor.previous_rank = actor.rank;
                actor.rank_changed = clock;
            }
            actor.rank = rank;
        }
    }

    fn inputs(&self, index: usize) -> Inputs {
        let actor = &self.actors[index];
        let clock = self.clock;
        Inputs {
            t: clock - actor.joined,
            time: clock,
            joined: actor.joined,
            index: index as f64,
            count: self.actors.len() as f64,
            rank: actor.rank as f64,
            score: actor.score,
            leader: self
                .actors
                .iter()
                .map(|actor| actor.score)
                .fold(0.0, f64::max),
            previous_rank: actor.previous_rank as f64,
            rank_since: clock - actor.rank_changed,
            previous_score: actor.previous_score,
            score_since: clock - actor.score_changed,
            seed: actor.seed,
        }
    }

    fn pose(&mut self, zone: &LiveZone, index: usize) -> Pose {
        let inputs = self.inputs(index);
        zone.behavior.eval(&inputs, &mut self.registers)
    }

    /// The next grid point, where [`ZoneRun::step`] moves the clock.
    pub fn next_step(&self) -> f64 {
        self.stepped + STEP
    }

    /// Move the clock to the next grid point and note expressions that
    /// changed, as started at the previous one.
    pub fn step(&mut self, zone: &LiveZone) {
        let previous = self.stepped;
        self.stepped += STEP;
        self.clock = self.stepped;
        for index in 0..self.actors.len() {
            let pose = self.pose(zone, index);
            let actor = &mut self.actors[index];
            actor.latch = pose.express.map(|express| match actor.latch {
                Some(latch) if latch.express == express && latch.looped == pose.looped => latch,
                _ => Latch {
                    express,
                    looped: pose.looped,
                    start: previous.max(actor.joined),
                },
            });
        }
    }

    /// Move the clock to `clock`, at or after the last grid point, without
    /// checking expressions.
    pub fn settle(&mut self, clock: f64) {
        self.clock = clock.max(self.stepped);
    }

    /// The zone's characters as filled paths in scene units, back to front.
    pub fn draw(&mut self, zone: &LiveZone) -> Vec<(BezPath, Brush)> {
        let catalog = catalog();
        let mut paths = Vec::new();
        for index in 0..self.actors.len() {
            let pose = self.pose(zone, index);
            let actor = &self.actors[index];
            let size = zone.size * pose.scale;
            if !pose.visible
                || !(size.is_finite() && size > 0.0)
                || !(pose.x.is_finite() && pose.y.is_finite() && pose.rotation.is_finite())
            {
                continue;
            }
            let expression = pose.express.map(|express| {
                let start = match (pose.since, actor.latch) {
                    (Some(since), _) => actor.joined + since,
                    (None, Some(latch)) if latch.express == express && latch.looped == pose.looped => {
                        latch.start
                    }
                    // Changed since the last grid point: it started there.
                    (None, _) => self.stepped.max(actor.joined),
                };
                ExpressionPlay {
                    name: zone.behavior.strings[express].clone(),
                    start,
                    looped: pose.looped,
                }
            });
            let local = to_scene(size);
            // The pose puts the feet at (x, y) and turns about the middle.
            let feet = local * Point::new(50.0, catalog.feet(&actor.player.character));
            let middle = Vec2::new(pose.x, pose.y) - feet.to_vec2();
            let flip = if pose.flip {
                Affine::scale_non_uniform(-1.0, 1.0)
            } else {
                Affine::IDENTITY
            };
            let place = Affine::translate(middle) * Affine::rotate(pose.rotation) * flip * local;
            for layer in catalog.pose(
                &actor.player.character,
                actor.seed,
                self.clock,
                expression.as_ref(),
            ) {
                let mut path = (*layer.outline).clone();
                path.apply_affine(place * layer.transform);
                paths.push((path, Brush::Solid(layer.color)));
            }
        }
        paths
    }

    /// Each character's pose now, by arrival, for tests and tools.
    pub fn poses(&mut self, zone: &LiveZone) -> Vec<(Arc<str>, Pose)> {
        (0..self.actors.len())
            .map(|index| (self.actors[index].player.name.clone(), self.pose(zone, index)))
            .collect()
    }

    /// The expression each character plays now and since when, by arrival.
    pub fn expressions(&mut self, zone: &LiveZone) -> Vec<Option<(String, f64)>> {
        (0..self.actors.len())
            .map(|index| {
                let pose = self.pose(zone, index);
                let actor = &self.actors[index];
                pose.express.map(|express| {
                    let start = match (pose.since, actor.latch) {
                        (Some(since), _) => actor.joined + since,
                        (None, Some(latch)) if latch.express == express => latch.start,
                        (None, _) => self.stepped.max(actor.joined),
                    };
                    (zone.behavior.strings[express].clone(), start)
                })
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::live::program::Program;

    /// The zipline of `tests/live/behaviors.py`, from the compiled cases.
    fn zipline() -> LiveZone {
        let entries: serde_json::Value =
            serde_json::from_str(include_str!("cases.json")).unwrap();
        let behavior: Program = serde_json::from_value(entries[0]["program"].clone()).unwrap();
        assert_eq!(behavior.name, "zipline");
        LiveZone {
            id: "z".into(),
            open: 0.0,
            close: 60.0,
            bounds: [-8.0, -4.5, 8.0, 4.5],
            size: 1.0,
            preview: Vec::new(),
            preview_every: 0.5,
            behavior: behavior.checked().unwrap(),
        }
    }

    fn played(zone: &LiveZone, names: &[&str], seconds: f64) -> ZoneRun {
        let mut run = ZoneRun::default();
        for (index, name) in names.iter().enumerate() {
            run.arrive(Player::named(name), index as f64 * 0.5);
        }
        run.standings(|_| 0.0);
        while run.next_step() <= seconds {
            run.step(zone);
        }
        run.settle(seconds);
        run
    }

    #[test]
    fn characters_land_where_the_behavior_says_and_react() {
        let zone = zipline();
        let mut run = played(&zone, &["Ana", "Beto", "Caro", "Dani"], 6.0);
        let expressions = run.expressions(&zone);
        let poses = run.poses(&zone);
        for (index, ((name, pose), expression)) in poses.into_iter().zip(expressions).enumerate() {
            let (mood, start) = expression.unwrap_or_else(|| panic!("{name} has no expression"));
            assert_eq!(mood, if pose.x > 1.0 { "sad" } else { "happy" }, "{name}");
            // `since` dates the expression at the landing, not a grid step.
            let joined = index as f64 * 0.5;
            assert_eq!(start, joined + pose.since.unwrap(), "{name}");
        }
    }

    #[test]
    fn a_player_arrives_once() {
        let mut run = ZoneRun::default();
        run.arrive(Player::named("Ana"), 0.0);
        run.arrive(Player::named("Ana"), 1.0);
        assert_eq!(run.len(), 1);
    }

    #[test]
    fn ranks_remember_their_last_change() {
        let zone = zipline();
        let mut run = ZoneRun::default();
        run.arrive(Player::named("Ana"), 0.0);
        run.arrive(Player::named("Beto"), 0.0);
        run.standings(|name| if name == "Ana" { 900.0 } else { 500.0 });
        for _ in 0..60 {
            run.step(&zone);
        }
        run.standings(|name| if name == "Ana" { 900.0 } else { 1200.0 });
        let beto = run.inputs(1);
        assert_eq!((beto.rank, beto.previous_rank, beto.rank_since), (0.0, 1.0, 0.0));
        assert_eq!((beto.previous_score, beto.score, beto.leader), (500.0, 1200.0, 1200.0));
        run.step(&zone);
        let ana = run.inputs(0);
        assert_eq!((ana.rank, ana.previous_rank), (1.0, 0.0));
        assert!((ana.rank_since - STEP).abs() < 1e-12);
    }

    #[test]
    fn expressions_start_on_the_grid_however_the_run_got_there() {
        // A behavior that turns happy at t = 1 without saying since when.
        let json = r#"{"version":[1,0],"name":"f","strings":["happy"],
            "code":[{"input":"t"},{"const":"1.0"},{"ge":[0,1]},{"const":"0.0"},
                    {"const":"-1.0"},{"select":[2,3,4]},{"const":"nan"}],
            "pose":{"x":3,"y":3,"rotation":3,"scale":1,"flip":3,"visible":1,
                    "express":5,"since":6,"loop":3}}"#;
        let mut zone = zipline();
        zone.behavior = Program::from_json(json).unwrap();
        let start = |run: &mut ZoneRun| run.expressions(&zone)[0].clone().unwrap().1;
        let mut once = played(&zone, &["Ana"], 1.5);
        let mut run = ZoneRun::default();
        run.arrive(Player::named("Ana"), 0.0);
        run.standings(|_| 0.0);
        for frame in 1..=45 {
            let at = frame as f64 / 30.0;
            while run.next_step() <= at {
                run.step(&zone);
            }
            run.settle(at);
        }
        assert_eq!(start(&mut once), start(&mut run));
        assert!(start(&mut once) <= 1.0 && start(&mut once) > 1.0 - 2.0 * STEP);
    }

    #[test]
    fn drawing_puts_each_character_on_its_feet() {
        let zone = zipline();
        let mut run = played(&zone, &["Ana"], 5.0);
        let pose = run.poses(&zone)[0].1;
        let paths = run.draw(&zone);
        assert!(!paths.is_empty());
        let bottom = paths
            .iter()
            .map(|(path, _)| gaanim_core::kurbo::Shape::bounding_box(path).y0)
            .fold(f64::INFINITY, f64::min);
        assert!((bottom - pose.y).abs() < zone.size * 0.1, "{bottom} vs {}", pose.y);
    }
}
