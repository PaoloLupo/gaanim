//! Echo copies: render-only entities that show a drawable as it was a
//! moment earlier.

use bevy::prelude::{Component, Entity};
use gaanim_core::ObjectId;

/// A render-only copy of the Mobject `source`, drawn as it was `lag` seconds
/// earlier.
///
/// The timeline re-evaluates the copy after every seek from `source`'s
/// keyframe and clips, so it is exact at any time and in every host. It
/// is hidden before the scene starts and across segment cuts.
#[derive(Component, Debug, Clone, PartialEq)]
pub struct EchoGhost {
    /// Runtime identity of the copied Mobject.
    pub source: ObjectId,
    /// How far back in timeline seconds the copy shows `source`.
    pub lag: f64,
    /// Opacity multiplier of the copy.
    pub opacity: f32,
    /// Echo copy of `source`'s parent, when the whole subtree is echoed;
    /// otherwise the copy shares `source`'s parent.
    pub parent: Option<Entity>,
    /// Draw order among copies of one source: a higher rank draws first,
    /// and the source itself (rank 0) above all of them.
    pub rank: u32,
}
