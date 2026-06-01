use pyo3::prelude::*;
use gaanim_api::prelude::LayoutDirection;
use gaanim_core::peniko;
use gaanim_core::ObjectId;
use gaanim_math::SpatialTransform;
use std::sync::{Arc, Mutex};

use crate::animation::PyAnimationSpec;
use crate::color::PyColor;
use crate::id::PyObjectId;

/// What kind of mobject will be spawned at replay time. The configuration
/// (`fill`, `stroke`, `z_index`, `transform`, `next_to`, …) is attached and
/// replayed during the Bevy `Startup` system.
#[derive(Clone, Debug)]
pub enum MobjectSpec {
    Circle {
        radius: f64,
        fill: Option<peniko::Color>,
        stroke: Option<(peniko::Color, f64)>,
        z_index: i32,
        opacity: f32,
        transform: SpatialTransform,
        next_to: Option<(ObjectId, LayoutDirection, f64)>,
    },
    Rectangle {
        width: f64,
        height: f64,
        fill: Option<peniko::Color>,
        stroke: Option<(peniko::Color, f64)>,
        z_index: i32,
        opacity: f32,
        transform: SpatialTransform,
        next_to: Option<(ObjectId, LayoutDirection, f64)>,
    },
    RoundedRect {
        width: f64,
        height: f64,
        radius: f64,
        fill: Option<peniko::Color>,
        stroke: Option<(peniko::Color, f64)>,
        z_index: i32,
        opacity: f32,
        transform: SpatialTransform,
        next_to: Option<(ObjectId, LayoutDirection, f64)>,
    },
    Line {
        start: (f64, f64),
        end: (f64, f64),
        stroke: Option<(peniko::Color, f64)>,
        z_index: i32,
        opacity: f32,
        transform: SpatialTransform,
        next_to: Option<(ObjectId, LayoutDirection, f64)>,
    },
    Polygon {
        points: Vec<(f64, f64)>,
        fill: Option<peniko::Color>,
        stroke: Option<(peniko::Color, f64)>,
        z_index: i32,
        opacity: f32,
        transform: SpatialTransform,
        next_to: Option<(ObjectId, LayoutDirection, f64)>,
    },
    Star {
        n_points: u32,
        outer_radius: f64,
        inner_radius: f64,
        fill: Option<peniko::Color>,
        stroke: Option<(peniko::Color, f64)>,
        z_index: i32,
        opacity: f32,
        transform: SpatialTransform,
        next_to: Option<(ObjectId, LayoutDirection, f64)>,
    },
    Ellipse {
        rx: f64,
        ry: f64,
        fill: Option<peniko::Color>,
        stroke: Option<(peniko::Color, f64)>,
        z_index: i32,
        opacity: f32,
        transform: SpatialTransform,
        next_to: Option<(ObjectId, LayoutDirection, f64)>,
    },
    Dot {
        radius: f64,
        fill: Option<peniko::Color>,
        stroke: Option<(peniko::Color, f64)>,
        z_index: i32,
        opacity: f32,
        transform: SpatialTransform,
        next_to: Option<(ObjectId, LayoutDirection, f64)>,
    },
    Square {
        side: f64,
        fill: Option<peniko::Color>,
        stroke: Option<(peniko::Color, f64)>,
        z_index: i32,
        opacity: f32,
        transform: SpatialTransform,
        next_to: Option<(ObjectId, LayoutDirection, f64)>,
    },
    Checkmark {
        size: f64,
        stroke: Option<(peniko::Color, f64)>,
        z_index: i32,
        opacity: f32,
        transform: SpatialTransform,
        next_to: Option<(ObjectId, LayoutDirection, f64)>,
    },
    Arrow {
        start: (f64, f64),
        end: (f64, f64),
        stroke: Option<(peniko::Color, f64)>,
        fill: Option<peniko::Color>,
        z_index: i32,
        opacity: f32,
        transform: SpatialTransform,
        next_to: Option<(ObjectId, LayoutDirection, f64)>,
    },
    RegularPolygon {
        n_sides: u32,
        radius: f64,
        fill: Option<peniko::Color>,
        stroke: Option<(peniko::Color, f64)>,
        z_index: i32,
        opacity: f32,
        transform: SpatialTransform,
        next_to: Option<(ObjectId, LayoutDirection, f64)>,
    },
    Text {
        content: String,
        role: TextRoleKind,
        fill: Option<peniko::Color>,
        z_index: i32,
        opacity: f32,
        transform: SpatialTransform,
        next_to: Option<(ObjectId, LayoutDirection, f64)>,
    },
    Equation {
        formula: String,
        fill: Option<peniko::Color>,
        z_index: i32,
        opacity: f32,
        transform: SpatialTransform,
        next_to: Option<(ObjectId, LayoutDirection, f64)>,
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
            Self::Text { .. } => "text",
            Self::Equation { .. } => "equation",
        }
    }

    pub fn fill(&self) -> Option<peniko::Color> {
        match self {
            Self::Circle { fill, .. }
            | Self::Rectangle { fill, .. }
            | Self::RoundedRect { fill, .. }
            | Self::Polygon { fill, .. }
            | Self::Star { fill, .. }
            | Self::Ellipse { fill, .. }
            | Self::Dot { fill, .. }
            | Self::Square { fill, .. }
            | Self::RegularPolygon { fill, .. }
            | Self::Arrow { fill, .. }
            | Self::Text { fill, .. }
            | Self::Equation { fill, .. } => *fill,
            Self::Line { .. } | Self::Checkmark { .. } => None,
        }
    }

    pub fn stroke(&self) -> Option<(peniko::Color, f64)> {
        match self {
            Self::Circle { stroke, .. }
            | Self::Rectangle { stroke, .. }
            | Self::RoundedRect { stroke, .. }
            | Self::Line { stroke, .. }
            | Self::Polygon { stroke, .. }
            | Self::Star { stroke, .. }
            | Self::Ellipse { stroke, .. }
            | Self::Dot { stroke, .. }
            | Self::Square { stroke, .. }
            | Self::Checkmark { stroke, .. }
            | Self::Arrow { stroke, .. }
            | Self::RegularPolygon { stroke, .. } => *stroke,
            Self::Text { .. } | Self::Equation { .. } => None,
        }
    }

    pub fn opacity(&self) -> f32 {
        match self {
            Self::Circle { opacity, .. }
            | Self::Rectangle { opacity, .. }
            | Self::RoundedRect { opacity, .. }
            | Self::Line { opacity, .. }
            | Self::Polygon { opacity, .. }
            | Self::Star { opacity, .. }
            | Self::Ellipse { opacity, .. }
            | Self::Dot { opacity, .. }
            | Self::Square { opacity, .. }
            | Self::Checkmark { opacity, .. }
            | Self::Arrow { opacity, .. }
            | Self::RegularPolygon { opacity, .. }
            | Self::Text { opacity, .. }
            | Self::Equation { opacity, .. } => *opacity,
        }
    }

    pub fn z_index(&self) -> i32 {
        match self {
            Self::Circle { z_index, .. }
            | Self::Rectangle { z_index, .. }
            | Self::RoundedRect { z_index, .. }
            | Self::Line { z_index, .. }
            | Self::Polygon { z_index, .. }
            | Self::Star { z_index, .. }
            | Self::Ellipse { z_index, .. }
            | Self::Dot { z_index, .. }
            | Self::Square { z_index, .. }
            | Self::Checkmark { z_index, .. }
            | Self::Arrow { z_index, .. }
            | Self::RegularPolygon { z_index, .. }
            | Self::Text { z_index, .. }
            | Self::Equation { z_index, .. } => *z_index,
        }
    }

    pub fn transform(&self) -> SpatialTransform {
        match self {
            Self::Circle { transform, .. }
            | Self::Rectangle { transform, .. }
            | Self::RoundedRect { transform, .. }
            | Self::Line { transform, .. }
            | Self::Polygon { transform, .. }
            | Self::Star { transform, .. }
            | Self::Ellipse { transform, .. }
            | Self::Dot { transform, .. }
            | Self::Square { transform, .. }
            | Self::Checkmark { transform, .. }
            | Self::Arrow { transform, .. }
            | Self::RegularPolygon { transform, .. }
            | Self::Text { transform, .. }
            | Self::Equation { transform, .. } => *transform,
        }
    }

    pub fn next_to(&self) -> Option<(ObjectId, LayoutDirection, f64)> {
        match self {
            Self::Circle { next_to, .. }
            | Self::Rectangle { next_to, .. }
            | Self::RoundedRect { next_to, .. }
            | Self::Line { next_to, .. }
            | Self::Polygon { next_to, .. }
            | Self::Star { next_to, .. }
            | Self::Ellipse { next_to, .. }
            | Self::Dot { next_to, .. }
            | Self::Square { next_to, .. }
            | Self::Checkmark { next_to, .. }
            | Self::Arrow { next_to, .. }
            | Self::RegularPolygon { next_to, .. }
            | Self::Text { next_to, .. }
            | Self::Equation { next_to, .. } => *next_to,
        }
    }

    fn set_fill(&mut self, color: Option<peniko::Color>) {
        match self {
            Self::Circle { fill, .. }
            | Self::Rectangle { fill, .. }
            | Self::RoundedRect { fill, .. }
            | Self::Polygon { fill, .. }
            | Self::Star { fill, .. }
            | Self::Ellipse { fill, .. }
            | Self::Dot { fill, .. }
            | Self::Square { fill, .. }
            | Self::RegularPolygon { fill, .. }
            | Self::Arrow { fill, .. }
            | Self::Text { fill, .. }
            | Self::Equation { fill, .. } => *fill = color,
            Self::Line { .. } | Self::Checkmark { .. } => {}
        }
    }

    fn set_stroke(&mut self, stroke: Option<(peniko::Color, f64)>) {
        match self {
            Self::Circle { stroke: s, .. }
            | Self::Rectangle { stroke: s, .. }
            | Self::RoundedRect { stroke: s, .. }
            | Self::Line { stroke: s, .. }
            | Self::Polygon { stroke: s, .. }
            | Self::Star { stroke: s, .. }
            | Self::Ellipse { stroke: s, .. }
            | Self::Dot { stroke: s, .. }
            | Self::Square { stroke: s, .. }
            | Self::Checkmark { stroke: s, .. }
            | Self::Arrow { stroke: s, .. }
            | Self::RegularPolygon { stroke: s, .. } => *s = stroke,
            Self::Text { .. } | Self::Equation { .. } => {}
        }
    }

    fn set_opacity(&mut self, opacity: f32) {
        match self {
            Self::Circle { opacity: o, .. }
            | Self::Rectangle { opacity: o, .. }
            | Self::RoundedRect { opacity: o, .. }
            | Self::Line { opacity: o, .. }
            | Self::Polygon { opacity: o, .. }
            | Self::Star { opacity: o, .. }
            | Self::Ellipse { opacity: o, .. }
            | Self::Dot { opacity: o, .. }
            | Self::Square { opacity: o, .. }
            | Self::Checkmark { opacity: o, .. }
            | Self::Arrow { opacity: o, .. }
            | Self::RegularPolygon { opacity: o, .. }
            | Self::Text { opacity: o, .. }
            | Self::Equation { opacity: o, .. } => *o = opacity,
        }
    }

    fn set_z_index(&mut self, z: i32) {
        match self {
            Self::Circle { z_index, .. }
            | Self::Rectangle { z_index, .. }
            | Self::RoundedRect { z_index, .. }
            | Self::Line { z_index, .. }
            | Self::Polygon { z_index, .. }
            | Self::Star { z_index, .. }
            | Self::Ellipse { z_index, .. }
            | Self::Dot { z_index, .. }
            | Self::Square { z_index, .. }
            | Self::Checkmark { z_index, .. }
            | Self::Arrow { z_index, .. }
            | Self::RegularPolygon { z_index, .. }
            | Self::Text { z_index, .. }
            | Self::Equation { z_index, .. } => *z_index = z,
        }
    }

    fn set_transform(&mut self, transform: SpatialTransform) {
        match self {
            Self::Circle { transform: t, .. }
            | Self::Rectangle { transform: t, .. }
            | Self::RoundedRect { transform: t, .. }
            | Self::Line { transform: t, .. }
            | Self::Polygon { transform: t, .. }
            | Self::Star { transform: t, .. }
            | Self::Ellipse { transform: t, .. }
            | Self::Dot { transform: t, .. }
            | Self::Square { transform: t, .. }
            | Self::Checkmark { transform: t, .. }
            | Self::Arrow { transform: t, .. }
            | Self::RegularPolygon { transform: t, .. }
            | Self::Text { transform: t, .. }
            | Self::Equation { transform: t, .. } => *t = transform,
        }
    }

    fn transform_mut(&mut self) -> &mut SpatialTransform {
        match self {
            Self::Circle { transform, .. }
            | Self::Rectangle { transform, .. }
            | Self::RoundedRect { transform, .. }
            | Self::Line { transform, .. }
            | Self::Polygon { transform, .. }
            | Self::Star { transform, .. }
            | Self::Ellipse { transform, .. }
            | Self::Dot { transform, .. }
            | Self::Square { transform, .. }
            | Self::Checkmark { transform, .. }
            | Self::Arrow { transform, .. }
            | Self::RegularPolygon { transform, .. }
            | Self::Text { transform, .. }
            | Self::Equation { transform, .. } => transform,
        }
    }

    fn set_next_to(&mut self, hint: Option<(ObjectId, LayoutDirection, f64)>) {
        match self {
            Self::Circle { next_to, .. }
            | Self::Rectangle { next_to, .. }
            | Self::RoundedRect { next_to, .. }
            | Self::Line { next_to, .. }
            | Self::Polygon { next_to, .. }
            | Self::Star { next_to, .. }
            | Self::Ellipse { next_to, .. }
            | Self::Dot { next_to, .. }
            | Self::Square { next_to, .. }
            | Self::Checkmark { next_to, .. }
            | Self::Arrow { next_to, .. }
            | Self::RegularPolygon { next_to, .. }
            | Self::Text { next_to, .. }
            | Self::Equation { next_to, .. } => *next_to = hint,
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

#[pymethods]
impl PyMobject {
    #[getter]
    fn id(&self) -> PyObjectId {
        PyObjectId(self.id)
    }

    fn __repr__(&self) -> String {
        let kind = self.spec.lock().unwrap().kind_name().to_string();
        format!(
            "Mobject(ObjectId({}v{}), kind={}, creation_order={})",
            self.id.index(),
            self.id.generation(),
            kind,
            self.creation_order,
        )
    }

    // ====== instant configuration (return new Mobject, mutate shared spec) ======

    fn fill(&self, color: &PyColor) -> Self {
        let new = self.clone();
        new.spec.lock().unwrap().set_fill(Some(color.0));
        new
    }

    fn no_fill(&self) -> Self {
        let new = self.clone();
        new.spec.lock().unwrap().set_fill(None);
        new
    }

    fn stroke(&self, color: &PyColor, width: f64) -> Self {
        let new = self.clone();
        new.spec.lock().unwrap().set_stroke(Some((color.0, width)));
        new
    }

    fn no_stroke(&self) -> Self {
        let new = self.clone();
        new.spec.lock().unwrap().set_stroke(None);
        new
    }

    fn opacity(&self, opacity: f32) -> Self {
        let new = self.clone();
        new.spec.lock().unwrap().set_opacity(opacity);
        new
    }

    fn z_index(&self, z: i32) -> Self {
        let new = self.clone();
        new.spec.lock().unwrap().set_z_index(z);
        new
    }

    /// Set absolute 2D position (applied at spawn time as the initial transform).
    fn at(&self, x: f64, y: f64) -> Self {
        let new = self.clone();
        new.spec
            .lock().unwrap()
            .set_transform(SpatialTransform::new_2d(x, y));
        new
    }

    /// Add to existing 2D position.
    fn shift(&self, dx: f64, dy: f64) -> Self {
        let new = self.clone();
        let mut s = new.spec.lock().unwrap();
        let t = s.transform_mut();
        *t = t.shift_2d(dx, dy);
        drop(s);
        new
    }

    fn scale(&self, factor: f64) -> Self {
        let new = self.clone();
        let mut s = new.spec.lock().unwrap();
        let t = s.transform_mut();
        *t = t.scale_uniform(factor);
        drop(s);
        new
    }

    fn rotate(&self, radians: f64) -> Self {
        let new = self.clone();
        let mut s = new.spec.lock().unwrap();
        let t = s.transform_mut();
        *t = t.with_rotation_2d(radians);
        drop(s);
        new
    }

    /// Place this mobject adjacent to a reference mobject in a layout direction.
    /// Resolved at spawn time using the reference's state in the scene.
    #[pyo3(signature = (reference, direction, spacing=10.0))]
    fn next_to(
        &self,
        reference: &PyMobject,
        direction: &str,
        spacing: f64,
    ) -> PyResult<Self> {
        let dir = direction_from_str(direction).ok_or_else(|| {
            pyo3::exceptions::PyValueError::new_err(format!(
                "unknown direction: {} (use 'up', 'down', 'left', 'right')",
                direction
            ))
        })?;
        let new = self.clone();
        new.spec
            .lock().unwrap()
            .set_next_to(Some((reference.id, dir, spacing)));
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
            gaanim_api::anim::AnimationType::RotateBy { angle_radians: radians },
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

