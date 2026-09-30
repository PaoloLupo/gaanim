//! What a live zone is: the data a scene authors and a bundle stores. Its
//! behavior is a compiled [`Program`], so a presented bundle runs it without
//! Python.

use serde::{Deserialize, Deserializer, Serialize};

use super::program::Program;

/// A region of the scene where the audience's characters play while a
/// presentation takes votes: from `open` to `close` on the timeline, one
/// character per player, posed every frame by the zone's behavior.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LiveZone {
    /// Stable within a scene, for keeping a zone's run across reloads.
    pub id: String,
    pub open: f64,
    pub close: f64,
    /// Whether a stop rests exactly at `open` or `close`. A stop holds the
    /// outgoing moment, as segments do at a shared boundary: resting at
    /// `open` shows what came before, resting at `close` shows the zone.
    #[serde(default)]
    pub stop_at_open: bool,
    #[serde(default)]
    pub stop_at_close: bool,
    /// [x0, y0, x1, y1] in scene units: characters are drawn clipped to it.
    pub bounds: [f64; 4],
    /// Height of a character at scale 1, in scene units.
    pub size: f64,
    /// Nicknames shown outside a live presentation, arriving one every
    /// `preview_every` seconds from `open`.
    pub preview: Vec<String>,
    pub preview_every: f64,
    /// Poses each player's character from what the engine tells it.
    #[serde(deserialize_with = "checked_program")]
    pub behavior: Program,
    /// How the engine deforms characters from how their poses move.
    #[serde(default)]
    pub motion: Motion,
    /// Nicknames drawn under the characters' feet, or none.
    #[serde(default)]
    pub names: Option<ZoneNames>,
}

/// How a zone draws each player's nickname under their character, when the
/// pose shows it. The glyphs are shaped when the scene compiles and travel
/// with the zone, so a bundle draws names without fonts.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ZoneNames {
    /// Font size, in scene units.
    pub size: f64,
    /// `#rrggbb` or `#rrggbbaa`.
    pub color: String,
    /// Gap between the feet and the top of the name.
    pub gap: f64,
    #[serde(default)]
    pub font: Option<String>,
    #[serde(default)]
    pub weight: Option<u16>,
    /// Printable ASCII and Latin-1 glyphs, filled when the scene compiles.
    #[serde(default)]
    pub glyphs: Vec<NameGlyph>,
}

/// One character's outline (SVG path data, baseline at y = 0, y up) and
/// advance, in scene units.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NameGlyph {
    pub ch: char,
    pub advance: f64,
    pub path: String,
}

/// Deformations the engine adds from how a pose moves, measured by
/// evaluating the behavior just before and after the frame, so they stay
/// a pure function of time. Zero turns one off.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Motion {
    /// Stretch along the velocity and squash across, keeping the area:
    /// `1 + squash * speed` (scene units per second), up to `max_stretch`.
    pub squash: f64,
    pub max_stretch: f64,
    /// Lean into horizontal motion: `lean * horizontal speed` radians, up
    /// to `max_lean`.
    pub lean: f64,
    pub max_lean: f64,
    /// How much hanging extras (ears, hats, sprouts) lag behind motion
    /// across the scene, as a spring; 1 as the catalog tunes it.
    #[serde(default = "one")]
    pub follow: f64,
    /// How far the eyes look toward the motion per unit of speed.
    #[serde(default = "look")]
    pub look: f64,
}

fn one() -> f64 {
    1.0
}

fn look() -> f64 {
    0.4
}

impl Default for Motion {
    fn default() -> Self {
        Self {
            squash: 0.0,
            max_stretch: 1.4,
            lean: 0.0,
            max_lean: 0.35,
            follow: 1.0,
            look: 0.4,
        }
    }
}

fn checked_program<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Program, D::Error> {
    Program::deserialize(deserializer)?
        .checked()
        .map_err(serde::de::Error::custom)
}
