//! Write / Create path-progression components and seed system.
//!
//! The Write animation has two phases:
//! 1. The path is progressively revealed by trimming the visible `Path2D`
//!    against the cached, un-trimmed `PathSource`.
//! 2. Once the path is fully drawn, the fill (if any) cross-fades in from
//!    invisible to fully visible, controlled by the `FillDrawProgress`
//!    component.
//!
//! `PathSource` is seeded once at spawn time by `path_source_seed_added_system`.
//! `FillDrawProgress` is inserted by the Write animation itself (it defaults
//! to fully visible, so entities without an active Write are unaffected).
//!
//! Ported and adapted from `crabanim::engine::animation::path_lens`.

use bevy::prelude::{Added, Commands, Component, Entity, Query, Without};
use gaanim_scene::Path2D;
pub use gaanim_scene::PathSource;

/// Multiplier applied to the fill brush's color alpha by the renderer
/// during a Write animation. `0.0` = fill fully hidden, `1.0` = fill
/// fully visible (default for entities that aren't being Written).
///
/// The Write animation schedules two clips per item:
/// - `PathCompletion 0.0 -> 1.0` over the first ~70% of `item_duration`
/// - `FillDrawProgress 0.0 -> 1.0` over the remaining ~30%
///
/// The renderer reads this component and multiplies the fill brush's
/// color alpha by it. If the component is absent, the fill is rendered
/// at full opacity (the default behavior for non-Writing entities).
#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub struct FillDrawProgress(pub f32);

impl Default for FillDrawProgress {
    fn default() -> Self {
        Self(1.0)
    }
}

/// System: For every newly-spawned entity that has `Path2D` but no
/// `PathSource` yet, seed the `PathSource` from the visible `Path2D`.
///
/// The insert is deferred to the end-of-schedule command flush. In
/// practice this is fine because the seek system runs in
/// `SceneSet::Animation` (after `SceneSet::Input` where this lives) and
/// the `Update` schedule's automatic `apply_deferred` step processes
/// the queued inserts before any later set runs.
///
/// The Write animation also directly inserts `FillDrawProgress(0.0)`
/// on its target entities at scheduling time, so the renderer is
/// guaranteed to hide the fill on the very first frame even if the
/// path-trim lens hasn't yet been applied to a freshly-spawned entity.
pub fn path_source_seed_added_system(
    mut commands: Commands,
    q: Query<(Entity, &Path2D), (Added<Path2D>, Without<PathSource>)>,
) {
    for (entity, path) in &q {
        commands.entity(entity).insert(PathSource(path.0.clone()));
    }
}
