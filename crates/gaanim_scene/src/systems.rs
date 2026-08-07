use crate::components::{
    Billboard, FillBrush, GlobalOpacity, GroupMarker, LineListData, LocalBounds, Mesh3DMarker,
    Opacity, StrokeBrush, TriangleMeshData, WorldBounds,
};
use bevy::prelude::{
    Added, Changed, ChildOf, Children, Entity, Local, Or, ParamSet, Query, Res, ResMut, With,
    Without,
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
/// Descendants are updated recursively in parent-before-child order. This prevents
/// grandchildren (for example text glyphs inside a grouped text object) from using
/// their parent's transform from the previous frame.
pub fn transform_propagation_system(
    roots: Query<Entity, (Without<ChildOf>, With<SpatialTransform>)>,
    children_query: Query<&Children>,
    mut transforms: Query<(&SpatialTransform, &mut GlobalSpatialTransform)>,
) {
    for root in &roots {
        propagate_transforms_recursive(root, None, &children_query, &mut transforms);
    }
}

fn propagate_transforms_recursive(
    entity: Entity,
    parent_global: Option<GlobalSpatialTransform>,
    children_query: &Query<&Children>,
    transforms: &mut Query<(&SpatialTransform, &mut GlobalSpatialTransform)>,
) {
    let Ok((local, mut global)) = transforms.get_mut(entity) else {
        return;
    };
    *global = parent_global
        .as_ref()
        .map(|parent| GlobalSpatialTransform::from_parent_and_local(parent, local))
        .unwrap_or_else(|| GlobalSpatialTransform::from_local(local));
    let current_global = *global;
    drop(global);

    if let Ok(children) = children_query.get(entity) {
        for child in children.iter() {
            propagate_transforms_recursive(
                *child,
                Some(current_global),
                children_query,
                transforms,
            );
        }
    }
}

/// Run condition: skip opacity propagation when no local opacity has changed.
pub fn has_opacity_changes(query: Query<&Opacity, Or<(Changed<Opacity>, Added<Opacity>)>>) -> bool {
    !query.is_empty()
}

/// System: Propagate opacity cascade down the hierarchy using Bevy 0.18's `ChildOf` relation.
pub fn opacity_propagation_system(
    roots: Query<(Entity, Option<&ChildOf>), With<Opacity>>,
    children_query: Query<&Children>,
    local_opacities: Query<&Opacity>,
    parents: Query<&ChildOf>,
    mut opacities: Query<(&Opacity, &mut GlobalOpacity)>,
) {
    for (root, parent) in &roots {
        // Text and imported assets can contain structural grouping entities
        // without an Opacity component. Such an entity must not cut the
        // cascade: an opacity-bearing child below it is a propagation root.
        if parent
            .is_none_or(|parent| !has_opacity_ancestor(parent.parent(), &parents, &local_opacities))
        {
            propagate_opacities_recursive(root, 1.0, &children_query, &mut opacities);
        }
    }
}

fn has_opacity_ancestor(
    mut entity: Entity,
    parents: &Query<&ChildOf>,
    opacities: &Query<&Opacity>,
) -> bool {
    loop {
        if opacities.get(entity).is_ok() {
            return true;
        }
        let Ok(parent) = parents.get(entity) else {
            return false;
        };
        entity = parent.parent();
    }
}

fn propagate_opacities_recursive(
    entity: Entity,
    parent_opacity: f32,
    children_query: &Query<&Children>,
    opacities: &mut Query<(&Opacity, &mut GlobalOpacity)>,
) {
    let current_opacity = if let Ok((local, mut global)) = opacities.get_mut(entity) {
        global.0 = local.0 * parent_opacity;
        global.0
    } else {
        parent_opacity
    };

    if let Ok(children) = children_query.get(entity) {
        for child in children.iter() {
            propagate_opacities_recursive(*child, current_opacity, children_query, opacities);
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

/// System: Sync `GlobalSpatialTransform::mat4` to Bevy `Transform` for 3D mesh entities.
pub fn sync_3d_mesh_transform_system(
    mut query: Query<(&GlobalSpatialTransform, &mut bevy::prelude::Transform), With<crate::components::Mesh3DMarker>>,
) {
    for (global, mut transform) in &mut query {
        let (scale, rot, trans) = global.mat4.to_scale_rotation_translation();
        transform.translation = bevy::prelude::Vec3::new(trans.x as f32, trans.y as f32, trans.z as f32);
        transform.rotation = bevy::prelude::Quat::from_xyzw(
            rot.x as f32, rot.y as f32, rot.z as f32, rot.w as f32,
        );
        transform.scale = bevy::prelude::Vec3::new(scale.x as f32, scale.y as f32, scale.z as f32);
    }
}

/// System: Billboard - make entities face the camera (for 3D labels).
pub fn billboard_system(
    camera: Option<bevy::prelude::Res<gaanim_math::Camera>>,
    mut query: Query<(&mut GlobalSpatialTransform, Option<&mut bevy::prelude::Transform>), With<crate::components::Billboard>>,
) {
    let Some(cam) = camera else { return };
    let cam_rot = cam.rotation;
    for (mut global, transform_opt) in &mut query {
        // Preserve world position and scale, replace rotation with camera rotation.
        let world = global.mat4;
        let (scale, _rot, trans) = world.to_scale_rotation_translation();
        let billboard_mat = gaanim_core::glam::DMat4::from_scale_rotation_translation(scale, cam_rot, trans);
        global.mat4 = billboard_mat;
        // Also update Affine for Vello fallback (use world pos XY and camera Z angle)
        let z_angle = cam.z_angle();
        global.affine_2d = gaanim_core::kurbo::Affine::translate((trans.x, trans.y))
            * gaanim_core::kurbo::Affine::rotate(-z_angle)
            * gaanim_core::kurbo::Affine::scale_non_uniform(scale.x, scale.y);
        if let Some(mut t) = transform_opt {
            let (scale_d, _, trans_d) = billboard_mat.to_scale_rotation_translation();
            t.translation = bevy::prelude::Vec3::new(trans_d.x as f32, trans_d.y as f32, trans_d.z as f32);
            t.rotation = bevy::prelude::Quat::from_xyzw(
                cam_rot.x as f32,
                cam_rot.y as f32,
                cam_rot.z as f32,
                cam_rot.w as f32,
            );
            t.scale = bevy::prelude::Vec3::new(scale_d.x as f32, scale_d.y as f32, scale_d.z as f32);
        }
    }
}

/// System: Build Bevy Meshes from raw Triangle/Line data components.
///
/// Converts `TriangleMeshData` and `LineListData` into `Mesh3d` + `MeshMaterial3d` + `Transform`.
pub fn build_3d_meshes_system(
    mut commands: bevy::prelude::Commands,
    mut meshes: ResMut<bevy::prelude::Assets<bevy::mesh::Mesh>>,
    mut materials: ResMut<bevy::prelude::Assets<bevy::pbr::StandardMaterial>>,
    query_tri: Query<(Entity, &TriangleMeshData), (Without<bevy::prelude::Mesh3d>, Without<LineListData>)>,
    query_line: Query<(Entity, &LineListData), (Without<bevy::prelude::Mesh3d>, Without<TriangleMeshData>)>,
) {
    use bevy::mesh::{Indices, Mesh, PrimitiveTopology};
    use bevy::asset::RenderAssetUsages;

    for (entity, data) in &query_tri {
        let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
        let positions: Vec<[f32; 3]> = data.vertices.clone();
        let indices = data.indices.clone();
        // Compute simple normals (up) if not provided
        let normals: Vec<[f32; 3]> = positions.iter().map(|_| [0.0, 1.0, 0.0]).collect();
        mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
        mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
        mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, vec![[0.0, 0.0]; mesh.count_vertices()]);
        mesh.insert_indices(Indices::U32(indices));
        let mesh_handle = meshes.add(mesh);
        let color = data.color.map(|c| {
            let rgba = c.to_rgba8();
            bevy::color::Color::srgba_u8(rgba.r, rgba.g, rgba.b, rgba.a)
        }).unwrap_or(bevy::color::Color::WHITE);
        let mat = materials.add(bevy::pbr::StandardMaterial {
            base_color: color,
            double_sided: true,
            cull_mode: None,
            ..Default::default()
        });
        commands.entity(entity).insert((
            bevy::prelude::Mesh3d(mesh_handle),
            bevy::prelude::MeshMaterial3d::<bevy::pbr::StandardMaterial>(mat),
            bevy::prelude::Transform::default(),
            bevy::prelude::Visibility::default(),
            Mesh3DMarker,
        ));
    }

    for (entity, data) in &query_line {
        let mut mesh = Mesh::new(PrimitiveTopology::LineList, RenderAssetUsages::default());
        let positions = data.points.clone();
        mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
        // Bevy line meshes don't need indices if sequential, but we support them
        if let Some(idx) = &data.indices {
            mesh.insert_indices(Indices::U32(idx.clone()));
        }
        let mesh_handle = meshes.add(mesh);
        let rgba = data.color.to_rgba8();
        let color = bevy::color::Color::srgba_u8(rgba.r, rgba.g, rgba.b, rgba.a);
        let mat = materials.add(bevy::pbr::StandardMaterial {
            base_color: color,
            unlit: true,
            ..Default::default()
        });
        commands.entity(entity).insert((
            bevy::prelude::Mesh3d(mesh_handle),
            bevy::prelude::MeshMaterial3d::<bevy::pbr::StandardMaterial>(mat),
            bevy::prelude::Transform::default(),
            bevy::prelude::Visibility::default(),
            Mesh3DMarker,
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::prelude::{BuildChildrenTransformExt, Schedule, World};

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
