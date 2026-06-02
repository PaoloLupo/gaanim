use bevy::prelude::{Entity, Resource, World};
use ordered_float::OrderedFloat;
use slotmap::SlotMap;
use std::collections::BTreeMap;

use crate::clip::{Clip, ClipId, ClipPayload, PropertyLensSpec, Track, TrackId};
use crate::snapshot::WorldSnapshot;
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
        })
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
            let same_kf_and_forward = self.last_restore_kf_time == Some(kf_time)
                && clamped_target >= self.current_time;
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
        for clip in candidate_clips {
            if let ClipPayload::Animation(ref anim) = clip.payload
                && let Some(&target_entity) = entity_map.get(&anim.target)
            {
                if clip.end() <= self.current_time {
                    // Animation finished before or at seek head: apply final state.
                    // Use rate_func.evaluate(1.0) rather than hard-coding 1.0 so that
                    // round-trip functions like ThereAndBack correctly restore the baseline.
                    let final_t = anim.rate_func.evaluate(1.0);
                    apply_lens_spec(world, target_entity, &anim.lens, final_t);
                } else if clip.start <= self.current_time && clip.end() > self.current_time {
                    // Animation is actively running at seek head: interpolate
                    let progress =
                        ((self.current_time - clip.start) / clip.duration).clamp(0.0, 1.0);
                    let t = anim.rate_func.evaluate(progress);
                    apply_lens_spec(world, target_entity, &anim.lens, t);
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
        PropertyLensSpec::Custom { .. } => {
            // Custom dynamically-registered extensions are evaluated by normal ECS tween systems.
        }
    }
}
