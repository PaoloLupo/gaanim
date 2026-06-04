use gaanim_api::prelude::LayoutDirection;
use gaanim_core::peniko;
use gaanim_core::ObjectId;
use gaanim_core::kurbo;
use gaanim_math::{Bounds3D, SpatialTransform};
use gaanim_layout::Anchor;
use pyo3::prelude::*;
use std::sync::{Arc, Mutex};

use crate::animation::PyAnimationSpec;
use crate::color::PyColor;
use crate::id::PyObjectId;

#[derive(Clone, Debug)]
pub enum PythonPositioningOp {
    At { target: gaanim_core::glam::DVec3, anchor: gaanim_layout::Anchor },
    ToEdge { direction: gaanim_layout::Direction, buff: f64 },
    ToCorner { corner: gaanim_layout::Anchor, buff: f64 },
    AlignTo { reference: ObjectId, target_anchor: gaanim_layout::Anchor, ref_anchor: gaanim_layout::Anchor },
    NextTo { reference: ObjectId, direction: gaanim_layout::Direction, spacing: f64, aligned_edge: gaanim_layout::Anchor },
}

#[derive(Clone, Debug)]
pub enum PythonGroupLayoutOp {
    Arrange { direction: gaanim_layout::Direction, spacing: f64 },
    ArrangeInGrid { rows: Option<usize>, cols: Option<usize>, h_spacing: f64, v_spacing: f64 },
    VStack { spacing: f64 },
    HStack { spacing: f64 },
}

/// Visual properties shared by all mobject kinds.
#[derive(Clone, Debug)]
pub struct CommonSpec {
    pub fill: Option<peniko::Color>,
    pub stroke: Option<(peniko::Color, f64)>,
    pub z_index: i32,
    pub opacity: f32,
    pub transform: SpatialTransform,
    pub next_to: Option<(ObjectId, LayoutDirection, f64)>,
    pub positioning_ops: Vec<PythonPositioningOp>,
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
    TangentLine {
        common: CommonSpec,
        curve: Vec<(f64, f64)>,
        t: f64,
        length: f64,
    },
    NumberPlane {
        common: CommonSpec,
        x_range: (f64, f64, f64),
        y_range: (f64, f64, f64),
        axis_stroke: f64,
        grid_stroke: f64,
    },
    BooleanResult {
        common: CommonSpec,
        contours: Vec<Vec<[f64; 2]>>,
    },
    Text { common: CommonSpec, content: String, role: TextRoleKind },
    Equation { common: CommonSpec, formula: String },
    DecimalNumber {
        common: CommonSpec,
        signal_id: ObjectId,
        num_decimals: usize,
        prefix: String,
        suffix: String,
        font_family: String,
        font_size: f64,
    },
    Group {
        common: CommonSpec,
        children: Vec<(ObjectId, Arc<Mutex<MobjectSpec>>, u64)>,
        layout_op: Option<PythonGroupLayoutOp>,
    },
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
    pub fn common(&self) -> &CommonSpec {
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
            | Self::TangentLine { common, .. }
            | Self::NumberPlane { common, .. }
            | Self::BooleanResult { common, .. }
            | Self::Text { common, .. }
            | Self::Equation { common, .. }
            | Self::DecimalNumber { common, .. }
            | Self::Group { common, .. } => common,
        }
    }

    pub fn common_mut(&mut self) -> &mut CommonSpec {
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
            | Self::TangentLine { common, .. }
            | Self::NumberPlane { common, .. }
            | Self::BooleanResult { common, .. }
            | Self::Text { common, .. }
            | Self::Equation { common, .. }
            | Self::DecimalNumber { common, .. }
            | Self::Group { common, .. } => common,
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
            Self::TangentLine { .. } => "tangent_line",
            Self::NumberPlane { .. } => "number_plane",
            Self::BooleanResult { .. } => "boolean_result",
            Self::Text { .. } => "text",
            Self::Equation { .. } => "equation",
            Self::DecimalNumber { .. } => "decimal_number",
            Self::Group { .. } => "group",
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
    fn transform_mut(&mut self) -> &mut SpatialTransform { &mut self.common_mut().transform }
}

impl MobjectSpec {
    /// Reconstruct the path of this mobject as a list of polylines (one per
    /// outer ring; holes are interleaved using even-odd ordering). The
    /// reconstruction runs in **world space** by applying the stored
    /// `common.transform`, so callers (notably boolean operations) see the
    /// geometry exactly where it will appear in the rendered scene.
    pub fn to_contours(&self) -> Vec<Vec<[f64; 2]>> {
        let affine = self.common().transform.to_affine_2d();
        let mut out: Vec<Vec<[f64; 2]>> = Vec::new();
        let push_circle = |cx: f64, cy: f64, r: f64, out: &mut Vec<Vec<[f64; 2]>>| {
            let steps = 64;
            let mut ring = Vec::with_capacity(steps + 1);
            for i in 0..=steps {
                let a = i as f64 * 2.0 * std::f64::consts::PI / steps as f64;
                let p = affine * kurbo::Point::new(cx + r * a.cos(), cy + r * a.sin());
                ring.push([p.x, p.y]);
            }
            out.push(ring);
        };
        let push_rect = |cx: f64, cy: f64, w: f64, h: f64, out: &mut Vec<Vec<[f64; 2]>>| {
            let corners = [
                (cx - w / 2.0, cy - h / 2.0),
                (cx + w / 2.0, cy - h / 2.0),
                (cx + w / 2.0, cy + h / 2.0),
                (cx - w / 2.0, cy + h / 2.0),
            ];
            let mut ring = Vec::with_capacity(corners.len());
            for (x, y) in corners {
                let p = affine * kurbo::Point::new(x, y);
                ring.push([p.x, p.y]);
            }
            out.push(ring);
        };
        let push_transformed_ring = |pts: &[(f64, f64)], out: &mut Vec<Vec<[f64; 2]>>| {
            let mut ring = Vec::with_capacity(pts.len());
            for (x, y) in pts {
                let p = affine * kurbo::Point::new(*x, *y);
                ring.push([p.x, p.y]);
            }
            out.push(ring);
        };
        match self {
            Self::Circle { radius, .. } => push_circle(0.0, 0.0, *radius, &mut out),
            Self::Square { side, .. } => push_rect(0.0, 0.0, *side, *side, &mut out),
            Self::Rectangle { width, height, .. } => push_rect(0.0, 0.0, *width, *height, &mut out),
            Self::RoundedRect { width, height, .. } => push_rect(0.0, 0.0, *width, *height, &mut out),
            Self::Polygon { points, .. } => push_transformed_ring(points, &mut out),
            Self::Dot { radius, .. } => push_circle(0.0, 0.0, *radius, &mut out),
            Self::Star { n_points, outer_radius, inner_radius, .. } => {
                let mut pts = Vec::new();
                let total = (*n_points as usize) * 2;
                for i in 0..total {
                    let a = i as f64 * std::f64::consts::PI / *n_points as f64;
                    let r = if i % 2 == 0 { *outer_radius } else { *inner_radius };
                    pts.push((r * a.cos(), r * a.sin()));
                }
                push_transformed_ring(&pts, &mut out);
            }
            Self::Ellipse { rx, ry, .. } => {
                let steps = 64;
                let mut pts = Vec::with_capacity(steps + 1);
                for i in 0..=steps {
                    let a = i as f64 * 2.0 * std::f64::consts::PI / steps as f64;
                    pts.push((rx * a.cos(), ry * a.sin()));
                }
                push_transformed_ring(&pts, &mut out);
            }
            Self::Checkmark { size, .. } => {
                let pts = [(0.0, 0.0), (*size * 0.5, *size * 0.5), (*size, 0.0)];
                push_transformed_ring(&pts, &mut out);
            }
            Self::RegularPolygon { n_sides, radius, .. } => {
                let mut pts = Vec::new();
                for i in 0..*n_sides {
                    let a = i as f64 * 2.0 * std::f64::consts::PI / *n_sides as f64;
                    pts.push((radius * a.cos(), radius * a.sin()));
                }
                push_transformed_ring(&pts, &mut out);
            }
            Self::Line { start, end, .. } => {
                let pts = [(start.0, start.1), (end.0, end.1)];
                push_transformed_ring(&pts, &mut out);
            }
            Self::Arrow { start, end, .. } => {
                let dx = end.0 - start.0;
                let dy = end.1 - start.1;
                let len = (dx * dx + dy * dy).sqrt().max(0.001);
                let ux = dx / len;
                let uy = dy / len;
                let h = 2.0;
                let pts = [
                    (start.0 - uy * h, start.1 + ux * h),
                    (end.0 - uy * h, end.1 + ux * h),
                    (end.0 + uy * h, end.1 - ux * h),
                    (start.0 + uy * h, start.1 - ux * h),
                ];
                push_transformed_ring(&pts, &mut out);
            }
            Self::DoubleArrow { start, end, .. } => {
                let dx = end.0 - start.0;
                let dy = end.1 - start.1;
                let len = (dx * dx + dy * dy).sqrt().max(0.001);
                let ux = dx / len;
                let uy = dy / len;
                let h = 2.0;
                let pts = [
                    (start.0 - uy * h, start.1 + ux * h),
                    (end.0 - uy * h, end.1 + ux * h),
                    (end.0 + uy * h, end.1 - ux * h),
                    (start.0 + uy * h, start.1 - ux * h),
                ];
                push_transformed_ring(&pts, &mut out);
            }
            Self::DashedLine { start, end, .. } => {
                let pts = [(start.0, start.1), (end.0, end.1)];
                push_transformed_ring(&pts, &mut out);
            }
            Self::Arc { center, rx, ry, start_angle, sweep_angle, .. } => {
                let steps = ((sweep_angle.abs() * 30.0).ceil() as u32).max(8);
                let mut pts = Vec::with_capacity(steps as usize + 1);
                for i in 0..=steps {
                    let t = i as f64 / steps as f64;
                    let a = start_angle + sweep_angle * t;
                    pts.push((center.0 + rx * a.cos(), center.1 + ry * a.sin()));
                }
                push_transformed_ring(&pts, &mut out);
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
                let mut pts = Vec::with_capacity(steps as usize + 1);
                for i in 0..=steps {
                    let t = i as f64 / steps as f64;
                    let a = sa + sweep * t;
                    pts.push((cx + radius * a.cos(), cy + radius * a.sin()));
                }
                push_transformed_ring(&pts, &mut out);
            }
            Self::Sector { center, radius, start_angle, sweep_angle, .. } => {
                let mut pts = vec![(center.0, center.1)];
                let steps = ((sweep_angle.abs() * 30.0).ceil() as u32).max(8);
                for i in 0..=steps {
                    let t = i as f64 / steps as f64;
                    let a = start_angle + sweep_angle * t;
                    pts.push((center.0 + radius * a.cos(), center.1 + radius * a.sin()));
                }
                push_transformed_ring(&pts, &mut out);
            }
            Self::Annulus { outer_radius, inner_radius, .. } => {
                let steps = 64;
                let mut outer = Vec::with_capacity(steps + 1);
                for i in 0..=steps {
                    let a = i as f64 * 2.0 * std::f64::consts::PI / steps as f64;
                    outer.push((outer_radius * a.cos(), outer_radius * a.sin()));
                }
                push_transformed_ring(&outer, &mut out);
                let mut inner = Vec::with_capacity(steps + 1);
                for i in 0..=steps {
                    let a = i as f64 * 2.0 * std::f64::consts::PI / steps as f64;
                    inner.push((inner_radius * a.cos(), inner_radius * a.sin()));
                }
                push_transformed_ring(&inner, &mut out);
            }
            Self::SurroundingRectangle { width, height, .. } => push_rect(0.0, 0.0, *width, *height, &mut out),
            Self::BackgroundRectangle { width, height, .. } => push_rect(0.0, 0.0, *width, *height, &mut out),
            Self::Cross { size, .. } => {
                let h = size * 0.5;
                let d1 = [(-h, -h), (h, h)];
                let d2 = [(h, -h), (-h, h)];
                push_transformed_ring(&d1, &mut out);
                push_transformed_ring(&d2, &mut out);
            }
            Self::RightAngle { arm_length, .. } => {
                let pts = [(0.0, 0.0), (*arm_length, 0.0), (0.0, 0.0), (0.0, *arm_length)];
                push_transformed_ring(&pts, &mut out);
            }
            Self::TangentLine { curve, t, length, .. } => {
                // Reconstruct the tangent line as a 2-point contour for
                // boolean operations. Reuse the same arc-length
                // sampling as the primitive.
                if curve.len() >= 2 {
                    let pts: Vec<kurbo::Point> = curve
                        .iter()
                        .map(|(x, y)| kurbo::Point::new(*x, *y))
                        .collect();
                    if let Some(bundle) =
                        gaanim_objects::primitives::tangent_line(ObjectId::from_raw(0), &pts, *t, *length)
                    {
                        // Extract the line's start/end from the bundle
                        let line_path = bundle.path.0;
                        let pts2: Vec<kurbo::Point> = line_path
                            .elements()
                            .iter()
                            .filter_map(|e| match e {
                                kurbo::PathEl::MoveTo(p) | kurbo::PathEl::LineTo(p) => Some(*p),
                                _ => None,
                            })
                            .collect();
                        let ring: Vec<(f64, f64)> =
                            pts2.iter().map(|p| (p.x, p.y)).collect();
                        push_transformed_ring(&ring, &mut out);
                    }
                }
            }
            Self::NumberPlane { .. } => {
                // NumberPlane has many thin grid lines; for boolean ops
                // we treat it as empty to avoid generating hundreds of
                // zero-area contours.
            }
            Self::BooleanResult { contours, .. } => {
                for c in contours {
                    out.push(c.clone());
                }
            }
            Self::Text { .. } | Self::Equation { .. } | Self::DecimalNumber { .. } | Self::Group { .. } => {
                // Text/equation/decimal_number/group geometry cannot be reconstructed at build time
                // because the actual layout is computed at replay time.
            }
        }
        out
    }

    pub fn get_local_bounds(&self) -> Bounds3D {
        match self {
            Self::Circle { radius, .. } => {
                Bounds3D::new_2d(-radius, -radius, *radius, *radius)
            }
            Self::Rectangle { width, height, .. } => {
                Bounds3D::new_2d(-width * 0.5, -height * 0.5, *width * 0.5, *height * 0.5)
            }
            Self::RoundedRect { width, height, .. } => {
                Bounds3D::new_2d(-width * 0.5, -height * 0.5, *width * 0.5, *height * 0.5)
            }
            Self::Square { side, .. } => {
                Bounds3D::new_2d(-side * 0.5, -side * 0.5, *side * 0.5, *side * 0.5)
            }
            Self::Text { content, .. } => {
                let w = content.len() as f64 * 15.0;
                Bounds3D::new_2d(-w * 0.5, -12.0, w * 0.5, 12.0)
            }
            Self::Equation { formula, .. } => {
                let w = formula.len() as f64 * 20.0;
                Bounds3D::new_2d(-w * 0.5, -15.0, w * 0.5, 15.0)
            }
            Self::Group { children, .. } => {
                let mut union_bounds = Bounds3D::default();
                let mut first = true;
                for (_, child_spec_mutex, _) in children {
                    if let Ok(child_spec) = child_spec_mutex.lock() {
                        let child_local_bounds = child_spec.get_local_bounds();
                        let child_world_bounds = gaanim_layout::transform_bounds(child_local_bounds, &child_spec.common().transform);
                        if first {
                            union_bounds = child_world_bounds;
                            first = false;
                        } else {
                            union_bounds = union_bounds.union(&child_world_bounds);
                        }
                    }
                }
                union_bounds
            }
            _ => Bounds3D::default(),
        }
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

    fn __getitem__(&self, index: usize) -> PyResult<PyMobject> {
        let spec_value = lock_spec!(self.spec);
        if let MobjectSpec::Group { children, .. } = &*spec_value {
            if let Some((id, spec, creation_order)) = children.get(index) {
                Ok(PyMobject {
                    id: *id,
                    spec: spec.clone(),
                    creation_order: *creation_order,
                })
            } else {
                Err(pyo3::exceptions::PyIndexError::new_err("Index out of range"))
            }
        } else {
            Err(pyo3::exceptions::PyTypeError::new_err("Mobject is not a Group"))
        }
    }

    fn __len__(&self) -> PyResult<usize> {
        let spec_value = lock_spec!(self.spec);
        if let MobjectSpec::Group { children, .. } = &*spec_value {
            Ok(children.len())
        } else {
            Ok(0)
        }
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

    /// Set absolute 2D position (default: centers visual bounds at (x, y)).
    #[pyo3(signature = (x, y, anchor="center"))]
    fn at(&self, x: f64, y: f64, anchor: &str) -> PyResult<Self> {
        let anc = anchor_from_str(anchor).ok_or_else(|| {
            pyo3::exceptions::PyValueError::new_err(format!("unknown anchor: {}", anchor))
        })?;
        let new = self.clone();
        let mut s = lock_spec!(new.spec);
        s.common_mut().positioning_ops.push(PythonPositioningOp::At {
            target: gaanim_core::glam::DVec3::new(x, y, 0.0),
            anchor: anc,
        });
        drop(s);
        Ok(new)
    }

    /// Align target anchor (defaults to center) to reference's anchor.
    #[pyo3(signature = (reference, anchor="center"))]
    fn move_to(&self, reference: &PyMobject, anchor: &str) -> PyResult<Self> {
        let anc = anchor_from_str(anchor).ok_or_else(|| {
            pyo3::exceptions::PyValueError::new_err(format!("unknown anchor: {}", anchor))
        })?;
        let new = self.clone();
        let mut s = lock_spec!(new.spec);
        s.common_mut().positioning_ops.push(PythonPositioningOp::AlignTo {
            reference: reference.id,
            target_anchor: Anchor::Center,
            ref_anchor: anc,
        });
        drop(s);
        Ok(new)
    }

    /// Place this mobject adjacent to a reference mobject in a layout direction.
    #[pyo3(signature = (reference, direction, spacing=10.0, aligned_edge="center"))]
    fn next_to(&self, reference: &PyMobject, direction: &str, spacing: f64, aligned_edge: &str) -> PyResult<Self> {
        let dir = direction_from_str(direction).ok_or_else(|| {
            pyo3::exceptions::PyValueError::new_err(format!("unknown direction: {}", direction))
        })?;
        let align = anchor_from_str(aligned_edge).ok_or_else(|| {
            pyo3::exceptions::PyValueError::new_err(format!("unknown aligned_edge: {}", aligned_edge))
        })?;
        let new = self.clone();
        let mut s = lock_spec!(new.spec);
        s.common_mut().positioning_ops.push(PythonPositioningOp::NextTo {
            reference: reference.id,
            direction: dir,
            spacing,
            aligned_edge: align,
        });
        drop(s);
        Ok(new)
    }

    /// Position at screen edge with buffer spacing.
    #[pyo3(signature = (direction, buff=0.5))]
    fn to_edge(&self, direction: &str, buff: f64) -> PyResult<Self> {
        let dir = direction_from_str(direction).ok_or_else(|| {
            pyo3::exceptions::PyValueError::new_err(format!("unknown direction: {}", direction))
        })?;
        let new = self.clone();
        let mut s = lock_spec!(new.spec);
        s.common_mut().positioning_ops.push(PythonPositioningOp::ToEdge {
            direction: dir,
            buff,
        });
        drop(s);
        Ok(new)
    }

    /// Position at screen corner with buffer spacing.
    #[pyo3(signature = (corner, buff=0.5))]
    fn to_corner(&self, corner: &str, buff: f64) -> PyResult<Self> {
        let anc = anchor_from_str(corner).ok_or_else(|| {
            pyo3::exceptions::PyValueError::new_err(format!("unknown corner: {}", corner))
        })?;
        let new = self.clone();
        let mut s = lock_spec!(new.spec);
        s.common_mut().positioning_ops.push(PythonPositioningOp::ToCorner {
            corner: anc,
            buff,
        });
        drop(s);
        Ok(new)
    }

    /// Arrange group children linearly.
    #[pyo3(signature = (direction, spacing=10.0))]
    fn arrange(&self, direction: &str, spacing: f64) -> PyResult<Self> {
        let dir = direction_from_str(direction).ok_or_else(|| {
            pyo3::exceptions::PyValueError::new_err(format!("unknown direction: {}", direction))
        })?;
        let new = self.clone();
        let mut s = lock_spec!(new.spec);
        if let MobjectSpec::Group { layout_op, .. } = &mut *s {
            *layout_op = Some(PythonGroupLayoutOp::Arrange { direction: dir, spacing });
            drop(s);
            Ok(new)
        } else {
            drop(s);
            Err(pyo3::exceptions::PyTypeError::new_err("Mobject is not a Group"))
        }
    }

    /// Arrange group children in a grid.
    #[pyo3(signature = (rows=None, cols=None, h_spacing=10.0, v_spacing=10.0))]
    fn arrange_in_grid(&self, rows: Option<usize>, cols: Option<usize>, h_spacing: f64, v_spacing: f64) -> PyResult<Self> {
        let new = self.clone();
        let mut s = lock_spec!(new.spec);
        if let MobjectSpec::Group { layout_op, .. } = &mut *s {
            *layout_op = Some(PythonGroupLayoutOp::ArrangeInGrid { rows, cols, h_spacing, v_spacing });
            drop(s);
            Ok(new)
        } else {
            drop(s);
            Err(pyo3::exceptions::PyTypeError::new_err("Mobject is not a Group"))
        }
    }

    /// Arrange group children in a vertical stack.
    #[pyo3(signature = (spacing=10.0))]
    fn vstack(&self, spacing: f64) -> PyResult<Self> {
        let new = self.clone();
        let mut s = lock_spec!(new.spec);
        if let MobjectSpec::Group { layout_op, .. } = &mut *s {
            *layout_op = Some(PythonGroupLayoutOp::VStack { spacing });
            drop(s);
            Ok(new)
        } else {
            drop(s);
            Err(pyo3::exceptions::PyTypeError::new_err("Mobject is not a Group"))
        }
    }

    /// Arrange group children in a horizontal stack.
    #[pyo3(signature = (spacing=10.0))]
    fn hstack(&self, spacing: f64) -> PyResult<Self> {
        let new = self.clone();
        let mut s = lock_spec!(new.spec);
        if let MobjectSpec::Group { layout_op, .. } = &mut *s {
            *layout_op = Some(PythonGroupLayoutOp::HStack { spacing });
            drop(s);
            Ok(new)
        } else {
            drop(s);
            Err(pyo3::exceptions::PyTypeError::new_err("Mobject is not a Group"))
        }
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

    // ====== layout queries ======

    fn get_center(&self) -> PyResult<(f64, f64)> {
        let spec = lock_spec!(self.spec);
        let common = spec.common();
        let bounds = spec.get_local_bounds();
        let world_bounds = gaanim_layout::transform_bounds(bounds, &common.transform);
        let center = world_bounds.center();
        Ok((center.x, center.y))
    }

    fn get_corner(&self, corner: &str) -> PyResult<(f64, f64)> {
        let anc = anchor_from_str(corner).ok_or_else(|| {
            pyo3::exceptions::PyValueError::new_err(format!("unknown corner: {}", corner))
        })?;
        let spec = lock_spec!(self.spec);
        let common = spec.common();
        let bounds = spec.get_local_bounds();
        let world_bounds = gaanim_layout::transform_bounds(bounds, &common.transform);
        let pt = anc.get_point(&world_bounds);
        Ok((pt.x, pt.y))
    }

    fn get_edge_center(&self, direction: &str) -> PyResult<(f64, f64)> {
        let dir = direction_from_str(direction).ok_or_else(|| {
            pyo3::exceptions::PyValueError::new_err(format!("unknown direction: {}", direction))
        })?;
        let spec = lock_spec!(self.spec);
        let common = spec.common();
        let bounds = spec.get_local_bounds();
        let world_bounds = gaanim_layout::transform_bounds(bounds, &common.transform);
        let pt = dir.to_anchor().get_point(&world_bounds);
        Ok((pt.x, pt.y))
    }

    fn get_width(&self) -> PyResult<f64> {
        let spec = lock_spec!(self.spec);
        let common = spec.common();
        let bounds = spec.get_local_bounds();
        let world_bounds = gaanim_layout::transform_bounds(bounds, &common.transform);
        Ok(world_bounds.size().x)
    }

    fn get_height(&self) -> PyResult<f64> {
        let spec = lock_spec!(self.spec);
        let common = spec.common();
        let bounds = spec.get_local_bounds();
        let world_bounds = gaanim_layout::transform_bounds(bounds, &common.transform);
        Ok(world_bounds.size().y)
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

    /// Move this Mobject along a Bézier path defined by a list of
    /// waypoints. Each waypoint is a `(x, y)` tuple. Adjacent points
    /// are connected by line segments, so polylines, polygons, and
    /// star shapes are all valid trajectories.
    fn move_along_path(
        &self,
        waypoints: Vec<(f64, f64)>,
        duration: f64,
    ) -> PyResult<PyAnimationSpec> {
        use gaanim_api::builder::MobjectRef;
        if waypoints.is_empty() {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "move_along_path requires at least one waypoint",
            ));
        }
        let mut path = kurbo::BezPath::new();
        let (x0, y0) = waypoints[0];
        path.move_to(kurbo::Point::new(x0, y0));
        for &(x, y) in &waypoints[1..] {
            path.line_to(kurbo::Point::new(x, y));
        }
        let mut builder = MobjectRef { id: self.id }.move_along_path(path);
        if duration > 0.0 {
            builder = builder.duration(duration);
        }
        Ok(PyAnimationSpec::from_builder(builder))
    }

    /// Specialized arrow draw: traces the outline and finishes with a
    /// scale "punch" that emphasizes the arrowhead's arrival at the
    /// end of the trajectory. Intended for `Arrow` mobjects.
    fn grow_arrow(&self, duration: f64) -> PyAnimationSpec {
        use gaanim_api::builder::MobjectRef;
        let mut builder = MobjectRef { id: self.id }.grow_arrow();
        if duration > 0.0 {
            builder = builder.duration(duration);
        }
        PyAnimationSpec::from_builder(builder)
    }

    /// ShowPassingFlash animation shortcut.
    #[pyo3(signature = (duration=1.0, time_width=0.2))]
    fn show_passing_flash(&self, duration: f64, time_width: f64) -> PyAnimationSpec {
        use gaanim_api::builder::MobjectRef;
        let builder = MobjectRef { id: self.id }.show_passing_flash(duration, time_width);
        PyAnimationSpec::from_builder(builder)
    }

    fn add_bob_updater(&self, scene: &crate::scene::PyScene, amplitude: f64, frequency: f64) -> PyResult<()> {
        let mut inner = match scene.inner.lock() {
            Ok(guard) => guard,
            Err(_) => return Err(pyo3::exceptions::PyRuntimeError::new_err("Scene mutex is poisoned")),
        };
        inner.ops.push(crate::scene::DeferredOp::AddUpdater {
            target: self.id,
            updater_type: "bob".to_string(),
            params: vec![amplitude, frequency],
            follow_target: None,
        });
        Ok(())
    }

    fn add_rotate_updater(&self, scene: &crate::scene::PyScene, speed: f64) -> PyResult<()> {
        let mut inner = match scene.inner.lock() {
            Ok(guard) => guard,
            Err(_) => return Err(pyo3::exceptions::PyRuntimeError::new_err("Scene mutex is poisoned")),
        };
        inner.ops.push(crate::scene::DeferredOp::AddUpdater {
            target: self.id,
            updater_type: "rotate".to_string(),
            params: vec![speed],
            follow_target: None,
        });
        Ok(())
    }

    fn add_orbit_updater(&self, scene: &crate::scene::PyScene, cx: f64, cy: f64, radius: f64, speed: f64) -> PyResult<()> {
        let mut inner = match scene.inner.lock() {
            Ok(guard) => guard,
            Err(_) => return Err(pyo3::exceptions::PyRuntimeError::new_err("Scene mutex is poisoned")),
        };
        inner.ops.push(crate::scene::DeferredOp::AddUpdater {
            target: self.id,
            updater_type: "orbit".to_string(),
            params: vec![cx, cy, radius, speed],
            follow_target: None,
        });
        Ok(())
    }

    fn add_pulse_updater(&self, scene: &crate::scene::PyScene, min_scale: f64, max_scale: f64, frequency: f64) -> PyResult<()> {
        let mut inner = match scene.inner.lock() {
            Ok(guard) => guard,
            Err(_) => return Err(pyo3::exceptions::PyRuntimeError::new_err("Scene mutex is poisoned")),
        };
        inner.ops.push(crate::scene::DeferredOp::AddUpdater {
            target: self.id,
            updater_type: "pulse".to_string(),
            params: vec![min_scale, max_scale, frequency],
            follow_target: None,
        });
        Ok(())
    }

    #[pyo3(signature = (scene, target, ox=0.0, oy=0.0, smoothing=0.0))]
    fn add_follow_updater(
        &self,
        scene: &crate::scene::PyScene,
        target: &PyMobject,
        ox: f64,
        oy: f64,
        smoothing: f64,
    ) -> PyResult<()> {
        let mut inner = match scene.inner.lock() {
            Ok(guard) => guard,
            Err(_) => return Err(pyo3::exceptions::PyRuntimeError::new_err("Scene mutex is poisoned")),
        };
        inner.ops.push(crate::scene::DeferredOp::AddUpdater {
            target: self.id,
            updater_type: "follow".to_string(),
            params: vec![ox, oy, smoothing],
            follow_target: Some(target.id),
        });
        Ok(())
    }

    fn remove_updater(&self, scene: &crate::scene::PyScene) -> PyResult<()> {
        let mut inner = match scene.inner.lock() {
            Ok(guard) => guard,
            Err(_) => return Err(pyo3::exceptions::PyRuntimeError::new_err("Scene mutex is poisoned")),
        };
        inner.ops.push(crate::scene::DeferredOp::RemoveUpdater {
            target: self.id,
        });
        Ok(())
    }
}

fn direction_from_str(s: &str) -> Option<gaanim_layout::Direction> {
    match s.to_lowercase().as_str() {
        "up" | "top" => Some(gaanim_layout::Direction::Up),
        "down" | "bottom" => Some(gaanim_layout::Direction::Down),
        "left" => Some(gaanim_layout::Direction::Left),
        "right" => Some(gaanim_layout::Direction::Right),
        "up_left" | "top_left" => Some(gaanim_layout::Direction::UpLeft),
        "up_right" | "top_right" => Some(gaanim_layout::Direction::UpRight),
        "down_left" | "bottom_left" => Some(gaanim_layout::Direction::DownLeft),
        "down_right" | "bottom_right" => Some(gaanim_layout::Direction::DownRight),
        _ => None,
    }
}

fn anchor_from_str(s: &str) -> Option<gaanim_layout::Anchor> {
    match s.to_lowercase().as_str() {
        "center" => Some(gaanim_layout::Anchor::Center),
        "top" | "up" => Some(gaanim_layout::Anchor::Top),
        "bottom" | "down" => Some(gaanim_layout::Anchor::Bottom),
        "left" => Some(gaanim_layout::Anchor::Left),
        "right" => Some(gaanim_layout::Anchor::Right),
        "top_left" | "up_left" => Some(gaanim_layout::Anchor::TopLeft),
        "top_right" | "up_right" => Some(gaanim_layout::Anchor::TopRight),
        "bottom_left" | "down_left" => Some(gaanim_layout::Anchor::BottomLeft),
        "bottom_right" | "down_right" => Some(gaanim_layout::Anchor::BottomRight),
        _ => None,
    }
}



#[cfg(test)]
mod tests {
    use super::*;
    use gaanim_core::glam::DVec3;

    fn make_circle(translation: (f64, f64)) -> MobjectSpec {
        let t = SpatialTransform {
            translation: DVec3::new(translation.0, translation.1, 0.0),
            ..Default::default()
        };
        MobjectSpec::Circle {
            common: CommonSpec {
                fill: None,
                stroke: None,
                z_index: 0,
                opacity: 1.0,
                transform: t,
                next_to: None,
                positioning_ops: Vec::new(),
            },
            radius: 80.0,
        }
    }

    #[test]
    fn circle_at_origin_has_centered_contours() {
        let spec = make_circle((0.0, 0.0));
        let contours = spec.to_contours();
        assert_eq!(contours.len(), 1);
        let min_x = contours[0].iter().map(|p| p[0]).fold(f64::INFINITY, f64::min);
        let max_x = contours[0].iter().map(|p| p[0]).fold(f64::NEG_INFINITY, f64::max);
        let min_y = contours[0].iter().map(|p| p[1]).fold(f64::INFINITY, f64::min);
        let max_y = contours[0].iter().map(|p| p[1]).fold(f64::NEG_INFINITY, f64::max);
        let cx = (min_x + max_x) * 0.5;
        let cy = (min_y + max_y) * 0.5;
        assert!(cx.abs() < 0.1, "expected center near 0, got {}", cx);
        assert!(cy.abs() < 0.1, "expected center near 0, got {}", cy);
    }

    #[test]
    fn circle_translated_offset_has_offcenter_contours() {
        let spec = make_circle((-40.0, 0.0));
        let contours = spec.to_contours();
        assert_eq!(contours.len(), 1);
        let min_x = contours[0].iter().map(|p| p[0]).fold(f64::INFINITY, f64::min);
        let max_x = contours[0].iter().map(|p| p[0]).fold(f64::NEG_INFINITY, f64::max);
        let min_y = contours[0].iter().map(|p| p[1]).fold(f64::INFINITY, f64::min);
        let max_y = contours[0].iter().map(|p| p[1]).fold(f64::NEG_INFINITY, f64::max);
        let cx = (min_x + max_x) * 0.5;
        let cy = (min_y + max_y) * 0.5;
        assert!((cx - (-40.0)).abs() < 0.1, "expected center near -40, got {}", cx);
        assert!(cy.abs() < 0.1, "expected center near 0, got {}", cy);
    }
}
