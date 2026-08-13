//! Thin PyDrawable wrapper over gaanim_api DrawableHandle.

use pyo3::exceptions::{PyKeyError, PyTypeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::PyTuple;

use crate::brush::PyPaint;
use crate::color::PyColor;
use crate::py3d::PyMaterial3D;
use crate::pylayout::{expression_for, PyAnchor, PyDirection, PyLayoutExpression};
use crate::pystyle::PyStrokeStyle;
use crate::updater::PyUpdater;

#[pyclass(name = "AnchorPoint", module = "gaanim_core", frozen, from_py_object)]
#[derive(Clone, Copy, Debug)]
pub struct PyAnchorPoint(pub gaanim_api::canvas::AnchorPoint);

#[pyclass(name = "Anim", module = "gaanim_core", from_py_object)]
#[derive(Clone, Debug)]
pub struct PyCanvasAnim {
    pub inner: gaanim_api::canvas::Anim,
}

#[pymethods]
impl PyCanvasAnim {
    fn fill(&self, color: PyColor) -> Self {
        Self {
            inner: self.inner.clone().fill(color.0),
        }
    }

    fn color(&self, color: PyColor) -> Self {
        Self {
            inner: self.inner.clone().color(color.0),
        }
    }

    fn stroke(&self, color: PyColor, width: f64) -> PyResult<Self> {
        if self.inner.property_target_is_text_selection() {
            return Err(PyTypeError::new_err(
                "TextSelection.animate() supports only fill/color and opacity targets",
            ));
        }
        if self.inner.property_target_is_primitive_3d() {
            return Err(PyTypeError::new_err(
                "stroke() is only available for vector drawables; animate material() or color() on Primitive3D",
            ));
        }
        Ok(Self {
            inner: self.inner.clone().stroke(color.0, width),
        })
    }

    fn stroke_color(&self, color: PyColor) -> PyResult<Self> {
        if self.inner.property_target_is_text_selection() {
            return Err(PyTypeError::new_err(
                "TextSelection.animate() supports only fill/color and opacity targets",
            ));
        }
        if self.inner.property_target_is_primitive_3d() {
            return Err(PyTypeError::new_err(
                "stroke_color() is only available for vector drawables",
            ));
        }
        Ok(Self {
            inner: self.inner.clone().stroke_color(color.0),
        })
    }

    fn material(&self, material: PyMaterial3D) -> PyResult<Self> {
        if self.inner.property_target_is_text_selection() {
            return Err(PyTypeError::new_err(
                "TextSelection.animate() supports only fill/color and opacity targets",
            ));
        }
        if !self.inner.property_target_is_primitive_3d() {
            return Err(PyTypeError::new_err(
                "material() is only available for native Primitive3D drawables",
            ));
        }
        Ok(Self {
            inner: self.inner.clone().material(material.0),
        })
    }

    fn opacity(&self, value: f32) -> Self {
        Self {
            inner: self.inner.clone().opacity(value),
        }
    }

    fn r#move(&self, dx: f64, dy: f64) -> PyResult<Self> {
        if self.inner.property_target_is_text_selection() {
            return Err(PyTypeError::new_err(
                "TextSelection.animate() supports only fill/color and opacity targets",
            ));
        }
        if !self.inner.property_position_is_free() {
            return Err(crate::LayoutOwnershipError::new_err(
                "layout owns this drawable's translation; animate the LayoutItem offset instead",
            ));
        }
        Ok(Self {
            inner: self.inner.clone().r#move(dx, dy),
        })
    }

    #[pyo3(signature = (x, y, anchor=None))]
    fn move_to(&self, x: f64, y: f64, anchor: Option<&PyAnchor>) -> PyResult<Self> {
        if self.inner.property_target_is_text_selection() {
            return Err(PyTypeError::new_err(
                "TextSelection.animate() supports only fill/color and opacity targets",
            ));
        }
        if !self.inner.property_position_is_free() {
            return Err(crate::LayoutOwnershipError::new_err(
                "layout owns this drawable's translation; animate the LayoutItem offset instead",
            ));
        }
        Ok(Self {
            inner: self.inner.clone().move_to_anchor(
                x,
                y,
                anchor.map(|anchor| anchor.0).unwrap_or_default(),
            ),
        })
    }

    fn move_3d(&self, dx: f64, dy: f64, dz: f64) -> PyResult<Self> {
        if self.inner.property_target_is_text_selection() {
            return Err(PyTypeError::new_err(
                "TextSelection.animate() supports only fill/color and opacity targets",
            ));
        }
        if !self.inner.property_position_is_free() {
            return Err(crate::LayoutOwnershipError::new_err(
                "layout owns this drawable's translation; animate the LayoutItem offset instead",
            ));
        }
        Ok(Self {
            inner: self.inner.clone().move_3d(dx, dy, dz),
        })
    }

    fn move_to_3d(&self, x: f64, y: f64, z: f64) -> PyResult<Self> {
        if self.inner.property_target_is_text_selection() {
            return Err(PyTypeError::new_err(
                "TextSelection.animate() supports only fill/color and opacity targets",
            ));
        }
        if !self.inner.property_position_is_free() {
            return Err(crate::LayoutOwnershipError::new_err(
                "layout owns this drawable's translation; animate the LayoutItem offset instead",
            ));
        }
        Ok(Self {
            inner: self.inner.clone().move_to_3d(x, y, z),
        })
    }

    fn scale(&self, factor: f64) -> PyResult<Self> {
        if self.inner.property_target_is_text_selection() {
            return Err(PyTypeError::new_err(
                "TextSelection.animate() supports only fill/color and opacity targets",
            ));
        }
        Ok(Self {
            inner: self.inner.clone().scale(factor),
        })
    }

    fn scale_to(&self, factor: f64) -> PyResult<Self> {
        if self.inner.property_target_is_text_selection() {
            return Err(PyTypeError::new_err(
                "TextSelection.animate() supports only fill/color and opacity targets",
            ));
        }
        Ok(Self {
            inner: self.inner.clone().scale_to(factor),
        })
    }

    fn scale_to_3d(&self, x: f64, y: f64, z: f64) -> PyResult<Self> {
        if self.inner.property_target_is_text_selection() {
            return Err(PyTypeError::new_err(
                "TextSelection.animate() supports only fill/color and opacity targets",
            ));
        }
        Ok(Self {
            inner: self.inner.clone().scale_to_3d(x, y, z),
        })
    }

    fn rotate(&self, radians: f64) -> PyResult<Self> {
        if self.inner.property_target_is_text_selection() {
            return Err(PyTypeError::new_err(
                "TextSelection.animate() supports only fill/color and opacity targets",
            ));
        }
        Ok(Self {
            inner: self.inner.clone().rotate(radians),
        })
    }

    fn rotate_to(&self, radians: f64) -> PyResult<Self> {
        if self.inner.property_target_is_text_selection() {
            return Err(PyTypeError::new_err(
                "TextSelection.animate() supports only fill/color and opacity targets",
            ));
        }
        Ok(Self {
            inner: self.inner.clone().rotate_to(radians),
        })
    }

    fn rotate_by_3d(&self, axis: &str, radians: f64) -> PyResult<Self> {
        if self.inner.property_target_is_text_selection() {
            return Err(PyTypeError::new_err(
                "TextSelection.animate() supports only fill/color and opacity targets",
            ));
        }
        self.inner
            .clone()
            .rotate_by_3d(axis, radians)
            .map(|inner| Self { inner })
            .map_err(PyValueError::new_err)
    }

    fn rotate_to_3d(&self, x: f64, y: f64, z: f64) -> PyResult<Self> {
        if self.inner.property_target_is_text_selection() {
            return Err(PyTypeError::new_err(
                "TextSelection.animate() supports only fill/color and opacity targets",
            ));
        }
        Ok(Self {
            inner: self.inner.clone().rotate_to_3d(x, y, z),
        })
    }

    fn duration(&self, d: f64) -> Self {
        Self {
            inner: self.inner.clone().duration(d),
        }
    }

    fn ease(&self, name: &str) -> Self {
        Self {
            inner: self.inner.clone().ease(name),
        }
    }

    fn rate(&self, name: &str) -> Self {
        self.ease(name)
    }

    fn delay(&self, sec: f64) -> Self {
        Self {
            inner: self.inner.clone().delay(sec),
        }
    }

    fn steps(&self, n: u32) -> Self {
        Self {
            inner: self.inner.clone().steps(n),
        }
    }

    fn spring(&self) -> Self {
        Self {
            inner: self.inner.clone().spring(),
        }
    }

    fn smooth(&self) -> Self {
        Self {
            inner: self.inner.clone().smooth(),
        }
    }

    fn linear(&self) -> Self {
        Self {
            inner: self.inner.clone().linear(),
        }
    }

    fn lag_ratio(&self, value: f64) -> Self {
        Self {
            inner: self.inner.clone().lag_ratio(value),
        }
    }

    fn stroke_width(&self, value: f64) -> Self {
        Self {
            inner: self.inner.clone().stroke_width(value),
        }
    }

    fn with_pen_tip(&self) -> Self {
        Self {
            inner: self.inner.clone().with_pen_tip(),
        }
    }

    fn pivot(&self, x: f64, y: f64) -> Self {
        Self {
            inner: self.inner.clone().pivot(x, y),
        }
    }

    fn about_point(&self, x: f64, y: f64) -> Self {
        self.pivot(x, y)
    }
}

#[pyclass(name = "Drawable", module = "gaanim_core", subclass, from_py_object)]
#[derive(Clone)]
pub struct PyDrawable(pub gaanim_api::canvas::DrawableHandle);

impl PyDrawable {
    fn require_free_position(&self, operation: &str) -> PyResult<()> {
        if self.0.layout_owner().is_some() {
            Err(crate::LayoutOwnershipError::new_err(format!(
                "layout owns this drawable's translation; use scene.item(..., offset=...) or layout.configure_item(...). Operation: {operation}"
            )))
        } else {
            Ok(())
        }
    }
}

#[pymethods]
impl PyDrawable {
    #[pyo3(signature = (anchor=None, *, offset=(0.0, 0.0)))]
    fn anchor_point(
        &self,
        anchor: Option<PyAnchor>,
        offset: (f64, f64),
    ) -> PyResult<PyAnchorPoint> {
        if !offset.0.is_finite() || !offset.1.is_finite() {
            return Err(PyValueError::new_err("anchor offset must be finite"));
        }
        Ok(PyAnchorPoint(self.0.anchor_point(
            anchor.map(|value| value.0).unwrap_or_default(),
            gaanim_core::glam::DVec3::new(offset.0, offset.1, 0.0),
        )))
    }

    #[getter]
    fn left(&self) -> PyLayoutExpression {
        expression_for(&self.0, gaanim_layout::LayoutAttribute::Left)
    }

    #[getter]
    fn right(&self) -> PyLayoutExpression {
        expression_for(&self.0, gaanim_layout::LayoutAttribute::Right)
    }

    #[getter]
    fn top(&self) -> PyLayoutExpression {
        expression_for(&self.0, gaanim_layout::LayoutAttribute::Top)
    }

    #[getter]
    fn bottom(&self) -> PyLayoutExpression {
        expression_for(&self.0, gaanim_layout::LayoutAttribute::Bottom)
    }

    #[getter]
    fn center_x(&self) -> PyLayoutExpression {
        expression_for(&self.0, gaanim_layout::LayoutAttribute::CenterX)
    }

    #[getter]
    fn center_y(&self) -> PyLayoutExpression {
        expression_for(&self.0, gaanim_layout::LayoutAttribute::CenterY)
    }

    #[getter]
    fn width(&self) -> PyLayoutExpression {
        expression_for(&self.0, gaanim_layout::LayoutAttribute::Width)
    }

    #[getter]
    fn height(&self) -> PyLayoutExpression {
        expression_for(&self.0, gaanim_layout::LayoutAttribute::Height)
    }

    /// Return a named source group or path from an imported SVG or glTF.
    fn part(&self, id: &str) -> PyResult<Self> {
        if id.is_empty() {
            return Err(PyKeyError::new_err("part selector must not be empty"));
        }
        match self.0.part(id) {
            Ok(part) => Ok(Self(part)),
            Err(gaanim_api::canvas::SvgPartError::NotSvg) => Err(PyValueError::new_err(
                "this drawable has no named SVG or glTF parts",
            )),
            Err(error @ gaanim_api::canvas::SvgPartError::Unknown { .. }) => {
                Err(PyKeyError::new_err(error.to_string()))
            }
        }
    }

    fn parts(&self, py: Python<'_>) -> PyResult<Py<PyTuple>> {
        Ok(PyTuple::new(py, self.0.parts())?.unbind())
    }

    fn animations(&self, py: Python<'_>) -> PyResult<Py<PyTuple>> {
        Ok(PyTuple::new(py, self.0.animations())?.unbind())
    }

    /// Start a typed compound property animation.
    fn animate(&self) -> PyCanvasAnim {
        PyCanvasAnim {
            inner: self.0.animate(),
        }
    }

    #[pyo3(signature = (name, *, duration=None, speed=1.0, r#loop=false, reverse=false, transition=0.0, start_time=0.0))]
    fn animation(
        &self,
        name: &str,
        duration: Option<f64>,
        speed: f64,
        r#loop: bool,
        reverse: bool,
        transition: f64,
        start_time: f64,
    ) -> PyResult<PyCanvasAnim> {
        self.0
            .animation(
                name, duration, speed, r#loop, reverse, transition, start_time,
            )
            .map(|inner| PyCanvasAnim { inner })
            .map_err(|error| match error {
                gaanim_api::canvas::GltfAnimationError::Unknown { .. } => {
                    PyKeyError::new_err(error.to_string())
                }
                _ => PyValueError::new_err(error.to_string()),
            })
    }

    fn fill(&self, paint: PyPaint) -> Self {
        Self(self.0.clone().fill_brush(paint.0))
    }
    fn no_fill(&self) -> Self {
        Self(self.0.clone().no_fill())
    }
    fn stroke(&self, paint: PyPaint, width: f64) -> Self {
        Self(self.0.clone().stroke_brush(paint.0, width))
    }
    /// Apply cap, join, miter, and dash geometry from a reusable StrokeStyle.
    fn stroke_style(&self, style: PyStrokeStyle) -> PyResult<Self> {
        let brush = match style.0.paint {
            gaanim_api::canvas::ThemePaint::Color(color) => {
                gaanim_core::peniko::Brush::Solid(color)
            }
            gaanim_api::canvas::ThemePaint::Brush(brush) => brush,
            gaanim_api::canvas::ThemePaint::Named(name) => {
                use std::str::FromStr;
                gaanim_core::peniko::Color::from_str(&name)
                    .map(gaanim_core::peniko::Brush::Solid)
                    .map_err(|_| {
                        PyValueError::new_err(
                            "individual stroke_style paint must be a literal CSS color; theme tokens are resolved inside Theme.styles",
                        )
                    })?
            }
        };
        Ok(Self(self.0.clone().stroke_with_style(brush, style.0.style)))
    }
    fn no_stroke(&self) -> Self {
        Self(self.0.clone().no_stroke())
    }
    /// Add a theme class; calls may be chained and later classes win.
    fn style_class(&self, name: &str) -> PyResult<Self> {
        self.0
            .clone()
            .style_class(name)
            .map(Self)
            .map_err(PyValueError::new_err)
    }
    /// Add a cached soft outer glow.
    #[pyo3(signature = (color, radius=16.0, intensity=1.0))]
    fn glow(&self, color: PyColor, radius: f64, intensity: f32) -> PyResult<Self> {
        if !radius.is_finite() || radius <= 0.0 {
            return Err(PyValueError::new_err("radius must be finite and positive"));
        }
        if !intensity.is_finite() || intensity <= 0.0 {
            return Err(PyValueError::new_err(
                "intensity must be finite and positive",
            ));
        }
        Ok(Self(self.0.clone().glow(color.0, radius, intensity)))
    }
    /// Apply a cached soft vector blur.
    #[pyo3(signature = (sigma=4.0))]
    fn blur(&self, sigma: f64) -> PyResult<Self> {
        if !sigma.is_finite() || sigma <= 0.0 {
            return Err(PyValueError::new_err("sigma must be finite and positive"));
        }
        Ok(Self(self.0.clone().blur(sigma)))
    }
    /// Add a cached soft shadow behind the drawable.
    #[pyo3(signature = (color, x=8.0, y=-8.0, blur=6.0))]
    fn shadow(&self, color: PyColor, x: f64, y: f64, blur: f64) -> PyResult<Self> {
        if !x.is_finite() || !y.is_finite() {
            return Err(PyValueError::new_err("shadow offset must be finite"));
        }
        if !blur.is_finite() || blur < 0.0 {
            return Err(PyValueError::new_err(
                "shadow blur must be finite and non-negative",
            ));
        }
        Ok(Self(self.0.clone().shadow(
            color.0,
            gaanim_core::glam::DVec2::new(x, y),
            blur,
        )))
    }
    /// Remove glow, blur, and shadow from the drawable.
    fn no_effects(&self) -> Self {
        Self(self.0.clone().no_effects())
    }
    /// Clip this drawable to another drawable's vector outline.
    #[pyo3(signature = (mask, rule="nonzero"))]
    fn clip(&self, mask: &PyDrawable, rule: &str) -> PyResult<Self> {
        let rule = match rule {
            "nonzero" => gaanim_core::peniko::Fill::NonZero,
            "evenodd" | "even_odd" => gaanim_core::peniko::Fill::EvenOdd,
            _ => {
                return Err(PyValueError::new_err("rule must be 'nonzero' or 'evenodd'"));
            }
        };
        Ok(Self(self.0.clone().clip(&mask.0, rule)))
    }
    /// Remove the clipping mask from this drawable.
    fn no_clip(&self) -> Self {
        Self(self.0.clone().no_clip())
    }
    fn opacity(&self, op: f32) -> Self {
        Self(self.0.clone().opacity(op))
    }
    fn z_index(&self, z: i32) -> Self {
        Self(self.0.clone().z_index(z))
    }
    #[pyo3(signature = (x, y, anchor=None))]
    fn at(&self, x: f64, y: f64, anchor: Option<&PyAnchor>) -> PyResult<Self> {
        self.require_free_position("at")?;
        Ok(Self(self.0.clone().at_anchor(
            x,
            y,
            anchor.map(|anchor| anchor.0).unwrap_or_default(),
        )))
    }
    fn at_3d(&self, x: f64, y: f64, z: f64) -> PyResult<Self> {
        self.require_free_position("at_3d")?;
        Ok(Self(self.0.clone().at_3d(x, y, z)))
    }
    fn billboard(&self) -> Self {
        Self(self.0.clone().billboard())
    }
    fn hud(&self) -> Self {
        Self(self.0.clone().hud())
    }
    fn scaled(&self, factor: f64) -> Self {
        Self(self.0.clone().scaled(factor))
    }
    fn scaled_3d(&self, x: f64, y: f64, z: f64) -> Self {
        Self(self.0.clone().scaled_3d(x, y, z))
    }
    fn rotated(&self, radians: f64) -> Self {
        Self(self.0.clone().rotated(radians))
    }
    fn rotated_3d(&self, x: f64, y: f64, z: f64) -> Self {
        Self(self.0.clone().rotated_3d(x, y, z))
    }
    fn with_pivot(&self, x: f64, y: f64) -> Self {
        Self(self.0.clone().with_pivot(x, y))
    }
    fn with_pivot_3d(&self, x: f64, y: f64, z: f64) -> Self {
        Self(self.0.clone().with_pivot_3d(x, y, z))
    }
    fn pivot(&self, x: f64, y: f64) -> Self {
        Self(self.0.clone().pivot(x, y))
    }
    fn at_anchor(&self, x: f64, y: f64, anchor: &PyAnchor) -> PyResult<Self> {
        self.require_free_position("at_anchor")?;
        Ok(Self(self.0.clone().at_anchor(x, y, anchor.0)))
    }
    #[pyo3(signature = (reference, direction, spacing=24.0, aligned_edge=None))]
    fn next_to(
        &self,
        reference: &PyDrawable,
        direction: &PyDirection,
        spacing: f64,
        aligned_edge: Option<&PyAnchor>,
    ) -> PyResult<Self> {
        self.require_free_position("next_to")?;
        let aligned_edge = aligned_edge
            .map(|anchor| anchor.0)
            .unwrap_or(gaanim_api::canvas::Anchor::Center);
        Ok(Self(self.0.clone().next_to_aligned(
            &reference.0,
            direction.0,
            spacing,
            aligned_edge,
        )))
    }
    #[pyo3(signature = (reference, target_anchor, reference_anchor=None))]
    fn align_to(
        &self,
        reference: &PyDrawable,
        target_anchor: &PyAnchor,
        reference_anchor: Option<&PyAnchor>,
    ) -> PyResult<Self> {
        self.require_free_position("align_to")?;
        let reference_anchor = reference_anchor
            .map(|anchor| anchor.0)
            .unwrap_or(target_anchor.0);
        Ok(Self(self.0.clone().align_to(
            &reference.0,
            target_anchor.0,
            reference_anchor,
        )))
    }
    #[pyo3(signature = (direction, buff=24.0))]
    fn to_edge(&self, direction: &PyDirection, buff: f64) -> PyResult<Self> {
        self.require_free_position("to_edge")?;
        Ok(Self(self.0.clone().to_edge(direction.0, buff)))
    }
    #[pyo3(signature = (corner, buff=24.0))]
    fn to_corner(&self, corner: &PyAnchor, buff: f64) -> PyResult<Self> {
        self.require_free_position("to_corner")?;
        Ok(Self(self.0.clone().to_corner(corner.0, buff)))
    }
    fn r#move(&self, dx: f64, dy: f64) -> PyResult<PyCanvasAnim> {
        self.require_free_position("move")?;
        Ok(PyCanvasAnim {
            inner: self.0.r#move(dx, dy),
        })
    }
    #[pyo3(signature = (x, y, anchor=None))]
    fn move_to(&self, x: f64, y: f64, anchor: Option<&PyAnchor>) -> PyResult<PyCanvasAnim> {
        self.require_free_position("move_to")?;
        Ok(PyCanvasAnim {
            inner: self
                .0
                .move_to_anchor(x, y, anchor.map(|anchor| anchor.0).unwrap_or_default()),
        })
    }
    fn move_3d(&self, dx: f64, dy: f64, dz: f64) -> PyCanvasAnim {
        PyCanvasAnim {
            inner: self.0.move_3d(dx, dy, dz),
        }
    }
    fn move_to_3d(&self, x: f64, y: f64, z: f64) -> PyResult<PyCanvasAnim> {
        self.require_free_position("move_to_3d")?;
        Ok(PyCanvasAnim {
            inner: self.0.move_to_3d(x, y, z),
        })
    }
    fn glide_to(&self, x: f64, y: f64) -> PyCanvasAnim {
        PyCanvasAnim {
            inner: self.0.glide_to(x, y),
        }
    }
    fn scale(&self, factor: f64) -> PyCanvasAnim {
        PyCanvasAnim {
            inner: self.0.scale(factor),
        }
    }
    fn scale_to_3d(&self, x: f64, y: f64, z: f64) -> PyCanvasAnim {
        PyCanvasAnim {
            inner: self.0.scale_to_3d(x, y, z),
        }
    }
    fn rotate(&self, rad: f64) -> PyCanvasAnim {
        PyCanvasAnim {
            inner: self.0.rotate(rad),
        }
    }
    fn rotate_by_3d(&self, axis: &str, radians: f64) -> PyResult<PyCanvasAnim> {
        self.0
            .rotate_by_3d(axis, radians)
            .map(|inner| PyCanvasAnim { inner })
            .map_err(|error| PyValueError::new_err(error.to_string()))
    }
    fn rotate_to_3d(&self, x: f64, y: f64, z: f64) -> PyCanvasAnim {
        PyCanvasAnim {
            inner: self.0.rotate_to_3d(x, y, z),
        }
    }
    #[pyo3(signature = (duration=None))]
    fn fade_in(&self, duration: Option<f64>) -> PyCanvasAnim {
        PyCanvasAnim {
            inner: self.0.fade_in(duration),
        }
    }
    /// Fade in while moving from ``direction`` (for example ``Direction.DOWN``).
    #[pyo3(signature = (direction, distance=48.0, duration=None))]
    fn fade_in_from(
        &self,
        direction: &PyDirection,
        distance: f64,
        duration: Option<f64>,
    ) -> PyResult<PyCanvasAnim> {
        if !distance.is_finite() || distance < 0.0 {
            return Err(PyValueError::new_err(
                "distance must be a finite non-negative number",
            ));
        }
        Ok(PyCanvasAnim {
            inner: self.0.fade_in_from(direction.0.clone(), distance, duration),
        })
    }
    #[pyo3(signature = (duration=None))]
    fn fade_out(&self, duration: Option<f64>) -> PyCanvasAnim {
        PyCanvasAnim {
            inner: self.0.fade_out(duration),
        }
    }
    fn fade_to(&self, alpha: f32) -> PyCanvasAnim {
        PyCanvasAnim {
            inner: self.0.fade_to(alpha),
        }
    }
    #[pyo3(signature = (duration=None))]
    fn write(&self, duration: Option<f64>) -> PyCanvasAnim {
        PyCanvasAnim {
            inner: self.0.write(duration),
        }
    }
    #[pyo3(signature = (duration=None))]
    fn create(&self, duration: Option<f64>) -> PyCanvasAnim {
        PyCanvasAnim {
            inner: self.0.create(duration),
        }
    }
    #[pyo3(signature = (duration=None))]
    fn unwrite(&self, duration: Option<f64>) -> PyCanvasAnim {
        PyCanvasAnim {
            inner: self.0.unwrite(duration),
        }
    }
    #[pyo3(signature = (duration=None))]
    fn uncreate(&self, duration: Option<f64>) -> PyCanvasAnim {
        PyCanvasAnim {
            inner: self.0.uncreate(duration),
        }
    }
    #[pyo3(signature = (duration=None))]
    fn grow_from_center(&self, duration: Option<f64>) -> PyCanvasAnim {
        PyCanvasAnim {
            inner: self.0.grow_from_center(duration),
        }
    }
    #[pyo3(signature = (duration=None))]
    fn shrink_to_center(&self, duration: Option<f64>) -> PyCanvasAnim {
        PyCanvasAnim {
            inner: self.0.shrink_to_center(duration),
        }
    }
    #[pyo3(signature = (duration=None))]
    fn spin_in_from_nothing(&self, duration: Option<f64>) -> PyCanvasAnim {
        PyCanvasAnim {
            inner: self.0.spin_in_from_nothing(duration),
        }
    }
    #[pyo3(signature = (duration=None))]
    fn draw_border_then_fill(&self, duration: Option<f64>) -> PyCanvasAnim {
        PyCanvasAnim {
            inner: self.0.draw_border_then_fill(duration),
        }
    }
    #[pyo3(signature = (duration=None))]
    fn indicate(&self, duration: Option<f64>) -> PyCanvasAnim {
        PyCanvasAnim {
            inner: self.0.indicate(duration),
        }
    }
    #[pyo3(signature = (duration=None))]
    fn wiggle(&self, duration: Option<f64>) -> PyCanvasAnim {
        PyCanvasAnim {
            inner: self.0.wiggle(duration),
        }
    }

    fn move_along_path(&self, target: &PyDrawable) -> PyCanvasAnim {
        PyCanvasAnim {
            inner: self.0.move_along_drawable(&target.0),
        }
    }

    fn move_along_drawable(&self, target: &PyDrawable) -> PyCanvasAnim {
        PyCanvasAnim {
            inner: self.0.move_along_drawable(&target.0),
        }
    }
    fn fade_transform(&self, target: &PyDrawable) -> PyCanvasAnim {
        PyCanvasAnim {
            inner: self.0.fade_transform(&target.0),
        }
    }
    fn transform(&self, target: &PyDrawable) -> PyCanvasAnim {
        PyCanvasAnim {
            inner: self.0.transform(&target.0),
        }
    }
    fn replacement_transform(&self, target: &PyDrawable) -> PyCanvasAnim {
        PyCanvasAnim {
            inner: self.0.replacement_transform(&target.0),
        }
    }

    // -- Reactive methods --

    /// Attach a preset updater that runs every frame.
    fn add_updater(&self, updater: &PyUpdater) {
        self.0.add_updater(updater.0.clone());
    }

    /// Attach a generic Python callback updater or deterministic simulation.
    ///
    /// `callback` must be a callable with signature `callback(pos, dt, elapsed) -> (x,y,z)`
    /// where `pos` is the current local `(x,y,z)` position and `dt`/`elapsed` are
    /// seconds. The callback returns the new local position.
    ///
    /// For stateful simulations, pass both `reset` and `fixed_dt`. `reset()` must
    /// restore all Python state captured by `callback`; Gaanim then replays fixed
    /// substeps after timeline seeks and during export.
    ///
    /// Example Lorenz:
    /// ```python
    /// def lorenz(pos, dt, t):
    ///     x,y,z = pos
    ///     dx = 10*(y-x); dy = x*(28-z)-y; dz = x*y - 8/3*z
    ///     # fixed integration step 0.01, repeat 3 substeps per frame for speed
    ///     for _ in range(3):
    ///         x += 0.01*dx; y += 0.01*dy; z += 0.01*dz
    ///         dx = 10*(y-x); dy = x*(28-z)-y; dz = x*y - 8/3*z
    ///     return (x,y,z)
    /// def reset_lorenz():
    ///     pass  # position is restored by Gaanim
    /// dot.add_updater_fn(lorenz, reset=reset_lorenz, fixed_dt=1/600)
    /// ```
    #[pyo3(signature = (callback, *, reset=None, fixed_dt=None))]
    fn add_updater_fn(
        &self,
        callback: Py<PyAny>,
        reset: Option<Py<PyAny>>,
        fixed_dt: Option<f64>,
    ) -> PyResult<Self> {
        if !Python::attach(|py| callback.bind(py).is_callable()) {
            return Err(PyValueError::new_err("callback must be callable"));
        }
        if let Some(reset) = reset.as_ref() {
            if !Python::attach(|py| reset.bind(py).is_callable()) {
                return Err(PyValueError::new_err("reset must be callable"));
            }
        }
        match (reset.is_some(), fixed_dt.is_some()) {
            (true, false) => {
                return Err(PyValueError::new_err(
                    "reset requires fixed_dt for deterministic replay",
                ));
            }
            (false, true) => {
                return Err(PyValueError::new_err(
                    "fixed_dt requires reset so seeks and exports can rebuild simulation state",
                ));
            }
            _ => {}
        }

        let callback_clone = callback.clone();
        let updater_fn = move |dt: f64,
                               elapsed: f64,
                               entity: gaanim_scene::prelude::Entity,
                               world: &mut gaanim_scene::prelude::World| {
            let current = world
                .get::<gaanim_math::SpatialTransform>(entity)
                .map(|t| t.translation);
            let current = match current {
                Some(p) => p,
                None => return true,
            };
            let result: PyResult<(f64, f64, f64)> = Python::attach(|py| {
                let func = callback_clone.bind(py);
                let pos = (current.x, current.y, current.z);
                // Preferred signature: callback(pos, dt, elapsed)
                let v = func.call1((pos, dt, elapsed))?;
                // Accept either (x,y,z) tuple or list
                if let Ok(tup) = v.extract::<(f64, f64, f64)>() {
                    Ok(tup)
                } else if let Ok(vec) = v.extract::<Vec<f64>>() {
                    if vec.len() == 3 {
                        Ok((vec[0], vec[1], vec[2]))
                    } else {
                        Err(PyErr::new::<pyo3::exceptions::PyTypeError, _>(
                            "callback must return (x,y,z) tuple of 3 floats",
                        ))
                    }
                } else {
                    Err(PyErr::new::<pyo3::exceptions::PyTypeError, _>(
                        "callback must return (x,y,z) tuple",
                    ))
                }
            });
            match result {
                Ok((nx, ny, nz)) => {
                    if !nx.is_finite() || !ny.is_finite() || !nz.is_finite() {
                        Python::attach(|py| {
                            PyValueError::new_err("callback must return three finite coordinates")
                                .print(py)
                        });
                        return false;
                    }
                    if let Some(mut t) = world.get_mut::<gaanim_math::SpatialTransform>(entity) {
                        t.translation = gaanim_core::glam::DVec3::new(nx, ny, nz);
                    }
                    true
                }
                Err(e) => {
                    Python::attach(|py| e.print(py));
                    false
                }
            }
        };

        let updater = if let (Some(reset), Some(fixed_dt)) = (reset, fixed_dt) {
            let reset_clone = reset.clone();
            let reset_fn =
                move |_entity: gaanim_scene::prelude::Entity,
                      _world: &mut gaanim_scene::prelude::World| {
                    match Python::attach(|py| reset_clone.bind(py).call0().map(|_| ())) {
                        Ok(()) => true,
                        Err(e) => {
                            Python::attach(|py| e.print(py));
                            false
                        }
                    }
                };
            gaanim_animation::Updater::new_simulation(updater_fn, reset_fn, fixed_dt).map_err(
                |_| PyValueError::new_err("fixed_dt must be finite and greater than zero"),
            )?
        } else {
            gaanim_animation::Updater::new(updater_fn)
        };

        self.0.add_custom_updater(updater);
        Ok(self.clone())
    }

    /// Remove any updater attached to this entity.
    fn remove_updater(&self) {
        self.0.remove_updater();
    }

    /// Copy the source entity's Y position each frame.
    fn bind_y_from(&self, source: &PyDrawable) {
        self.0.bind_y_from(&source.0);
    }

    /// Copy the source entity's X position each frame.
    fn bind_x_from(&self, source: &PyDrawable) {
        self.0.bind_x_from(&source.0);
    }

    /// Keep this drawable centered on ``source`` each frame.
    fn attach_to(&self, source: &PyDrawable) {
        self.0.attach_to(&source.0);
    }

    /// Follow ``source`` while keeping an ``(x, y)`` scene-space offset.
    fn follow_to(&self, source: &PyDrawable, offset: (f64, f64)) {
        self.0.follow_to(&source.0, offset.0, offset.1);
    }

    /// Follow any endpoint and return this drawable for fluent chaining.
    #[pyo3(signature = (source, *, offset=(0.0, 0.0), offset_space="world"))]
    fn follow(
        &self,
        source: Bound<'_, PyAny>,
        offset: (f64, f64),
        offset_space: &str,
    ) -> PyResult<Self> {
        if !offset.0.is_finite() || !offset.1.is_finite() {
            return Err(PyValueError::new_err("offset must be finite"));
        }
        let space = match offset_space {
            "world" => gaanim_animation::FollowOffsetSpace::World,
            "local" => gaanim_animation::FollowOffsetSpace::Local,
            _ => {
                return Err(PyValueError::new_err(
                    "offset_space must be 'world' or 'local'",
                ))
            }
        };
        Ok(Self(self.0.follow_endpoint(
            crate::pycanvas::resolve_endpoint(&source)?,
            gaanim_core::glam::DVec3::new(offset.0, offset.1, 0.0),
            space,
        )))
    }

    #[pyo3(signature = (source, *, ratio=1.0, phase=0.0))]
    fn bind_rotation_from(&self, source: &PyDrawable, ratio: f64, phase: f64) -> PyResult<Self> {
        if !ratio.is_finite() || !phase.is_finite() {
            return Err(PyValueError::new_err("ratio and phase must be finite"));
        }
        Ok(Self(self.0.bind_rotation_from(&source.0, ratio, phase)))
    }

    #[pyo3(signature = (source, *, axis=None, scale=1.0))]
    fn bind_translation_from_rotation(
        &self,
        source: &PyDrawable,
        axis: Option<PyDirection>,
        scale: f64,
    ) -> PyResult<Self> {
        if !scale.is_finite() {
            return Err(PyValueError::new_err("scale must be finite"));
        }
        let axis = axis
            .map(|value| value.0.to_vector())
            .unwrap_or(gaanim_core::glam::DVec3::X);
        if axis.length_squared() <= 1e-12 {
            return Err(PyValueError::new_err("axis cannot be zero"));
        }
        Ok(Self(
            self.0
                .bind_translation_from_rotation(&source.0, axis, scale),
        ))
    }

    /// Copy selected source axes each frame. ``axes`` accepts ``"x"``,
    /// ``"y"``, ``"xy"`` (the default), or ``"xyz"``.
    #[pyo3(signature = (source, axes="xy"))]
    fn bind_position_from(&self, source: &PyDrawable, axes: &str) -> PyResult<()> {
        let axes = match axes {
            "x" => gaanim_api::canvas::AxisMask::X,
            "y" => gaanim_api::canvas::AxisMask::Y,
            "xy" => gaanim_api::canvas::AxisMask::XY,
            "xyz" => gaanim_api::canvas::AxisMask::XYZ,
            _ => {
                return Err(PyValueError::new_err(
                    "axes must be one of: 'x', 'y', 'xy', or 'xyz'",
                ));
            }
        };
        self.0.bind_position_from(&source.0, axes);
        Ok(())
    }

    // --- manim Axes compatibility ---
    /// manim `coords_to_point` — data coords → scene point (respects auto_fit/x_length).
    #[pyo3(name = "_legacy_coords_to_point")]
    fn legacy_coords_to_point(&self, x: f64, y: f64) -> PyResult<(f64, f64)> {
        let Some(((x_min, x_max, _), (y_min, y_max, _), config)) = self.0.axes_info() else {
            return Err(PyValueError::new_err(
                "coords_to_point() only valid on axes",
            ));
        };
        let (avail_w, avail_h) = (800.0, 480.0);
        let manim_w: f64 = 14.222222222222221;
        let manim_h: f64 = 8.0;
        let (scale_x, scale_y) = match (config.x_length, config.y_length) {
            (Some(xl), Some(yl)) => (
                xl * avail_w / manim_w / (x_max - x_min).max(1e-9),
                yl * avail_h / manim_h / (y_max - y_min).max(1e-9),
            ),
            (Some(xl), None) => {
                let s = xl * avail_w / manim_w / (x_max - x_min).max(1e-9);
                (s, s)
            }
            (None, Some(yl)) => {
                let s = yl * avail_h / manim_h / (y_max - y_min).max(1e-9);
                (s, s)
            }
            (None, None) if config.auto_fit => {
                let s =
                    (avail_w / (x_max - x_min).max(1e-9)).min(avail_h / (y_max - y_min).max(1e-9));
                (s, s)
            }
            _ => (1.0, 1.0),
        };
        let x_center = (x_min + x_max) * 0.5;
        let y_center = (y_min + y_max) * 0.5;
        let (sx, sy) = if config.auto_fit || config.x_length.is_some() || config.y_length.is_some()
        {
            ((x - x_center) * scale_x, (y - y_center) * scale_y)
        } else {
            (x, y)
        };
        Ok((sx, sy))
    }

    /// manim `point_to_coords` — scene point → data coords (inverse of coords_to_point).
    #[pyo3(name = "_legacy_point_to_coords")]
    fn legacy_point_to_coords(&self, point: (f64, f64)) -> PyResult<(f64, f64)> {
        let Some(((x_min, x_max, _), (y_min, y_max, _), config)) = self.0.axes_info() else {
            return Err(PyValueError::new_err(
                "point_to_coords() only valid on axes",
            ));
        };
        let (avail_w, avail_h) = (800.0, 480.0);
        let manim_w: f64 = 14.222222222222221;
        let manim_h: f64 = 8.0;
        let (scale_x, scale_y) = match (config.x_length, config.y_length) {
            (Some(xl), Some(yl)) => (
                xl * avail_w / manim_w / (x_max - x_min).max(1e-9),
                yl * avail_h / manim_h / (y_max - y_min).max(1e-9),
            ),
            (Some(xl), None) => {
                let s = xl * avail_w / manim_w / (x_max - x_min).max(1e-9);
                (s, s)
            }
            (None, Some(yl)) => {
                let s = yl * avail_h / manim_h / (y_max - y_min).max(1e-9);
                (s, s)
            }
            (None, None) if config.auto_fit => {
                let s =
                    (avail_w / (x_max - x_min).max(1e-9)).min(avail_h / (y_max - y_min).max(1e-9));
                (s, s)
            }
            _ => (1.0, 1.0),
        };
        let x_center = (x_min + x_max) * 0.5;
        let y_center = (y_min + y_max) * 0.5;
        let (x, y) = if config.auto_fit || config.x_length.is_some() || config.y_length.is_some() {
            (point.0 / scale_x + x_center, point.1 / scale_y + y_center)
        } else {
            (point.0, point.1)
        };
        Ok((x, y))
    }

    /// manim `get_x_axis` / `get_y_axis` — return the axes itself (group) for compatibility.
    #[pyo3(name = "_legacy_get_x_axis")]
    fn legacy_get_x_axis(&self) -> PyResult<Self> {
        if self.0.axes_info().is_none() {
            return Err(PyValueError::new_err("get_x_axis() only valid on axes"));
        }
        Ok(Self(self.0.clone()))
    }
    #[pyo3(name = "_legacy_get_y_axis")]
    fn legacy_get_y_axis(&self) -> PyResult<Self> {
        if self.0.axes_info().is_none() {
            return Err(PyValueError::new_err("get_y_axis() only valid on axes"));
        }
        Ok(Self(self.0.clone()))
    }
    #[pyo3(name = "_legacy_get_axes")]
    fn legacy_get_axes(&self) -> PyResult<Self> {
        self.legacy_get_x_axis()
    }

    /// manim `add_coordinates` — no-op in gaanim (numbers already via config), kept for compat.
    #[pyo3(name = "_legacy_add_coordinates")]
    fn legacy_add_coordinates(&self) -> Self {
        Self(self.0.clone())
    }

    /// manim `get_graph` / `plot` alias — note: use `scene.plot(axes, func, ...)` for actual graph creation.
    /// This is a compatibility shim that creates an empty polyline.
    #[pyo3(name = "_legacy_get_graph")]
    fn legacy_get_graph(&self) -> PyResult<Self> {
        if self.0.axes_info().is_none() {
            return Err(PyValueError::new_err("get_graph() only valid on axes"));
        }
        Ok(Self(self.0.clone()))
    }
}
