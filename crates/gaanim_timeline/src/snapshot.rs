use bevy::prelude::{
    BuildChildrenTransformExt, Component, Entity, EntityWorldMut, Resource, World,
};
use gaanim_core::ObjectId;
use gaanim_math::{GlobalSpatialTransform, SpatialTransform};
use gaanim_scene::{
    FillBrush, FillLevel, GlobalOpacity, LocalBounds, MobjectId, ObjectTag, Opacity, Path2D,
    PathSource, RenderLayer, RenderOrder, StrokeBrush, Visible, WorldBounds,
};
use std::collections::HashMap;

use crate::clip::SceneId;
use crate::scene::SceneMember;

/// Authored camera poses captured by timeline events and referenced by later clips.
#[derive(Resource, Debug, Clone, Default, PartialEq)]
pub struct CapturedCameraStates(pub HashMap<u64, gaanim_math::CameraPose>);

/// A snapshot capturing the complete state of a single Mobject entity.
///
/// This structure is fully serializable when the `serde` feature is active,
/// making it perfect for saving/loading keyframes, network rendering, and undo/redo histories.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct EntitySnapshot {
    /// The unique identity of the object.
    pub id: ObjectId,
    /// The parent object ID if part of a group or parented hierarchy.
    pub parent: Option<ObjectId>,
    /// The local spatial transform (position, rotation, scale, pivot).
    pub transform: SpatialTransform,
    /// The local opacity factor.
    pub opacity: f32,
    /// Optional fill color or gradient brush.
    pub fill: Option<gaanim_core::peniko::Brush>,
    /// Whether the entity had a `FillBrush` component. Groups without an
    /// explicit style must keep this false so replay does not accidentally
    /// propagate a transparent fill to their descendants.
    #[cfg_attr(feature = "serde", serde(default))]
    pub has_fill_component: bool,
    /// Optional outline brush.
    pub stroke: Option<gaanim_core::peniko::Brush>,
    /// Optional outline styling parameters (width, dashes, joins).
    pub stroke_style: Option<gaanim_core::kurbo::Stroke>,
    /// Ordering layer index.
    pub render_order: i32,
    /// Tie-breaker creation sequence index.
    pub creation_order: u64,
    /// The rendering pipeline backend target.
    pub render_layer: RenderLayer,
    /// Whether the object is active and visible in the scene.
    pub visible: bool,
    /// Descriptive tags for identifying the object type or attributes.
    pub tags: Vec<String>,
    /// Current 2D Bézier path geometry.
    pub path2d: Option<std::sync::Arc<gaanim_core::kurbo::BezPath>>,
    /// Cached original path for write/unwrite animations.
    pub path_source: Option<std::sync::Arc<gaanim_core::kurbo::BezPath>>,
    /// Fill-draw progress for write/unwrite animations (0.0 = outline only, 1.0 = full fill).
    pub fill_draw_progress: Option<f32>,
    #[cfg_attr(feature = "serde", serde(default))]
    pub fill_level: Option<f64>,
    /// Live target/interpolation state for a reactive surrounding rectangle.
    #[cfg_attr(feature = "serde", serde(default))]
    pub surrounding_rect: Option<gaanim_animation::SurroundingRect>,
    /// Runtime progress for the transient Write pen-tip illumination.
    /// Restoring it prevents a partial glyph/head highlight from surviving a rewind.
    #[cfg_attr(feature = "serde", serde(skip))]
    pub write_tip_glow: Option<gaanim_animation::WriteTipGlow>,
    /// Path-reveal progress for draw animations (0.0 = hidden, 1.0 = fully drawn).
    /// Used to keep reactive regenerators (e.g. ExpressionPlot) in sync
    /// with the current trim so they do not overwrite it with the full path.
    #[cfg_attr(feature = "serde", serde(default))]
    pub path_reveal: Option<f64>,
    /// Value of a `FloatSignal` (e.g. `Parameter` / `ValueTracker`) at capture time.
    /// Restoring this on seek ensures looped playback returns to the initial
    /// parameter value instead of staying at the final animated value.
    #[cfg_attr(feature = "serde", serde(default))]
    pub float_signal: Option<f64>,
    /// PBR material state for deterministic backward and forward seeks.
    #[cfg_attr(feature = "serde", serde(default))]
    pub material_3d: Option<gaanim_scene::Material3D>,
    /// Runtime state of a traced path, used to restore scrubbing/replay cleanly.
    pub traced_path_points: Option<Vec<gaanim_core::glam::DVec3>>,
    /// Timeline timestamps paired with `traced_path_points`.
    #[cfg_attr(feature = "serde", serde(default))]
    pub traced_path_sample_times: Option<Vec<f64>>,
    /// Whether the entity is a group container.
    pub is_group: bool,
    /// Propagated global spatial transform.
    pub global_transform: Option<GlobalSpatialTransform>,
    /// Optional local bounding box.
    pub local_bounds: Option<LocalBounds>,
    /// Optional world bounding box.
    pub world_bounds: Option<WorldBounds>,
    /// Propagated global opacity factor.
    pub global_opacity: Option<GlobalOpacity>,
    /// The scene this entity belongs to (None for global entities).
    pub scene: Option<SceneId>,
}

/// Captures the complete state of all Mobject entities within the ECS world.
#[derive(Debug, Clone, Default, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct WorldSnapshot {
    /// A map from unique object IDs to their captured snapshots.
    pub entities: HashMap<ObjectId, EntitySnapshot>,
    /// Complete authored camera state. Presentation viewport fit is excluded.
    #[cfg_attr(feature = "serde", serde(default))]
    pub camera: Option<gaanim_math::Camera>,
    /// Timeline camera captures required by later state transitions.
    #[cfg_attr(feature = "serde", serde(default))]
    pub camera_states: HashMap<u64, gaanim_math::CameraPose>,
}

/// Insert a component only when the snapshot differs from the live world.
///
/// Bevy marks a component as changed even if an insertion replaces it with an
/// equal value. Snapshot restores run for every seek, so unconditional inserts
/// would invalidate renderer caches for otherwise static SVG paths.
fn insert_if_changed<T: Component + PartialEq>(entity_mut: &mut EntityWorldMut<'_>, value: T) {
    if entity_mut.get::<T>() != Some(&value) {
        entity_mut.insert(value);
    }
}

/// Remove a component only when it is present, preserving Bevy change ticks for
/// a snapshot that already matches the world.
fn remove_if_present<T: Component>(entity_mut: &mut EntityWorldMut<'_>) {
    if entity_mut.contains::<T>() {
        entity_mut.remove::<T>();
    }
}

fn sync_optional<T: Component + PartialEq>(entity_mut: &mut EntityWorldMut<'_>, value: Option<T>) {
    if let Some(value) = value {
        insert_if_changed(entity_mut, value);
    } else {
        remove_if_present::<T>(entity_mut);
    }
}

/// Insert or update all components of an `EntitySnapshot` onto a Bevy entity.
///
/// Restoration remains complete and deterministic, while equal renderer-invalidating
/// values are left untouched so Bevy does not report false geometry/style changes.
fn insert_snapshot_components(entity_mut: &mut EntityWorldMut<'_>, snap: &EntitySnapshot) {
    let global_transform = snap
        .global_transform
        .unwrap_or_else(|| GlobalSpatialTransform::from_local(&snap.transform));
    let global_opacity = snap.global_opacity.unwrap_or(GlobalOpacity(snap.opacity));

    // Restoring local transforms/opacity must wake the propagation systems.
    // Their derived global values are intentionally recomputed after every
    // exact seek, even when the authored local value itself is unchanged.
    // Unlike path/style changes this does not invalidate retained geometry.
    entity_mut.insert(snap.transform);
    entity_mut.insert(Opacity(snap.opacity));
    insert_if_changed(
        entity_mut,
        RenderOrder {
            z_index: snap.render_order,
            creation_order: snap.creation_order,
        },
    );
    insert_if_changed(entity_mut, snap.render_layer);
    insert_if_changed(entity_mut, global_transform);
    insert_if_changed(entity_mut, global_opacity);

    sync_optional(
        entity_mut,
        snap.has_fill_component
            .then(|| FillBrush(snap.fill.clone())),
    );
    sync_optional(
        entity_mut,
        snap.stroke_style.as_ref().map(|style| StrokeBrush {
            brush: snap.stroke.clone(),
            style: style.clone(),
        }),
    );
    sync_optional(entity_mut, snap.visible.then_some(Visible));
    sync_optional(entity_mut, snap.tags.first().cloned().map(ObjectTag));
    sync_optional(entity_mut, snap.path2d.clone().map(Path2D));
    sync_optional(entity_mut, snap.path_source.clone().map(PathSource));
    sync_optional(
        entity_mut,
        snap.fill_draw_progress
            .map(gaanim_animation::FillDrawProgress),
    );
    sync_optional(entity_mut, snap.fill_level.map(FillLevel));
    sync_optional(entity_mut, snap.surrounding_rect.clone());
    sync_optional(entity_mut, snap.write_tip_glow.clone());
    sync_optional(
        entity_mut,
        snap.path_reveal.map(gaanim_animation::PathReveal),
    );
    match snap.float_signal {
        Some(value)
            if entity_mut
                .get::<gaanim_animation::FloatSignal>()
                .is_none_or(|signal| signal.value != value) =>
        {
            entity_mut.insert(gaanim_animation::FloatSignal::new(value));
        }
        None => remove_if_present::<gaanim_animation::FloatSignal>(entity_mut),
        _ => {}
    }
    sync_optional(entity_mut, snap.material_3d);

    if let Some(points) = &snap.traced_path_points
        && let Some(mut traced_path) = entity_mut.get_mut::<gaanim_animation::TracedPath>()
    {
        traced_path.points = points.clone();
        traced_path.sample_times = snap.traced_path_sample_times.clone().unwrap_or_default();
    }
    // 3D traced path — restore geometry and any registered colormap.
    if let Some(points) = &snap.traced_path_points {
        if let Some(mut traced_3d) = entity_mut.get_mut::<gaanim_animation::TracedPath3D>() {
            traced_3d.points = points.clone();
            traced_3d.sample_times = snap.traced_path_sample_times.clone().unwrap_or_default();
        }
        let colormap_opt = entity_mut
            .get::<gaanim_animation::TracedPath3D>()
            .and_then(|t| t.colormap.clone());
        if let Some(mut line) = entity_mut.get_mut::<gaanim_scene::LineListData>() {
            let pts: Vec<[f32; 3]> = points
                .iter()
                .map(|p| [p.x as f32, p.y as f32, p.z as f32])
                .collect();
            line.points.clone_from(&pts);
            line.colors = colormap_opt.and_then(|map| map.rgba_f32(pts.len()).ok());
        }
    }

    sync_optional(
        entity_mut,
        snap.is_group.then_some(gaanim_scene::GroupMarker),
    );
    sync_optional(entity_mut, snap.local_bounds);
    sync_optional(entity_mut, snap.world_bounds);
    sync_optional(entity_mut, snap.scene.map(SceneMember));
}

impl WorldSnapshot {
    /// Captures a new `WorldSnapshot` of all Mobjects currently registered in the Bevy `World`.
    pub fn capture(world: &mut World) -> Self {
        let mut entities = HashMap::new();
        let camera = world.get_resource::<gaanim_math::Camera>().copied();
        let camera_states = world
            .get_resource::<CapturedCameraStates>()
            .map(|states| states.0.clone())
            .unwrap_or_default();

        // Query all entities with a MobjectId component
        let mut query = world.query::<(Entity, &MobjectId)>();

        let mut captured_data = Vec::new();

        for (entity, mobj_id) in query.iter(world) {
            let obj_id = mobj_id.0;
            // Find parent entity's ObjectId if parent is set
            let parent_entity = world
                .get::<bevy::prelude::ChildOf>(entity)
                .map(|c| c.parent());
            let parent_id =
                parent_entity.and_then(|p| world.get::<MobjectId>(p).copied().map(|m| m.0));

            let transform = world
                .get::<SpatialTransform>(entity)
                .copied()
                .unwrap_or_default();
            let opacity = world.get::<Opacity>(entity).map(|o| o.0).unwrap_or(1.0);
            let has_fill_component = world.get::<FillBrush>(entity).is_some();
            let fill = world.get::<FillBrush>(entity).and_then(|f| f.0.clone());
            let stroke = world
                .get::<StrokeBrush>(entity)
                .and_then(|s| s.brush.clone());
            let stroke_style = world.get::<StrokeBrush>(entity).map(|s| s.style.clone());
            let render_order_opt = world.get::<RenderOrder>(entity);
            let render_order = render_order_opt.map(|r| r.z_index).unwrap_or(0);
            let creation_order = render_order_opt.map(|r| r.creation_order).unwrap_or(0);
            let render_layer = world
                .get::<RenderLayer>(entity)
                .copied()
                .unwrap_or(RenderLayer::Vello2D);
            let visible = world.get::<Visible>(entity).is_some();
            let is_group = world.get::<gaanim_scene::GroupMarker>(entity).is_some();

            let mut tags = Vec::new();
            if let Some(tag) = world.get::<ObjectTag>(entity) {
                tags.push(tag.0.clone());
            }

            captured_data.push((
                obj_id,
                EntitySnapshot {
                    id: obj_id,
                    parent: parent_id,
                    transform,
                    opacity,
                    fill,
                    has_fill_component,
                    stroke,
                    stroke_style,
                    render_order,
                    creation_order,
                    render_layer,
                    visible,
                    tags,
                    path2d: world.get::<Path2D>(entity).map(|p| p.0.clone()),
                    path_source: world.get::<PathSource>(entity).map(|p| p.0.clone()),
                    fill_draw_progress: world
                        .get::<gaanim_animation::FillDrawProgress>(entity)
                        .map(|p| p.0),
                    fill_level: world.get::<FillLevel>(entity).map(|level| level.0),
                    surrounding_rect: world
                        .get::<gaanim_animation::SurroundingRect>(entity)
                        .cloned(),
                    write_tip_glow: world.get::<gaanim_animation::WriteTipGlow>(entity).cloned(),
                    path_reveal: world
                        .get::<gaanim_animation::PathReveal>(entity)
                        .map(|p| p.0),
                    float_signal: world
                        .get::<gaanim_animation::FloatSignal>(entity)
                        .map(|s| s.value),
                    material_3d: world.get::<gaanim_scene::Material3D>(entity).copied(),
                    traced_path_points: world
                        .get::<gaanim_animation::TracedPath>(entity)
                        .map(|t| t.points.clone())
                        .or_else(|| {
                            world
                                .get::<gaanim_animation::TracedPath3D>(entity)
                                .map(|t| t.points.clone())
                        }),
                    traced_path_sample_times: world
                        .get::<gaanim_animation::TracedPath>(entity)
                        .map(|t| t.sample_times.clone())
                        .or_else(|| {
                            world
                                .get::<gaanim_animation::TracedPath3D>(entity)
                                .map(|t| t.sample_times.clone())
                        }),
                    is_group,
                    global_transform: world.get::<GlobalSpatialTransform>(entity).copied(),
                    local_bounds: world.get::<LocalBounds>(entity).copied(),
                    world_bounds: world.get::<WorldBounds>(entity).copied(),
                    global_opacity: world.get::<GlobalOpacity>(entity).copied(),
                    scene: world.get::<SceneMember>(entity).map(|s| s.0),
                },
            ));
        }

        for (id, snapshot) in captured_data {
            entities.insert(id, snapshot);
        }

        Self {
            entities,
            camera,
            camera_states,
        }
    }

    /// Restores the states stored in this snapshot back to the Bevy `World`.
    pub fn restore(&self, world: &mut World) {
        if let Some(camera) = self.camera {
            if world.get_resource::<gaanim_math::Camera>() != Some(&camera) {
                world.insert_resource(camera);
            }
        }
        world.insert_resource(CapturedCameraStates(self.camera_states.clone()));
        // 1. Gather all existing entities and build a dynamic mapping of ObjectIds to Bevy Entities
        let mut existing_entities = Vec::new();
        let mut entity_map = HashMap::new();
        {
            let mut query = world.query::<(Entity, &MobjectId)>();
            for (entity, mobj_id) in query.iter(world) {
                existing_entities.push((entity, mobj_id.0));
                entity_map.insert(mobj_id.0, entity);
            }
        }

        // 2. Hide any active Mobjects that do not exist in the snapshot
        for (entity, obj_id) in &existing_entities {
            if !self.entities.contains_key(obj_id) && world.get::<Visible>(*entity).is_some() {
                world.entity_mut(*entity).remove::<Visible>();
            }
        }

        // 3. Spawn any missing entities first so they exist in entity_map
        for (obj_id, snap) in &self.entities {
            if !entity_map.contains_key(obj_id) {
                let mut entity = world.spawn((
                    MobjectId(*obj_id),
                    snap.transform,
                    Opacity(snap.opacity),
                    snap.render_layer,
                ));
                if snap.has_fill_component {
                    entity.insert(FillBrush(snap.fill.clone()));
                }
                let new_entity = entity.id();

                entity_map.insert(*obj_id, new_entity);
            }
        }

        // 4. Pass 1: Set parent-child relationships for all entities
        for (obj_id, snap) in &self.entities {
            if let Some(&entity) = entity_map.get(obj_id) {
                if let Some(parent_id) = snap.parent {
                    if let Some(&parent_entity) = entity_map.get(&parent_id) {
                        // `set_parent_in_place` also reconciles Bevy's native
                        // `Transform` from the preserved `GlobalTransform`.
                        // Those components are runtime state, not snapshot
                        // fields, so this must run even when `ChildOf` already
                        // names the expected parent.
                        world.entity_mut(entity).set_parent_in_place(parent_entity);
                    }
                } else {
                    world.entity_mut(entity).remove_parent_in_place();
                }
            }
        }

        // 5. Pass 2: Overwrite all properties (including transforms) with correct snapshot values
        for (obj_id, snap) in &self.entities {
            if let Some(&entity) = entity_map.get(obj_id) {
                let mut entity_mut = world.entity_mut(entity);
                insert_snapshot_components(&mut entity_mut, snap);
            }
        }
    }

    /// Computes the delta/diff between this snapshot and a target snapshot.
    pub fn diff(&self, target: &Self) -> SnapshotDiff {
        let mut updates = Vec::new();
        let mut removals = Vec::new();

        // Identify entities in target that are new or changed compared to self
        for (id, target_entity) in &target.entities {
            if let Some(self_entity) = self.entities.get(id) {
                if self_entity != target_entity {
                    updates.push(target_entity.clone());
                }
            } else {
                updates.push(target_entity.clone());
            }
        }

        // Identify entities present in self but missing in target
        for id in self.entities.keys() {
            if !target.entities.contains_key(id) {
                removals.push(*id);
            }
        }

        SnapshotDiff { updates, removals }
    }
}

/// A compact delta snapshot representing only changed and removed entities.
///
/// This provides a highly optimized representation for network packet sync and undo/redo buffers.
#[derive(Debug, Clone, Default, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct SnapshotDiff {
    /// Mobjects whose component states have been modified or newly added.
    pub updates: Vec<EntitySnapshot>,
    /// Mobjects that were removed.
    pub removals: Vec<ObjectId>,
}

impl SnapshotDiff {
    /// Applies this diff to a Bevy `World`, bringing it to the target state.
    pub fn apply(&self, world: &mut World) {
        // Build dynamic entity map
        let mut entity_map = HashMap::new();
        {
            let mut query = world.query::<(Entity, &MobjectId)>();
            for (entity, mobj_id) in query.iter(world) {
                entity_map.insert(mobj_id.0, entity);
            }
        }

        // 1. Process removals: Hide/deactivate removed Mobjects
        for obj_id in &self.removals {
            if let Some(&entity) = entity_map.get(obj_id) {
                world.entity_mut(entity).remove::<Visible>();
            }
        }

        // 2. Spawn missing entities first
        for snap in &self.updates {
            entity_map.entry(snap.id).or_insert_with(|| {
                let mut entity = world.spawn((
                    MobjectId(snap.id),
                    snap.transform,
                    Opacity(snap.opacity),
                    snap.render_layer,
                ));
                if snap.has_fill_component {
                    entity.insert(FillBrush(snap.fill.clone()));
                }
                entity.id()
            });
        }

        // 3. Pass 1: hierarchy parenting
        for snap in &self.updates {
            if let Some(&entity) = entity_map.get(&snap.id) {
                if let Some(parent_id) = snap.parent {
                    if let Some(&parent_entity) = entity_map.get(&parent_id) {
                        world.entity_mut(entity).set_parent_in_place(parent_entity);
                    }
                } else {
                    world.entity_mut(entity).remove_parent_in_place();
                }
            }
        }

        // 4. Pass 2: Overwrite components with correct snapshot values
        for snap in &self.updates {
            if let Some(&entity) = entity_map.get(&snap.id) {
                let mut entity_mut = world.entity_mut(entity);
                insert_snapshot_components(&mut entity_mut, snap);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::prelude::{
        BuildChildrenTransformExt, Changed, ChildOf, GlobalTransform, Schedule, Transform, Vec3,
        World,
    };
    use std::sync::Arc;

    fn changed_count<T: bevy::prelude::Component>(world: &mut World) -> usize {
        world
            .query_filtered::<bevy::prelude::Entity, Changed<T>>()
            .iter(world)
            .count()
    }

    #[test]
    fn restoring_snapshot_rewinds_fill_level() {
        let mut world = World::new();
        let entity = world
            .spawn((MobjectId(ObjectId::from_parts(1, 1)), FillLevel(0.2)))
            .id();
        let snapshot = WorldSnapshot::capture(&mut world);

        world.get_mut::<FillLevel>(entity).unwrap().0 = 0.9;
        snapshot.restore(&mut world);

        assert_eq!(world.get::<FillLevel>(entity), Some(&FillLevel(0.2)));
    }

    #[test]
    fn restoring_an_identical_svg_like_snapshot_does_not_change_render_components() {
        let mut world = World::new();
        let path = Arc::new(gaanim_core::kurbo::BezPath::from_svg("M0,0 L10,10").unwrap());
        let fill = FillBrush::color(gaanim_core::peniko::Color::from_rgb8(0x2d, 0x7d, 0xff));
        let stroke = StrokeBrush::new(gaanim_core::peniko::Color::BLACK, 1.5);

        // Model 100 `scene.svg()` copies, each with several leaf paths. This
        // deliberately exercises the cache-invalidating components without a
        // brittle wall-clock assertion.
        for copy in 0..100 {
            let group = world
                .spawn((
                    MobjectId(ObjectId::from_parts(copy, 0)),
                    gaanim_scene::GroupMarker,
                ))
                .id();
            for leaf in 1..=4 {
                let child = world
                    .spawn((
                        MobjectId(ObjectId::from_parts(copy, leaf)),
                        Path2D(path.clone()),
                        PathSource(path.clone()),
                        fill.clone(),
                        stroke.clone(),
                    ))
                    .id();
                world.entity_mut(child).set_parent_in_place(group);
            }
        }

        let snapshot = WorldSnapshot::capture(&mut world);
        world.clear_trackers();
        snapshot.restore(&mut world);

        assert_eq!(changed_count::<Path2D>(&mut world), 0);
        assert_eq!(changed_count::<PathSource>(&mut world), 0);
        assert_eq!(changed_count::<FillBrush>(&mut world), 0);
        assert_eq!(changed_count::<StrokeBrush>(&mut world), 0);
    }

    #[test]
    fn restoring_an_identical_hierarchy_reconciles_native_transforms() {
        let mut world = World::new();
        let parent = world
            .spawn((
                MobjectId(ObjectId::from_parts(10, 0)),
                Transform::from_translation(Vec3::new(10.0, 0.0, 0.0)),
                GlobalTransform::from(Transform::from_translation(Vec3::new(10.0, 0.0, 0.0))),
            ))
            .id();
        let child = world
            .spawn((
                MobjectId(ObjectId::from_parts(11, 0)),
                Transform::from_translation(Vec3::ZERO),
                GlobalTransform::from(Transform::from_translation(Vec3::new(15.0, 0.0, 0.0))),
            ))
            .id();
        world.entity_mut(child).set_parent_in_place(parent);
        let snapshot = WorldSnapshot::capture(&mut world);

        world
            .entity_mut(child)
            .insert(Transform::from_translation(Vec3::splat(99.0)));
        snapshot.restore(&mut world);

        assert_eq!(
            world.get::<Transform>(child).unwrap().translation,
            Vec3::new(5.0, 0.0, 0.0),
            "restoring an unchanged ChildOf must still reconcile Bevy Transform from GlobalTransform"
        );
    }

    #[test]
    fn restoring_an_identical_snapshot_marks_local_state_for_propagation() {
        let mut world = World::new();
        world.spawn((
            MobjectId(ObjectId::from_parts(12, 0)),
            SpatialTransform::default(),
            Opacity::default(),
        ));
        let snapshot = WorldSnapshot::capture(&mut world);

        world.clear_trackers();
        snapshot.restore(&mut world);

        assert_eq!(changed_count::<SpatialTransform>(&mut world), 1);
        assert_eq!(changed_count::<Opacity>(&mut world), 1);
    }

    #[test]
    fn restoring_a_different_snapshot_updates_components_and_hierarchy() {
        let mut world = World::new();
        let parent_id = ObjectId::from_parts(1, 0);
        let child_id = ObjectId::from_parts(1, 1);
        let parent = world.spawn(MobjectId(parent_id)).id();
        let child = world
            .spawn((
                MobjectId(child_id),
                Path2D(Arc::new(
                    gaanim_core::kurbo::BezPath::from_svg("M0,0 L1,1").unwrap(),
                )),
                PathSource(Arc::new(
                    gaanim_core::kurbo::BezPath::from_svg("M0,0 L1,1").unwrap(),
                )),
                FillBrush::color(gaanim_core::peniko::Color::from_rgb8(0xff, 0x00, 0x00)),
                StrokeBrush::new(gaanim_core::peniko::Color::BLACK, 1.0),
            ))
            .id();
        world.entity_mut(child).set_parent_in_place(parent);
        let snapshot = WorldSnapshot::capture(&mut world);

        let other_parent = world.spawn(MobjectId(ObjectId::from_parts(2, 0))).id();
        world.entity_mut(child).insert((
            Path2D(Arc::new(
                gaanim_core::kurbo::BezPath::from_svg("M0,0 L2,2").unwrap(),
            )),
            PathSource(Arc::new(
                gaanim_core::kurbo::BezPath::from_svg("M0,0 L2,2").unwrap(),
            )),
            FillBrush::color(gaanim_core::peniko::Color::from_rgb8(0x00, 0x00, 0xff)),
            StrokeBrush::new(gaanim_core::peniko::Color::BLACK, 2.0),
        ));
        world.entity_mut(child).set_parent_in_place(other_parent);
        world.clear_trackers();
        snapshot.restore(&mut world);

        assert_eq!(
            world.get::<ChildOf>(child).map(|parent| parent.parent()),
            Some(parent)
        );
        assert_eq!(changed_count::<Path2D>(&mut world), 1);
        assert_eq!(changed_count::<PathSource>(&mut world), 1);
        assert_eq!(changed_count::<FillBrush>(&mut world), 1);
        assert_eq!(changed_count::<StrokeBrush>(&mut world), 1);
        assert_eq!(changed_count::<ChildOf>(&mut world), 1);
    }

    #[test]
    fn restoring_an_unstyled_group_does_not_clear_child_fills() {
        let mut world = World::new();
        let group_id = ObjectId::from_parts(1, 1);
        let child_id = ObjectId::from_parts(2, 1);
        let gold = gaanim_core::peniko::Color::from_rgb8(0xff, 0xd7, 0x00);

        let group = world
            .spawn((MobjectId(group_id), gaanim_scene::GroupMarker))
            .id();
        let child = world
            .spawn((MobjectId(child_id), FillBrush::color(gold)))
            .id();
        world.entity_mut(child).set_parent_in_place(group);

        let snapshot = WorldSnapshot::capture(&mut world);
        assert!(!snapshot.entities[&group_id].has_fill_component);

        // Simulate the pre-fix replay state that injected a transparent fill
        // into a group before restoring the timeline snapshot.
        world.entity_mut(group).insert(FillBrush::transparent());
        snapshot.restore(&mut world);

        assert!(world.get::<FillBrush>(group).is_none());

        let expected = FillBrush::color(gold);
        let mut schedule = Schedule::default();
        schedule.add_systems(gaanim_scene::systems::style_propagation_system);
        schedule.run(&mut world);
        assert_eq!(world.get::<FillBrush>(child), Some(&expected));
    }

    #[test]
    fn restoring_a_snapshot_resets_write_tip_progress() {
        let mut world = World::new();
        let id = ObjectId::from_parts(3, 1);
        let entity = world
            .spawn((
                MobjectId(id),
                gaanim_animation::WriteTipGlow {
                    completion: 0.0,
                    ..Default::default()
                },
            ))
            .id();
        let snapshot = WorldSnapshot::capture(&mut world);

        world
            .get_mut::<gaanim_animation::WriteTipGlow>(entity)
            .unwrap()
            .completion = 0.5;
        snapshot.restore(&mut world);

        assert_eq!(
            world
                .get::<gaanim_animation::WriteTipGlow>(entity)
                .unwrap()
                .completion,
            0.0,
            "seeking before a Write clip must not retain its previous partial glow"
        );
    }

    #[test]
    fn snapshot_restores_complete_authored_camera_state() {
        let mut world = World::new();
        let mut authored = gaanim_math::Camera::perspective_3d(1920, 1080, 0.9);
        authored
            .look_at(
                gaanim_core::glam::DVec3::new(7.0, 4.0, 9.0),
                gaanim_core::glam::DVec3::new(1.0, 2.0, -3.0),
                gaanim_core::glam::DVec3::Y,
            )
            .unwrap();
        world.insert_resource(authored);
        let snapshot = WorldSnapshot::capture(&mut world);

        let mut changed = gaanim_math::Camera::ortho_2d(320, 240);
        changed.position = gaanim_core::glam::DVec3::splat(42.0);
        changed.target = gaanim_core::glam::DVec3::X;
        changed.up = gaanim_core::glam::DVec3::Z;
        world.insert_resource(changed);
        snapshot.restore(&mut world);

        assert_eq!(*world.resource::<gaanim_math::Camera>(), authored);
    }
}
