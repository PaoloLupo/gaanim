use bevy::animation::AnimationPlayer;
use bevy::prelude::{BuildChildrenTransformExt, ChildOf, Entity, Or, Resource, With, World};
use ordered_float::OrderedFloat;
use slotmap::SlotMap;
use std::collections::{BTreeMap, HashMap, HashSet};

use crate::clip::{
    Clip, ClipId, ClipPayload, GltfAnimationSpec, PropertyLensSpec, SceneId, Track, TrackId,
};
use crate::scene::{SceneMember, SceneMetadata};
use crate::snapshot::WorldSnapshot;
use crate::transition::{SceneConnection, TransitionType};
use gaanim_math::SpatialTransform;
use gaanim_scene::{
    FillBrush, LineListData, LineListSource, MobjectId, Opacity, Path2D, StrokeBrush,
};

/// Whether real-time playback pauses at authored presentation stops.
///
/// Explicit seeks, snapshots, and exports do not consult this policy.
#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PlaybackStopPolicy {
    /// Pause when playback crosses the next authored stop.
    #[default]
    Respect,
    /// Traverse authored stops continuously.
    Ignore,
}

#[derive(Debug, Clone, Default)]
struct ReactiveEntityState {
    updater_elapsed: Option<f64>,
    updater_stop_at: Option<f64>,
    traced_path_points: Option<Vec<gaanim_core::glam::DVec3>>,
    traced_path_sample_times: Option<Vec<f64>>,
    translation: Option<gaanim_core::glam::DVec3>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum AbsoluteLensChannel {
    Translation,
    Rotation,
    Scale,
    Opacity,
    FillColor,
    StrokeColor,
    StrokeWidth,
    PathCompletion,
    PathMorph,
    FillDrawProgress,
    FillLevel,
}

fn absolute_lens_channel(lens: &PropertyLensSpec) -> Option<AbsoluteLensChannel> {
    Some(match lens {
        PropertyLensSpec::Translation { .. } => AbsoluteLensChannel::Translation,
        PropertyLensSpec::Rotation { .. } => AbsoluteLensChannel::Rotation,
        PropertyLensSpec::Scale { .. } => AbsoluteLensChannel::Scale,
        PropertyLensSpec::Opacity { .. } => AbsoluteLensChannel::Opacity,
        PropertyLensSpec::FillColor { .. } => AbsoluteLensChannel::FillColor,
        PropertyLensSpec::StrokeColor { .. } => AbsoluteLensChannel::StrokeColor,
        PropertyLensSpec::StrokeWidth { .. } => AbsoluteLensChannel::StrokeWidth,
        PropertyLensSpec::PathCompletion { .. } => AbsoluteLensChannel::PathCompletion,
        PropertyLensSpec::PathMorph { .. } => AbsoluteLensChannel::PathMorph,
        PropertyLensSpec::FillDrawProgress { .. } => AbsoluteLensChannel::FillDrawProgress,
        PropertyLensSpec::FillLevel { .. } => AbsoluteLensChannel::FillLevel,
        _ => return None,
    })
}

#[cfg(test)]
mod gltf_action_tests {
    use super::*;

    #[test]
    fn sampling_supports_loop_reverse_and_resume_offsets() {
        let base = GltfAnimationSpec {
            target: gaanim_core::ObjectId::from_parts(1, 1),
            animation_index: 0,
            source_duration: 2.0,
            speed: 2.0,
            looped: false,
            reverse: false,
            transition: 0.0,
            start_time: 0.5,
        };
        assert_eq!(sample_gltf_action(&base, 0.25), 1.0);
        assert_eq!(sample_gltf_action(&base, 10.0), 2.0);

        let looped = GltfAnimationSpec {
            looped: true,
            ..base.clone()
        };
        assert!((sample_gltf_action(&looped, 1.25) - 1.0).abs() < 1e-9);

        let reversed = GltfAnimationSpec {
            reverse: true,
            ..base
        };
        assert_eq!(sample_gltf_action(&reversed, 0.25), 1.0);
        assert_eq!(sample_gltf_action(&reversed, 10.0), 0.0);
    }
}

fn sample_gltf_action(spec: &GltfAnimationSpec, elapsed: f64) -> f64 {
    if spec.source_duration <= f64::EPSILON {
        return 0.0;
    }
    let authored = spec.start_time + elapsed * spec.speed;
    let sampled = if spec.looped {
        authored.rem_euclid(spec.source_duration)
    } else {
        authored.clamp(0.0, spec.source_duration)
    };
    if spec.reverse {
        (spec.source_duration - sampled).clamp(0.0, spec.source_duration)
    } else {
        sampled
    }
}

/// A named interactive pause inside a segment.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct SegmentStop {
    pub name: Option<String>,
    pub time: f64,
}

/// Semantic metadata for one authored segment.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct SegmentMetadata {
    pub id: u32,
    pub name: String,
    pub notes: Option<String>,
    pub start_time: f64,
    pub end_time: f64,
    pub stops: Vec<SegmentStop>,
}

/// The current semantic location in the authored segment sequence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct SegmentPosition {
    pub segment_id: u32,
    /// `None` means the segment's initial state; otherwise this is a zero-based stop.
    pub stop_index: Option<usize>,
}

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
    /// Semantic structure compiled from the canvas segments.
    pub segments: Vec<SegmentMetadata>,
    /// Cached segment position matching [`Self::current_time`].
    pub segment_position: Option<SegmentPosition>,
    /// Flag to ignore interactive stop inputs (e.g. when GUI has focus).
    #[cfg_attr(feature = "serde", serde(skip))]
    pub ignore_input: bool,
    /// Active loop range (start_time, end_time) if loop playback is enabled.
    pub loop_range: Option<(f64, f64)>,
    /// Pending seek request, processed at the end of the frame using exclusive world access.
    #[cfg_attr(feature = "serde", serde(skip))]
    pub seek_request: Option<f64>,
    /// The keyframe time last used as a restore base.
    ///
    /// Absolute 2D clip replay can reuse this restored base within the same
    /// keyframe interval; stateful and dynamic timelines always restore again.
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
            segments: Vec::new(),
            segment_position: None,
            ignore_input: false,
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

    /// Replace the semantic segment metadata compiled from the current canvas.
    pub fn set_segments(&mut self, mut segments: Vec<SegmentMetadata>) {
        segments.sort_by(|left, right| left.start_time.total_cmp(&right.start_time));
        for segment in &mut segments {
            segment
                .stops
                .sort_by(|left, right| left.time.total_cmp(&right.time));
        }
        self.segments = segments;
        self.update_segment_position();
    }

    /// Find the semantic segment and most recent stop at `time`.
    pub fn segment_position_at(&self, time: f64) -> Option<SegmentPosition> {
        const EPSILON: f64 = 1e-5;
        let segment = self.terminal_stop_segment_at(time).or_else(|| {
            self.segments.iter().rev().find(|segment| {
                segment.start_time <= time + EPSILON && time <= segment.end_time + EPSILON
            })
        })?;
        let stop_index = segment
            .stops
            .iter()
            .enumerate()
            .rev()
            .find_map(|(index, stop)| (stop.time <= time + EPSILON).then_some(index));
        Some(SegmentPosition {
            segment_id: segment.id,
            stop_index,
        })
    }

    /// Return the timestamp for a semantic position in the segment sequence.
    pub fn segment_time(&self, position: SegmentPosition) -> Option<f64> {
        let segment = self
            .segments
            .iter()
            .find(|segment| segment.id == position.segment_id)?;
        match position.stop_index {
            Some(index) => segment.stops.get(index).map(|stop| stop.time),
            None => Some(segment.start_time),
        }
    }

    /// Resolve a segment by name and return its initial timestamp.
    ///
    /// Names are matched exactly first, then Unicode case-insensitively. This keeps
    /// authored identifiers deterministic while making presenter navigation
    /// forgiving when driven from a search field.
    pub fn segment_time_named(&self, segment_name: &str) -> Option<f64> {
        self.segment_named(segment_name)
            .map(|segment| segment.start_time)
    }

    /// Resolve a named segment and one of its named stops.
    ///
    /// `stop_name` follows the same exact-then-case-insensitive matching rule as
    /// segment names. Unnamed stops intentionally cannot be addressed by this
    /// method; use [`Self::segment_time_indexed`] for those.
    pub fn segment_stop_time_named(&self, segment_name: &str, stop_name: &str) -> Option<f64> {
        let segment = self.segment_named(segment_name)?;
        let normalized_stop_name = stop_name.to_lowercase();
        segment
            .stops
            .iter()
            .find(|stop| stop.name.as_deref() == Some(stop_name))
            .or_else(|| {
                segment.stops.iter().find(|stop| {
                    stop.name
                        .as_deref()
                        .is_some_and(|name| name.to_lowercase() == normalized_stop_name)
                })
            })
            .map(|stop| stop.time)
    }

    /// Resolve a segment and zero-based stop index.
    pub fn segment_time_indexed(
        &self,
        segment_name: &str,
        stop_index: Option<usize>,
    ) -> Option<f64> {
        let segment = self.segment_named(segment_name)?;
        match stop_index {
            Some(index) => segment.stops.get(index).map(|stop| stop.time),
            None => Some(segment.start_time),
        }
    }

    fn segment_named(&self, segment_name: &str) -> Option<&SegmentMetadata> {
        let normalized_name = segment_name.to_lowercase();
        self.segments
            .iter()
            .find(|segment| segment.name == segment_name)
            .or_else(|| {
                self.segments
                    .iter()
                    .find(|segment| segment.name.to_lowercase() == normalized_name)
            })
    }

    /// Return the previous explicit interactive stop before `time`.
    pub fn previous_stop(&self, time: f64) -> Option<f64> {
        self.interactive_stops()
            .into_iter()
            .filter(|stop| *stop < time - 1e-5)
            .last()
    }

    /// Return the next explicit interactive stop after `time`.
    pub fn next_stop(&self, time: f64) -> Option<f64> {
        self.interactive_stops()
            .into_iter()
            .find(|stop| *stop > time + 1e-5)
    }

    /// Return the earliest explicit interactive stop crossed during playback.
    pub(crate) fn next_playback_stop(&self, current: f64, next: f64) -> Option<f64> {
        const EPSILON: f64 = 1e-5;
        self.interactive_stops()
            .into_iter()
            .find(|stop| *stop > current + EPSILON && *stop <= next + EPSILON)
    }

    /// A compact label suitable for editor transport controls.
    pub fn segment_label(&self) -> Option<String> {
        let position = self.segment_position?;
        let index = self
            .segments
            .iter()
            .position(|segment| segment.id == position.segment_id)?;
        let segment = &self.segments[index];
        let stop = position.stop_index.map(|stop| stop + 1).unwrap_or(0);
        Some(if stop == 0 {
            format!("{} / {} · {}", index + 1, self.segments.len(), segment.name)
        } else {
            format!(
                "{} / {} · {} · stop {}",
                index + 1,
                self.segments.len(),
                segment.name,
                stop
            )
        })
    }

    /// Update the cached segment position after a seek or metadata change.
    pub fn update_segment_position(&mut self) {
        self.segment_position = self.segment_position_at(self.current_time);
    }

    fn interactive_stops(&self) -> Vec<f64> {
        let mut stops = self
            .segments
            .iter()
            .flat_map(|segment| segment.stops.iter().map(|stop| stop.time))
            .collect::<Vec<_>>();
        stops.sort_by(|left, right| left.total_cmp(right));
        stops.dedup_by(|left, right| (*left - *right).abs() < 1e-5);
        stops
    }

    fn terminal_stop_segment_at(&self, time: f64) -> Option<&SegmentMetadata> {
        const EPSILON: f64 = 1e-9;
        self.segments.iter().find(|segment| {
            (segment.end_time - time).abs() <= EPSILON
                && segment
                    .stops
                    .iter()
                    .any(|stop| (stop.time - time).abs() <= EPSILON)
        })
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
    /// lookup. A terminal presentation stop keeps its owning scene active at a
    /// shared boundary; otherwise this returns the scene whose start time is
    /// latest but ≤ `time`.
    pub fn scene_at(&self, time: f64) -> Option<SceneId> {
        if let Some(segment) = self.terminal_stop_segment_at(time)
            && let Some(&scene_id) = self.scene_index.get(&OrderedFloat(segment.start_time))
        {
            return Some(scene_id);
        }
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

        // A clip is active if: clip.start <= time && clip.start + clip.duration >= time
        // Therefore, clip.start must be in the range [time - max_clip_duration, time]
        // Note: uses >= to include zero-duration clips at their exact start time.
        let lower_bound = OrderedFloat((time - self.max_clip_duration).max(0.0));

        for (_, ids) in self.clip_index.range(lower_bound..=time_key) {
            for &id in ids {
                if let Some(clip) = self.clips.get(id)
                    && clip.end() >= time
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

    fn can_replay_without_restore(
        &self,
        world: &mut World,
        keyframe_time: OrderedFloat<f64>,
        target_time: f64,
    ) -> bool {
        if self.last_restore_kf_time != Some(keyframe_time) || !self.scenes.is_empty() {
            return false;
        }

        let mut reactive = world.query_filtered::<Entity, Or<(
            With<gaanim_animation::Updater>,
            With<gaanim_animation::SampledSeriesDriver>,
            With<gaanim_animation::TracedPath>,
            With<gaanim_animation::TracedPath3D>,
            With<gaanim_animation::FloatSignal>,
            With<gaanim_animation::SurroundingRect>,
        )>>();
        if reactive.iter(world).next().is_some() {
            return false;
        }

        let upper = self.current_time.max(target_time);
        let mut path_completion_targets = HashSet::new();
        let mut path_morph_targets = HashSet::new();
        for clip in self
            .clips
            .values()
            .filter(|clip| clip.start <= upper && clip.end() >= keyframe_time.0)
        {
            match &clip.payload {
                ClipPayload::Animation(animation) => {
                    let Some(channel) = absolute_lens_channel(&animation.lens) else {
                        return false;
                    };
                    match channel {
                        AbsoluteLensChannel::PathCompletion => {
                            path_completion_targets.insert(animation.target);
                        }
                        AbsoluteLensChannel::PathMorph => {
                            path_morph_targets.insert(animation.target);
                        }
                        _ => {}
                    }
                }
                ClipPayload::Wait
                | ClipPayload::Audio { .. }
                | ClipPayload::Marker(_)
                | ClipPayload::Stop
                | ClipPayload::SegmentStart(_) => {}
                _ => return false,
            }
        }

        path_completion_targets.is_disjoint(&path_morph_targets)
    }

    /// Performs a random-access seek on the entire Bevy `World`, jumping instantly to `target_time`.
    ///
    /// This restores the closest keyframe snapshot before `target_time` and replays all subsequent
    /// animations in correct temporal order up to `target_time`.
    ///
    /// Stateful timelines restore the closest keyframe on every seek. Timelines containing only
    /// absolute 2D property clips may replay from an already-restored keyframe without repeating
    /// the full world restore; every other payload keeps the deterministic restore path.
    pub fn seek(&mut self, world: &mut World, target_time: f64) {
        let max_time = self
            .loop_range
            .map(|(_, end)| end)
            .unwrap_or(self.cached_duration);
        let clamped_target = target_time.clamp(0.0, max_time);
        let previous_time = self.current_time;
        let expected_forward_dt = world
            .get_resource::<gaanim_animation::PlaybackState>()
            .map(|s| s.scaled_dt)
            .unwrap_or(0.0);
        let forward_delta = (clamped_target - previous_time).max(0.0);
        let realtime_forward = expected_forward_dt > 0.0
            && forward_delta <= (expected_forward_dt * 1.5).max(1.0 / 120.0);
        // Exporters and paused scrubbing drive the timeline through explicit
        // monotonic seeks. Preserve the already reconstructed simulation for
        // nearby frames and advance it by the exact seek delta instead of
        // replaying from t=0 for every exported frame.
        let explicit_forward =
            expected_forward_dt <= 0.0 && forward_delta > 0.0 && forward_delta <= 0.25;
        let preserve_reactive_state =
            clamped_target >= previous_time && (realtime_forward || explicit_forward);
        let advance_reactive_during_seek = preserve_reactive_state && explicit_forward;
        let reactive_state = if preserve_reactive_state {
            capture_reactive_state(world)
        } else {
            HashMap::new()
        };

        // 1. Locate the nearest recorded keyframe <= target_time
        let keyframe_time = self
            .keyframes
            .range(..=OrderedFloat(clamped_target))
            .next_back()
            .map(|(&time, _)| time);

        let mut restored_entity_map = None;
        let mut replay_without_restore = false;
        let kf_start_time = if let Some(kf_time) = keyframe_time {
            replay_without_restore =
                self.can_replay_without_restore(world, kf_time, clamped_target);
            if !replay_without_restore {
                restored_entity_map = Some(self.keyframes[&kf_time].restore_with_entity_map(world));
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
        let entity_map = restored_entity_map.unwrap_or_else(|| {
            let mut entity_map = HashMap::new();
            let mut query = world.query::<(Entity, &gaanim_scene::MobjectId)>();
            for (entity, mobj_id) in query.iter(world) {
                entity_map.insert(mobj_id.0, entity);
            }
            entity_map
        });

        // A future animation still owns the property's state before its first
        // clip: it must remain at the lens' `from` value. Path2D objects are
        // pre-seeded during scene construction, but native 3D lines have no
        // Path2D to replace; FillLevel can likewise retain its previous value
        // when continuous playback loops or crosses a segment boundary.
        // Initialize only the earliest future clip per object, then let
        // past/current clips below replay over it as usual.
        let mut future_property_initials = HashMap::new();
        for clip in self.clips_in_range(self.current_time, self.cached_duration) {
            if clip.start <= self.current_time {
                continue;
            }
            let ClipPayload::Animation(anim) = &clip.payload else {
                continue;
            };
            let channel = if replay_without_restore {
                absolute_lens_channel(&anim.lens)
            } else {
                match anim.lens {
                    PropertyLensSpec::PathCompletion { .. } => {
                        Some(AbsoluteLensChannel::PathCompletion)
                    }
                    PropertyLensSpec::FillLevel { .. } => Some(AbsoluteLensChannel::FillLevel),
                    _ => None,
                }
            };
            let Some(channel) = channel else { continue };
            let initial_t = anim.rate_func.evaluate(0.0);
            future_property_initials
                .entry((anim.target, channel))
                .and_modify(|current: &mut (f64, PropertyLensSpec, f64)| {
                    if clip.start < current.0 {
                        *current = (clip.start, anim.lens.clone(), initial_t);
                    }
                })
                .or_insert_with(|| (clip.start, anim.lens.clone(), initial_t));
        }
        for ((target, _), (_, lens, initial_t)) in future_property_initials {
            if let Some(&target_entity) = entity_map.get(&target) {
                apply_lens_spec(world, target_entity, &lens, initial_t, false);
            }
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
                            apply_lens_spec(world, target_entity, &anim.lens, final_t, true);
                        } else if clip.start <= self.current_time && clip.end() > self.current_time
                        {
                            // Animation is actively running at seek head: interpolate
                            let progress =
                                ((self.current_time - clip.start) / clip.duration).clamp(0.0, 1.0);
                            let t = anim.rate_func.evaluate(progress);
                            apply_lens_spec(world, target_entity, &anim.lens, t, false);
                        }
                    }
                }
                ClipPayload::CameraCapture { id } => {
                    if clip.start <= self.current_time
                        && let Some(pose) = world
                            .get_resource::<gaanim_math::Camera>()
                            .map(|camera| camera.pose())
                    {
                        if let Some(mut states) =
                            world.get_resource_mut::<crate::snapshot::CapturedCameraStates>()
                        {
                            states.0.insert(id, pose);
                        } else {
                            let mut states = crate::snapshot::CapturedCameraStates::default();
                            states.0.insert(id, pose);
                            world.insert_resource(states);
                        }
                    }
                }
                ClipPayload::SceneStart(_) | ClipPayload::SceneEnd(_) => {
                    // Scene boundary markers — visibility is handled in the post-pass below.
                }
                ClipPayload::RemoveUpdater { target } => {
                    if clip.start <= self.current_time
                        && let Some(&target_entity) = entity_map.get(&target)
                        && let Ok(mut entity_mut) = world.get_entity_mut(target_entity)
                    {
                        if let Some(mut updater) = entity_mut.get_mut::<gaanim_animation::Updater>()
                        {
                            updater.stop_at = Some(clip.start);
                            if updater.elapsed > clip.start {
                                updater.elapsed = clip.start;
                            }
                        }
                        if let Some(mut driver) =
                            entity_mut.get_mut::<gaanim_animation::SampledSeriesDriver>()
                        {
                            driver.stop_at = Some(clip.start);
                        }
                    }
                }
                ClipPayload::SetSceneMember { target, scene } => {
                    if clip.start <= self.current_time
                        && let Some(&target_entity) = entity_map.get(&target)
                        && let Ok(mut entity_mut) = world.get_entity_mut(target_entity)
                    {
                        if let Some(scene) = scene {
                            entity_mut.insert(SceneMember(scene));
                        } else {
                            entity_mut.remove::<SceneMember>();
                            // Global objects are outside the per-scene visibility pass.
                            // Restore render eligibility without changing authored opacity.
                            entity_mut.insert(gaanim_scene::Visible);
                        }
                    }
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

        // Native glTF players are never advanced by wall-clock time. Sample
        // every Action directly from this absolute playhead after snapshot replay.
        self.evaluate_gltf_animations(world, &entity_map);

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

        if preserve_reactive_state {
            restore_reactive_state(world, &reactive_state);
            if advance_reactive_during_seek {
                gaanim_animation::advance_updaters_by(world, forward_delta);
            }
        } else {
            rebuild_traced_paths(world, self.current_time);
        }

        self.update_segment_position();
        self.restore_followed_shake_origin(world);
        if let Some(mut playback_state) =
            world.get_resource_mut::<gaanim_animation::PlaybackState>()
        {
            playback_state.current_time = self.current_time;
        }
    }

    fn evaluate_gltf_animations(
        &self,
        world: &mut World,
        entity_map: &HashMap<gaanim_core::ObjectId, Entity>,
    ) {
        let mut actions =
            HashMap::<gaanim_core::ObjectId, Vec<(f64, f64, GltfAnimationSpec)>>::new();
        for clip in self.clips.values() {
            if let ClipPayload::GltfAnimation(spec) = &clip.payload {
                actions.entry(spec.target).or_default().push((
                    clip.start,
                    clip.duration,
                    spec.clone(),
                ));
            }
        }

        for (target, clips) in &mut actions {
            clips.sort_by(|left, right| left.0.total_cmp(&right.0));
            let Some(&root) = entity_map.get(target) else {
                continue;
            };
            let Some(state) = world.get::<gaanim_scene::GltfAnimationState>(root).cloned() else {
                continue;
            };
            let current_index = clips
                .iter()
                .rposition(|(start, _, _)| *start <= self.current_time);
            for player_entity in &state.players {
                let Some(mut player) = world.get_mut::<AnimationPlayer>(*player_entity) else {
                    continue;
                };
                for node in &state.nodes {
                    player.play(*node).pause().set_weight(0.0);
                }
                let Some(index) = current_index else { continue };
                let (start, duration, current) = &clips[index];
                let Some(&node) = state.nodes.get(current.animation_index) else {
                    continue;
                };
                let elapsed = (self.current_time - *start).clamp(0.0, *duration);
                let sample = sample_gltf_action(current, elapsed);
                let transition = current.transition.min(*duration).max(0.0);
                let blend = if index > 0 && transition > 0.0 {
                    (elapsed / transition).clamp(0.0, 1.0) as f32
                } else {
                    1.0
                };
                player
                    .play(node)
                    .pause()
                    .set_seek_time(sample as f32)
                    .set_weight(blend);

                if blend < 1.0 {
                    let (previous_start, previous_duration, previous) = &clips[index - 1];
                    if previous.animation_index != current.animation_index
                        && let Some(&previous_node) = state.nodes.get(previous.animation_index)
                    {
                        let previous_elapsed =
                            (self.current_time - *previous_start).clamp(0.0, *previous_duration);
                        let previous_sample = sample_gltf_action(previous, previous_elapsed);
                        player
                            .play(previous_node)
                            .pause()
                            .set_seek_time(previous_sample as f32)
                            .set_weight(1.0 - blend);
                    }
                }
            }
        }
    }

    /// A shake immediately after a follow must be based on the target's position
    /// at the end of the follow, not the compile-time camera position. Snapshot
    /// seeking skips intermediate frames, so resolve that anchor explicitly and
    /// restore reactive updaters to the requested time afterwards.
    fn restore_followed_shake_origin(&self, world: &mut World) {
        let Some(shake_start) = self
            .clips
            .values()
            .filter_map(|clip| match &clip.payload {
                ClipPayload::Animation(anim) => match &anim.lens {
                    PropertyLensSpec::CameraShake { .. } if clip.start <= self.current_time => {
                        Some(clip.start)
                    }
                    _ => None,
                },
                _ => None,
            })
            .max_by(|left, right| left.total_cmp(right))
        else {
            return;
        };

        // A later pan/frame/follow owns the camera position instead.
        let has_later_position_control = self.clips.values().any(|clip| {
            clip.start > shake_start
                && clip.start <= self.current_time
                && matches!(
                    &clip.payload,
                    ClipPayload::Animation(anim)
                        if matches!(
                            anim.lens,
                            PropertyLensSpec::CameraPosition { .. }
                                | PropertyLensSpec::CameraState { .. }
                                | PropertyLensSpec::CameraFollow { .. }
                        )
                )
        });
        if has_later_position_control {
            return;
        }

        let Some((follow_end, target)) = self
            .clips
            .values()
            .filter_map(|clip| match &clip.payload {
                ClipPayload::Animation(anim) => match anim.lens {
                    PropertyLensSpec::CameraFollow { target } if clip.end() <= shake_start => {
                        Some((clip.end(), target))
                    }
                    _ => None,
                },
                _ => None,
            })
            .max_by(|(left, _), (right, _)| left.total_cmp(right))
        else {
            return;
        };

        resync_updaters(world, follow_end);
        let anchor = {
            let mut query = world.query::<(&MobjectId, &SpatialTransform)>();
            query
                .iter(world)
                .find_map(|(id, transform)| (id.0 == target).then_some(transform.translation))
        };
        resync_updaters(world, self.current_time);

        let Some(anchor) = anchor else {
            return;
        };
        if let Some(mut camera) = world.get_resource_mut::<gaanim_math::Camera>() {
            camera.position = anchor;
        }
    }
}

fn capture_reactive_state(
    world: &mut World,
) -> HashMap<gaanim_core::ObjectId, ReactiveEntityState> {
    let mut state = HashMap::new();
    let mut query = world.query::<(
        Entity,
        &gaanim_scene::MobjectId,
        Option<&gaanim_animation::Updater>,
        Option<&gaanim_animation::TracedPath>,
        Option<&gaanim_animation::TracedPath3D>,
        Option<&SpatialTransform>,
    )>();
    for (_entity, mobject_id, updater, traced_path, traced_3d, transform) in query.iter(world) {
        if updater.is_none() && traced_path.is_none() && traced_3d.is_none() {
            continue;
        }
        state.insert(
            mobject_id.0,
            ReactiveEntityState {
                updater_elapsed: updater.map(|u| u.elapsed),
                updater_stop_at: updater.and_then(|u| u.stop_at),
                traced_path_points: traced_path
                    .map(|t| t.points.clone())
                    .or_else(|| traced_3d.map(|t| t.points.clone())),
                traced_path_sample_times: traced_path
                    .map(|t| t.sample_times.clone())
                    .or_else(|| traced_3d.map(|t| t.sample_times.clone())),
                translation: updater
                    .is_some()
                    .then_some(transform.map(|t| t.translation))
                    .flatten(),
            },
        );
    }
    state
}

fn restore_reactive_state(
    world: &mut World,
    reactive_state: &HashMap<gaanim_core::ObjectId, ReactiveEntityState>,
) {
    let mut entity_map = HashMap::new();
    let mut query = world.query::<(Entity, &gaanim_scene::MobjectId)>();
    for (entity, mobject_id) in query.iter(world) {
        entity_map.insert(mobject_id.0, entity);
    }

    for (object_id, state) in reactive_state {
        let Some(&entity) = entity_map.get(object_id) else {
            continue;
        };

        if let Some(mut updater) = world.get_mut::<gaanim_animation::Updater>(entity) {
            if let Some(elapsed) = state.updater_elapsed {
                updater.elapsed = updater
                    .stop_at
                    .map(|stop_at| elapsed.min(stop_at))
                    .unwrap_or(elapsed);
            }
            if updater.stop_at.is_none() {
                updater.stop_at = state.updater_stop_at;
            }
        }
        if let Some(pos) = state.translation {
            if let Some(mut transform) = world.get_mut::<SpatialTransform>(entity) {
                transform.translation = pos;
            }
        }

        if let Some(points) = &state.traced_path_points {
            if let Some(mut traced_path) = world.get_mut::<gaanim_animation::TracedPath>(entity) {
                traced_path.points = points.clone();
                if let Some(sample_times) = &state.traced_path_sample_times {
                    traced_path.sample_times = sample_times.clone();
                }
            }
            if let Some(mut path_comp) = world.get_mut::<Path2D>(entity) {
                let mut path = gaanim_core::kurbo::BezPath::new();
                if let Some(first) = points.first() {
                    path.move_to(gaanim_core::kurbo::Point::new(first.x, first.y));
                    for pt in &points[1..] {
                        path.line_to(gaanim_core::kurbo::Point::new(pt.x, pt.y));
                    }
                }
                path_comp.0 = std::sync::Arc::new(path);
            }
            // 3D traced path restore (generic)
            let colormap_clone = world
                .get::<gaanim_animation::TracedPath3D>(entity)
                .and_then(|t| t.colormap.clone());
            if let Some(mut traced_3d) = world.get_mut::<gaanim_animation::TracedPath3D>(entity) {
                traced_3d.points = points.clone();
                if let Some(sample_times) = &state.traced_path_sample_times {
                    traced_3d.sample_times = sample_times.clone();
                }
            }
            if let Some(mut line) = world.get_mut::<gaanim_scene::LineListData>(entity) {
                let pts: Vec<[f32; 3]> = points
                    .iter()
                    .map(|p| [p.x as f32, p.y as f32, p.z as f32])
                    .collect();
                line.points = pts.clone();
                line.colors = colormap_clone
                    .as_ref()
                    .and_then(|map| map.rgba_f32(pts.len()).ok());
            }
            if let Some(mut bounds) = world.get_mut::<gaanim_scene::LocalBounds>(entity) {
                if points.is_empty() {
                    bounds.0 = gaanim_math::Bounds3D::default();
                } else {
                    let mut min = gaanim_core::glam::DVec3::splat(f64::INFINITY);
                    let mut max = gaanim_core::glam::DVec3::splat(f64::NEG_INFINITY);
                    for p in points {
                        let v = *p;
                        min = min.min(v);
                        max = max.max(v);
                    }
                    bounds.0 = gaanim_math::Bounds3D::new(min, max);
                }
            }
        }
    }
}

fn resync_updaters(world: &mut World, target_time: f64) {
    gaanim_animation::seek_updaters(world, target_time);
}

fn rebuild_traced_paths(world: &mut World, target_time: f64) {
    let traces: Vec<(Entity, Entity, f64, Option<usize>, f64, Option<f64>)> = {
        let mut query = world.query::<(Entity, &gaanim_animation::TracedPath)>();
        query
            .iter(world)
            .map(|(entity, trace)| {
                (
                    entity,
                    trace.source,
                    trace.min_distance,
                    trace.max_points,
                    trace.start_at,
                    trace.dissipating_time,
                )
            })
            .collect()
    };
    let traces_3d: Vec<(
        Entity,
        Entity,
        f64,
        Option<usize>,
        Option<gaanim_core::ColorMap>,
        f64,
        Option<f64>,
    )> = {
        let mut query = world.query::<(Entity, &gaanim_animation::TracedPath3D)>();
        query
            .iter(world)
            .map(|(entity, trace)| {
                (
                    entity,
                    trace.source,
                    trace.min_distance,
                    trace.max_points,
                    trace.colormap.clone(),
                    trace.start_at,
                    trace.dissipating_time,
                )
            })
            .collect()
    };

    if traces.is_empty() && traces_3d.is_empty() {
        resync_updaters(world, target_time);
        return;
    }

    for (trace_entity, _, _, _, _, _) in &traces {
        if let Some(mut traced_path) = world.get_mut::<gaanim_animation::TracedPath>(*trace_entity)
        {
            traced_path.points.clear();
            traced_path.sample_times.clear();
        }
        if let Some(mut path_comp) = world.get_mut::<Path2D>(*trace_entity) {
            path_comp.0 = std::sync::Arc::new(gaanim_core::kurbo::BezPath::new());
        }
    }
    for (trace_entity, _, _, _, _, _, _) in &traces_3d {
        if let Some(mut t) = world.get_mut::<gaanim_animation::TracedPath3D>(*trace_entity) {
            t.points.clear();
            t.sample_times.clear();
        }
        if let Some(mut line) = world.get_mut::<gaanim_scene::LineListData>(*trace_entity) {
            line.points.clear();
            line.colors = None;
        }
        if let Some(mut bounds) = world.get_mut::<gaanim_scene::LocalBounds>(*trace_entity) {
            bounds.0 = gaanim_math::Bounds3D::default();
        }
    }

    // Restore callback state and source positions before collecting samples.
    resync_updaters(world, 0.0);

    let mut sample_time = 0.0;
    let step = 1.0 / 60.0;
    let mut sample_times = Vec::new();
    while sample_time < target_time {
        sample_times.push(sample_time);
        sample_time += step;
    }
    sample_times.push(target_time);

    let mut previous_sample_time = 0.0;
    for sample_time in sample_times {
        gaanim_animation::advance_updaters_by(world, sample_time - previous_sample_time);
        previous_sample_time = sample_time;
        gaanim_animation::position_binding_system(world);

        for (trace_entity, source_entity, min_distance, max_points, start_at, dissipating_time) in
            &traces
        {
            if sample_time + f64::EPSILON < *start_at {
                continue;
            }
            let Some(source_pos) = world
                .get::<SpatialTransform>(*source_entity)
                .map(|t| t.translation)
            else {
                continue;
            };

            if let Some(mut traced_path) =
                world.get_mut::<gaanim_animation::TracedPath>(*trace_entity)
            {
                if let Some(duration) = dissipating_time {
                    let cutoff = sample_time - duration;
                    let expired = traced_path
                        .sample_times
                        .partition_point(|time| *time < cutoff);
                    traced_path.points.drain(0..expired);
                    traced_path.sample_times.drain(0..expired);
                }
                let should_add = match traced_path.points.last() {
                    Some(last_point) => last_point.distance(source_pos) >= *min_distance,
                    None => true,
                };

                if should_add {
                    traced_path.points.push(source_pos);
                    traced_path.sample_times.push(sample_time);
                    if let Some(max) = max_points {
                        if traced_path.points.len() > *max {
                            let overflow = traced_path.points.len() - *max;
                            traced_path.points.drain(0..overflow);
                            traced_path.sample_times.drain(0..overflow);
                        }
                    }
                }
            }
        }
        for (
            trace_entity,
            source_entity,
            min_distance,
            max_points,
            _colormap,
            start_at,
            dissipating_time,
        ) in &traces_3d
        {
            if sample_time + f64::EPSILON < *start_at {
                continue;
            }
            let Some(source_pos) = world
                .get::<SpatialTransform>(*source_entity)
                .map(|t| t.translation)
            else {
                continue;
            };
            if let Some(mut traced) = world.get_mut::<gaanim_animation::TracedPath3D>(*trace_entity)
            {
                if let Some(duration) = dissipating_time {
                    let cutoff = sample_time - duration;
                    let expired = traced.sample_times.partition_point(|time| *time < cutoff);
                    traced.points.drain(0..expired);
                    traced.sample_times.drain(0..expired);
                }
                let should_add = match traced.points.last() {
                    Some(last) => last.distance(source_pos) >= *min_distance,
                    None => true,
                };
                if should_add {
                    traced.points.push(source_pos);
                    traced.sample_times.push(sample_time);
                    if let Some(max) = max_points {
                        if traced.points.len() > *max {
                            let overflow = traced.points.len() - *max;
                            traced.points.drain(0..overflow);
                            traced.sample_times.drain(0..overflow);
                        }
                    }
                }
            }
        }
    }

    for (trace_entity, _, _, _, _, _) in &traces {
        let points = world
            .get::<gaanim_animation::TracedPath>(*trace_entity)
            .map(|trace| trace.points.clone())
            .unwrap_or_default();
        if let Some(mut path_comp) = world.get_mut::<Path2D>(*trace_entity) {
            let mut path = gaanim_core::kurbo::BezPath::new();
            if let Some(first) = points.first() {
                path.move_to(gaanim_core::kurbo::Point::new(first.x, first.y));
                for pt in &points[1..] {
                    path.line_to(gaanim_core::kurbo::Point::new(pt.x, pt.y));
                }
            }
            path_comp.0 = std::sync::Arc::new(path);
        }
    }
    // Rebuild 3D traced paths (LineList + vertex colors + bounds)
    for (trace_entity, _, _, _, colormap, _, _) in &traces_3d {
        let (points, colormap_clone) = world
            .get::<gaanim_animation::TracedPath3D>(*trace_entity)
            .map(|t| (t.points.clone(), t.colormap.clone()))
            .unwrap_or_default();
        let pts_f32: Vec<[f32; 3]> = points
            .iter()
            .map(|p| [p.x as f32, p.y as f32, p.z as f32])
            .collect();
        if let Some(mut line) = world.get_mut::<gaanim_scene::LineListData>(*trace_entity) {
            line.points = pts_f32.clone();
            line.colors = colormap_clone
                .or_else(|| colormap.clone())
                .and_then(|map| map.rgba_f32(pts_f32.len()).ok());
        }
        if let Some(mut bounds) = world.get_mut::<gaanim_scene::LocalBounds>(*trace_entity) {
            if points.is_empty() {
                bounds.0 = gaanim_math::Bounds3D::default();
            } else {
                let mut min = gaanim_core::glam::DVec3::splat(f64::INFINITY);
                let mut max = gaanim_core::glam::DVec3::splat(f64::NEG_INFINITY);
                for p in &points {
                    min = min.min(*p);
                    max = max.max(*p);
                }
                bounds.0 = gaanim_math::Bounds3D::new(min, max);
            }
        }
    }
}

/// Helper function to evaluate and apply a PropertyLensSpec to an entity.
fn lerp_line_point(from: [f32; 3], to: [f32; 3], t: f32) -> [f32; 3] {
    [
        from[0] + (to[0] - from[0]) * t,
        from[1] + (to[1] - from[1]) * t,
        from[2] + (to[2] - from[2]) * t,
    ]
}

fn lerp_line_color(from: [f32; 4], to: [f32; 4], t: f32) -> [f32; 4] {
    [
        from[0] + (to[0] - from[0]) * t,
        from[1] + (to[1] - from[1]) * t,
        from[2] + (to[2] - from[2]) * t,
        from[3] + (to[3] - from[3]) * t,
    ]
}

fn trim_line_list(source: &LineListData, completion: f64) -> LineListData {
    let completion = completion.clamp(0.0, 1.0);
    if completion <= f64::EPSILON {
        return LineListData {
            points: Vec::new(),
            indices: source.indices.as_ref().map(|_| Vec::new()),
            strip: source.strip,
            color: source.color,
            colors: source.colors.as_ref().map(|_| Vec::new()),
        };
    }
    if completion >= 1.0 {
        return source.clone();
    }

    if source.strip && source.indices.is_none() {
        let segment_count = source.points.len().saturating_sub(1);
        if segment_count == 0 {
            return source.clone();
        }
        let scaled = completion * segment_count as f64;
        let completed = scaled.floor() as usize;
        let fraction = (scaled - completed as f64) as f32;
        let point_count = (completed + 1).min(source.points.len());
        let mut points = source.points[..point_count].to_vec();
        let mut colors = source
            .colors
            .as_ref()
            .map(|colors| colors[..point_count.min(colors.len())].to_vec());
        if fraction > f32::EPSILON && completed + 1 < source.points.len() {
            points.push(lerp_line_point(
                source.points[completed],
                source.points[completed + 1],
                fraction,
            ));
            if let (Some(source_colors), Some(colors)) = (&source.colors, &mut colors)
                && completed + 1 < source_colors.len()
            {
                colors.push(lerp_line_color(
                    source_colors[completed],
                    source_colors[completed + 1],
                    fraction,
                ));
            }
        }
        return LineListData {
            points,
            indices: None,
            strip: true,
            color: source.color,
            colors,
        };
    }

    let pair_count = source
        .indices
        .as_ref()
        .map_or(source.points.len() / 2, |indices| indices.len() / 2);
    let scaled = completion * pair_count as f64;
    let completed = (scaled.floor() as usize).min(pair_count);
    let fraction = (scaled - completed as f64) as f32;

    if let Some(source_indices) = &source.indices {
        let mut points = source.points.clone();
        let mut colors = source.colors.clone();
        let mut indices = source_indices[..completed * 2].to_vec();
        if fraction > f32::EPSILON && completed < pair_count {
            let start_index = source_indices[completed * 2] as usize;
            let end_index = source_indices[completed * 2 + 1] as usize;
            if let (Some(start), Some(end)) = (
                source.points.get(start_index).copied(),
                source.points.get(end_index).copied(),
            ) {
                let new_start = points.len() as u32;
                points.push(start);
                points.push(lerp_line_point(start, end, fraction));
                indices.extend_from_slice(&[new_start, new_start + 1]);
                if let Some(colors) = &mut colors
                    && let (Some(start), Some(end)) = (
                        colors.get(start_index).copied(),
                        colors.get(end_index).copied(),
                    )
                {
                    colors.push(start);
                    colors.push(lerp_line_color(start, end, fraction));
                }
            }
        }
        return LineListData {
            points,
            indices: Some(indices),
            strip: source.strip,
            color: source.color,
            colors,
        };
    }

    let point_count = completed * 2;
    let mut points = source.points[..point_count].to_vec();
    let mut colors = source
        .colors
        .as_ref()
        .map(|colors| colors[..point_count.min(colors.len())].to_vec());
    if fraction > f32::EPSILON && completed < pair_count {
        let start_index = completed * 2;
        let end_index = start_index + 1;
        points.push(source.points[start_index]);
        points.push(lerp_line_point(
            source.points[start_index],
            source.points[end_index],
            fraction,
        ));
        if let (Some(source_colors), Some(colors)) = (&source.colors, &mut colors)
            && end_index < source_colors.len()
        {
            colors.push(source_colors[start_index]);
            colors.push(lerp_line_color(
                source_colors[start_index],
                source_colors[end_index],
                fraction,
            ));
        }
    }
    LineListData {
        points,
        indices: None,
        strip: false,
        color: source.color,
        colors,
    }
}

fn trim_line_strip_range(source: &LineListData, start: f64, end: f64) -> LineListData {
    let segment_count = source.points.len().saturating_sub(1);
    let start = start.clamp(0.0, 1.0);
    let end = end.clamp(start, 1.0);
    if segment_count == 0 || end <= start + f64::EPSILON {
        return LineListData {
            points: Vec::new(),
            indices: None,
            strip: true,
            color: source.color,
            colors: source.colors.as_ref().map(|_| Vec::new()),
        };
    }

    let scaled_start = start * segment_count as f64;
    let scaled_end = end * segment_count as f64;
    let start_segment = (scaled_start.floor() as usize).min(segment_count - 1);
    let end_segment = (scaled_end.floor() as usize).min(segment_count - 1);
    let start_fraction = (scaled_start - start_segment as f64) as f32;
    let end_fraction = if end >= 1.0 {
        1.0
    } else {
        (scaled_end - end_segment as f64) as f32
    };

    let mut points = vec![lerp_line_point(
        source.points[start_segment],
        source.points[start_segment + 1],
        start_fraction,
    )];
    for index in (start_segment + 1)..=end_segment {
        points.push(source.points[index]);
    }
    points.push(lerp_line_point(
        source.points[end_segment],
        source.points[end_segment + 1],
        end_fraction,
    ));

    let colors = source.colors.as_ref().map(|colors| {
        let mut visible = vec![lerp_line_color(
            colors[start_segment],
            colors[start_segment + 1],
            start_fraction,
        )];
        for index in (start_segment + 1)..=end_segment {
            visible.push(colors[index]);
        }
        visible.push(lerp_line_color(
            colors[end_segment],
            colors[end_segment + 1],
            end_fraction,
        ));
        visible
    });

    LineListData {
        points,
        indices: None,
        strip: true,
        color: source.color,
        colors,
    }
}

fn apply_lens_spec(
    world: &mut World,
    target: Entity,
    lens: &PropertyLensSpec,
    t: f64,
    completed: bool,
) {
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
        PropertyLensSpec::Material3D { from, to } => {
            if let Some(mut material) = world.get_mut::<gaanim_scene::Material3D>(target) {
                *material = from.lerp(*to, t);
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
                let trimmed = gaanim_math::get_subpath(&source.0, completion);
                if let Some(mut path) = world.get_mut::<Path2D>(target) {
                    path.0 = std::sync::Arc::new(trimmed);
                }
            }

            if let Some(mut tip) = world.get_mut::<gaanim_animation::WriteTipGlow>(target) {
                tip.completion = completion;
            }
            if world.get::<LineListSource>(target).is_none() {
                let line_clone = world.get::<LineListData>(target).cloned();
                if let Some(line) = line_clone
                    && let Ok(mut entity) = world.get_entity_mut(target)
                {
                    entity.insert(LineListSource(line));
                }
            }
            if let Some(source) = world.get::<LineListSource>(target) {
                let visible = trim_line_list(&source.0, completion);
                if let Some(mut line) = world.get_mut::<LineListData>(target) {
                    *line = visible;
                }
                if let Some(mut visibility) = world.get_mut::<bevy::prelude::Visibility>(target) {
                    *visibility = if completion <= f64::EPSILON {
                        bevy::prelude::Visibility::Hidden
                    } else {
                        bevy::prelude::Visibility::Inherited
                    };
                }
            }
            if let Ok(mut em) = world.get_entity_mut(target) {
                em.insert(gaanim_animation::PathReveal(completion));
            }
        }
        PropertyLensSpec::PathMorph { from, to } => {
            let morphed = if completed {
                to.clone()
            } else {
                gaanim_math::interpolate_paths_continuous(from, to, t)
            };
            if let Some(mut path) = world.get_mut::<Path2D>(target) {
                path.0 = std::sync::Arc::new(morphed.clone());
            }
            // Keep the stroke clipping source in lockstep with `Path2D`.
            // A stale source geometry otherwise leaks the previous outline
            // (notably the circle around a morphing diamond) during seeks.
            if let Some(mut source) = world.get_mut::<gaanim_animation::PathSource>(target) {
                source.0 = std::sync::Arc::new(morphed);
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
        PropertyLensSpec::FillLevel { from, to } => {
            let value = (*from + (*to - *from) * t).clamp(0.0, 1.0);
            if let Some(mut level) = world.get_mut::<gaanim_scene::FillLevel>(target) {
                level.0 = value;
            } else {
                world
                    .entity_mut(target)
                    .insert(gaanim_scene::FillLevel(value));
            }
        }
        PropertyLensSpec::SurroundingRectTargets { from, to } => {
            if let Some(mut frame) = world.get_mut::<gaanim_animation::SurroundingRect>(target) {
                frame.from.clone_from(from);
                frame.to.clone_from(to);
                frame.progress = t.clamp(0.0, 1.0);
            }
        }
        PropertyLensSpec::CameraState { from, to } => {
            let resolve = |source: &gaanim_animation::CameraStateSource| match source {
                gaanim_animation::CameraStateSource::Concrete(pose) => Some(*pose),
                gaanim_animation::CameraStateSource::Captured(id) => world
                    .get_resource::<crate::snapshot::CapturedCameraStates>()
                    .and_then(|states| states.0.get(id).copied()),
            };
            if let Some((from, to)) = resolve(from).zip(resolve(to)) {
                let pose = from.interpolate(to, t);
                if let Some(mut camera) = world.get_resource_mut::<gaanim_math::Camera>() {
                    camera.position = pose.position;
                    camera.rotation = pose.rotation;
                    camera.target = pose.target;
                    camera.up = pose.up;
                    camera.projection = pose.projection;
                }
            }
        }
        PropertyLensSpec::CameraPosition { from, to } => {
            if let Some(mut camera) = world.get_resource_mut::<gaanim_math::Camera>() {
                camera.position = from.lerp(*to, t);
            }
        }
        PropertyLensSpec::CameraPositionSource { from, to } => {
            if let Some(target) = gaanim_animation::resolve_tracking_endpoint(to, world)
                && target.is_finite()
                && let Some(mut camera) = world.get_resource_mut::<gaanim_math::Camera>()
            {
                let destination = gaanim_core::glam::DVec3::new(target.x, target.y, from.z);
                camera.position = from.lerp(destination, t);
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
        PropertyLensSpec::CameraZoomSource { from, to } => {
            if let Some(target) = to.evaluate(world)
                && target.is_finite()
                && target > 0.0
                && let Some(mut camera) = world.get_resource_mut::<gaanim_math::Camera>()
            {
                camera.projection = gaanim_math::Projection::Orthographic {
                    zoom: *from + (target - *from) * t,
                };
            }
        }
        PropertyLensSpec::CameraRotationSource { from, to } => {
            if let Some(target) = to.evaluate(world)
                && target.is_finite()
                && let Some(mut camera) = world.get_resource_mut::<gaanim_math::Camera>()
            {
                let angle = *from + (target - *from) * t;
                camera.rotation = gaanim_core::glam::DQuat::from_rotation_z(angle);
            }
        }
        PropertyLensSpec::CameraOrthographic { from, to } => {
            if let Some(mut camera) = world.get_resource_mut::<gaanim_math::Camera>() {
                camera.projection = gaanim_math::Projection::Orthographic {
                    zoom: *from + (*to - *from) * t,
                };
            }
        }
        PropertyLensSpec::CameraReset {
            from_position,
            from_rotation,
            from_target,
            from_up,
            from_zoom,
            to_zoom,
        } => {
            if let Some(mut camera) = world.get_resource_mut::<gaanim_math::Camera>() {
                camera.position = from_position.lerp(gaanim_core::glam::DVec3::ZERO, t);
                camera.rotation = from_rotation.slerp(gaanim_core::glam::DQuat::IDENTITY, t);
                camera.target = from_target.lerp(gaanim_core::glam::DVec3::ZERO, t);
                let blended_up = from_up.lerp(gaanim_core::glam::DVec3::Y, t);
                camera.up = if blended_up.length_squared() > f64::EPSILON {
                    blended_up.normalize()
                } else {
                    gaanim_core::glam::DVec3::Y
                };
                camera.projection = gaanim_math::Projection::Orthographic {
                    zoom: *from_zoom + (*to_zoom - *from_zoom) * t,
                };
            }
        }
        PropertyLensSpec::CameraFollow { target: followed } => {
            let position = {
                let mut query = world.query::<(&MobjectId, &SpatialTransform)>();
                query.iter(world).find_map(|(id, transform)| {
                    (id.0 == *followed).then_some(transform.translation)
                })
            };
            if let Some(position) = position
                && let Some(mut camera) = world.get_resource_mut::<gaanim_math::Camera>()
            {
                camera.position.x = position.x;
                camera.position.y = position.y;
            }
        }
        PropertyLensSpec::CameraFollowEndpoint { .. } => {
            // Applied in the camera phase after persistent bindings.
        }
        PropertyLensSpec::CameraFrameDynamic { .. } => {
            // Applied in the camera phase after persistent bindings and layout.
        }
        PropertyLensSpec::CameraShake { .. } => {}
        PropertyLensSpec::CameraTarget { from, to } => {
            if let Some(mut camera) = world.get_resource_mut::<gaanim_math::Camera>() {
                camera.target = from.lerp(*to, t);
                // Keep rotation consistent with look_at
                let eye = camera.position;
                let up = camera.up;
                let view =
                    gaanim_core::glam::dcamera::rh::view::look_at_mat4(eye, camera.target, up);
                let rot = view.inverse().to_scale_rotation_translation().1;
                camera.rotation = rot;
            }
        }
        PropertyLensSpec::CameraLookAt {
            from_position,
            from_target,
            eye,
            target,
            up,
        } => {
            let position = from_position.lerp(*eye, t);
            let target = from_target.lerp(*target, t);
            if gaanim_math::Camera::validate_look_at(position, target, *up).is_ok()
                && let Some(mut camera) = world.get_resource_mut::<gaanim_math::Camera>()
            {
                camera
                    .look_at(position, target, *up)
                    .expect("validated look-at pose");
            }
        }
        PropertyLensSpec::CameraOrbit {
            from_position,
            target,
            up,
            delta_yaw,
            delta_pitch,
        } => {
            let mut orbit = gaanim_math::Camera::ortho_2d(1, 1);
            orbit
                .look_at(*from_position, *target, *up)
                .expect("compiled orbit starts from a valid authored pose");
            orbit
                .orbit_around_target(delta_yaw * t, delta_pitch * t)
                .expect("validated orbit interpolation");
            if let Some(mut camera) = world.get_resource_mut::<gaanim_math::Camera>() {
                camera.position = orbit.position;
                camera.target = orbit.target;
                camera.up = orbit.up;
                camera.rotation = orbit.rotation;
            }
        }
        PropertyLensSpec::CameraLookAtSource {
            from_position,
            from_target,
            from_rotation: _,
            eye,
            target,
            up,
        } => {
            let resolved = gaanim_animation::resolve_tracking_endpoint(eye, world)
                .zip(gaanim_animation::resolve_tracking_endpoint(target, world));
            if let Some((eye, target)) = resolved {
                let position = from_position.lerp(eye, t);
                let target = from_target.lerp(target, t);
                if gaanim_math::Camera::validate_look_at(position, target, *up).is_ok()
                    && let Some(mut camera) = world.get_resource_mut::<gaanim_math::Camera>()
                {
                    camera
                        .look_at(position, target, *up)
                        .expect("validated tracked look-at pose");
                }
            }
        }
        PropertyLensSpec::CameraPerspective {
            from_fov,
            to_fov,
            from_near,
            to_near,
            from_far,
            to_far,
        } => {
            if let Some(mut camera) = world.get_resource_mut::<gaanim_math::Camera>() {
                let fov = from_fov + (to_fov - from_fov) * t;
                let near = from_near + (to_near - from_near) * t;
                let far = from_far + (to_far - from_far) * t;
                camera.projection = gaanim_math::Projection::Perspective {
                    fov_y: fov,
                    near,
                    far,
                };
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
        PropertyLensSpec::PathFollow3D { points } => {
            if let Some(mut transform) = world.get_mut::<SpatialTransform>(target) {
                transform.translation = gaanim_math::get_point_on_polyline(points, t);
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
            if world.get::<LineListSource>(target).is_none() {
                let line_clone = world.get::<LineListData>(target).cloned();
                if let Some(line) = line_clone
                    && let Ok(mut entity) = world.get_entity_mut(target)
                {
                    entity.insert(LineListSource(line));
                }
            }
            if let Some(source) = world.get::<LineListSource>(target)
                && source.0.strip
                && source.0.indices.is_none()
            {
                let visible = trim_line_strip_range(&source.0, start, end);
                if let Some(mut line) = world.get_mut::<LineListData>(target) {
                    *line = visible;
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
            // Crossfade: multiplicatively modulate existing opacity.
            // Only touch root entities — opacity_propagation_system cascades to children.
            for (entity, scene_id) in scene_entities {
                if world.get::<ChildOf>(*entity).is_some() {
                    continue; // skip children, they inherit from root
                }
                let factor = if *scene_id == from {
                    1.0 - t as f32
                } else if *scene_id == to {
                    t as f32
                } else {
                    continue;
                };
                if let Some(mut opacity) = world.get_mut::<Opacity>(*entity) {
                    opacity.0 *= factor;
                }
            }
        }
        TransitionType::FadeThrough { fade_color, .. } => {
            // Fade-through: multiplicatively modulate existing opacity.
            // Only touch root entities — opacity_propagation_system cascades to children.
            let _ = fade_color;
            for (entity, scene_id) in scene_entities {
                if world.get::<ChildOf>(*entity).is_some() {
                    continue; // skip children, they inherit from root
                }
                let factor = if t < 0.5 {
                    // First half: fade out from-scene, keep to-scene hidden
                    if *scene_id == from {
                        1.0 - (t * 2.0) as f32
                    } else {
                        0.0
                    }
                } else {
                    // Second half: fade in to-scene, keep from-scene hidden
                    if *scene_id == to {
                        ((t - 0.5) * 2.0) as f32
                    } else {
                        0.0
                    }
                };
                if let Some(mut opacity) = world.get_mut::<Opacity>(*entity) {
                    opacity.0 *= factor;
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clip::{AnimationSpec, ClipPayload};
    use crate::snapshot::WorldSnapshot;
    use gaanim_core::ObjectId;
    use gaanim_core::kurbo::BezPath;
    use gaanim_math::{RateFunc, SpatialTransform};
    use gaanim_scene::{FillLevel, Material3D, MobjectId, PathSource};
    use std::sync::Arc;

    fn absolute_seek_fixture() -> (World, Timeline, Entity) {
        let object_id = ObjectId::from_raw(404);
        let mut path = BezPath::new();
        path.move_to((0.0, 0.0));
        path.line_to((100.0, 0.0));
        let mut world = World::new();
        let entity = world
            .spawn((
                MobjectId(object_id),
                SpatialTransform::default(),
                Opacity(1.0),
                Path2D(Arc::new(path.clone())),
                PathSource(Arc::new(path)),
            ))
            .id();
        let snapshot = WorldSnapshot::capture(&mut world);
        let mut timeline = Timeline::default();
        let track = timeline.add_track("absolute", 0);
        timeline.add_keyframe(0.0, snapshot);
        timeline.add_clip(
            track,
            0.0,
            1.0,
            ClipPayload::Animation(AnimationSpec {
                target: object_id,
                lens: PropertyLensSpec::PathCompletion { from: 0.0, to: 1.0 },
                rate_func: RateFunc::Linear,
                delay: 0.0,
                label: None,
            }),
        );
        timeline.add_clip(
            track,
            0.5,
            1.0,
            ClipPayload::Animation(AnimationSpec {
                target: object_id,
                lens: PropertyLensSpec::Translation {
                    from: gaanim_core::glam::DVec3::ZERO,
                    to: gaanim_core::glam::DVec3::new(40.0, -20.0, 0.0),
                },
                rate_func: RateFunc::Linear,
                delay: 0.0,
                label: None,
            }),
        );
        timeline.add_clip(
            track,
            1.0,
            1.0,
            ClipPayload::Animation(AnimationSpec {
                target: object_id,
                lens: PropertyLensSpec::Opacity {
                    from: 1.0,
                    to: 0.25,
                },
                rate_func: RateFunc::Linear,
                delay: 0.0,
                label: None,
            }),
        );
        (world, timeline, entity)
    }

    #[test]
    fn absolute_replay_matches_forced_restore_for_random_access() {
        let (mut optimized_world, mut optimized, optimized_entity) = absolute_seek_fixture();
        let (mut reference_world, mut reference, reference_entity) = absolute_seek_fixture();

        for target in [2.0, 0.25, 1.75, 0.0, 1.2] {
            optimized.seek(&mut optimized_world, target);
            reference.last_restore_kf_time = None;
            reference.seek(&mut reference_world, target);

            assert_eq!(
                optimized_world.get::<SpatialTransform>(optimized_entity),
                reference_world.get::<SpatialTransform>(reference_entity)
            );
            assert_eq!(
                optimized_world.get::<Opacity>(optimized_entity),
                reference_world.get::<Opacity>(reference_entity)
            );
            assert_eq!(
                optimized_world.get::<Path2D>(optimized_entity),
                reference_world.get::<Path2D>(reference_entity)
            );
        }
    }

    #[test]
    fn dynamic_camera_clip_disables_restore_skipping() {
        let (mut world, mut timeline, _) = absolute_seek_fixture();
        timeline.seek(&mut world, 0.25);
        let track = timeline.tracks.keys().next().unwrap();
        timeline.add_clip(
            track,
            0.5,
            0.5,
            ClipPayload::Animation(AnimationSpec {
                target: ObjectId::from_raw(404),
                lens: PropertyLensSpec::CameraPosition {
                    from: gaanim_core::glam::DVec3::ZERO,
                    to: gaanim_core::glam::DVec3::X,
                },
                rate_func: RateFunc::Linear,
                delay: 0.0,
                label: None,
            }),
        );

        assert!(!timeline.can_replay_without_restore(&mut world, OrderedFloat(0.0), 0.75));
    }

    #[test]
    fn captured_camera_state_restores_deterministically_across_seeks() {
        let mut world = World::new();
        let camera_id = ObjectId::from_parts(0, 1);
        world.spawn((MobjectId(camera_id), SpatialTransform::default()));
        world.insert_resource(gaanim_math::Camera::ortho_2d(960, 540));
        world.insert_resource(crate::snapshot::CapturedCameraStates::default());

        let mut timeline = Timeline::new();
        let track = timeline.add_track("Camera", 0);
        timeline.add_clip(
            track,
            0.0,
            1.0,
            ClipPayload::Animation(AnimationSpec {
                target: camera_id,
                lens: PropertyLensSpec::CameraPosition {
                    from: gaanim_core::glam::DVec3::ZERO,
                    to: gaanim_core::glam::DVec3::new(120.0, -30.0, 0.0),
                },
                rate_func: RateFunc::Linear,
                delay: 0.0,
                label: Some("Camera".into()),
            }),
        );
        timeline.add_clip(track, 1.0, 0.0, ClipPayload::CameraCapture { id: 10 });
        timeline.add_clip(track, 1.0, 0.0, ClipPayload::CameraCapture { id: 11 });
        timeline.add_clip(
            track,
            1.0,
            1.0,
            ClipPayload::Animation(AnimationSpec {
                target: camera_id,
                lens: PropertyLensSpec::CameraPosition {
                    from: gaanim_core::glam::DVec3::new(120.0, -30.0, 0.0),
                    to: gaanim_core::glam::DVec3::ZERO,
                },
                rate_func: RateFunc::Linear,
                delay: 0.0,
                label: Some("Camera".into()),
            }),
        );
        timeline.add_clip(track, 2.0, 0.0, ClipPayload::CameraCapture { id: 12 });
        timeline.add_clip(
            track,
            2.0,
            1.0,
            ClipPayload::Animation(AnimationSpec {
                target: camera_id,
                lens: PropertyLensSpec::CameraState {
                    from: gaanim_animation::CameraStateSource::Captured(12),
                    to: gaanim_animation::CameraStateSource::Captured(10),
                },
                rate_func: RateFunc::Linear,
                delay: 0.0,
                label: Some("Camera".into()),
            }),
        );
        timeline.add_keyframe(0.0, WorldSnapshot::capture(&mut world));

        timeline.seek(&mut world, 3.0);
        let expected = world.resource::<gaanim_math::Camera>().pose();
        assert_eq!(
            expected.position,
            gaanim_core::glam::DVec3::new(120.0, -30.0, 0.0)
        );

        timeline.seek(&mut world, 0.25);
        timeline.seek(&mut world, 3.0);
        assert_eq!(world.resource::<gaanim_math::Camera>().pose(), expected);

        let snapshot = WorldSnapshot::capture(&mut world);
        world
            .resource_mut::<crate::snapshot::CapturedCameraStates>()
            .0
            .clear();
        snapshot.restore(&mut world);
        assert!(
            world
                .resource::<crate::snapshot::CapturedCameraStates>()
                .0
                .contains_key(&10)
        );
    }

    #[test]
    fn surrounding_rect_retarget_seek_is_exact_and_reversible() {
        let mut world = World::new();
        let frame_id = ObjectId::from_parts(1, 1);
        let from = ObjectId::from_parts(2, 1);
        let to = ObjectId::from_parts(3, 1);
        let frame_entity = world
            .spawn((
                MobjectId(frame_id),
                SpatialTransform::default(),
                gaanim_animation::SurroundingRect::new(vec![from], [12.0; 4], 8.0),
            ))
            .id();
        let snapshot = WorldSnapshot::capture(&mut world);
        let mut timeline = Timeline::default();
        let track = timeline.add_track("SurroundingRect", 0);
        timeline.add_keyframe(0.0, snapshot);
        timeline.add_clip(
            track,
            1.0,
            1.0,
            ClipPayload::Animation(AnimationSpec {
                target: frame_id,
                lens: PropertyLensSpec::SurroundingRectTargets {
                    from: vec![from],
                    to: vec![to],
                },
                rate_func: RateFunc::Linear,
                delay: 0.0,
                label: None,
            }),
        );

        timeline.seek(&mut world, 1.5);
        let active = world
            .get::<gaanim_animation::SurroundingRect>(frame_entity)
            .unwrap();
        assert_eq!(active.from, vec![from]);
        assert_eq!(active.to, vec![to]);
        assert_eq!(active.progress, 0.5);

        timeline.seek(&mut world, 0.5);
        let restored = world
            .get::<gaanim_animation::SurroundingRect>(frame_entity)
            .unwrap();
        assert_eq!(restored.from, vec![from]);
        assert_eq!(restored.to, vec![from]);
        assert_eq!(restored.progress, 1.0);
    }

    #[test]
    fn material_3d_seek_is_exact_and_reversible() {
        let mut world = World::new();
        let object_id = ObjectId::from_raw(0);
        let from = Material3D::new(gaanim_core::peniko::Color::BLACK, 0.8, 0.1, None, 0.0).unwrap();
        let to = Material3D::new(
            gaanim_core::peniko::Color::WHITE,
            0.2,
            0.9,
            Some(gaanim_core::peniko::Color::WHITE),
            4.0,
        )
        .unwrap();
        let entity = world
            .spawn((MobjectId(object_id), SpatialTransform::default(), from))
            .id();
        let snapshot = WorldSnapshot::capture(&mut world);
        let mut timeline = Timeline::default();
        let track = timeline.add_track("Material", 0);
        timeline.add_keyframe(0.0, snapshot);
        timeline.add_clip(
            track,
            1.0,
            1.0,
            ClipPayload::Animation(AnimationSpec {
                target: object_id,
                lens: PropertyLensSpec::Material3D { from, to },
                rate_func: RateFunc::Linear,
                delay: 0.0,
                label: Some("Material".into()),
            }),
        );

        timeline.seek(&mut world, 2.0);
        assert_eq!(*world.get::<Material3D>(entity).unwrap(), to);
        timeline.seek(&mut world, 0.5);
        assert_eq!(*world.get::<Material3D>(entity).unwrap(), from);
        timeline.seek(&mut world, 1.5);
        let middle = *world.get::<Material3D>(entity).unwrap();
        assert!((middle.roughness - 0.5).abs() < 1e-6);
        assert!((middle.metallic - 0.5).abs() < 1e-6);
        assert!((middle.emissive_strength - 2.0).abs() < 1e-6);
    }

    #[test]
    fn future_fill_level_clip_resets_to_its_initial_value_without_a_snapshot() {
        let mut world = World::new();
        let object_id = ObjectId::from_raw(91);
        let entity = world
            .spawn((
                MobjectId(object_id),
                SpatialTransform::default(),
                FillLevel(0.75),
            ))
            .id();
        let mut timeline = Timeline::default();
        let track = timeline.add_track("FillLevel", 0);
        timeline.add_clip(
            track,
            1.0,
            1.0,
            ClipPayload::Animation(AnimationSpec {
                target: object_id,
                lens: PropertyLensSpec::FillLevel {
                    from: 0.0,
                    to: 0.75,
                },
                rate_func: RateFunc::Linear,
                delay: 0.0,
                label: Some("FillLevel".into()),
            }),
        );

        timeline.seek(&mut world, 0.5);

        assert_eq!(world.get::<FillLevel>(entity).unwrap().0, 0.0);
    }

    #[test]
    fn zero_path_completion_restores_an_empty_path() {
        let mut source = BezPath::new();
        source.move_to((0.0, 0.0));
        source.line_to((100.0, 0.0));
        let object_id = ObjectId::from_raw(77);
        let mut world = World::new();
        let entity = world
            .spawn((
                MobjectId(object_id),
                Path2D(Arc::new(source.clone())),
                PathSource(Arc::new(source)),
                SpatialTransform::default(),
            ))
            .id();
        let line_object_id = ObjectId::from_raw(78);
        let line_entity = world
            .spawn((
                MobjectId(line_object_id),
                LineListData {
                    points: vec![[0.0, 0.0, 0.0], [100.0, 0.0, 0.0]],
                    indices: None,
                    strip: true,
                    color: gaanim_core::peniko::Color::WHITE,
                    colors: None,
                },
                SpatialTransform::default(),
                bevy::prelude::Visibility::Inherited,
            ))
            .id();
        let snapshot = WorldSnapshot::capture(&mut world);
        let mut timeline = Timeline::default();
        let track = timeline.add_track("Path", 0);
        timeline.add_keyframe(0.0, snapshot);
        timeline.add_clip(
            track,
            0.0,
            1.0,
            ClipPayload::Animation(AnimationSpec {
                target: object_id,
                lens: PropertyLensSpec::PathCompletion { from: 0.0, to: 1.0 },
                rate_func: RateFunc::Linear,
                delay: 0.0,
                label: Some("PathCompletion".into()),
            }),
        );
        timeline.add_clip(
            track,
            1.0,
            1.0,
            ClipPayload::Animation(AnimationSpec {
                target: line_object_id,
                lens: PropertyLensSpec::PathCompletion { from: 0.0, to: 1.0 },
                rate_func: RateFunc::Linear,
                delay: 0.0,
                label: Some("LineListCompletion".into()),
            }),
        );

        timeline.seek(&mut world, 0.0);
        assert!(world.get::<Path2D>(entity).unwrap().0.elements().is_empty());
        assert!(
            world
                .get::<LineListData>(line_entity)
                .unwrap()
                .points
                .is_empty()
        );
        assert_eq!(
            *world.get::<bevy::prelude::Visibility>(line_entity).unwrap(),
            bevy::prelude::Visibility::Hidden
        );

        timeline.seek(&mut world, 1.5);
        assert_eq!(
            world.get::<LineListData>(line_entity).unwrap().points.len(),
            2
        );
        assert_eq!(
            *world.get::<bevy::prelude::Visibility>(line_entity).unwrap(),
            bevy::prelude::Visibility::Inherited
        );
    }

    #[test]
    fn fixed_step_simulation_moves_during_explicit_export_seeks_and_rewinds() {
        let mut world = World::new();
        world.insert_resource(gaanim_animation::PlaybackState {
            is_playing: false,
            scaled_dt: 0.0,
            current_time: 0.0,
        });
        let object_id = ObjectId::from_raw(0);
        let updater = gaanim_animation::Updater::new_simulation(
            |dt, _elapsed, entity, world| {
                if let Some(mut transform) = world.get_mut::<SpatialTransform>(entity) {
                    transform.translation.x += dt;
                }
                true
            },
            |_entity, _world| true,
            1.0 / 240.0,
        )
        .unwrap();
        let mut initial_transform = SpatialTransform::default();
        initial_transform.translation.x = 3.0;
        let entity = world
            .spawn((MobjectId(object_id), initial_transform, updater))
            .id();

        let snapshot = WorldSnapshot::capture(&mut world);
        let mut timeline = Timeline::default();
        timeline.cached_duration = 1.0;
        timeline.add_keyframe(0.0, snapshot);

        timeline.seek(&mut world, 0.0);
        timeline.seek(&mut world, 0.1);
        let first = world.get::<SpatialTransform>(entity).unwrap().translation.x;
        assert!((first - 3.1).abs() < 1e-12);

        timeline.seek(&mut world, 0.2);
        let second = world.get::<SpatialTransform>(entity).unwrap().translation.x;
        assert!((second - 3.2).abs() < 1e-12);

        timeline.seek(&mut world, 0.1);
        let rewound = world.get::<SpatialTransform>(entity).unwrap().translation.x;
        assert!((rewound - first).abs() < 1e-12);
    }

    #[test]
    fn traced_path_seek_respects_start_and_dissipating_window() {
        let mut world = World::new();
        world.insert_resource(gaanim_animation::PlaybackState::default());

        let source_id = ObjectId::from_raw(0);
        let trace_id = ObjectId::from_raw(1);
        let updater = gaanim_animation::Updater::new_simulation(
            |dt, _elapsed, entity, world| {
                if let Some(mut transform) = world.get_mut::<SpatialTransform>(entity) {
                    transform.translation.x += dt;
                }
                true
            },
            |_entity, _world| true,
            1.0 / 240.0,
        )
        .unwrap()
        .starting_at(1.0);
        let source = world
            .spawn((MobjectId(source_id), SpatialTransform::default(), updater))
            .id();
        let traced_path = gaanim_animation::TracedPath::new(source, 0.01, None)
            .starting_at(1.0)
            .with_dissipating_time(Some(1.0));
        let trace = world
            .spawn((
                MobjectId(trace_id),
                SpatialTransform::default(),
                Path2D(Arc::new(BezPath::new())),
                traced_path,
            ))
            .id();

        let snapshot = WorldSnapshot::capture(&mut world);
        let mut timeline = Timeline::default();
        timeline.cached_duration = 3.0;
        timeline.add_keyframe(0.0, snapshot);

        timeline.seek(&mut world, 0.5);
        assert!(
            world
                .get::<gaanim_animation::TracedPath>(trace)
                .unwrap()
                .points
                .is_empty()
        );

        timeline.seek(&mut world, 2.5);
        let at_two_and_a_half = world
            .get::<gaanim_animation::TracedPath>(trace)
            .unwrap()
            .sample_times
            .clone();
        assert!(!at_two_and_a_half.is_empty());
        assert!(at_two_and_a_half.iter().all(|time| *time >= 1.5 - 1e-9));
        assert!(at_two_and_a_half.iter().all(|time| *time <= 2.5 + 1e-9));

        timeline.seek(&mut world, 1.5);
        timeline.seek(&mut world, 2.5);
        assert_eq!(
            world
                .get::<gaanim_animation::TracedPath>(trace)
                .unwrap()
                .sample_times,
            at_two_and_a_half
        );
    }

    #[test]
    fn remove_updater_clip_removes_component_after_timestamp() {
        let mut world = World::new();
        let object_id = ObjectId::from_raw(0);
        let entity = world
            .spawn((
                MobjectId(object_id),
                SpatialTransform::default(),
                gaanim_animation::orbit_updater(gaanim_core::glam::DVec3::ZERO, 10.0, 1.0),
            ))
            .id();

        let snapshot = WorldSnapshot::capture(&mut world);

        let mut timeline = Timeline::default();
        let track = timeline.tracks.insert_with_key(|id| crate::clip::Track {
            id,
            name: "Reactive".into(),
            order: 0,
            object_id: Some(object_id),
            scene: None,
        });
        timeline.add_keyframe(0.0, snapshot);
        timeline.add_clip(
            track,
            1.0,
            0.0,
            ClipPayload::RemoveUpdater { target: object_id },
        );

        timeline.seek(&mut world, 0.5);
        assert!(world.get::<gaanim_animation::Updater>(entity).is_some());
        assert_eq!(
            world
                .get::<gaanim_animation::Updater>(entity)
                .and_then(|u| u.stop_at),
            None
        );

        timeline.seek(&mut world, 1.5);
        let updater = world
            .get::<gaanim_animation::Updater>(entity)
            .expect("updater should remain present but frozen");
        assert_eq!(updater.stop_at, Some(1.0));
        assert!((updater.elapsed - 1.0).abs() < 1e-9);
    }

    #[test]
    fn scene_membership_event_is_applied_at_its_timestamp_and_reversible() {
        let mut world = World::new();
        let object_id = ObjectId::from_raw(0);
        let mut timeline = Timeline::default();
        let first_scene = timeline.add_scene("first");
        let second_scene = timeline.add_scene("second");
        let entity = world
            .spawn((
                MobjectId(object_id),
                SpatialTransform::default(),
                SceneMember(first_scene),
            ))
            .id();
        let snapshot = WorldSnapshot::capture(&mut world);
        let track = timeline.add_track("Scene membership", 0);
        timeline.add_keyframe(0.0, snapshot);
        timeline.add_clip(
            track,
            1.0,
            0.0,
            ClipPayload::SetSceneMember {
                target: object_id,
                scene: Some(second_scene),
            },
        );
        timeline.add_clip(
            track,
            2.0,
            0.0,
            ClipPayload::SetSceneMember {
                target: object_id,
                scene: None,
            },
        );

        timeline.seek(&mut world, 0.5);
        assert_eq!(
            world.get::<SceneMember>(entity).map(|s| s.0),
            Some(first_scene)
        );

        timeline.seek(&mut world, 1.0);
        assert_eq!(
            world.get::<SceneMember>(entity).map(|s| s.0),
            Some(second_scene)
        );

        timeline.seek(&mut world, 2.0);
        assert_eq!(world.get::<SceneMember>(entity), None);
        assert!(world.get::<gaanim_scene::Visible>(entity).is_some());

        timeline.seek(&mut world, 0.5);
        assert_eq!(
            world.get::<SceneMember>(entity).map(|s| s.0),
            Some(first_scene)
        );
    }

    #[test]
    fn completed_spring_morph_commits_exact_target_path() {
        let mut from = BezPath::new();
        from.move_to((0.0, 0.0));
        from.line_to((10.0, 0.0));
        from.line_to((5.0, 10.0));
        from.close_path();

        let mut to = BezPath::new();
        to.move_to((0.0, 0.0));
        to.curve_to((0.0, 8.0), (10.0, 8.0), (10.0, 0.0));
        to.close_path();

        let mut world = World::new();
        let object_id = ObjectId::from_raw(0);
        let entity = world
            .spawn((
                MobjectId(object_id),
                SpatialTransform::default(),
                Path2D(Arc::new(from.clone())),
                PathSource(Arc::new(from.clone())),
            ))
            .id();
        let snapshot = WorldSnapshot::capture(&mut world);

        let mut timeline = Timeline::default();
        let track = timeline.add_track("Morph", 0);
        timeline.add_keyframe(0.0, snapshot);
        timeline.add_clip(
            track,
            0.0,
            1.0,
            ClipPayload::Animation(AnimationSpec {
                target: object_id,
                lens: PropertyLensSpec::PathMorph {
                    from,
                    to: to.clone(),
                },
                rate_func: RateFunc::Spring {
                    stiffness: 90.0,
                    damping: 12.0,
                },
                delay: 0.0,
                label: Some("Transform".into()),
            }),
        );

        timeline.seek(&mut world, 0.5);
        let active_path = world.get::<Path2D>(entity).unwrap().0.clone();
        assert_eq!(world.get::<PathSource>(entity).unwrap().0, active_path);
        assert_ne!(active_path.as_ref(), &to);

        timeline.seek(&mut world, 1.0);

        assert_eq!(world.get::<Path2D>(entity).unwrap().0.as_ref(), &to);
        assert_eq!(world.get::<PathSource>(entity).unwrap().0.as_ref(), &to);
    }

    #[test]
    fn semantic_segment_positions_and_stops_are_explicit() {
        let mut timeline = Timeline::default();
        timeline.set_segments(vec![
            SegmentMetadata {
                id: 10,
                name: "intro".to_string(),
                notes: Some("Opening".to_string()),
                start_time: 0.0,
                end_time: 3.0,
                stops: vec![SegmentStop {
                    name: Some("señal".to_string()),
                    time: 1.0,
                }],
            },
            SegmentMetadata {
                id: 20,
                name: "Área".to_string(),
                notes: None,
                start_time: 3.0,
                end_time: 5.0,
                stops: vec![
                    SegmentStop {
                        name: None,
                        time: 4.0,
                    },
                    SegmentStop {
                        name: Some("nearby".to_string()),
                        time: 4.1,
                    },
                ],
            },
        ]);

        assert_eq!(
            timeline.segment_position_at(1.5),
            Some(SegmentPosition {
                segment_id: 10,
                stop_index: Some(0),
            })
        );
        assert_eq!(timeline.next_stop(1.0), Some(4.0));
        assert_eq!(timeline.previous_stop(3.0), Some(1.0));
        assert_eq!(timeline.previous_stop(4.1), Some(4.0));
        assert_eq!(
            timeline.segment_time(SegmentPosition {
                segment_id: 20,
                stop_index: Some(0),
            }),
            Some(4.0)
        );
        assert_eq!(timeline.segment_time_named("ÁREA"), Some(3.0));
        assert_eq!(
            timeline.segment_stop_time_named("INTRO", "SEÑAL"),
            Some(1.0)
        );
        assert_eq!(timeline.segment_time_indexed("área", Some(0)), Some(4.0));
        assert_eq!(timeline.segment_time_indexed("área", Some(2)), None);

        timeline.current_time = 4.0;
        timeline.update_segment_position();
        assert_eq!(
            timeline.segment_label().as_deref(),
            Some("2 / 2 · Área · stop 1")
        );
    }
}
