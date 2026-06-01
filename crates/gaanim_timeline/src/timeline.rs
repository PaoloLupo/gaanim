use std::collections::BTreeMap;
use bevy::prelude::{Entity, Resource, World};
use ordered_float::OrderedFloat;
use slotmap::SlotMap;

use crate::clip::{Clip, ClipId, ClipPayload, PropertyLensSpec, Track, TrackId};
use crate::snapshot::WorldSnapshot;
use gaanim_math::SpatialTransform;
use gaanim_scene::{FillBrush, Opacity, StrokeBrush};

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
        })
    }

    /// Adds a clip to the timeline under a specific track and time interval.
    pub fn add_clip(&mut self, track: TrackId, start: f64, duration: f64, payload: ClipPayload) -> ClipId {
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

        // Recompute max duration and total duration if needed
        self.recompute_bounds();

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
                    && clip.end() > time {
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
    pub fn seek(&mut self, world: &mut World, target_time: f64) {
        let max_time = self.loop_range.map(|(_, end)| end).unwrap_or(self.cached_duration);
        self.current_time = target_time.clamp(0.0, max_time);

        // 1. Locate the nearest recorded keyframe <= target_time
        let keyframe = self.keyframes
            .range(..=OrderedFloat(self.current_time))
            .next_back();

        let kf_start_time = if let Some((&kf_time, snapshot)) = keyframe {
            // Restore snapshot state
            snapshot.restore(world);
            kf_time.0
        } else {
            // No keyframe found: keep active Mobjects visible as default baseline
            0.0
        };

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
                && let Some(&target_entity) = entity_map.get(&anim.target) {
                    if clip.end() <= self.current_time {
                        // Animation finished before or at seek head: apply final state
                        apply_lens_spec(world, target_entity, &anim.lens, 1.0);
                    } else if clip.start <= self.current_time && clip.end() > self.current_time {
                        // Animation is actively running at seek head: interpolate
                        let progress = ((self.current_time - clip.start) / clip.duration).clamp(0.0, 1.0);
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
            if let Some(mut completion) = world.get_mut::<gaanim_animation::PathCompletion>(target) {
                completion.0 = *from + (*to - *from) * t;
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
                && let gaanim_math::Projection::Orthographic { ref mut zoom } = camera.projection {
                    *zoom = *from + (*to - *from) * t;
                }
        }
        PropertyLensSpec::Custom { .. } => {
            // Custom dynamically-registered extensions are evaluated by normal ECS tween systems.
        }
    }
}


