use gaanim_animation::PropertyLens;
use gaanim_core::kurbo::BezPath;
use gaanim_core::{ObjectId, peniko::Color};
use gaanim_math::{RateFunc, SpatialTransform};

// We use slotmap to manage IDs cleanly without generational index validation bugs
slotmap::new_key_type! {
    /// Unique identifier for an individual timeline clip.
    pub struct ClipId;
    /// Unique identifier for an individual timeline track.
    pub struct TrackId;
    /// Unique identifier for a scene in a multi-scene timeline.
    pub struct SceneId;
}

/// Represents a track in the multi-track timeline (e.g. "Graphics", "Audio", etc.).
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Track {
    /// Unique key for the track.
    pub id: TrackId,
    /// Human-readable name.
    pub name: String,
    /// Layout order for display in the timeline editor panel.
    pub order: i32,
    /// The ObjectId of the mobject associated with this track (if any).
    pub object_id: Option<ObjectId>,
    /// The scene this track belongs to (None for global tracks).
    pub scene: Option<SceneId>,
}

/// A discrete event or continuous element in the timeline with a start time and duration.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Clip {
    /// Unique ID within the timeline arena.
    pub id: ClipId,
    /// Reference to the parent track containing this clip.
    pub track: TrackId,
    /// Start time in seconds.
    pub start: f64,
    /// Duration in seconds.
    pub duration: f64,
    /// The actual payload/event represented by this clip.
    pub payload: ClipPayload,
}

impl Clip {
    /// Returns the end time of the clip (start + duration).
    pub fn end(&self) -> f64 {
        self.start + self.duration
    }

    /// Checks if a given time is within the clip's timespan (inclusive of start, exclusive of end).
    pub fn contains(&self, time: f64) -> bool {
        time >= self.start && time < self.end()
    }
}

/// The payload data representing the behavior/event of a timeline clip.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum ClipPayload {
    /// An animation that runs programmatically on one or more Mobjects.
    Animation(AnimationSpec),
    /// Deterministically sampled Blender Action on a native glTF instance.
    GltfAnimation(GltfAnimationSpec),
    /// A wait period (empty space in timeline, but blocks cursor flow).
    Wait,
    /// An audio source synchronized at a specific start offset.
    Audio {
        /// Asset path or identifier for the audio track.
        source: String,
        /// The starting playback offset within the audio file itself (seconds).
        offset: f64,
        /// Audio volume multiplier.
        volume: f32,
    },
    /// A custom event marker (e.g. for syncing external hooks or triggers).
    Marker(String),
    /// An explicit zero-duration interactive playback stop.
    Stop,
    /// Named segment start marker for dividing long sequences.
    SegmentStart(String),
    /// A structural change to ungroup a group and reparent its children.
    Ungroup {
        /// The group ObjectId to dissolve.
        group: ObjectId,
        /// The list of child ObjectIds.
        children: Vec<ObjectId>,
        /// The original parent of the group (if any), for reversible regroup.
        group_parent: Option<ObjectId>,
        /// The group's spatial transform at the time of ungroup, for reversible regroup.
        group_transform: SpatialTransform,
        /// Pre-computed world-space transforms for each child at ungroup time.
        /// After the group entity is despawned, stale local-space animation clips
        /// keep replaying on each seek. These stored world transforms are
        /// re-applied to overwrite those stale values.
        children_world_transforms: Vec<(ObjectId, SpatialTransform)>,
    },
    /// Marks the beginning of a scene at this timestamp.
    SceneStart(SceneId),
    /// Marks the end of a scene at this timestamp.
    SceneEnd(SceneId),
    /// Removes a continuous updater from a mobject at a specific timestamp.
    RemoveUpdater {
        /// Target object whose `Updater` component should be removed.
        target: ObjectId,
    },
    /// Changes an object's scene membership at this exact timeline position.
    /// Snapshot restore makes the event reversible when scrubbing backwards.
    SetSceneMember {
        target: ObjectId,
        scene: Option<SceneId>,
    },
    /// A scene transition spanning the boundary between two scenes.
    Transition {
        /// The outgoing scene.
        from: SceneId,
        /// The incoming scene.
        to: SceneId,
        /// The transition effect to apply.
        transition_type: crate::transition::TransitionType,
    },
}

/// Timeline-owned sampling parameters for one glTF Action.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct GltfAnimationSpec {
    pub target: ObjectId,
    pub animation_index: usize,
    pub source_duration: f64,
    pub speed: f64,
    pub looped: bool,
    pub reverse: bool,
    pub transition: f64,
    pub start_time: f64,
}

/// Specification of a property tween animation suitable for storage and serialization.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct AnimationSpec {
    /// The identity of the target Mobject to animate.
    pub target: ObjectId,
    /// The serializable lens specifying what property to interpolate.
    pub lens: PropertyLensSpec,
    /// Easing curve or physical spring rate function.
    pub rate_func: RateFunc,
    /// Initial delay in seconds before the animation starts.
    pub delay: f64,
    /// High-level animation label (e.g. "Write", "Grow", "SpinIn").
    pub label: Option<String>,
}

/// A fully serializable description of a Mobject property lens.
///
/// Under Bevy ECS, `PropertyLens` contains `dyn AnimatableLens` which is not serializable.
/// `PropertyLensSpec` bridges the gap by storing a serializable version that maps to the Bevy component types.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum PropertyLensSpec {
    Translation {
        from: gaanim_core::glam::DVec3,
        to: gaanim_core::glam::DVec3,
    },
    Rotation {
        from: gaanim_core::glam::DQuat,
        to: gaanim_core::glam::DQuat,
    },
    Scale {
        from: gaanim_core::glam::DVec3,
        to: gaanim_core::glam::DVec3,
    },
    Opacity {
        from: f32,
        to: f32,
    },
    FillColor {
        from: Color,
        to: Color,
    },
    StrokeColor {
        from: Color,
        to: Color,
    },
    StrokeWidth {
        from: f64,
        to: f64,
    },
    Material3D {
        from: gaanim_scene::Material3D,
        to: gaanim_scene::Material3D,
    },
    PathCompletion {
        from: f64,
        to: f64,
    },
    PathMorph {
        from: BezPath,
        to: BezPath,
    },
    /// Cross-fade the fill alpha from `from` to `to` (both in `[0, 1]`).
    /// Used by the Write animation to reveal the fill after the path
    /// has been fully drawn. Applied by inserting/updating a
    /// `gaanim_animation::FillDrawProgress` component on the target.
    FillDrawProgress {
        from: f32,
        to: f32,
    },
    FillLevel {
        from: f64,
        to: f64,
    },
    SurroundingRectTargets {
        from: Vec<ObjectId>,
        to: Vec<ObjectId>,
    },
    CameraPosition {
        from: gaanim_core::glam::DVec3,
        to: gaanim_core::glam::DVec3,
    },
    #[cfg_attr(feature = "serde", serde(skip))]
    CameraPositionSource {
        from: gaanim_core::glam::DVec3,
        to: gaanim_animation::TrackingEndpoint,
    },
    CameraRotation {
        from: gaanim_core::glam::DQuat,
        to: gaanim_core::glam::DQuat,
    },
    CameraZoom {
        from: f64,
        to: f64,
    },
    #[cfg_attr(feature = "serde", serde(skip))]
    CameraZoomSource {
        from: f64,
        to: gaanim_animation::TrackingScalar,
    },
    #[cfg_attr(feature = "serde", serde(skip))]
    CameraRotationSource {
        from: f64,
        to: gaanim_animation::TrackingScalar,
    },
    /// Select orthographic projection while tweening its zoom.
    CameraOrthographic {
        from: f64,
        to: f64,
    },
    /// Restore the complete authored camera rig to its 2D defaults.
    CameraReset {
        from_position: gaanim_core::glam::DVec3,
        from_rotation: gaanim_core::glam::DQuat,
        from_target: gaanim_core::glam::DVec3,
        from_up: gaanim_core::glam::DVec3,
        from_zoom: f64,
        to_zoom: f64,
    },
    /// Center the camera on a moving mobject for the lifetime of the clip.
    CameraFollow {
        target: ObjectId,
    },
    #[cfg_attr(feature = "serde", serde(skip))]
    CameraFollowEndpoint {
        target: gaanim_animation::TrackingEndpoint,
        from: gaanim_core::glam::DVec3,
        offset: gaanim_core::glam::DVec3,
        offset_space: gaanim_animation::FollowOffsetSpace,
        lag: f64,
    },
    #[cfg_attr(feature = "serde", serde(skip))]
    CameraFrameDynamic {
        targets: Vec<bevy::prelude::Entity>,
        from_position: gaanim_core::glam::DVec3,
        from_zoom: f64,
        margins: [f64; 4],
        frame_width: f64,
        frame_height: f64,
    },
    /// A deterministic damped shake around `origin`.
    CameraShake {
        origin: gaanim_core::glam::DVec3,
        amplitude: f64,
        frequency: f64,
    },
    /// Tween the camera's look-at target (for orbit/look_at).
    CameraTarget {
        from: gaanim_core::glam::DVec3,
        to: gaanim_core::glam::DVec3,
    },
    /// Interpolate a complete look-at pose and derive rotation atomically.
    CameraLookAt {
        from_position: gaanim_core::glam::DVec3,
        from_target: gaanim_core::glam::DVec3,
        eye: gaanim_core::glam::DVec3,
        target: gaanim_core::glam::DVec3,
        up: gaanim_core::glam::DVec3,
    },
    /// Orbit from an authored pose without contracting the eye-target radius.
    CameraOrbit {
        from_position: gaanim_core::glam::DVec3,
        target: gaanim_core::glam::DVec3,
        up: gaanim_core::glam::DVec3,
        delta_yaw: f64,
        delta_pitch: f64,
    },
    #[cfg_attr(feature = "serde", serde(skip))]
    CameraLookAtSource {
        from_position: gaanim_core::glam::DVec3,
        from_target: gaanim_core::glam::DVec3,
        from_rotation: gaanim_core::glam::DQuat,
        eye: gaanim_animation::TrackingEndpoint,
        target: gaanim_animation::TrackingEndpoint,
        up: gaanim_core::glam::DVec3,
    },
    /// Tween perspective projection fov/near/far.
    CameraPerspective {
        from_fov: f64,
        to_fov: f64,
        from_near: f64,
        to_near: f64,
        from_far: f64,
        to_far: f64,
    },
    /// Move the entity's translation along a Bézier path. Sampled at
    /// the rate-function-eased `t` and applied as the entity's
    /// world-space translation.
    PathFollow {
        path: BezPath,
    },
    PathFollow3D {
        points: Vec<gaanim_core::glam::DVec3>,
    },
    /// Tween a reactive FloatSignal.
    SignalFloat {
        from: f64,
        to: f64,
    },
    /// Trims the path in a sliding range window.
    PathRange {
        from: f64,
        to: f64,
        time_width: f64,
    },
    Custom {
        type_name: String,
        params: String,
    },
}

impl PropertyLensSpec {
    /// Converts this serializable specification into a runtime Bevy `PropertyLens`.
    pub fn to_lens(&self) -> PropertyLens {
        match self {
            Self::Translation { from, to } => PropertyLens::Translation {
                from: *from,
                to: *to,
            },
            Self::Rotation { from, to } => PropertyLens::Rotation {
                from: *from,
                to: *to,
            },
            Self::Scale { from, to } => PropertyLens::Scale {
                from: *from,
                to: *to,
            },
            Self::Opacity { from, to } => PropertyLens::Opacity {
                from: *from,
                to: *to,
            },
            Self::FillColor { from, to } => PropertyLens::FillColor {
                from: *from,
                to: *to,
            },
            Self::StrokeColor { from, to } => PropertyLens::StrokeColor {
                from: *from,
                to: *to,
            },
            Self::StrokeWidth { from, to } => PropertyLens::StrokeWidth {
                from: *from,
                to: *to,
            },
            Self::Material3D { from, to } => PropertyLens::Material3D {
                from: *from,
                to: *to,
            },
            Self::PathCompletion { from, to } => PropertyLens::PathCompletion {
                from: *from,
                to: *to,
            },
            Self::PathMorph { from, to } => PropertyLens::PathMorph {
                from: from.clone(),
                to: to.clone(),
                table: gaanim_animation::MorphTable,
            },
            Self::FillDrawProgress { from, to } => PropertyLens::FillDrawProgress {
                from: *from,
                to: *to,
            },
            Self::FillLevel { from, to } => PropertyLens::FillLevel {
                from: *from,
                to: *to,
            },
            Self::SurroundingRectTargets { from, to } => PropertyLens::SurroundingRectTargets {
                from: from.clone(),
                to: to.clone(),
            },
            Self::CameraPosition { from, to } => PropertyLens::CameraPosition {
                from: *from,
                to: *to,
            },
            Self::CameraPositionSource { from, to } => PropertyLens::CameraPositionSource {
                from: *from,
                to: to.clone(),
            },
            Self::CameraRotation { from, to } => PropertyLens::CameraRotation {
                from: *from,
                to: *to,
            },
            Self::CameraZoom { from, to } => PropertyLens::CameraZoom {
                from: *from,
                to: *to,
            },
            Self::CameraZoomSource { from, to } => PropertyLens::CameraZoomSource {
                from: *from,
                to: to.clone(),
            },
            Self::CameraRotationSource { from, to } => PropertyLens::CameraRotationSource {
                from: *from,
                to: to.clone(),
            },
            Self::CameraOrthographic { from, to } => PropertyLens::CameraOrthographic {
                from: *from,
                to: *to,
            },
            Self::CameraReset {
                from_position,
                from_rotation,
                from_target,
                from_up,
                from_zoom,
                to_zoom,
            } => PropertyLens::CameraReset {
                from_position: *from_position,
                from_rotation: *from_rotation,
                from_target: *from_target,
                from_up: *from_up,
                from_zoom: *from_zoom,
                to_zoom: *to_zoom,
            },
            Self::CameraFollow { target } => PropertyLens::CameraFollow { target: *target },
            Self::CameraFollowEndpoint {
                target,
                from,
                offset,
                offset_space,
                lag,
            } => PropertyLens::CameraFollowEndpoint {
                target: target.clone(),
                from: *from,
                offset: *offset,
                offset_space: *offset_space,
                lag: *lag,
            },
            Self::CameraFrameDynamic {
                targets,
                from_position,
                from_zoom,
                margins,
                frame_width,
                frame_height,
            } => PropertyLens::CameraFrameDynamic {
                targets: targets.clone(),
                from_position: *from_position,
                from_zoom: *from_zoom,
                margins: *margins,
                frame_width: *frame_width,
                frame_height: *frame_height,
            },
            Self::CameraShake {
                origin,
                amplitude,
                frequency,
            } => PropertyLens::CameraShake {
                origin: *origin,
                amplitude: *amplitude,
                frequency: *frequency,
            },
            Self::CameraTarget { from, to } => PropertyLens::CameraTarget {
                from: *from,
                to: *to,
            },
            Self::CameraLookAt {
                from_position,
                from_target,
                eye,
                target,
                up,
            } => PropertyLens::CameraLookAt {
                from_position: *from_position,
                from_target: *from_target,
                eye: *eye,
                target: *target,
                up: *up,
            },
            Self::CameraOrbit {
                from_position,
                target,
                up,
                delta_yaw,
                delta_pitch,
            } => PropertyLens::CameraOrbit {
                from_position: *from_position,
                target: *target,
                up: *up,
                delta_yaw: *delta_yaw,
                delta_pitch: *delta_pitch,
            },
            Self::CameraLookAtSource {
                from_position,
                from_target,
                from_rotation,
                eye,
                target,
                up,
            } => PropertyLens::CameraLookAtSource {
                from_position: *from_position,
                from_target: *from_target,
                from_rotation: *from_rotation,
                eye: eye.clone(),
                target: target.clone(),
                up: *up,
            },
            Self::CameraPerspective {
                from_fov,
                to_fov,
                from_near,
                to_near,
                from_far,
                to_far,
            } => PropertyLens::CameraPerspective {
                from_fov: *from_fov,
                to_fov: *to_fov,
                from_near: *from_near,
                to_near: *to_near,
                from_far: *from_far,
                to_far: *to_far,
            },
            Self::PathFollow { path } => PropertyLens::PathFollow {
                path: std::sync::Arc::new(path.clone()),
            },
            Self::PathFollow3D { points } => PropertyLens::PathFollow3D {
                points: std::sync::Arc::new(points.clone()),
            },
            Self::SignalFloat { from, to } => PropertyLens::SignalFloat {
                from: *from,
                to: *to,
            },
            Self::PathRange {
                from,
                to,
                time_width,
            } => PropertyLens::PathRange {
                from: *from,
                to: *to,
                time_width: *time_width,
            },
            Self::Custom { type_name, .. } => PropertyLens::Custom(Box::new(DummyLens {
                type_name: type_name.clone(),
            })),
        }
    }
}

/// Fallback lens used when restoring custom plugins that aren't registered at runtime.
#[derive(Debug, Clone)]
pub struct DummyLens {
    type_name: String,
}

impl gaanim_animation::AnimatableLens for DummyLens {
    fn interpolate(
        &self,
        _world: &mut bevy::prelude::World,
        _entity: bevy::prelude::Entity,
        _t: f64,
    ) {
        // Fallback: does nothing
    }

    fn clone_box(&self) -> Box<dyn gaanim_animation::AnimatableLens> {
        Box::new(self.clone())
    }

    fn type_name(&self) -> &'static str {
        // Leaking the string into static lifetime is acceptable for a fallback/dummy error recovery
        Box::leak(self.type_name.clone().into_boxed_str())
    }
}
