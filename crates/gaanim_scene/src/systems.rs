use crate::components::{
    FillBrush, GlobalOpacity, GroupMarker, LocalBounds, Opacity, StrokeBrush, WorldBounds,
};
use bevy::prelude::{Added, Changed, ChildOf, Entity, Local, Or, ParamSet, Query, With, Without};
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
            (
                Without<ChildOf>,
                Or<(Changed<SpatialTransform>, Added<SpatialTransform>)>,
            ),
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
pub fn has_opacity_changes(query: Query<&Opacity, Or<(Changed<Opacity>, Added<Opacity>)>>) -> bool {
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

/// Run condition: skip bounds propagation when no relevant inputs changed.
pub fn has_bounds_changes(
    q_local: Query<Entity, Or<(Changed<LocalBounds>, Added<LocalBounds>)>>,
    q_transform: Query<
        Entity,
        Or<(
            Changed<GlobalSpatialTransform>,
            Added<GlobalSpatialTransform>,
        )>,
    >,
) -> bool {
    !q_local.is_empty() || !q_transform.is_empty()
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
    mut query: Query<
        (&GlobalSpatialTransform, &mut WorldBounds),
        (Without<LocalBounds>, Without<GroupMarker>),
    >,
) {
    for (global, mut world) in &mut query {
        // Approximate a 1x1 unit box centered at the transform's translation.
        let pos = global.affine_2d * gaanim_core::kurbo::Point::new(0.0, 0.0);
        world.0 = gaanim_math::Bounds3D::new_2d(pos.x - 0.5, pos.y - 0.5, pos.x + 0.5, pos.y + 0.5);
    }
}

/// System: Propagate WorldBounds bottom-up for nested group hierarchies.
pub fn hierarchical_bounds_system(
    root_query: Query<Entity, (With<bevy::prelude::Children>, Without<ChildOf>)>,
    empty_root_group_query: Query<
        Entity,
        (With<GroupMarker>, Without<bevy::prelude::Children>, Without<ChildOf>),
    >,
    children_query: Query<&bevy::prelude::Children>,
    mut bounds_query: Query<&mut WorldBounds>,
    is_group_query: Query<(), With<GroupMarker>>,
) {
    for root in &root_query {
        compute_bounds_recursive(root, &children_query, &mut bounds_query, &is_group_query);
    }

    // Reset bounds of empty root groups (GroupMarker without children).
    // These are not reached by the recursive traversal, which only descends
    // through Children. Resetting here mirrors the old cleanup pass and
    // prevents stale bounds from persisting after all children are removed.
    for entity in &empty_root_group_query {
        if let Ok(mut b) = bounds_query.get_mut(entity) {
            b.0 = gaanim_math::Bounds3D::default();
        }
    }
}

fn compute_bounds_recursive(
    entity: Entity,
    children_query: &Query<&bevy::prelude::Children>,
    bounds_query: &mut Query<&mut WorldBounds>,
    is_group_query: &Query<(), With<GroupMarker>>,
) -> gaanim_math::Bounds3D {
    let mut union_bounds = gaanim_math::Bounds3D::new(
        gaanim_core::glam::DVec3::splat(f64::INFINITY),
        gaanim_core::glam::DVec3::splat(f64::NEG_INFINITY),
    );

    if let Ok(children) = children_query.get(entity) {
        for &child in children.iter() {
            let child_bounds =
                compute_bounds_recursive(child, children_query, bounds_query, is_group_query);
            if child_bounds.min.x != f64::INFINITY {
                union_bounds = union_bounds.union(&child_bounds);
            }
        }
    }

    if is_group_query.contains(entity) {
        if let Ok(mut b) = bounds_query.get_mut(entity) {
            b.0 = if union_bounds.min.x != f64::INFINITY {
                union_bounds
            } else {
                gaanim_math::Bounds3D::default()
            };
            return b.0;
        }
    } else if let Ok(b) = bounds_query.get(entity) {
        return b.0;
    }

    union_bounds
}

/// System: Propagate styling changes (FillBrush/StrokeBrush) from groups to their children.
pub fn style_propagation_system(
    mut param_set: ParamSet<(
        Query<
            (Entity, Option<&FillBrush>, Option<&StrokeBrush>),
            (
                With<GroupMarker>,
                Or<(Changed<FillBrush>, Changed<StrokeBrush>)>,
            ),
        >,
        Query<(&mut FillBrush, &mut StrokeBrush)>,
    )>,
    children_query: Query<&bevy::prelude::Children>,
    // Reuse the update buffer across frames to avoid per-call allocation.
    mut updates: Local<Vec<(Entity, Option<FillBrush>, Option<StrokeBrush>)>>,
) {
    updates.clear();
    for (group_entity, fill_opt, stroke_opt) in param_set.p0().iter() {
        updates.push((group_entity, fill_opt.cloned(), stroke_opt.cloned()));
    }

    let mut style_query = param_set.p1();
    for (group_entity, fill_opt, stroke_opt) in updates.drain(..) {
        propagate_style_recursive(
            group_entity,
            fill_opt.as_ref(),
            stroke_opt.as_ref(),
            &children_query,
            &mut style_query,
        );
    }
}

/// Recursively propagate fill/stroke from a parent group down through all descendants.
fn propagate_style_recursive(
    parent: Entity,
    fill_val: Option<&FillBrush>,
    stroke_val: Option<&StrokeBrush>,
    children_query: &Query<&bevy::prelude::Children>,
    style_query: &mut Query<(&mut FillBrush, &mut StrokeBrush)>,
) {
    let Ok(children) = children_query.get(parent) else {
        return;
    };
    for &child in children.iter() {
        if let Ok((mut child_fill, mut child_stroke)) = style_query.get_mut(child) {
            if let Some(f) = fill_val {
                child_fill.0 = f.0.clone();
            }
            if let Some(s) = stroke_val {
                *child_stroke = s.clone();
            }
        }
        // Recurse into children of children
        propagate_style_recursive(child, fill_val, stroke_val, children_query, style_query);
    }
}
