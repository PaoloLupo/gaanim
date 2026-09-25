//! Scene transition types for multi-scene timelines.

use gaanim_core::ObjectId;
use gaanim_core::glam::DVec2;
use gaanim_core::peniko::Color;
use gaanim_math::RateFunc;

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
    /// The incoming scene slides in over the outgoing one, which stays still
    /// and is covered. See `Push` for moving both scenes together.
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
    /// A straight edge sweeps across the frame, revealing the incoming scene.
    ///
    /// `direction` is the unit vector the edge travels along; `feather` is the
    /// soft-edge width as a fraction of the travel distance.
    Wipe {
        duration: f64,
        direction: DVec2,
        feather: f64,
    },
    /// A radial hand sweeps clockwise around the frame center.
    ///
    /// `start_angle` is in degrees, counter-clockwise from +x (90 = twelve
    /// o'clock).
    ClockWipe { duration: f64, start_angle: f64 },
    /// A shape grows from `center` (world units) until it covers the frame.
    Iris {
        duration: f64,
        center: DVec2,
        shape: IrisShape,
    },
    /// `count` parallel slats open together. `angle` is the slat orientation in
    /// degrees (0 = horizontal slats that open downwards).
    Blinds {
        duration: f64,
        count: u32,
        angle: f64,
    },
    /// The incoming scene pushes the outgoing one out of the frame.
    Push {
        duration: f64,
        direction: SlideDirection,
    },
    /// Another transition shaped by an easing curve and/or decorated by an
    /// overlay drawn above the cut.
    Styled {
        base: Box<TransitionType>,
        easing: Option<RateFunc>,
        overlay: Option<TransitionOverlay>,
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
            Self::Wipe { duration, .. }
            | Self::ClockWipe { duration, .. }
            | Self::Iris { duration, .. }
            | Self::Blinds { duration, .. }
            | Self::Push { duration, .. } => *duration,
            Self::Styled { base, .. } => base.duration(),
        }
    }

    /// The underlying effect, without easing or overlay decoration.
    pub fn base(&self) -> &TransitionType {
        match self {
            Self::Styled { base, .. } => base.base(),
            other => other,
        }
    }

    /// Overlay drawn above the cut, if any.
    pub fn overlay(&self) -> Option<&TransitionOverlay> {
        match self {
            Self::Styled { overlay, base, .. } => overlay.as_ref().or_else(|| base.overlay()),
            _ => None,
        }
    }

    /// Eased progress of the effect at linear progress `t` in `[0, 1]`.
    ///
    /// Without an explicit easing, the original transitions stay linear and
    /// the vector reveals (wipes, iris, blinds, push) use `Smooth`. The result
    /// may overshoot `[0, 1]` for springs and back curves.
    pub fn eased_progress(&self, t: f64) -> f64 {
        match self {
            Self::Styled {
                easing: Some(easing),
                ..
            } => easing.evaluate(t),
            Self::Styled { base, .. } => base.eased_progress(t),
            Self::Wipe { .. }
            | Self::ClockWipe { .. }
            | Self::Iris { .. }
            | Self::Blinds { .. }
            | Self::Push { .. } => RateFunc::Smooth.evaluate(t),
            _ => t,
        }
    }

    /// Wrap this transition with an easing curve (replacing a previous one).
    pub fn with_easing(self, easing: RateFunc) -> Self {
        match self {
            Self::Styled { base, overlay, .. } => Self::Styled {
                base,
                easing: Some(easing),
                overlay,
            },
            base => Self::Styled {
                base: Box::new(base),
                easing: Some(easing),
                overlay: None,
            },
        }
    }

    /// Wrap this transition with an overlay drawn above the cut.
    pub fn with_overlay(self, overlay: TransitionOverlay) -> Self {
        match self {
            Self::Styled { base, easing, .. } => Self::Styled {
                base,
                easing,
                overlay: Some(overlay),
            },
            base => Self::Styled {
                base: Box::new(base),
                easing: None,
                overlay: Some(overlay),
            },
        }
    }

    /// Creates a linear wipe travelling along `direction`.
    pub fn wipe(duration: f64, direction: DVec2, feather: f64) -> Self {
        Self::Wipe {
            duration,
            direction: direction.try_normalize().unwrap_or(DVec2::NEG_X),
            feather: feather.max(0.0),
        }
    }

    /// Creates a clockwise radial wipe starting at `start_angle` degrees.
    pub fn clock_wipe(duration: f64, start_angle: f64) -> Self {
        Self::ClockWipe {
            duration,
            start_angle,
        }
    }

    /// Creates an iris opening from `center`.
    pub fn iris(duration: f64, center: DVec2, shape: IrisShape) -> Self {
        Self::Iris {
            duration,
            center,
            shape,
        }
    }

    /// Creates a venetian-blinds reveal.
    pub fn blinds(duration: f64, count: u32, angle: f64) -> Self {
        Self::Blinds {
            duration,
            count: count.max(1),
            angle,
        }
    }

    /// Creates a push transition.
    pub fn push(duration: f64, direction: SlideDirection) -> Self {
        Self::Push {
            duration,
            direction,
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

impl SlideDirection {
    /// Unit vector of the motion in a Y-up frame.
    pub fn vector(self) -> DVec2 {
        match self {
            Self::Left => DVec2::NEG_X,
            Self::Right => DVec2::X,
            Self::Up => DVec2::Y,
            Self::Down => DVec2::NEG_Y,
        }
    }
}

/// Outline grown by an iris transition.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum IrisShape {
    Circle,
    Diamond,
    Square,
    /// Five-pointed star.
    Star,
    /// The vector outline of an authored drawable.
    Drawable(ObjectId),
}

/// Overlay drawn above a transition without changing any segment timing.
///
/// The overlay is centered on the transition midpoint (the cut itself for
/// `Cut`) and lasts `duration` seconds.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum TransitionOverlay {
    /// A full-frame flash of `color` peaking at the cut.
    Flash { color: Color, duration: f64 },
    /// Soft, warm light blobs drifting across the frame. Deterministic in `seed`.
    LightLeak {
        seed: u64,
        /// Base hue in turns (`0.0` red, `0.1` orange, `0.6` blue).
        hue: f64,
        duration: f64,
        intensity: f64,
    },
}

impl TransitionOverlay {
    /// Duration of the overlay in seconds.
    pub fn duration(&self) -> f64 {
        match self {
            Self::Flash { duration, .. } | Self::LightLeak { duration, .. } => *duration,
        }
    }

    /// Creates a flash overlay.
    pub fn flash(color: Color, duration: f64) -> Self {
        Self::Flash { color, duration }
    }

    /// Creates a light-leak overlay.
    pub fn light_leak(seed: u64, hue: f64, duration: f64, intensity: f64) -> Self {
        Self::LightLeak {
            seed,
            hue,
            duration,
            intensity,
        }
    }

    /// Overlay window `(start, end)` for a transition clip spanning
    /// `[clip_start, clip_start + clip_duration]`.
    pub fn window(&self, clip_start: f64, clip_duration: f64) -> (f64, f64) {
        let mid = clip_start + clip_duration.max(0.0) * 0.5;
        let half = self.duration().max(0.0) * 0.5;
        (mid - half, mid + half)
    }
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
