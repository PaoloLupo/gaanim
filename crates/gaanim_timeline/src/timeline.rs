use bevy::prelude::{BuildChildrenTransformExt, Entity, Resource, World};
use ordered_float::OrderedFloat;
use slotmap::SlotMap;
use std::collections::BTreeMap;

use crate::clip::{Clip, ClipId, ClipPayload, PropertyLensSpec, SceneId, Track, TrackId};
use crate::scene::{SceneMember, SceneMetadata};
use crate::snapshot::WorldSnapshot;
use crate::transition::{SceneConnection, TransitionType};
use gaanim_math::SpatialTransform;
use gaanim_scene::{FillBrush, Opacity, Path2D, StrokeBrush};

/// The primary Timeline manager, stored as a Bevy ECS resource.
///
/// It coordinates tracks, clips, keyframes, and seek operations,
/// using a B-Tree index to achieve highly performant O(log n) seeks.
#[derive(Resource, Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Timeline {
    /// Arena collection of tracks.
    pub tracks: SlotMap<TrackId, Track>,
    /// Arena collection of clips.
    pub clips: SlotMap<ClipId, Clip>,
    /// O(log n) index mapping clip start times to clip IDs.
    pub clip_index: BTreeMap<OrderedFloat<f64>, Vec<ClipId>>,
    /// Snapshots captured at strategic keyframe timestamps for fast delta replay seeks.
    pub keyframes: BTreeMap<OrderedFloat<f64>, WorldSnapshot>,
    /// Current playback head location in seconds.
    pub current_time: f64,
    /// Cached maximum clip duration, used to bound range queries during seek.
    pub max_clip_duration: f64,
    /// Cached total timeline duration.
    pub cached_duration: f64,
    /// Playback state indicator.
    pub is_playing: bool,
    /// Playback speed multiplier (e.g. 1.0 for real-time).
    pub playback_rate: f64,
    /// Interactive slide presentation breakpoints.
    pub breakpoints: Vec<f64>,
    /// Active loop range (start_time, end_time) if loop playback is enabled.
    pub loop_range: Option<(f64, f64)>,
    /// Pending seek request, processed at the end of the frame using exclusive world access.
    #[cfg_attr(feature = "serde", serde(skip))]
    pub seek_request: Option<f64>,
    /// The keyframe time last used as a restore base.
    ///
    /// When seeking forward within the same keyframe interval, the full snapshot
    /// restore can be skipped because clip replay (which uses explicit `from`/`to`
    /// values) produces the same deterministic result regardless of prior entity state.
    #[cfg_attr(feature = "serde", serde(skip))]
    pub last_restore_kf_time: Option<OrderedFloat<f64>>,
    /// Arena of scene metadata for multi-scene timelines.
    pub scenes: SlotMap<SceneId, SceneMetadata>,
    /// Index mapping scene start times to scene IDs for O(log n) lookup.
    pub scene_index: BTreeMap<OrderedFloat<f64>, SceneId>,
    /// Ordered list of connections between scenes.
    pub scene_connections: Vec<SceneConnection>,
}

impl Default for Timeline {
    fn default() -> Self {
        Self {
            tracks: SlotMap::with_key(),
            clips: SlotMap::with_key(),
            clip_index: BTreeMap::new(),
            keyframes: BTreeMap::new(),
            current_time: 0.0,
            max_clip_duration: 0.0,
            cached_duration: 0.0,
            is_playing: false,
            playback_rate: 1.0,
            breakpoints: Vec::new(),
            loop_range: None,
            seek_request: None,
            last_restore_kf_time: None,
            scenes: SlotMap::with_key(),
            scene_index: BTreeMap::new(),
            scene_connections: Vec::new(),
        }
    }
}

impl Timeline {
    /// Creates a new empty `Timeline`.
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a new track to the timeline.
    pub fn add_track(&mut self, name: impl Into<String>, order: i32) -> TrackId {
        self.tracks.insert_with_key(|id| Track {
            id,
            name: name.into(),
            order,
            object_id: None,
            scene: None,
        })
    }

    /// Adds a new scene to the timeline and returns its ID.
    pub fn add_scene(&mut self, name: &str) -> SceneId {
        let scene_id = self.scenes.insert_with_key(|id| SceneMetadata {
            id,
            name: name.to_string(),
            tracks: Vec::new(),
            camera_override: None,
            background_override: None,
        });
        scene_id
    }

    /// Returns the scene active at the given timestamp, if any.
    ///
    /// Uses the `scene_index` BTreeMap (populated by `begin_scene`) for O(log n)
    /// lookup. Returns the scene whose start time is latest but ≤ `time`.
    pub fn scene_at(&self, time: f64) -> Option<SceneId> {
        let time_key = OrderedFloat(time);
        self.scene_index
            .range(..=time_key)
            .next_back()
            .map(|(_, &id)| id)
    }

    /// Records a scene's start time in the `scene_index` for fast lookup by `scene_at()`.
    pub fn index_scene(&mut self, scene_id: SceneId, start_time: f64) {
        self.scene_index.insert(OrderedFloat(start_time), scene_id);
    }

    /// Returns the (start, end) time bounds for a scene, computed from its clips.
    pub fn scene_bounds(&self, id: SceneId) -> Option<(f64, f64)> {
        let mut start: Option<f64> = None;
        let mut end: Option<f64> = None;

        for clip in self.clips.values() {
            match &clip.payload {
                ClipPayload::SceneStart(sid) if *sid == id => {
                    start = Some(clip.start);
                }
                ClipPayload::SceneEnd(sid) if *sid == id => {
                    end = Some(clip.start);
                }
                _ => {}
            }
        }

        match (start, end) {
            (Some(s), Some(e)) => Some((s, e)),
            _ => None,
        }
    }

    /// Records a connection between two scenes with a transition.
    pub fn connect(&mut self, from: SceneId, to: SceneId, transition: TransitionType) {
        let from_end = self
            .scene_bounds(from)
            .map(|(_, e)| e)
            .unwrap_or(self.current_time);
        let duration = transition.duration();
        self.scene_connections.push(SceneConnection {
            from,
            to,
            transition: transition.clone(),
        });
        let track = self.tracks.keys().next().unwrap();
        self.add_clip(
            track,
            from_end,
            duration,
            ClipPayload::Transition {
                from,
                to,
                transition_type: transition,
            },
        );
    }

    /// Connects a sequence of scenes with the same transition type.
    pub fn sequence(&mut self, scenes: &[SceneId], default_transition: TransitionType) {
        for window in scenes.windows(2) {
            self.connect(window[0], window[1], default_transition.clone());
        }
    }

    /// Adds a clip to the timeline under a specific track and time interval.
    pub fn add_clip(
        &mut self,
        track: TrackId,
        start: f64,
        duration: f64,
        payload: ClipPayload,
    ) -> ClipId {
        let clip_id = self.clips.insert_with_key(|id| Clip {
            id,
            track,
            start,
            duration,
            payload,
        });

        // Insert into the B-Tree index for O(log n) lookup
        let start_key = OrderedFloat(start);
        self.clip_index.entry(start_key).or_default().push(clip_id);

        // Update cached bounds
        if duration > self.max_clip_duration {
            self.max_clip_duration = duration;
        }

        let end_time = start + duration;
        if end_time > self.cached_duration {
            self.cached_duration = end_time;
        }

        clip_id
    }

    /// Removes a clip from the timeline.
    pub fn remove_clip(&mut self, id: ClipId) -> Option<Clip> {
        let clip = self.clips.remove(id)?;

        // Remove from index
        let start_key = OrderedFloat(clip.start);
        if let Some(ids) = self.clip_index.get_mut(&start_key) {
            ids.retain(|&x| x != id);
            if ids.is_empty() {
                self.clip_index.remove(&start_key);
            }
        }

        // Update cached bounds incrementally: only recompute when the removed
        // clip defined one of the extremes. In the common case (removing a
        // non-max clip) this is O(1) instead of O(n).
        let removed_end = clip.start + clip.duration;
        if clip.duration >= self.max_clip_duration || removed_end >= self.cached_duration {
            self.recompute_bounds();
        }

        Some(clip)
    }

    /// Registers a world state snapshot as a seek keyframe at the specified timestamp.
    pub fn add_keyframe(&mut self, time: f64, snapshot: WorldSnapshot) {
        self.keyframes.insert(OrderedFloat(time), snapshot);
    }

    /// Removes a keyframe from the timeline.
    pub fn remove_keyframe(&mut self, time: f64) -> Option<WorldSnapshot> {
        self.keyframes.remove(&OrderedFloat(time))
    }

    /// Recomputes cached bounds (max clip duration and total timeline duration).
    pub fn recompute_bounds(&mut self) {
        self.max_clip_duration = 0.0;
        self.cached_duration = 0.0;

        for clip in self.clips.values() {
            if clip.duration > self.max_clip_duration {
                self.max_clip_duration = clip.duration;
            }
            let end_time = clip.start + clip.duration;
            if end_time > self.cached_duration {
                self.cached_duration = end_time;
            }
        }
    }

    /// Rebuilds the `clip_index` BTreeMap from scratch after direct modification
    /// of clip start times (e.g. during editor drag/resize operations).
    pub fn rebuild_clip_index(&mut self) {
        self.clip_index.clear();
        for clip in self.clips.values() {
            let start_key = OrderedFloat(clip.start);
            self.clip_index.entry(start_key).or_default().push(clip.id);
        }
    }

    /// Fetches all clips active at the specified timestamp.
    ///
    /// This utilizes `max_clip_duration` to bound the B-Tree range query,
    /// achieving highly optimized performance even with large timelines.
    pub fn active_clips_at(&self, time: f64) -> Vec<&Clip> {
        let mut result = Vec::new();
        let time_key = OrderedFloat(time);

        // A clip is active if: clip.start <= time && clip.start + clip.duration > time
        // Therefore, clip.start must be in the range [time - max_clip_duration, time]
        let lower_bound = OrderedFloat((time - self.max_clip_duration).max(0.0));

        for (_, ids) in self.clip_index.range(lower_bound..=time_key) {
            for &id in ids {
                if let Some(clip) = self.clips.get(id)
                    && clip.end() > time
                {
                    result.push(clip);
                }
            }
        }

        result
    }

    /// Fetches all clips starting within a specific time interval.
    pub fn clips_in_range(&self, start: f64, end: f64) -> Vec<&Clip> {
        let mut result = Vec::new();
        let start_key = OrderedFloat(start);
        let end_key = OrderedFloat(end);

        for (_, ids) in self.clip_index.range(start_key..=end_key) {
            for &id in ids {
                if let Some(clip) = self.clips.get(id) {
                    result.push(clip);
                }
            }
        }

        // Ensure deterministic order
        result.sort_by(|a, b| a.start.partial_cmp(&b.start).unwrap());
        result
    }

    /// Performs a random-access seek on the entire Bevy `World`, jumping instantly to `target_time`.
    ///
    /// This restores the closest keyframe snapshot before `target_time` and replays all subsequent
    /// animations in correct temporal order up to `target_time`.
    ///
    /// During sequential forward playback (the common case), the full snapshot restore is
    /// skipped because each clip stores explicit `from`/`to` values, making clip replay
    /// deterministic and independent of prior entity state. This avoids cloning and writing
    /// all entity components every frame.
    pub fn seek(&mut self, world: &mut World, target_time: f64) {
        let max_time = self
            .loop_range
            .map(|(_, end)| end)
            .unwrap_or(self.cached_duration);
        let clamped_target = target_time.clamp(0.0, max_time);

        // 1. Locate the nearest recorded keyframe <= target_time
        let keyframe = self
            .keyframes
            .range(..=OrderedFloat(clamped_target))
            .next_back();

        let kf_start_time = if let Some((&kf_time, snapshot)) = keyframe {
            // Skip full restore when seeking forward within the same keyframe interval:
            // clip replay is deterministic and produces correct state from any baseline.
            let same_kf_and_forward =
                self.last_restore_kf_time == Some(kf_time) && clamped_target >= self.current_time;
            if !same_kf_and_forward {
                snapshot.restore(world);
                self.last_restore_kf_time = Some(kf_time);
            }
            kf_time.0
        } else {
            self.last_restore_kf_time = None;
            // No keyframe found: keep active Mobjects visible as default baseline
            0.0
        };

        self.current_time = clamped_target;

        // 2. Fetch all clips starting within [kf_start_time, target_time]
        let candidate_clips = self.clips_in_range(kf_start_time, self.current_time);

        // Map ObjectIds to current Bevy Entities dynamically
        let mut entity_map = std::collections::HashMap::new();
        let mut query = world.query::<(Entity, &gaanim_scene::MobjectId)>();
        for (entity, mobj_id) in query.iter(world) {
            entity_map.insert(mobj_id.0, entity);
        }

        // 3. Replay and interpolate clip properties up to target_time
        //    Also track active transitions for scene visibility.
        let mut active_transition: Option<(
            &crate::transition::TransitionType,
            f64,
            SceneId,
            SceneId,
        )> = None;

        for clip in candidate_clips {
            match clip.payload {
                ClipPayload::Animation(ref anim) => {
                    if let Some(&target_entity) = entity_map.get(&anim.target) {
                        if clip.end() <= self.current_time {
                            // Animation finished before or at seek head: apply final state.
                            let final_t = anim.rate_func.evaluate(1.0);
                            apply_lens_spec(world, target_entity, &anim.lens, final_t);
                        } else if clip.start <= self.current_time && clip.end() > self.current_time
                        {
                            // Animation is actively running at seek head: interpolate
                            let progress =
                                ((self.current_time - clip.start) / clip.duration).clamp(0.0, 1.0);
                            let t = anim.rate_func.evaluate(progress);
                            apply_lens_spec(world, target_entity, &anim.lens, t);
                        }
                    }
                }
                ClipPayload::SceneStart(_) | ClipPayload::SceneEnd(_) => {
                    // Scene boundary markers — visibility is handled in the post-pass below.
                }
                ClipPayload::Transition {
                    ref transition_type,
                    from,
                    to,
                } => {
                    if clip.start <= self.current_time && clip.end() > self.current_time {
                        let progress = if clip.duration > 0.0 {
                            ((self.current_time - clip.start) / clip.duration).clamp(0.0, 1.0)
                        } else {
                            1.0
                        };
                        active_transition = Some((transition_type, progress, from, to));
                    }
                }
                ClipPayload::Ungroup {
                    group,
                    ref children,
                    group_parent,
                    ref group_transform,
                    ref children_world_transforms,
                } => {
                    if clip.start <= self.current_time {
                        if let Some(&group_entity) = entity_map.get(&group) {
                            // Read the group's ACTUAL current transform (which includes
                            // animations) instead of the initial transform stored in the clip.
                            let actual_group_transform = world
                                .get::<SpatialTransform>(group_entity)
                                .copied()
                                .unwrap_or(*group_transform);
                            let g_affine = actual_group_transform.to_affine_2d();

                            for child_id in children {
                                if let Some(&child_entity) = entity_map.get(child_id)
                                    && let Ok(mut child_mut) = world.get_entity_mut(child_entity)
                                {
                                    let child_local_transform = child_mut
                                        .get::<SpatialTransform>()
                                        .copied()
                                        .unwrap_or_default();
                                    let child_world_affine =
                                        g_affine * child_local_transform.to_affine_2d();
                                    let child_world_transform =
                                        SpatialTransform::from_affine_2d(&child_world_affine);

                                    child_mut.remove_parent_in_place();
                                    child_mut.insert(child_world_transform);

                                    if let Some(gp) = group_parent
                                        && let Some(&gp_entity) = entity_map.get(&gp)
                                    {
                                        child_mut.set_parent_in_place(gp_entity);
                                    }
                                }
                            }

                            if let Ok(group_mut) = world.get_entity_mut(group_entity) {
                                group_mut.despawn();
                            }
                        } else {
                            // Group entity already despawned (from a previous seek).
                            // Re-apply stored world transforms to prevent stale local-space
                            // animation clips from overwriting the ungrouped positions.
                            for &(child_id, world_transform) in children_world_transforms {
                                if let Some(&child_entity) = entity_map.get(&child_id) {
                                    if let Ok(mut child_mut) = world.get_entity_mut(child_entity) {
                                        child_mut.remove_parent_in_place();
                                        child_mut.insert(world_transform);

                                        if let Some(gp) = group_parent
                                            && let Some(&gp_entity) = entity_map.get(&gp)
                                        {
                                            child_mut.set_parent_in_place(gp_entity);
                                        }
                                    }
                                }
                            }
                        }
                    } else {
                        // current_time < clip.start: regroup — restore the group entity
                        let group_exists = entity_map.contains_key(&group);
                        let group_entity = if group_exists {
                            entity_map[&group]
                        } else {
                            world
                                .spawn((
                                    gaanim_scene::GroupMarker,
                                    gaanim_scene::MobjectId(group),
                                    *group_transform,
                                    gaanim_math::GlobalSpatialTransform::from_local(
                                        group_transform,
                                    ),
                                    Opacity(1.0),
                                    gaanim_scene::GlobalOpacity(1.0),
                                    gaanim_scene::RenderOrder::default(),
                                    gaanim_scene::Visible,
                                    FillBrush::transparent(),
                                    StrokeBrush::transparent(),
                                ))
                                .id()
                        };

                        // Use the same transform the group had at ungroup time.
                        // If the group entity still exists (was re-created by a forward
                        // seek), read its actual transform; otherwise fall back to the
                        // stored initial transform.
                        let regroup_transform = world
                            .get::<SpatialTransform>(group_entity)
                            .copied()
                            .unwrap_or(*group_transform);
                        let inv_g = regroup_transform.to_affine_2d().inverse();

                        for child_id in children {
                            if let Some(&child_entity) = entity_map.get(child_id)
                                && let Ok(mut child_mut) = world.get_entity_mut(child_entity)
                            {
                                let child_current = child_mut
                                    .get::<SpatialTransform>()
                                    .copied()
                                    .unwrap_or_default();
                                let child_local_affine = inv_g * child_current.to_affine_2d();
                                let child_local =
                                    SpatialTransform::from_affine_2d(&child_local_affine);

                                child_mut.remove_parent_in_place();
                                child_mut.insert(child_local);
                                child_mut.set_parent_in_place(group_entity);
                            }
                        }

                        if let Some(gp) = group_parent
                            && let Some(&gp_entity) = entity_map.get(&gp)
                        {
                            world
                                .entity_mut(group_entity)
                                .set_parent_in_place(gp_entity);
                        }
                    }
                }
                _ => {}
            }
        }

        // 4. Scene visibility post-pass.
        //    Determine which scene is active and toggle visibility on SceneMember entities.
        if !self.scenes.is_empty() {
            let active_scene = self.scene_at(self.current_time);

            // Collect entities with SceneMember to avoid borrow conflicts
            let scene_entities: Vec<(Entity, SceneId)> = {
                let mut q = world.query::<(Entity, &SceneMember)>();
                q.iter(world).map(|(e, sm)| (e, sm.0)).collect()
            };

            // Determine which scenes should be visible
            let visible_scenes: std::collections::HashSet<SceneId> =
                if let Some((_, _, from, to)) = active_transition {
                    // During a transition, BOTH scenes are visible
                    [from, to].into_iter().collect()
                } else if let Some(scene) = active_scene {
                    std::iter::once(scene).collect()
                } else {
                    std::collections::HashSet::new()
                };

            // Apply transition effects if active (before visibility toggle)
            if let Some((ref transition_type, t, from, to)) = active_transition {
                apply_transition(world, &scene_entities, transition_type, t, from, to);
            }

            // Toggle visibility: entities belonging to non-visible scenes get hidden
            for (entity, scene_id) in &scene_entities {
                if visible_scenes.contains(scene_id) {
                    if world.get::<gaanim_scene::Visible>(*entity).is_none() {
                        if let Ok(mut em) = world.get_entity_mut(*entity) {
                            em.insert(gaanim_scene::Visible);
                        }
                    }
                } else {
                    if world.get::<gaanim_scene::Visible>(*entity).is_some() {
                        if let Ok(mut em) = world.get_entity_mut(*entity) {
                            em.remove::<gaanim_scene::Visible>();
                        }
                    }
                }
            }
        }
    }
}

/// Helper function to evaluate and apply a PropertyLensSpec to an entity.
fn apply_lens_spec(world: &mut World, target: Entity, lens: &PropertyLensSpec, t: f64) {
    match lens {
        PropertyLensSpec::Translation { from, to } => {
            if let Some(mut transform) = world.get_mut::<SpatialTransform>(target) {
                transform.translation = from.lerp(*to, t);
            }
        }
        PropertyLensSpec::Rotation { from, to } => {
            if let Some(mut transform) = world.get_mut::<SpatialTransform>(target) {
                transform.rotation = from.slerp(*to, t);
            }
        }
        PropertyLensSpec::Scale { from, to } => {
            if let Some(mut transform) = world.get_mut::<SpatialTransform>(target) {
                transform.scale = from.lerp(*to, t);
            }
        }
        PropertyLensSpec::Opacity { from, to } => {
            if let Some(mut opacity) = world.get_mut::<Opacity>(target) {
                opacity.0 = *from + (*to - *from) * t as f32;
            }
        }
        PropertyLensSpec::FillColor { from, to } => {
            if let Some(mut fill) = world.get_mut::<FillBrush>(target) {
                let c = gaanim_core::interpolate_color(*from, *to, t);
                *fill = FillBrush(Some(gaanim_core::peniko::Brush::Solid(c)));
            }
        }
        PropertyLensSpec::StrokeColor { from, to } => {
            if let Some(mut stroke) = world.get_mut::<StrokeBrush>(target) {
                let c = gaanim_core::interpolate_color(*from, *to, t);
                stroke.brush = Some(gaanim_core::peniko::Brush::Solid(c));
            }
        }
        PropertyLensSpec::StrokeWidth { from, to } => {
            if let Some(mut stroke) = world.get_mut::<StrokeBrush>(target) {
                stroke.style.width = *from + (*to - *from) * t;
            }
        }
        PropertyLensSpec::PathCompletion { from, to } => {
            let completion = *from + (*to - *from) * t;

            // Safety net: if the `PathSource` seed system didn't run
            // (timing race between Startup and the first Update frame,
            // keyframe restore, etc.) the lens would otherwise be a
            // no-op and the full path would stay visible the whole
            // time.
            if world.get::<gaanim_animation::PathSource>(target).is_none() {
                let path_clone = world.get::<Path2D>(target).map(|p| p.0.clone());
                if let Some(bez) = path_clone
                    && let Ok(mut em) = world.get_entity_mut(target)
                {
                    em.insert(gaanim_animation::PathSource(bez));
                }
            }

            if completion >= 1.0 {
                // Full path: assign source directly.
                // Avoids get_subpath's internal clone at alpha=1.0.
                if let Some(source) = world.get::<gaanim_animation::PathSource>(target) {
                    let full = source.0.clone();
                    if let Some(mut path) = world.get_mut::<Path2D>(target) {
                        path.0 = full;
                    }
                }
            } else if let Some(source) = world.get::<gaanim_animation::PathSource>(target) {
                // Take by reference instead of cloning — get_subpath
                // already returns an owned trimmed BezPath.
                let trimmed = gaanim_math::get_subpath(&source.0, completion);
                if let Some(mut path) = world.get_mut::<Path2D>(target) {
                    path.0 = std::sync::Arc::new(trimmed);
                }
            }
        }
        PropertyLensSpec::FillDrawProgress { from, to } => {
            // Insert/update the `FillDrawProgress` component. The
            // renderer reads it to modulate the fill brush's color
            // alpha, producing the cross-fade from outline to fill
            // that the Write animation needs.
            let v = *from + (*to - *from) * t as f32;
            if let Ok(mut em) = world.get_entity_mut(target) {
                em.insert(gaanim_animation::FillDrawProgress(v));
            }
        }
        PropertyLensSpec::CameraPosition { from, to } => {
            if let Some(mut camera) = world.get_resource_mut::<gaanim_math::Camera>() {
                camera.position = from.lerp(*to, t);
            }
        }
        PropertyLensSpec::CameraRotation { from, to } => {
            if let Some(mut camera) = world.get_resource_mut::<gaanim_math::Camera>() {
                camera.rotation = from.slerp(*to, t);
            }
        }
        PropertyLensSpec::CameraZoom { from, to } => {
            if let Some(mut camera) = world.get_resource_mut::<gaanim_math::Camera>()
                && let gaanim_math::Projection::Orthographic { ref mut zoom } = camera.projection
            {
                *zoom = *from + (*to - *from) * t;
            }
        }
        PropertyLensSpec::PathFollow { path } => {
            // Sample the Bézier path at the eased `t` and set the
            // entity's translation to the sampled world point.
            let p = gaanim_math::get_point_at_alpha(path, t);
            if let Some(mut transform) = world.get_mut::<SpatialTransform>(target) {
                transform.translation = gaanim_core::glam::DVec3::new(p.x, p.y, 0.0);
            }
        }
        PropertyLensSpec::SignalFloat { from, to } => {
            if let Some(mut signal) =
                world.get_mut::<gaanim_animation::signals::FloatSignal>(target)
            {
                signal.value = *from + (*to - *from) * t;
            }
        }
        PropertyLensSpec::PathRange {
            from,
            to,
            time_width,
        } => {
            let p = *from + (*to - *from) * t;
            let start = (p - *time_width).max(0.0);
            let end = p.min(1.0);
            if world.get::<gaanim_animation::PathSource>(target).is_none() {
                let path_clone = world.get::<Path2D>(target).map(|p| p.0.clone());
                if let Some(bez) = path_clone
                    && let Ok(mut em) = world.get_entity_mut(target)
                {
                    em.insert(gaanim_animation::PathSource(bez));
                }
            }
            if let Some(source) = world.get::<gaanim_animation::PathSource>(target) {
                let trimmed = gaanim_math::get_subpath_range(&source.0, start, end);
                if let Some(mut path) = world.get_mut::<Path2D>(target) {
                    path.0 = std::sync::Arc::new(trimmed);
                }
            }
        }
        PropertyLensSpec::Custom { .. } => {
            // Custom dynamically-registered extensions are evaluated by normal ECS tween systems.
        }
    }
}

/// Applies transition effects to scene entities based on the transition type and progress.
///
/// `from` and `to` are the scene IDs involved in the transition.
fn apply_transition(
    world: &mut World,
    scene_entities: &[(Entity, SceneId)],
    transition: &crate::transition::TransitionType,
    t: f64,
    from: SceneId,
    to: SceneId,
) {
    use crate::transition::TransitionType;

    match transition {
        TransitionType::Cut => {
            // Instant cut — no visual effect needed.
        }
        TransitionType::CrossFade { .. } => {
            // Crossfade: from scene fades out (opacity 1→0), to scene fades in (opacity 0→1).
            for (entity, scene_id) in scene_entities {
                let target_opacity = if *scene_id == from {
                    1.0 - t as f32
                } else if *scene_id == to {
                    t as f32
                } else {
                    continue;
                };
                if let Some(mut opacity) = world.get_mut::<Opacity>(*entity) {
                    opacity.0 = target_opacity;
                }
            }
        }
        TransitionType::FadeThrough { fade_color, .. } => {
            // Fade-through: first half fades everything to fade_color, second half reveals to-scene.
            let _ = fade_color;
            for (entity, scene_id) in scene_entities {
                let target_opacity = if t < 0.5 {
                    // First half: fade out both scenes
                    if *scene_id == from {
                        1.0 - (t * 2.0) as f32
                    } else {
                        0.0
                    }
                } else {
                    // Second half: fade in to-scene
                    if *scene_id == to {
                        ((t - 0.5) * 2.0) as f32
                    } else {
                        0.0
                    }
                };
                if let Some(mut opacity) = world.get_mut::<Opacity>(*entity) {
                    opacity.0 = target_opacity;
                }
            }
        }
        TransitionType::Slide { direction, .. } => {
            // Slide: from scene slides out, to scene slides in from opposite side.
            let viewport_width = 1280.0;
            let viewport_height = 720.0;

            for (entity, scene_id) in scene_entities {
                let (dx, dy) = if *scene_id == from {
                    match direction {
                        crate::transition::SlideDirection::Left => (-viewport_width * t, 0.0),
                        crate::transition::SlideDirection::Right => (viewport_width * t, 0.0),
                        crate::transition::SlideDirection::Up => (0.0, viewport_height * t),
                        crate::transition::SlideDirection::Down => (0.0, -viewport_height * t),
                    }
                } else if *scene_id == to {
                    match direction {
                        crate::transition::SlideDirection::Left => {
                            (viewport_width * (1.0 - t), 0.0)
                        }
                        crate::transition::SlideDirection::Right => {
                            (-viewport_width * (1.0 - t), 0.0)
                        }
                        crate::transition::SlideDirection::Up => {
                            (0.0, -viewport_height * (1.0 - t))
                        }
                        crate::transition::SlideDirection::Down => {
                            (0.0, viewport_height * (1.0 - t))
                        }
                    }
                } else {
                    continue;
                };
                if let Some(mut transform) = world.get_mut::<SpatialTransform>(*entity) {
                    transform.translation.x += dx;
                    transform.translation.y += dy;
                }
            }
        }
        TransitionType::ZoomThrough {
            center, max_zoom, ..
        } => {
            let _ = (center, max_zoom);
            if let Some(mut camera) = world.get_resource_mut::<gaanim_math::Camera>() {
                if let gaanim_math::Projection::Orthographic { ref mut zoom } = camera.projection {
                    let zoom_factor = if t < 0.5 {
                        1.0 + (max_zoom - 1.0) * (t * 2.0)
                    } else {
                        max_zoom - (max_zoom - 1.0) * ((t - 0.5) * 2.0)
                    };
                    *zoom = zoom_factor;
                }
            }
        }
        TransitionType::Morph { mappings, .. } => {
            let _ = mappings;
        }
    }
}
