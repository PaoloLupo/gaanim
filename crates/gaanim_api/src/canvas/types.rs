//! Core types: coordinate system, mobject kinds, specs, and queued Anim.

use gaanim_core::ObjectId;
use gaanim_core::glam::DVec3;
use gaanim_core::peniko::{Brush, Color, ImageData};
use gaanim_layout::{Anchor, Direction};
use gaanim_math::{Bounds3D, EasingCurve, RateFunc};
use gaanim_objects::prelude::{ImageView, SvgPath};
use std::path::PathBuf;

use crate::anim::{AnimationBuilder, AnimationType};
use crate::canvas::ops::{Op, SharedCanvasState};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CoordinateSystem {
    Pixels,
    Scene { frame_width: f64, frame_height: f64 },
}

impl Default for CoordinateSystem {
    fn default() -> Self {
        Self::Pixels
    }
}

impl CoordinateSystem {
    pub fn frame_bounds(&self, canvas_width: u32, canvas_height: u32) -> Bounds3D {
        match self {
            Self::Pixels => {
                let half_width = canvas_width as f64 * 0.5;
                let half_height = canvas_height as f64 * 0.5;
                Bounds3D::new_2d(-half_width, -half_height, half_width, half_height)
            }
            Self::Scene {
                frame_width,
                frame_height,
            } => {
                let half_width = *frame_width * 0.5;
                let half_height = *frame_height * 0.5;
                Bounds3D::new_2d(-half_width, -half_height, half_width, half_height)
            }
        }
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

/// Horizontal alignment for multi-line paragraph text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TextAlign {
    #[default]
    Left,
    Center,
    Right,
    Justify,
}

/// How a paragraph behaves when a maximum line count constrains its height.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ParagraphOverflow {
    /// Keep all lines visible even if they extend beyond the nominal text box.
    Visible,
    /// Clip lines outside the text box. This is the safe default for video layouts.
    #[default]
    Clip,
}

/// Vector paragraph layout options.
#[derive(Debug, Clone, PartialEq)]
pub struct ParagraphOptions {
    /// Maximum line width in canvas units/Typst points.
    pub width: f64,
    pub align: TextAlign,
    /// Baseline multiplier. `1.0` is compact and `1.2` is a readable default.
    pub line_spacing: f64,
    /// Overrides the configured Body role when present.
    pub font_size: Option<f64>,
    /// Overrides the configured Body role when present.
    pub font_family: Option<String>,
    /// Optional maximum number of visible lines.
    pub max_lines: Option<usize>,
    pub overflow: ParagraphOverflow,
}

impl ParagraphOptions {
    pub fn new(width: f64) -> Self {
        Self {
            width: width.max(1.0),
            ..Self::default()
        }
    }
}

impl Default for ParagraphOptions {
    fn default() -> Self {
        Self {
            width: 640.0,
            align: TextAlign::Left,
            line_spacing: 1.2,
            font_size: None,
            font_family: None,
            max_lines: None,
            overflow: ParagraphOverflow::Clip,
        }
    }
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
            axis_width: 3.0,
            grid_width: 1.0,
            tick_width: 2.0,
            tick_length: 8.0,
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

/// Optional destination sizing and source crop for `Canvas::image_with_options`.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct ImageOptions {
    pub width: Option<f64>,
    pub height: Option<f64>,
    pub fit: ImageFit,
    pub crop: Option<ImageCrop>,
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
        })
    }
}

#[derive(Debug, Clone)]
pub enum SpawnKind {
    Circle(f64),
    Rect(f64, f64),
    RoundedRect(f64, f64, f64),
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
    },
    /// 3D polyline (e.g., curve) defined by world-space points.
    /// If `colors` is Some and length matches `points`, per-vertex colors are used (colormap).
    Polyline3D {
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
    Text(String),
    Paragraph {
        text: String,
        options: ParagraphOptions,
    },
    Title(String),
    Subtitle(String),
    Equation(String),
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
    /// One resolved vector path imported from an SVG document.
    SvgPath(Box<SvgPath>),
    Group(Vec<ObjectId>),
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
    SetScale(f64),
    SetScale3D(DVec3),
    SetRotation(f64),
    SetRotation3D(DVec3),
    /// Scene-space point around which rotation and scaling are performed.
    SetPivot(DVec3),
    MoveAnchorTo {
        target: DVec3,
        anchor: Anchor,
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
    /// Arrange the direct children of a group before its own placement is resolved.
    Arrange {
        direction: Direction,
        spacing: f64,
        aligned_edge: Anchor,
    },
}

/// The small set of container algorithms exposed by the high-level layout API.
///
/// Layouts intentionally use the same primitives as groups: every child can be
/// a shape, text, equation, group, or another layout container.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayoutKind {
    Row,
    Column,
    Grid { columns: usize },
}

#[derive(Debug, Clone)]
pub struct ObjectSpec {
    pub id: ObjectId,
    pub kind: SpawnKind,
    pub fill: Option<Brush>,
    pub fill_overridden: bool,
    pub stroke: Option<(Brush, f64)>,
    pub stroke_overridden: bool,
    pub glow: Option<gaanim_renderer::effects::Glow>,
    pub blur: Option<gaanim_renderer::effects::GaussianBlur>,
    pub shadow: Option<gaanim_renderer::effects::DropShadow>,
    pub opacity: f32,
    pub z_index: i32,
    /// If true, this object should billboard (face camera) in 3D.
    pub billboard: bool,
    /// If true, this object is a HUD overlay (screen-space, fixed).
    pub hud: bool,
    /// Fill overrides applied to matching glyph fragments after textual objects
    /// have been compiled into their vector hierarchy.
    pub fragment_fills: Vec<(String, Color)>,
    /// Named fragment queries attached by the high-level equation API.
    pub fragment_tags: Vec<(String, String, Option<usize>)>,
    pub layout_ops: Vec<LayoutOp>,
}

impl ObjectSpec {
    pub(crate) fn new(id: ObjectId, kind: SpawnKind) -> Self {
        Self {
            id,
            kind,
            fill: None,
            fill_overridden: false,
            stroke: None,
            stroke_overridden: false,
            glow: None,
            blur: None,
            shadow: None,
            opacity: 1.0,
            z_index: 0,
            billboard: false,
            hud: false,
            fragment_fills: Vec::new(),
            fragment_tags: Vec::new(),
            layout_ops: Vec::new(),
        }
    }
}

/// A queued Canvas animation.
///
/// Methods like `DrawableHandle::fade_in()` create one of these and immediately
/// append an active `Op::Animate` to the owning segment. Fluent configuration
/// methods update both this value and the queued op, so `obj.fade_in(2.0)` or
/// keeps the deferred timeline consistent.
#[derive(Debug, Clone)]
pub struct Anim {
    pub inner: AnimationBuilder,
    queued: Option<QueuedAnim>,
}

#[derive(Debug, Clone)]
struct QueuedAnim {
    state: SharedCanvasState,
    segment_idx: usize,
    op_idx: usize,
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
            queued: None,
        }
    }

    pub(crate) fn queued(
        target: ObjectId,
        anim_type: AnimationType,
        state: SharedCanvasState,
        segment_idx: usize,
    ) -> Self {
        let mut anim = Self::new(target, anim_type);
        let mut guard = state.lock().expect("canvas state poisoned");
        let segment = &mut guard.segments[segment_idx];
        let op_idx = segment.ops.len();
        // Only advance cursor by duration; delay is per-animation and does not
        // occupy segment time (it shifts the clip start during compilation).
        segment.cursor += anim.inner.duration;
        segment.ops.push(Op::Animate {
            anim: anim.inner.clone(),
            active: true,
        });
        drop(guard);
        anim.queued = Some(QueuedAnim {
            state,
            segment_idx,
            op_idx,
        });
        anim
    }

    pub fn into_builder(self) -> AnimationBuilder {
        self.inner
    }

    /// Set the duration if `sec` is `Some`, otherwise leave the default.
    /// Used internally by animation methods that accept an optional duration
    /// parameter (e.g. `obj.fade_in(2.0)`).
    pub(crate) fn with_duration(mut self, sec: Option<f64>) -> Self {
        if let Some(sec) = sec {
            let old = self.inner.duration;
            self.inner.duration = sec.max(0.0);
            self.sync_queued(Some(old));
        }
        self
    }

    pub(crate) fn deactivate_auto_queue(&self) -> bool {
        let Some(queue) = &self.queued else {
            return false;
        };
        let mut guard = queue.state.lock().expect("canvas state poisoned");
        let Some(segment) = guard.segments.get_mut(queue.segment_idx) else {
            return false;
        };
        let Some(Op::Animate { anim, active }) = segment.ops.get_mut(queue.op_idx) else {
            return false;
        };
        if !*active {
            return false;
        }
        *active = false;
        segment.cursor -= anim.duration;
        true
    }

    fn sync_queued(&self, old_duration: Option<f64>) {
        let Some(queue) = &self.queued else {
            return;
        };
        let mut guard = queue.state.lock().expect("canvas state poisoned");
        let Some(segment) = guard.segments.get_mut(queue.segment_idx) else {
            return;
        };
        let Some(Op::Animate { anim, active }) = segment.ops.get_mut(queue.op_idx) else {
            return;
        };
        if let Some(old_duration) = old_duration {
            if *active {
                segment.cursor += self.inner.duration - old_duration;
            }
        }
        *anim = self.inner.clone();
    }

    pub fn duration(mut self, sec: f64) -> Self {
        let old = self.inner.duration;
        self.inner.duration = sec.max(0.0);
        self.sync_queued(Some(old));
        self
    }

    pub fn rate_func(mut self, f: RateFunc) -> Self {
        self.inner.rate_func = f;
        self.sync_queued(None);
        self
    }

    pub fn spring(self) -> Self {
        self.rate_func(RateFunc::Spring {
            stiffness: 90.0,
            damping: 12.0,
        })
    }

    pub fn smooth(self) -> Self {
        self.rate_func(RateFunc::Smooth)
    }

    pub fn linear(self) -> Self {
        self.rate_func(RateFunc::Linear)
    }

    pub fn lag_ratio(mut self, lag_ratio: f64) -> Self {
        self.inner = self.inner.lag_ratio(lag_ratio);
        self.sync_queued(None);
        self
    }

    pub fn stroke_width(mut self, stroke_width: f64) -> Self {
        self.inner = self.inner.stroke_width(stroke_width);
        self.sync_queued(None);
        self
    }

    pub fn with_pen_tip(mut self) -> Self {
        self.inner = self.inner.with_pen_tip();
        self.sync_queued(None);
        self
    }

    pub fn pivot(mut self, x: f64, y: f64) -> Self {
        self.inner = self.inner.pivot(x, y);
        self.sync_queued(None);
        self
    }

    pub fn about_point(self, x: f64, y: f64) -> Self {
        self.pivot(x, y)
    }

    pub fn delay(mut self, sec: f64) -> Self {
        let delay = sec.max(0.0);
        self.inner.delay = delay;
        // Sync the delay into the queued Op so the compiler sees it.
        // Do NOT adjust segment.cursor: delay is a per-animation offset,
        // not segment time. The cursor only tracks cumulative duration.
        if let Some(queue) = &self.queued {
            let mut guard = queue.state.lock().expect("canvas state poisoned");
            if let Some(segment) = guard.segments.get_mut(queue.segment_idx) {
                if let Some(Op::Animate { anim, .. }) = segment.ops.get_mut(queue.op_idx) {
                    anim.delay = delay;
                }
            }
        }
        self
    }

    pub fn steps(self, n: u32) -> Self {
        self.rate_func(RateFunc::Steps(n.max(1)))
    }

    pub fn rate(self, name: &str) -> Self {
        self.ease(name)
    }

    pub fn ease(self, name: &str) -> Self {
        let rate_func = match name {
            "linear" => RateFunc::Linear,
            "smooth" | "ease" => RateFunc::Smooth,
            "ease_in" | "ease_in_quad" => RateFunc::EaseIn(EasingCurve::Quadratic),
            "ease_out" | "ease_out_quad" => RateFunc::EaseOut(EasingCurve::Quadratic),
            "ease_in_out" => RateFunc::EaseInOut(EasingCurve::Quadratic),
            "ease_in_cubic" => RateFunc::EaseIn(EasingCurve::Cubic),
            "ease_out_cubic" => RateFunc::EaseOut(EasingCurve::Cubic),
            "bounce" | "ease_out_bounce" => RateFunc::EaseOut(EasingCurve::Bounce),
            "elastic" | "ease_out_elastic" => RateFunc::EaseOut(EasingCurve::Elastic),
            "spring" => RateFunc::Spring {
                stiffness: 300.0,
                damping: 20.0,
            },
            "back" | "ease_out_back" => RateFunc::EaseOut(EasingCurve::Back),
            "there_and_back" => RateFunc::ThereAndBack,
            "running_start" => RateFunc::RunningStart,
            "exponential_decay" => RateFunc::ExponentialDecay,
            _ => RateFunc::Smooth,
        };
        self.rate_func(rate_func)
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
    use super::{ImageCrop, ImageFit, ImageOptions};

    #[test]
    fn image_fit_resolves_contain_cover_and_crop() {
        let contain = ImageOptions {
            width: Some(200.0),
            height: Some(100.0),
            fit: ImageFit::Contain,
            crop: None,
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
        }
        .resolve(400, 100)
        .unwrap();
        assert_eq!((crop.source_x, crop.source_y), (50.0, 20.0));
        assert_eq!((crop.display_width, crop.display_height), (120.0, 60.0));
    }
}
