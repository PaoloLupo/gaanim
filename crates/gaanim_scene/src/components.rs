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

/// Direction in which a vector silhouette's fill grows: `Up` starts at the
/// bottom, `Right` at the left edge.
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

/// Rank of the named draw layer (`scene.z_layers`) an entity and its
/// descendants draw in: every element of a higher layer draws above every
/// element of a lower one, whatever their `z_index`. An entity without one
/// takes its nearest ancestor's, or draws in the default layer, rank 0.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ZLayer(pub i32);

/// Marks an entity, and its descendants, that hides texts on purpose, like a
/// dialog drawn over a model: `gaanim check` does not warn about texts it
/// covers. It changes nothing in the drawing.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct CoversOnPurpose;

/// Marks an entity that belongs to one segment of the timeline. In a scene
/// with segments, a drawable whose chain of parents carries no marker is
/// persistent (`scene.persist`) and draws above segment content with the
/// same `z_index`.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SegmentContent;

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

/// What a scene script authored a drawable as, for the editor's inspector.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthoredObject {
    /// The kind of drawable in the scripting vocabulary, such as `circle`.
    pub kind: String,
    /// The name the script gave it (`Drawable.named`).
    pub name: Option<String>,
    /// The script line that created it.
    pub location: Option<gaanim_core::console::ScriptLocation>,
    /// The script lines that called the function holding `location`,
    /// innermost first, when a script's own function created it.
    pub callers: Vec<gaanim_core::console::ScriptLocation>,
}

/// The authored drawables of the scene a reload compiled, by the
/// [`MobjectId`] of the entity compiled for each. Every reload replaces it,
/// so its script lines stay current when a reload keeps earlier entities.
///
/// A reload only hands it a way to build the index; it is built the first
/// time the editor asks, so a reload nobody inspects pays nothing for it.
#[derive(Resource, Clone, Default)]
pub struct AuthoredObjects {
    index: Arc<std::sync::OnceLock<AuthoredIndex>>,
    build: Option<Arc<dyn Fn() -> AuthoredIndex + Send + Sync>>,
}

/// What a scene script authored, for the editor.
#[derive(Debug, Clone, Default)]
pub struct AuthoredIndex {
    /// The authored drawables by the id of their compiled entity.
    pub objects: std::collections::HashMap<gaanim_core::ObjectId, AuthoredObject>,
    /// The script lines of each `play`, by the origin key its clips carry
    /// in `Timeline::clip_origins`.
    pub plays: std::collections::HashMap<u64, AuthoredCall>,
}

/// A script call that authored something, such as a `play`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthoredCall {
    /// The line that made the call.
    pub location: gaanim_core::console::ScriptLocation,
    /// The lines that called the function holding `location`, innermost first.
    pub callers: Vec<gaanim_core::console::ScriptLocation>,
}

impl AuthoredObjects {
    /// An index already built.
    pub fn new(index: AuthoredIndex) -> Self {
        Self {
            index: Arc::new(std::sync::OnceLock::from(index)),
            build: None,
        }
    }

    /// An index that `build` makes when it is first read.
    pub fn deferred(build: impl Fn() -> AuthoredIndex + Send + Sync + 'static) -> Self {
        Self {
            index: Arc::default(),
            build: Some(Arc::new(build)),
        }
    }

    fn index(&self) -> &AuthoredIndex {
        self.index
            .get_or_init(|| self.build.as_ref().map(|build| build()).unwrap_or_default())
    }

    /// The drawable compiled as entity id `id`, if the script authored it.
    pub fn get(&self, id: &gaanim_core::ObjectId) -> Option<&AuthoredObject> {
        self.index().objects.get(id)
    }

    pub fn contains(&self, id: &gaanim_core::ObjectId) -> bool {
        self.index().objects.contains_key(id)
    }

    /// The `play` whose clips carry `origin`.
    pub fn play(&self, origin: u64) -> Option<&AuthoredCall> {
        self.index().plays.get(&origin)
    }
}

impl std::fmt::Debug for AuthoredObjects {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AuthoredObjects")
            .field("built", &self.index.get().map(|index| index.objects.len()))
            .finish_non_exhaustive()
    }
}

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
    #[cfg_attr(feature = "serde", serde(with = "range_serde"))]
    pub source_range: core::range::Range<usize>,
}

/// `core::range::Range` has no serde support: it is written as `(start, end)`.
#[cfg(feature = "serde")]
mod range_serde {
    use core::range::Range;

    pub fn serialize<S: serde::Serializer>(
        range: &Range<usize>,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        serde::Serialize::serialize(&(range.start, range.end), serializer)
    }

    pub fn deserialize<'de, D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Range<usize>, D::Error> {
        let (start, end) = <(usize, usize) as serde::Deserialize>::deserialize(deserializer)?;
        Ok(Range { start, end })
    }
}

/// Billboard component: makes an entity always face the camera (for 3D labels).
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Billboard;

/// Internal: where the 3D pass draws a [`Billboard`] it was handed (see
/// [`InWorld3D`]): a plane facing the camera, as large on screen as the 2D
/// renderer would draw the label, kept by
/// [`crate::systems::billboard_system`]. The billboard's own
/// [`gaanim_math::GlobalSpatialTransform`] keeps the label's scale, which
/// its bounds measure.
#[doc(hidden)]
#[derive(Component, Debug, Clone, Copy, PartialEq, Default)]
pub struct BillboardPlane(pub gaanim_math::GlobalSpatialTransform);

/// Internal: keeps a 3D axis number or title a fixed gap from its axis on
/// screen, as 2D axes do, whatever the camera's distance or angle. The
/// billboard is anchored on the axis, at a tick or at the axis's end, and
/// [`crate::systems::billboard_system`] moves its text on screen: a number
/// beside its tick, outside the box, and a title past the axis's end.
/// Vectors and points are in the parent's coordinates.
#[doc(hidden)]
#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub struct AxisLabelGap {
    /// The axis's direction, from its start toward its end.
    pub axis: gaanim_core::glam::DVec3,
    /// Half a tick, across the axis, toward the side a number takes when the
    /// box does not decide it; zero for a title.
    pub tick: gaanim_core::glam::DVec3,
    /// The lowest and highest corners of the box the axes frame.
    pub bounds: [gaanim_core::glam::DVec3; 2],
    /// Space between the tick (or the axis's end) and the text, in scene
    /// units on screen.
    pub gap: f64,
    /// For a title, half the size of the widest number beside the axis's
    /// end, which it clears too; `None` for a number.
    pub title: Option<gaanim_core::glam::DVec2>,
}

/// Box background marker: drawn before the box content it shares a render
/// order with, so it stays beneath that content but above earlier siblings.
/// A box inside another shares that order with it when they start with the
/// same content: the outer background, the shallower `depth`, draws first.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct LayoutBackdrop {
    /// How many boxes hold this background's box.
    pub depth: u32,
}

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

/// Parallax layer root (`SceneModel::layer`): its subtree moves by
/// `1 / depth` of the 2D camera motion; see [`crate::systems::parallax_pin`].
#[derive(Component, Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ParallaxLayer {
    /// Distance from the camera in units of the scene plane's distance.
    pub depth: f64,
}

/// Depth of field of parallax layers: a layer at `depth` blurs by its
/// circle of confusion, `aperture * |1 - focus / depth|` scene units, at
/// most `max_blur`. Depths are those of [`ParallaxLayer`], so `focus = 1`
/// keeps the scene plane sharp; an aperture of 0 turns it off.
#[derive(bevy::prelude::Resource, Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct DepthOfField {
    pub focus: f64,
    pub aperture: f64,
    pub max_blur: f64,
}

impl Default for DepthOfField {
    fn default() -> Self {
        Self {
            focus: 1.0,
            aperture: 0.0,
            max_blur: 0.5,
        }
    }
}

impl DepthOfField {
    /// Gaussian blur, in scene units, of a layer at `depth`.
    pub fn blur_at(&self, depth: f64) -> f64 {
        if !(self.aperture > 0.0 && self.focus > 0.0 && depth > 0.0) {
            return 0.0;
        }
        let confusion = if depth.is_infinite() {
            self.aperture
        } else {
            self.aperture * (1.0 - self.focus / depth).abs()
        };
        confusion.min(self.max_blur).max(0.0)
    }
}

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
    /// Color of the outline drawn around the mesh's silhouette, as an
    /// illustration's contour; transparent draws none.
    pub outline: gaanim_core::peniko::Color,
    /// Width of the outline in scene units, as a stroke's.
    pub outline_width: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum Material3DError {
    #[error("roughness and metallic must be finite values in the 0..=1 range")]
    InvalidSurface,
    #[error("emissive strength must be finite and non-negative")]
    InvalidEmissiveStrength,
    #[error("outline width must be finite and non-negative")]
    InvalidOutlineWidth,
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
            ..Self::default()
        })
    }

    /// This material with an outline of `color`, `width` scene units wide,
    /// around the mesh's silhouette.
    pub fn with_outline(
        self,
        color: gaanim_core::peniko::Color,
        width: f32,
    ) -> Result<Self, Material3DError> {
        if !width.is_finite() || width < 0.0 {
            return Err(Material3DError::InvalidOutlineWidth);
        }
        Ok(Self {
            outline: color,
            outline_width: width,
            ..self
        })
    }

    /// Whether it draws an outline.
    pub fn outlined(&self) -> bool {
        self.outline.components[3] > 0.0 && self.outline_width > 0.0
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
            outline: gaanim_core::interpolate_color(self.outline, other.outline, t),
            outline_width: self.outline_width
                + (other.outline_width - self.outline_width) * t as f32,
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
            outline: gaanim_core::peniko::Color::TRANSPARENT,
            outline_width: 0.0,
        }
    }
}

/// Scene-level lighting of lit 3D content.
#[derive(Resource, Debug, Clone, Copy, PartialEq)]
pub struct Lighting3D {
    pub enabled: bool,
    pub intensity: f32,
    /// Whether opaque meshes cast shadows from the first lights onto lit
    /// meshes.
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

/// Where a [`Light3D`] shines from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LightKind3D {
    /// From the entity's position, in every direction.
    Point,
    /// Along parallel rays, from the direction of the entity's position seen
    /// from the origin, as the sun does.
    Directional,
}

/// A light for lit 3D content, at the position of the entity that carries
/// it. While a scene shows lights they replace the fixed key light of
/// [`Lighting3D`], whose sky and ground light stays; the entity's opacity
/// dims its light.
#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub struct Light3D {
    pub kind: LightKind3D,
    pub color: gaanim_core::peniko::Color,
    pub intensity: f32,
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
#[derive(Component, Debug, Clone, PartialEq)]
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
    /// When these points are the part of the drawable's [`LineListSource`]
    /// that a reveal shows: that part, from and to, as fractions of the
    /// source's segments (a strip's or its pairs'). The renderer then draws
    /// the source over that range, which stays uploaded while it grows, and
    /// recordings keep the source once.
    pub revealed: Option<[f32; 2]>,
}

impl LineListData {
    /// These points, a trim of `source` that shows it from `start` to `end`
    /// (fractions of its segments), marked as such in [`Self::revealed`]
    /// when the renderer takes the source's segments in the order the trim
    /// does: a strip's in turn, or its pairs. A trim that shows all of the
    /// source is left unmarked.
    pub fn revealing(mut self, source: &LineListData, start: f64, end: f64) -> Self {
        let ordered = !source.strip || source.indices.is_none();
        let whole = start <= 0.0 && end >= 1.0;
        self.revealed = (ordered && !whole).then(|| {
            let start = start.clamp(0.0, 1.0);
            [start as f32, end.clamp(start, 1.0) as f32]
        });
        self
    }
}

#[cfg(test)]
mod line_tests {
    use super::*;

    fn line(strip: bool, indices: Option<Vec<u32>>) -> LineListData {
        LineListData {
            points: vec![[0.0; 3], [1.0; 3], [2.0; 3], [3.0; 3]],
            indices,
            strip,
            color: gaanim_core::peniko::Color::WHITE,
            colors: None,
            revealed: None,
        }
    }

    #[test]
    fn trims_are_marked_with_the_part_of_their_source_they_show() {
        let strip = line(true, None);
        assert_eq!(
            strip.clone().revealing(&strip, 0.0, 0.4).revealed,
            Some([0.0, 0.4])
        );
        let pairs = line(false, Some(vec![0, 1, 2, 3]));
        assert_eq!(
            pairs.clone().revealing(&pairs, 0.2, 2.0).revealed,
            Some([0.2, 1.0])
        );
        // A trim that shows all of it, or that orders segments as the
        // renderer does not, stays unmarked.
        assert_eq!(strip.clone().revealing(&strip, 0.0, 1.0).revealed, None);
        let indexed = line(true, Some(vec![0, 2]));
        assert_eq!(indexed.clone().revealing(&indexed, 0.0, 0.5).revealed, None);
    }
}

/// Immutable source geometry retained while `PathCompletion` reveals a 3D line.
#[derive(Component, Debug, Clone)]
pub struct LineListSource(pub LineListData);

/// A 2D drawable placed in the 3D world: its 3D position and rotation tilt
/// it under perspective, and the 3D pass draws it from its outlines, hidden
/// pixel by pixel by meshes in front of it, instead of the 2D renderer
/// drawing it above all 3D content. With a material it is lit, and receives
/// shadows, as a surface.
#[derive(Component, Debug, Clone, Copy, PartialEq, Default)]
pub struct InWorld3D {
    pub material: Option<Material3D>,
}

/// A part of an [`InWorld3D`] drawable: the 2D renderer leaves it to the 3D
/// pass.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DrawnIn3D;

/// Filled outlines lying in the drawable's z = 0 plane: what an
/// [`InWorld3D`] drawable's parts draw, kept on it each frame. The 3D pass
/// draws them from their curves, per pixel, beneath or in front of meshes
/// as the depth buffer decides.
#[derive(Component, Debug, Clone, PartialEq)]
pub struct PlaneFillData {
    pub glyphs: Vec<PlaneGlyph>,
}

/// One filled outline of a [`PlaneFillData`]: a letter of a text, or a
/// shape. Outlines alike (the same letter) share the curves the renderer
/// uploads for them.
#[derive(Debug, Clone, PartialEq)]
pub struct PlaneGlyph {
    /// Closed contours filled with the nonzero rule, in their own units.
    pub outline: Arc<BezPath>,
    /// From the outline's units to the drawable's plane.
    pub placement: Affine,
    /// The color it is filled with; with a `gradient`, that gradient's first
    /// color, which readers that predate gradients draw.
    pub color: gaanim_core::peniko::Color,
    /// A gradient it is filled with instead, its opacity included, and the
    /// map from the outline's units to the gradient's.
    pub gradient: Option<(Arc<gaanim_core::peniko::Gradient>, Affine)>,
    /// A contour that keeps the part of it inside, or outside, as a stroke
    /// aligned inside or outside its shape is drawn.
    pub clip: Option<PlaneClip>,
}

/// The contour a [`PlaneGlyph`] is clipped to.
#[derive(Debug, Clone, PartialEq)]
pub struct PlaneClip {
    /// Closed contours filled with the nonzero rule, in their own units.
    pub outline: Arc<BezPath>,
    /// From the glyph's outline's units to this one's.
    pub offset: gaanim_core::kurbo::Vec2,
    /// Whether the glyph keeps what lies outside the contour rather than
    /// inside it.
    pub outside: bool,
}

#[cfg(test)]
mod material_3d_tests {
    use super::*;

    #[test]
    fn depth_of_field_blurs_by_the_distance_from_the_focus() {
        let off = DepthOfField::default();
        assert_eq!(off.blur_at(3.0), 0.0);
        let dof = DepthOfField {
            focus: 1.0,
            aperture: 0.2,
            max_blur: 0.5,
        };
        assert_eq!(dof.blur_at(1.0), 0.0);
        assert!((dof.blur_at(2.0) - 0.1).abs() < 1e-12);
        assert!((dof.blur_at(0.5) - 0.2).abs() < 1e-12);
        // An infinite backdrop blurs by the aperture; near layers clamp.
        assert!((dof.blur_at(f64::INFINITY) - 0.2).abs() < 1e-12);
        assert_eq!(dof.blur_at(0.01), 0.5);
        assert_eq!(dof.blur_at(0.0), 0.0);
    }

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
