use crate::components::{GlobalOpacity, LocalBounds, Opacity, WorldBounds};
use bevy::prelude::{Added, ChildOf, Entity, Local, ParamSet, Query, With, Without};
use gaanim_math::{GlobalSpatialTransform, SpatialTransform};

/// System: Propagate spatial transforms hierarchically using Bevy 0.18's `ChildOf` relation.
///
/// This system computes the `GlobalSpatialTransform` for all entities:
/// - Root Mobjects (Without<ChildOf>): Global = Local
/// - Child Mobjects (With<ChildOf>): Global = ParentGlobal * Local
///
/// Under Bevy 0.18, standard `Parent`/`Children` components are replaced with the highly
/// efficient relationship-based `ChildOf` system, which we target here directly.
#[allow(clippy::type_complexity)]
pub fn transform_propagation_system(
    mut param_set: ParamSet<(
        // P0: Root entities transform query
        Query<(&SpatialTransform, &mut GlobalSpatialTransform), Without<ChildOf>>,
        // P1: Child entities parent lookup query
        Query<(Entity, &ChildOf), With<SpatialTransform>>,
        // P2: General transform query for hierarchy resolution
        Query<(&SpatialTransform, &mut GlobalSpatialTransform)>,
    )>,
    mut children_to_update: Local<Vec<(Entity, Entity)>>,
) {
    // 1. Root pass: Initialize global transforms from local transforms for all roots
    for (local, mut global) in param_set.p0().iter_mut() {
        *global = GlobalSpatialTransform::from_local(local);
    }

    // 2. Child collection: Fetch child-parent entity pairs
    children_to_update.clear();
    children_to_update.extend(param_set.p1().iter().map(|(e, c)| (e, c.parent())));

    // 3. Child propagation pass: Multiply parent global transforms down the tree
    let mut transforms = param_set.p2();
    for &(child_entity, parent_entity) in children_to_update.iter() {
        if let Ok([child_data, parent_data]) =
            transforms.get_many_mut([child_entity, parent_entity])
        {
            let (child_local, mut child_global) = child_data;
            let (_, parent_global) = parent_data;

            *child_global =
                GlobalSpatialTransform::from_parent_and_local(&parent_global, child_local);
        } else {
            // Fallback: If parent's global transform cannot be read, treat child as root
            if let Ok((local, mut global)) = transforms.get_mut(child_entity) {
                *global = GlobalSpatialTransform::from_local(local);
            }
        }
    }
}

/// System: Propagate opacity cascade down the hierarchy using Bevy 0.18's `ChildOf` relation.
#[allow(clippy::type_complexity)]
pub fn opacity_propagation_system(
    mut param_set: ParamSet<(
        // P0: Root entities opacity query
        Query<(&Opacity, &mut GlobalOpacity), Without<ChildOf>>,
        // P1: Child entities parent lookup query
        Query<(Entity, &ChildOf), With<Opacity>>,
        // P2: General opacity query for hierarchy resolution
        Query<(&Opacity, &mut GlobalOpacity)>,
    )>,
    mut children_to_update: Local<Vec<(Entity, Entity)>>,
) {
    // 1. Root pass: Global opacity equals local opacity
    for (local, mut global) in param_set.p0().iter_mut() {
        global.0 = local.0;
    }

    // 2. Child collection: Fetch child-parent entity pairs
    children_to_update.clear();
    children_to_update.extend(param_set.p1().iter().map(|(e, c)| (e, c.parent())));

    // 3. Child propagation pass: Multiply parent opacity down the tree
    let mut opacities = param_set.p2();
    for &(child_entity, parent_entity) in children_to_update.iter() {
        if let Ok([child_data, parent_data]) = opacities.get_many_mut([child_entity, parent_entity])
        {
            let (child_local, mut child_global) = child_data;
            let (_, parent_global) = parent_data;

            child_global.0 = child_local.0 * parent_global.0;
        } else {
            // Fallback: If parent is un-configured or missing, treat child as root
            if let Ok((local, mut global)) = opacities.get_mut(child_entity) {
                global.0 = local.0;
            }
        }
    }
}

/// System: Sync GlobalOpacity for newly added entities before hierarchy runs.
pub fn sync_new_opacities(mut query: Query<(&Opacity, &mut GlobalOpacity), Added<Opacity>>) {
    for (local, mut global) in &mut query {
        global.0 = local.0;
    }
}

/// System: Compute world-space bounding boxes from local bounds and propagated transforms.
///
/// Runs in the `Bounds` phase after transform propagation so that `GlobalSpatialTransform`
/// already contains the full hierarchy matrix for each entity.
pub fn world_bounds_propagation_system(
    mut query: Query<(&LocalBounds, &GlobalSpatialTransform, &mut WorldBounds)>,
) {
    for (local, global, mut world) in &mut query {
        world.0 = local.0.transform_2d(&global.affine_2d);
    }
}

/// System: Approximate WorldBounds for entities without LocalBounds using transform position.
pub fn world_bounds_fallback_system(
    mut query: Query<(&GlobalSpatialTransform, &mut WorldBounds), Without<LocalBounds>>,
) {
    for (global, mut world) in &mut query {
        // Approximate a 1x1 unit box centered at the transform's translation.
        let pos = global.affine_2d * gaanim_core::kurbo::Point::new(0.0, 0.0);
        world.0 = gaanim_math::Bounds3D::new_2d(pos.x - 0.5, pos.y - 0.5, pos.x + 0.5, pos.y + 0.5);
    }
}
