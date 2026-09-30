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
}

fn checked_program<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Program, D::Error> {
    Program::deserialize(deserializer)?
        .checked()
        .map_err(serde::de::Error::custom)
}
