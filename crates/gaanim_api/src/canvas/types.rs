//! Core types: coordinate system, mobject kinds, specs, and queued Anim.

use gaanim_animation::{PropertySources, ScalarSource};
use gaanim_core::ObjectId;
use gaanim_core::glam::{DQuat, DVec3, EulerRot};
use gaanim_core::peniko::{Brush, Color, ImageData, ImageQuality};
use gaanim_layout::{Anchor, Direction, LayoutItemStyle, LayoutNodeKind, LayoutStyle};
use gaanim_math::{Bounds3D, RateFunc};
use gaanim_objects::prelude::{ImageView, SvgPath};
use gaanim_text::prelude::TextAnchor;
use std::path::PathBuf;

use crate::anim::{
    AnimationBuilder, AnimationType, DrawAnimationConfig, PropertyAnimation, PropertyRotation,
    PropertyScale, PropertyTranslation,
};
use crate::canvas::ops::SharedCanvasState;

/// Public operation used by vector boolean drawables.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BooleanOperation {
    Union,
    Intersection,
    Difference,
    Xor,
}

impl BooleanOperation {
    pub(crate) fn native(self) -> gaanim_objects::boolean::BooleanOp {
        match self {
            Self::Union => gaanim_objects::boolean::BooleanOp::Union,
            Self::Intersection => gaanim_objects::boolean::BooleanOp::Intersection,
            Self::Difference => gaanim_objects::boolean::BooleanOp::Difference,
            Self::Xor => gaanim_objects::boolean::BooleanOp::Exclusion,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BooleanRule {
    #[default]
    NonZero,
    EvenOdd,
}

impl BooleanRule {
    pub(crate) fn native(self) -> gaanim_objects::boolean::BooleanFillRule {
        match self {
            Self::NonZero => gaanim_objects::boolean::BooleanFillRule::NonZero,
            Self::EvenOdd => gaanim_objects::boolean::BooleanFillRule::EvenOdd,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FillLevelDirection {
    #[default]
    Up,
    Down,
    Left,
    Right,
}
impl FillLevelDirection {
    pub(crate) fn native(self) -> gaanim_scene::FillDirection {
        match self {
            Self::Up => gaanim_scene::FillDirection::Up,
            Self::Down => gaanim_scene::FillDirection::Down,
            Self::Left => gaanim_scene::FillDirection::Left,
            Self::Right => gaanim_scene::FillDirection::Right,
        }
    }
}

/// Resolution-independent authored frame, measured in logical scene units.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SceneFrame {
    pub width: f64,
    pub height: f64,
}

impl SceneFrame {
    pub const WIDESCREEN: Self = Self::new(16.0, 9.0);
    pub const VERTICAL: Self = Self::new(9.0, 16.0);
    pub const SQUARE: Self = Self::new(10.0, 10.0);

    pub const fn new(width: f64, height: f64) -> Self {
        Self { width, height }
    }

    pub fn validate(self) -> Result<Self, &'static str> {
        if !self.width.is_finite()
            || !self.height.is_finite()
            || self.width <= 0.0
            || self.height <= 0.0
        {
            return Err("frame width and height must be finite positive numbers");
        }
        Ok(self)
    }

    pub fn aspect_ratio(self) -> f64 {
        self.width / self.height
    }

    pub fn bounds(self) -> Bounds3D {
        Bounds3D::new_2d(
            -self.width * 0.5,
            -self.height * 0.5,
            self.width * 0.5,
            self.height * 0.5,
        )
    }

    /// Stable raster used only by the interactive host. Export chooses its own pixels.
    pub fn preview_pixel_size(self) -> (u32, u32) {
        const LONG_EDGE: f64 = 1280.0;
        if self.width >= self.height {
            (
                LONG_EDGE as u32,
                (LONG_EDGE / self.aspect_ratio()).round().max(1.0) as u32,
            )
        } else {
            (
                (LONG_EDGE * self.aspect_ratio()).round().max(1.0) as u32,
                LONG_EDGE as u32,
            )
        }
    }
}

impl Default for SceneFrame {
    fn default() -> Self {
        Self::WIDESCREEN
    }
}

/// Display and styling options for Cartesian axes.
///
/// Compatible with `manim.mobject.graphing.coordinate_systems.Axes`:
/// `x_range`/`y_range` define data intervals, `x_length`/`y_length` control
/// scene size (like Manim), `tips` adds arrowheads, and `axis_config` dicts
/// allow per-axis overrides. When `auto_fit` is true and no explicit lengths
/// are given, axes scale to `safe_frame` (gaanim layout idiom).
#[derive(Debug, Clone)]
pub struct AxesConfig {
    pub grid: bool,
    pub ticks: bool,
    pub numbers: bool,
    pub labels: bool,
    pub x_axis: bool,
    pub y_axis: bool,
    pub x_grid: bool,
    pub y_grid: bool,
    pub x_ticks: bool,
    pub y_ticks: bool,
    pub x_numbers: bool,
    pub y_numbers: bool,
    pub x_label: Option<String>,
    pub y_label: Option<String>,
    pub axis_color: Color,
    pub grid_color: Color,
    pub tick_color: Color,
    pub number_color: Color,
    pub label_color: Color,
    pub number_size: Option<f64>,
    pub label_size: Option<f64>,
    pub axis_width: f64,
    pub grid_width: f64,
    pub tick_width: f64,
    pub tick_length: f64,
    /// If true, scale axes to occupy the safe_frame (default true).
    /// Ignored when `x_length`/`y_length` are set (Manim-like explicit size).
    pub auto_fit: bool,
    /// Manim-compatible explicit lengths (scene units). `None` → auto.
    pub x_length: Option<f64>,
    pub y_length: Option<f64>,
    /// Whether to draw arrow tips at positive ends (Manim `tips=True`).
    pub tips: bool,
}

impl Default for AxesConfig {
    fn default() -> Self {
        Self {
            grid: true,
            ticks: true,
            numbers: true,
            labels: true,
            x_axis: true,
            y_axis: true,
            x_grid: true,
            y_grid: true,
            x_ticks: true,
            y_ticks: true,
            x_numbers: true,
            y_numbers: true,
            x_label: None,
            y_label: None,
            axis_color: Color::from_rgb8(0x20, 0x20, 0x20),
            grid_color: Color::from_rgb8(0xC0, 0xC0, 0xC0),
            tick_color: Color::from_rgb8(0x20, 0x20, 0x20),
            number_color: Color::from_rgb8(0x20, 0x20, 0x20),
            label_color: Color::from_rgb8(0x20, 0x20, 0x20),
            number_size: None,
            label_size: None,
            axis_width: 0.03,
            grid_width: 0.01,
            tick_width: 0.02,
            tick_length: 0.08,
            auto_fit: true,
            x_length: None,
            y_length: None,
            tips: true,
        }
    }
}

/// How a 3D label should face the camera.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LabelMode {
    /// Always faces the camera (billboard). Scales with distance.
    #[default]
    Billboard,
    /// Fixed screen-space HUD overlay (does not scale with distance).
    Hud,
}

/// Display and styling options for 3D Cartesian axes.
///
/// Extends `AxesConfig` with a third dimension, three grid planes,
/// and perspective-aware label modes.
#[derive(Debug, Clone)]
pub struct Axes3DConfig {
    pub grid: bool,
    pub ticks: bool,
    pub numbers: bool,
    pub labels: bool,
    pub x_axis: bool,
    pub y_axis: bool,
    pub z_axis: bool,
    pub xy_grid: bool,
    pub xz_grid: bool,
    pub yz_grid: bool,
    pub x_ticks: bool,
    pub y_ticks: bool,
    pub z_ticks: bool,
    pub x_numbers: bool,
    pub y_numbers: bool,
    pub z_numbers: bool,
    pub x_label: Option<String>,
    pub y_label: Option<String>,
    pub z_label: Option<String>,
    pub label_mode: LabelMode,
    pub axis_color: Color,
    pub grid_color: Color,
    pub tick_color: Color,
    pub number_color: Color,
    pub label_color: Color,
    pub axis_width: f64,
    pub grid_width: f64,
    pub tick_width: f64,
    pub tick_length: f64,
    pub auto_fit: bool,
    pub x_length: Option<f64>,
    pub y_length: Option<f64>,
    pub z_length: Option<f64>,
    pub tips: bool,
}

impl Default for Axes3DConfig {
    fn default() -> Self {
        Self {
            grid: true,
            ticks: true,
            numbers: true,
            labels: true,
            x_axis: true,
            y_axis: true,
            z_axis: true,
            xy_grid: true,
            xz_grid: true,
            yz_grid: true,
            x_ticks: true,
            y_ticks: true,
            z_ticks: true,
            x_numbers: true,
            y_numbers: true,
            z_numbers: true,
            x_label: None,
            y_label: None,
            z_label: None,
            label_mode: LabelMode::Billboard,
            axis_color: Color::from_rgb8(0x20, 0x20, 0x20),
            grid_color: Color::from_rgb8(0xC0, 0xC0, 0xC0),
            tick_color: Color::from_rgb8(0x20, 0x20, 0x20),
            number_color: Color::from_rgb8(0x20, 0x20, 0x20),
            label_color: Color::from_rgb8(0x20, 0x20, 0x20),
            axis_width: 3.0,
            grid_width: 1.0,
            tick_width: 2.0,
            tick_length: 0.2,
            auto_fit: true,
            x_length: None,
            y_length: None,
            z_length: None,
            tips: true,
        }
    }
}

/// Per-side canvas margin (in the same unit as the coordinate system).
///
/// Layout operations like `to_edge` and `to_corner` respect these margins
/// as an automatic inset from the frame bounds, so individual `buff`
/// values stack on top of the margin.
#[derive(Debug, Clone, Copy)]
pub struct Margin {
    pub left: f64,
    pub right: f64,
    pub top: f64,
    pub bottom: f64,
}

impl Margin {
    pub const ZERO: Self = Self {
        left: 0.0,
        right: 0.0,
        top: 0.0,
        bottom: 0.0,
    };

    /// Uniform margin on all four sides.
    pub fn all(v: f64) -> Self {
        Self {
            left: v,
            right: v,
            top: v,
            bottom: v,
        }
    }

    /// Horizontal (left+right) and vertical (top+bottom) margins.
    pub fn hv(h: f64, v: f64) -> Self {
        Self {
            left: h,
            right: h,
            top: v,
            bottom: v,
        }
    }
}

impl Default for Margin {
    fn default() -> Self {
        Self::ZERO
    }
}

/// How an image should use a requested target size.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ImageFit {
    /// Preserve aspect ratio inside the target rectangle.
    #[default]
    Contain,
    /// Preserve aspect ratio while filling the target rectangle, clipping excess pixels.
    Cover,
    /// Fill the target rectangle even when that distorts the source aspect ratio.
    Stretch,
}

/// A rectangle in source image pixel coordinates (top-left origin).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ImageCrop {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

/// Optional destination sizing, crop, and sampling quality for `SceneModel::image_with_options`.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct ImageOptions {
    pub width: Option<f64>,
    pub height: Option<f64>,
    pub fit: ImageFit,
    pub crop: Option<ImageCrop>,
    /// Vello sampling hint. `High` selects bicubic sampling in Vello 0.9.
    pub quality: ImageQuality,
}

/// Playback, sizing, and embedded-audio options for `SceneModel::video_with_options`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VideoOptions {
    pub image: ImageOptions,
    pub offset: f64,
    /// Length of the selected source interval, in source seconds.
    pub duration: Option<f64>,
    pub looping: bool,
    pub speed: f64,
    pub audio: bool,
    pub volume: f64,
}

impl Default for VideoOptions {
    fn default() -> Self {
        Self {
            image: ImageOptions::default(),
            offset: 0.0,
            duration: None,
            looping: false,
            speed: 1.0,
            audio: true,
            volume: 1.0,
        }
    }
}

/// Playback and destination sizing options for a Lottie JSON composition.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LottieOptions {
    pub width: Option<f64>,
    pub height: Option<f64>,
    pub fit: ImageFit,
    pub offset: f64,
    /// Length of the selected source interval, in source seconds.
    pub duration: Option<f64>,
    pub looping: bool,
    pub speed: f64,
}

impl Default for LottieOptions {
    fn default() -> Self {
        Self {
            width: None,
            height: None,
            fit: ImageFit::Contain,
            offset: 0.0,
            duration: None,
            looping: false,
            speed: 1.0,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ImageOptionsError {
    #[error("image width and height must be finite positive values")]
    InvalidTargetSize,
    #[error("crop must be finite, positive in size, and contained within the source image")]
    InvalidCrop,
}

impl ImageOptions {
    /// Resolve options into the exact source-to-destination mapping used by the renderer.
    pub fn resolve(
        self,
        source_width: u32,
        source_height: u32,
    ) -> Result<ImageView, ImageOptionsError> {
        let source_width = f64::from(source_width);
        let source_height = f64::from(source_height);
        let crop = self.crop.unwrap_or(ImageCrop {
            x: 0.0,
            y: 0.0,
            width: source_width,
            height: source_height,
        });
        if !crop.x.is_finite()
            || !crop.y.is_finite()
            || !crop.width.is_finite()
            || !crop.height.is_finite()
            || crop.x < 0.0
            || crop.y < 0.0
            || crop.width <= 0.0
            || crop.height <= 0.0
            || crop.x + crop.width > source_width
            || crop.y + crop.height > source_height
        {
            return Err(ImageOptionsError::InvalidCrop);
        }

        for value in [self.width, self.height].into_iter().flatten() {
            if !value.is_finite() || value <= 0.0 {
                return Err(ImageOptionsError::InvalidTargetSize);
            }
        }

        let (display_width, display_height, scale_x, scale_y) = match (self.width, self.height) {
            (None, None) => (crop.width, crop.height, 1.0, 1.0),
            (Some(width), None) => {
                let scale = width / crop.width;
                (width, crop.height * scale, scale, scale)
            }
            (None, Some(height)) => {
                let scale = height / crop.height;
                (crop.width * scale, height, scale, scale)
            }
            (Some(width), Some(height)) => match self.fit {
                ImageFit::Contain => {
                    let scale = (width / crop.width).min(height / crop.height);
                    (crop.width * scale, crop.height * scale, scale, scale)
                }
                ImageFit::Cover => {
                    let scale = (width / crop.width).max(height / crop.height);
                    (width, height, scale, scale)
                }
                ImageFit::Stretch => (width, height, width / crop.width, height / crop.height),
            },
        };

        Ok(ImageView {
            source_x: crop.x,
            source_y: crop.y,
            source_width: crop.width,
            source_height: crop.height,
            display_width,
            display_height,
            scale_x,
            scale_y,
            quality: self.quality,
        })
    }
}

#[derive(Debug, Clone)]
pub enum SpawnKind {
    FillLevelOutline {
        mask: ObjectId,
    },
    FillLevel {
        mask: ObjectId,
        level: f64,
        direction: FillLevelDirection,
    },
    /// A materialized vector boolean. Sources remain visible and independent.
    Boolean {
        sources: Vec<ObjectId>,
        op: BooleanOperation,
        live: bool,
        tolerance: f64,
        rule: BooleanRule,
    },
    Circle(f64),
    Rect(f64, f64),
    RoundedRect(f64, f64, f64),
    /// Placeholder path regenerated from live object bounds.
    SurroundingRect,
    Square(f64),
    Dot(f64),
    Ellipse(f64, f64),
    Line(f64, f64, f64, f64),
    Arrow(f64, f64, f64, f64),
    DashedLine {
        start: (f64, f64),
        end: (f64, f64),
        dash_length: f64,
        gap_length: f64,
    },
    DoubleArrow {
        start: (f64, f64),
        end: (f64, f64),
        head_length: Option<f64>,
        head_width: Option<f64>,
    },
    /// Closed polygon defined by scene-space vertices.
    Polygon(Vec<(f64, f64)>),
    /// Symmetric star centered at the origin.
    Star {
        points: u32,
        outer_radius: f64,
        inner_radius: f64,
    },
    /// Regular polygon centered at the origin.
    RegularPolygon {
        sides: u32,
        radius: f64,
    },
    Sector {
        center: (f64, f64),
        radius: f64,
        start_angle: f64,
        sweep_angle: f64,
    },
    Annulus {
        outer_radius: f64,
        inner_radius: f64,
    },
    Brace {
        start: (f64, f64),
        end: (f64, f64),
        height: f64,
    },
    Checkmark(f64),
    Cross(f64),
    RightAngle(f64),
    /// Circular arc centered at `(cx, cy)`, in radians.
    Arc {
        center: (f64, f64),
        radius: f64,
        start_angle: f64,
        sweep_angle: f64,
    },
    /// Curved arrow connecting two points with an angular deflection.
    CurvedArrow(f64, f64, f64, f64, f64),
    /// Curved arrow following an explicit circular arc.
    CurvedArrowArc {
        center: (f64, f64),
        radius: f64,
        start_angle: f64,
        sweep_angle: f64,
    },
    /// Technical measurement: extension lines plus a double-headed arrow.
    Dimension {
        start: (f64, f64),
        end: (f64, f64),
        offset: f64,
    },
    /// Open sequence of straight segments. Useful for springs, rails, and paths.
    Polyline(Vec<(f64, f64)>),
    /// Native quadratic (one control) or cubic (two controls) Bézier path.
    Bezier {
        start: (f64, f64),
        controls: Vec<(f64, f64)>,
        end: (f64, f64),
    },
    /// A native path with Typst-style cursor commands.
    Curve(Vec<CurveElement>),
    /// A deterministic callable sampled in a coordinate space.
    ReactivePlot {
        map: gaanim_visualization::CoordinateMap2D,
        function: gaanim_animation::ReactiveFunction,
        domain: (f64, f64),
        /// Optional data-space end value used to reveal the path exactly.
        reveal: Option<gaanim_animation::ScalarSource>,
        sampling: gaanim_visualization::Sampling,
    },
    /// A deterministic 2D parametric callback sampled in a coordinate space.
    ReactiveParametric2D {
        map: gaanim_visualization::CoordinateMap2D,
        function: gaanim_animation::ReactiveFunction,
        domain: (f64, f64),
        sampling: gaanim_visualization::Sampling,
    },
    /// A deterministic 3D parametric callback sampled into line segments.
    ReactiveParametric3D {
        map: gaanim_visualization::CoordinateMap3D,
        function: gaanim_animation::ReactiveFunction,
        domain: (f64, f64),
        samples: usize,
    },
    /// A deterministic height callback sampled into a 3D surface mesh.
    ReactiveSurface3D {
        map: gaanim_visualization::CoordinateMap3D,
        function: gaanim_animation::ReactiveFunction,
        resolution: [usize; 2],
    },
    /// Numeric text evaluated from a deterministic scalar source.
    ReactiveReadout {
        source: gaanim_animation::ScalarSource,
        format: String,
        prefix: String,
        suffix: String,
        invalid: String,
        font_size: Option<f64>,
    },
    /// One table-backed mark regenerated natively when its DataSource changes.
    DataMark {
        map: gaanim_visualization::CoordinateMap2D,
        source: gaanim_visualization::DataSource,
        kind: gaanim_visualization::DataMarkKind,
    },
    Axes {
        x_range: (f64, f64, f64),
        y_range: (f64, f64, f64),
        config: AxesConfig,
    },
    Axes3D {
        x_range: (f64, f64, f64),
        y_range: (f64, f64, f64),
        z_range: (f64, f64, f64),
        config: Axes3DConfig,
    },
    /// Triangulated 3D surface mesh with explicit vertices/indices in world space.
    SurfaceMesh {
        vertices: Vec<[f32; 3]>,
        indices: Vec<u32>,
        /// Optional base color for the material. If None, uses theme.
        color: Option<Color>,
        /// Optional per-vertex colors.
        colors: Option<Vec<Color>>,
    },
    /// Friendly native PBR primitive with complete geometry attributes.
    Primitive3D(gaanim_scene::TriangleMeshData),
    /// 3D polyline (e.g., curve) defined by world-space points.
    /// If `colors` is Some and length matches `points`, per-vertex colors are used (colormap).
    Polyline3D {
        points: Vec<[f32; 3]>,
        colors: Option<Vec<Color>>,
    },
    /// Independent 3D line segments stored in one retained mesh.
    LineSegments3D {
        points: Vec<[f32; 3]>,
        colors: Option<Vec<Color>>,
    },
    /// Stable manual-animation wrapper for one native glTF node.
    GltfNode {
        node_index: usize,
        path: String,
        bounds: Bounds3D,
    },
    /// Root of a native Bevy glTF scene instance.
    GltfModel {
        path: PathBuf,
        scene_index: usize,
        bounds: Bounds3D,
        nodes: Vec<(usize, Option<usize>, String, ObjectId)>,
        animation_names: Vec<String>,
    },
    /// Unified structured text, including paragraphs and inline/display math.
    Text(gaanim_text::prelude::TextSpec),
    /// Full Typst document markup, compiled as vector text and shapes.
    Typst {
        source: String,
        page_width: Option<String>,
    },
    /// Decoded RGBA texture plus its source and destination mapping.
    Image {
        image: ImageData,
        view: ImageView,
    },
    /// Timeline-sampled MP4 frame rendered through the native raster path.
    Video {
        poster: ImageData,
        view: ImageView,
        playback: gaanim_media::VideoPlayback,
    },
    /// Timeline-sampled vector composition rendered through Velato/Vello.
    Lottie {
        playback: gaanim_renderer::lottie::LottiePlayback,
    },
    /// One resolved vector path imported from an SVG document.
    SvgPath(Box<SvgPath>),
    Group(Vec<ObjectId>),
    /// Group that preserves children's local transforms (used for coordinate-space view).
    GroupNoCenter(Vec<ObjectId>),
    /// Invisible value tracker entity (FloatSignal). No visual output.
    ValueTracker(f64),
    /// Placeholder line entity whose Path2D will be overwritten by TracedPath.
    TracedPathLine,
    /// Placeholder line entity whose Path2D will be overwritten by TrackingLine.
    TrackingLine,
    /// Placeholder 3D line (LineList) that will be fed by TracedPath3D.
    TracedPath3DLine,
}

/// A control point for a curve segment.
#[derive(Debug, Clone, Copy)]
pub enum CurveControl {
    None,
    Auto,
    Point((f64, f64)),
}

/// One cursor command in a native composed curve.
#[derive(Debug, Clone)]
pub enum CurveElement {
    Move {
        to: (f64, f64),
        relative: bool,
    },
    Line {
        to: (f64, f64),
        relative: bool,
    },
    Quad {
        control: CurveControl,
        to: (f64, f64),
        relative: bool,
    },
    Cubic {
        control_start: CurveControl,
        control_end: CurveControl,
        to: (f64, f64),
        relative: bool,
    },
    Close {
        smooth: bool,
    },
}

#[derive(Debug, Clone)]
pub enum LayoutOp {
    SetTranslation(DVec3),
    ShiftBy(DVec3),
    SetScale(f64),
    SetScale3D(DVec3),
    ScaleBy(DVec3),
    SetRotation(f64),
    SetRotation3D(DVec3),
    RotateBy(DQuat),
    /// Scene-space point around which rotation and scaling are performed.
    SetPivot(DVec3),
    MoveAnchorTo {
        target: DVec3,
        anchor: Anchor,
    },
    MoveToAnchorPoint {
        point: super::AnchorPoint,
    },
    MoveTextAnchorTo {
        target: DVec3,
        anchor: TextAnchor,
        center_multiline: bool,
    },
    NextTo {
        reference: ObjectId,
        direction: Direction,
        spacing: f64,
        aligned_edge: Anchor,
    },
    AlignTo {
        reference: ObjectId,
        target_anchor: Anchor,
        reference_anchor: Anchor,
    },
    ToEdge {
        direction: Direction,
        buff: f64,
    },
    ToCorner {
        corner: Anchor,
        buff: f64,
    },
}

/// Containing block used by a root layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LayoutWithin {
    #[default]
    Intrinsic,
    Safe,
    Frame,
}

/// Complete rule set for one persistent Layout v2 container.
#[derive(Debug, Clone, PartialEq)]
pub struct LayoutSpec {
    pub kind: LayoutNodeKind,
    pub style: LayoutStyle,
    pub within: LayoutWithin,
}

impl Default for LayoutSpec {
    fn default() -> Self {
        Self {
            kind: LayoutNodeKind::Column { wrap: false },
            style: LayoutStyle::default(),
            within: LayoutWithin::Intrinsic,
        }
    }
}

/// Per-child rules captured in a layout snapshot.
#[derive(Debug, Clone, PartialEq)]
pub struct LayoutMemberSpec {
    pub id: ObjectId,
    pub style: LayoutItemStyle,
}

/// Immutable, versioned authoring snapshot materialized by the timeline.
#[derive(Debug, Clone, PartialEq)]
pub struct LayoutTreeSnapshot {
    pub version: u64,
    pub container: ObjectId,
    pub members: Vec<LayoutMemberSpec>,
    pub spec: LayoutSpec,
}

#[derive(Debug, Clone)]
pub(crate) struct ReactiveReadoutLayoutSpec {
    pub label: Option<ObjectId>,
    pub equals: Option<ObjectId>,
    pub number: ObjectId,
    pub unit: Option<ObjectId>,
    pub spacing: f64,
}

#[derive(Debug, Clone)]
pub struct ObjectSpec {
    pub id: ObjectId,
    pub kind: SpawnKind,
    pub fill: Option<Brush>,
    pub fill_overridden: bool,
    pub stroke: Option<(Brush, f64)>,
    /// Full kurbo stroke geometry when cap/join/dash options are configured.
    pub stroke_style: Option<gaanim_core::kurbo::Stroke>,
    pub stroke_overridden: bool,
    /// This group is the public root of an imported SVG hierarchy.
    pub(crate) svg_root: bool,
    pub glow: Option<gaanim_renderer::effects::Glow>,
    pub blur: Option<gaanim_renderer::effects::GaussianBlur>,
    pub shadow: Option<gaanim_renderer::effects::DropShadow>,
    pub opacity: f32,
    pub opacity_overridden: bool,
    /// Ordered theme classes. Later classes have higher cascade priority.
    pub style_classes: Vec<String>,
    /// Internal semantic part selector such as `axes/grid` or `table/header`.
    pub theme_selector: Option<String>,
    /// If true, keep this reactive visual hidden until an animation targets it.
    pub defer_visibility_until_play: bool,
    pub z_index: i32,
    /// If true, this object is a HUD overlay (screen-space, fixed).
    pub hud: bool,
    /// If true, this object was attached to a coordinate space as data.
    /// Its parent's Create/Write animations should not include it as a leaf
    /// (otherwise a plane.create() would also draw every plot).
    pub exclude_from_parent_draw: bool,
    /// If true, this object should billboard (face camera) in 3D.
    pub billboard: bool,
    /// Fill overrides applied to matching glyph fragments after textual objects
    /// have been compiled into their vector hierarchy.
    pub fragment_fills: Vec<(String, Color)>,
    /// Named fragment queries attached by the high-level equation API.
    pub fragment_tags: Vec<(String, String, Option<usize>)>,
    /// Layout v2 container that owns this drawable's translation.
    pub layout_owner: Option<ObjectId>,
    /// Whether the author has queued a manual translation animation.
    pub manual_position_animation: bool,
    /// Deferred material state after queued `material_to` calls.
    pub material_animation_cursor: Option<gaanim_scene::Material3D>,
    pub layout_ops: Vec<LayoutOp>,
    pub(crate) reactive_readout_layout: Option<ReactiveReadoutLayoutSpec>,
    pub fill_level_cursor: Option<f64>,
}

impl ObjectSpec {
    pub(crate) fn new(id: ObjectId, kind: SpawnKind) -> Self {
        Self {
            id,
            kind,
            fill: None,
            fill_overridden: false,
            stroke: None,
            stroke_style: None,
            stroke_overridden: false,
            svg_root: false,
            glow: None,
            blur: None,
            shadow: None,
            opacity: 1.0,
            opacity_overridden: false,
            style_classes: Vec::new(),
            theme_selector: None,
            defer_visibility_until_play: false,
            z_index: 0,
            billboard: false,
            hud: false,
            exclude_from_parent_draw: false,
            fragment_fills: Vec::new(),
            fragment_tags: Vec::new(),
            layout_owner: None,
            manual_position_animation: false,
            material_animation_cursor: None,
            layout_ops: Vec::new(),
            reactive_readout_layout: None,
            fill_level_cursor: None,
        }
    }
}

/// A pure, scene-bound animation description.
///
/// Constructing and configuring an `Anim` never mutates the timeline. The
/// owning [`SceneModel`](super::SceneModel) schedules it atomically from
/// `play`, after validating ownership, conflicts, and single-use state.
#[derive(Debug, Clone)]
pub struct Anim {
    pub inner: AnimationBuilder,
    pub(crate) owner: Option<SharedCanvasState>,
    consumed: std::sync::Arc<std::sync::atomic::AtomicBool>,
    duration_explicit: bool,
    rate_explicit: bool,
    property_spec: Option<std::sync::Arc<std::sync::Mutex<ObjectSpec>>>,
    camera_capture_before_play: Option<u64>,
}

impl Anim {
    pub(crate) fn new(target: ObjectId, anim_type: AnimationType) -> Self {
        Self {
            inner: AnimationBuilder {
                target,
                rate_func: anim_type.default_rate_func(),
                anim_type,
                duration: 1.0,
                delay: 0.0,
            },
            owner: None,
            consumed: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
            duration_explicit: false,
            rate_explicit: false,
            property_spec: None,
            camera_capture_before_play: None,
        }
    }

    pub(crate) fn properties(
        target: ObjectId,
        state: SharedCanvasState,
        _segment_idx: usize,
        spec: std::sync::Arc<std::sync::Mutex<ObjectSpec>>,
    ) -> Self {
        let mut anim = Self::new(
            target,
            AnimationType::Properties(PropertyAnimation::default()),
        );
        anim.owner = Some(state);
        anim.property_spec = Some(spec);
        anim
    }

    pub(crate) fn text_selection_properties(
        target: ObjectId,
        fragment: String,
        occurrence: Option<usize>,
        state: SharedCanvasState,
        _segment_idx: usize,
    ) -> Self {
        let mut anim = Self::new(
            target,
            AnimationType::TextSelectionProperties {
                fragment,
                occurrence,
                properties: PropertyAnimation::default(),
            },
        );
        anim.owner = Some(state);
        anim
    }

    pub(crate) fn queued(
        target: ObjectId,
        anim_type: AnimationType,
        state: SharedCanvasState,
        _segment_idx: usize,
    ) -> Self {
        let mut anim = Self::new(target, anim_type);
        anim.owner = Some(state);
        anim
    }

    pub(crate) fn capture_camera_before_play(mut self, id: u64) -> Self {
        self.camera_capture_before_play = Some(id);
        self
    }

    pub(crate) fn camera_capture_before_play(&self) -> Option<u64> {
        self.camera_capture_before_play
    }

    /// Replace an empty animation proxy with a pure property callback.
    pub fn custom(mut self, animation: gaanim_animation::CustomAnimation) -> Result<Self, String> {
        if !matches!(&self.inner.anim_type, AnimationType::Properties(properties) if properties.is_empty())
        {
            return Err("custom() requires an empty Drawable.animate proxy; combine separate animations with parallel()".into());
        }
        if animation.channels().iter().any(|channel| {
            matches!(
                channel,
                gaanim_animation::CustomChannel::Position
                    | gaanim_animation::CustomChannel::Rotation
                    | gaanim_animation::CustomChannel::Scale
            )
        }) && !self.property_position_is_free()
        {
            return Err("layout or live derived geometry owns this drawable's transform".into());
        }
        if self.property_target_is_primitive_3d()
            && animation
                .channels()
                .iter()
                .any(|channel| channel.is_paint())
        {
            return Err(
                "custom paint channels require a vector Drawable; use material() for Primitive3D"
                    .into(),
            );
        }
        if let Some(drawable) = self.property_drawable() {
            for channel in animation.channels() {
                let property = match channel {
                    gaanim_animation::CustomChannel::Position => {
                        Some(gaanim_animation::PropertyChannel::Translation)
                    }
                    gaanim_animation::CustomChannel::Rotation => {
                        Some(gaanim_animation::PropertyChannel::Rotation)
                    }
                    gaanim_animation::CustomChannel::Scale => {
                        Some(gaanim_animation::PropertyChannel::Scale)
                    }
                    gaanim_animation::CustomChannel::Opacity => {
                        Some(gaanim_animation::PropertyChannel::Opacity)
                    }
                    _ => None,
                };
                if property.is_some_and(|property| drawable.property_is_bound(property)) {
                    return Err(format!(
                        "{} is reactively bound; assign a fixed value before custom animation",
                        channel.name()
                    ));
                }
            }
        }
        self.inner.anim_type = AnimationType::CustomProperties(animation);
        Ok(self)
    }

    pub(crate) fn update_properties(mut self, update: impl FnOnce(&mut PropertyAnimation)) -> Self {
        let properties = match &mut self.inner.anim_type {
            AnimationType::Properties(properties)
            | AnimationType::TextSelectionProperties { properties, .. } => properties,
            _ => panic!("property modifiers require a compound property animation"),
        };
        update(properties);
        let translation = properties.translation.is_some();
        let rotation = properties.rotation.is_some();
        let scale = properties.scale.is_some();
        let opacity = properties.opacity.is_some();
        properties
            .source_targets
            .retain(|target| match target.sources.channel() {
                gaanim_animation::PropertyChannel::Translation => !translation,
                gaanim_animation::PropertyChannel::Rotation => !rotation,
                gaanim_animation::PropertyChannel::Scale => !scale,
                gaanim_animation::PropertyChannel::Opacity => !opacity,
            });
        self
    }

    fn effect(mut self, anim_type: AnimationType) -> Self {
        assert!(
            self.inner.anim_type.is_empty_properties(),
            "temporal effects cannot be combined with property targets in one Anim"
        );
        self.inner.rate_func = anim_type.default_rate_func();
        self.inner.anim_type = anim_type;
        self
    }

    fn selection_effect(mut self, effect: crate::anim::TextSelectionEffect) -> Self {
        let AnimationType::TextSelectionProperties {
            fragment,
            occurrence,
            properties,
        } = &self.inner.anim_type
        else {
            panic!("selection effects require a TextSelection animation proxy");
        };
        assert!(
            properties.is_empty(),
            "selection effects cannot be combined with property targets"
        );
        self.inner.anim_type = AnimationType::TextSelection {
            fragment: fragment.clone(),
            occurrence: *occurrence,
            effect,
        };
        self
    }

    pub(crate) fn belongs_to(&self, state: &SharedCanvasState) -> bool {
        self.owner
            .as_ref()
            .is_some_and(|owner| std::sync::Arc::ptr_eq(owner, state))
    }

    pub(crate) fn is_consumed(&self) -> bool {
        self.consumed.load(std::sync::atomic::Ordering::Acquire)
    }

    pub(crate) fn same_token(&self, other: &Self) -> bool {
        std::sync::Arc::ptr_eq(&self.consumed, &other.consumed)
    }

    pub(crate) fn mark_consumed(&self) {
        self.consumed
            .store(true, std::sync::atomic::Ordering::Release);
    }

    pub(crate) fn apply_play_defaults(
        &mut self,
        duration: Option<f64>,
        rate_func: Option<RateFunc>,
    ) {
        if !self.duration_explicit
            && let Some(duration) = duration
        {
            self.inner.duration = duration.max(0.0);
        }
        if !self.rate_explicit
            && let Some(rate_func) = rate_func
        {
            self.inner.rate_func = rate_func;
        }
    }

    pub(crate) fn commit_authoring_target(&self) {
        match &self.inner.anim_type {
            AnimationType::CustomProperties(animation) => {
                // Evaluate before locking authoring state: Python callbacks are
                // guarded against mutation and must never run beneath this lock.
                if let (Some(spec), Ok(values)) = (
                    &self.property_spec,
                    animation.evaluate(self.inner.rate_func.evaluate(1.0)),
                ) {
                    let mut spec = spec.lock().expect("object spec poisoned");
                    if let Some(paint) = values.fill {
                        spec.fill = Some(paint);
                    }
                    if let Some(paint) = values.stroke {
                        let width = values
                            .stroke_width
                            .or_else(|| spec.stroke.as_ref().map(|(_, width)| *width))
                            .unwrap_or(1.0);
                        spec.stroke = Some((paint, width));
                    } else if let Some(width) = values.stroke_width {
                        if let Some((_, current_width)) = &mut spec.stroke {
                            *current_width = width;
                        }
                    }
                    if let Some(width) = values.stroke_width {
                        if let Some(style) = &mut spec.stroke_style {
                            style.width = width;
                        }
                    }
                }
            }
            AnimationType::Properties(properties) => {
                if let Some(spec) = &self.property_spec {
                    let mut spec = spec.lock().expect("object spec poisoned");
                    if let Some((_, material)) = properties.material {
                        spec.material_animation_cursor = Some(material);
                    }
                    if let Some((_, level)) = properties.fill_level {
                        spec.fill_level_cursor = Some(level);
                    }
                    if let Some(color) = properties.visible_color {
                        if spec.fill.is_some() {
                            spec.fill = Some(Brush::Solid(color));
                        }
                        if let Some((paint, _)) = &mut spec.stroke {
                            *paint = Brush::Solid(color);
                        }
                    }
                    if let Some(paint) = &properties.fill {
                        spec.fill = Some(paint.clone());
                    }
                    if let Some(paint) = &properties.stroke_color {
                        let width = properties
                            .stroke_width
                            .or_else(|| spec.stroke.as_ref().map(|(_, width)| *width))
                            .unwrap_or(1.0);
                        spec.stroke = Some((paint.clone(), width));
                    }
                }
            }
            AnimationType::Material3DTo { to, .. } => {
                if let Some(spec) = &self.property_spec {
                    spec.lock()
                        .expect("object spec poisoned")
                        .material_animation_cursor = Some(*to);
                }
            }
            AnimationType::FillLevelTo { to, .. } => {
                if let Some(spec) = &self.property_spec {
                    spec.lock().expect("object spec poisoned").fill_level_cursor = Some(*to);
                }
            }
            AnimationType::SignalFloat { to } => {
                if let Some(owner) = &self.owner {
                    let mirror = owner
                        .lock()
                        .expect("canvas state poisoned")
                        .parameter_values
                        .get(&self.inner.target)
                        .cloned();
                    if let Some(mirror) = mirror {
                        *mirror.lock().expect("parameter poisoned") = *to;
                    }
                }
                if let Some(spec) = &self.property_spec {
                    let mut spec = spec.lock().expect("object spec poisoned");
                    if let SpawnKind::ValueTracker(value) = &mut spec.kind {
                        *value = *to;
                    }
                }
            }
            _ => {}
        }
    }

    fn configured_pivot(&self) -> Option<DVec3> {
        self.property_spec.as_ref().and_then(|spec| {
            let spec = spec.lock().ok()?;
            spec.layout_ops.iter().rev().find_map(|op| match op {
                LayoutOp::SetPivot(pivot) => Some(*pivot),
                _ => None,
            })
        })
    }

    fn assert_free_position(&self) {
        let unavailable = self
            .property_spec
            .as_ref()
            .and_then(|spec| {
                spec.lock().ok().map(|spec| {
                    spec.layout_owner.is_some()
                        || matches!(spec.kind, SpawnKind::Boolean { live: true, .. })
                })
            })
            .unwrap_or(false);
        assert!(
            !unavailable,
            "layout or live derived geometry owns this drawable's transform"
        );
    }

    #[doc(hidden)]
    pub fn property_position_is_free(&self) -> bool {
        self.property_spec
            .as_ref()
            .and_then(|spec| {
                spec.lock().ok().map(|spec| {
                    spec.layout_owner.is_none()
                        && !matches!(spec.kind, SpawnKind::Boolean { live: true, .. })
                })
            })
            .unwrap_or(true)
    }

    #[doc(hidden)]
    pub fn property_target_is_primitive_3d(&self) -> bool {
        self.property_spec
            .as_ref()
            .and_then(|spec| {
                spec.lock()
                    .ok()
                    .map(|spec| matches!(spec.kind, SpawnKind::Primitive3D(_)))
            })
            .unwrap_or(false)
    }

    #[doc(hidden)]
    pub fn property_target_is_text_selection(&self) -> bool {
        matches!(
            self.inner.anim_type,
            AnimationType::TextSelectionProperties { .. }
        )
    }

    fn material_target(&self) -> Option<gaanim_scene::Material3D> {
        let spec = self.property_spec.as_ref()?;
        let spec = spec.lock().ok()?;
        let SpawnKind::Primitive3D(mesh) = &spec.kind else {
            return None;
        };
        Some(
            spec.material_animation_cursor
                .or(mesh.material)
                .unwrap_or_default(),
        )
    }

    fn set_material_target(
        mut self,
        from: gaanim_scene::Material3D,
        to: gaanim_scene::Material3D,
    ) -> Self {
        self = self.update_properties(|properties| {
            let baseline = properties
                .material
                .map(|(baseline, _)| baseline)
                .unwrap_or(from);
            properties.material = Some((baseline, to));
            properties.visible_color = None;
            properties.fill = None;
        });
        self
    }

    /// Animate all PBR channels of a native 3D primitive.
    pub fn material(self, material: gaanim_scene::Material3D) -> Self {
        let from = self
            .material_target()
            .expect("material() requires a native Primitive3D animation");
        self.set_material_target(from, material)
    }

    /// Animate the visible color. For Primitive3D this changes its PBR base color.
    pub fn color(self, color: Color) -> Self {
        if let Some(from) = self.material_target() {
            let mut to = from;
            to.color = color;
            return self.set_material_target(from, to);
        }
        self.update_properties(|properties| {
            properties.visible_color = Some(color);
            properties.fill = None;
            properties.stroke_color = None;
        })
    }

    /// Animate vector paint, or a solid PBR base color for a Primitive3D.
    pub fn fill(self, paint: impl Into<Brush>) -> Self {
        self.try_fill_paint(paint.into())
            .expect("incompatible fill paint")
    }

    fn fill_solid(self, color: Color) -> Self {
        if let Some(from) = self.material_target() {
            let mut to = from;
            to.color = color;
            return self.set_material_target(from, to);
        }
        self.update_properties(|properties| properties.fill = Some(Brush::Solid(color)))
    }

    /// Animate a solid or compatible gradient fill.
    pub fn try_fill_paint(self, paint: Brush) -> Result<Self, &'static str> {
        self.validate_paint(&paint, false)?;
        if let Brush::Solid(color) = paint {
            return Ok(self.fill_solid(color));
        }
        if self.property_target_is_primitive_3d() || self.property_target_is_text_selection() {
            return Err("gradient fill animations require a vector Drawable");
        }
        Ok(self.update_properties(|properties| properties.fill = Some(paint)))
    }

    pub fn fill_paint(self, paint: Brush) -> Self {
        self.try_fill_paint(paint)
            .expect("incompatible fill paints")
    }

    fn validate_paint(&self, paint: &Brush, _stroke: bool) -> Result<(), &'static str> {
        gaanim_animation::paint::validate_paint_transition(paint, paint)
    }

    pub(crate) fn validate_paint_targets(
        &self,
        paints: &mut std::collections::HashMap<(ObjectId, bool), Brush>,
    ) -> Result<(), &'static str> {
        let (fill, stroke) = match &self.inner.anim_type {
            AnimationType::Properties(properties)
            | AnimationType::TextSelectionProperties { properties, .. } => (
                properties
                    .fill
                    .clone()
                    .or_else(|| properties.visible_color.map(Brush::Solid)),
                properties
                    .stroke_color
                    .clone()
                    .or_else(|| properties.visible_color.map(Brush::Solid)),
            ),
            AnimationType::FillPaintTo { to } => (Some(to.clone()), None),
            AnimationType::StrokePaintTo { to } => (None, Some(to.clone())),
            AnimationType::FillColorTo { to } => (Some(Brush::Solid(*to)), None),
            AnimationType::StrokeColorTo { to } => (None, Some(Brush::Solid(*to))),
            AnimationType::CustomProperties(callback) => {
                let values = callback
                    .evaluate(self.inner.rate_func.evaluate(1.0))
                    .map_err(|_| "custom animation endpoint failed validation")?;
                // Custom callbacks author complete paints directly and may change
                // gradient kinds without interpolation. Seed subsequent native targets.
                if let Some(fill) = values.fill {
                    paints.insert((self.inner.target, false), fill);
                }
                if let Some(stroke) = values.stroke {
                    paints.insert((self.inner.target, true), stroke);
                }
                return Ok(());
            }
            _ => return Ok(()),
        };
        for (stroke, to) in [(false, fill), (true, stroke)] {
            let Some(to) = to else {
                continue;
            };
            let key = (self.inner.target, stroke);
            let initial = self.property_spec.as_ref().and_then(|spec| {
                let spec = spec.lock().expect("object spec poisoned");
                if stroke {
                    spec.stroke.as_ref().map(|(paint, _)| paint.clone())
                } else {
                    spec.fill.clone()
                }
            });
            if let Some(from) = paints.get(&key).or(initial.as_ref()) {
                gaanim_animation::paint::validate_paint_transition(from, &to)?;
            }
            paints.insert(key, to);
        }
        Ok(())
    }

    /// Animate a solid or compatible gradient stroke and its width.
    pub fn try_stroke_paint(self, paint: Brush, width: f64) -> Result<Self, &'static str> {
        if self.property_target_is_primitive_3d() || self.property_target_is_text_selection() {
            return Err("stroke animations require a vector Drawable");
        }
        if !width.is_finite() || width < 0.0 {
            return Err("stroke width must be finite and nonnegative");
        }
        self.validate_paint(&paint, true)?;
        Ok(self.update_properties(|properties| {
            properties.stroke_color = Some(paint);
            properties.stroke_width = Some(width);
        }))
    }

    pub fn stroke_paint(self, paint: Brush, width: f64) -> Self {
        self.try_stroke_paint(paint, width)
            .expect("incompatible stroke paints")
    }

    pub fn stroke(self, paint: impl Into<Brush>, width: f64) -> Self {
        self.try_stroke_paint(paint.into(), width.max(0.0))
            .expect("incompatible stroke paint")
    }

    pub fn stroke_color(self, color: Color) -> Self {
        assert!(
            !self.property_target_is_primitive_3d(),
            "stroke_color() is only available for vector drawables"
        );
        self.update_properties(|properties| properties.stroke_color = Some(Brush::Solid(color)))
    }

    pub fn opacity(self, opacity: impl Into<ScalarSource>) -> Self {
        let opacity = opacity.into();
        let Some(opacity) = opacity.constant_value() else {
            return self
                .property_source(PropertySources::Opacity(opacity))
                .expect("invalid reactive opacity source");
        };
        let opacity = opacity as f32;
        self.update_properties(|properties| properties.opacity = Some(opacity.clamp(0.0, 1.0)))
    }

    pub fn try_fill_level(self, level: f64) -> Result<Self, &'static str> {
        if !level.is_finite() || !(0.0..=1.0).contains(&level) {
            return Err("fill level must be finite and between zero and one");
        }
        let spec = self
            .property_spec
            .as_ref()
            .ok_or("fill_level() requires Drawable.animate()")?;
        let spec = spec.lock().expect("object spec poisoned");
        let from = spec
            .fill_level_cursor
            .ok_or("fill_level() requires a Scene.fill_level drawable")?;
        drop(spec);
        Ok(self.update_properties(|properties| properties.fill_level = Some((from, level))))
    }

    pub fn fill_level(self, level: f64) -> Self {
        self.try_fill_level(level)
            .expect("invalid fill level animation")
    }

    pub fn shift_by(self, dx: f64, dy: f64) -> Self {
        self.assert_free_position();
        self.update_properties(|properties| {
            properties.translation = Some(PropertyTranslation::By(DVec3::new(dx, dy, 0.0)))
        })
    }

    pub fn move_to(self, x: impl Into<ScalarSource>, y: impl Into<ScalarSource>) -> Self {
        self.move_to_anchor(x, y, Anchor::Center)
    }

    /// Capture a drawable's bounds center when the animation is scheduled.
    pub fn move_to_drawable(self, target: &super::DrawableHandle) -> Result<Self, &'static str> {
        if !self.belongs_to(&target.state) {
            return Err("move_to target must belong to the same Scene");
        }
        self.move_to_anchor_point(target.anchor_point(Anchor::Center, DVec3::ZERO))
    }

    /// Capture a bounds anchor when the animation is scheduled.
    pub fn move_to_anchor_point(self, point: super::AnchorPoint) -> Result<Self, &'static str> {
        self.assert_free_position();
        if !self.owner.as_ref().is_some_and(|owner| {
            let owner = owner.lock().expect("canvas state poisoned");
            owner.scene_id == point.scene_id && owner.object_specs.contains_key(&point.object)
        }) {
            return Err("move_to anchor must belong to the same Scene");
        }
        Ok(self.update_properties(|properties| {
            properties.translation = Some(PropertyTranslation::ToAnchorPoint(point));
        }))
    }

    /// Animate so the selected local bounds anchor reaches `(x, y)`.
    pub fn move_to_anchor(
        self,
        x: impl Into<ScalarSource>,
        y: impl Into<ScalarSource>,
        anchor: Anchor,
    ) -> Self {
        let x = x.into();
        let y = y.into();
        let (Some(x), Some(y)) = (x.constant_value(), y.constant_value()) else {
            let offset = anchor.to_offset();
            return self
                .property_source(PropertySources::Translation {
                    values: [x, y, 0.0.into()],
                    anchor: Some(DVec3::new(offset.x, offset.y, 0.0)),
                })
                .expect("invalid reactive position source");
        };
        self.assert_free_position();
        self.update_properties(|properties| {
            properties.translation = Some(PropertyTranslation::ToAnchor {
                to: DVec3::new(x, y, 0.0),
                anchor,
            })
        })
    }

    pub fn shift_by_3d(self, dx: f64, dy: f64, dz: f64) -> Self {
        self.assert_free_position();
        self.update_properties(|properties| {
            properties.translation = Some(PropertyTranslation::By(DVec3::new(dx, dy, dz)))
        })
    }

    pub fn move_to_3d(
        self,
        x: impl Into<ScalarSource>,
        y: impl Into<ScalarSource>,
        z: impl Into<ScalarSource>,
    ) -> Self {
        let x = x.into();
        let y = y.into();
        let z = z.into();
        let (Some(x), Some(y), Some(z)) =
            (x.constant_value(), y.constant_value(), z.constant_value())
        else {
            return self
                .property_source(PropertySources::Translation {
                    values: [x, y, z],
                    anchor: None,
                })
                .expect("invalid reactive property source");
        };
        self.assert_free_position();
        self.update_properties(|properties| {
            properties.translation = Some(PropertyTranslation::To(DVec3::new(x, y, z)))
        })
    }

    pub fn scale_by(self, factor: f64) -> Self {
        self.update_properties(|properties| properties.scale = Some(PropertyScale::Uniform(factor)))
    }

    pub fn scale_to(self, factor: impl Into<ScalarSource>) -> Self {
        let factor = factor.into();
        let Some(factor) = factor.constant_value() else {
            return self
                .property_source(PropertySources::Scale([
                    factor.clone(),
                    factor.clone(),
                    factor,
                ]))
                .expect("invalid reactive property source");
        };
        self.update_properties(|properties| {
            properties.scale = Some(PropertyScale::To(DVec3::splat(factor)))
        })
    }

    pub fn scale_to_3d(
        self,
        x: impl Into<ScalarSource>,
        y: impl Into<ScalarSource>,
        z: impl Into<ScalarSource>,
    ) -> Self {
        let x = x.into();
        let y = y.into();
        let z = z.into();
        let (Some(x), Some(y), Some(z)) =
            (x.constant_value(), y.constant_value(), z.constant_value())
        else {
            return self
                .property_source(PropertySources::Scale([x, y, z]))
                .expect("invalid reactive property source");
        };
        self.update_properties(|properties| {
            properties.scale = Some(PropertyScale::To(DVec3::new(x, y, z)))
        })
    }

    pub fn scale_by_3d(self, x: f64, y: f64, z: f64) -> Self {
        self.update_properties(|properties| {
            properties.scale = Some(PropertyScale::By(DVec3::new(x, y, z)))
        })
    }

    pub fn rotate_by(self, radians: f64) -> Self {
        let pivot = self.configured_pivot();
        self.update_properties(|properties| {
            properties.rotation = Some(PropertyRotation::By2D { radians, pivot })
        })
    }

    pub fn rotate_to(self, radians: impl Into<ScalarSource>) -> Self {
        let radians = radians.into();
        let Some(radians) = radians.constant_value() else {
            return self
                .property_source(PropertySources::Rotation([0.0.into(), 0.0.into(), radians]))
                .expect("invalid reactive property source");
        };
        self.update_properties(|properties| {
            properties.rotation = Some(PropertyRotation::To(DQuat::from_rotation_z(radians)))
        })
    }

    pub fn rotate_by_3d(self, axis: &str, radians: f64) -> Result<Self, String> {
        let delta = match axis.to_ascii_lowercase().as_str() {
            "x" => DQuat::from_rotation_x(radians),
            "y" => DQuat::from_rotation_y(radians),
            "z" => DQuat::from_rotation_z(radians),
            _ => {
                return Err(format!(
                    "invalid rotation axis {axis:?}; expected x, y, or z"
                ));
            }
        };
        Ok(self.update_properties(|properties| {
            properties.rotation = Some(PropertyRotation::By3D(delta))
        }))
    }

    pub fn rotate_to_3d(
        self,
        x: impl Into<ScalarSource>,
        y: impl Into<ScalarSource>,
        z: impl Into<ScalarSource>,
    ) -> Self {
        let x = x.into();
        let y = y.into();
        let z = z.into();
        let (Some(x), Some(y), Some(z)) =
            (x.constant_value(), y.constant_value(), z.constant_value())
        else {
            return self
                .property_source(PropertySources::Rotation([x, y, z]))
                .expect("invalid reactive property source");
        };
        self.update_properties(|properties| {
            properties.rotation = Some(PropertyRotation::To(DQuat::from_euler(
                EulerRot::XYZ,
                x,
                y,
                z,
            )))
        })
    }

    /// Select the drawable's entry fade effect. Scheduling still belongs to `Scene.play`.
    pub fn fade_in(self) -> Self {
        self.effect(AnimationType::FadeIn)
    }

    pub fn fade_in_from(self, direction: Direction, distance: f64) -> Self {
        self.effect(AnimationType::FadeInFrom {
            offset: direction.to_vector() * distance.max(0.0),
        })
    }

    pub fn fade_out(self) -> Self {
        self.effect(AnimationType::FadeOut)
    }

    pub fn write(self) -> Self {
        self.effect(AnimationType::Write {
            config: DrawAnimationConfig::default(),
        })
    }

    pub fn create(self) -> Self {
        let is_3d = self
            .property_spec
            .as_ref()
            .and_then(|spec| spec.lock().ok())
            .is_some_and(|spec| matches!(spec.kind, SpawnKind::Primitive3D(..)));
        if is_3d {
            self.effect(AnimationType::Create3D)
        } else {
            self.effect(AnimationType::Create {
                config: DrawAnimationConfig::default(),
            })
        }
    }

    pub fn unwrite(self) -> Self {
        self.effect(AnimationType::Unwrite {
            config: DrawAnimationConfig::default(),
        })
    }

    pub fn uncreate(self) -> Self {
        self.effect(AnimationType::Uncreate {
            config: DrawAnimationConfig::default(),
        })
    }

    pub fn grow_from_center(self) -> Self {
        self.effect(AnimationType::GrowFromCenter)
    }

    pub fn shrink_to_center(self) -> Self {
        self.effect(AnimationType::ShrinkToCenter)
    }

    pub fn spin_in_from_nothing(self) -> Self {
        self.effect(AnimationType::SpinInFromNothing)
    }

    pub fn draw_border_then_fill(self) -> Self {
        self.effect(AnimationType::DrawBorderThenFill {
            config: DrawAnimationConfig::default(),
        })
    }

    pub fn circumscribe(self) -> Self {
        self.effect(AnimationType::Circumscribe { color: None })
    }

    pub fn flash(self) -> Self {
        self.effect(AnimationType::Flash {
            color: None,
            n_lines: 16,
            radius: 100.0,
        })
    }

    pub fn show_passing_flash(self, time_width: f64) -> Self {
        self.effect(AnimationType::ShowPassingFlash {
            time_width: time_width.clamp(f64::EPSILON, 1.0),
        })
    }

    pub fn indicate(self) -> Self {
        if self.property_target_is_text_selection() {
            return self.selection_effect(crate::anim::TextSelectionEffect::Indicate);
        }
        self.effect(AnimationType::Indicate {
            color: None,
            scale_factor: 1.1,
        })
    }

    pub fn wiggle(self) -> Self {
        if self.property_target_is_text_selection() {
            return self.selection_effect(crate::anim::TextSelectionEffect::Wiggle);
        }
        self.effect(AnimationType::Wiggle)
    }

    pub fn pulse(self) -> Self {
        self.selection_effect(crate::anim::TextSelectionEffect::Pulse)
    }

    pub fn wave(self) -> Self {
        self.selection_effect(crate::anim::TextSelectionEffect::Wave)
    }

    pub fn highlight(self) -> Self {
        self.selection_effect(crate::anim::TextSelectionEffect::Highlight)
    }

    pub fn focus(self) -> Self {
        self.selection_effect(crate::anim::TextSelectionEffect::Focus)
    }

    pub fn cancel(self) -> Self {
        self.selection_effect(crate::anim::TextSelectionEffect::Cancel)
    }

    /// Target a scalar Parameter/Variable value through the common proxy.
    pub fn set(self, value: f64) -> Self {
        assert!(value.is_finite(), "parameter values must be finite");
        self.effect(AnimationType::SignalFloat { to: value })
    }

    pub fn transform_to(
        self,
        target: &super::drawable::DrawableHandle,
    ) -> Result<Self, &'static str> {
        if !self.belongs_to(&target.state) {
            return Err("transform targets must belong to the same Scene");
        }
        Ok(self.effect(AnimationType::Transform { target: target.id }))
    }

    pub fn move_along(
        self,
        target: &super::drawable::DrawableHandle,
    ) -> Result<Self, &'static str> {
        if !self.belongs_to(&target.state) {
            return Err("path targets must belong to the same Scene");
        }
        Ok(self.effect(AnimationType::MoveAlongPath {
            path: gaanim_core::kurbo::BezPath::new(),
            path_target: Some(target.id),
        }))
    }

    pub fn fade_transform_to(
        self,
        target: &super::drawable::DrawableHandle,
    ) -> Result<Self, &'static str> {
        if !self.belongs_to(&target.state) {
            return Err("transform targets must belong to the same Scene");
        }
        Ok(self.effect(AnimationType::FadeTransform { target: target.id }))
    }

    pub fn replacement_transform_to(
        self,
        target: &super::drawable::DrawableHandle,
    ) -> Result<Self, &'static str> {
        if !self.belongs_to(&target.state) {
            return Err("transform targets must belong to the same Scene");
        }
        Ok(self.effect(AnimationType::ReplacementTransform { target: target.id }))
    }

    pub fn into_builder(self) -> AnimationBuilder {
        self.inner
    }

    /// Set the duration if `sec` is `Some`, otherwise leave the default.
    /// Used internally by animation methods that accept an optional duration
    /// parameter (e.g. `obj.fade_in(2.0)`).
    pub(crate) fn with_duration(mut self, sec: Option<f64>) -> Self {
        if let Some(sec) = sec {
            self.inner.duration = sec.max(0.0);
            self.duration_explicit = true;
        }
        self
    }

    pub fn duration(mut self, sec: f64) -> Self {
        self.inner.duration = sec.max(0.0);
        self.duration_explicit = true;
        self
    }

    pub fn rate_func(mut self, f: RateFunc) -> Self {
        self.inner.rate_func = f;
        self.rate_explicit = true;
        self
    }

    pub fn lag_ratio(mut self, lag_ratio: f64) -> Self {
        self.inner = self.inner.lag_ratio(lag_ratio);
        self
    }

    pub fn stroke_width(mut self, stroke_width: f64) -> Self {
        if matches!(self.inner.anim_type, AnimationType::Properties(_)) {
            assert!(
                !self.property_target_is_primitive_3d(),
                "stroke_width() is only available for vector drawables"
            );
            return self.update_properties(|properties| {
                properties.stroke_width = Some(stroke_width.max(0.0));
            });
        }
        self.inner = self.inner.stroke_width(stroke_width);
        self
    }

    pub fn with_pen_tip(mut self) -> Self {
        self.inner = self.inner.with_pen_tip();
        self
    }

    pub fn pivot(mut self, x: f64, y: f64) -> Self {
        self.inner = self.inner.pivot(x, y);
        self
    }

    pub fn about_point(self, x: f64, y: f64) -> Self {
        self.pivot(x, y)
    }

    pub fn delay(mut self, sec: f64) -> Self {
        let delay = sec.max(0.0);
        self.inner.delay = delay;
        self
    }
}

impl From<Anim> for AnimationBuilder {
    fn from(anim: Anim) -> Self {
        anim.inner
    }
}

// ---------------------------------------------------------------------------
// OptDuration — allow animation methods to accept an optional duration
// ---------------------------------------------------------------------------

/// Trait that converts `()`, `f64`, or `Option<f64>` into an optional duration.
///
/// This lets animation methods accept all three forms:
/// - `obj.fade_in()`          — uses the default duration (1.0s)
/// - `obj.fade_in(2.0)`       — uses 2.0s
/// - `obj.fade_in(None)`      — uses the default duration (explicit)
pub trait OptDuration {
    fn into_opt(self) -> Option<f64>;
}

impl OptDuration for () {
    fn into_opt(self) -> Option<f64> {
        None
    }
}

impl OptDuration for f64 {
    fn into_opt(self) -> Option<f64> {
        Some(self)
    }
}

impl OptDuration for Option<f64> {
    fn into_opt(self) -> Option<f64> {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::{ImageCrop, ImageFit, ImageOptions, SceneFrame};
    use gaanim_core::peniko::ImageQuality;

    #[test]
    fn paint_targets_validate_and_commit_without_mutating_pending_anim() {
        use crate::canvas::SceneModel;
        use gaanim_core::peniko::{Brush, Color, Gradient};
        let mut scene = SceneModel::new(640, 360);
        let shape = scene.circle(10.0).fill(Color::BLACK);
        let paint = Brush::Gradient(
            Gradient::new_linear((0., 0.), (10., 0.)).with_stops([Color::BLACK, Color::WHITE]),
        );
        let anim = shape.animate().try_fill_paint(paint.clone()).unwrap();
        assert_eq!(
            shape.spec.lock().unwrap().fill,
            Some(Brush::Solid(Color::BLACK))
        );
        scene.play(vec![anim]);
        assert_eq!(shape.spec.lock().unwrap().fill, Some(paint));
        let incompatible = Brush::Gradient(
            Gradient::new_radial((0., 0.), 10.).with_stops([Color::BLACK, Color::WHITE]),
        );
        let invalid = shape.animate().try_fill_paint(incompatible).unwrap();
        let before = scene.current_time();
        assert!(scene.play_items(vec![invalid.clone().into()]).is_err());
        assert!(!invalid.is_consumed());
        assert_eq!(scene.current_time(), before);
        assert!(
            shape
                .animate()
                .try_stroke_paint(Brush::Solid(Color::BLACK), f64::NAN)
                .is_err()
        );
    }

    #[test]
    fn animation_move_to_accepts_same_scene_bounds_targets() {
        use crate::canvas::{Anchor, SceneModel};
        use gaanim_core::glam::DVec3;
        let mut scene = SceneModel::new(640, 360);
        let shape = scene.circle(10.0);
        let target = scene.rect(20.0, 30.0);
        assert!(shape.animate().move_to_drawable(&target).is_ok());
        assert!(
            shape
                .animate()
                .move_to_anchor_point(target.anchor_point(Anchor::TopRight, DVec3::ZERO))
                .is_ok()
        );
        let mut other = SceneModel::new(640, 360);
        let foreign = other.circle(2.0);
        assert!(shape.animate().move_to_drawable(&foreign).is_err());
        assert!(
            shape
                .animate()
                .move_to_anchor_point(foreign.anchor_point(Anchor::Center, DVec3::ZERO))
                .is_err()
        );
    }

    #[test]
    fn image_fit_resolves_contain_cover_and_crop() {
        let contain = ImageOptions {
            width: Some(200.0),
            height: Some(100.0),
            fit: ImageFit::Contain,
            crop: None,
            quality: ImageQuality::Medium,
        }
        .resolve(400, 100)
        .unwrap();
        assert_eq!(
            (contain.display_width, contain.display_height),
            (200.0, 50.0)
        );
        assert_eq!((contain.scale_x, contain.scale_y), (0.5, 0.5));

        let cover = ImageOptions {
            width: Some(200.0),
            height: Some(100.0),
            fit: ImageFit::Cover,
            crop: None,
            quality: ImageQuality::Medium,
        }
        .resolve(400, 100)
        .unwrap();
        assert_eq!((cover.display_width, cover.display_height), (200.0, 100.0));
        assert_eq!((cover.scale_x, cover.scale_y), (1.0, 1.0));

        let crop = ImageOptions {
            width: None,
            height: None,
            fit: ImageFit::Contain,
            crop: Some(ImageCrop {
                x: 50.0,
                y: 20.0,
                width: 120.0,
                height: 60.0,
            }),
            quality: ImageQuality::Medium,
        }
        .resolve(400, 100)
        .unwrap();
        assert_eq!((crop.source_x, crop.source_y), (50.0, 20.0));
        assert_eq!((crop.display_width, crop.display_height), (120.0, 60.0));
    }

    #[test]
    fn image_quality_is_preserved_in_the_resolved_view() {
        let view = ImageOptions {
            quality: ImageQuality::High,
            ..Default::default()
        }
        .resolve(640, 360)
        .unwrap();
        assert_eq!(view.quality, ImageQuality::High);
    }

    #[test]
    fn scene_frame_is_centered_validated_and_resolution_independent() {
        let frame = SceneFrame::WIDESCREEN.validate().unwrap();
        let bounds = frame.bounds();
        assert_eq!((bounds.min.x, bounds.min.y), (-8.0, -4.5));
        assert_eq!((bounds.max.x, bounds.max.y), (8.0, 4.5));
        assert_eq!(frame.preview_pixel_size(), (1280, 720));
        assert_eq!(SceneFrame::VERTICAL.bounds().min.x, -4.5);
        assert_eq!(SceneFrame::VERTICAL.bounds().max.y, 8.0);
        assert_eq!(SceneFrame::SQUARE.aspect_ratio(), 1.0);
        assert!(SceneFrame::new(0.0, 9.0).validate().is_err());
        assert!(SceneFrame::new(-16.0, 9.0).validate().is_err());
        assert!(SceneFrame::new(16.0, f64::INFINITY).validate().is_err());
        assert!(SceneFrame::new(f64::NAN, 9.0).validate().is_err());
    }
}
