//! Running a live zone: characters arrive, fly, land, take their places
//! and react. [`ZoneRun::step`] advances it by a fixed step, so a run fed
//! the same arrivals and standings is the same everywhere.

use std::collections::{HashMap, VecDeque};
use std::sync::Arc;

use gaanim_core::kurbo::{Affine, BezPath, Point, Vec2};
use gaanim_core::peniko::Brush;
use gaanim_objects::character::{Character, ExpressionPlay, catalog, character_seed, to_scene};

use super::spec::{Event, Express, LiveZone};

/// The simulation step, in seconds.
pub const STEP: f64 = 1.0 / 120.0;

/// How a character is moving.
#[derive(Debug, Clone, PartialEq)]
enum Motion {
    Flying,
    Landed,
    /// Standing at a podium place or running a race.
    Placed,
}

/// A player's character in a zone.
#[derive(Debug, Clone)]
struct Actor {
    name: Arc<str>,
    character: Character,
    seed: u32,
    /// Where its feet are, in scene units.
    at: Point,
    velocity: Vec2,
    motion: Motion,
    /// When it was launched, for its spin in the air.
    launched: f64,
    expression: Option<ExpressionPlay>,
    rank: Option<usize>,
}

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

/// A zone's state: its clock, characters and those waiting to launch.
#[derive(Debug, Clone, Default)]
pub struct ZoneRun {
    /// Seconds the zone has run.
    pub clock: f64,
    actors: Vec<Actor>,
    waiting: VecDeque<Player>,
    next_launch: f64,
    /// Players who arrived, so none arrives twice.
    arrived: HashMap<Arc<str>, ()>,
}

impl ZoneRun {
    /// A player arrived: launched in turn, dropped in, or kept for its
    /// place or race lane.
    pub fn arrive(&mut self, zone: &LiveZone, player: Player) {
        if self.arrived.insert(player.name.clone(), ()).is_some() {
            return;
        }
        if zone.places.is_empty() && zone.race.is_none() {
            self.waiting.push_back(player);
        }
    }

    /// How many characters are in the zone.
    pub fn len(&self) -> usize {
        self.actors.len()
    }

    pub fn is_empty(&self) -> bool {
        self.actors.is_empty()
    }

    fn react(actor: &mut Actor, zone: &LiveZone, clock: f64, event: &Event) {
        for rule in &zone.rules {
            let matches = match (&rule.on, event) {
                (Event::Land { tag: None }, Event::Land { .. }) => true,
                (on, happened) => on == happened,
            };
            if matches {
                actor.expression = Some(play(&rule.express, clock));
            }
        }
    }

    /// Advance by one [`STEP`]. `standings` are the players by rank with
    /// their scores, best first, for places and races.
    pub fn step(&mut self, zone: &LiveZone, standings: &[(Player, u64)]) {
        self.clock += STEP;
        let clock = self.clock;
        self.launch(zone);
        let actor_width = zone.size * 0.8;
        let bottom = zone.bounds[1];
        let (left, right) = (zone.bounds[0], zone.bounds[2]);
        for index in 0..self.actors.len() {
            if self.actors[index].motion != Motion::Flying {
                continue;
            }
            let before = self.actors[index].at;
            let actor = &mut self.actors[index];
            actor.velocity.y -= zone.gravity * STEP;
            actor.at += actor.velocity * STEP;
            if actor.at.x < left || actor.at.x > right {
                actor.at.x = actor.at.x.clamp(left, right);
                actor.velocity.x = -actor.velocity.x * 0.5;
            }
            let landing = zone.surfaces.iter().find_map(|surface| {
                let (from, to) = (surface.from, surface.to);
                let (x0, x1) = (from[0].min(to[0]), from[0].max(to[0]));
                let x = actor.at.x;
                if actor.velocity.y > 0.0 || x < x0 || x > x1 || (to[0] - from[0]).abs() < 1e-9 {
                    return None;
                }
                let t = (x - from[0]) / (to[0] - from[0]);
                let y = from[1] + (to[1] - from[1]) * t;
                (before.y >= y && actor.at.y <= y)
                    .then(|| (surface.tag.clone(), y - surface.sink * zone.size, (x0, x1)))
            });
            let landing = landing.or_else(|| {
                (actor.at.y <= bottom).then(|| ("bottom".to_string(), bottom, (left, right)))
            });
            if let Some((tag, y, (x0, x1))) = landing {
                actor.at.y = y;
                actor.velocity = Vec2::ZERO;
                actor.motion = Motion::Landed;
                let x = actor.at.x;
                // Make room beside the characters already there.
                let taken: Vec<f64> = self
                    .actors
                    .iter()
                    .enumerate()
                    .filter(|(other, actor)| {
                        *other != index
                            && actor.motion == Motion::Landed
                            && (actor.at.y - y).abs() < 1e-6
                    })
                    .map(|(_, actor)| actor.at.x)
                    .collect();
                let free = free_spot(x, &taken, actor_width, x0, x1);
                let actor = &mut self.actors[index];
                actor.at.x = free;
                Self::react(actor, zone, clock, &Event::Land { tag: Some(tag) });
            }
        }
        self.place(zone, standings);
    }

    fn launch(&mut self, zone: &LiveZone) {
        if self.clock < self.next_launch {
            return;
        }
        let Some(player) = self.waiting.pop_front() else {
            return;
        };
        let clock = self.clock;
        let seed = character_seed(&player.name);
        let (at, velocity, every) = match zone.launchers.first() {
            Some(launcher) => {
                let angle = launcher.angle.at(clock).to_radians();
                let velocity = Vec2::new(launcher.direction.signum() * angle.cos(), angle.sin())
                    * launcher.speed;
                (
                    Point::new(launcher.from[0], launcher.from[1]),
                    velocity,
                    launcher.every,
                )
            }
            None => {
                // Dropped in from the top, at a place read from the name.
                let [x0, _, x1, y1] = zone.bounds;
                let margin = zone.size / 2.0;
                let u = f64::from(seed & 0xffff) / 65535.0;
                let x = x0 + margin + (x1 - x0 - 2.0 * margin).max(0.0) * u;
                (Point::new(x, y1 - zone.size), Vec2::ZERO, 0.25)
            }
        };
        let mut actor = Actor {
            name: player.name,
            character: player.character,
            seed,
            at,
            velocity,
            motion: Motion::Flying,
            launched: clock,
            expression: None,
            rank: None,
        };
        Self::react(&mut actor, zone, clock, &Event::Join);
        self.actors.push(actor);
        self.next_launch = clock + every.max(0.0);
    }

    /// Podium places and race lanes follow the standings.
    fn place(&mut self, zone: &LiveZone, standings: &[(Player, u64)]) {
        if zone.places.is_empty() && zone.race.is_none() {
            return;
        }
        let clock = self.clock;
        let leader = standings.first().map_or(0, |(_, score)| *score);
        let mut wanted: Vec<(Player, Point, usize, Option<&Express>)> = Vec::new();
        for place in &zone.places {
            if let Some((player, _)) = standings.get(place.rank) {
                let at = Point::new(place.at[0], place.at[1]);
                wanted.push((player.clone(), at, place.rank, place.express.as_ref()));
            }
        }
        if let Some(race) = &zone.race {
            for (rank, (player, score)) in standings.iter().take(race.count).enumerate() {
                let fraction = if leader > 0 {
                    *score as f64 / leader as f64
                } else {
                    0.0
                };
                let at = Point::new(
                    race.origin[0]
                        + race.step[0] * rank as f64
                        + race.direction[0] * race.length * fraction,
                    race.origin[1]
                        + race.step[1] * rank as f64
                        + race.direction[1] * race.length * fraction,
                );
                wanted.push((player.clone(), at, rank, None));
            }
        }
        // Characters no longer placed leave; new ones come in at their place.
        self.actors
            .retain(|actor| wanted.iter().any(|(player, ..)| player.name == actor.name));
        for (player, at, rank, express) in wanted {
            let index = match self
                .actors
                .iter()
                .position(|actor| actor.name == player.name)
            {
                Some(index) => index,
                None => {
                    let mut actor = Actor {
                        seed: character_seed(&player.name),
                        name: player.name.clone(),
                        character: player.character,
                        at,
                        velocity: Vec2::ZERO,
                        motion: Motion::Placed,
                        launched: clock,
                        expression: None,
                        rank: None,
                    };
                    Self::react(&mut actor, zone, clock, &Event::Join);
                    self.actors.push(actor);
                    self.actors.len() - 1
                }
            };
            let actor = &mut self.actors[index];
            // Glide to the place, so a change of rank reads as a move.
            let pull = (STEP * 6.0).min(1.0);
            actor.at += (at - actor.at) * pull;
            if let Some(previous) = actor.rank
                && previous != rank
            {
                let event = if rank < previous {
                    Event::RankUp
                } else {
                    Event::RankDown
                };
                Self::react(actor, zone, clock, &event);
                if rank == 0 {
                    Self::react(actor, zone, clock, &Event::Leader);
                }
            }
            actor.rank = Some(rank);
            if let Some(express) = express {
                let same = actor
                    .expression
                    .as_ref()
                    .is_some_and(|playing| playing.name == express.name);
                if !same {
                    actor.expression = Some(play(express, clock));
                }
            }
        }
    }

    /// The zone's characters as filled paths in scene units, back to front.
    pub fn draw(&self, zone: &LiveZone) -> Vec<(BezPath, Brush)> {
        let catalog = catalog();
        let local = to_scene(zone.size);
        let mut paths = Vec::new();
        for actor in &self.actors {
            // The character's feet are at the base of its body.
            let feet = local * Point::new(50.0, catalog.feet(&actor.character));
            let spin = if actor.motion == Motion::Flying {
                let turns = (self.clock - actor.launched) * 1.2;
                Affine::rotate(-turns * std::f64::consts::TAU * actor.velocity.x.signum())
            } else {
                Affine::IDENTITY
            };
            let place =
                Affine::translate(actor.at.to_vec2()) * spin * Affine::translate(-feet.to_vec2());
            for layer in catalog.pose(
                &actor.character,
                actor.seed,
                self.clock,
                actor.expression.as_ref(),
            ) {
                let mut path = (*layer.outline).clone();
                path.apply_affine(place * local * layer.transform);
                paths.push((path, Brush::Solid(layer.color)));
            }
        }
        paths
    }
}

fn play(express: &Express, clock: f64) -> ExpressionPlay {
    ExpressionPlay {
        name: express.name.clone(),
        start: clock,
        looped: express.looped,
    }
}

/// The spot nearest `x` in [x0, x1] at least `width` from every one taken.
fn free_spot(x: f64, taken: &[f64], width: f64, x0: f64, x1: f64) -> f64 {
    let fits = |candidate: f64| {
        taken
            .iter()
            .all(|other| (other - candidate).abs() >= width - 1e-9)
    };
    if fits(x) {
        return x;
    }
    let mut offset = width;
    while offset <= (x1 - x0) + width {
        for candidate in [x + offset, x - offset] {
            if candidate >= x0 && candidate <= x1 && fits(candidate) {
                return candidate;
            }
        }
        offset += width / 4.0;
    }
    x
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::live::spec::{Launcher, Place, Rule, Surface, Wave};

    fn zipline() -> LiveZone {
        LiveZone {
            id: "z".into(),
            open: 0.0,
            close: 60.0,
            bounds: [-8.0, -4.5, 8.0, 4.5],
            size: 1.0,
            gravity: 20.0,
            preview: Vec::new(),
            preview_every: 0.5,
            surfaces: vec![
                Surface {
                    tag: "floor".into(),
                    from: [-8.0, -3.0],
                    to: [0.0, -3.0],
                    sink: 0.0,
                },
                Surface {
                    tag: "water".into(),
                    from: [0.0, -3.5],
                    to: [8.0, -3.5],
                    sink: 0.3,
                },
            ],
            launchers: vec![Launcher {
                from: [-7.0, 3.0],
                angle: Wave {
                    min: 10.0,
                    max: 60.0,
                    period: 3.0,
                },
                speed: 7.0,
                every: 0.5,
                direction: 1.0,
            }],
            places: Vec::new(),
            race: None,
            rules: vec![
                Rule {
                    on: Event::Land {
                        tag: Some("water".into()),
                    },
                    express: Express {
                        name: "sad".into(),
                        looped: false,
                    },
                },
                Rule {
                    on: Event::Land {
                        tag: Some("floor".into()),
                    },
                    express: Express {
                        name: "happy".into(),
                        looped: false,
                    },
                },
            ],
        }
    }

    fn run(zone: &LiveZone, names: &[&str], seconds: f64) -> ZoneRun {
        let mut run = ZoneRun::default();
        for name in names {
            run.arrive(zone, Player::named(name));
        }
        while run.clock < seconds {
            run.step(zone, &[]);
        }
        run
    }

    #[test]
    fn launched_characters_land_and_react_to_where() {
        let zone = zipline();
        let run = run(&zone, &["Ana", "Beto", "Caro", "Dani", "Eli", "Fede"], 6.0);
        assert_eq!(run.len(), 6);
        assert!(
            run.actors
                .iter()
                .all(|actor| actor.motion == Motion::Landed)
        );
        for actor in &run.actors {
            let expected = if actor.at.x < 0.0 { "happy" } else { "sad" };
            assert_eq!(
                actor.expression.as_ref().unwrap().name,
                expected,
                "{actor:?}"
            );
        }
        // Different launch angles send them to different places.
        let mut xs: Vec<f64> = run.actors.iter().map(|actor| actor.at.x).collect();
        xs.sort_by(f64::total_cmp);
        xs.dedup_by(|a, b| (*a - *b).abs() < 0.79);
        assert_eq!(xs.len(), 6, "characters overlap: {xs:?}");
    }

    #[test]
    fn a_player_arrives_once_and_launches_in_turn() {
        let zone = zipline();
        let mut run = ZoneRun::default();
        run.arrive(&zone, Player::named("Ana"));
        run.arrive(&zone, Player::named("Ana"));
        run.arrive(&zone, Player::named("Beto"));
        run.step(&zone, &[]);
        assert_eq!(run.len(), 1);
        while run.clock < 0.6 {
            run.step(&zone, &[]);
        }
        assert_eq!(run.len(), 2);
    }

    #[test]
    fn places_follow_the_standings_and_rank_changes_react() {
        let mut zone = zipline();
        zone.launchers.clear();
        zone.places = vec![
            Place {
                rank: 0,
                at: [0.0, 1.0],
                express: Some(Express {
                    name: "winner".into(),
                    looped: true,
                }),
            },
            Place {
                rank: 1,
                at: [-3.0, 0.0],
                express: None,
            },
        ];
        zone.rules = vec![Rule {
            on: Event::RankDown,
            express: Express {
                name: "sad".into(),
                looped: false,
            },
        }];
        let (ana, beto) = (Player::named("Ana"), Player::named("Beto"));
        let mut run = ZoneRun::default();
        for _ in 0..240 {
            run.step(&zone, &[(ana.clone(), 900), (beto.clone(), 500)]);
        }
        let find = |run: &ZoneRun, name: &str| {
            run.actors
                .iter()
                .find(|actor| &*actor.name == name)
                .unwrap()
                .clone()
        };
        let leader = find(&run, "Ana");
        assert!((leader.at - Point::new(0.0, 1.0)).hypot() < 0.05);
        assert_eq!(leader.expression.unwrap().name, "winner");
        // Beto overtakes: Ana goes down and is sad, Beto wins.
        run.step(&zone, &[(beto.clone(), 1200), (ana.clone(), 900)]);
        assert_eq!(find(&run, "Ana").expression.unwrap().name, "sad");
        assert_eq!(find(&run, "Beto").expression.unwrap().name, "winner");
    }

    #[test]
    fn drawing_places_each_character_on_its_feet() {
        let zone = zipline();
        let run = run(&zone, &["Ana"], 4.0);
        let paths = run.draw(&zone);
        assert!(!paths.is_empty());
        let bottom = paths
            .iter()
            .map(|(path, _)| gaanim_core::kurbo::Shape::bounding_box(path).y0)
            .fold(f64::INFINITY, f64::min);
        let feet = run.actors[0].at.y;
        assert!(
            (bottom - feet).abs() < zone.size * 0.1,
            "{bottom} vs {feet}"
        );
    }
}
