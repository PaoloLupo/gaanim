use crate::components::{
    AxisLabelGap, CoordinateLabelOffset, CoordinateTickLevel, CoordinateViewRole, FillBrush,
    GlobalOpacity, GroupMarker, LineListData, LocalBounds, Opacity, StrokeBrush, TriangleMeshData,
    WorldBounds,
};
use bevy::ecs::entity::EntityHashSet;
use bevy::prelude::{
    Added, Changed, ChildOf, Children, Entity, Local, Or, ParamSet, Query, RemovedComponents, Res,
    ResMut, With, Without,
};
use gaanim_math::{GlobalSpatialTransform, SpatialTransform};

/// Resolve the timeline-authored camera and an optional presentation override.
///
/// While a transition shows the outgoing segment with the camera it ended
/// with ([`crate::SceneTransitionFrame::outgoing_rest`]), that side keeps
/// it when it differs from the resolved camera; an editor view override
/// shows both sides through itself.
pub fn resolve_camera_system(
    authored: Option<Res<gaanim_math::Camera>>,
    rig: Option<Res<gaanim_math::CameraRigCamera>>,
    view_override: Res<gaanim_math::CameraViewOverride>,
    viewport: Res<gaanim_math::CameraViewport>,
    mut resolved: ResMut<gaanim_math::ResolvedCamera>,
    transition: Option<ResMut<crate::SceneTransitionFrame>>,
) {
    if let Some(camera) = view_override
        .0
        .or_else(|| rig.as_deref().map(|rig| rig.0))
        .or_else(|| authored.as_deref().copied())
    {
        resolved.camera = camera;
        resolved.viewport = *viewport;
    }
    if let Some(mut frame) = transition {
        let outgoing = frame
            .outgoing_camera
            .filter(|_| view_override.0.is_none() && frame.outgoing_rest.is_some());
        if frame.outgoing_camera.is_some() || outgoing.is_some() {
            frame.set_outgoing_camera(outgoing, &resolved.camera);
        }
    }
}

/// Run condition: skip transform propagation when no local transform has changed.
///
/// Since all clip animations explicitly store `from`/`to` values and write to
/// `SpatialTransform` (which updates Bevy's change tick), and the only other
/// mutation path is `seek()` snapshot restore (which also updates ticks), this
/// condition correctly detects every scenario where propagation is needed.
#[allow(clippy::type_complexity)]
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
pub(crate) struct Propagation<'a, 'w1, 's1, 'w2, 's2, 'w3, 's3, 'w4, 's4> {
    pub(crate) children_query: &'a Query<'w1, 's1, &'static Children>,
    pub(crate) view_roles: &'a Query<'w2, 's2, &'static CoordinateViewRole>,
    pub(crate) label_offsets: &'a Query<'w3, 's3, &'static CoordinateLabelOffset>,
    pub(crate) parents: &'a Query<'w4, 's4, &'static ChildOf>,
    /// Entities to recompute even when their parent did not change; `None`
    /// recomputes the whole subtree.
    pub(crate) stale: Option<&'a EntityHashSet>,
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

pub(crate) fn propagate_transforms_recursive(
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
        while let Ok((local, _, _)) = transforms.get(ancestor) {
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

/// The camera `entity` is drawn with: the one its side of a transition
/// keeps, or `current`.
fn side_camera<'a>(
    transition: Option<&'a crate::SceneTransitionFrame>,
    current: &'a gaanim_math::Camera,
    entity: Entity,
    parents: &Query<&'static ChildOf>,
) -> &'a gaanim_math::Camera {
    transition.map_or(current, |frame| {
        let side = frame.side_of(entity, |entity| {
            parents.get(entity).ok().map(ChildOf::parent)
        });
        frame.camera_for(side, current)
    })
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

/// World transform that moves a parallax layer at `depth` as if the camera
/// sat `depth` times farther from it than from the scene plane.
///
/// The layer sees a camera centered at `position / depth`, zoomed by
/// `zoom^(1/depth)` and with the same roll: depth 1 is the scene plane (no
/// correction), larger depths drift and zoom less, smaller depths more, and
/// an infinite depth only follows the roll. `None` when no correction applies,
/// as with a perspective camera or an invalid depth.
pub fn parallax_pin(
    camera: &gaanim_math::Camera,
    depth: f64,
) -> Option<gaanim_core::kurbo::Affine> {
    use gaanim_core::kurbo::Affine;
    if !(depth > 0.0) {
        return None;
    }
    let gaanim_math::Projection::Orthographic { zoom } = camera.projection else {
        return None;
    };
    if !zoom.is_finite() || zoom <= 0.0 {
        return None;
    }
    let factor = depth.recip();
    let view = |x: f64, y: f64, zoom: f64| {
        Affine::translate((x, y)) * Affine::rotate(camera.z_angle()) * Affine::scale(zoom.recip())
    };
    // A point authored at `p` must sit at `pin * p` so that the real camera
    // shows it where the layer's own camera would.
    let pin = view(camera.position.x, camera.position.y, zoom)
        * view(
            camera.position.x * factor,
            camera.position.y * factor,
            zoom.powf(factor),
        )
        .inverse();
    (pin != Affine::IDENTITY).then_some(pin)
}

/// [`parallax_pin`] of the camera a frame shows, read like [`world_hud_pin`].
pub fn world_parallax_pin(
    world: &bevy::prelude::World,
    depth: f64,
) -> Option<gaanim_core::kurbo::Affine> {
    let camera = world
        .get_resource::<gaanim_math::CameraViewOverride>()
        .and_then(|view| view.0)
        .or_else(|| world.get_resource::<gaanim_math::Camera>().copied())?;
    parallax_pin(&camera, depth)
}

/// System: move parallax layers by their share of the camera motion.
///
/// Runs after [`transform_propagation_system`] and before
/// [`pin_hud_overlays_system`], recomposing every layer subtree under its
/// [`parallax_pin`] like the HUD pin does. A layer nested in another layer
/// follows the outer one only.
#[allow(clippy::too_many_arguments)]
pub fn pin_parallax_layers_system(
    camera: Option<Res<gaanim_math::ResolvedCamera>>,
    transition: Option<Res<crate::SceneTransitionFrame>>,
    layers: Query<(Entity, &crate::components::ParallaxLayer)>,
    children_query: Query<&'static Children>,
    mut transforms: Query<(
        &SpatialTransform,
        &mut GlobalSpatialTransform,
        Option<&crate::ShapeDeform>,
    )>,
    view_roles: Query<&'static CoordinateViewRole>,
    label_offsets: Query<&'static CoordinateLabelOffset>,
    parents: Query<&'static ChildOf>,
    mut pinned: Local<EntityHashSet>,
) {
    let inside_layer = |entity: Entity| {
        let mut current = entity;
        for _ in 0..256 {
            let Ok(parent) = parents.get(current).map(ChildOf::parent) else {
                return false;
            };
            if layers.contains(parent) {
                return true;
            }
            current = parent;
        }
        false
    };
    let mut now_pinned = EntityHashSet::default();
    for (root, layer) in &layers {
        let pin = camera
            .as_deref()
            .map(|camera| side_camera(transition.as_deref(), &camera.camera, root, &parents))
            .and_then(|camera| parallax_pin(camera, layer.depth));
        // Unpinned layers are exactly what propagation produced; recompose
        // once more after a pin ends so none keeps the last pinned placement.
        if pin.is_none() && !pinned.contains(&root) || inside_layer(root) {
            continue;
        }
        if pin.is_some() {
            now_pinned.insert(root);
        }
        let pin = pin.unwrap_or(gaanim_core::kurbo::Affine::IDENTITY);
        let pin = GlobalSpatialTransform::from_local(&SpatialTransform::from_affine_2d(&pin));
        let parent_global = parents
            .get(root)
            .ok()
            .and_then(|parent| transforms.get(parent.parent()).ok().map(|(_, g, _)| *g));
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
    *pinned = now_pinned;
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
    transition: Option<Res<crate::SceneTransitionFrame>>,
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
    layers: Query<&'static crate::components::ParallaxLayer>,
    mut pinned: Local<bool>,
) {
    let transition = transition.as_deref();
    // A HUD of the side a transition takes away stays where that side's
    // camera put it.
    let camera_of = |root: Entity| {
        camera
            .as_deref()
            .map(|camera| side_camera(transition, &camera.camera, root, &parents))
    };
    let pin = camera.as_deref().and_then(|camera| hud_pin(camera));
    let outgoing_pin = transition
        .and_then(|frame| frame.outgoing_camera.as_ref())
        .and_then(hud_pin);
    // A HUD inside a parallax layer stays on screen: the layer's pin, which
    // `pin_parallax_layers_system` put first in its parent's transform, is
    // taken back out.
    let parallax_unpin = |root: Entity| {
        let mut depth = None;
        let mut current = root;
        for _ in 0..256 {
            let Ok(parent) = parents.get(current).map(ChildOf::parent) else {
                break;
            };
            if let Ok(layer) = layers.get(parent) {
                depth = Some(layer.depth);
            }
            current = parent;
        }
        parallax_pin(camera_of(root)?, depth?).map(|pin| {
            GlobalSpatialTransform::from_local(&SpatialTransform::from_affine_2d(&pin.inverse()))
        })
    };
    // Unpinned HUD transforms are exactly what propagation produced; recompose
    // once more after a pin ends so none keeps the last pinned placement.
    if pin.is_none() && outgoing_pin.is_none() && !*pinned {
        return;
    }
    *pinned = pin.is_some() || outgoing_pin.is_some();
    for root in &hud {
        let parent = parents.get(root).ok().map(ChildOf::parent);
        if parent.is_some_and(|parent| hud.contains(parent)) {
            continue;
        }
        let pin = camera_of(root)
            .and_then(hud_pin)
            .unwrap_or(gaanim_core::kurbo::Affine::IDENTITY);
        let pin = GlobalSpatialTransform::from_local(&SpatialTransform::from_affine_2d(&pin));
        let parent_global = parent
            .and_then(|parent| transforms.get(parent).ok().map(|(_, g, _)| *g))
            .map(|parent| match parallax_unpin(root) {
                Some(unpin) => GlobalSpatialTransform {
                    affine_2d: unpin.affine_2d * parent.affine_2d,
                    mat4: unpin.mat4 * parent.mat4,
                },
                None => parent,
            });
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
#[allow(clippy::type_complexity)]
pub fn has_opacity_changes(
    query: Query<&Opacity, Or<(Changed<Opacity>, Added<Opacity>)>>,
    presences: Query<
        (),
        Or<(
            Changed<crate::Presence>,
            Changed<crate::Occlusion>,
            Changed<crate::AxisBoxShown>,
        )>,
    >,
    mut removed: (
        RemovedComponents<crate::Presence>,
        RemovedComponents<crate::Occlusion>,
        RemovedComponents<crate::AxisBoxShown>,
    ),
) -> bool {
    !query.is_empty()
        || !presences.is_empty()
        || removed.0.read().next().is_some()
        || removed.1.read().next().is_some()
        || removed.2.read().next().is_some()
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
        RemovedComponents<crate::Occlusion>,
        RemovedComponents<crate::AxisBoxShown>,
    ),
    children_query: Query<&'static Children>,
    local_opacities: Query<&'static Opacity>,
    parents: Query<&'static ChildOf>,
    presences: Query<OpacityFactors>,
) {
    let mut stale: EntityHashSet = queries.p0().iter().collect();
    stale.extend(removed.0.read());
    stale.extend(removed.1.read());
    stale.extend(removed.2.read());
    stale.extend(removed.3.read());
    stale.extend(removed.4.read());
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
    Changed<crate::Occlusion>,
    Changed<crate::AxisBoxShown>,
    Changed<ChildOf>,
)>;

/// An entity's extra opacity factors: a repeater copy's presence, a
/// billboard's occlusion by 3D meshes, and whether a grid line on a 3D box's
/// edge shows.
type OpacityFactors = (
    Option<&'static crate::Presence>,
    Option<&'static crate::Occlusion>,
    Option<&'static crate::AxisBoxShown>,
);

/// The product of an entity's extra opacity factors.
fn opacity_factor(
    factors: (
        Option<&crate::Presence>,
        Option<&crate::Occlusion>,
        Option<&crate::AxisBoxShown>,
    ),
) -> f32 {
    let shown = factors.2.is_none_or(|shown| shown.0);
    factors.0.map_or(1.0, |presence| presence.0)
        * factors.1.map_or(1.0, |occlusion| occlusion.0)
        * if shown { 1.0 } else { 0.0 }
}

struct OpacityPropagation<'a, 'w1, 's1, 'w2, 's2> {
    children_query: &'a Query<'w1, 's1, &'static Children>,
    presences: &'a Query<'w2, 's2, OpacityFactors>,
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
    presences: &Query<OpacityFactors>,
) -> f32 {
    let mut product = 1.0;
    let mut cascade = 1.0;
    let mut ancestor = parents.get(entity).ok().map(ChildOf::parent);
    while let Some(current) = ancestor {
        if let Ok((_, global)) = opacities.get(current) {
            return product * global.0;
        }
        product *= presences.get(current).map_or(1.0, opacity_factor);
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
            .map_or(1.0, opacity_factor)
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
#[allow(clippy::type_complexity)]
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
#[allow(clippy::type_complexity)]
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
        let bounds = local.0.transform_mat4(&global.mat4);
        if world.0 != bounds {
            world.0 = bounds;
        }
    }
}

/// System: Approximate WorldBounds for entities without LocalBounds using transform position.
#[allow(clippy::type_complexity)]
pub fn world_bounds_fallback_system(
    mut query: Query<
        (&GlobalSpatialTransform, &mut WorldBounds),
        (Without<LocalBounds>, Without<GroupMarker>),
    >,
) {
    for (global, mut world) in &mut query {
        // Use 3D mat4 to extract true world position (supports 3D groups).
        let pos = global.mat4.transform_point3(gaanim_core::glam::DVec3::ZERO);
        let bounds = gaanim_math::Bounds3D::new_3d(
            pos.x - 0.5,
            pos.y - 0.5,
            pos.z - 0.5,
            pos.x + 0.5,
            pos.y + 0.5,
            pos.z + 0.5,
        );
        // Unchanged bounds are not rewritten, so change detection only
        // reports entities that actually moved.
        if world.0 != bounds {
            world.0 = bounds;
        }
    }
}

/// System: Propagate WorldBounds bottom-up for nested group hierarchies.
#[allow(clippy::type_complexity)]
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
        if let Ok(mut b) = bounds_query.get_mut(entity)
            && b.0 != gaanim_math::Bounds3D::default()
        {
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
            let bounds = if union_bounds.min.x != f64::INFINITY {
                union_bounds
            } else {
                gaanim_math::Bounds3D::default()
            };
            if b.0 != bounds {
                b.0 = bounds;
            }
            return bounds;
        }
    } else if let Ok(b) = bounds_query.get(entity) {
        return b.0;
    }

    union_bounds
}

/// System: Propagate styling changes (FillBrush/StrokeBrush) from groups to their children.
#[allow(clippy::type_complexity)]
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
///
/// A billboard with an [`AxisLabelGap`] is placed afresh from its anchor on
/// the axis every frame, then moved on screen to keep its gap from the axis
/// (see [`axis_label_shift`]); its world position becomes the point at the
/// anchor's depth that is drawn there, so occlusion tests where it shows.
///
/// A billboard the 3D pass draws has its [`BillboardPlane`] kept too: a
/// plane facing the camera at that point, drawn where the 2D one would be
/// and as large (see [`billboard_plane`]), so meshes and lines in front of
/// it hide it pixel by pixel.
///
/// A billboard of the side a transition takes away faces the camera that
/// side keeps (see [`crate::SceneTransitionFrame::camera_for`]).
///
/// [`BillboardPlane`]: crate::components::BillboardPlane
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
pub fn billboard_system(
    camera: Option<bevy::prelude::Res<gaanim_math::ResolvedCamera>>,
    transition: Option<Res<crate::SceneTransitionFrame>>,
    parents: Query<&ChildOf>,
    children_query: Query<&Children>,
    mut query: Query<
        (
            Entity,
            &mut GlobalSpatialTransform,
            Option<&mut bevy::prelude::Transform>,
            Option<(
                &AxisLabelGap,
                &SpatialTransform,
                Option<&ChildOf>,
                Option<&LocalBounds>,
            )>,
            Option<&mut crate::components::BillboardPlane>,
            Option<&LocalBounds>,
            Option<&crate::components::BillboardShift>,
        ),
        With<crate::components::Billboard>,
    >,
    mut child_transforms: Query<
        (&SpatialTransform, &mut GlobalSpatialTransform),
        Without<crate::components::Billboard>,
    >,
) {
    use gaanim_core::glam::{DMat4, DVec2, DVec3};
    use gaanim_core::kurbo::{Affine, Point};

    let Some(current) = camera else { return };
    let current = &current.camera;
    for (entity, mut global, transform_opt, axis_label, plane, bounds, shift) in &mut query {
        let cam = transition.as_deref().map_or(current, |frame| {
            let side = frame.side_of(entity, |entity| {
                parents.get(entity).ok().map(ChildOf::parent)
            });
            frame.camera_for(side, current)
        });
        let cam_rot = cam.rotation;
        let is_perspective = matches!(cam.projection, gaanim_math::Projection::Perspective { .. });
        // In perspective, the canvas is the fixed Vello world the 3D pass maps
        // to (`canvas_projection` in the renderer), in scene units with y up;
        // the canvas camera applies the host viewport to both.
        let ppu = cam.pixels_per_unit().max(1e-9);
        let hw = cam.viewport_width as f64 * 0.5;
        let hh = cam.viewport_height as f64 * 0.5;
        let vello = Affine::translate((hw, hh)) * Affine::scale_non_uniform(ppu, -ppu);
        let inv = vello.inverse();
        // Orthographic: turn with the camera, whose view turns the other way,
        // so the label stays upright on screen.
        let z_angle = if is_perspective { 0.0 } else { cam.z_angle() };
        let project = |point: DVec3| {
            if is_perspective {
                let screen = cam.world_to_screen(point);
                let at = inv * Point::new(screen.x, screen.y);
                DVec2::new(at.x, at.y)
            } else {
                point.truncate()
            }
        };
        // Preserve world position and scale, replace rotation with camera rotation.
        // The result is built aside and written only when it differs, so a
        // static billboard does not report a changed transform every frame.
        let mut billboard = *global;
        let parent_of = |parent: Option<&ChildOf>| match parent {
            None => Some(DMat4::IDENTITY),
            Some(parent) => child_transforms
                .get(parent.parent())
                .ok()
                .map(|(_, global)| global.mat4),
        };
        let axis_label = axis_label.and_then(|(gap, local, parent, bounds)| {
            Some((gap, parent_of(parent)?, local, bounds))
        });
        // A label that follows its axis to the edge the camera shows best
        // moves with it, and its tick turns to that edge's outside.
        let follow = axis_label.and_then(|(gap, parent, _, _)| {
            let axis = gap.follows?;
            let layout = crate::AxisBoxLayout::for_view(&cam, parent, gap.bounds);
            Some((axis, layout))
        });
        let world = match axis_label {
            Some((gap, parent, local, _)) => {
                let offset = follow.map_or(DVec3::ZERO, |(axis, layout)| {
                    layout.edge_offset(axis, gap.bounds)
                });
                if offset == DVec3::ZERO {
                    parent * local.to_mat4()
                } else {
                    parent * DMat4::from_translation(offset) * local.to_mat4()
                }
            }
            None => billboard.mat4,
        };
        let (scale, _rot, mut trans) = world.to_scale_rotation_translation();
        let mut at = project(trans);
        // How near an axis label seen through its box comes: in front of the
        // far faces, where its grid lies, and behind what the box holds.
        let mut front = None;
        if let Some((gap, parent, _, bounds)) = axis_label {
            let turn = Affine::rotate(z_angle);
            let corners: Vec<DVec2> = bounds
                .map(|bounds| {
                    let (low, high) = (bounds.0.min, bounds.0.max);
                    [
                        (low.x, low.y),
                        (high.x, low.y),
                        (high.x, high.y),
                        (low.x, high.y),
                    ]
                    .map(|(x, y)| {
                        let corner = turn * Point::new(x * scale.x, y * scale.y);
                        DVec2::new(corner.x, corner.y)
                    })
                    .to_vec()
                })
                .unwrap_or_default();
            // The axis's direction, from a short step: a whole one could
            // reach behind the camera.
            const STEP: f64 = 1e-2;
            let axis = parent.transform_vector3(gap.axis).normalize_or_zero() * STEP;
            let [low, high] = gap.bounds;
            let box_corners: Vec<DVec2> = (0..8)
                .map(|corner| {
                    let pick = |bit: usize, low: f64, high: f64| {
                        if corner & bit == 0 { low } else { high }
                    };
                    let point = DVec3::new(
                        pick(1, low.x, high.x),
                        pick(2, low.y, high.y),
                        pick(4, low.z, high.z),
                    );
                    project(parent.transform_point3(point)) - at
                })
                .filter(|corner| corner.is_finite())
                .collect();
            let tick = follow.map_or(gap.tick, |(axis, layout)| {
                DVec3::from_array(std::array::from_fn(|dimension| {
                    if layout.edges[axis][dimension] {
                        -gap.tick[dimension]
                    } else {
                        gap.tick[dimension]
                    }
                }))
            });
            let shift = axis_label_shift(
                gap,
                (project(trans + axis) - at) / STEP,
                project(trans + parent.transform_vector3(tick)) - at,
                &box_corners,
                &corners,
            );
            let shifted = at + shift;
            let point = if is_perspective {
                // The point at the anchor's depth drawn at `shifted`.
                let view_projection = cam.projection_matrix() * cam.view_matrix();
                let depth = view_projection.project_point3(trans).z;
                let screen = vello * Point::new(shifted.x, shifted.y);
                let ndc = DVec3::new(screen.x / hw - 1.0, 1.0 - screen.y / hh, depth);
                view_projection.inverse().project_point3(ndc)
            } else {
                DVec3::new(shifted.x, shifted.y, trans.z)
            };
            if shifted.is_finite() && point.is_finite() {
                at = shifted;
                trans = point;
            }
            // Through its text's middle and corners.
            let middle = corners.iter().copied().sum::<DVec2>() / corners.len().max(1) as f64;
            let sights: Vec<DVec2> = corners.iter().copied().chain([middle]).collect();
            front = box_exit_depth(&cam, parent, gap.bounds, trans, &sights);
        }
        // Its anchor on the point and its offset, in its own units.
        let shift = shift.map_or(DVec3::ZERO, |shift| {
            shift.local(bounds.map(|bounds| bounds.0), scale)
        });
        let billboard_mat = DMat4::from_scale_rotation_translation(scale, cam_rot, trans)
            * DMat4::from_translation(shift);
        billboard.mat4 = billboard_mat;
        billboard.affine_2d = Affine::translate((at.x, at.y))
            * Affine::rotate(z_angle)
            * Affine::scale_non_uniform(scale.x, scale.y)
            * Affine::translate((shift.x, shift.y));
        if *global != billboard {
            *global = billboard;
        }
        if let Some(mut plane) = plane {
            // Half the label's larger side on the canvas.
            let half = bounds
                .map(|bounds| bounds.0.max - bounds.0.min)
                .filter(|size| size.is_finite())
                .map_or(0.0, |size| {
                    0.5 * (size.x * scale.x.abs()).max(size.y * scale.y.abs())
                });
            // An axis label behind its box comes to where its lines of sight
            // leave it, just in front of its grid, so that the grid does not
            // cross it and the plot inside the box still hides it; it comes
            // no nearer, as a surface in the box may lie right by it.
            let depth = (trans - cam.position).dot(cam.rotation * DVec3::NEG_Z);
            let pull = if front.is_some_and(|front| front < depth) {
                0.0
            } else {
                half
            };
            let mut drawn = billboard;
            if let Some((point, units)) = billboard_plane(&cam, trans, pull, front) {
                drawn.mat4 = DMat4::from_scale_rotation_translation(scale * units, cam_rot, point)
                    * DMat4::from_translation(shift);
            }
            if plane.0 != drawn {
                plane.0 = drawn;
            }
        }
        if let Some(mut t) = transform_opt {
            let (scale_d, _, trans_d) = billboard_mat.to_scale_rotation_translation();
            let mut next = *t;
            next.translation =
                bevy::prelude::Vec3::new(trans_d.x as f32, trans_d.y as f32, trans_d.z as f32);
            next.rotation = bevy::prelude::Quat::from_xyzw(
                cam_rot.x as f32,
                cam_rot.y as f32,
                cam_rot.z as f32,
                cam_rot.w as f32,
            );
            next.scale =
                bevy::prelude::Vec3::new(scale_d.x as f32, scale_d.y as f32, scale_d.z as f32);
            if *t != next {
                *t = next;
            }
        }
        let current_global = billboard;

        // Propagate updated billboard transform to non-billboard child entities (e.g. text glyphs)
        propagate_billboard_children_recursive(
            entity,
            &current_global,
            &children_query,
            &mut child_transforms,
        );
    }
}

/// The nearest depth along the view at which the lines of sight through
/// `sights` leave the box `bounds` of `parent`'s space, a hundredth of their
/// way through it before its far face so that lines drawn on that face stay
/// behind: `sights` are canvas offsets on the plane facing the camera at
/// `point`, as a billboard drawn in the 3D pass lies (see
/// [`billboard_plane`]). `None` when none crosses the box.
fn box_exit_depth(
    camera: &gaanim_math::Camera,
    parent: gaanim_core::glam::DMat4,
    [low, high]: [gaanim_core::glam::DVec3; 2],
    point: gaanim_core::glam::DVec3,
    sights: &[gaanim_core::glam::DVec2],
) -> Option<f64> {
    use gaanim_core::glam::DVec3;
    let forward = camera.rotation * DVec3::NEG_Z;
    let (right, up) = (camera.rotation * DVec3::X, camera.rotation * DVec3::Y);
    let depth = (point - camera.position).dot(forward);
    // World units a canvas unit spans on the plane, and whether the lines of
    // sight meet at the eye.
    let (units, perspective) = match camera.projection {
        gaanim_math::Projection::Orthographic { .. } => (1.0, false),
        gaanim_math::Projection::Perspective { fov_y, .. } => (
            2.0 * (fov_y * 0.5).tan() / camera.frame_height.max(1e-9) * depth,
            true,
        ),
    };
    let to_local = parent.inverse();
    sights
        .iter()
        .filter_map(|sight| {
            let on_plane = point + (right * sight.x + up * sight.y) * units;
            let (origin, direction) = if perspective {
                (camera.position, on_plane - camera.position)
            } else {
                (on_plane - forward * depth, forward)
            };
            let (from, toward) = (
                to_local.transform_point3(origin),
                to_local.transform_vector3(direction),
            );
            // The part of the line of sight within each pair of the box's
            // faces, from the camera on.
            let (mut enter, mut exit) = (0.0_f64, f64::INFINITY);
            for axis in 0..3 {
                if toward[axis].abs() < 1e-12 {
                    if from[axis] < low[axis] || from[axis] > high[axis] {
                        return None;
                    }
                } else {
                    let near = (low[axis] - from[axis]) / toward[axis];
                    let far = (high[axis] - from[axis]) / toward[axis];
                    enter = enter.max(near.min(far));
                    exit = exit.min(near.max(far));
                }
            }
            let leave = exit - 0.01 * (exit - enter);
            (enter <= exit).then(|| (origin + direction * leave - camera.position).dot(forward))
        })
        .filter(|depth| depth.is_finite())
        .reduce(f64::min)
}

/// Where the plane of a billboard drawn in the 3D pass lies, and how many
/// world units a canvas unit of it spans: it faces the camera at `anchor`,
/// pulled toward the camera by `pull` canvas units, half its larger side, so
/// that a surface it rests on does not cut it, and at least as near as the
/// depth `front` along the view when given; and it covers on screen what it
/// would on the canvas, drawn by the 2D renderer. Under perspective the
/// canvas is the frame at every depth, so a canvas unit spans more of the
/// world farther away; pulled along the line of sight, the label stays where
/// it shows. `None` behind the camera.
fn billboard_plane(
    camera: &gaanim_math::Camera,
    anchor: gaanim_core::glam::DVec3,
    pull: f64,
    front: Option<f64>,
) -> Option<(gaanim_core::glam::DVec3, f64)> {
    use gaanim_core::glam::DVec3;
    let forward = camera.rotation * DVec3::NEG_Z;
    let depth = (anchor - camera.position).dot(forward);
    // How much nearer `front` lies than the anchor.
    let to_front = front.map_or(0.0, |front| (depth - front).max(0.0));
    match camera.projection {
        gaanim_math::Projection::Orthographic { .. } => {
            Some((anchor - forward * pull.max(to_front), 1.0))
        }
        gaanim_math::Projection::Perspective { fov_y, near, .. } => {
            // World units a canvas unit spans at `depth` along the view.
            let per_unit = 2.0 * (fov_y * 0.5).tan() / camera.frame_height.max(1e-9);
            if !(depth > near) || !per_unit.is_finite() {
                return None;
            }
            // At most halfway to the near plane, so the label stays in view.
            let pull = (pull * per_unit * depth)
                .max(to_front)
                .min(0.5 * (depth - near));
            let toward = (camera.position - anchor).normalize_or_zero();
            let point = anchor + toward * pull / toward.dot(-forward).max(1e-9);
            let depth = (point - camera.position).dot(forward);
            Some((point, per_unit * depth))
        }
    }
}

/// How far an axis label's text moves on the canvas (scene units, y up)
/// from its anchor: `gap.gap` clear of its tick, on the side of the axis
/// outside the box, or past the axis's end for a title. `axis` is the
/// projected direction of the axis; `tick` and `box_corners` are projected
/// relative to the anchor, and `corners` are the text's box as drawn around
/// its origin.
///
/// A number goes to the side of the axis the whole box leaves free on
/// screen. Where the box lies on both sides, as around an axis between two
/// grids, it goes to its tick's side, as authored; and where the tick points
/// at the camera, away from the box's middle.
fn axis_label_shift(
    gap: &AxisLabelGap,
    axis: gaanim_core::glam::DVec2,
    tick: gaanim_core::glam::DVec2,
    box_corners: &[gaanim_core::glam::DVec2],
    corners: &[gaanim_core::glam::DVec2],
) -> gaanim_core::glam::DVec2 {
    use gaanim_core::glam::DVec2;
    // Shorter than this, a projected direction counts as none: the camera
    // looks along it.
    const EDGE_ON: f64 = 1e-4;
    let direction = |v: DVec2| (v.is_finite() && v.length() > EDGE_ON).then(|| v.normalize());
    // How far the text reaches back toward the anchor along `n`.
    let behind = |n: DVec2| {
        corners
            .iter()
            .map(|corner| corner.dot(n))
            .reduce(f64::min)
            .unwrap_or(0.0)
    };
    let center = box_corners.iter().copied().sum::<DVec2>() / box_corners.len().max(1) as f64;
    let away = direction(-center);
    if let Some(numbers) = gap.title
        && !gap.beside
    {
        let n = direction(axis).or(away).unwrap_or(DVec2::Y);
        let numbers = n.x.abs() * numbers.x + n.y.abs() * numbers.y;
        return n * (gap.gap + numbers - behind(n));
    }
    let n = match direction(axis) {
        Some(along) => {
            let across = along.perp();
            let (lowest, highest) = box_corners
                .iter()
                .map(|corner| corner.dot(across))
                .fold((f64::INFINITY, f64::NEG_INFINITY), |(low, high), side| {
                    (low.min(side), high.max(side))
                });
            if lowest > -EDGE_ON {
                -across
            } else if highest < EDGE_ON {
                across
            } else if across.dot(tick).abs() > EDGE_ON {
                across * across.dot(tick).signum()
            } else if across.dot(center) > 0.0 {
                -across
            } else {
                across
            }
        }
        None => away.or(direction(tick)).unwrap_or(DVec2::NEG_Y),
    };
    // A title beside its axis clears its numbers' whole width along `n`
    // and a second gap.
    let numbers = gap.title.map_or(0.0, |numbers| {
        2.0 * (n.x.abs() * numbers.x + n.y.abs() * numbers.y) + gap.gap
    });
    n * (tick.dot(n).abs() + gap.gap + numbers - behind(n))
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
                let value =
                    GlobalSpatialTransform::from_parent_and_local(parent_global, child_local);
                if *child_global != value {
                    *child_global = value;
                }
                let current_child_global = value;
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
    fn parallax_layers_move_by_their_share_of_the_camera_motion() {
        use bevy::prelude::IntoScheduleConfigs;
        use gaanim_core::glam::DVec3;
        use gaanim_core::kurbo::Point;
        let authored = gaanim_math::Camera::ortho_2d_frame(16.0, 9.0, 1600, 900);
        let mut moved = authored;
        moved.position = DVec3::new(4.0, -2.0, 0.0);

        let mut world = World::new();
        world.insert_resource(gaanim_math::ResolvedCamera::new(
            moved,
            gaanim_math::CameraViewport::default(),
        ));
        let spawn_layer = |world: &mut World, depth: f64| {
            let layer = world
                .spawn((
                    SpatialTransform::default(),
                    GlobalSpatialTransform::default(),
                    crate::components::ParallaxLayer { depth },
                ))
                .id();
            let child = world
                .spawn((
                    SpatialTransform::new_2d(1.0, 1.0),
                    GlobalSpatialTransform::default(),
                    ChildOf(layer),
                ))
                .id();
            (layer, child)
        };
        let (far_layer, far) = spawn_layer(&mut world, 2.0);
        // A HUD inside a layer stays on screen.
        let hud = world
            .spawn((
                SpatialTransform::new_2d(1.0, 1.0),
                GlobalSpatialTransform::default(),
                crate::components::HudOverlay,
                ChildOf(far_layer),
            ))
            .id();
        let (_, near) = spawn_layer(&mut world, 0.5);
        let (plane_layer, plane) = spawn_layer(&mut world, 1.0);
        let (sky_layer, sky) = spawn_layer(&mut world, f64::INFINITY);
        // A layer inside another follows the outer one only.
        world.entity_mut(sky_layer).insert(ChildOf(plane_layer));

        let mut schedule = Schedule::default();
        schedule.add_systems(
            (
                transform_propagation_system.run_if(has_transform_changes),
                pin_parallax_layers_system,
                pin_hud_overlays_system,
            )
                .chain(),
        );
        let pixel = |world: &World, camera: &gaanim_math::Camera, entity: Entity| {
            let origin = world
                .get::<GlobalSpatialTransform>(entity)
                .unwrap()
                .affine_2d
                * Point::ORIGIN;
            camera.to_vello_transform() * origin
        };
        let at_rest = authored.to_vello_transform() * Point::new(1.0, 1.0);
        let full_shift = moved.to_vello_transform() * Point::new(1.0, 1.0) - at_rest;
        for _ in 0..2 {
            schedule.run(&mut world);
            for (entity, share) in [(far, 0.5), (near, 2.0), (plane, 1.0)] {
                let shift = pixel(&world, &moved, entity) - at_rest;
                assert!(
                    (shift - full_shift * share).hypot() < 1e-6,
                    "{shift:?} != {:?}",
                    full_shift * share
                );
            }
            // Nested in the depth-1 layer, which never moves it.
            assert!((pixel(&world, &moved, sky) - at_rest - full_shift).hypot() < 1e-6);
            assert!((pixel(&world, &moved, hud) - at_rest).hypot() < 1e-6);
        }

        // Zoom scales a layer by `zoom^(1/depth)` about the camera center.
        let mut zoomed = authored;
        zoomed.projection = gaanim_math::Projection::Orthographic { zoom: 4.0 };
        let pin = parallax_pin(&zoomed, 2.0).unwrap();
        let shown = zoomed.to_vello_transform() * pin * Point::new(1.0, 0.0);
        let center = zoomed.to_vello_transform() * Point::ORIGIN;
        let unit = authored.to_vello_transform() * Point::new(1.0, 0.0) - center;
        assert!(((shown - center) - unit * 2.0).hypot() < 1e-6);
        assert_eq!(parallax_pin(&zoomed, 1.0), None);
        assert_eq!(parallax_pin(&zoomed, 0.0), None);

        // Resetting the camera returns every layer to its authored place.
        world.insert_resource(gaanim_math::ResolvedCamera::new(
            authored,
            gaanim_math::CameraViewport::default(),
        ));
        schedule.run(&mut world);
        for entity in [far, near, plane] {
            assert!((pixel(&world, &authored, entity) - at_rest).hypot() < 1e-6);
        }
    }

    #[test]
    fn orthographic_billboards_stay_upright_while_the_camera_turns() {
        let mut camera = gaanim_math::Camera::ortho_2d_frame(16.0, 9.0, 1600, 900);
        camera.rotation = gaanim_core::glam::DQuat::from_rotation_z(0.7);
        let mut world = World::new();
        world.insert_resource(gaanim_math::ResolvedCamera::new(
            camera,
            gaanim_math::CameraViewport::default(),
        ));
        let local = SpatialTransform::new_2d(2.0, 1.0);
        let label = world
            .spawn((
                local,
                GlobalSpatialTransform::from_local(&local),
                crate::components::Billboard,
            ))
            .id();
        let mut schedule = Schedule::default();
        schedule.add_systems(billboard_system);
        schedule.run(&mut world);

        let global = world
            .get::<GlobalSpatialTransform>(label)
            .unwrap()
            .affine_2d;
        let [a, b, c, d, _, _] = (camera.to_vello_transform() * global).as_coeffs();
        assert!(
            b.abs() < 1e-9 && c.abs() < 1e-9,
            "turned on screen: {b} {c}"
        );
        assert!(a > 0.0 && d < 0.0);
        let at = global * gaanim_core::kurbo::Point::ORIGIN;
        assert!((at - gaanim_core::kurbo::Point::new(2.0, 1.0)).hypot() < 1e-9);
    }

    /// A billboard of the side a transition takes away faces the camera
    /// that side keeps: through the current camera and the side's view, it
    /// shows where that camera shows it. One of the incoming side faces the
    /// current camera.
    #[test]
    fn a_billboard_leaving_through_a_transition_faces_the_camera_its_side_keeps() {
        use gaanim_core::kurbo::Point;
        let current = gaanim_math::Camera::ortho_2d_frame(16.0, 9.0, 1600, 900);
        let kept = axis_camera(gaanim_core::glam::DVec3::new(6.0, 4.0, 10.0));
        let point = gaanim_core::glam::DVec3::new(1.0, 0.5, -0.5);
        let mut world = World::new();
        world.insert_resource(gaanim_math::ResolvedCamera::new(
            current,
            gaanim_math::CameraViewport::default(),
        ));
        let local = SpatialTransform::new_3d(point.x, point.y, point.z);
        let mut label = || {
            world
                .spawn((
                    local,
                    GlobalSpatialTransform::from_local(&local),
                    crate::components::Billboard,
                ))
                .id()
        };
        let (leaving, entering) = (label(), label());
        let mut frame = crate::SceneTransitionFrame::default();
        frame.outgoing.insert(leaving);
        frame.incoming.insert(entering);
        frame.set_outgoing_camera(Some(kept), &current);
        let view = frame.view_for(crate::TransitionSide::Outgoing).unwrap();
        world.insert_resource(frame);
        let mut schedule = Schedule::default();
        schedule.add_systems(billboard_system);
        schedule.run(&mut world);

        let on_screen = |entity: Entity, view: gaanim_core::kurbo::Affine| {
            let global = world.get::<GlobalSpatialTransform>(entity).unwrap();
            current.canvas_transform() * view * global.affine_2d * Point::ORIGIN
        };
        let expected = kept.world_to_screen(point);
        let shown = on_screen(leaving, view);
        assert!(
            (shown - Point::new(expected.x, expected.y)).hypot() < 1e-6,
            "{shown:?} {expected:?}"
        );
        let expected = current.world_to_screen(point);
        let shown = on_screen(entering, gaanim_core::kurbo::Affine::IDENTITY);
        assert!(
            (shown - Point::new(expected.x, expected.y)).hypot() < 1e-6,
            "{shown:?} {expected:?}"
        );
    }

    /// A perspective camera on a 16 by 9 frame looking from `eye` at the
    /// origin, and the canvas point it draws `point` at.
    fn axis_camera(eye: gaanim_core::glam::DVec3) -> gaanim_math::Camera {
        let mut camera = gaanim_math::Camera::ortho_2d_frame(16.0, 9.0, 1600, 900);
        camera.projection = gaanim_math::Projection::Perspective {
            fov_y: 0.8,
            near: 0.1,
            far: 1000.0,
        };
        camera
            .look_at(
                eye,
                gaanim_core::glam::DVec3::ZERO,
                gaanim_core::glam::DVec3::Y,
            )
            .unwrap();
        camera
    }

    fn canvas_point(
        camera: &gaanim_math::Camera,
        point: gaanim_core::glam::DVec3,
    ) -> gaanim_core::glam::DVec2 {
        let screen = camera.world_to_screen(point);
        let ppu = camera.pixels_per_unit();
        gaanim_core::glam::DVec2::new(
            (screen.x - camera.viewport_width as f64 * 0.5) / ppu,
            (camera.viewport_height as f64 * 0.5 - screen.y) / ppu,
        )
    }

    /// Where `billboard_system` draws an axis label anchored at `anchor`
    /// under `camera`: its text's origin and its 0.4 by 0.3 box's corners on
    /// the canvas, and the canvas point its world position falls on.
    fn place_axis_label(
        camera: gaanim_math::Camera,
        anchor: gaanim_core::glam::DVec3,
        gap: AxisLabelGap,
    ) -> (
        gaanim_core::glam::DVec2,
        Vec<gaanim_core::glam::DVec2>,
        gaanim_core::glam::DVec2,
    ) {
        use gaanim_core::glam::{DVec2, DVec3};
        let mut world = World::new();
        world.insert_resource(gaanim_math::ResolvedCamera::new(
            camera,
            gaanim_math::CameraViewport::default(),
        ));
        let axes = world
            .spawn((
                SpatialTransform::default(),
                GlobalSpatialTransform::default(),
            ))
            .id();
        let mut local = SpatialTransform::default();
        local.translation = anchor;
        let bounds = gaanim_math::Bounds3D {
            min: DVec3::new(-0.2, -0.15, 0.0),
            max: DVec3::new(0.2, 0.15, 0.0),
        };
        let label = world
            .spawn((
                local,
                GlobalSpatialTransform::from_local(&local),
                crate::components::Billboard,
                gap,
                LocalBounds(bounds),
                ChildOf(axes),
            ))
            .id();
        let mut schedule = Schedule::default();
        schedule.add_systems(billboard_system);
        // A second run starts from the first's result, as later frames do.
        schedule.run(&mut world);
        schedule.run(&mut world);
        let global = world.get::<GlobalSpatialTransform>(label).unwrap();
        let at = |point: gaanim_core::kurbo::Point| DVec2::new(point.x, point.y);
        let origin = at(global.affine_2d * gaanim_core::kurbo::Point::ORIGIN);
        let corners = [(-0.2, -0.15), (0.2, -0.15), (0.2, 0.15), (-0.2, 0.15)]
            .map(|(x, y)| at(global.affine_2d * gaanim_core::kurbo::Point::new(x, y)))
            .to_vec();
        let placed = canvas_point(&camera, global.mat4.transform_point3(DVec3::ZERO));
        (origin, corners, placed)
    }

    /// How far the nearest corner lies past the line through `from` across `n`.
    fn clearance(
        corners: &[gaanim_core::glam::DVec2],
        from: gaanim_core::glam::DVec2,
        n: gaanim_core::glam::DVec2,
    ) -> f64 {
        corners
            .iter()
            .map(|corner| (*corner - from).dot(n))
            .fold(f64::INFINITY, f64::min)
    }

    #[test]
    fn axis_numbers_take_the_side_the_box_leaves_free() {
        use gaanim_core::glam::{DVec2, DVec3};
        let gap = AxisLabelGap {
            axis: DVec3::X,
            tick: DVec3::NEG_Y,
            bounds: [DVec3::ZERO, DVec3::ONE],
            gap: 0.1,
            title: None,
            beside: false,
            follows: None,
        };
        let square = |low: f64, high: f64| {
            vec![
                DVec2::new(-1.0, low),
                DVec2::new(1.0, low),
                DVec2::new(1.0, high),
                DVec2::new(-1.0, high),
            ]
        };
        let side = |tick: DVec2, box_corners: &[DVec2]| {
            axis_label_shift(&gap, DVec2::X, tick, box_corners, &[]).y
        };
        // The box above the axis: below, whichever way the tick points.
        assert!(side(DVec2::new(0.0, 0.05), &square(0.0, 2.0)) < 0.0);
        // Below it: above.
        assert!(side(DVec2::new(0.0, -0.05), &square(-2.0, 0.0)) > 0.0);
        // On both sides, as around an axis between two grids: the tick's.
        assert!(side(DVec2::new(0.0, 0.05), &square(-1.0, 2.0)) > 0.0);
        assert!(side(DVec2::new(0.0, -0.05), &square(-1.0, 2.0)) < 0.0);
        // A tick pointing at the camera: away from the box's middle.
        assert!(side(DVec2::ZERO, &square(-1.0, 2.0)) < 0.0);
    }

    #[test]
    fn axis_numbers_keep_their_gap_on_screen_at_any_distance() {
        use gaanim_core::glam::DVec3;
        let anchor = DVec3::new(1.0, -1.0, -1.0);
        let gap = AxisLabelGap {
            axis: DVec3::X,
            tick: DVec3::new(0.0, -0.05, 0.0),
            bounds: [DVec3::splat(-1.0), DVec3::splat(1.0)],
            gap: 0.12,
            title: None,
            beside: false,
            follows: None,
        };
        for eye in [DVec3::new(1.0, 2.0, 6.0), DVec3::new(10.0, 20.0, 60.0)] {
            let camera = axis_camera(eye);
            let (origin, corners, placed) = place_axis_label(camera, anchor, gap);
            let tick = canvas_point(&camera, anchor);
            let along = (canvas_point(&camera, anchor + DVec3::X * 0.01) - tick).normalize();
            let across = -along.perp();
            // Beside the axis, below it on screen, away from the box.
            assert!(across.y < 0.0, "{across:?}");
            let clear = clearance(&corners, tick, across);
            let tick_reach = (canvas_point(&camera, anchor + gap.tick) - tick)
                .dot(across)
                .abs();
            assert!(
                (clear - tick_reach - 0.12).abs() < 1e-6,
                "{eye:?}: {clear} past the axis"
            );
            // Its world position is drawn where its text is, for occlusion.
            assert!((placed - origin).length() < 1e-6, "{placed:?} {origin:?}");
        }
    }

    #[test]
    fn an_axis_seen_end_on_puts_its_numbers_away_from_the_box() {
        use gaanim_core::glam::DVec3;
        // The camera looks down the z axis, which runs along x = y = -1.
        let anchor = DVec3::new(-1.0, -1.0, 0.5);
        let mut camera = axis_camera(DVec3::new(0.0, 0.0, 8.0));
        camera
            .look_at(
                DVec3::new(-1.0, -1.0, 8.0),
                DVec3::new(-1.0, -1.0, 0.0),
                DVec3::Y,
            )
            .unwrap();
        let gap = AxisLabelGap {
            axis: DVec3::Z,
            tick: DVec3::new(-0.05, 0.0, 0.0),
            bounds: [DVec3::splat(-1.0), DVec3::splat(1.0)],
            gap: 0.12,
            title: None,
            beside: false,
            follows: None,
        };
        let (origin, corners, _) = place_axis_label(camera, anchor, gap);
        let tick = canvas_point(&camera, anchor);
        let box_center = canvas_point(&camera, DVec3::ZERO);
        let away = (tick - box_center).normalize();
        assert!((origin - tick).normalize().dot(away) > 0.99);
        assert!(clearance(&corners, tick, away) > 0.12);
    }

    #[test]
    fn axis_titles_sit_past_the_end_and_its_numbers() {
        use gaanim_core::glam::{DVec2, DVec3};
        let end = DVec3::new(2.0, -1.0, -1.0);
        let camera = axis_camera(DVec3::new(10.0, 20.0, 60.0));
        let gap = AxisLabelGap {
            axis: DVec3::X,
            tick: DVec3::ZERO,
            bounds: [DVec3::splat(-1.0), DVec3::splat(1.0)],
            gap: 0.12,
            title: Some(DVec2::new(0.25, 0.1)),
            beside: false,
            follows: None,
        };
        let (_, corners, _) = place_axis_label(camera, end, gap);
        let at = canvas_point(&camera, end);
        let along = (canvas_point(&camera, end + DVec3::X * 0.01) - at).normalize();
        let numbers = along.x.abs() * 0.25 + along.y.abs() * 0.1;
        assert!((clearance(&corners, at, along) - 0.12 - numbers).abs() < 1e-6);
    }

    #[test]
    fn axis_titles_beside_their_axis_clear_its_numbers() {
        use gaanim_core::glam::{DVec2, DVec3};
        let middle = DVec3::new(0.0, -1.0, -1.0);
        let camera = axis_camera(DVec3::new(10.0, 20.0, 60.0));
        let gap = AxisLabelGap {
            axis: DVec3::X,
            tick: DVec3::new(0.0, -0.05, 0.0),
            bounds: [DVec3::splat(-1.0), DVec3::splat(1.0)],
            gap: 0.12,
            title: Some(DVec2::new(0.25, 0.1)),
            beside: true,
            follows: None,
        };
        let (_, corners, _) = place_axis_label(camera, middle, gap);
        let at = canvas_point(&camera, middle);
        let along = (canvas_point(&camera, middle + DVec3::X * 0.01) - at).normalize();
        let across = -along.perp();
        // On the numbers' side of the axis, below it, past their width.
        assert!(across.y < 0.0, "{across:?}");
        let tick_reach = (canvas_point(&camera, middle + gap.tick) - at)
            .dot(across)
            .abs();
        let numbers = 2.0 * (across.x.abs() * 0.25 + across.y.abs() * 0.1);
        let clear = clearance(&corners, at, across);
        assert!(
            (clear - tick_reach - 0.24 - numbers).abs() < 1e-6,
            "{clear} past the axis"
        );
    }

    #[test]
    fn billboards_drawn_in_3d_face_the_camera_as_large_as_on_the_canvas() {
        use gaanim_core::glam::{DVec3, DVec4};
        let eye = DVec3::new(3.0, 2.0, 9.0);
        let camera = axis_camera(eye);
        let mut world = World::new();
        world.insert_resource(gaanim_math::ResolvedCamera::new(
            camera,
            gaanim_math::CameraViewport::default(),
        ));
        let anchor = DVec3::new(1.0, 0.5, -2.0);
        let local = SpatialTransform::new_3d(anchor.x, anchor.y, anchor.z);
        let label = world
            .spawn((
                local,
                GlobalSpatialTransform::from_local(&local),
                crate::components::Billboard,
                crate::components::BillboardPlane::default(),
                // A 0.8 by 0.4 label: pulled 0.4 canvas units toward the eye.
                LocalBounds(gaanim_math::Bounds3D::new_2d(-0.4, -0.2, 0.4, 0.2)),
            ))
            .id();
        let mut schedule = Schedule::default();
        schedule.add_systems(billboard_system);
        schedule.run(&mut world);

        let plane = world
            .get::<crate::components::BillboardPlane>(label)
            .unwrap()
            .0
            .mat4;
        let at = |x: f64, y: f64| (plane * DVec4::new(x, y, 0.0, 1.0)).truncate();
        // Drawn where the anchor is, a unit of it a canvas unit, upright.
        let origin = canvas_point(&camera, at(0.0, 0.0));
        assert!((origin - canvas_point(&camera, anchor)).length() < 1e-9);
        let across = canvas_point(&camera, at(1.0, 0.0)) - origin;
        let up = canvas_point(&camera, at(0.0, 1.0)) - origin;
        assert!(
            (across - gaanim_core::glam::DVec2::X).length() < 1e-9,
            "{across}"
        );
        assert!((up - gaanim_core::glam::DVec2::Y).length() < 1e-9, "{up}");
        // Nearer by half its larger side, at the anchor's scale.
        let forward = camera.rotation * DVec3::NEG_Z;
        let depth = |point: DVec3| (point - eye).dot(forward);
        let per_unit = 2.0 * 0.4_f64.tan() / 9.0;
        let pulled = depth(anchor) - depth(at(0.0, 0.0));
        assert!(
            (pulled - 0.4 * per_unit * depth(anchor)).abs() < 1e-9,
            "{pulled}"
        );
        // Its own transform keeps the label's scale, which bounds measure.
        let global = world.get::<GlobalSpatialTransform>(label).unwrap().mat4;
        assert!((global.x_axis.truncate().length() - 1.0).abs() < 1e-12);
        assert!((global.w_axis.truncate() - anchor).length() < 1e-12);
    }

    #[test]
    fn axis_labels_come_just_before_where_their_sight_leaves_their_box() {
        use gaanim_core::glam::{DMat4, DVec2, DVec3};
        let camera = axis_camera(DVec3::new(0.0, 0.0, 10.0));
        let unit_box = [DVec3::splat(-1.0), DVec3::splat(1.0)];
        let exit = |point: DVec3| {
            box_exit_depth(&camera, DMat4::IDENTITY, unit_box, point, &[DVec2::ZERO])
        };
        // Seen through the box, a label behind it comes just before its far
        // face, behind what the box holds: a hundredth of the 2 units across.
        let through = exit(DVec3::new(0.0, 0.0, -3.0)).unwrap();
        assert!((through - 10.98).abs() < 1e-9, "{through}");
        // Seen beside it, the box does not move it.
        assert_eq!(exit(DVec3::new(4.0, 0.0, -3.0)), None);
        // A box in a scaled space, seen through its side.
        let scaled = DMat4::from_scale(DVec3::splat(2.0));
        let side = box_exit_depth(
            &camera,
            scaled,
            unit_box,
            DVec3::new(0.0, 0.0, -3.0),
            &[DVec2::ZERO],
        )
        .unwrap();
        assert!((side - 11.96).abs() < 1e-9, "{side}");
    }

    #[test]
    fn perspective_billboards_land_where_their_anchor_is_drawn() {
        // A perspective camera showing the same 16 by 9 frame on the z = 0
        // plane as the orthographic one: canvas points there are world points.
        let mut camera = gaanim_math::Camera::ortho_2d_frame(16.0, 9.0, 1600, 900);
        let pose = camera.pose().perspective_equivalent(0.8, 0.1, 100.0, 9.0);
        camera.apply_pose(pose).unwrap();
        let mut world = World::new();
        world.insert_resource(gaanim_math::ResolvedCamera::new(
            camera,
            gaanim_math::CameraViewport {
                scale: 0.5,
                offset_x: 40.0,
                offset_y: 30.0,
            },
        ));
        let local = SpatialTransform::new_2d(2.0, 1.0);
        let label = world
            .spawn((
                local,
                GlobalSpatialTransform::from_local(&local),
                crate::components::Billboard,
            ))
            .id();
        let mut schedule = Schedule::default();
        schedule.add_systems(billboard_system);
        schedule.run(&mut world);

        let global = world
            .get::<GlobalSpatialTransform>(label)
            .unwrap()
            .affine_2d;
        let at = global * gaanim_core::kurbo::Point::ORIGIN;
        assert!(
            (at - gaanim_core::kurbo::Point::new(2.0, 1.0)).hypot() < 1e-6,
            "drawn at {at:?}"
        );
    }

    /// A shifted billboard puts its box's anchor on its point and moves it by
    /// its offset on the canvas, whatever the point's depth, in the 2D
    /// drawing and in the plane the 3D pass draws.
    #[test]
    fn shifted_billboards_keep_their_offset_on_screen_at_every_depth() {
        use gaanim_core::glam::{DVec2, DVec3};
        let camera = axis_camera(DVec3::new(0.0, 0.0, 12.0));
        for depth in [0.0, -20.0] {
            let mut world = World::new();
            world.insert_resource(gaanim_math::ResolvedCamera::new(
                camera,
                gaanim_math::CameraViewport::default(),
            ));
            let mut local = SpatialTransform::default();
            local.translation = DVec3::new(1.0, 0.5, depth);
            let label = world
                .spawn((
                    local,
                    GlobalSpatialTransform::from_local(&local),
                    LocalBounds(gaanim_math::Bounds3D {
                        min: DVec3::new(-0.5, -0.1, 0.0),
                        max: DVec3::new(0.5, 0.3, 0.0),
                    }),
                    crate::components::Billboard,
                    crate::components::BillboardPlane::default(),
                    crate::components::BillboardShift {
                        offset: DVec2::new(0.0, 0.4),
                        anchor: Some(DVec2::new(0.0, -1.0)),
                    },
                ))
                .id();
            let mut schedule = Schedule::default();
            schedule.add_systems(billboard_system);
            schedule.run(&mut world);

            let point = canvas_point(&camera, local.translation);
            let global = *world.get::<GlobalSpatialTransform>(label).unwrap();
            let bottom = global.affine_2d * gaanim_core::kurbo::Point::new(0.0, -0.1);
            let expected = point + DVec2::new(0.0, 0.4);
            assert!(
                (DVec2::new(bottom.x, bottom.y) - expected).length() < 1e-6,
                "depth {depth}: bottom drawn at {bottom:?}, expected {expected:?}"
            );
            // The 3D pass draws the same point there.
            let plane = world
                .get::<crate::components::BillboardPlane>(label)
                .unwrap()
                .0;
            let drawn = canvas_point(
                &camera,
                plane.mat4.transform_point3(DVec3::new(0.0, -0.1, 0.0)),
            );
            assert!(
                (drawn - expected).length() < 1e-6,
                "depth {depth}: plane draws it at {drawn:?}, expected {expected:?}"
            );
        }
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
                    revealed: None,
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
