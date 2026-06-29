//! Scene transition types for multi-scene timelines.

use gaanim_core::ObjectId;
use gaanim_core::glam::DVec2;
use gaanim_core::peniko::Color;

use crate::clip::SceneId;

/// The type of transition between two scenes.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum TransitionType {
    /// Instant cut (0 duration).
    Cut,
    /// Cross-fade: outgoing scene fades out, incoming scene fades in.
    CrossFade { duration: f64 },
    /// Fade to a color, then fade in from that color.
    FadeThrough { duration: f64, fade_color: Color },
    /// Outgoing scene slides out, incoming scene slides in.
    Slide {
        duration: f64,
        direction: SlideDirection,
    },
    /// Zoom into a point on the outgoing scene, revealing the incoming scene.
    ZoomThrough {
        duration: f64,
        center: DVec2,
        max_zoom: f64,
    },
    /// Morph specific mobjects from the outgoing scene into the incoming scene.
    Morph {
        duration: f64,
        mappings: Vec<MorphMapping>,
    },
}

impl TransitionType {
    /// Returns the duration of this transition (0.0 for Cut).
    pub fn duration(&self) -> f64 {
        match self {
            Self::Cut => 0.0,
            Self::CrossFade { duration } => *duration,
            Self::FadeThrough { duration, .. } => *duration,
            Self::Slide { duration, .. } => *duration,
            Self::ZoomThrough { duration, .. } => *duration,
            Self::Morph { duration, .. } => *duration,
        }
    }

    /// Creates a cut (instant) transition.
    pub fn cut() -> Self {
        Self::Cut
    }

    /// Creates a cross-fade transition.
    pub fn cross_fade(duration: f64) -> Self {
        Self::CrossFade { duration }
    }

    /// Creates a fade-through-color transition.
    pub fn fade_through(duration: f64, fade_color: Color) -> Self {
        Self::FadeThrough {
            duration,
            fade_color,
        }
    }

    /// Creates a slide transition.
    pub fn slide(duration: f64, direction: SlideDirection) -> Self {
        Self::Slide {
            duration,
            direction,
        }
    }
}

/// Direction for slide transitions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum SlideDirection {
    Left,
    Right,
    Up,
    Down,
}

/// Maps a source mobject to a target mobject for morph transitions.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct MorphMapping {
    pub source: ObjectId,
    pub target: ObjectId,
    pub property: MorphProperty,
}

/// Which property to morph during a morph transition.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum MorphProperty {
    Shape,
    Position,
    Color,
    All,
}

/// Metadata about a connection between two scenes.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct SceneConnection {
    pub from: SceneId,
    pub to: SceneId,
    pub transition: TransitionType,
}
