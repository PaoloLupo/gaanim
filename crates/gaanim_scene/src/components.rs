use bevy::prelude::{Component, Resource};
use gaanim_core::kurbo::{Affine, BezPath, Stroke};
use gaanim_core::peniko::{Brush, ImageBrush};
use gaanim_math::Bounds3D;
use std::sync::Arc;

/// Represents the fill style of a visual Mobject.
///
/// Contains an optional `peniko::Brush` which directly represents solid colors,
/// linear/radial/conic gradients, or image-backed textures.
#[derive(Component, Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Default)]
pub struct FillBrush(pub Option<Brush>);

impl FillBrush {
    /// Creates a solid color fill.
    pub fn color(color: gaanim_core::peniko::Color) -> Self {
        Self(Some(Brush::Solid(color)))
    }

    /// Creates a generic brush-backed fill.
    pub fn brush(brush: Brush) -> Self {
        Self(Some(brush))
    }

    /// Creates a transparent/no-fill style.
    pub fn transparent() -> Self {
        Self(None)
    }
}

/// A decoded raster image drawn by the Vello backend.
///
/// The image owns a reference-counted pixel blob, so cloned mobjects and
/// renderer fragments share the same decoded texture data. `local_transform`
/// positions the image relative to the mobject origin before the regular
/// spatial transform hierarchy is applied.
#[derive(Component, Debug, Clone, PartialEq)]
pub struct RasterImage {
    pub image: Option<ImageBrush>,
    pub local_transform: Affine,
}

impl RasterImage {
    /// A vector mobject without raster content.
    pub fn none() -> Self {
        Self {
            image: None,
            local_transform: Affine::IDENTITY,
        }
    }

    /// Raster content positioned in the mobject's local coordinates.
    pub fn new(image: ImageBrush, local_transform: Affine) -> Self {
        Self {
            image: Some(image),
            local_transform,
        }
    }
}

/// Represents the outline stroke style of a visual Mobject.
#[derive(Component, Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Default)]
pub struct StrokeBrush {
    /// The outline color or brush. `None` represents no outline.
    pub brush: Option<Brush>,
    /// The geometric line properties (width, caps, joins, miter limit, dash patterns).
    pub style: Stroke,
}

impl StrokeBrush {
    /// Creates a new solid color stroke with a given width.
    pub fn new(color: gaanim_core::peniko::Color, width: f64) -> Self {
        Self {
            brush: Some(Brush::Solid(color)),
            style: Stroke::new(width),
        }
    }

    /// Creates a transparent/no-stroke outline.
    pub fn transparent() -> Self {
        Self {
            brush: None,
            style: Stroke::default(),
        }
    }
}

/// Local opacity of a single Mobject.
#[derive(Component, Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Opacity(pub f32);

impl Default for Opacity {
    fn default() -> Self {
        Self(1.0)
    }
}

/// Propagated global opacity used by the rendering backend.
///
/// Automatically computed by the hierarchy propagation systems in `gaanim_scene`.
/// For instance, a child with `Opacity(0.8)` under a parent with `GlobalOpacity(0.5)`
/// will be computed to have `GlobalOpacity(0.4)`.
#[derive(Component, Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct GlobalOpacity(pub f32);

impl Default for GlobalOpacity {
    fn default() -> Self {
        Self(1.0)
    }
}

/// A 2D vector geometry represented as a shared Bézier path (`Arc<kurbo::BezPath>`).
#[derive(Component, Debug, Clone, Default, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Path2D(pub Arc<BezPath>);

/// A mirror component that caches the original, unmodified Bézier path
/// of a Mobject for time-based trimming / interpolation during writing/drawing.
#[derive(Component, Debug, Clone, Default, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct PathSource(pub Arc<BezPath>);

/// How draw animations (`PathCompletion`) reveal a path with several sub-paths.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum PathRevealOrder {
    /// Every sub-path grows at once, like a pen tracing each glyph contour.
    #[default]
    Parallel,
    /// Sub-paths are traced one after another along the total arc length,
    /// for pieces of a single stroke such as the dashes of a dashed line.
    Sequential,
}

impl PathRevealOrder {
    /// The visible part of `path` at draw progress `alpha` in `[0, 1]`.
    pub fn trim(self, path: &BezPath, alpha: f64) -> BezPath {
        match self {
            Self::Parallel => gaanim_math::get_subpath(path, alpha),
            Self::Sequential => gaanim_math::get_subpath_sequential(path, alpha),
        }
    }
}

/// Normalized amount used by a derived vector fill. Timeline lenses update it
/// deterministically, including during seek.
#[derive(Component, Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct FillLevel(pub f64);

/// Direction in which a vector silhouette is filled.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum FillDirection {
    Up,
    Down,
    Left,
    Right,
}

/// The computed local bounding box of a Mobject.
#[derive(Component, Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Default)]
pub struct LocalBounds(pub Bounds3D);

/// A linear deformation, in the parent's space, applied about the drawable's
/// position on top of its local transform, e.g. squash and stretch. Its
/// members inherit it; the authored `SpatialTransform` is left unchanged.
#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub struct ShapeDeform(pub gaanim_core::kurbo::Affine);

/// Extra opacity factor, between zero and one, that a repeater's `count`
/// gives each of its copies. It multiplies into the propagated opacity on
/// top of the authored `Opacity`, so opacity animations stay independent.
#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub struct Presence(pub f32);

/// Local Y coordinate of a text object's typographic baseline.
///
/// Unlike visual bounds, this metric is stable across ascenders, descenders,
/// subscripts, and glyph changes, allowing independently compiled text runs to
/// share a true baseline.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct TextBaseline(pub f64);

/// The computed world bounding box of a Mobject in world coordinates.
///
/// Propagated automatically down the scene hierarchy.
#[derive(Component, Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Default)]
pub struct WorldBounds(pub Bounds3D);

/// Deterministic rendering order of Mobjects.
///
/// Resolves ordering bugs by coupling the typical `z_index` with a monotonically
/// increasing `creation_order` counter. When sorting entities for extraction and composition,
/// the `creation_order` serves as a clean tie-breaker when `z_index` matches, respecting
/// the exact programmatic construction sequence.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Default)]
pub struct RenderOrder {
    /// Manual depth index (higher renders on top).
    pub z_index: i32,
    /// Monotonically increasing creation counter.
    pub creation_order: u64,
}

/// Marker component indicating that the Mobject is visible and should be extracted for rendering.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Visible;

/// Marker component indicating that the entity represents an organizational group of children.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct GroupMarker;

/// User-facing descriptive tag for Mobject classification and query filtering.
#[derive(Component, Debug, Clone, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ObjectTag(pub String);

/// The target rendering backend layer for a given entity.
///
/// Serves as the foundation for the hybrid 2D/3D pipeline, instructing the compositor
/// which render pass should draw this entity.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Default)]
pub enum RenderLayer {
    /// Vector rendering backend using Vello (Default).
    #[default]
    Vello2D,
    /// Overlay layer drawn in screen space on top of all cameras (HUD/Editor UI).
    Overlay,
}

/// A Bevy ECS component wrapping the zero-dependency `ObjectId`.
///
/// This resolves Bevy integration and Orphan Rule constraints while preserving
/// the isolation of `gaanim_core`.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct MobjectId(pub gaanim_core::ObjectId);

/// Internal transform roles for a Cartesian domain view and its text roots.
/// Labels follow the view's positions while retaining their authored glyph size.
#[doc(hidden)]
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum CoordinateViewRole {
    View,
    Label,
}

/// Internal: offset of a coordinate-view label from the data point it annotates,
/// in its parent's coordinates. The offset stays unzoomed, so zooming the view
/// along the other axis does not push a tick number away from its axis.
#[doc(hidden)]
#[derive(Component, Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct CoordinateLabelOffset(pub gaanim_core::glam::DVec2);

/// Internal: one set of axis ticks, grid lines and numbers built for a view
/// scale. Its opacity follows the scale of its `CoordinateViewRole::View`
/// ancestor, cross-fading between the generations of neighbouring anchors.
#[doc(hidden)]
#[derive(Component, Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct CoordinateTickLevel {
    /// 0 follows the view's x scale, 1 its y scale.
    pub axis: u8,
    pub generation: u32,
    /// `(ln view scale, generation)` for every authored view of this axis,
    /// sorted by scale.
    pub anchors: Vec<(f64, u32)>,
}

/// A metadata component attached to individual glyph and shape entities of text or equations,
/// tracking their character value, sequence index, and source range.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct TextSpan {
    /// The character represented by this entity (e.g. 'E', '=', '+', 'x', '2').
    pub character: char,
    /// The 0-indexed character sequence index within the flat string representation.
    pub char_index: usize,
    /// The source span range in the original source markup text.
    pub source_range: core::range::Range<usize>,
}

/// Billboard component: makes an entity always face the camera (for 3D labels).
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Billboard;

/// Box background marker: drawn before the box content it shares a render
/// order with, so it stays beneath that content but above earlier siblings.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct LayoutBackdrop;

/// How a box arranges its children, for the editor's layout inspector.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayoutInspectionKind {
    Row,
    Column,
    Grid,
    Stack,
}

/// One child's place in a box: its cell in the box's local space (the box's
/// center is the origin) and the margin around it, top/right/bottom/left.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LayoutInspectionCell {
    pub bounds: Bounds3D,
    pub margin: [f64; 4],
}

/// A box's arrangement from one moment on: its padding (top/right/bottom/
/// left), gap and the cells of its children, in its local space.
#[derive(Debug, Clone, PartialEq)]
pub struct LayoutInspectionFrame {
    pub kind: LayoutInspectionKind,
    pub padding: [f64; 4],
    pub gap: gaanim_core::glam::DVec2,
    pub cells: Vec<LayoutInspectionCell>,
}

/// What the editor's layout inspector draws for a box, as the timeline
/// changes it: each arrangement with the time it starts.
#[derive(Component, Debug, Clone, Default, PartialEq)]
pub struct LayoutInspection(pub Vec<(f64, LayoutInspectionFrame)>);

impl LayoutInspection {
    /// Record `frame` from `time` on, dropping arrangements recorded for the
    /// same moment or later (a recompile replays them).
    pub fn record(&mut self, time: f64, frame: LayoutInspectionFrame) {
        self.0.retain(|(start, _)| *start < time - 1.0e-9);
        self.0.push((time, frame));
    }

    /// The arrangement shown at `time`: the last one started by then, or
    /// the first before any starts.
    pub fn at(&self, time: f64) -> Option<&LayoutInspectionFrame> {
        self.0
            .iter()
            .rev()
            .find(|(start, _)| *start <= time + 1.0e-9)
            .or_else(|| self.0.first())
            .map(|(_, frame)| frame)
    }
}

/// A named zone of `scene.layout.zones(...)`, in scene coordinates, with the
/// segment it was created in and the times it is shown.
#[derive(Debug, Clone, PartialEq)]
pub struct LayoutZoneRecord {
    pub name: String,
    pub bounds: Bounds3D,
    pub segment: usize,
    pub start: f64,
    pub end: f64,
}

/// Every zone the scene created, for the editor's layout inspector.
#[derive(Resource, Debug, Clone, Default, PartialEq)]
pub struct LayoutZones(pub Vec<LayoutZoneRecord>);

/// HUD overlay marker: entity is rendered in screen-space overlay layer.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct HudOverlay;

/// Marks 3D content (triangle meshes and line lists) that the renderer
/// projects through the camera instead of drawing as a 2D path.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Mesh3DMarker;

/// Surface parameters of a lit 3D primitive: base color, roughness, metalness
/// and emission, shaded by the renderer.
#[derive(Component, Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Material3D {
    pub color: gaanim_core::peniko::Color,
    pub roughness: f32,
    pub metallic: f32,
    pub emissive: gaanim_core::peniko::Color,
    pub emissive_strength: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum Material3DError {
    #[error("roughness and metallic must be finite values in the 0..=1 range")]
    InvalidSurface,
    #[error("emissive strength must be finite and non-negative")]
    InvalidEmissiveStrength,
}

impl Material3D {
    pub fn new(
        color: gaanim_core::peniko::Color,
        roughness: f32,
        metallic: f32,
        emissive: Option<gaanim_core::peniko::Color>,
        emissive_strength: f32,
    ) -> Result<Self, Material3DError> {
        if !roughness.is_finite()
            || !metallic.is_finite()
            || !(0.0..=1.0).contains(&roughness)
            || !(0.0..=1.0).contains(&metallic)
        {
            return Err(Material3DError::InvalidSurface);
        }
        if !emissive_strength.is_finite() || emissive_strength < 0.0 {
            return Err(Material3DError::InvalidEmissiveStrength);
        }
        Ok(Self {
            color,
            roughness,
            metallic,
            emissive: emissive.unwrap_or(gaanim_core::peniko::Color::TRANSPARENT),
            emissive_strength,
        })
    }

    pub fn matte(color: gaanim_core::peniko::Color) -> Self {
        Self {
            color,
            ..Self::default()
        }
    }

    pub fn metal(color: gaanim_core::peniko::Color) -> Self {
        Self {
            color,
            roughness: 0.22,
            metallic: 1.0,
            ..Self::default()
        }
    }

    pub fn emissive(
        color: gaanim_core::peniko::Color,
        strength: f32,
    ) -> Result<Self, Material3DError> {
        Self::new(color, 0.45, 0.0, Some(color), strength)
    }

    pub fn lerp(self, other: Self, t: f64) -> Self {
        if t <= 0.0 {
            return self;
        }
        if t >= 1.0 {
            return other;
        }
        Self {
            color: gaanim_core::interpolate_color(self.color, other.color, t),
            roughness: self.roughness + (other.roughness - self.roughness) * t as f32,
            metallic: self.metallic + (other.metallic - self.metallic) * t as f32,
            emissive: gaanim_core::interpolate_color(self.emissive, other.emissive, t),
            emissive_strength: self.emissive_strength
                + (other.emissive_strength - self.emissive_strength) * t as f32,
        }
    }
}

impl Default for Material3D {
    fn default() -> Self {
        Self {
            color: gaanim_core::peniko::Color::WHITE,
            roughness: 0.55,
            metallic: 0.0,
            emissive: gaanim_core::peniko::Color::TRANSPARENT,
            emissive_strength: 0.0,
        }
    }
}

/// Scene-level lighting of lit 3D content.
#[derive(Resource, Debug, Clone, Copy, PartialEq)]
pub struct Lighting3D {
    pub enabled: bool,
    pub intensity: f32,
    /// Kept for scripts that set it; the renderer casts no shadows.
    pub shadows: bool,
}

impl Default for Lighting3D {
    fn default() -> Self {
        Self {
            enabled: true,
            intensity: 1.0,
            shadows: true,
        }
    }
}

/// A triangle mesh in local 3D coordinates, projected and shaded by the renderer.
#[derive(Component, Debug, Clone)]
pub struct TriangleMeshData {
    pub vertices: Vec<[f32; 3]>,
    pub indices: Vec<u32>,
    /// Optional explicit vertex normals. Missing/invalid normals are generated.
    pub normals: Option<Vec<[f32; 3]>>,
    /// Optional UV0 coordinates. Missing/invalid UVs fall back to zeroes.
    pub uvs: Option<Vec<[f32; 2]>>,
    pub color: Option<gaanim_core::peniko::Color>,
    pub colors: Option<Vec<[f32; 4]>>,
    /// `None` preserves the historical unlit surface-mesh behavior.
    pub material: Option<Material3D>,
}

/// A 3D line list (point pairs, or a strip), projected by the renderer.
#[derive(Component, Debug, Clone)]
pub struct LineListData {
    pub points: Vec<[f32; 3]>,
    /// Indices as line pairs. If None, points are sequential pairs.
    pub indices: Option<Vec<u32>>,
    /// `true` connects every point as a strip; `false` consumes point pairs.
    pub strip: bool,
    pub color: gaanim_core::peniko::Color,
    /// Optional per-vertex RGBA colors (linear, 0..1). If Some, length must match `points`.
    /// When present the renderer uses vertex colors instead of the uniform `color`.
    pub colors: Option<Vec<[f32; 4]>>,
}

/// Immutable source geometry retained while `PathCompletion` reveals a 3D line.
#[derive(Component, Debug, Clone)]
pub struct LineListSource(pub LineListData);

#[cfg(test)]
mod material_3d_tests {
    use super::*;

    #[test]
    fn material_validates_pbr_ranges() {
        assert!(Material3D::new(gaanim_core::peniko::Color::WHITE, -0.1, 0.0, None, 0.0).is_err());
        assert!(Material3D::new(gaanim_core::peniko::Color::WHITE, 0.5, 1.1, None, 0.0).is_err());
        assert!(Material3D::new(gaanim_core::peniko::Color::WHITE, 0.5, 0.0, None, -1.0).is_err());
        assert!(Material3D::new(gaanim_core::peniko::Color::WHITE, 0.5, 0.0, None, 2.0).is_ok());
    }

    #[test]
    fn material_lerp_reaches_exact_endpoints() {
        let from = Material3D::matte(gaanim_core::peniko::Color::BLACK);
        let to = Material3D::new(
            gaanim_core::peniko::Color::WHITE,
            0.1,
            0.9,
            Some(gaanim_core::peniko::Color::WHITE),
            3.0,
        )
        .unwrap();
        assert_eq!(from.lerp(to, 0.0), from);
        assert_eq!(from.lerp(to, 1.0), to);
    }
}
