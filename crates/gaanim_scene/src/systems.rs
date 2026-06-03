use crate::components::{FillBrush, GlobalOpacity, GroupMarker, LocalBounds, Opacity, StrokeBrush, WorldBounds};
use bevy::prelude::{
    Added, Changed, ChildOf, Entity, Local, Or, ParamSet, Query, With, Without,
};
use gaanim_math::{GlobalSpatialTransform, SpatialTransform};

/// Run condition: skip transform propagation when no local transform has changed.
///
/// Since all clip animations explicitly store `from`/`to` values and write to
/// `SpatialTransform` (which updates Bevy's change tick), and the only other
/// mutation path is `seek()` snapshot restore (which also updates ticks), this
/// condition correctly detects every scenario where propagation is needed.
pub fn has_transform_changes(
    query: Query<&SpatialTransform, Or<(Changed<SpatialTransform>, Added<SpatialTransform>)>>,
) -> bool {
    !query.is_empty()
}

/// System: Propagate spatial transforms hierarchically using Bevy 0.18's `ChildOf` relation.
///
/// This system computes the `GlobalSpatialTransform` for all entities:
/// - Root Mobjects (Without<ChildOf>): Global = Local
/// - Child Mobjects (With<ChildOf>): Global = ParentGlobal * Local
///
/// Under Bevy 0.18, standard `Parent`/`Children` components are replaced with the highly
/// efficient relationship-based `ChildOf` system, which we target here directly.
///
/// # Performance
/// The root pass only processes entities whose `SpatialTransform` actually changed
/// or were newly added. The child pass still iterates all children to handle parent
/// chain changes, but the entire system is skipped via `run_if(has_transform_changes)`
/// when no entity's local transform was modified.
#[allow(clippy::type_complexity)]
pub fn transform_propagation_system(
    mut param_set: ParamSet<(
        // P0: Root entities whose local transform changed
        Query<
            (&SpatialTransform, &mut GlobalSpatialTransform),
            (Without<ChildOf>, Or<(Changed<SpatialTransform>, Added<SpatialTransform>)>),
        >,
        // P1: Child entities parent lookup query
        Query<(Entity, &ChildOf), With<SpatialTransform>>,
        // P2: General transform query for hierarchy resolution
        Query<(&SpatialTransform, &mut GlobalSpatialTransform)>,
    )>,
    mut children_to_update: Local<Vec<(Entity, Entity)>>,
) {
    // 1. Root pass: Only process roots whose local transform changed or were added
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

/// Run condition: skip opacity propagation when no local opacity has changed.
pub fn has_opacity_changes(
    query: Query<&Opacity, Or<(Changed<Opacity>, Added<Opacity>)>>,
) -> bool {
    !query.is_empty()
}

/// System: Propagate opacity cascade down the hierarchy using Bevy 0.18's `ChildOf` relation.
#[allow(clippy::type_complexity)]
pub fn opacity_propagation_system(
    mut param_set: ParamSet<(
        // P0: Root entities whose local opacity changed
        Query<
            (&Opacity, &mut GlobalOpacity),
            (Without<ChildOf>, Or<(Changed<Opacity>, Added<Opacity>)>),
        >,
        // P1: Child entities parent lookup query
        Query<(Entity, &ChildOf), With<Opacity>>,
        // P2: General opacity query for hierarchy resolution
        Query<(&Opacity, &mut GlobalOpacity)>,
    )>,
    mut children_to_update: Local<Vec<(Entity, Entity)>>,
) {
    // 1. Root pass: Only process roots whose opacity changed or were added
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
    mut query: Query<(&GlobalSpatialTransform, &mut WorldBounds), (Without<LocalBounds>, Without<GroupMarker>)>,
) {
    for (global, mut world) in &mut query {
        // Approximate a 1x1 unit box centered at the transform's translation.
        let pos = global.affine_2d * gaanim_core::kurbo::Point::new(0.0, 0.0);
        world.0 = gaanim_math::Bounds3D::new_2d(pos.x - 0.5, pos.y - 0.5, pos.x + 0.5, pos.y + 0.5);
    }
}

/// Helper function to compute the depth of an entity in the hierarchy.
fn get_entity_depth(entity: Entity, child_query: &Query<(Entity, &ChildOf)>) -> usize {
    let mut depth = 0;
    let mut current = entity;
    
    // Simple traversal up the parent chain
    while let Ok((_, child_of)) = child_query.get(current) {
        current = child_of.parent();
        depth += 1;
    }
    
    depth
}

/// System: Propagate WorldBounds bottom-up for nested group hierarchies.
pub fn hierarchical_bounds_system(
    group_query: Query<Entity, With<GroupMarker>>,
    child_query: Query<(Entity, &ChildOf)>,
    mut bounds_query: Query<&mut WorldBounds>,
) {
    // 1. Initialize all group bounds to an empty null bounds
    let null_bounds = gaanim_math::Bounds3D::new(
        gaanim_core::glam::DVec3::splat(f64::INFINITY),
        gaanim_core::glam::DVec3::splat(f64::NEG_INFINITY),
    );
    
    for group_entity in &group_query {
        if let Ok(mut world_bounds) = bounds_query.get_mut(group_entity) {
            world_bounds.0 = null_bounds;
        }
    }
    
    // 2. Collect child entities with their hierarchical depth
    let mut children_with_depth: Vec<(Entity, Entity, usize)> = Vec::new();
    for (child_entity, child_of) in &child_query {
        let parent_entity = child_of.parent();
        let depth = get_entity_depth(child_entity, &child_query);
        children_with_depth.push((child_entity, parent_entity, depth));
    }
    
    // 3. Sort by depth in descending order (deepest child nodes first)
    children_with_depth.sort_by_key(|&(_, _, depth)| std::cmp::Reverse(depth));
    
    // 4. Propagate child bounds up to their parent's WorldBounds
    for (child_entity, parent_entity, _) in children_with_depth {
        if let Ok(child_bounds) = bounds_query.get(child_entity) {
            let cb = child_bounds.0;
            // Only propagate if the child has valid bounds (not null/infinite)
            if cb.min.x != f64::INFINITY
                && let Ok(mut parent_bounds) = bounds_query.get_mut(parent_entity) {
                    parent_bounds.0 = parent_bounds.0.union(&cb);
                }
        }
    }
    
    // 5. Clean up any empty groups (that ended up with no children or infinite bounds)
    for group_entity in &group_query {
        if let Ok(mut world_bounds) = bounds_query.get_mut(group_entity)
            && world_bounds.0.min.x == f64::INFINITY {
                world_bounds.0 = gaanim_math::Bounds3D::default();
            }
    }
}

/// System: Propagate styling changes (FillBrush/StrokeBrush) from groups to their children.
pub fn style_propagation_system(
    mut param_set: ParamSet<(
        Query<
            (Entity, Option<&FillBrush>, Option<&StrokeBrush>),
            (With<GroupMarker>, Or<(Changed<FillBrush>, Changed<StrokeBrush>)>),
        >,
        Query<(&mut FillBrush, &mut StrokeBrush)>,
    )>,
    child_query: Query<(Entity, &ChildOf)>,
) {
    let mut queue = Vec::new();

    for (parent_entity, fill_opt, stroke_opt) in &param_set.p0() {
        queue.push((
            parent_entity,
            fill_opt.cloned(),
            stroke_opt.cloned(),
        ));
    }

    let mut style_query = param_set.p1();
    let mut visited = std::collections::HashSet::new();
    while let Some((current_parent, fill_val, stroke_val)) = queue.pop() {
        if !visited.insert(current_parent) {
            continue;
        }

        for (child_entity, child_of) in &child_query {
            if child_of.parent() == current_parent {
                if let Ok((mut child_fill, mut child_stroke)) = style_query.get_mut(child_entity) {
                    if let Some(ref f) = fill_val {
                        child_fill.0 = f.0.clone();
                    }
                    if let Some(ref s) = stroke_val {
                        *child_stroke = s.clone();
                    }
                }

                queue.push((
                    child_entity,
                    fill_val.clone(),
                    stroke_val.clone(),
                ));
            }
        }
    }
}
