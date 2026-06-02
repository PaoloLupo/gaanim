use gaanim_api::prelude::LayoutDirection;
use gaanim_core::peniko;
use gaanim_core::ObjectId;
use gaanim_math::SpatialTransform;
use pyo3::prelude::*;
use std::sync::{Arc, Mutex};

use crate::animation::PyAnimationSpec;
use crate::color::PyColor;
use crate::id::PyObjectId;

/// Visual properties shared by all mobject kinds.
#[derive(Clone, Copy, Debug)]
pub struct CommonSpec {
    pub fill: Option<peniko::Color>,
    pub stroke: Option<(peniko::Color, f64)>,
    pub z_index: i32,
    pub opacity: f32,
    pub transform: SpatialTransform,
    pub next_to: Option<(ObjectId, LayoutDirection, f64)>,
}

/// What kind of mobject will be spawned at replay time. The configuration
/// (`fill`, `stroke`, `z_index`, `transform`, `next_to`, …) is attached and
/// replayed during the Bevy `Startup` system.
#[derive(Clone, Debug)]
pub enum MobjectSpec {
    Circle { common: CommonSpec, radius: f64 },
    Rectangle { common: CommonSpec, width: f64, height: f64 },
    RoundedRect { common: CommonSpec, width: f64, height: f64, radius: f64 },
    Line { common: CommonSpec, start: (f64, f64), end: (f64, f64) },
    Polygon { common: CommonSpec, points: Vec<(f64, f64)> },
    Star { common: CommonSpec, n_points: u32, outer_radius: f64, inner_radius: f64 },
    Ellipse { common: CommonSpec, rx: f64, ry: f64 },
    Dot { common: CommonSpec, radius: f64 },
    Square { common: CommonSpec, side: f64 },
    Checkmark { common: CommonSpec, size: f64 },
    Arrow { common: CommonSpec, start: (f64, f64), end: (f64, f64) },
    RegularPolygon { common: CommonSpec, n_sides: u32, radius: f64 },
    DashedLine { common: CommonSpec, start: (f64, f64), end: (f64, f64), dash_length: f64, gap_length: f64 },
    Arc { common: CommonSpec, center: (f64, f64), rx: f64, ry: f64, start_angle: f64, sweep_angle: f64 },
    ArcBetweenPoints { common: CommonSpec, start: (f64, f64), end: (f64, f64), angle: f64 },
    DoubleArrow { common: CommonSpec, start: (f64, f64), end: (f64, f64), head_len: Option<f64>, head_width: Option<f64> },
    Sector { common: CommonSpec, center: (f64, f64), radius: f64, start_angle: f64, sweep_angle: f64 },
    Annulus { common: CommonSpec, outer_radius: f64, inner_radius: f64 },
    SurroundingRectangle { common: CommonSpec, width: f64, height: f64, corner_radius: f64 },
    BackgroundRectangle { common: CommonSpec, width: f64, height: f64 },
    Cross { common: CommonSpec, size: f64 },
    RightAngle { common: CommonSpec, arm_length: f64 },
    BooleanResult {
        common: CommonSpec,
        contours: Vec<Vec<[f64; 2]>>,
    },
    Text { common: CommonSpec, content: String, role: TextRoleKind },
    Equation { common: CommonSpec, formula: String },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextRoleKind {
    Title,
    Subtitle,
    Body,
    Caption,
    Code,
}

impl TextRoleKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Title => "title",
            Self::Subtitle => "subtitle",
            Self::Body => "body",
            Self::Caption => "caption",
            Self::Code => "code",
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s {
            "title" => Self::Title,
            "subtitle" => Self::Subtitle,
            "caption" => Self::Caption,
            "code" => Self::Code,
            _ => Self::Body,
        }
    }
}

impl MobjectSpec {
    fn common(&self) -> &CommonSpec {
        match self {
            Self::Circle { common, .. }
            | Self::Rectangle { common, .. }
            | Self::RoundedRect { common, .. }
            | Self::Line { common, .. }
            | Self::Polygon { common, .. }
            | Self::Star { common, .. }
            | Self::Ellipse { common, .. }
            | Self::Dot { common, .. }
            | Self::Square { common, .. }
            | Self::Checkmark { common, .. }
            | Self::Arrow { common, .. }
            | Self::RegularPolygon { common, .. }
            | Self::DashedLine { common, .. }
            | Self::Arc { common, .. }
            | Self::ArcBetweenPoints { common, .. }
            | Self::DoubleArrow { common, .. }
            | Self::Sector { common, .. }
            | Self::Annulus { common, .. }
            | Self::SurroundingRectangle { common, .. }
            | Self::BackgroundRectangle { common, .. }
            | Self::Cross { common, .. }
            | Self::RightAngle { common, .. }
            | Self::BooleanResult { common, .. }
            | Self::Text { common, .. }
            | Self::Equation { common, .. } => common,
        }
    }

    fn common_mut(&mut self) -> &mut CommonSpec {
        match self {
            Self::Circle { common, .. }
            | Self::Rectangle { common, .. }
            | Self::RoundedRect { common, .. }
            | Self::Line { common, .. }
            | Self::Polygon { common, .. }
            | Self::Star { common, .. }
            | Self::Ellipse { common, .. }
            | Self::Dot { common, .. }
            | Self::Square { common, .. }
            | Self::Checkmark { common, .. }
            | Self::Arrow { common, .. }
            | Self::RegularPolygon { common, .. }
            | Self::DashedLine { common, .. }
            | Self::Arc { common, .. }
            | Self::ArcBetweenPoints { common, .. }
            | Self::DoubleArrow { common, .. }
            | Self::Sector { common, .. }
            | Self::Annulus { common, .. }
            | Self::SurroundingRectangle { common, .. }
            | Self::BackgroundRectangle { common, .. }
            | Self::Cross { common, .. }
            | Self::RightAngle { common, .. }
            | Self::BooleanResult { common, .. }
            | Self::Text { common, .. }
            | Self::Equation { common, .. } => common,
        }
    }

    pub fn kind_name(&self) -> &'static str {
        match self {
            Self::Circle { .. } => "circle",
            Self::Rectangle { .. } => "rectangle",
            Self::RoundedRect { .. } => "rounded_rect",
            Self::Line { .. } => "line",
            Self::Polygon { .. } => "polygon",
            Self::Star { .. } => "star",
            Self::Ellipse { .. } => "ellipse",
            Self::Dot { .. } => "dot",
            Self::Square { .. } => "square",
            Self::Checkmark { .. } => "checkmark",
            Self::Arrow { .. } => "arrow",
            Self::RegularPolygon { .. } => "regular_polygon",
            Self::DashedLine { .. } => "dashed_line",
            Self::Arc { .. } => "arc",
            Self::ArcBetweenPoints { .. } => "arc_between_points",
            Self::DoubleArrow { .. } => "double_arrow",
            Self::Sector { .. } => "sector",
            Self::Annulus { .. } => "annulus",
            Self::SurroundingRectangle { .. } => "surrounding_rectangle",
            Self::BackgroundRectangle { .. } => "background_rectangle",
            Self::Cross { .. } => "cross",
            Self::RightAngle { .. } => "right_angle",
            Self::BooleanResult { .. } => "boolean_result",
            Self::Text { .. } => "text",
            Self::Equation { .. } => "equation",
        }
    }

    pub fn fill(&self) -> Option<peniko::Color> { self.common().fill }
    pub fn stroke(&self) -> Option<(peniko::Color, f64)> { self.common().stroke }
    pub fn opacity(&self) -> f32 { self.common().opacity }
    pub fn z_index(&self) -> i32 { self.common().z_index }
    pub fn transform(&self) -> SpatialTransform { self.common().transform }
    pub fn next_to(&self) -> Option<(ObjectId, LayoutDirection, f64)> { self.common().next_to }

    fn set_fill(&mut self, color: Option<peniko::Color>) { self.common_mut().fill = color; }
    fn set_stroke(&mut self, stroke: Option<(peniko::Color, f64)>) { self.common_mut().stroke = stroke; }
    fn set_opacity(&mut self, opacity: f32) { self.common_mut().opacity = opacity; }
    fn set_z_index(&mut self, z: i32) { self.common_mut().z_index = z; }
    fn set_transform(&mut self, t: SpatialTransform) { self.common_mut().transform = t; }
    fn transform_mut(&mut self) -> &mut SpatialTransform { &mut self.common_mut().transform }
    fn set_next_to(&mut self, hint: Option<(ObjectId, LayoutDirection, f64)>) { self.common_mut().next_to = hint; }
}

impl MobjectSpec {
    /// Reconstruct the path of this mobject as a list of polylines (one per
    /// outer ring; holes are interleaved using even-odd ordering).
    /// Used at build time by boolean operations to compose geometry.
    pub fn to_contours(&self) -> Vec<Vec<[f64; 2]>> {
        let mut out: Vec<Vec<[f64; 2]>> = Vec::new();
        let push_circle = |cx: f64, cy: f64, r: f64, out: &mut Vec<Vec<[f64; 2]>>| {
            let steps = 64;
            let mut ring = Vec::with_capacity(steps + 1);
            for i in 0..=steps {
                let a = i as f64 * 2.0 * std::f64::consts::PI / steps as f64;
                ring.push([cx + r * a.cos(), cy + r * a.sin()]);
            }
            out.push(ring);
        };
        let push_rect = |cx: f64, cy: f64, w: f64, h: f64, out: &mut Vec<Vec<[f64; 2]>>| {
            out.push(vec![
                [cx - w / 2.0, cy - h / 2.0],
                [cx + w / 2.0, cy - h / 2.0],
                [cx + w / 2.0, cy + h / 2.0],
                [cx - w / 2.0, cy + h / 2.0],
            ]);
        };
        let push_polygon = |pts: &[(f64, f64)], out: &mut Vec<Vec<[f64; 2]>>| {
            out.push(pts.iter().map(|(x, y)| [*x, *y]).collect());
        };
        let push_line = |x1: f64, y1: f64, x2: f64, y2: f64, out: &mut Vec<Vec<[f64; 2]>>| {
            out.push(vec![[x1, y1], [x2, y2]]);
        };
        match self {
            Self::Circle { radius, .. } => push_circle(0.0, 0.0, *radius, &mut out),
            Self::Square { side, .. } => push_rect(0.0, 0.0, *side, *side, &mut out),
            Self::Rectangle { width, height, .. } => push_rect(0.0, 0.0, *width, *height, &mut out),
            Self::RoundedRect { width, height, .. } => push_rect(0.0, 0.0, *width, *height, &mut out),
            Self::Polygon { points, .. } => push_polygon(points, &mut out),
            Self::Dot { radius, .. } => push_circle(0.0, 0.0, *radius, &mut out),
            Self::Star { n_points, outer_radius, inner_radius, .. } => {
                let mut ring = Vec::new();
                let total = (*n_points as usize) * 2;
                for i in 0..total {
                    let a = i as f64 * std::f64::consts::PI / *n_points as f64;
                    let r = if i % 2 == 0 { *outer_radius } else { *inner_radius };
                    ring.push([r * a.cos(), r * a.sin()]);
                }
                out.push(ring);
            }
            Self::Ellipse { rx, ry, .. } => {
                let steps = 64;
                let mut ring = Vec::with_capacity(steps + 1);
                for i in 0..=steps {
                    let a = i as f64 * 2.0 * std::f64::consts::PI / steps as f64;
                    ring.push([rx * a.cos(), ry * a.sin()]);
                }
                out.push(ring);
            }
            Self::Checkmark { size, .. } => {
                out.push(vec![[0.0, 0.0], [*size * 0.5, *size * 0.5], [*size, 0.0]]);
            }
            Self::RegularPolygon { n_sides, radius, .. } => {
                let mut ring = Vec::new();
                for i in 0..*n_sides {
                    let a = i as f64 * 2.0 * std::f64::consts::PI / *n_sides as f64;
                    ring.push([radius * a.cos(), radius * a.sin()]);
                }
                out.push(ring);
            }
            Self::Line { start, end, .. } => push_line(start.0, start.1, end.0, end.1, &mut out),
            Self::Arrow { start, end, .. } => {
                // Approximate arrow as a thin rectangle for boolean purposes
                let dx = end.0 - start.0;
                let dy = end.1 - start.1;
                let len = (dx * dx + dy * dy).sqrt().max(0.001);
                let ux = dx / len;
                let uy = dy / len;
                let h = 2.0;
                out.push(vec![
                    [start.0 - uy * h, start.1 + ux * h],
                    [end.0 - uy * h, end.1 + ux * h],
                    [end.0 + uy * h, end.1 - ux * h],
                    [start.0 + uy * h, start.1 - ux * h],
                ]);
            }
            Self::DoubleArrow { start, end, .. } => {
                let dx = end.0 - start.0;
                let dy = end.1 - start.1;
                let len = (dx * dx + dy * dy).sqrt().max(0.001);
                let ux = dx / len;
                let uy = dy / len;
                let h = 2.0;
                out.push(vec![
                    [start.0 - uy * h, start.1 + ux * h],
                    [end.0 - uy * h, end.1 + ux * h],
                    [end.0 + uy * h, end.1 - ux * h],
                    [start.0 + uy * h, start.1 - ux * h],
                ]);
            }
            Self::DashedLine { start, end, .. } => push_line(start.0, start.1, end.0, end.1, &mut out),
            Self::Arc { center, rx, ry, start_angle, sweep_angle, .. } => {
                let steps = ((sweep_angle.abs() * 30.0).ceil() as u32).max(8);
                let mut ring = Vec::with_capacity(steps as usize + 1);
                for i in 0..=steps {
                    let t = i as f64 / steps as f64;
                    let a = start_angle + sweep_angle * t;
                    ring.push([center.0 + rx * a.cos(), center.1 + ry * a.sin()]);
                }
                out.push(ring);
            }
            Self::ArcBetweenPoints { start, end, angle, .. } => {
                let mid_x = (start.0 + end.0) * 0.5;
                let mid_y = (start.1 + end.1) * 0.5;
                let dx = end.0 - start.0;
                let dy = end.1 - start.1;
                let chord = (dx * dx + dy * dy).sqrt();
                let radius = if angle.abs() < 1e-6 { chord * 0.5 } else { (chord * 0.5) / (angle * 0.5).sin().abs() };
                let r_sign = if *angle >= 0.0 { 1.0 } else { -1.0 };
                let h = (radius * radius - chord * chord * 0.25).sqrt();
                let nx = -dy / chord;
                let ny = dx / chord;
                let cx = mid_x + nx * h * r_sign;
                let cy = mid_y + ny * h * r_sign;
                let sa = (start.1 - cy).atan2(start.0 - cx);
                let ea = (end.1 - cy).atan2(end.0 - cx);
                let mut sweep = ea - sa;
                if *angle > 0.0 && sweep < 0.0 { sweep += 2.0 * std::f64::consts::PI; }
                if *angle < 0.0 && sweep > 0.0 { sweep -= 2.0 * std::f64::consts::PI; }
                let steps = ((sweep.abs() * 30.0).ceil() as u32).max(8);
                let mut ring = Vec::with_capacity(steps as usize + 1);
                for i in 0..=steps {
                    let t = i as f64 / steps as f64;
                    let a = sa + sweep * t;
                    ring.push([cx + radius * a.cos(), cy + radius * a.sin()]);
                }
                out.push(ring);
            }
            Self::Sector { center, radius, start_angle, sweep_angle, .. } => {
                let mut ring = vec![[center.0, center.1]];
                let steps = ((sweep_angle.abs() * 30.0).ceil() as u32).max(8);
                for i in 0..=steps {
                    let t = i as f64 / steps as f64;
                    let a = start_angle + sweep_angle * t;
                    ring.push([center.0 + radius * a.cos(), center.1 + radius * a.sin()]);
                }
                out.push(ring);
            }
            Self::Annulus { outer_radius, inner_radius, .. } => {
                let steps = 64;
                let mut outer = Vec::with_capacity(steps + 1);
                for i in 0..=steps {
                    let a = i as f64 * 2.0 * std::f64::consts::PI / steps as f64;
                    outer.push([outer_radius * a.cos(), outer_radius * a.sin()]);
                }
                out.push(outer);
                let mut inner = Vec::with_capacity(steps + 1);
                for i in 0..=steps {
                    let a = i as f64 * 2.0 * std::f64::consts::PI / steps as f64;
                    inner.push([inner_radius * a.cos(), inner_radius * a.sin()]);
                }
                out.push(inner);
            }
            Self::SurroundingRectangle { width, height, .. } => push_rect(0.0, 0.0, *width, *height, &mut out),
            Self::BackgroundRectangle { width, height, .. } => push_rect(0.0, 0.0, *width, *height, &mut out),
            Self::Cross { size, .. } => {
                let h = size * 0.5;
                out.push(vec![[-h, -h], [h, h]]);
                out.push(vec![[h, -h], [-h, h]]);
            }
            Self::RightAngle { arm_length, .. } => {
                out.push(vec![[0.0, 0.0], [*arm_length, 0.0], [0.0, 0.0], [0.0, *arm_length]]);
            }
            Self::BooleanResult { contours, .. } => {
                for c in contours {
                    out.push(c.clone());
                }
            }
            Self::Text { .. } | Self::Equation { .. } => {
                // Text/equation geometry cannot be reconstructed at build time
                // because the actual layout is computed at replay time.
            }
        }
        out
    }
}

/// A Python handle to a Mobject (real or about to be spawned).
///
/// All "instant" setters (`fill`, `stroke`, `at`, `next_to`, `z_index`, …)
/// mutate the shared `spec` (Arc<Mutex<>>) so the change propagates to the
/// queued `Spawn` op that the runtime reads at replay time.
#[pyclass(name = "Mobject", module = "gaanim_core", from_py_object)]
#[derive(Clone, Debug)]
pub struct PyMobject {
    pub id: ObjectId,
    pub spec: Arc<Mutex<MobjectSpec>>,
    /// Monotonically increasing counter set at allocation time. Used as the
    /// `RenderOrder.creation_order` to keep a stable z-order tiebreak even
    /// after scene rebuilds.
    pub creation_order: u64,
}

macro_rules! lock_spec {
    ($spec:expr) => {
        match $spec.lock() {
            Ok(guard) => guard,
            Err(_) => {
                return Err(pyo3::exceptions::PyRuntimeError::new_err(
                    "MobjectSpec mutex is poisoned",
                ))
            }
        }
    };
}

#[pymethods]
impl PyMobject {
    #[getter]
    fn id(&self) -> PyObjectId {
        PyObjectId(self.id)
    }

    fn __repr__(&self) -> PyResult<String> {
        let kind = lock_spec!(self.spec).kind_name().to_string();
        Ok(format!(
            "Mobject(ObjectId({}v{}), kind={}, creation_order={})",
            self.id.index(),
            self.id.generation(),
            kind,
            self.creation_order,
        ))
    }

    // ====== instant configuration (return new Mobject, mutate shared spec) ======

    fn fill(&self, color: &PyColor) -> PyResult<Self> {
        let new = self.clone();
        lock_spec!(new.spec).set_fill(Some(color.0));
        Ok(new)
    }

    fn no_fill(&self) -> PyResult<Self> {
        let new = self.clone();
        lock_spec!(new.spec).set_fill(None);
        Ok(new)
    }

    fn stroke(&self, color: &PyColor, width: f64) -> PyResult<Self> {
        let new = self.clone();
        lock_spec!(new.spec).set_stroke(Some((color.0, width)));
        Ok(new)
    }

    fn no_stroke(&self) -> PyResult<Self> {
        let new = self.clone();
        lock_spec!(new.spec).set_stroke(None);
        Ok(new)
    }

    fn opacity(&self, opacity: f32) -> PyResult<Self> {
        let new = self.clone();
        lock_spec!(new.spec).set_opacity(opacity);
        Ok(new)
    }

    fn z_index(&self, z: i32) -> PyResult<Self> {
        let new = self.clone();
        lock_spec!(new.spec).set_z_index(z);
        Ok(new)
    }

    /// Set absolute 2D position (applied at spawn time as the initial transform).
    fn at(&self, x: f64, y: f64) -> PyResult<Self> {
        let new = self.clone();
        lock_spec!(new.spec).set_transform(SpatialTransform::new_2d(x, y));
        Ok(new)
    }

    /// Add to existing 2D position.
    fn shift(&self, dx: f64, dy: f64) -> PyResult<Self> {
        let new = self.clone();
        let mut s = lock_spec!(new.spec);
        let t = s.transform_mut();
        *t = t.shift_2d(dx, dy);
        drop(s);
        Ok(new)
    }

    fn scale(&self, factor: f64) -> PyResult<Self> {
        let new = self.clone();
        let mut s = lock_spec!(new.spec);
        let t = s.transform_mut();
        *t = t.scale_uniform(factor);
        drop(s);
        Ok(new)
    }

    fn rotate(&self, radians: f64) -> PyResult<Self> {
        let new = self.clone();
        let mut s = lock_spec!(new.spec);
        let t = s.transform_mut();
        *t = t.with_rotation_2d(radians);
        drop(s);
        Ok(new)
    }

    /// Place this mobject adjacent to a reference mobject in a layout direction.
    /// Resolved at spawn time using the reference's state in the scene.
    #[pyo3(signature = (reference, direction, spacing=10.0))]
    fn next_to(&self, reference: &PyMobject, direction: &str, spacing: f64) -> PyResult<Self> {
        let dir = direction_from_str(direction).ok_or_else(|| {
            pyo3::exceptions::PyValueError::new_err(format!(
                "unknown direction: {} (use 'up', 'down', 'left', 'right')",
                direction
            ))
        })?;
        let new = self.clone();
        lock_spec!(new.spec).set_next_to(Some((reference.id, dir, spacing)));
        Ok(new)
    }

    // ====== animations (return AnimSpec) ======

    /// Begin a fluent animation. Chain kind / duration / rate_func before
    /// passing to `Scene.play(*specs)`.
    fn animate(&self) -> PyAnimationSpec {
        PyAnimationSpec::new(self.id)
    }

    fn shift_anim(&self, dx: f64, dy: f64) -> PyAnimationSpec {
        PyAnimationSpec::from_kind(
            self.id,
            gaanim_api::anim::AnimationType::TranslateBy {
                delta: gaanim_core::glam::DVec3::new(dx, dy, 0.0),
            },
        )
    }

    fn translate_to_anim(&self, x: f64, y: f64) -> PyAnimationSpec {
        PyAnimationSpec::from_kind(
            self.id,
            gaanim_api::anim::AnimationType::TranslateTo {
                to: gaanim_core::glam::DVec3::new(x, y, 0.0),
            },
        )
    }

    fn scale_anim(&self, factor: f64) -> PyAnimationSpec {
        PyAnimationSpec::from_kind(
            self.id,
            gaanim_api::anim::AnimationType::ScaleUniform { factor },
        )
    }

    fn rotate_anim(&self, radians: f64) -> PyAnimationSpec {
        PyAnimationSpec::from_kind(
            self.id,
            gaanim_api::anim::AnimationType::RotateBy {
                angle_radians: radians,
            },
        )
    }

    fn fade_in_anim(&self) -> PyAnimationSpec {
        PyAnimationSpec::from_kind(self.id, gaanim_api::anim::AnimationType::FadeIn)
    }

    fn fade_out_anim(&self) -> PyAnimationSpec {
        PyAnimationSpec::from_kind(self.id, gaanim_api::anim::AnimationType::FadeOut)
    }

    fn fade_to_anim(&self, opacity: f32) -> PyAnimationSpec {
        PyAnimationSpec::from_kind(
            self.id,
            gaanim_api::anim::AnimationType::FadeTo { to: opacity },
        )
    }

    fn fill_color_anim(&self, color: &PyColor) -> PyAnimationSpec {
        PyAnimationSpec::from_kind(
            self.id,
            gaanim_api::anim::AnimationType::FillColorTo { to: color.0 },
        )
    }

    fn stroke_color_anim(&self, color: &PyColor) -> PyAnimationSpec {
        PyAnimationSpec::from_kind(
            self.id,
            gaanim_api::anim::AnimationType::StrokeColorTo { to: color.0 },
        )
    }

    /// Begin a Write (pen-stroke draw) animation. Chain `.smooth()` /
    /// `.linear()` / `.spring()` to choose a rate function, then pass
    /// to `Scene.play(*specs)`.
    #[pyo3(signature = (duration=1.0, stroke_width=None))]
    fn write(&self, duration: f64, stroke_width: Option<f64>) -> PyAnimationSpec {
        use gaanim_api::builder::MobjectRef;
        let builder = MobjectRef { id: self.id }.write_with_stroke_width(duration, stroke_width);
        PyAnimationSpec::from_builder(builder)
    }

    /// Progressive draw animation in parallel (without character/element stagger).
    #[pyo3(signature = (duration=1.0, stroke_width=None))]
    fn create(&self, duration: f64, stroke_width: Option<f64>) -> PyAnimationSpec {
        use gaanim_api::builder::MobjectRef;
        let builder = MobjectRef { id: self.id }.create_with_stroke_width(duration, stroke_width);
        PyAnimationSpec::from_builder(builder)
    }

    /// Progressive erasure of the Mobject's path(s) and fill in parallel.
    #[pyo3(signature = (duration=1.0, stroke_width=None))]
    fn uncreate(&self, duration: f64, stroke_width: Option<f64>) -> PyAnimationSpec {
        use gaanim_api::builder::MobjectRef;
        let builder = MobjectRef { id: self.id }.uncreate_with_stroke_width(duration, stroke_width);
        PyAnimationSpec::from_builder(builder)
    }

    /// Staggered sequential erasure of the Mobject's path(s) and fill in reverse order.
    #[pyo3(signature = (duration=1.0, stroke_width=None))]
    fn unwrite(&self, duration: f64, stroke_width: Option<f64>) -> PyAnimationSpec {
        use gaanim_api::builder::MobjectRef;
        let builder = MobjectRef { id: self.id }.unwrite_with_stroke_width(duration, stroke_width);
        PyAnimationSpec::from_builder(builder)
    }

    /// Scale up from 0.0 to original size centered at current local position.
    fn grow_from_center(&self) -> PyAnimationSpec {
        use gaanim_api::builder::MobjectRef;
        let builder = MobjectRef { id: self.id }.grow_from_center();
        PyAnimationSpec::from_builder(builder)
    }

    /// Scale down from current size to 0.0 centered at current local position.
    fn shrink_to_center(&self) -> PyAnimationSpec {
        use gaanim_api::builder::MobjectRef;
        let builder = MobjectRef { id: self.id }.shrink_to_center();
        PyAnimationSpec::from_builder(builder)
    }

    /// Scale up from 0.0 and rotate 360 degrees concurrently.
    fn spin_in_from_nothing(&self) -> PyAnimationSpec {
        use gaanim_api::builder::MobjectRef;
        let builder = MobjectRef { id: self.id }.spin_in_from_nothing();
        PyAnimationSpec::from_builder(builder)
    }

    /// Temporarily scale up and highlight with custom parameters before returning to baseline.
    #[pyo3(signature = (color=None, scale_factor=1.25))]
    fn indicate(&self, color: Option<&PyColor>, scale_factor: f64) -> PyAnimationSpec {
        use gaanim_api::builder::MobjectRef;
        let builder = MobjectRef { id: self.id }
            .indicate_with_color_and_scale(color.map(|c| c.0), scale_factor);
        PyAnimationSpec::from_builder(builder)
    }

    /// Fade transform: fade out this mobject while fading in another.
    fn fade_transform(&self, target: &PyMobject) -> PyAnimationSpec {
        use gaanim_api::builder::MobjectRef;
        let builder = MobjectRef { id: self.id }.fade_transform(target.id);
        PyAnimationSpec::from_builder(builder)
    }

    /// Oscillating horizontal wiggle.
    fn wiggle(&self) -> PyAnimationSpec {
        use gaanim_api::builder::MobjectRef;
        let builder = MobjectRef { id: self.id }.wiggle();
        PyAnimationSpec::from_builder(builder)
    }

    /// Scale from zero at a specific anchor point.
    fn grow_from_point(&self, px: f64, py: f64) -> PyAnimationSpec {
        use gaanim_api::builder::MobjectRef;
        let builder = MobjectRef { id: self.id }.grow_from_point(px, py);
        PyAnimationSpec::from_builder(builder)
    }

    /// Scale from zero at a specific edge.
    fn grow_from_edge(&self, direction: &str) -> PyAnimationSpec {
        use gaanim_api::builder::MobjectRef;
        let builder = MobjectRef { id: self.id }.grow_from_edge(direction);
        PyAnimationSpec::from_builder(builder)
    }

    /// Draw outline first, then fill in.
    fn draw_border_then_fill(&self) -> PyAnimationSpec {
        use gaanim_api::builder::MobjectRef;
        let builder = MobjectRef { id: self.id }.draw_border_then_fill();
        PyAnimationSpec::from_builder(builder)
    }

    /// Flash radiant lines outward.
    #[pyo3(signature = (color=None, n_lines=12, radius=100.0))]
    fn flash(&self, color: Option<&PyColor>, n_lines: u32, radius: f64) -> PyAnimationSpec {
        use gaanim_api::builder::MobjectRef;
        let builder =
            MobjectRef { id: self.id }.flash(color.map(|c| c.0), n_lines, radius);
        PyAnimationSpec::from_builder(builder)
    }

    /// Highlight with a circumscribing shape that grows and fades.
    #[pyo3(signature = (color=None))]
    fn circumscribe(&self, color: Option<&PyColor>) -> PyAnimationSpec {
        use gaanim_api::builder::MobjectRef;
        let builder =
            MobjectRef { id: self.id }.circumscribe(color.map(|c| c.0));
        PyAnimationSpec::from_builder(builder)
    }
}

fn direction_from_str(s: &str) -> Option<LayoutDirection> {
    Some(match s {
        "up" => LayoutDirection::Up,
        "down" => LayoutDirection::Down,
        "left" => LayoutDirection::Left,
        "right" => LayoutDirection::Right,
        _ => return None,
    })
}
