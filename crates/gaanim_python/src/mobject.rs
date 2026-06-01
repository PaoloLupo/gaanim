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
