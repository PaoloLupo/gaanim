use crate::components::{
    FillBrush, GlobalOpacity, GroupMarker, LineListData, LocalBounds, Material3D,
    Material3DBaseline, Mesh3DMarker, Opacity, StrokeBrush, TriangleMeshData, WorldBounds,
};
use bevy::animation::AnimationPlayer;
use bevy::animation::graph::{AnimationGraph, AnimationGraphHandle};
use bevy::color::Alpha;
use bevy::prelude::{
    Added, AssetServer, Assets, Camera, Camera3d, Changed, ChildOf, Children, Commands,
    DirectionalLight, Entity, GlobalAmbientLight, Handle, Local, MeshMaterial3d, Name, Or,
    ParamSet, PointLight, Query, Res, ResMut, SceneRoot, SceneSpawner, SpotLight, StandardMaterial,
    Transform, Visibility, With, Without,
};
use bevy::scene::SceneInstance;
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
use std::collections::HashSet;

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
    mut query: Query<
        (&GlobalSpatialTransform, &mut bevy::prelude::Transform),
        With<crate::components::Mesh3DMarker>,
    >,
) {
    for (global, mut transform) in &mut query {
        let (scale, rot, trans) = global.mat4.to_scale_rotation_translation();
        transform.translation =
            bevy::prelude::Vec3::new(trans.x as f32, trans.y as f32, trans.z as f32);
        transform.rotation =
            bevy::prelude::Quat::from_xyzw(rot.x as f32, rot.y as f32, rot.z as f32, rot.w as f32);
        transform.scale = bevy::prelude::Vec3::new(scale.x as f32, scale.y as f32, scale.z as f32);
    }
}

/// Request native Bevy assets for newly compiled glTF model roots.
pub fn request_gltf_assets_system(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    query: Query<
        (Entity, &crate::components::GltfModelRoot),
        Added<crate::components::GltfModelRoot>,
    >,
) {
    for (entity, model) in &query {
        let handle: Handle<bevy::gltf::Gltf> = asset_server.load_override(model.path.clone());
        commands
            .entity(entity)
            .insert(crate::components::GltfAssetHandle(handle));
    }
}

pub fn ensure_default_3d_light_system(
    mut commands: Commands,
    models: Query<(), With<crate::components::GltfModelRoot>>,
    meshes: Query<&TriangleMeshData>,
    lights: Query<(), With<crate::components::GaanimDefault3dLight>>,
    lighting: Option<Res<crate::components::Lighting3D>>,
    ambient: Option<ResMut<GlobalAmbientLight>>,
) {
    let has_lit_content = !models.is_empty() || meshes.iter().any(|mesh| mesh.material.is_some());
    let lighting = lighting.as_deref().copied().unwrap_or_default();
    if let Some(mut ambient) = ambient {
        ambient.color = bevy::color::Color::srgb(0.72, 0.78, 1.0);
        ambient.brightness = if has_lit_content && lighting.enabled {
            180.0 * lighting.intensity.max(0.0)
        } else {
            0.0
        };
    }
    if !lighting.enabled {
        return;
    }
    if has_lit_content && lights.is_empty() {
        commands.spawn((
            crate::components::GaanimDefault3dLight,
            DirectionalLight {
                color: bevy::color::Color::srgb(1.0, 0.93, 0.82),
                illuminance: 11_000.0 * lighting.intensity.max(0.0),
                shadows_enabled: lighting.shadows,
                ..Default::default()
            },
            Transform::from_xyz(4.0, 8.0, 4.0)
                .looking_at(bevy::prelude::Vec3::ZERO, bevy::prelude::Vec3::Y),
        ));
        commands.spawn((
            crate::components::GaanimDefault3dLight,
            DirectionalLight {
                color: bevy::color::Color::srgb(0.58, 0.72, 1.0),
                illuminance: 4_000.0 * lighting.intensity.max(0.0),
                shadows_enabled: false,
                ..Default::default()
            },
            Transform::from_xyz(-5.0, 3.0, -4.0)
                .looking_at(bevy::prelude::Vec3::ZERO, bevy::prelude::Vec3::Y),
        ));
    }
}

fn bevy_color(color: gaanim_core::peniko::Color) -> bevy::color::Color {
    let rgba = color.to_rgba8();
    bevy::color::Color::srgba_u8(rgba.r, rgba.g, rgba.b, rgba.a)
}

fn bevy_emissive(material: Material3D) -> bevy::color::LinearRgba {
    let mut linear = bevy_color(material.emissive).to_linear();
    linear.red *= material.emissive_strength;
    linear.green *= material.emissive_strength;
    linear.blue *= material.emissive_strength;
    linear
}

/// Attach the selected scene once Bevy has decoded the glTF container.
pub fn attach_gltf_scenes_system(
    mut commands: Commands,
    gltfs: Res<Assets<bevy::gltf::Gltf>>,
    query: Query<
        (
            Entity,
            &crate::components::GltfModelRoot,
            &crate::components::GltfAssetHandle,
        ),
        Without<SceneRoot>,
    >,
) {
    for (entity, model, source) in &query {
        let Some(gltf) = gltfs.get(&source.0) else {
            continue;
        };
        if let Some(scene) = gltf.scenes.get(model.scene_index) {
            commands.entity(entity).insert(SceneRoot(scene.clone()));
        }
    }
}

/// Link stable Gaanim wrappers to native glTF nodes and prepare deterministic
/// paused animation players. Imported lights and cameras are deliberately removed.
#[allow(clippy::too_many_arguments)]
pub fn finalize_gltf_instances_system(
    mut commands: Commands,
    spawner: Res<SceneSpawner>,
    gltfs: Res<Assets<bevy::gltf::Gltf>>,
    mut graphs: ResMut<Assets<AnimationGraph>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    roots: Query<
        (
            Entity,
            &crate::components::GltfModelRoot,
            &crate::components::GltfAssetHandle,
            &SceneInstance,
        ),
        Without<crate::components::GltfModelReady>,
    >,
    names: Query<&Name>,
    parents: Query<&ChildOf>,
    mut animation_players: Query<&mut AnimationPlayer>,
    mesh_materials: Query<&MeshMaterial3d<StandardMaterial>>,
) {
    for (root_entity, model, source, instance) in &roots {
        if !spawner.instance_is_ready(**instance) {
            continue;
        }
        let Some(gltf) = gltfs.get(&source.0) else {
            continue;
        };
        let mut entities = spawner
            .iter_instance_entities(**instance)
            .collect::<Vec<_>>();
        entities.sort_by_key(|entity| entity.to_bits());
        let instance_entities = entities.iter().copied().collect::<HashSet<_>>();

        let mut by_path = std::collections::HashMap::<String, Vec<Entity>>::new();
        for entity in entities.iter().copied() {
            if let Some(path) =
                gltf_instance_path(entity, root_entity, &instance_entities, &names, &parents)
            {
                by_path.entry(path).or_default().push(entity);
            }
        }
        for candidates in by_path.values_mut() {
            candidates.sort_by_key(|entity| entity.to_bits());
        }

        let mut used = HashSet::<Entity>::new();
        for binding in &model.nodes {
            let path = binding
                .path
                .rsplit_once('#')
                .map_or(binding.path.as_str(), |(base, _)| base);
            let Some(native) = by_path
                .get(path)
                .and_then(|items| items.iter().copied().find(|entity| used.insert(*entity)))
            else {
                continue;
            };
            let original_parent = parents
                .get(native)
                .ok()
                .map(ChildOf::parent)
                .unwrap_or(root_entity);
            commands
                .entity(binding.wrapper)
                .insert(ChildOf(original_parent));
            commands.entity(native).insert(ChildOf(binding.wrapper));
        }

        let (graph, graph_nodes) = AnimationGraph::from_clips(gltf.animations.iter().cloned());
        let graph_handle = graphs.add(graph);
        let mut players = Vec::new();
        for entity in entities.iter().copied() {
            if let Ok(mut player) = animation_players.get_mut(entity) {
                for node in &graph_nodes {
                    player
                        .start(*node)
                        .pause()
                        .set_weight(0.0)
                        .set_seek_time(0.0);
                }
                commands
                    .entity(entity)
                    .insert(AnimationGraphHandle(graph_handle.clone()));
                players.push(entity);
            }

            commands.entity(entity).remove::<Camera>();
            commands.entity(entity).remove::<Camera3d>();
            commands.entity(entity).remove::<DirectionalLight>();
            commands.entity(entity).remove::<PointLight>();
            commands.entity(entity).remove::<SpotLight>();

            if let Ok(material_handle) = mesh_materials.get(entity)
                && let Some(source_material) = materials.get(&material_handle.0).cloned()
            {
                let alpha = source_material.base_color.alpha();
                let alpha_mode = source_material.alpha_mode;
                let clone_handle = materials.add(source_material);
                commands.entity(entity).insert((
                    MeshMaterial3d(clone_handle),
                    Opacity::default(),
                    GlobalOpacity::default(),
                    crate::components::GltfMaterialBaseline { alpha, alpha_mode },
                ));
            }
        }
        commands.entity(root_entity).insert((
            crate::components::GltfAnimationState {
                graph: graph_handle,
                nodes: graph_nodes,
                players,
            },
            crate::components::GltfModelReady,
        ));
    }
}

fn gltf_instance_path(
    mut entity: Entity,
    root: Entity,
    instance_entities: &HashSet<Entity>,
    names: &Query<&Name>,
    parents: &Query<&ChildOf>,
) -> Option<String> {
    let mut segments = Vec::new();
    while entity != root && instance_entities.contains(&entity) {
        let name = names.get(entity).ok()?;
        segments.push(name.as_str().to_owned());
        entity = parents.get(entity).ok()?.parent();
    }
    segments.reverse();
    (!segments.is_empty()).then(|| segments.join("/"))
}

/// Manual Gaanim transforms are local to wrappers and therefore compose with
/// the authored Bevy transform retained on each native glTF node.
pub fn sync_gltf_wrapper_transform_system(
    mut query: Query<
        (&SpatialTransform, &mut Transform),
        Or<(
            With<crate::components::GltfNodeWrapper>,
            With<crate::components::GltfModelRoot>,
        )>,
    >,
) {
    for (local, mut transform) in &mut query {
        transform.translation = bevy::prelude::Vec3::new(
            local.translation.x as f32,
            local.translation.y as f32,
            local.translation.z as f32,
        );
        transform.rotation = bevy::prelude::Quat::from_xyzw(
            local.rotation.x as f32,
            local.rotation.y as f32,
            local.rotation.z as f32,
            local.rotation.w as f32,
        );
        transform.scale = bevy::prelude::Vec3::new(
            local.scale.x as f32,
            local.scale.y as f32,
            local.scale.z as f32,
        );
    }
}

/// Apply propagated wrapper opacity to per-instance cloned PBR materials.
pub fn sync_gltf_material_opacity_system(
    mut materials: ResMut<Assets<StandardMaterial>>,
    query: Query<
        (
            &GlobalOpacity,
            &crate::components::GltfMaterialBaseline,
            &MeshMaterial3d<StandardMaterial>,
        ),
        Changed<GlobalOpacity>,
    >,
) {
    for (opacity, baseline, handle) in &query {
        if let Some(material) = materials.get_mut(&handle.0) {
            material
                .base_color
                .set_alpha((baseline.alpha * opacity.0).clamp(0.0, 1.0));
            if opacity.0 < 0.999 {
                material.alpha_mode = bevy::render::alpha::AlphaMode::Blend;
            } else {
                material.alpha_mode = baseline.alpha_mode;
            }
        }
    }
}

/// Mirror Gaanim's marker visibility onto Bevy hierarchy visibility.
pub fn sync_gltf_visibility_system(
    mut query: Query<
        (Option<&crate::components::Visible>, &mut Visibility),
        Or<(
            With<crate::components::GltfNodeWrapper>,
            With<crate::components::GltfModelRoot>,
        )>,
    >,
) {
    for (visible, mut bevy_visibility) in &mut query {
        *bevy_visibility = if visible.is_some() {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
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

/// System: Build Bevy Meshes from raw Triangle/Line data components.
///
/// Converts `TriangleMeshData` and `LineListData` into `Mesh3d` + `MeshMaterial3d` + `Transform`.
pub fn build_3d_meshes_system(
    mut commands: bevy::prelude::Commands,
    meshes: Option<ResMut<bevy::prelude::Assets<bevy::mesh::Mesh>>>,
    materials: Option<ResMut<bevy::prelude::Assets<bevy::pbr::StandardMaterial>>>,
    query_tri: Query<
        (Entity, &TriangleMeshData),
        (Without<bevy::prelude::Mesh3d>, Without<LineListData>),
    >,
    query_line: Query<
        (Entity, &LineListData),
        (Without<bevy::prelude::Mesh3d>, Without<TriangleMeshData>),
    >,
) {
    let (Some(mut meshes), Some(mut materials)) = (meshes, materials) else {
        return;
    };
    use bevy::asset::RenderAssetUsages;
    use bevy::mesh::{Indices, Mesh, PrimitiveTopology};

    for (entity, data) in &query_tri {
        let mut mesh = Mesh::new(
            PrimitiveTopology::TriangleList,
            RenderAssetUsages::default(),
        );
        let positions: Vec<[f32; 3]> = data.vertices.clone();
        let indices = data.indices.clone();
        mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
        if let Some(normals) = &data.normals
            && normals.len() == mesh.count_vertices()
        {
            mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals.clone());
        }
        let has_vertex_colors = data
            .colors
            .as_ref()
            .is_some_and(|colors| colors.len() == mesh.count_vertices());
        let has_transparent_vertex_colors = data.colors.as_ref().is_some_and(|colors| {
            colors.len() == mesh.count_vertices() && colors.iter().any(|color| color[3] < 0.999)
        });
        if let Some(colors) = &data.colors
            && colors.len() == mesh.count_vertices()
        {
            mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, colors.clone());
        }
        mesh.insert_attribute(
            Mesh::ATTRIBUTE_UV_0,
            data.uvs
                .clone()
                .filter(|uvs| uvs.len() == mesh.count_vertices())
                .unwrap_or_else(|| vec![[0.0, 0.0]; mesh.count_vertices()]),
        );
        mesh.insert_indices(Indices::U32(indices));
        if !mesh.contains_attribute(Mesh::ATTRIBUTE_NORMAL) {
            mesh.compute_smooth_normals();
        }
        let mesh_handle = meshes.add(mesh);
        let pbr = data.material;
        let source = pbr.unwrap_or(Material3D {
            color: data.color.unwrap_or(gaanim_core::peniko::Color::WHITE),
            ..Default::default()
        });
        let source_color = if has_vertex_colors {
            bevy::color::Color::WHITE
        } else {
            bevy_color(source.color)
        };
        let alpha = source_color.alpha();
        let alpha_mode = if has_transparent_vertex_colors || alpha < 0.999 {
            bevy::render::alpha::AlphaMode::Blend
        } else {
            bevy::render::alpha::AlphaMode::Opaque
        };
        let mat = materials.add(bevy::pbr::StandardMaterial {
            base_color: source_color,
            emissive: bevy_emissive(source),
            perceptual_roughness: source.roughness,
            metallic: source.metallic,
            alpha_mode,
            unlit: pbr.is_none(),
            double_sided: pbr.is_none(),
            cull_mode: if pbr.is_none() {
                None
            } else {
                Some(bevy::render::render_resource::Face::Back)
            },
            ..Default::default()
        });
        let mut entity_commands = commands.entity(entity);
        entity_commands.insert((
            bevy::prelude::Mesh3d(mesh_handle),
            bevy::prelude::MeshMaterial3d::<bevy::pbr::StandardMaterial>(mat),
            bevy::prelude::Transform::default(),
            bevy::prelude::Visibility::default(),
            Mesh3DMarker,
            Material3DBaseline { alpha, alpha_mode },
        ));
        if pbr.is_some() {
            entity_commands.insert(source);
        }
    }

    for (entity, data) in &query_line {
        let mut mesh = Mesh::new(
            if data.strip {
                PrimitiveTopology::LineStrip
            } else {
                PrimitiveTopology::LineList
            },
            RenderAssetUsages::default(),
        );
        let positions = data.points.clone();
        mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
        // Bevy line meshes don't need indices if sequential, but we support them
        if let Some(idx) = &data.indices {
            mesh.insert_indices(Indices::U32(idx.clone()));
        }
        // Per-vertex colors (colormap) — if present, use vertex colors instead of uniform material tint.
        // StandardMaterial will multiply base_color * vertex_color, so we set base to WHITE.
        let has_vertex_colors = data.colors.as_ref().is_some_and(|c| !c.is_empty());
        let has_transparent_vertex_colors = data.colors.as_ref().is_some_and(|colors| {
            colors.len() == mesh.count_vertices() && colors.iter().any(|color| color[3] < 0.999)
        });
        if let Some(cols) = &data.colors
            && cols.len() == mesh.count_vertices()
        {
            mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, cols.clone());
        }
        let mesh_handle = meshes.add(mesh);
        let (base_color, alpha_mode) = if has_vertex_colors {
            // Vertex colors carry the gradient; keep base white and blend only
            // when at least one authored vertex is actually transparent.
            (
                bevy::color::Color::WHITE,
                if has_transparent_vertex_colors {
                    bevy::render::alpha::AlphaMode::Blend
                } else {
                    bevy::render::alpha::AlphaMode::Opaque
                },
            )
        } else {
            let rgba = data.color.to_rgba8();
            let color = bevy::color::Color::srgba_u8(rgba.r, rgba.g, rgba.b, rgba.a);
            let mode = if color.alpha() < 0.999 {
                bevy::render::alpha::AlphaMode::Blend
            } else {
                bevy::render::alpha::AlphaMode::Opaque
            };
            (color, mode)
        };
        let alpha = base_color.alpha();
        let mat = materials.add(bevy::pbr::StandardMaterial {
            base_color,
            alpha_mode,
            unlit: true,
            ..Default::default()
        });
        commands.entity(entity).insert((
            bevy::prelude::Mesh3d(mesh_handle),
            bevy::prelude::MeshMaterial3d::<bevy::pbr::StandardMaterial>(mat),
            bevy::prelude::Transform::default(),
            bevy::prelude::Visibility::default(),
            Mesh3DMarker,
            Material3DBaseline { alpha, alpha_mode },
        ));
    }
}

/// Synchronize animatable Gaanim PBR parameters and propagated opacity.
#[allow(clippy::type_complexity)]
pub fn sync_material_3d_system(
    mut materials: ResMut<Assets<StandardMaterial>>,
    query: Query<
        (
            Option<&Material3D>,
            &GlobalOpacity,
            &Material3DBaseline,
            &MeshMaterial3d<StandardMaterial>,
        ),
        Or<(
            Changed<Material3D>,
            Changed<GlobalOpacity>,
            Added<Material3DBaseline>,
        )>,
    >,
) {
    for (source, opacity, baseline, handle) in &query {
        if let Some(material) = materials.get_mut(&handle.0) {
            let mut color = source
                .map(|source| bevy_color(source.color))
                .unwrap_or(material.base_color);
            color.set_alpha((baseline.alpha * opacity.0).clamp(0.0, 1.0));
            material.base_color = color;
            if let Some(source) = source {
                material.emissive = bevy_emissive(*source);
                material.perceptual_roughness = source.roughness;
                material.metallic = source.metallic;
            }
            material.alpha_mode = if color.alpha() < 0.999 {
                bevy::render::alpha::AlphaMode::Blend
            } else {
                baseline.alpha_mode
            };
        }
    }
}

/// System: Update existing 3D line meshes when `LineListData` changes (e.g. traced path growing).
pub fn update_3d_line_meshes_system(
    mut commands: bevy::prelude::Commands,
    meshes: Option<bevy::prelude::ResMut<bevy::asset::Assets<bevy::mesh::Mesh>>>,
    materials: Option<bevy::prelude::ResMut<bevy::asset::Assets<bevy::pbr::StandardMaterial>>>,
    mut query: bevy::prelude::Query<
        (
            bevy::prelude::Entity,
            &LineListData,
            &bevy::prelude::Mesh3d,
            &bevy::prelude::MeshMaterial3d<bevy::pbr::StandardMaterial>,
            Option<&mut LocalBounds>,
        ),
        bevy::prelude::Changed<LineListData>,
    >,
) {
    use bevy::asset::RenderAssetUsages;
    use bevy::mesh::{Indices, Mesh, PrimitiveTopology};
    let (Some(mut meshes), Some(mut materials)) = (meshes, materials) else {
        return;
    };
    if query.is_empty() {
        return;
    }
    for (entity, data, mesh_handle, mat_handle, local_bounds_opt) in &mut query {
        // Rebuild mesh from current points/colors
        let mut mesh = Mesh::new(
            if data.strip {
                PrimitiveTopology::LineStrip
            } else {
                PrimitiveTopology::LineList
            },
            RenderAssetUsages::default(),
        );
        mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, data.points.clone());
        if let Some(idx) = &data.indices {
            mesh.insert_indices(Indices::U32(idx.clone()));
        }
        let has_vertex_colors = data
            .colors
            .as_ref()
            .is_some_and(|c| c.len() == data.points.len());
        if let Some(cols) = &data.colors
            && cols.len() == data.points.len()
        {
            mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, cols.clone());
        }
        let new_handle = meshes.add(mesh);
        commands
            .entity(entity)
            .insert(bevy::prelude::Mesh3d(new_handle));

        // Update material alpha mode to match vertex-color presence (transparency)
        if let Some(mat) = materials.get_mut(&mat_handle.0) {
            mat.alpha_mode = if has_vertex_colors {
                bevy::render::alpha::AlphaMode::Blend
            } else {
                bevy::render::alpha::AlphaMode::Opaque
            };
            if has_vertex_colors {
                mat.base_color = bevy::color::Color::WHITE;
            } else {
                let rgba = data.color.to_rgba8();
                mat.base_color = bevy::color::Color::srgba_u8(rgba.r, rgba.g, rgba.b, rgba.a);
            }
        }

        // Keep LocalBounds in sync for correct frustum culling
        if let Some(mut bounds) = local_bounds_opt {
            if data.points.is_empty() {
                bounds.0 = gaanim_math::Bounds3D::default();
            } else {
                let mut min = gaanim_core::glam::DVec3::splat(f64::INFINITY);
                let mut max = gaanim_core::glam::DVec3::splat(f64::NEG_INFINITY);
                for p in &data.points {
                    let v = gaanim_core::glam::DVec3::new(p[0] as f64, p[1] as f64, p[2] as f64);
                    min = min.min(v);
                    max = max.max(v);
                }
                bounds.0 = gaanim_math::Bounds3D::new(min, max);
            }
        }
        let _ = mesh_handle; // suppress unused warning if needed
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::prelude::{App, BuildChildrenTransformExt, Schedule, Update, World};

    fn lit_triangle() -> TriangleMeshData {
        TriangleMeshData {
            vertices: vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
            indices: vec![0, 1, 2],
            normals: Some(vec![[0.0, 0.0, 1.0]; 3]),
            uvs: Some(vec![[0.0, 0.0]; 3]),
            color: None,
            colors: None,
            material: Some(Material3D::default()),
        }
    }

    #[test]
    fn studio_rig_is_created_once_and_none_creates_no_lights() {
        let mut app = App::new();
        app.insert_resource(GlobalAmbientLight::default())
            .insert_resource(crate::components::Lighting3D::default())
            .add_systems(Update, ensure_default_3d_light_system);
        app.world_mut().spawn(lit_triangle());
        app.update();
        app.update();
        let count = app
            .world_mut()
            .query_filtered::<Entity, With<crate::components::GaanimDefault3dLight>>()
            .iter(app.world())
            .count();
        assert_eq!(count, 2);

        let mut none = App::new();
        none.insert_resource(GlobalAmbientLight::default())
            .insert_resource(crate::components::Lighting3D {
                enabled: false,
                intensity: 1.0,
                shadows: true,
            })
            .add_systems(Update, ensure_default_3d_light_system);
        none.world_mut().spawn(lit_triangle());
        none.update();
        let count = none
            .world_mut()
            .query_filtered::<Entity, With<crate::components::GaanimDefault3dLight>>()
            .iter(none.world())
            .count();
        assert_eq!(count, 0);
    }

    #[test]
    fn native_primitive_compiles_to_lit_standard_material() {
        let mut world = World::new();
        world.insert_resource(Assets::<bevy::mesh::Mesh>::default());
        world.insert_resource(Assets::<StandardMaterial>::default());
        let source =
            Material3D::new(gaanim_core::peniko::Color::WHITE, 0.3, 0.8, None, 0.0).unwrap();
        let mut triangle = lit_triangle();
        triangle.material = Some(source);
        let entity = world
            .spawn((triangle, GlobalOpacity(1.0), Opacity(1.0)))
            .id();
        let mut schedule = Schedule::default();
        schedule.add_systems(build_3d_meshes_system);
        schedule.run(&mut world);

        let handle = world
            .get::<MeshMaterial3d<StandardMaterial>>(entity)
            .expect("compiled material handle")
            .0
            .clone();
        let material = world
            .resource::<Assets<StandardMaterial>>()
            .get(&handle)
            .unwrap();
        assert!(!material.unlit);
        assert!((material.perceptual_roughness - 0.3).abs() < 1e-6);
        assert!((material.metallic - 0.8).abs() < 1e-6);
        assert_eq!(*world.get::<Material3D>(entity).unwrap(), source);
    }

    #[test]
    fn unlit_mesh_material_tracks_propagated_opacity() {
        let mut world = World::new();
        world.insert_resource(Assets::<bevy::mesh::Mesh>::default());
        world.insert_resource(Assets::<StandardMaterial>::default());
        let mut triangle = lit_triangle();
        triangle.material = None;
        triangle.colors = Some(vec![[0.2, 0.4, 0.8, 1.0]; 3]);
        let entity = world
            .spawn((triangle, GlobalOpacity(0.25), Opacity(1.0)))
            .id();

        let mut build = Schedule::default();
        build.add_systems(build_3d_meshes_system);
        build.run(&mut world);
        let handle = world
            .get::<MeshMaterial3d<StandardMaterial>>(entity)
            .expect("compiled unlit material handle")
            .0
            .clone();

        let mut sync = Schedule::default();
        sync.add_systems(sync_material_3d_system);
        sync.run(&mut world);
        let material = world
            .resource::<Assets<StandardMaterial>>()
            .get(&handle)
            .unwrap();
        assert!(material.unlit);
        assert!((material.base_color.alpha() - 0.25).abs() < f32::EPSILON);
        assert_eq!(material.alpha_mode, bevy::render::alpha::AlphaMode::Blend);

        world.entity_mut(entity).insert(GlobalOpacity(1.0));
        sync.run(&mut world);
        let material = world
            .resource::<Assets<StandardMaterial>>()
            .get(&handle)
            .unwrap();
        assert!((material.base_color.alpha() - 1.0).abs() < f32::EPSILON);
        assert_eq!(
            material.alpha_mode,
            bevy::render::alpha::AlphaMode::Opaque,
            "opaque vertex colors restore their authored opaque mode",
        );
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

    #[test]
    fn gltf_material_restores_authored_alpha_mode_after_fade() {
        let mut world = World::new();
        let mut materials = Assets::<StandardMaterial>::default();
        let handle = materials.add(StandardMaterial {
            alpha_mode: bevy::render::alpha::AlphaMode::Opaque,
            ..Default::default()
        });
        world.insert_resource(materials);
        let entity = world
            .spawn((
                GlobalOpacity(0.5),
                crate::components::GltfMaterialBaseline {
                    alpha: 1.0,
                    alpha_mode: bevy::render::alpha::AlphaMode::Opaque,
                },
                MeshMaterial3d(handle.clone()),
            ))
            .id();
        let mut schedule = Schedule::default();
        schedule.add_systems(sync_gltf_material_opacity_system);

        schedule.run(&mut world);
        assert_eq!(
            world
                .resource::<Assets<StandardMaterial>>()
                .get(&handle)
                .unwrap()
                .alpha_mode,
            bevy::render::alpha::AlphaMode::Blend
        );

        world.entity_mut(entity).insert(GlobalOpacity(1.0));
        schedule.run(&mut world);
        let materials = world.resource::<Assets<StandardMaterial>>();
        let material = materials.get(&handle).unwrap();
        assert_eq!(material.alpha_mode, bevy::render::alpha::AlphaMode::Opaque);
        assert!((material.base_color.alpha() - 1.0).abs() < f32::EPSILON);
    }
}
