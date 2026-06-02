use bevy::prelude::{BuildChildrenTransformExt, Entity, EntityWorldMut, World};
use gaanim_core::ObjectId;
use gaanim_math::SpatialTransform;
use gaanim_scene::{
    FillBrush, MobjectId, ObjectTag, Opacity, Path2D, PathSource, RenderLayer, RenderOrder,
    StrokeBrush, Visible,
};
use std::collections::HashMap;

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
    pub path2d: Option<gaanim_core::kurbo::BezPath>,
    /// Cached original path for write/unwrite animations.
    pub path_source: Option<gaanim_core::kurbo::BezPath>,
    /// Fill-draw progress for write/unwrite animations (0.0 = outline only, 1.0 = full fill).
    pub fill_draw_progress: Option<f32>,
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
    entity_mut.insert(snap.transform);
    entity_mut.insert(Opacity(snap.opacity));
    entity_mut.insert(FillBrush(snap.fill.clone()));

    if let Some(ref style) = snap.stroke_style {
        entity_mut.insert(StrokeBrush {
            brush: snap.stroke.clone(),
            style: style.clone(),
        });
    } else {
        entity_mut.remove::<StrokeBrush>();
    }

    entity_mut.insert(RenderOrder {
        z_index: snap.render_order,
        creation_order: snap.creation_order,
    });

    entity_mut.insert(snap.render_layer);

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
}

impl WorldSnapshot {
    /// Captures a new `WorldSnapshot` of all Mobjects currently registered in the Bevy `World`.
    pub fn capture(world: &mut World) -> Self {
        let mut entities = HashMap::new();

        // Query all entities with a MobjectId component
        let mut query = world.query::<(
            Entity,
            &MobjectId,
            Option<&bevy::prelude::ChildOf>,
            Option<&SpatialTransform>,
            Option<&Opacity>,
            Option<&FillBrush>,
            Option<&StrokeBrush>,
            Option<&RenderOrder>,
            Option<&RenderLayer>,
            Option<&ObjectTag>,
            Option<&Path2D>,
            Option<&PathSource>,
            Option<&gaanim_animation::FillDrawProgress>,
        )>();

        let mut captured_data = Vec::new();

        for (
            entity,
            mobj_id,
            child_of_opt,
            transform_opt,
            opacity_opt,
            fill_opt,
            stroke_opt,
            render_order_opt,
            render_layer_opt,
            tag_opt,
            path2d_opt,
            path_source_opt,
            fill_draw_progress_opt,
        ) in query.iter(world)
        {
            let obj_id = mobj_id.0;
            // Find parent entity's ObjectId if parent is set
            let parent_entity = child_of_opt.map(|c| c.parent());
            let parent_id =
                parent_entity.and_then(|p| world.get::<MobjectId>(p).copied().map(|m| m.0));

            let transform = transform_opt.copied().unwrap_or_default();
            let opacity = opacity_opt.map(|o| o.0).unwrap_or(1.0);
            let fill = fill_opt.and_then(|f| f.0.clone());
            let stroke = stroke_opt.and_then(|s| s.brush.clone());
            let stroke_style = stroke_opt.map(|s| s.style.clone());
            let render_order = render_order_opt.map(|r| r.z_index).unwrap_or(0);
            let creation_order = render_order_opt.map(|r| r.creation_order).unwrap_or(0);
            let render_layer = render_layer_opt.copied().unwrap_or(RenderLayer::Vello2D);
            let visible = world.get::<Visible>(entity).is_some();

            let mut tags = Vec::new();
            if let Some(tag) = tag_opt {
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
                    stroke,
                    stroke_style,
                    render_order,
                    creation_order,
                    render_layer,
                    visible,
                    tags,
                    path2d: path2d_opt.map(|p| p.0.clone()),
                    path_source: path_source_opt.map(|p| p.0.clone()),
                    fill_draw_progress: fill_draw_progress_opt.map(|p| p.0),
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

        // 3. Restore properties for entities specified in this snapshot
        for (obj_id, snap) in &self.entities {
            if let Some(&entity) = entity_map.get(obj_id) {
                // Scope mutable borrow of entity_mut to release the borrow before hierarchy queries
                {
                    let mut entity_mut = world.entity_mut(entity);
                    insert_snapshot_components(&mut entity_mut, snap);
                }

                if let Some(parent_id) = snap.parent {
                    if let Some(&parent_entity) = entity_map.get(&parent_id) {
                        world.entity_mut(entity).set_parent_in_place(parent_entity);
                    }
                } else {
                    world.entity_mut(entity).remove_parent_in_place();
                }
            } else {
                // Entity was deleted or missing; spawn a new entity with the snapshotted components
                let new_entity = world
                    .spawn((
                        MobjectId(*obj_id),
                        snap.transform,
                        Opacity(snap.opacity),
                        FillBrush(snap.fill.clone()),
                        snap.render_layer,
                    ))
                    .id();

                entity_map.insert(*obj_id, new_entity);

                let mut entity_mut = world.entity_mut(new_entity);
                insert_snapshot_components(&mut entity_mut, snap);

                if let Some(parent_id) = snap.parent
                    && let Some(&parent_entity) = entity_map.get(&parent_id)
                {
                    entity_mut.set_parent_in_place(parent_entity);
                }
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

        // 2. Process updates: Upsert new or modified states
        for snap in &self.updates {
            if let Some(&entity) = entity_map.get(&snap.id) {
                // Scope mutable borrow of entity_mut to release before hierarchy check
                {
                    let mut entity_mut = world.entity_mut(entity);
                    insert_snapshot_components(&mut entity_mut, snap);
                }

                if let Some(parent_id) = snap.parent {
                    if let Some(&parent_entity) = entity_map.get(&parent_id) {
                        world.entity_mut(entity).set_parent_in_place(parent_entity);
                    }
                } else {
                    world.entity_mut(entity).remove_parent_in_place();
                }
            } else {
                // Spawn missing entity
                let new_entity = world
                    .spawn((
                        MobjectId(snap.id),
                        snap.transform,
                        Opacity(snap.opacity),
                        FillBrush(snap.fill.clone()),
                        snap.render_layer,
                    ))
                    .id();

                entity_map.insert(snap.id, new_entity);

                let mut entity_mut = world.entity_mut(new_entity);
                insert_snapshot_components(&mut entity_mut, snap);

                if let Some(parent_id) = snap.parent
                    && let Some(&parent_entity) = entity_map.get(&parent_id)
                {
                    entity_mut.set_parent_in_place(parent_entity);
                }
            }
        }
    }
}
