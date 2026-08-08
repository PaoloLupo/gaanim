use bevy::prelude::{BuildChildrenTransformExt, Entity, EntityWorldMut, World};
use gaanim_core::ObjectId;
use gaanim_math::{GlobalSpatialTransform, SpatialTransform};
use gaanim_scene::{
    FillBrush, GlobalOpacity, LocalBounds, MobjectId, ObjectTag, Opacity, Path2D, PathSource,
    RenderLayer, RenderOrder, StrokeBrush, Visible, WorldBounds,
};
use std::collections::HashMap;

use crate::clip::SceneId;
use crate::scene::SceneMember;

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
    /// Runtime state of a traced path, used to restore scrubbing/replay cleanly.
    pub traced_path_points: Option<Vec<gaanim_core::glam::DVec3>>,
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
}

/// Insert or update all components of an `EntitySnapshot` onto a Bevy entity.
///
/// This helper centralizes the heavy component-insertion logic that was previously
/// duplicated across `restore`, `apply`, and entity spawning paths.
fn insert_snapshot_components(entity_mut: &mut EntityWorldMut<'_>, snap: &EntitySnapshot) {
    // Batch insert always-present components in a single archetype move.
    // This reduces up to ~15 individual inserts to 1 batch + conditional inserts.
    let global_transform = snap
        .global_transform
        .unwrap_or_else(|| GlobalSpatialTransform::from_local(&snap.transform));
    let global_opacity = snap.global_opacity.unwrap_or(GlobalOpacity(snap.opacity));

    entity_mut.insert((
        snap.transform,
        Opacity(snap.opacity),
        RenderOrder {
            z_index: snap.render_order,
            creation_order: snap.creation_order,
        },
        snap.render_layer,
        global_transform,
        global_opacity,
    ));

    if snap.has_fill_component {
        entity_mut.insert(FillBrush(snap.fill.clone()));
    } else {
        entity_mut.remove::<FillBrush>();
    }

    // Handle conditional components (insert or remove)
    if let Some(ref style) = snap.stroke_style {
        entity_mut.insert(StrokeBrush {
            brush: snap.stroke.clone(),
            style: style.clone(),
        });
    } else {
        entity_mut.remove::<StrokeBrush>();
    }

    if snap.visible {
        entity_mut.insert(Visible);
    } else {
        entity_mut.remove::<Visible>();
    }

    if !snap.tags.is_empty() {
        entity_mut.insert(ObjectTag(snap.tags[0].clone()));
    }

    if let Some(ref path) = snap.path2d {
        entity_mut.insert(Path2D(path.clone()));
    } else {
        entity_mut.remove::<Path2D>();
    }

    if let Some(ref path) = snap.path_source {
        entity_mut.insert(PathSource(path.clone()));
    } else {
        entity_mut.remove::<PathSource>();
    }

    if let Some(progress) = snap.fill_draw_progress {
        entity_mut.insert(gaanim_animation::FillDrawProgress(progress));
    } else {
        entity_mut.remove::<gaanim_animation::FillDrawProgress>();
    }

    if let Some(points) = &snap.traced_path_points
        && let Some(mut traced_path) = entity_mut.get_mut::<gaanim_animation::TracedPath>()
    {
        traced_path.points = points.clone();
    }
    // 3D traced path — generic, handles inferno/viridis/plasma colormaps
    if let Some(points) = &snap.traced_path_points {
        if let Some(mut traced_3d) = entity_mut.get_mut::<gaanim_animation::TracedPath3D>() {
            traced_3d.points = points.clone();
        }
        let colormap_opt = entity_mut
            .get::<gaanim_animation::TracedPath3D>()
            .and_then(|t| t.colormap.clone());
        if let Some(mut line) = entity_mut.get_mut::<gaanim_scene::LineListData>() {
            let pts: Vec<[f32; 3]> = points.iter().map(|p| [p.x as f32, p.y as f32, p.z as f32]).collect();
            line.points.clone_from(&pts);
            if let Some(name) = colormap_opt {
                let n = pts.len();
                let mut cols = Vec::with_capacity(n);
                for i in 0..n {
                    let t = if n > 1 { i as f32 / (n - 1) as f32 } else { 0.0 };
                    let (r, g, b) = match name.as_str() {
                        "inferno" => {
                            const PALETTE: [(u8, u8, u8); 10] = [
                                (0, 0, 4), (31, 12, 72), (85, 15, 109), (136, 34, 106), (168, 50, 88),
                                (210, 72, 55), (233, 100, 28), (249, 157, 87), (247, 209, 61), (252, 255, 164),
                            ];
                            let scaled = t * (PALETTE.len() - 1) as f32;
                            let idx = scaled.floor() as usize;
                            let f = scaled - idx as f32;
                            if idx >= PALETTE.len() - 1 {
                                PALETTE[PALETTE.len() - 1]
                            } else {
                                let (r0, g0, b0) = PALETTE[idx];
                                let (r1, g1, b1) = PALETTE[idx + 1];
                                (
                                    (r0 as f32 + (r1 as f32 - r0 as f32) * f) as u8,
                                    (g0 as f32 + (g1 as f32 - g0 as f32) * f) as u8,
                                    (b0 as f32 + (b1 as f32 - b0 as f32) * f) as u8,
                                )
                            }
                        }
                        "viridis" => {
                            const PALETTE: [(u8, u8, u8); 5] = [(68, 1, 84), (59, 82, 139), (33, 144, 140), (94, 201, 98), (253, 231, 37)];
                            let scaled = t * (PALETTE.len() - 1) as f32;
                            let idx = scaled.floor() as usize;
                            let f = scaled - idx as f32;
                            if idx >= PALETTE.len() - 1 { PALETTE[PALETTE.len() - 1] } else {
                                let (r0, g0, b0) = PALETTE[idx];
                                let (r1, g1, b1) = PALETTE[idx + 1];
                                ((r0 as f32 + (r1 as f32 - r0 as f32) * f) as u8, (g0 as f32 + (g1 as f32 - g0 as f32) * f) as u8, (b0 as f32 + (b1 as f32 - b0 as f32) * f) as u8)
                            }
                        }
                        "plasma" => {
                            const PALETTE: [(u8, u8, u8); 5] = [(13, 8, 135), (126, 3, 168), (203, 70, 121), (248, 149, 64), (240, 249, 33)];
                            let scaled = t * (PALETTE.len() - 1) as f32;
                            let idx = scaled.floor() as usize;
                            let f = scaled - idx as f32;
                            if idx >= PALETTE.len() - 1 { PALETTE[PALETTE.len() - 1] } else {
                                let (r0, g0, b0) = PALETTE[idx];
                                let (r1, g1, b1) = PALETTE[idx + 1];
                                ((r0 as f32 + (r1 as f32 - r0 as f32) * f) as u8, (g0 as f32 + (g1 as f32 - g0 as f32) * f) as u8, (b0 as f32 + (b1 as f32 - b0 as f32) * f) as u8)
                            }
                        }
                        _ => (255, 255, 255),
                    };
                    cols.push([r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0, 1.0]);
                }
                line.colors = Some(cols);
            }
        }
    }

    if snap.is_group {
        entity_mut.insert(gaanim_scene::GroupMarker);
    } else {
        entity_mut.remove::<gaanim_scene::GroupMarker>();
    }

    if let Some(lb) = snap.local_bounds {
        entity_mut.insert(lb);
    } else {
        entity_mut.remove::<LocalBounds>();
    }

    if let Some(wb) = snap.world_bounds {
        entity_mut.insert(wb);
    } else {
        entity_mut.remove::<WorldBounds>();
    }

    if let Some(scene_id) = snap.scene {
        entity_mut.insert(SceneMember(scene_id));
    } else {
        entity_mut.remove::<SceneMember>();
    }
}

impl WorldSnapshot {
    /// Captures a new `WorldSnapshot` of all Mobjects currently registered in the Bevy `World`.
    pub fn capture(world: &mut World) -> Self {
        let mut entities = HashMap::new();

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
                    traced_path_points: world
                        .get::<gaanim_animation::TracedPath>(entity)
                        .map(|t| t.points.clone())
                        .or_else(|| {
                            world
                                .get::<gaanim_animation::TracedPath3D>(entity)
                                .map(|t| t.points.clone())
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

        Self { entities }
    }

    /// Restores the states stored in this snapshot back to the Bevy `World`.
    pub fn restore(&self, world: &mut World) {
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
            if !self.entities.contains_key(obj_id) {
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
    use bevy::prelude::{BuildChildrenTransformExt, Schedule, World};

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
}
