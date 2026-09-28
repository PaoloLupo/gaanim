use crate::components::{
    CoordinateLabelOffset, CoordinateTickLevel, CoordinateViewRole, FillBrush, GlobalOpacity,
    GroupMarker, LineListData, LocalBounds, Opacity, StrokeBrush, TriangleMeshData, WorldBounds,
};
use bevy::ecs::entity::EntityHashSet;
use bevy::prelude::{
    Added, Changed, ChildOf, Children, Entity, Local, Or, ParamSet, Query, RemovedComponents, Res,
    ResMut, With, Without,
};
use gaanim_math::{GlobalSpatialTransform, SpatialTransform};

/// Resolve the timeline-authored camera and an optional presentation override.
pub fn resolve_camera_system(
    authored: Option<Res<gaanim_math::Camera>>,
    rig: Option<Res<gaanim_math::CameraRigCamera>>,
    view_override: Res<gaanim_math::CameraViewOverride>,
    viewport: Res<gaanim_math::CameraViewport>,
    mut resolved: ResMut<gaanim_math::ResolvedCamera>,
) {
    if let Some(camera) = view_override
        .0
        .or_else(|| rig.as_deref().map(|rig| rig.0))
        .or_else(|| authored.as_deref().copied())
    {
        resolved.camera = camera;
        resolved.viewport = *viewport;
    }
}

/// Run condition: skip transform propagation when no local transform has changed.
///
/// Since all clip animations explicitly store `from`/`to` values and write to
/// `SpatialTransform` (which updates Bevy's change tick), and the only other
/// mutation path is `seek()` snapshot restore (which also updates ticks), this
/// condition correctly detects every scenario where propagation is needed.
pub fn has_transform_changes(
    query: Query<&SpatialTransform, Or<(Changed<SpatialTransform>, Added<SpatialTransform>)>>,
    deforms: Query<(), Changed<crate::ShapeDeform>>,
) -> bool {
    !query.is_empty() || !deforms.is_empty()
}

/// System: Propagate spatial transforms hierarchically using Bevy 0.19's `ChildOf` relation.
///
/// This system computes the `GlobalSpatialTransform` for all entities:
/// - Root Mobjects (Without<ChildOf>): Global = Local
/// - Child Mobjects (With<ChildOf>): Global = ParentGlobal * Local
///
/// Under Bevy 0.19, standard `Parent`/`Children` components are replaced with the highly
/// efficient relationship-based `ChildOf` system, which we target here directly.
///
/// Descendants are updated recursively in parent-before-child order. This prevents
/// grandchildren (for example text glyphs inside a grouped text object) from using
/// their parent's transform from the previous frame. Only the subtrees below
/// entities whose inputs changed since the last run are visited.
pub fn transform_propagation_system(
    mut queries: ParamSet<(Query<Entity, StaleTransformFilter>, TransformTargets)>,
    mut removed: (
        RemovedComponents<ChildOf>,
        RemovedComponents<crate::ShapeDeform>,
        RemovedComponents<CoordinateViewRole>,
        RemovedComponents<CoordinateLabelOffset>,
    ),
    children_query: Query<&'static Children>,
    view_roles: Query<&'static CoordinateViewRole>,
    label_offsets: Query<&'static CoordinateLabelOffset>,
    parents: Query<&'static ChildOf>,
) {
    // A presentation keeps every object of every segment alive, so recomputing
    // the whole hierarchy would cost the full scene on every animated frame.
    let mut stale_set: EntityHashSet = queries.p0().iter().collect();
    stale_set.extend(removed.0.read());
    stale_set.extend(removed.1.read());
    stale_set.extend(removed.2.read());
    stale_set.extend(removed.3.read());
    let propagation = Propagation {
        children_query: &children_query,
        view_roles: &view_roles,
        label_offsets: &label_offsets,
        parents: &parents,
        stale: Some(&stale_set),
    };
    let mut transforms = queries.p1();
    for &entity in &stale_set {
        // A stale ancestor's subtree already covers this entity.
        let mut ancestor = parents.get(entity).ok().map(ChildOf::parent);
        let mut covered = false;
        while let Some(current) = ancestor {
            if stale_set.contains(&current) {
                covered = true;
                break;
            }
            ancestor = parents.get(current).ok().map(ChildOf::parent);
        }
        if covered {
            continue;
        }
        let parent_global = match parents.get(entity) {
            // A parent without a transform never reaches its children.
            Ok(parent) => match transforms.get(parent.parent()) {
                Ok((_, global, _)) => Some(*global),
                Err(_) => continue,
            },
            Err(_) => None,
        };
        propagate_transforms_recursive(entity, parent_global, true, &mut transforms, &propagation);
    }
}

/// Local inputs and world transform of every entity propagation writes.
type TransformTargets<'w, 's> = Query<
    'w,
    's,
    (
        &'static SpatialTransform,
        &'static mut GlobalSpatialTransform,
        Option<&'static crate::ShapeDeform>,
    ),
>;

/// Entities whose world transform may differ from what propagation last
/// wrote: their own inputs changed, or another system wrote their world
/// transform (HUD pinning, billboards).
type StaleTransformFilter = Or<(
    Changed<SpatialTransform>,
    Changed<GlobalSpatialTransform>,
    Changed<ChildOf>,
    Changed<crate::ShapeDeform>,
    Changed<CoordinateViewRole>,
    Changed<CoordinateLabelOffset>,
)>;

/// Read-only inputs of [`propagate_transforms_recursive`].
struct Propagation<'a, 'w1, 's1, 'w2, 's2, 'w3, 's3, 'w4, 's4> {
    children_query: &'a Query<'w1, 's1, &'static Children>,
    view_roles: &'a Query<'w2, 's2, &'static CoordinateViewRole>,
    label_offsets: &'a Query<'w3, 's3, &'static CoordinateLabelOffset>,
    parents: &'a Query<'w4, 's4, &'static ChildOf>,
    /// Entities to recompute even when their parent did not change; `None`
    /// recomputes the whole subtree.
    stale: Option<&'a EntityHashSet>,
}

/// `local` with `deform` applied about its position, in the parent's space.
fn deformed_local(
    local: &SpatialTransform,
    deform: gaanim_core::kurbo::Affine,
) -> GlobalSpatialTransform {
    let position = local.translation;
    let about = gaanim_core::kurbo::Affine::translate((position.x, position.y))
        * deform
        * gaanim_core::kurbo::Affine::translate((-position.x, -position.y));
    let [a, b, c, d, e, f] = about.as_coeffs();
    let about_mat4 = gaanim_core::glam::DMat4::from_cols_array(&[
        a, b, 0.0, 0.0, c, d, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, e, f, 0.0, 1.0,
    ]);
    GlobalSpatialTransform {
        affine_2d: about * local.to_affine_2d(),
        mat4: about_mat4 * local.to_mat4(),
    }
}

fn propagate_transforms_recursive(
    entity: Entity,
    parent_global: Option<GlobalSpatialTransform>,
    parent_changed: bool,
    transforms: &mut Query<(
        &SpatialTransform,
        &mut GlobalSpatialTransform,
        Option<&crate::ShapeDeform>,
    )>,
    propagation: &Propagation,
) {
    let stale = propagation
        .stale
        .is_none_or(|stale| stale.contains(&entity));
    let recompute = parent_changed || stale;
    let (current_global, changed) = if recompute {
        let Some(computed) =
            computed_global(entity, parent_global.as_ref(), transforms, propagation)
        else {
            return;
        };
        let Ok((_, mut global, _)) = transforms.get_mut(entity) else {
            return;
        };
        // Leave an unchanged value alone so change detection stays precise.
        let changed = *global != computed;
        if changed {
            *global = computed;
        }
        (computed, changed)
    } else {
        let Ok((_, global, _)) = transforms.get(entity) else {
            return;
        };
        (*global, false)
    };

    if let Ok(children) = propagation.children_query.get(entity) {
        for child in children.iter() {
            propagate_transforms_recursive(
                *child,
                Some(current_global),
                // Another writer may already have stored this entity's new
                // value, so an unchanged value does not prove its children
                // are current.
                changed || stale,
                transforms,
                propagation,
            );
        }
    }
}

/// World transform of `entity` under `parent_global`.
fn computed_global(
    entity: Entity,
    parent_global: Option<&GlobalSpatialTransform>,
    transforms: &Query<(
        &SpatialTransform,
        &mut GlobalSpatialTransform,
        Option<&crate::ShapeDeform>,
    )>,
    propagation: &Propagation,
) -> Option<GlobalSpatialTransform> {
    let (local, _, deform) = transforms.get(entity).ok()?;
    let deformed = deform
        .filter(|deform| deform.0 != gaanim_core::kurbo::Affine::IDENTITY)
        .map(|deform| deformed_local(local, deform.0));
    let mut current_global = match (parent_global, deformed) {
        (Some(parent), Some(local)) => GlobalSpatialTransform {
            affine_2d: parent.affine_2d * local.affine_2d,
            mat4: parent.mat4 * local.mat4,
        },
        (None, Some(local)) => local,
        (Some(parent), None) => GlobalSpatialTransform::from_parent_and_local(parent, local),
        (None, None) => GlobalSpatialTransform::from_local(local),
    };
    if matches!(
        propagation.view_roles.get(entity),
        Ok(CoordinateViewRole::Label)
    ) {
        // Only text roots need this alternate linear transform. Keep the fully
        // transformed origin, but omit domain-view scale from the glyph basis.
        // Apply before descending so every glyph gets the correction this frame.
        // Recompose from locals rather than inverting a possibly singular scale.
        let mut basis = GlobalSpatialTransform::default();
        // The same product without the label's own local transform.
        let mut parent_basis = gaanim_core::kurbo::Affine::IDENTITY;
        let mut ancestor = entity;
        let mut needs_compensation = false;
        loop {
            let Ok((local, _, _)) = transforms.get(ancestor) else {
                break;
            };
            let mut local = *local;
            if matches!(
                propagation.view_roles.get(ancestor),
                Ok(CoordinateViewRole::View)
            ) {
                needs_compensation |= local.scale != gaanim_core::glam::DVec3::ONE;
                local.scale = gaanim_core::glam::DVec3::ONE;
            }
            basis.affine_2d = local.to_affine_2d() * basis.affine_2d;
            basis.mat4 = local.to_mat4() * basis.mat4;
            if ancestor != entity {
                parent_basis = local.to_affine_2d() * parent_basis;
            }
            let Ok(parent) = propagation.parents.get(ancestor) else {
                break;
            };
            ancestor = parent.parent();
        }
        // Keep unzoomed output bit-for-bit identical to ordinary propagation.
        if needs_compensation {
            let [a, b, c, d, _, _] = basis.affine_2d.as_coeffs();
            let [_, _, _, _, tx, ty] = current_global.affine_2d.as_coeffs();
            // Place the label at its zoomed data point plus an unzoomed offset:
            // swap the parent's zoomed linear map for its unzoomed one on the offset.
            let (mut dx, mut dy) = (0.0, 0.0);
            if let (Ok(offset), Some(parent)) =
                (propagation.label_offsets.get(entity), parent_global)
            {
                let [pa, pb, pc, pd, _, _] = parent.affine_2d.as_coeffs();
                let [ua, ub, uc, ud, _, _] = parent_basis.as_coeffs();
                let (ox, oy) = (offset.0.x, offset.0.y);
                dx = (ua - pa) * ox + (uc - pc) * oy;
                dy = (ub - pb) * ox + (ud - pd) * oy;
            }
            current_global.affine_2d =
                gaanim_core::kurbo::Affine::new([a, b, c, d, tx + dx, ty + dy]);
            basis.mat4.w_axis = current_global.mat4.w_axis;
            basis.mat4.w_axis.x += dx;
            basis.mat4.w_axis.y += dy;
            current_global.mat4 = basis.mat4;
        }
    }

    Some(current_global)
}

/// World transform that keeps HUD overlays fixed on the output frame while an
/// orthographic camera pans, zooms or rotates.
///
/// A point authored at `p` for the unmoved camera must sit at `pin * p` so the
/// moved camera shows it at the same pixel. `None` when no correction applies,
/// as with a perspective camera, whose 2D overlay pass never moves.
pub fn hud_pin(camera: &gaanim_math::Camera) -> Option<gaanim_core::kurbo::Affine> {
    use gaanim_core::kurbo::Affine;
    let gaanim_math::Projection::Orthographic { zoom } = camera.projection else {
        return None;
    };
    if !zoom.is_finite() || zoom <= 0.0 {
        return None;
    }
    let pin = Affine::translate((camera.position.x, camera.position.y))
        * Affine::rotate(camera.z_angle())
        * Affine::scale(zoom.recip());
    (pin != Affine::IDENTITY).then_some(pin)
}

/// [`hud_pin`] of the camera a frame shows, read straight from a world by
/// code that composes world transforms itself before the camera phase.
///
/// The editor's view override wins, as in [`resolve_camera_system`]; otherwise
/// the authored camera, which the timeline has already written this frame.
/// The camera rig is skipped: before the camera phase it still holds the pose
/// of the previous frame, which after a seek can be any earlier time.
pub fn world_hud_pin(world: &bevy::prelude::World) -> Option<gaanim_core::kurbo::Affine> {
    let camera = world
        .get_resource::<gaanim_math::CameraViewOverride>()
        .and_then(|view| view.0)
        .or_else(|| world.get_resource::<gaanim_math::Camera>().copied())?;
    hud_pin(&camera)
}

/// System: pin HUD overlays to the output frame.
///
/// Runs after [`transform_propagation_system`] and recomposes every HUD
/// subtree from its local transforms under the camera's [`hud_pin`], so the
/// result is the same whether or not propagation ran this frame. Bounds,
/// anchors, picking and rendering all read the pinned world transforms.
#[allow(clippy::too_many_arguments)]
pub fn pin_hud_overlays_system(
    camera: Option<Res<gaanim_math::ResolvedCamera>>,
    hud: Query<Entity, With<crate::components::HudOverlay>>,
    children_query: Query<&'static Children>,
    mut transforms: Query<(
        &SpatialTransform,
        &mut GlobalSpatialTransform,
        Option<&crate::ShapeDeform>,
    )>,
    view_roles: Query<&'static CoordinateViewRole>,
    label_offsets: Query<&'static CoordinateLabelOffset>,
    parents: Query<&'static ChildOf>,
    mut pinned: Local<bool>,
) {
    let pin = camera.as_deref().and_then(|camera| hud_pin(camera));
    // Unpinned HUD transforms are exactly what propagation produced; recompose
    // once more after a pin ends so none keeps the last pinned placement.
    if pin.is_none() && !*pinned {
        return;
    }
    *pinned = pin.is_some();
    let pin = pin.unwrap_or(gaanim_core::kurbo::Affine::IDENTITY);
    let pin = GlobalSpatialTransform::from_local(&SpatialTransform::from_affine_2d(&pin));
    for root in &hud {
        let parent = parents.get(root).ok().map(ChildOf::parent);
        if parent.is_some_and(|parent| hud.contains(parent)) {
            continue;
        }
        let parent_global =
            parent.and_then(|parent| transforms.get(parent).ok().map(|(_, g, _)| *g));
        let pinned_parent = parent_global.map_or(pin, |parent| GlobalSpatialTransform {
            affine_2d: pin.affine_2d * parent.affine_2d,
            mat4: pin.mat4 * parent.mat4,
        });
        propagate_transforms_recursive(
            root,
            Some(pinned_parent),
            true,
            &mut transforms,
            &Propagation {
                children_query: &children_query,
                view_roles: &view_roles,
                label_offsets: &label_offsets,
                parents: &parents,
                stale: None,
            },
        );
    }
}

/// Run condition: skip opacity propagation when no local opacity has changed.
pub fn has_opacity_changes(
    query: Query<&Opacity, Or<(Changed<Opacity>, Added<Opacity>)>>,
    presences: Query<(), Changed<crate::Presence>>,
    mut removed: RemovedComponents<crate::Presence>,
) -> bool {
    !query.is_empty() || !presences.is_empty() || removed.read().next().is_some()
}

/// System: Propagate opacity cascade down the hierarchy using Bevy 0.19's `ChildOf` relation.
///
/// Only subtrees below an entity whose opacity inputs changed are visited.
pub fn opacity_propagation_system(
    mut queries: ParamSet<(Query<Entity, StaleOpacityFilter>, OpacityTargets)>,
    mut removed: (
        RemovedComponents<ChildOf>,
        RemovedComponents<crate::Presence>,
        RemovedComponents<Opacity>,
    ),
    children_query: Query<&'static Children>,
    local_opacities: Query<&'static Opacity>,
    parents: Query<&'static ChildOf>,
    presences: Query<&'static crate::Presence>,
) {
    let mut stale: EntityHashSet = queries.p0().iter().collect();
    stale.extend(removed.0.read());
    stale.extend(removed.1.read());
    stale.extend(removed.2.read());
    let mut opacities = queries.p1();
    let propagation = OpacityPropagation {
        children_query: &children_query,
        presences: &presences,
        stale: &stale,
    };
    for &entity in &stale {
        let mut ancestor = parents.get(entity).ok().map(ChildOf::parent);
        let mut covered = false;
        while let Some(current) = ancestor {
            if stale.contains(&current) {
                covered = true;
                break;
            }
            ancestor = parents.get(current).ok().map(ChildOf::parent);
        }
        if covered {
            continue;
        }
        let parent_opacity =
            inherited_opacity(entity, &parents, &local_opacities, &opacities, &presences);
        propagate_opacities_recursive(entity, parent_opacity, true, &mut opacities, &propagation);
    }
}

/// Local and world opacity of every entity propagation writes.
type OpacityTargets<'w, 's> = Query<'w, 's, (&'static Opacity, &'static mut GlobalOpacity)>;

/// Entities whose world opacity may differ from what propagation last wrote.
type StaleOpacityFilter = Or<(
    Changed<Opacity>,
    Changed<GlobalOpacity>,
    Changed<crate::Presence>,
    Changed<ChildOf>,
)>;

struct OpacityPropagation<'a, 'w1, 's1, 'w2, 's2> {
    children_query: &'a Query<'w1, 's1, &'static Children>,
    presences: &'a Query<'w2, 's2, &'static crate::Presence>,
    stale: &'a EntityHashSet,
}

/// The opacity the cascade hands to `entity`: that of the nearest ancestor
/// with a world opacity, times the presence of the structural entities in
/// between. Text and imported assets can contain structural grouping entities
/// without an `Opacity`; they never cut the cascade, and the cascade starts at
/// the topmost entity with an `Opacity`.
fn inherited_opacity(
    entity: Entity,
    parents: &Query<&'static ChildOf>,
    local_opacities: &Query<&'static Opacity>,
    opacities: &Query<(&Opacity, &mut GlobalOpacity)>,
    presences: &Query<&'static crate::Presence>,
) -> f32 {
    let mut product = 1.0;
    let mut cascade = 1.0;
    let mut ancestor = parents.get(entity).ok().map(ChildOf::parent);
    while let Some(current) = ancestor {
        if let Ok((_, global)) = opacities.get(current) {
            return product * global.0;
        }
        product *= presences.get(current).map_or(1.0, |presence| presence.0);
        if local_opacities.contains(current) {
            cascade = product;
        }
        ancestor = parents.get(current).ok().map(ChildOf::parent);
    }
    cascade
}

fn propagate_opacities_recursive(
    entity: Entity,
    parent_opacity: f32,
    parent_changed: bool,
    opacities: &mut Query<(&Opacity, &mut GlobalOpacity)>,
    propagation: &OpacityPropagation,
) {
    let stale = propagation.stale.contains(&entity);
    let recompute = parent_changed || stale;
    let presence = || {
        propagation
            .presences
            .get(entity)
            .map_or(1.0, |presence| presence.0)
    };
    let (current_opacity, changed) = match opacities.get_mut(entity) {
        Ok((local, mut global)) if recompute => {
            let value = local.0 * parent_opacity * presence();
            let changed = global.0 != value;
            if changed {
                global.0 = value;
            }
            (value, changed)
        }
        Ok((_, global)) => (global.0, false),
        Err(_) => (parent_opacity * presence(), recompute),
    };

    if let Ok(children) = propagation.children_query.get(entity) {
        for child in children.iter() {
            // A restored snapshot may already hold this entity's new value.
            propagate_opacities_recursive(
                *child,
                current_opacity,
                changed || stale,
                opacities,
                propagation,
            );
        }
    }
}

/// Visibility of tick `generation` at `ln_scale`, given `(ln scale, generation)`
/// anchors sorted by scale. Beyond the outermost anchors the nearest
/// generation is fully visible; between anchors of different generations the
/// incoming set fades in before the outgoing one fades out, so shared lines
/// never dip and the weights of all generations stay close to one.
pub fn coordinate_tick_level_weight(anchors: &[(f64, u32)], ln_scale: f64, generation: u32) -> f64 {
    let smoothstep = |edge0: f64, edge1: f64, x: f64| {
        let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
        t * t * (3.0 - 2.0 * t)
    };
    let (Some(first), Some(last)) = (anchors.first(), anchors.last()) else {
        return 1.0;
    };
    if ln_scale <= first.0 {
        return f64::from(u8::from(first.1 == generation));
    }
    if ln_scale >= last.0 {
        return f64::from(u8::from(last.1 == generation));
    }
    let Some(pair) = anchors
        .windows(2)
        .find(|pair| pair[0].0 <= ln_scale && ln_scale <= pair[1].0)
    else {
        return 0.0;
    };
    let ((low, low_generation), (high, high_generation)) = (pair[0], pair[1]);
    if low_generation == high_generation || high - low <= f64::EPSILON {
        return f64::from(u8::from(low_generation == generation));
    }
    let t = (ln_scale - low) / (high - low);
    if generation == low_generation {
        1.0 - smoothstep(0.35, 0.8, t)
    } else if generation == high_generation {
        smoothstep(0.2, 0.65, t)
    } else {
        0.0
    }
}

/// System: fade coordinate tick generations from the scale of their view.
pub fn coordinate_tick_level_system(
    mut levels: Query<(Entity, &CoordinateTickLevel, &mut Opacity)>,
    parents: Query<&ChildOf>,
    views: Query<(&CoordinateViewRole, &SpatialTransform)>,
) {
    for (entity, level, mut opacity) in &mut levels {
        let mut ancestor = entity;
        let mut view_scale = None;
        while let Ok(parent) = parents.get(ancestor) {
            ancestor = parent.parent();
            if let Ok((CoordinateViewRole::View, transform)) = views.get(ancestor) {
                view_scale = Some(transform.scale);
                break;
            }
        }
        let Some(scale) = view_scale else {
            continue;
        };
        let scale = if level.axis == 0 { scale.x } else { scale.y };
        let weight = coordinate_tick_level_weight(
            &level.anchors,
            scale.abs().max(1e-12).ln(),
            level.generation,
        ) as f32;
        if opacity.0 != weight {
            opacity.0 = weight;
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
///
/// Only entities whose local bounds or world transform changed, or whose world
/// bounds another system wrote (a snapshot restore), are recomputed.
pub fn world_bounds_propagation_system(
    mut query: Query<
        (&LocalBounds, &GlobalSpatialTransform, &mut WorldBounds),
        Or<(
            Changed<LocalBounds>,
            Changed<GlobalSpatialTransform>,
            Changed<WorldBounds>,
        )>,
    >,
) {
    for (local, global, mut world) in &mut query {
        // Use full 3D transform so that rotated/scaled 3D objects get correct AABB.
        // For pure 2D objects mat4 == affine_2d lifted to 3D, so result is identical to 2D path.
        world.0 = local.0.transform_mat4(&global.mat4);
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
        // Use 3D mat4 to extract true world position (supports 3D groups).
        let pos = global.mat4.transform_point3(gaanim_core::glam::DVec3::ZERO);
        world.0 = gaanim_math::Bounds3D::new_3d(
            pos.x - 0.5,
            pos.y - 0.5,
            pos.z - 0.5,
            pos.x + 0.5,
            pos.y + 0.5,
            pos.z + 0.5,
        );
    }
}

/// System: Propagate WorldBounds bottom-up for nested group hierarchies.
pub fn hierarchical_bounds_system(
    root_query: Query<Entity, (With<bevy::prelude::Children>, Without<ChildOf>)>,
    empty_root_group_query: Query<
        Entity,
        (
            With<GroupMarker>,
            Without<bevy::prelude::Children>,
            Without<ChildOf>,
        ),
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

/// System: Billboard - make entities face the camera (for 3D labels).
///
/// In perspective mode the Vello 2D camera is fixed at the origin (see
/// `sync_gaanim_camera_to_bevy_system`). The billboard's Vello `affine_2d` is
/// therefore computed by projecting the 3D world position to screen pixels via
/// `Camera::world_to_screen` and then mapping back to the fixed Vello world
/// through the inverse of the fixed orthographic Vello transform. This makes
/// the label appear at the correct screen location over the 3D geometry and
/// stay upright regardless of camera orbit.
pub fn billboard_system(
    camera: Option<bevy::prelude::Res<gaanim_math::ResolvedCamera>>,
    children_query: Query<&Children>,
    mut query: Query<
        (
            Entity,
            &mut GlobalSpatialTransform,
            Option<&mut bevy::prelude::Transform>,
        ),
        With<crate::components::Billboard>,
    >,
    mut child_transforms: Query<
        (&SpatialTransform, &mut GlobalSpatialTransform),
        Without<crate::components::Billboard>,
    >,
) {
    let Some(cam) = camera else { return };
    let cam_rot = cam.rotation;
    let is_perspective = matches!(cam.projection, gaanim_math::Projection::Perspective { .. });
    for (entity, mut global, transform_opt) in &mut query {
        // Preserve world position and scale, replace rotation with camera rotation.
        let world = global.mat4;
        let (scale, _rot, trans) = world.to_scale_rotation_translation();
        let billboard_mat =
            gaanim_core::glam::DMat4::from_scale_rotation_translation(scale, cam_rot, trans);
        global.mat4 = billboard_mat;
        if is_perspective {
            // Project 3D world position to screen, then map to fixed Vello world.
            let world_pos = gaanim_core::glam::DVec3::new(trans.x, trans.y, trans.z);
            let screen = cam.world_to_screen(world_pos);
            let eff = cam.viewport.scale.max(0.01);
            let hw = cam.viewport_width as f64 * 0.5;
            let hh = cam.viewport_height as f64 * 0.5 + cam.viewport.offset_y;
            // Fixed Vello transform: translate to center + scale (no rotation, no cam pos)
            let vello = gaanim_core::kurbo::Affine::translate((hw, hh))
                * gaanim_core::kurbo::Affine::scale_non_uniform(eff, -eff);
            let inv = vello.inverse();
            let vpos = inv * gaanim_core::kurbo::Point::new(screen.x, screen.y);
            global.affine_2d = gaanim_core::kurbo::Affine::translate((vpos.x, vpos.y))
                * gaanim_core::kurbo::Affine::scale_non_uniform(scale.x, scale.y);
        } else {
            // Orthographic: previous 2D behavior (rotate to stay upright)
            let z_angle = cam.z_angle();
            global.affine_2d = gaanim_core::kurbo::Affine::translate((trans.x, trans.y))
                * gaanim_core::kurbo::Affine::rotate(-z_angle)
                * gaanim_core::kurbo::Affine::scale_non_uniform(scale.x, scale.y);
        }
        if let Some(mut t) = transform_opt {
            let (scale_d, _, trans_d) = billboard_mat.to_scale_rotation_translation();
            t.translation =
                bevy::prelude::Vec3::new(trans_d.x as f32, trans_d.y as f32, trans_d.z as f32);
            t.rotation = bevy::prelude::Quat::from_xyzw(
                cam_rot.x as f32,
                cam_rot.y as f32,
                cam_rot.z as f32,
                cam_rot.w as f32,
            );
            t.scale =
                bevy::prelude::Vec3::new(scale_d.x as f32, scale_d.y as f32, scale_d.z as f32);
        }
        let current_global = *global;
        drop(global);

        // Propagate updated billboard transform to non-billboard child entities (e.g. text glyphs)
        propagate_billboard_children_recursive(
            entity,
            &current_global,
            &children_query,
            &mut child_transforms,
        );
    }
}

fn propagate_billboard_children_recursive(
    entity: Entity,
    parent_global: &GlobalSpatialTransform,
    children_query: &Query<&Children>,
    child_transforms: &mut Query<
        (&SpatialTransform, &mut GlobalSpatialTransform),
        Without<crate::components::Billboard>,
    >,
) {
    if let Ok(children) = children_query.get(entity) {
        for &child in children.iter() {
            if let Ok((child_local, mut child_global)) = child_transforms.get_mut(child) {
                *child_global =
                    GlobalSpatialTransform::from_parent_and_local(parent_global, child_local);
                let current_child_global = *child_global;
                drop(child_global);
                propagate_billboard_children_recursive(
                    child,
                    &current_child_global,
                    children_query,
                    child_transforms,
                );
            }
        }
    }
}

/// System: keep the local bounds of 3D content in step with its
/// geometry, for example a 3D traced path that grows.
#[allow(clippy::type_complexity)]
pub fn sync_3d_bounds_system(
    mut query: Query<
        (
            Option<&TriangleMeshData>,
            Option<&LineListData>,
            &mut LocalBounds,
        ),
        Or<(Changed<TriangleMeshData>, Changed<LineListData>)>,
    >,
) {
    for (mesh, lines, mut bounds) in &mut query {
        let points = mesh
            .map(|mesh| mesh.vertices.as_slice())
            .or(lines.map(|lines| lines.points.as_slice()))
            .unwrap_or_default();
        let mut min = gaanim_core::glam::DVec3::splat(f64::INFINITY);
        let mut max = gaanim_core::glam::DVec3::splat(f64::NEG_INFINITY);
        for point in points {
            let point =
                gaanim_core::glam::DVec3::new(point[0].into(), point[1].into(), point[2].into());
            min = min.min(point);
            max = max.max(point);
        }
        bounds.0 = if points.is_empty() {
            gaanim_math::Bounds3D::default()
        } else {
            gaanim_math::Bounds3D::new(min, max)
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::prelude::{BuildChildrenTransformExt, Schedule, World};

    #[test]
    fn hud_overlays_keep_their_pixels_while_the_2d_camera_moves() {
        use bevy::prelude::IntoScheduleConfigs;
        use gaanim_core::glam::{DQuat, DVec3};
        let authored = gaanim_math::Camera::ortho_2d_frame(16.0, 9.0, 1600, 900);
        let mut moved = authored;
        moved.position = DVec3::new(3.0, -1.0, 0.0);
        moved.rotation = DQuat::from_rotation_z(0.3);
        moved.projection = gaanim_math::Projection::Orthographic { zoom: 2.5 };

        let mut world = World::new();
        world.insert_resource(gaanim_math::ResolvedCamera::new(
            moved,
            gaanim_math::CameraViewport::default(),
        ));
        let panel = world
            .spawn((
                SpatialTransform::new_2d(5.0, 3.0),
                GlobalSpatialTransform::default(),
                crate::components::HudOverlay,
            ))
            .id();
        let glyph = world
            .spawn((
                SpatialTransform::new_2d(0.5, 0.0),
                GlobalSpatialTransform::default(),
                ChildOf(panel),
            ))
            .id();
        let world_item = world
            .spawn((
                SpatialTransform::new_2d(5.0, 3.0),
                GlobalSpatialTransform::default(),
            ))
            .id();
        let mut schedule = Schedule::default();
        schedule.add_systems(
            (
                transform_propagation_system.run_if(has_transform_changes),
                pin_hud_overlays_system,
            )
                .chain(),
        );

        let pixel = |world: &World, camera: &gaanim_math::Camera, entity: Entity| {
            let origin = world
                .get::<GlobalSpatialTransform>(entity)
                .unwrap()
                .affine_2d
                * gaanim_core::kurbo::Point::ORIGIN;
            camera.to_vello_transform() * origin
        };
        // Running twice checks that pinning does not compound when
        // propagation is skipped on an unchanged frame.
        for _ in 0..2 {
            schedule.run(&mut world);
            for (entity, authored_at) in [(panel, (5.0, 3.0)), (glyph, (5.5, 3.0))] {
                let expected =
                    authored.to_vello_transform() * gaanim_core::kurbo::Point::from(authored_at);
                let actual = pixel(&world, &moved, entity);
                assert!(
                    (actual - expected).hypot() < 1e-6,
                    "{actual:?} != {expected:?}"
                );
            }
        }
        // Scene content still moves with the camera.
        let expected = authored.to_vello_transform() * gaanim_core::kurbo::Point::new(5.0, 3.0);
        assert!((pixel(&world, &moved, world_item) - expected).hypot() > 1.0);

        // Resetting the camera returns the overlay to its authored place.
        world.insert_resource(gaanim_math::ResolvedCamera::new(
            authored,
            gaanim_math::CameraViewport::default(),
        ));
        schedule.run(&mut world);
        let global = world.get::<GlobalSpatialTransform>(panel).unwrap();
        assert_eq!(
            global.affine_2d * gaanim_core::kurbo::Point::ORIGIN,
            (5.0, 3.0).into()
        );
    }

    #[test]
    fn label_offsets_stay_unzoomed_while_their_anchor_follows_the_view() {
        use gaanim_core::glam::DVec2;
        let mut world = World::new();
        let root_local = SpatialTransform::new_2d(1.0, 2.0).scale_uniform(2.0);
        let root = world
            .spawn((root_local, GlobalSpatialTransform::default()))
            .id();
        let view = world
            .spawn((
                SpatialTransform::default(),
                GlobalSpatialTransform::default(),
                CoordinateViewRole::View,
                ChildOf(root),
            ))
            .id();
        // A tick number anchored at data point (3, 0), drawn 0.4 below it.
        let label = world
            .spawn((
                SpatialTransform::new_2d(3.0, -0.4),
                GlobalSpatialTransform::default(),
                CoordinateViewRole::Label,
                CoordinateLabelOffset(DVec2::new(0.0, -0.4)),
                ChildOf(view),
            ))
            .id();
        let mut schedule = Schedule::default();
        schedule.add_systems(transform_propagation_system);
        let origin = |world: &World| {
            let [_, _, _, _, x, y] = world
                .get::<GlobalSpatialTransform>(label)
                .unwrap()
                .affine_2d
                .as_coeffs();
            let w = world
                .get::<GlobalSpatialTransform>(label)
                .unwrap()
                .mat4
                .w_axis;
            assert!((w.x - x).abs() < 1e-12 && (w.y - y).abs() < 1e-12);
            (x, y)
        };

        schedule.run(&mut world);
        assert_eq!(
            origin(&world),
            (1.0 + 6.0, 2.0 - 0.8),
            "unzoomed is unchanged"
        );

        // Zoom y by 5 around the axis: the anchor (y = 0) stays, the offset
        // stays 0.4 (x2 root) instead of growing to 2.0.
        let zoom = SpatialTransform::default().with_scale_2d(3.0, 5.0);
        world.entity_mut(view).insert(zoom);
        schedule.run(&mut world);
        let (x, y) = origin(&world);
        assert!((x - (1.0 + 2.0 * 9.0)).abs() < 1e-12, "{x}");
        assert!((y - (2.0 - 0.8)).abs() < 1e-12, "{y}");
    }

    #[test]
    fn tick_levels_cross_fade_between_neighbouring_anchors() {
        let anchors = [(-1.0, 2), (0.0, 0), (0.5, 0), (1.5, 1)];
        let weight = |value, generation| coordinate_tick_level_weight(&anchors, value, generation);
        // At and beyond anchors exactly one generation is visible.
        assert_eq!((weight(-3.0, 2), weight(-3.0, 0)), (1.0, 0.0));
        assert_eq!((weight(0.0, 0), weight(0.0, 1)), (1.0, 0.0));
        assert_eq!(
            (weight(0.25, 0), weight(0.25, 1)),
            (1.0, 0.0),
            "same generation"
        );
        assert_eq!(
            (weight(1.5, 1), weight(9.0, 1), weight(9.0, 0)),
            (1.0, 1.0, 0.0)
        );
        // Between generations the incoming set appears before the outgoing fades.
        let early = 0.5 + 0.1;
        assert!(weight(early, 0) == 1.0 && weight(early, 1) == 0.0);
        let middle = 1.0;
        assert!(weight(middle, 0) > 0.5 && weight(middle, 1) > 0.5);
        let late = 1.5 - 0.1;
        assert!(weight(late, 0) == 0.0 && weight(late, 1) == 1.0);
        assert_eq!(weight(middle, 2), 0.0);
        assert_eq!(coordinate_tick_level_weight(&[], 3.0, 7), 1.0);
    }

    #[test]
    fn coordinate_view_scales_positions_but_keeps_label_basis_and_glyph_offsets() {
        use gaanim_core::glam::DVec3;
        let mut world = World::new();
        let root_local = SpatialTransform::new_2d(4.0, -2.0)
            .with_rotation_2d(0.3)
            .scale_uniform(1.5);
        let root = world
            .spawn((root_local, GlobalSpatialTransform::default()))
            .id();
        let view_local = SpatialTransform::new_2d(-3.0, 1.0).with_scale_2d(4.0, 0.5);
        let view = world
            .spawn((
                view_local,
                GlobalSpatialTransform::default(),
                CoordinateViewRole::View,
                ChildOf(root),
            ))
            .id();
        let layer_local = SpatialTransform::new_2d(0.5, 0.8).with_rotation_2d(0.1);
        let layer = world
            .spawn((
                layer_local,
                GlobalSpatialTransform::default(),
                ChildOf(view),
            ))
            .id();
        let label_local = SpatialTransform::new_2d(2.0, -1.0)
            .with_rotation_2d(0.7)
            .scale_uniform(0.8);
        let label = world
            .spawn((
                label_local,
                GlobalSpatialTransform::default(),
                CoordinateViewRole::Label,
                ChildOf(layer),
            ))
            .id();
        let glyph_local = SpatialTransform::new_2d(0.4, 0.2);
        let glyph = world
            .spawn((
                glyph_local,
                GlobalSpatialTransform::default(),
                ChildOf(label),
            ))
            .id();
        let plot = world
            .spawn((
                label_local,
                GlobalSpatialTransform::default(),
                ChildOf(layer),
            ))
            .id();
        let mut schedule = Schedule::default();
        schedule.add_systems(transform_propagation_system);

        // Include singular scales from fade animations: compensation must not
        // produce NaNs or cancel a user's intentional scale on the whole plot.
        for scale in [
            DVec3::new(4.0, 0.5, 1.0),
            DVec3::ONE,
            DVec3::new(0.0, 2.0, 1.0),
        ] {
            let mut view_local = view_local;
            view_local.scale = scale;
            world.entity_mut(view).insert(view_local);
            schedule.run(&mut world);
            let normal = root_local.to_mat4()
                * view_local.to_mat4()
                * layer_local.to_mat4()
                * label_local.to_mat4();
            let mut unscaled_view = view_local;
            unscaled_view.scale = DVec3::ONE;
            let mut expected = root_local.to_mat4()
                * unscaled_view.to_mat4()
                * layer_local.to_mat4()
                * label_local.to_mat4();
            expected.w_axis = normal.w_axis;
            let global = world.get::<GlobalSpatialTransform>(label).unwrap();
            assert!(global.mat4.abs_diff_eq(expected, 1e-9));
            if scale == DVec3::ONE {
                assert_eq!(global.mat4, normal, "unzoomed output must remain exact");
                assert_eq!(
                    global.affine_2d,
                    root_local.to_affine_2d()
                        * view_local.to_affine_2d()
                        * layer_local.to_affine_2d()
                        * label_local.to_affine_2d()
                );
            }
            for point in [DVec3::ZERO, DVec3::X, DVec3::Y] {
                let affine_point =
                    global.affine_2d * gaanim_core::kurbo::Point::new(point.x, point.y);
                let expected_point = expected.transform_point3(point);
                assert!((affine_point.x - expected_point.x).abs() < 1e-9);
                assert!((affine_point.y - expected_point.y).abs() < 1e-9);
            }
            assert!(
                world
                    .get::<GlobalSpatialTransform>(glyph)
                    .unwrap()
                    .mat4
                    .abs_diff_eq(expected * glyph_local.to_mat4(), 1e-9)
            );
            assert!(
                world
                    .get::<GlobalSpatialTransform>(plot)
                    .unwrap()
                    .mat4
                    .abs_diff_eq(normal, 1e-9)
            );
        }
    }

    #[test]
    fn three_d_bounds_follow_changed_geometry() {
        let mut world = World::new();
        let line = world
            .spawn((
                LineListData {
                    points: vec![[0.0, 0.0, 0.0], [1.0, 2.0, 3.0]],
                    indices: None,
                    strip: true,
                    color: gaanim_core::peniko::Color::WHITE,
                    colors: None,
                },
                LocalBounds::default(),
            ))
            .id();
        let mut schedule = Schedule::default();
        schedule.add_systems(sync_3d_bounds_system);
        schedule.run(&mut world);
        let bounds = world.get::<LocalBounds>(line).unwrap().0;
        assert_eq!(bounds.max, gaanim_core::glam::DVec3::new(1.0, 2.0, 3.0));

        world
            .get_mut::<LineListData>(line)
            .unwrap()
            .points
            .push([-4.0, 0.0, 0.0]);
        schedule.run(&mut world);
        assert_eq!(world.get::<LocalBounds>(line).unwrap().0.min.x, -4.0);
    }

    #[test]
    fn nested_descendants_receive_current_transform_and_opacity() {
        let mut world = World::new();
        let group = world
            .spawn((
                SpatialTransform::new_2d(10.0, 0.0),
                GlobalSpatialTransform::default(),
                Opacity(0.5),
                GlobalOpacity::default(),
            ))
            .id();
        let text = world
            .spawn((
                SpatialTransform::new_2d(2.0, 0.0),
                GlobalSpatialTransform::default(),
                Opacity(0.4),
                GlobalOpacity::default(),
            ))
            .id();
        let glyph = world
            .spawn((
                SpatialTransform::new_2d(3.0, 0.0),
                GlobalSpatialTransform::default(),
                Opacity(0.25),
                GlobalOpacity::default(),
            ))
            .id();
        world.entity_mut(text).set_parent_in_place(group);
        world.entity_mut(glyph).set_parent_in_place(text);

        let mut schedule = Schedule::default();
        schedule.add_systems((transform_propagation_system, opacity_propagation_system));
        schedule.run(&mut world);

        let tx = world
            .get::<GlobalSpatialTransform>(glyph)
            .unwrap()
            .affine_2d
            .as_coeffs()[4];
        assert!((tx - 15.0).abs() < f64::EPSILON);
        assert!((world.get::<GlobalOpacity>(glyph).unwrap().0 - 0.05).abs() < f32::EPSILON);
    }

    #[test]
    fn presence_scales_the_cascade_without_touching_local_opacity() {
        let mut world = World::new();
        let group = world.spawn((Opacity(0.5), GlobalOpacity::default())).id();
        let copy = world
            .spawn((
                Opacity(0.8),
                GlobalOpacity::default(),
                crate::Presence(0.25),
            ))
            .id();
        let leaf = world.spawn((Opacity(1.0), GlobalOpacity::default())).id();
        world.entity_mut(copy).set_parent_in_place(group);
        world.entity_mut(leaf).set_parent_in_place(copy);

        let mut schedule = Schedule::default();
        schedule.add_systems(opacity_propagation_system);
        schedule.run(&mut world);

        assert!((world.get::<GlobalOpacity>(copy).unwrap().0 - 0.1).abs() < 1e-6);
        assert!((world.get::<GlobalOpacity>(leaf).unwrap().0 - 0.1).abs() < 1e-6);
        assert_eq!(world.get::<Opacity>(copy).unwrap().0, 0.8);
    }

    #[test]
    fn children_follow_a_parent_whose_new_value_was_already_written() {
        // A snapshot restore writes both local and world values; the children
        // must still receive the parent's new world value.
        let mut world = World::new();
        let parent = world
            .spawn((
                Opacity(0.2),
                GlobalOpacity::default(),
                SpatialTransform::default(),
                GlobalSpatialTransform::default(),
            ))
            .id();
        let child = world
            .spawn((
                Opacity(1.0),
                GlobalOpacity::default(),
                SpatialTransform::default(),
                GlobalSpatialTransform::default(),
            ))
            .id();
        world.entity_mut(child).set_parent_in_place(parent);
        let mut schedule = Schedule::default();
        schedule.add_systems((opacity_propagation_system, transform_propagation_system));
        schedule.run(&mut world);
        assert!((world.get::<GlobalOpacity>(child).unwrap().0 - 0.2).abs() < 1e-6);

        let moved = SpatialTransform {
            translation: gaanim_core::glam::DVec3::new(3.0, 0.0, 0.0),
            ..Default::default()
        };
        world.entity_mut(parent).insert((
            Opacity(0.8),
            GlobalOpacity(0.8),
            moved,
            GlobalSpatialTransform::from_local(&moved),
        ));
        schedule.run(&mut world);

        assert!((world.get::<GlobalOpacity>(child).unwrap().0 - 0.8).abs() < 1e-6);
        let global = world.get::<GlobalSpatialTransform>(child).unwrap();
        assert_eq!(global.affine_2d.translation().x, 3.0);
    }

    #[test]
    fn propagation_revisits_only_changed_subtrees_and_keeps_the_rest() {
        let mut world = World::new();
        let spawn = |world: &mut World, x: f64| {
            world
                .spawn((
                    Opacity(0.5),
                    GlobalOpacity::default(),
                    SpatialTransform {
                        translation: gaanim_core::glam::DVec3::new(x, 0.0, 0.0),
                        ..Default::default()
                    },
                    GlobalSpatialTransform::default(),
                ))
                .id()
        };
        let still = spawn(&mut world, 1.0);
        let still_child = spawn(&mut world, 1.0);
        let moving = spawn(&mut world, 2.0);
        let moving_child = spawn(&mut world, 1.0);
        world.entity_mut(still_child).set_parent_in_place(still);
        world.entity_mut(moving_child).set_parent_in_place(moving);
        let mut schedule = Schedule::default();
        schedule.add_systems((opacity_propagation_system, transform_propagation_system));
        schedule.run(&mut world);
        let x = |world: &World, entity| {
            world
                .get::<GlobalSpatialTransform>(entity)
                .unwrap()
                .affine_2d
                .translation()
                .x
        };
        assert_eq!(x(&world, still_child), 2.0);
        assert_eq!(x(&world, moving_child), 3.0);

        world
            .get_mut::<SpatialTransform>(moving)
            .unwrap()
            .translation
            .x = 5.0;
        world.get_mut::<Opacity>(moving).unwrap().0 = 1.0;
        let before = world.change_tick();
        schedule.run(&mut world);

        assert_eq!(x(&world, moving_child), 6.0);
        assert!((world.get::<GlobalOpacity>(moving_child).unwrap().0 - 0.5).abs() < 1e-6);
        assert_eq!(x(&world, still_child), 2.0);
        // The untouched subtree keeps its change ticks.
        let ticks = world
            .entity(still_child)
            .get_change_ticks::<GlobalSpatialTransform>()
            .unwrap();
        assert!(!ticks.is_changed(before, world.change_tick()));
    }

    #[test]
    fn opacity_cascade_crosses_structural_nodes_without_opacity() {
        let mut world = World::new();
        let root = world.spawn((Opacity(0.5), GlobalOpacity::default())).id();
        let structural = world.spawn_empty().id();
        let glyph = world.spawn((Opacity(0.4), GlobalOpacity::default())).id();
        world.entity_mut(structural).set_parent_in_place(root);
        world.entity_mut(glyph).set_parent_in_place(structural);

        let mut schedule = Schedule::default();
        schedule.add_systems(opacity_propagation_system);
        schedule.run(&mut world);

        let opacity = world.get::<GlobalOpacity>(glyph).unwrap().0;
        assert!(
            (opacity - 0.2).abs() < f32::EPSILON,
            "expected propagated opacity 0.2, got {opacity}"
        );
    }
}
