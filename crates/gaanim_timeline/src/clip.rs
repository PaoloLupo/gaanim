use gaanim_animation::PropertyLens;
use gaanim_core::{ObjectId, peniko::Color};
use gaanim_math::RateFunc;

// We use slotmap to manage IDs cleanly without generational index validation bugs
slotmap::new_key_type! {
    /// Unique identifier for an individual timeline clip.
    pub struct ClipId;
    /// Unique identifier for an individual timeline track.
    pub struct TrackId;
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
    /// A slide presentation breakpoint (interactive slide transition).
    Breakpoint,
    /// Named segment start marker for dividing long sequences.
    SegmentStart(String),
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
    PathCompletion {
        from: f64,
        to: f64,
    },
    /// Cross-fade the fill alpha from `from` to `to` (both in `[0, 1]`).
    /// Used by the Write animation to reveal the fill after the path
    /// has been fully drawn. Applied by inserting/updating a
    /// `gaanim_animation::FillDrawProgress` component on the target.
    FillDrawProgress {
        from: f32,
        to: f32,
    },
    CameraPosition {
        from: gaanim_core::glam::DVec3,
        to: gaanim_core::glam::DVec3,
    },
    CameraRotation {
        from: gaanim_core::glam::DQuat,
        to: gaanim_core::glam::DQuat,
    },
    CameraZoom {
        from: f64,
        to: f64,
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
            Self::PathCompletion { from, to } => PropertyLens::PathCompletion {
                from: *from,
                to: *to,
            },
            Self::FillDrawProgress { from, to } => PropertyLens::FillDrawProgress {
                from: *from,
                to: *to,
            },
            Self::CameraPosition { from, to } => PropertyLens::CameraPosition {
                from: *from,
                to: *to,
            },
            Self::CameraRotation { from, to } => PropertyLens::CameraRotation {
                from: *from,
                to: *to,
            },
            Self::CameraZoom { from, to } => PropertyLens::CameraZoom {
                from: *from,
                to: *to,
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
