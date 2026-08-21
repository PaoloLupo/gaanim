use bevy::prelude::{Component, Entity};
use gaanim_core::peniko::Color;

/// Component: Adds a soft drop shadow effect to a 2D Mobject.
#[derive(Component, Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct DropShadow {
    /// Horizontal and vertical offset of the shadow in world units.
    pub offset: gaanim_core::glam::DVec2,
    /// Standard deviation of the Gaussian blur filter.
    pub blur_radius: f64,
    /// Shadow color (usually semi-transparent black).
    pub color: Color,
}

impl Default for DropShadow {
    fn default() -> Self {
        Self {
            offset: gaanim_core::glam::DVec2::new(5.0, -5.0),
            blur_radius: 5.0,
            color: Color::from_rgba8(0, 0, 0, 128),
        }
    }
}

/// Component: Adds an outer glow outline effect to a 2D Mobject.
#[derive(Component, Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Glow {
    /// Total spread radius of the glow outline.
    pub radius: f64,
    /// Intensity multiplier of the glow color.
    pub intensity: f32,
    /// Color of the outer glow.
    pub color: Color,
}

impl Default for Glow {
    fn default() -> Self {
        Self {
            radius: 8.0,
            intensity: 1.0,
            color: Color::from_rgba8(255, 255, 255, 255),
        }
    }
}

/// Component: Applies a Gaussian blur filter to the Mobject.
#[derive(Component, Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct GaussianBlur {
    /// Standard deviation of the Gaussian filter.
    pub sigma: f64,
}

impl Default for GaussianBlur {
    fn default() -> Self {
        Self { sigma: 2.0 }
    }
}

/// Component: Clips the rendering of this Mobject and all its children using a vector path.
#[derive(Component, Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ClipMask {
    /// The geometric clipping outline path.
    pub path: gaanim_core::kurbo::BezPath,
    /// The fill rule (NonZero or EvenOdd) used to interpret path interior.
    pub rule: gaanim_core::peniko::Fill,
    /// Vector leaves that define this mask.  They are resolved each frame in
    /// `SceneSet::DerivedGeometry`, after hierarchy transforms have settled.
    pub sources: Vec<Entity>,
    /// Interpret the mask as its complement. Inversion is represented as an
    /// even-odd layer with a deliberately large enclosing rectangle.
    pub invert: bool,
}

/// A retained reactive vector boolean. Sources are vector leaves in world
/// space; the result path is rebuilt in `SceneSet::DerivedGeometry`.
#[derive(Component, Debug, Clone)]
pub struct BooleanBinding {
    pub sources: Vec<Entity>,
    pub op: gaanim_objects::boolean::BooleanOp,
    pub tolerance: f64,
    pub rule: gaanim_objects::boolean::BooleanFillRule,
}

#[derive(Component, Debug, Clone)]
pub struct FillLevelBinding {
    pub sources: Vec<Entity>,
    pub direction: gaanim_scene::FillDirection,
}

/// A stroke-only live copy of a fill-level source silhouette.
#[derive(Component, Debug, Clone)]
pub struct VectorOutlineBinding {
    pub sources: Vec<Entity>,
}

impl Default for ClipMask {
    fn default() -> Self {
        Self {
            path: gaanim_core::kurbo::BezPath::default(),
            rule: gaanim_core::peniko::Fill::NonZero,
            sources: Vec::new(),
            invert: false,
        }
    }
}
