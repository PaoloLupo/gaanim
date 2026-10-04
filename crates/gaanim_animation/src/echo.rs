//! Echo copies: render-only entities that show a drawable as it was a
//! moment earlier.

use bevy::prelude::{Component, Entity};
use gaanim_core::ObjectId;
use std::sync::Arc;

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
    /// Delay along the motion of `motion_sources` instead of the timeline:
    /// while none of them is animated the copy freezes where it was, like
    /// an onion skin, and it moves on when they move again.
    pub hold: bool,
    /// The echoed subtree, whose animation clips make up the motion a held
    /// copy follows. Every copy of one echo shares it.
    pub motion_sources: Arc<[ObjectId]>,
    /// Scene seconds the copy records, when limited: it shows the source
    /// only as it was inside them, and is hidden otherwise.
    pub window: Option<(f64, f64)>,
}
