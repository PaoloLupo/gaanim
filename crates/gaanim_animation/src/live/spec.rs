//! What a live zone is: the data a scene authors and a bundle stores. It
//! holds no code, so a presented bundle runs it without Python.

use serde::{Deserialize, Serialize};

/// A region of the scene where the audience's characters play while a
/// presentation takes votes: from `open` to `close` on the timeline, one
/// character per player, placed by the zone's launchers, surfaces, podium
/// places or race, and reacting by its rules.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LiveZone {
    /// Stable within a scene, for keeping a zone's run across reloads.
    pub id: String,
    pub open: f64,
    pub close: f64,
    /// [x0, y0, x1, y1] in scene units: characters are drawn clipped to it
    /// and fall no lower than its bottom.
    pub bounds: [f64; 4],
    /// Height of a character in scene units.
    pub size: f64,
    /// Downward acceleration of flying characters, scene units per s².
    pub gravity: f64,
    /// Nicknames shown outside a live presentation, arriving one every
    /// `preview_every` seconds from `open`.
    pub preview: Vec<String>,
    pub preview_every: f64,
    #[serde(default)]
    pub surfaces: Vec<Surface>,
    #[serde(default)]
    pub launchers: Vec<Launcher>,
    /// Podium places: the player at a rank stands at a point.
    #[serde(default)]
    pub places: Vec<Place>,
    #[serde(default)]
    pub race: Option<Race>,
    #[serde(default)]
    pub rules: Vec<Rule>,
}

/// A segment characters land on, tagged for rules ("water", "floor").
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Surface {
    pub tag: String,
    pub from: [f64; 2],
    pub to: [f64; 2],
    /// How deep a character sinks into it, in character heights.
    #[serde(default)]
    pub sink: f64,
}

/// Where arriving characters are launched from, one every `every` seconds,
/// at an angle that sweeps between `angle.min` and `angle.max` degrees
/// above the horizontal, toward `direction` (1 right, -1 left).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Launcher {
    pub from: [f64; 2],
    pub angle: Wave,
    pub speed: f64,
    pub every: f64,
    #[serde(default = "rightward")]
    pub direction: f64,
}

fn rightward() -> f64 {
    1.0
}

/// A value sweeping from `min` to `max` and back every `period` seconds of
/// the zone's clock: `min + (max - min) * (1 - cos(2 pi t / period)) / 2`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Wave {
    pub min: f64,
    pub max: f64,
    pub period: f64,
}

impl Wave {
    pub fn at(&self, t: f64) -> f64 {
        if self.period <= 0.0 {
            return self.min;
        }
        let phase = (1.0 - (std::f64::consts::TAU * t / self.period).cos()) / 2.0;
        self.min + (self.max - self.min) * phase
    }
}

/// The player at `rank` (0 for the leader) stands at `at`, playing
/// `express` while there.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Place {
    pub rank: usize,
    pub at: [f64; 2],
    #[serde(default)]
    pub express: Option<Express>,
}

/// The first `count` players run along bars: rank `r` stands at
/// `origin + step * r + direction * length * score / leader's score`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Race {
    pub origin: [f64; 2],
    pub step: [f64; 2],
    pub direction: [f64; 2],
    pub length: f64,
    pub count: usize,
}

/// An expression to play, once or looped until the next one.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Express {
    pub name: String,
    #[serde(default)]
    pub looped: bool,
}

/// When `on` happens to a character, it plays `express`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Rule {
    pub on: Event,
    pub express: Express,
}

/// Something that happens to a character.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Event {
    /// It arrived in the zone.
    Join,
    /// It landed: on a surface with `tag`, on any surface when `None`.
    /// Falling to the zone's bottom lands on the tag "bottom".
    Land {
        #[serde(default)]
        tag: Option<String>,
    },
    /// It went up the leaderboard.
    RankUp,
    /// It went down the leaderboard.
    RankDown,
    /// It became the leader.
    Leader,
}
