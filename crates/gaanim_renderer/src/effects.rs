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
            offset: gaanim_core::glam::DVec2::new(0.08, -0.08),
            blur_radius: 0.06,
            color: Color::from_rgba8(0, 0, 0, 128),
        }
    }
}

/// Component: composites the element onto what is drawn beneath it with a
/// blend mode instead of plain source-over (e.g. a multiply highlighter).
///
/// The element is drawn in its own layer, so inside an isolated group (an
/// opacity group, a clip or a transition reveal) it blends with that group's
/// content only.
#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub struct ElementBlend(pub gaanim_core::peniko::BlendMode);

/// Component: where a stroke sits relative to a closed contour.
///
/// Without it a closed contour keeps its stroke inside the shape, so a
/// `write` draws a constant width, and an open path centers its stroke.
/// `Outside` draws the whole width beyond the contour, e.g. for a text halo.
/// Open paths always center their stroke.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum StrokeAlign {
    #[default]
    Inside,
    Center,
    Outside,
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
            radius: 0.16,
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

/// How the region framed by a [`CameraView`] source fills its screen.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum CameraViewFit {
    /// Scale uniformly so the whole framed region stays visible.
    #[default]
    Contain,
    /// Scale uniformly so the framed region covers the whole screen.
    Cover,
    /// Scale each axis independently so the region matches the screen.
    Stretch,
}

/// Paint behind the content of a [`CameraView`], inside its screen.
#[derive(Debug, Clone, Default, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum CameraViewBackground {
    /// The canvas background as the view's camera sees it.
    #[default]
    Canvas,
    /// No backdrop: the screen's own fill shows behind the content.
    None,
    /// A brush in the screen's local coordinates.
    Brush(gaanim_core::peniko::Brush),
}

/// Component: turns a closed vector leaf into a screen that shows the scene
/// as a second camera sees it.
///
/// The camera frames `source`: its local bounds under its world transform, so
/// moving, scaling or rotating the source pans, zooms or rotates the view.
/// With an explicit `zoom` the source only sets where the camera looks (its
/// center) and its rotation, and `zoom` scales world units into the screen's
/// local units. The screen draws its shadow, glow and fill, then the backdrop
/// and the scene clipped to its outline, then its stroke on top. The screen,
/// the source and every `exclude` subtree are left out of the view, and views
/// never show the content of other views.
#[derive(Component, Debug, Clone)]
pub struct CameraView {
    /// Drawable whose bounds frame the captured region.
    pub source: Entity,
    pub fit: CameraViewFit,
    pub background: CameraViewBackground,
    /// Roots of subtrees the view does not show.
    pub exclude: Vec<Entity>,
    /// Explicit magnification; `None` derives it from the source's size.
    pub zoom: Option<gaanim_animation::TrackingScalar>,
    /// View layers shown besides the drawables on no layer.
    pub layers: Vec<std::sync::Arc<str>>,
}

/// Component: puts a drawable on a named view layer. The main camera does not
/// draw it; only camera views that list the layer show it.
#[derive(Component, Debug, Clone, PartialEq, Eq, Hash)]
pub struct ViewLayer(pub std::sync::Arc<str>);

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
