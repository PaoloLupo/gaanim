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
use gaanim_objects::character::{
    CHARACTER_ENVELOPE, Character, CharacterDrive, ExpressionPlay, catalog, character_seed,
    follow_lag, to_scene,
};

use super::program::{Inputs, Pose};
use super::spec::LiveZone;

fn finite_or(value: f64, fallback: f64) -> f64 {
    if value.is_finite() { value } else { fallback }
}

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

    /// How fast the pose's feet move now, measured across two grid steps.
    fn velocity(&mut self, zone: &LiveZone, index: usize) -> Vec2 {
        let inputs = self.inputs(index);
        let before = zone.behavior.eval(&inputs.later(-STEP), &mut self.registers);
        let after = zone.behavior.eval(&inputs.later(STEP), &mut self.registers);
        let velocity = Vec2::new(after.x - before.x, after.y - before.y) / (2.0 * STEP);
        if velocity.is_finite() {
            velocity
        } else {
            Vec2::ZERO
        }
    }

    /// Where a character's paths go and what moves its parts: its feet
    /// at the pose, squashed and leaning about them, turned and mirrored
    /// about its middle and stretched along its velocity when the zone
    /// asks; its hanging extras lag behind its motion and its eyes look
    /// where it goes, unless the pose says where.
    fn placement(
        &mut self,
        zone: &LiveZone,
        index: usize,
        pose: &Pose,
        size: f64,
    ) -> (Affine, CharacterDrive) {
        let catalog = catalog();
        let local = to_scene(size);
        let character = self.actors[index].player.character;
        // Feet at the origin; the middle is where the drawing is centred.
        let feet = local * Point::new(50.0, catalog.feet(&character));
        let middle = -feet.to_vec2();
        let about = |point: Vec2, transform: Affine| {
            Affine::translate(point) * transform * Affine::translate(-point)
        };
        let flip = if pose.flip {
            Affine::scale_non_uniform(-1.0, 1.0)
        } else {
            Affine::IDENTITY
        };
        let body = about(middle, Affine::rotate(pose.rotation) * flip)
            * Affine::scale_non_uniform(finite_or(pose.sx, 1.0), finite_or(pose.sy, 1.0))
            * Affine::translate(middle)
            * local;
        let motion = zone.motion;
        let velocity = self.velocity(zone, index);
        let mut lean = finite_or(pose.lean, 0.0);
        lean -= (motion.lean * velocity.x).clamp(-motion.max_lean, motion.max_lean);
        let mut stretch = Affine::IDENTITY;
        let speed = velocity.hypot();
        let ratio = (1.0 + motion.squash * speed).clamp(1.0, motion.max_stretch.max(1.0));
        if ratio > 1.0 + 1e-9 {
            let along = Affine::rotate(velocity.atan2());
            stretch = about(
                middle,
                along * Affine::scale_non_uniform(ratio, 1.0 / ratio) * along.inverse(),
            );
        }
        let place = Affine::translate((pose.x, pose.y)) * Affine::rotate(lean) * stretch * body;

        // Scene directions (y up) to the drawing's (y down, maybe mirrored).
        let mirror = if pose.flip { -1.0 } else { 1.0 };
        let look = match pose.look {
            Some((x, y)) => (mirror * x, -y),
            None => (
                mirror * motion.look * velocity.x,
                -motion.look * velocity.y,
            ),
        };
        let mut drive = CharacterDrive {
            lag: (0.0, 0.0),
            look,
        };
        if motion.follow > 0.0 && catalog.hangs(&character) {
            let (step, samples, frequency, damping) = catalog.follow_sampling();
            let inputs = self.inputs(index);
            let positions: Vec<(f64, f64)> = (0..=samples + 1)
                .map(|j| {
                    let then = inputs.later(-(j as f64) * step);
                    let pose = zone.behavior.eval(&then, &mut self.registers);
                    (pose.x, pose.y)
                })
                .collect();
            if positions.iter().all(|(x, y)| x.is_finite() && y.is_finite()) {
                let (x, y) = follow_lag(&positions, step, frequency, damping);
                // Into the body's frame, then character units.
                let turned = Affine::rotate(-(pose.rotation + lean)) * Point::new(x, y);
                let units = CHARACTER_ENVELOPE.height() / size * motion.follow;
                drive.lag = (mirror * turned.x * units, -turned.y * units);
            }
        }
        (place, drive)
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
            let size = zone.size * pose.scale;
            if !pose.visible
                || !(size.is_finite() && size > 0.0)
                || !(pose.x.is_finite() && pose.y.is_finite() && pose.rotation.is_finite())
            {
                continue;
            }
            let (place, drive) = self.placement(zone, index, &pose, size);
            let actor = &self.actors[index];
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
            for layer in catalog.pose_driven(
                &actor.player.character,
                actor.seed,
                self.clock,
                expression.as_ref(),
                &drive,
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
    use crate::live::spec::Motion;

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
            stop_at_open: false,
            stop_at_close: false,
            bounds: [-8.0, -4.5, 8.0, 4.5],
            size: 1.0,
            preview: Vec::new(),
            preview_every: 0.5,
            behavior: behavior.checked().unwrap(),
            motion: Default::default(),
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
            "pose":{"x":3,"y":3,"rotation":3,"scale":1,"sx":1,"sy":1,"lean":3,"look_x":6,"look_y":6,"flip":3,"visible":1,
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

    #[test]
    fn moving_characters_stretch_and_lean_into_their_motion() {
        // x = speed * t, standing on y = 0.
        let behavior = |speed: &str| {
            let json = format!(
                r#"{{"version":[1,0],"name":"run","strings":[],
                "code":[{{"input":"t"}},{{"const":"{speed}"}},{{"mul":[0,1]}},{{"const":"0.0"}},
                        {{"const":"1.0"}},{{"const":"-1.0"}},{{"const":"nan"}}],
                "pose":{{"x":2,"y":3,"rotation":3,"scale":4,"sx":4,"sy":4,"lean":3,"look_x":6,"look_y":6,"flip":3,
                        "visible":4,"express":5,"since":6,"loop":3}}}}"#
            );
            Program::from_json(&json).unwrap()
        };
        let extent = |speed: &str, motion: Motion| {
            let mut zone = zipline();
            zone.behavior = behavior(speed);
            zone.motion = motion;
            let mut run = played(&zone, &["Ana"], 1.0);
            let bounds = run
                .draw(&zone)
                .iter()
                .map(|(path, _)| gaanim_core::kurbo::Shape::bounding_box(path))
                .reduce(|a, b| a.union(b))
                .unwrap();
            (bounds.width(), bounds.height(), bounds.center().x - run.poses(&zone)[0].1.x)
        };
        let motion = Motion {
            squash: 0.05,
            lean: 0.05,
            ..Motion::default()
        };
        let still = extent("0.0", motion);
        let running = extent("4.0", motion);
        let plain = extent("4.0", Motion::default());
        let same = |a: f64, b: f64| (a - b).abs() < 1e-9;
        assert!(
            same(still.0, plain.0) && same(still.1, plain.1) && same(still.2, plain.2),
            "only motion deforms: {still:?} vs {plain:?}"
        );
        // Stretched along the run and squashed across it.
        assert!(running.0 > still.0 && running.1 < still.1, "{running:?} vs {still:?}");
        // Leaning forward puts the body ahead of the feet.
        assert!(running.2 > still.2 + 0.01, "{running:?} vs {still:?}");
    }

    #[test]
    fn ears_follow_through_after_a_stop_and_eyes_look_ahead() {
        // Runs right at 4 units/s until t = 1, then stops dead.
        let json = r#"{"version":[1,0],"name":"dash","strings":[],
            "code":[{"input":"t"},{"const":"4.0"},{"mul":[0,1]},{"min":[2,1]},
                    {"const":"0.0"},{"const":"1.0"},{"const":"-1.0"},{"const":"nan"}],
            "pose":{"x":3,"y":4,"rotation":4,"scale":5,"sx":5,"sy":5,"lean":4,
                    "look_x":7,"look_y":7,"flip":4,"visible":5,"express":6,"since":7,"loop":4}}"#;
        let mut zone = zipline();
        zone.behavior = Program::from_json(json).unwrap();
        let bunny = Player {
            name: "Ana".into(),
            character: [3, 4, 6, 6, 4],
        };
        assert!(catalog().hangs(&bunny.character));
        let drive_at = |seconds: f64| {
            let mut run = ZoneRun::default();
            run.arrive(bunny.clone(), 0.0);
            run.standings(|_| 0.0);
            while run.next_step() <= seconds {
                run.step(&zone);
            }
            run.settle(seconds);
            let pose = run.poses(&zone)[0].1;
            run.placement(&zone, 0, &pose, zone.size).1
        };
        let running = drive_at(0.5);
        let stopped = drive_at(1.15);
        let settled = drive_at(4.0);
        // Running steadily: no swing, eyes ahead.
        assert!(running.lag.0.abs() < 1e-6, "{running:?}");
        assert!(running.look.0 > 0.5, "{running:?}");
        // Just stopped: the ears keep going forward, then settle.
        assert!(stopped.lag.0 > 1.0, "{stopped:?}");
        assert!(settled.lag.0.abs() < 1e-6 && settled.look == (0.0, -0.0), "{settled:?}");
    }
}
