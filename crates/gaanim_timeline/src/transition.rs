//! Scene transition types for multi-scene timelines.

use gaanim_core::ObjectId;
use gaanim_core::glam::DVec2;
use gaanim_core::peniko::Color;
use gaanim_math::RateFunc;

use crate::clip::SceneId;
use crate::sound::SoundCue;

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
    /// Keyed morph ("magic move"): drawables of both segments that share a
    /// key morph into each other and the rest cross-fade. The authoring layer
    /// resolves the keys into a [`TransitionType::Morph`] when the scene is
    /// compiled; the runtime treats an unresolved one as a morph without pairs.
    MagicMove { duration: f64, key: MagicMoveKey },
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
    /// Each segment is rendered alone and `shader` blends them, like the
    /// gl-transitions `transition(uv)` with `getFromColor`/`getToColor`.
    #[cfg_attr(feature = "serde", serde(skip))]
    Shader {
        duration: f64,
        shader: std::sync::Arc<gaanim_scene::TransitionShader>,
    },
    /// Another transition shaped by an easing curve, decorated by an
    /// overlay drawn above the cut and/or accompanied by a sound effect that
    /// starts with the transition.
    Styled {
        base: Box<TransitionType>,
        easing: Option<RateFunc>,
        overlay: Option<TransitionOverlay>,
        sound: Option<SoundCue>,
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
            Self::Morph { duration, .. } | Self::MagicMove { duration, .. } => *duration,
            Self::Wipe { duration, .. }
            | Self::ClockWipe { duration, .. }
            | Self::Iris { duration, .. }
            | Self::Blinds { duration, .. }
            | Self::Push { duration, .. } => *duration,
            Self::Shader { duration, .. } => *duration,
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

    /// Sound effect that starts with the transition, if any.
    pub fn sound(&self) -> Option<&SoundCue> {
        match self {
            Self::Styled { sound, base, .. } => sound.as_ref().or_else(|| base.sound()),
            _ => None,
        }
    }

    /// Eased progress of the effect at linear progress `t` in `[0, 1]`.
    ///
    /// Without an explicit easing, the original transitions and shaders stay
    /// linear and the vector reveals (wipes, iris, blinds, push) use `Smooth`. The result
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
            Self::Styled {
                base,
                overlay,
                sound,
                ..
            } => Self::Styled {
                base,
                easing: Some(easing),
                overlay,
                sound,
            },
            base => Self::Styled {
                base: Box::new(base),
                easing: Some(easing),
                overlay: None,
                sound: None,
            },
        }
    }

    /// Wrap this transition with an overlay drawn above the cut.
    pub fn with_overlay(self, overlay: TransitionOverlay) -> Self {
        match self {
            Self::Styled {
                base,
                easing,
                sound,
                ..
            } => Self::Styled {
                base,
                easing,
                overlay: Some(overlay),
                sound,
            },
            base => Self::Styled {
                base: Box::new(base),
                easing: None,
                overlay: Some(overlay),
                sound: None,
            },
        }
    }

    /// Play `sound` when the transition starts (replacing a previous one).
    ///
    /// The cue carries no absolute time: the scene resolves it at the start
    /// of the segment the transition enters, so it follows that segment.
    pub fn with_sound(self, sound: SoundCue) -> Self {
        match self {
            Self::Styled {
                base,
                easing,
                overlay,
                ..
            } => Self::Styled {
                base,
                easing,
                overlay,
                sound: Some(sound),
            },
            base => Self::Styled {
                base: Box::new(base),
                easing: None,
                overlay: None,
                sound: Some(sound),
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

    /// Creates a keyed morph between the drawables of two segments.
    pub fn magic_move(duration: f64, key: MagicMoveKey) -> Self {
        Self::MagicMove { duration, key }
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

/// What identifies the same drawable on both sides of a magic move.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum MagicMoveKey {
    /// The name given to the drawable by the author, or else its SVG `id`.
    Name,
    /// Only the `id` attribute of an imported SVG group or path.
    SvgId,
}

impl MagicMoveKey {
    /// Parse the Python spelling: `"name"` or `"id"`.
    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "name" => Some(Self::Name),
            "id" | "svg_id" => Some(Self::SvgId),
            _ => None,
        }
    }

    /// The Python spelling of this key.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Name => "name",
            Self::SvgId => "id",
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sound_survives_easing_and_overlay_wrappers() {
        let cue = SoundCue::new("whoosh.wav").unwrap();
        let transition = TransitionType::Slide {
            duration: 0.5,
            direction: SlideDirection::Left,
        }
        .with_sound(cue.clone())
        .with_easing(RateFunc::Linear)
        .with_overlay(TransitionOverlay::Flash {
            color: Color::WHITE,
            duration: 0.2,
        });
        assert_eq!(transition.sound(), Some(&cue));
        assert_eq!(transition.duration(), 0.5);
        assert!(matches!(transition.base(), TransitionType::Slide { .. }));
        assert!(transition.overlay().is_some());
        assert!(TransitionType::Cut.sound().is_none());
    }

    #[test]
    fn with_sound_replaces_a_previous_sound() {
        let first = SoundCue::new("a.wav").unwrap();
        let second = SoundCue::new("b.wav").unwrap();
        let transition = TransitionType::CrossFade { duration: 0.3 }
            .with_sound(first)
            .with_sound(second.clone());
        assert_eq!(transition.sound(), Some(&second));
    }

    #[test]
    fn magic_move_keys_parse_their_python_spelling() {
        assert_eq!(MagicMoveKey::parse("name"), Some(MagicMoveKey::Name));
        assert_eq!(MagicMoveKey::parse(" ID "), Some(MagicMoveKey::SvgId));
        assert_eq!(MagicMoveKey::parse("color"), None);
        for key in [MagicMoveKey::Name, MagicMoveKey::SvgId] {
            assert_eq!(MagicMoveKey::parse(key.as_str()), Some(key));
        }
    }

    #[test]
    fn magic_move_keeps_its_duration_under_styling() {
        let transition =
            TransitionType::magic_move(0.8, MagicMoveKey::Name).with_easing(RateFunc::Smooth);
        assert_eq!(transition.duration(), 0.8);
        assert!(matches!(
            transition.base(),
            TransitionType::MagicMove {
                key: MagicMoveKey::Name,
                ..
            }
        ));
    }
}
