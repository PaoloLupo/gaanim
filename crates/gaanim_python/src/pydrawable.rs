//! Thin PyDrawable wrapper over gaanim_api DrawableHandle.

use pyo3::exceptions::{PyKeyError, PyTypeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::PyTuple;

use crate::brush::PyPaint;
use crate::color::PyColor;
use crate::easing::PyEasing;
use crate::py3d::PyMaterial3D;
use crate::pylayout::{expression_for, PyAnchor, PyDirection, PyLayoutExpression};
use crate::pystyle::PyStrokeStyle;
use crate::updater::PyUpdater;
use crate::visualization::extract_scalar_source_for_drawable;
use gaanim_animation::{PropertyChannel, PropertySources, ScalarSource};

fn scalar_for_anim(value: &Bound<'_, PyAny>, anim: &PyCanvasAnim) -> PyResult<ScalarSource> {
    let drawable = anim.inner.property_drawable().ok_or_else(|| {
        PyTypeError::new_err("reactive sources require a Drawable animation proxy")
    })?;
    extract_scalar_source_for_drawable(value.clone(), &drawable)
}

fn binding_result(drawable: &PyDrawable, sources: PropertySources) -> PyResult<PyDrawable> {
    drawable
        .0
        .clone()
        .bind_property(sources)
        .map(PyDrawable)
        .map_err(PyValueError::new_err)
}

fn source_anim(anim: &PyCanvasAnim, sources: PropertySources) -> PyResult<PyCanvasAnim> {
    anim.inner
        .clone()
        .property_source(sources)
        .map(|inner| PyCanvasAnim { inner })
        .map_err(PyValueError::new_err)
}

fn free_channel(
    drawable: &gaanim_api::canvas::DrawableHandle,
    channel: PropertyChannel,
) -> PyResult<()> {
    if drawable.property_is_bound(channel) {
        Err(PyValueError::new_err(format!(
            "{} is reactively bound; animate its Parameter or assign a fixed value first",
            channel.name()
        )))
    } else {
        Ok(())
    }
}

fn parse_sampled_property(value: &str) -> PyResult<gaanim_animation::SampledProperty> {
    match value {
        "x" => Ok(gaanim_animation::SampledProperty::TranslateX),
        "y" => Ok(gaanim_animation::SampledProperty::TranslateY),
        "z" => Ok(gaanim_animation::SampledProperty::TranslateZ),
        "rotation" => Ok(gaanim_animation::SampledProperty::RotateZ),
        "scale" => Ok(gaanim_animation::SampledProperty::UniformScale),
        "opacity" => Ok(gaanim_animation::SampledProperty::Opacity),
        "signal" => Ok(gaanim_animation::SampledProperty::Signal),
        _ => Err(PyValueError::new_err(
            "property must be one of 'x', 'y', 'z', 'rotation', 'scale', 'opacity', 'signal'",
        )),
    }
}

pub(crate) fn parse_sampled_interpolation(
    value: &str,
) -> PyResult<gaanim_animation::SampledInterpolation> {
    match value {
        "linear" => Ok(gaanim_animation::SampledInterpolation::Linear),
        "step" => Ok(gaanim_animation::SampledInterpolation::Step),
        _ => Err(PyValueError::new_err(
            "interpolation must be 'linear' or 'step'",
        )),
    }
}

#[pyclass(name = "AnchorPoint", module = "gaanim_core", frozen, from_py_object)]
#[derive(Clone, Copy, Debug)]
pub struct PyAnchorPoint(pub gaanim_api::canvas::AnchorPoint);

#[pyclass(name = "Anim", module = "gaanim_core", from_py_object)]
#[derive(Clone, Debug)]
pub struct PyCanvasAnim {
    pub inner: gaanim_api::canvas::Anim,
}

impl PyCanvasAnim {
    fn require_native_animation(&self) -> PyResult<()> {
        crate::custom::ensure_authoring_allowed()?;
        if matches!(
            self.inner.inner.anim_type,
            gaanim_api::anim::AnimationType::CustomProperties(_)
        ) {
            Err(PyValueError::new_err("custom() cannot be combined with property setters or native effects in one Anim; combine separate animations with parallel()"))
        } else {
            Ok(())
        }
    }
}

#[pymethods]
impl PyCanvasAnim {
    /// Evaluate a pure callback at exact eased progress during playback and seek.
    #[pyo3(signature = (callback, *, channels))]
    fn custom(&self, callback: Py<PyAny>, channels: Vec<String>) -> PyResult<Self> {
        crate::custom::ensure_authoring_allowed()?;
        let animation = crate::custom::animation(callback, channels)?;
        self.inner
            .clone()
            .custom(animation)
            .map(|inner| Self { inner })
            .map_err(PyValueError::new_err)
    }

    fn require_transformable(&self) -> PyResult<()> {
        if self.inner.property_target_is_text_selection() {
            return Err(PyTypeError::new_err(
                "TextSelection.animate supports only fill and opacity targets",
            ));
        }
        crate::custom::ensure_authoring_allowed()?;
        if self.inner.property_position_is_free() {
            Ok(())
        } else {
            Err(PyValueError::new_err(
                "live derived geometry or layout owns this drawable's transform",
            ))
        }
    }
    fn fill_level(&self, level: f64) -> PyResult<Self> {
        crate::custom::ensure_authoring_allowed()?;
        self.require_native_animation()?;
        if !level.is_finite() || !(0.0..=1.0).contains(&level) {
            return Err(PyValueError::new_err(
                "fill level must be finite and between zero and one",
            ));
        }
        self.inner
            .clone()
            .try_fill_level(level)
            .map(|inner| Self { inner })
            .map_err(PyValueError::new_err)
    }
    fn fill(&self, color: PyPaint) -> PyResult<Self> {
        crate::custom::ensure_authoring_allowed()?;
        self.require_native_animation()?;
        self.inner
            .clone()
            .try_fill_paint(color.0)
            .map(|inner| Self { inner })
            .map_err(PyValueError::new_err)
    }

    fn stroke(&self, color: PyPaint, width: f64) -> PyResult<Self> {
        crate::custom::ensure_authoring_allowed()?;
        self.require_native_animation()?;
        if self.inner.property_target_is_text_selection() {
            return Err(PyTypeError::new_err(
                "TextSelection.animate supports only fill and opacity targets",
            ));
        }
        if self.inner.property_target_is_primitive_3d() {
            return Err(PyTypeError::new_err(
                "stroke() is only available for vector drawables; animate fill() or material() on Primitive3D",
            ));
        }
        self.inner
            .clone()
            .try_stroke_paint(color.0, width)
            .map(|inner| Self { inner })
            .map_err(PyValueError::new_err)
    }

    fn material(&self, material: PyMaterial3D) -> PyResult<Self> {
        crate::custom::ensure_authoring_allowed()?;
        self.require_native_animation()?;
        if self.inner.property_target_is_text_selection() {
            return Err(PyTypeError::new_err(
                "TextSelection.animate supports only fill and opacity targets",
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

    fn opacity(&self, value: &Bound<'_, PyAny>) -> PyResult<Self> {
        crate::custom::ensure_authoring_allowed()?;
        self.require_native_animation()?;
        if let Ok(value) = value.extract::<f32>() {
            return Ok(Self {
                inner: self.inner.clone().opacity(value),
            });
        }
        source_anim(
            self,
            PropertySources::Opacity(scalar_for_anim(value, self)?),
        )
    }

    fn set(&self, value: f64) -> PyResult<Self> {
        crate::custom::ensure_authoring_allowed()?;
        self.require_native_animation()?;
        if !value.is_finite() {
            return Err(PyValueError::new_err("parameter values must be finite"));
        }
        Ok(Self {
            inner: self.inner.clone().set(value),
        })
    }

    fn transform_to(&self, target: &PyDrawable) -> PyResult<Self> {
        crate::custom::ensure_authoring_allowed()?;
        self.require_native_animation()?;
        self.inner
            .clone()
            .transform_to(&target.0)
            .map(|inner| Self { inner })
            .map_err(PyValueError::new_err)
    }

    fn shift_by(&self, dx: f64, dy: f64) -> PyResult<Self> {
        crate::custom::ensure_authoring_allowed()?;
        self.require_native_animation()?;
        if self.inner.property_target_is_text_selection() {
            return Err(PyTypeError::new_err(
                "TextSelection.animate supports only fill and opacity targets",
            ));
        }
        if !self.inner.property_position_is_free() {
            return Err(crate::LayoutOwnershipError::new_err(
                "layout owns this drawable's translation; animate the LayoutItem offset instead",
            ));
        }
        Ok(Self {
            inner: self.inner.clone().shift_by(dx, dy),
        })
    }

    #[pyo3(signature = (x, y=None, anchor=None))]
    fn move_to(
        &self,
        x: &Bound<'_, PyAny>,
        y: Option<&Bound<'_, PyAny>>,
        anchor: Option<&PyAnchor>,
    ) -> PyResult<Self> {
        crate::custom::ensure_authoring_allowed()?;
        self.require_native_animation()?;
        self.require_transformable()?;
        if let Some(y) = y {
            let sx = scalar_for_anim(x, self)?;
            let sy = scalar_for_anim(y, self)?;
            if let (Some(x), Some(y)) = (sx.constant_value(), sy.constant_value()) {
                return Ok(Self {
                    inner: self.inner.clone().move_to_anchor(
                        x,
                        y,
                        anchor.map(|anchor| anchor.0).unwrap_or_default(),
                    ),
                });
            }
            let anchor = anchor
                .map(|anchor| anchor.0)
                .unwrap_or_default()
                .to_offset();
            return source_anim(
                self,
                PropertySources::Translation {
                    values: [sx, sy, 0.0.into()],
                    anchor: Some(gaanim_core::glam::DVec3::new(anchor.x, anchor.y, 0.0)),
                },
            );
        }
        let inner = match resolve_at_target("move_to", x, None, anchor.is_some())? {
            PyAtTarget::Drawable(reference) => self.inner.clone().move_to_drawable(&reference),
            PyAtTarget::AnchorPoint(point) => self.inner.clone().move_to_anchor_point(point),
            PyAtTarget::Coordinates { x, y } => Ok(self.inner.clone().move_to(x, y)),
        }
        .map_err(PyValueError::new_err)?;
        Ok(Self { inner })
    }

    fn shift_by_3d(&self, dx: f64, dy: f64, dz: f64) -> PyResult<Self> {
        crate::custom::ensure_authoring_allowed()?;
        self.require_native_animation()?;
        if self.inner.property_target_is_text_selection() {
            return Err(PyTypeError::new_err(
                "TextSelection.animate supports only fill and opacity targets",
            ));
        }
        if !self.inner.property_position_is_free() {
            return Err(crate::LayoutOwnershipError::new_err(
                "layout owns this drawable's translation; animate the LayoutItem offset instead",
            ));
        }
        Ok(Self {
            inner: self.inner.clone().shift_by_3d(dx, dy, dz),
        })
    }

    fn move_to_3d(
        &self,
        x: &Bound<'_, PyAny>,
        y: &Bound<'_, PyAny>,
        z: &Bound<'_, PyAny>,
    ) -> PyResult<Self> {
        crate::custom::ensure_authoring_allowed()?;
        self.require_native_animation()?;
        self.require_transformable()?;
        let values = [
            scalar_for_anim(x, self)?,
            scalar_for_anim(y, self)?,
            scalar_for_anim(z, self)?,
        ];
        if let (Some(x), Some(y), Some(z)) = (
            values[0].constant_value(),
            values[1].constant_value(),
            values[2].constant_value(),
        ) {
            return Ok(Self {
                inner: self.inner.clone().move_to_3d(x, y, z),
            });
        }
        source_anim(
            self,
            PropertySources::Translation {
                values,
                anchor: None,
            },
        )
    }

    fn scale_by(&self, factor: f64) -> PyResult<Self> {
        crate::custom::ensure_authoring_allowed()?;
        self.require_native_animation()?;
        if self.inner.property_target_is_text_selection() {
            return Err(PyTypeError::new_err(
                "TextSelection.animate supports only fill and opacity targets",
            ));
        }
        self.require_transformable()?;
        Ok(Self {
            inner: self.inner.clone().scale_by(factor),
        })
    }

    fn scale_to(&self, factor: &Bound<'_, PyAny>) -> PyResult<Self> {
        crate::custom::ensure_authoring_allowed()?;
        self.require_native_animation()?;
        self.require_transformable()?;
        let source = scalar_for_anim(factor, self)?;
        if let Some(value) = source.constant_value() {
            return Ok(Self {
                inner: self.inner.clone().scale_to(value),
            });
        }
        source_anim(
            self,
            PropertySources::Scale([source.clone(), source.clone(), source]),
        )
    }

    fn scale_to_3d(
        &self,
        x: &Bound<'_, PyAny>,
        y: &Bound<'_, PyAny>,
        z: &Bound<'_, PyAny>,
    ) -> PyResult<Self> {
        crate::custom::ensure_authoring_allowed()?;
        self.require_native_animation()?;
        self.require_transformable()?;
        let values = [
            scalar_for_anim(x, self)?,
            scalar_for_anim(y, self)?,
            scalar_for_anim(z, self)?,
        ];
        if let (Some(x), Some(y), Some(z)) = (
            values[0].constant_value(),
            values[1].constant_value(),
            values[2].constant_value(),
        ) {
            return Ok(Self {
                inner: self.inner.clone().scale_to_3d(x, y, z),
            });
        }
        source_anim(self, PropertySources::Scale(values))
    }

    fn scale_by_3d(&self, x: f64, y: f64, z: f64) -> PyResult<Self> {
        crate::custom::ensure_authoring_allowed()?;
        self.require_native_animation()?;
        if self.inner.property_target_is_text_selection() {
            return Err(PyTypeError::new_err(
                "TextSelection.animate supports only fill and opacity targets",
            ));
        }
        self.require_transformable()?;
        Ok(Self {
            inner: self.inner.clone().scale_by_3d(x, y, z),
        })
    }

    fn rotate_by(&self, radians: f64) -> PyResult<Self> {
        crate::custom::ensure_authoring_allowed()?;
        self.require_native_animation()?;
        if self.inner.property_target_is_text_selection() {
            return Err(PyTypeError::new_err(
                "TextSelection.animate supports only fill and opacity targets",
            ));
        }
        self.require_transformable()?;
        Ok(Self {
            inner: self.inner.clone().rotate_by(radians),
        })
    }

    fn rotate_to(&self, radians: &Bound<'_, PyAny>) -> PyResult<Self> {
        crate::custom::ensure_authoring_allowed()?;
        self.require_native_animation()?;
        self.require_transformable()?;
        let source = scalar_for_anim(radians, self)?;
        if let Some(value) = source.constant_value() {
            return Ok(Self {
                inner: self.inner.clone().rotate_to(value),
            });
        }
        source_anim(
            self,
            PropertySources::Rotation([0.0.into(), 0.0.into(), source]),
        )
    }

    fn rotate_by_3d(&self, axis: &str, radians: f64) -> PyResult<Self> {
        crate::custom::ensure_authoring_allowed()?;
        self.require_native_animation()?;
        if self.inner.property_target_is_text_selection() {
            return Err(PyTypeError::new_err(
                "TextSelection.animate() supports only fill/color and opacity targets",
            ));
        }
        self.require_transformable()?;
        self.inner
            .clone()
            .rotate_by_3d(axis, radians)
            .map(|inner| Self { inner })
            .map_err(PyValueError::new_err)
    }

    fn rotate_to_3d(
        &self,
        x: &Bound<'_, PyAny>,
        y: &Bound<'_, PyAny>,
        z: &Bound<'_, PyAny>,
    ) -> PyResult<Self> {
        crate::custom::ensure_authoring_allowed()?;
        self.require_native_animation()?;
        self.require_transformable()?;
        let values = [
            scalar_for_anim(x, self)?,
            scalar_for_anim(y, self)?,
            scalar_for_anim(z, self)?,
        ];
        if let (Some(x), Some(y), Some(z)) = (
            values[0].constant_value(),
            values[1].constant_value(),
            values[2].constant_value(),
        ) {
            return Ok(Self {
                inner: self.inner.clone().rotate_to_3d(x, y, z),
            });
        }
        source_anim(self, PropertySources::Rotation(values))
    }

    fn fade_in(&self) -> PyResult<Self> {
        crate::custom::ensure_authoring_allowed()?;
        self.require_native_animation()?;
        if self.inner.property_target_is_text_selection() {
            return Err(PyTypeError::new_err(
                "fade_in() requires a Drawable animation proxy",
            ));
        }
        Ok(Self {
            inner: self.inner.clone().fade_in(),
        })
    }

    #[pyo3(signature = (direction, distance=0.48))]
    fn fade_in_from(&self, direction: &PyDirection, distance: f64) -> PyResult<Self> {
        crate::custom::ensure_authoring_allowed()?;
        self.require_native_animation()?;
        if self.inner.property_target_is_text_selection() {
            return Err(PyTypeError::new_err(
                "fade_in_from() requires a Drawable animation proxy",
            ));
        }
        self.require_transformable()?;
        if !distance.is_finite() || distance < 0.0 {
            return Err(PyValueError::new_err(
                "distance must be a finite non-negative number",
            ));
        }
        Ok(Self {
            inner: self
                .inner
                .clone()
                .fade_in_from(direction.0.clone(), distance),
        })
    }

    fn fade_out(&self) -> PyResult<Self> {
        crate::custom::ensure_authoring_allowed()?;
        self.require_native_animation()?;
        if self.inner.property_target_is_text_selection() {
            return Err(PyTypeError::new_err(
                "fade_out() requires a Drawable animation proxy",
            ));
        }
        Ok(Self {
            inner: self.inner.clone().fade_out(),
        })
    }

    #[pyo3(signature = (*, by="grapheme", order="forward", stagger=None))]
    fn write(&self, by: &str, order: &str, stagger: Option<f64>) -> PyResult<Self> {
        crate::custom::ensure_authoring_allowed()?;
        self.require_native_animation()?;
        if self.inner.property_target_is_text_selection() {
            return Err(PyTypeError::new_err(
                "write() requires a Drawable animation proxy",
            ));
        }
        if !matches!(by, "grapheme" | "word" | "line" | "part") {
            return Err(PyValueError::new_err(
                "by must be grapheme, word, line, or part",
            ));
        }
        if !matches!(order, "forward" | "reverse" | "center" | "random") {
            return Err(PyValueError::new_err(
                "order must be forward, reverse, center, or random",
            ));
        }
        if let Some(stagger) = stagger {
            if !stagger.is_finite() || stagger < 0.0 {
                return Err(PyValueError::new_err(
                    "stagger must be finite and non-negative",
                ));
            }
        }
        let mut inner = self.inner.clone().write();
        if let Some(stagger) = stagger {
            inner = inner.lag_ratio(stagger);
        }
        Ok(Self { inner })
    }

    fn create(&self) -> PyResult<Self> {
        crate::custom::ensure_authoring_allowed()?;
        self.require_native_animation()?;
        if self.inner.property_target_is_text_selection() {
            return Err(PyTypeError::new_err(
                "create() requires a Drawable animation proxy",
            ));
        }
        Ok(Self {
            inner: self.inner.clone().create(),
        })
    }

    fn unwrite(&self) -> PyResult<Self> {
        crate::custom::ensure_authoring_allowed()?;
        self.require_native_animation()?;
        if self.inner.property_target_is_text_selection() {
            return Err(PyTypeError::new_err(
                "unwrite() requires a Drawable animation proxy",
            ));
        }
        Ok(Self {
            inner: self.inner.clone().unwrite(),
        })
    }

    fn uncreate(&self) -> PyResult<Self> {
        crate::custom::ensure_authoring_allowed()?;
        self.require_native_animation()?;
        if self.inner.property_target_is_text_selection() {
            return Err(PyTypeError::new_err(
                "uncreate() requires a Drawable animation proxy",
            ));
        }
        Ok(Self {
            inner: self.inner.clone().uncreate(),
        })
    }

    fn grow_from_center(&self) -> PyResult<Self> {
        crate::custom::ensure_authoring_allowed()?;
        self.require_native_animation()?;
        Ok({
            Self {
                inner: self.inner.clone().grow_from_center(),
            }
        })
    }

    fn shrink_to_center(&self) -> PyResult<Self> {
        crate::custom::ensure_authoring_allowed()?;
        self.require_native_animation()?;
        Ok({
            Self {
                inner: self.inner.clone().shrink_to_center(),
            }
        })
    }

    fn spin_in_from_nothing(&self) -> PyResult<Self> {
        crate::custom::ensure_authoring_allowed()?;
        self.require_native_animation()?;
        self.require_transformable()?;
        Ok(Self {
            inner: self.inner.clone().spin_in_from_nothing(),
        })
    }

    fn draw_border_then_fill(&self) -> PyResult<Self> {
        crate::custom::ensure_authoring_allowed()?;
        self.require_native_animation()?;
        Ok({
            Self {
                inner: self.inner.clone().draw_border_then_fill(),
            }
        })
    }

    fn circumscribe(&self) -> PyResult<Self> {
        crate::custom::ensure_authoring_allowed()?;
        self.require_native_animation()?;
        Ok({
            Self {
                inner: self.inner.clone().circumscribe(),
            }
        })
    }

    fn flash(&self) -> PyResult<Self> {
        crate::custom::ensure_authoring_allowed()?;
        self.require_native_animation()?;
        Ok({
            Self {
                inner: self.inner.clone().flash(),
            }
        })
    }

    #[pyo3(signature = (*, time_width=0.2))]
    fn show_passing_flash(&self, time_width: f64) -> PyResult<Self> {
        crate::custom::ensure_authoring_allowed()?;
        self.require_native_animation()?;
        if !time_width.is_finite() || time_width <= 0.0 || time_width > 1.0 {
            return Err(PyValueError::new_err(
                "time_width must be finite and in (0, 1]",
            ));
        }
        Ok(Self {
            inner: self.inner.clone().show_passing_flash(time_width),
        })
    }

    fn move_along(&self, target: &PyDrawable) -> PyResult<Self> {
        crate::custom::ensure_authoring_allowed()?;
        self.require_native_animation()?;
        self.require_transformable()?;
        self.inner
            .clone()
            .move_along(&target.0)
            .map(|inner| Self { inner })
            .map_err(PyValueError::new_err)
    }

    fn fade_transform_to(&self, target: &PyDrawable) -> PyResult<Self> {
        crate::custom::ensure_authoring_allowed()?;
        self.require_native_animation()?;
        self.inner
            .clone()
            .fade_transform_to(&target.0)
            .map(|inner| Self { inner })
            .map_err(PyValueError::new_err)
    }

    fn replacement_transform_to(&self, target: &PyDrawable) -> PyResult<Self> {
        crate::custom::ensure_authoring_allowed()?;
        self.require_native_animation()?;
        self.inner
            .clone()
            .replacement_transform_to(&target.0)
            .map(|inner| Self { inner })
            .map_err(PyValueError::new_err)
    }

    fn indicate(&self) -> PyResult<Self> {
        crate::custom::ensure_authoring_allowed()?;
        self.require_native_animation()?;
        Ok(Self {
            inner: self.inner.clone().indicate(),
        })
    }

    fn wiggle(&self) -> PyResult<Self> {
        crate::custom::ensure_authoring_allowed()?;
        self.require_native_animation()?;
        Ok(Self {
            inner: self.inner.clone().wiggle(),
        })
    }

    fn pulse(&self) -> PyResult<Self> {
        crate::custom::ensure_authoring_allowed()?;
        self.require_native_animation()?;
        Ok({
            Self {
                inner: self.inner.clone().pulse(),
            }
        })
    }

    fn wave(&self) -> PyResult<Self> {
        crate::custom::ensure_authoring_allowed()?;
        self.require_native_animation()?;
        Ok({
            Self {
                inner: self.inner.clone().wave(),
            }
        })
    }

    fn highlight(&self) -> PyResult<Self> {
        crate::custom::ensure_authoring_allowed()?;
        self.require_native_animation()?;
        Ok({
            Self {
                inner: self.inner.clone().highlight(),
            }
        })
    }

    fn focus(&self) -> PyResult<Self> {
        crate::custom::ensure_authoring_allowed()?;
        self.require_native_animation()?;
        Ok({
            Self {
                inner: self.inner.clone().focus(),
            }
        })
    }

    fn cancel(&self) -> PyResult<Self> {
        crate::custom::ensure_authoring_allowed()?;
        self.require_native_animation()?;
        Ok({
            Self {
                inner: self.inner.clone().cancel(),
            }
        })
    }

    fn duration(&self, seconds: f64) -> PyResult<Self> {
        crate::custom::ensure_authoring_allowed()?;
        if !seconds.is_finite() || seconds < 0.0 {
            return Err(PyValueError::new_err(
                "seconds must be finite and non-negative",
            ));
        }
        Ok(Self {
            inner: self.inner.clone().duration(seconds),
        })
    }

    fn easing(&self, easing: &PyEasing) -> PyResult<Self> {
        crate::custom::ensure_authoring_allowed()?;
        Ok(Self {
            inner: self.inner.clone().rate_func(easing.inner.clone()),
        })
    }

    fn delay(&self, seconds: f64) -> PyResult<Self> {
        crate::custom::ensure_authoring_allowed()?;
        if !seconds.is_finite() || seconds < 0.0 {
            return Err(PyValueError::new_err(
                "seconds must be finite and non-negative",
            ));
        }
        Ok(Self {
            inner: self.inner.clone().delay(seconds),
        })
    }

    fn lag_ratio(&self, value: f64) -> PyResult<Self> {
        crate::custom::ensure_authoring_allowed()?;
        self.require_native_animation()?;
        Ok({
            Self {
                inner: self.inner.clone().lag_ratio(value),
            }
        })
    }

    fn stroke_width(&self, value: f64) -> PyResult<Self> {
        crate::custom::ensure_authoring_allowed()?;
        self.require_native_animation()?;
        Ok({
            Self {
                inner: self.inner.clone().stroke_width(value),
            }
        })
    }

    fn with_pen_tip(&self) -> PyResult<Self> {
        crate::custom::ensure_authoring_allowed()?;
        self.require_native_animation()?;
        Ok({
            Self {
                inner: self.inner.clone().with_pen_tip(),
            }
        })
    }

    fn pivot(&self, x: f64, y: f64) -> PyResult<Self> {
        crate::custom::ensure_authoring_allowed()?;
        self.require_native_animation()?;
        Ok({
            Self {
                inner: self.inner.clone().pivot(x, y),
            }
        })
    }

    fn about_point(&self, x: f64, y: f64) -> PyResult<Self> {
        crate::custom::ensure_authoring_allowed()?;
        self.require_native_animation()?;
        self.pivot(x, y)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gaanim_api::anim::AnimationType;
    use gaanim_api::canvas::SceneModel;

    #[test]
    fn write_without_explicit_stagger_preserves_adaptive_scheduling() {
        let mut scene = SceneModel::new(640, 360);
        let text = scene.text("sequential");
        let animation = PyCanvasAnim {
            inner: text.animate(),
        }
        .write("grapheme", "forward", None)
        .expect("default write should be valid");

        let AnimationType::Write { config } = animation.inner.inner.anim_type else {
            panic!("write() should produce a Write animation");
        };
        assert_eq!(config.lag_ratio, None);
    }
}

#[pyclass(name = "Drawable", module = "gaanim_core", subclass, from_py_object)]
#[derive(Clone)]
pub struct PyDrawable(pub gaanim_api::canvas::DrawableHandle);

pub(crate) enum PyAtTarget {
    Coordinates { x: f64, y: f64 },
    Drawable(gaanim_api::canvas::DrawableHandle),
    AnchorPoint(gaanim_api::canvas::AnchorPoint),
}

pub(crate) fn validate_at_target_owner(
    target: &PyAtTarget,
    drawable: &gaanim_api::canvas::DrawableHandle,
) -> PyResult<()> {
    let valid = match target {
        PyAtTarget::Drawable(reference) => drawable.same_canvas(reference),
        PyAtTarget::AnchorPoint(point) => drawable.owns_anchor_point(*point),
        PyAtTarget::Coordinates { .. } => true,
    };
    if valid {
        Ok(())
    } else {
        Err(PyValueError::new_err(
            "move_to target must belong to the same Scene",
        ))
    }
}

pub(crate) fn resolve_at_target(
    operation: &str,
    x: &Bound<'_, PyAny>,
    y: Option<f64>,
    has_anchor: bool,
) -> PyResult<PyAtTarget> {
    if let Ok(point) = x.extract::<PyRef<'_, PyAnchorPoint>>() {
        if y.is_some() || has_anchor {
            return Err(PyTypeError::new_err(format!(
                "{operation}() with an AnchorPoint accepts no y or anchor"
            )));
        }
        return Ok(PyAtTarget::AnchorPoint(point.0));
    }

    if let Ok(reference) = x.extract::<PyRef<'_, PyDrawable>>() {
        if y.is_some() || has_anchor {
            return Err(PyTypeError::new_err(format!(
                "{operation}() with a Drawable accepts no y or anchor"
            )));
        }
        return Ok(PyAtTarget::Drawable(reference.0.clone()));
    }

    let x = x.extract::<f64>().map_err(|_| {
        PyTypeError::new_err(format!(
            "{operation}() expects a Drawable, AnchorPoint, or numeric x and y coordinates"
        ))
    })?;
    let y = y.ok_or_else(|| {
        PyTypeError::new_err(format!(
            "{operation}() with numeric coordinates requires both x and y"
        ))
    })?;
    Ok(PyAtTarget::Coordinates { x, y })
}

impl PyDrawable {
    fn require_free_position(&self, operation: &str) -> PyResult<()> {
        if self.0.is_live_derived_geometry() {
            return Err(PyValueError::new_err(format!(
                "live derived geometry owns this drawable's path and position; operation '{operation}' is not available"
            )));
        }
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
        crate::custom::ensure_authoring_allowed()?;
        if !offset.0.is_finite() || !offset.1.is_finite() {
            return Err(PyValueError::new_err("anchor offset must be finite"));
        }
        Ok(PyAnchorPoint(self.0.anchor_point(
            anchor.map(|value| value.0).unwrap_or_default(),
            gaanim_core::glam::DVec3::new(offset.0, offset.1, 0.0),
        )))
    }

    #[getter]
    fn left(&self) -> PyResult<PyLayoutExpression> {
        crate::custom::ensure_authoring_allowed()?;
        Ok(expression_for(
            &self.0,
            gaanim_layout::LayoutAttribute::Left,
        ))
    }

    #[getter]
    fn right(&self) -> PyResult<PyLayoutExpression> {
        crate::custom::ensure_authoring_allowed()?;
        Ok(expression_for(
            &self.0,
            gaanim_layout::LayoutAttribute::Right,
        ))
    }

    #[getter]
    fn top(&self) -> PyResult<PyLayoutExpression> {
        crate::custom::ensure_authoring_allowed()?;
        Ok(expression_for(&self.0, gaanim_layout::LayoutAttribute::Top))
    }

    #[getter]
    fn bottom(&self) -> PyResult<PyLayoutExpression> {
        crate::custom::ensure_authoring_allowed()?;
        Ok(expression_for(
            &self.0,
            gaanim_layout::LayoutAttribute::Bottom,
        ))
    }

    #[getter]
    fn center_x(&self) -> PyResult<PyLayoutExpression> {
        crate::custom::ensure_authoring_allowed()?;
        Ok(expression_for(
            &self.0,
            gaanim_layout::LayoutAttribute::CenterX,
        ))
    }

    #[getter]
    fn center_y(&self) -> PyResult<PyLayoutExpression> {
        crate::custom::ensure_authoring_allowed()?;
        Ok(expression_for(
            &self.0,
            gaanim_layout::LayoutAttribute::CenterY,
        ))
    }

    #[getter]
    fn width(&self) -> PyResult<PyLayoutExpression> {
        crate::custom::ensure_authoring_allowed()?;
        Ok(expression_for(
            &self.0,
            gaanim_layout::LayoutAttribute::Width,
        ))
    }

    #[getter]
    fn height(&self) -> PyResult<PyLayoutExpression> {
        crate::custom::ensure_authoring_allowed()?;
        Ok(expression_for(
            &self.0,
            gaanim_layout::LayoutAttribute::Height,
        ))
    }

    /// Return a named source group or path from an imported SVG or glTF.
    fn part(&self, id: &str) -> PyResult<Self> {
        crate::custom::ensure_authoring_allowed()?;
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
        crate::custom::ensure_authoring_allowed()?;
        Ok(PyTuple::new(py, self.0.parts())?.unbind())
    }

    fn animations(&self, py: Python<'_>) -> PyResult<Py<PyTuple>> {
        crate::custom::ensure_authoring_allowed()?;
        Ok(PyTuple::new(py, self.0.animations())?.unbind())
    }

    /// Return a fresh, pure animation proxy. Accessing it never schedules work.
    #[getter]
    fn animate(&self) -> PyResult<PyCanvasAnim> {
        crate::custom::ensure_authoring_allowed()?;
        Ok(PyCanvasAnim {
            inner: self.0.animate(),
        })
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
        crate::custom::ensure_authoring_allowed()?;
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

    fn fill(&self, paint: PyPaint) -> PyResult<Self> {
        crate::custom::ensure_authoring_allowed()?;
        Ok(Self(self.0.clone().fill_brush(paint.0)))
    }
    fn no_fill(&self) -> PyResult<Self> {
        crate::custom::ensure_authoring_allowed()?;
        Ok(Self(self.0.clone().no_fill()))
    }
    fn stroke(&self, paint: PyPaint, width: f64) -> PyResult<Self> {
        crate::custom::ensure_authoring_allowed()?;
        Ok(Self(self.0.clone().stroke_brush(paint.0, width)))
    }
    /// Apply cap, join, miter, and dash geometry from a reusable StrokeStyle.
    fn stroke_style(&self, style: PyStrokeStyle) -> PyResult<Self> {
        crate::custom::ensure_authoring_allowed()?;
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
    fn no_stroke(&self) -> PyResult<Self> {
        crate::custom::ensure_authoring_allowed()?;
        Ok(Self(self.0.clone().no_stroke()))
    }
    /// Add a theme class; calls may be chained and later classes win.
    fn style_class(&self, name: &str) -> PyResult<Self> {
        crate::custom::ensure_authoring_allowed()?;
        self.0
            .clone()
            .style_class(name)
            .map(Self)
            .map_err(PyValueError::new_err)
    }
    /// Add a cached soft outer glow.
    #[pyo3(signature = (color, radius=0.16, intensity=1.0))]
    fn glow(&self, color: PyColor, radius: f64, intensity: f32) -> PyResult<Self> {
        crate::custom::ensure_authoring_allowed()?;
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
    #[pyo3(signature = (sigma=0.04))]
    fn blur(&self, sigma: f64) -> PyResult<Self> {
        crate::custom::ensure_authoring_allowed()?;
        if !sigma.is_finite() || sigma <= 0.0 {
            return Err(PyValueError::new_err("sigma must be finite and positive"));
        }
        Ok(Self(self.0.clone().blur(sigma)))
    }
    /// Add a cached soft shadow behind the drawable.
    #[pyo3(signature = (color, x=0.08, y=-0.08, blur=0.06))]
    fn shadow(&self, color: PyColor, x: f64, y: f64, blur: f64) -> PyResult<Self> {
        crate::custom::ensure_authoring_allowed()?;
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
    fn no_effects(&self) -> PyResult<Self> {
        crate::custom::ensure_authoring_allowed()?;
        Ok(Self(self.0.clone().no_effects()))
    }
    /// Clip this drawable to another drawable's vector outline.
    #[pyo3(signature = (mask, rule="nonzero", invert=false))]
    fn clip(&self, mask: &PyDrawable, rule: &str, invert: bool) -> PyResult<Self> {
        crate::custom::ensure_authoring_allowed()?;
        let rule = match rule {
            "nonzero" => gaanim_core::peniko::Fill::NonZero,
            "evenodd" | "even_odd" => gaanim_core::peniko::Fill::EvenOdd,
            _ => {
                return Err(PyValueError::new_err("rule must be 'nonzero' or 'evenodd'"));
            }
        };
        Ok(Self(self.0.clone().clip_with(
            &mask.0,
            gaanim_api::canvas::ClipOptions { rule, invert },
        )))
    }
    /// Remove the clipping mask from this drawable.
    fn no_clip(&self) -> PyResult<Self> {
        crate::custom::ensure_authoring_allowed()?;
        Ok(Self(self.0.clone().no_clip()))
    }
    fn set_fill_level(&self, level: f64) -> PyResult<Self> {
        crate::custom::ensure_authoring_allowed()?;
        self.0
            .clone()
            .set_fill_level(level)
            .map(Self)
            .map_err(PyValueError::new_err)
    }
    pub(crate) fn opacity(&self, op: &Bound<'_, PyAny>) -> PyResult<Self> {
        crate::custom::ensure_authoring_allowed()?;
        let value = extract_scalar_source_for_drawable(op.clone(), &self.0)?;
        if let Some(value) = value.constant_value() {
            return Ok(Self(self.0.clone().opacity(value as f32)));
        }
        binding_result(self, PropertySources::Opacity(value))
    }
    fn z_index(&self, z: i32) -> PyResult<Self> {
        crate::custom::ensure_authoring_allowed()?;
        Ok(Self(self.0.clone().z_index(z)))
    }
    #[pyo3(signature = (x, y=None, anchor=None))]
    pub(crate) fn move_to(
        &self,
        x: &Bound<'_, PyAny>,
        y: Option<&Bound<'_, PyAny>>,
        anchor: Option<&PyAnchor>,
    ) -> PyResult<Self> {
        crate::custom::ensure_authoring_allowed()?;
        self.require_free_position("move_to")?;
        if let Some(y) = y {
            let sx = extract_scalar_source_for_drawable(x.clone(), &self.0)?;
            let sy = extract_scalar_source_for_drawable(y.clone(), &self.0)?;
            if let (Some(x), Some(y)) = (sx.constant_value(), sy.constant_value()) {
                return Ok(Self(if let Some(anchor) = anchor {
                    self.0.clone().at_anchor(x, y, anchor.0)
                } else {
                    self.0.clone().move_to_default(x, y)
                }));
            }
            let anchor = anchor
                .map(|anchor| anchor.0)
                .unwrap_or_default()
                .to_offset();
            return binding_result(
                self,
                PropertySources::Translation {
                    values: [sx, sy, 0.0.into()],
                    anchor: Some(gaanim_core::glam::DVec3::new(anchor.x, anchor.y, 0.0)),
                },
            );
        }
        match resolve_at_target("move_to", x, None, anchor.is_some())? {
            PyAtTarget::Coordinates { x, y } => Ok(Self(self.0.clone().move_to_default(x, y))),
            PyAtTarget::Drawable(reference) => {
                if !self.0.same_canvas(&reference) {
                    return Err(PyValueError::new_err("target belongs to another Scene"));
                }
                Ok(Self(self.0.clone().at_anchor_point(
                    reference.anchor_point(
                        gaanim_api::canvas::Anchor::Center,
                        gaanim_core::glam::DVec3::ZERO,
                    ),
                )))
            }
            PyAtTarget::AnchorPoint(point) => {
                validate_at_target_owner(&PyAtTarget::AnchorPoint(point), &self.0)?;
                Ok(Self(self.0.clone().at_anchor_point(point)))
            }
        }
    }
    pub(crate) fn move_to_3d(
        &self,
        x: &Bound<'_, PyAny>,
        y: &Bound<'_, PyAny>,
        z: &Bound<'_, PyAny>,
    ) -> PyResult<Self> {
        crate::custom::ensure_authoring_allowed()?;
        self.require_free_position("move_to_3d")?;
        let values = [
            extract_scalar_source_for_drawable(x.clone(), &self.0)?,
            extract_scalar_source_for_drawable(y.clone(), &self.0)?,
            extract_scalar_source_for_drawable(z.clone(), &self.0)?,
        ];
        if let (Some(x), Some(y), Some(z)) = (
            values[0].constant_value(),
            values[1].constant_value(),
            values[2].constant_value(),
        ) {
            return Ok(Self(self.0.clone().move_to_3d(x, y, z)));
        }
        binding_result(
            self,
            PropertySources::Translation {
                values,
                anchor: None,
            },
        )
    }
    pub(crate) fn shift_by(&self, dx: f64, dy: f64) -> PyResult<Self> {
        crate::custom::ensure_authoring_allowed()?;
        self.require_free_position("shift_by")?;
        free_channel(&self.0, PropertyChannel::Translation)?;
        Ok(Self(self.0.clone().shift_by(dx, dy)))
    }
    pub(crate) fn shift_by_3d(&self, dx: f64, dy: f64, dz: f64) -> PyResult<Self> {
        crate::custom::ensure_authoring_allowed()?;
        self.require_free_position("shift_by_3d")?;
        free_channel(&self.0, PropertyChannel::Translation)?;
        Ok(Self(self.0.clone().shift_by_3d(dx, dy, dz)))
    }
    fn billboard(&self) -> PyResult<Self> {
        crate::custom::ensure_authoring_allowed()?;
        Ok(Self(self.0.clone().billboard()))
    }
    fn hud(&self) -> PyResult<Self> {
        crate::custom::ensure_authoring_allowed()?;
        Ok(Self(self.0.clone().hud()))
    }
    pub(crate) fn scale_to(&self, factor: &Bound<'_, PyAny>) -> PyResult<Self> {
        crate::custom::ensure_authoring_allowed()?;
        self.require_free_position("scale_to")?;
        let source = extract_scalar_source_for_drawable(factor.clone(), &self.0)?;
        if let Some(value) = source.constant_value() {
            return Ok(Self(self.0.clone().scale_to(value)));
        }
        binding_result(
            self,
            PropertySources::Scale([source.clone(), source.clone(), source]),
        )
    }
    pub(crate) fn scale_to_3d(
        &self,
        x: &Bound<'_, PyAny>,
        y: &Bound<'_, PyAny>,
        z: &Bound<'_, PyAny>,
    ) -> PyResult<Self> {
        crate::custom::ensure_authoring_allowed()?;
        self.require_free_position("scale_to_3d")?;
        let values = [
            extract_scalar_source_for_drawable(x.clone(), &self.0)?,
            extract_scalar_source_for_drawable(y.clone(), &self.0)?,
            extract_scalar_source_for_drawable(z.clone(), &self.0)?,
        ];
        if let (Some(x), Some(y), Some(z)) = (
            values[0].constant_value(),
            values[1].constant_value(),
            values[2].constant_value(),
        ) {
            return Ok(Self(self.0.clone().scale_to_3d(x, y, z)));
        }
        binding_result(self, PropertySources::Scale(values))
    }
    pub(crate) fn scale_by(&self, factor: f64) -> PyResult<Self> {
        crate::custom::ensure_authoring_allowed()?;
        self.require_free_position("scale_by")?;
        free_channel(&self.0, PropertyChannel::Scale)?;
        Ok(Self(self.0.clone().scale_by(factor)))
    }
    pub(crate) fn scale_by_3d(&self, x: f64, y: f64, z: f64) -> PyResult<Self> {
        crate::custom::ensure_authoring_allowed()?;
        self.require_free_position("scale_by_3d")?;
        free_channel(&self.0, PropertyChannel::Scale)?;
        Ok(Self(self.0.clone().scale_by_3d(x, y, z)))
    }
    pub(crate) fn rotate_to(&self, radians: &Bound<'_, PyAny>) -> PyResult<Self> {
        crate::custom::ensure_authoring_allowed()?;
        self.require_free_position("rotate_to")?;
        let source = extract_scalar_source_for_drawable(radians.clone(), &self.0)?;
        if let Some(value) = source.constant_value() {
            return Ok(Self(self.0.clone().rotate_to(value)));
        }
        binding_result(
            self,
            PropertySources::Rotation([0.0.into(), 0.0.into(), source]),
        )
    }
    pub(crate) fn rotate_to_3d(
        &self,
        x: &Bound<'_, PyAny>,
        y: &Bound<'_, PyAny>,
        z: &Bound<'_, PyAny>,
    ) -> PyResult<Self> {
        crate::custom::ensure_authoring_allowed()?;
        self.require_free_position("rotate_to_3d")?;
        let values = [
            extract_scalar_source_for_drawable(x.clone(), &self.0)?,
            extract_scalar_source_for_drawable(y.clone(), &self.0)?,
            extract_scalar_source_for_drawable(z.clone(), &self.0)?,
        ];
        if let (Some(x), Some(y), Some(z)) = (
            values[0].constant_value(),
            values[1].constant_value(),
            values[2].constant_value(),
        ) {
            return Ok(Self(self.0.clone().rotate_to_3d(x, y, z)));
        }
        binding_result(self, PropertySources::Rotation(values))
    }
    pub(crate) fn rotate_by(&self, radians: f64) -> PyResult<Self> {
        crate::custom::ensure_authoring_allowed()?;
        self.require_free_position("rotate_by")?;
        free_channel(&self.0, PropertyChannel::Rotation)?;
        Ok(Self(self.0.clone().rotate_by(radians)))
    }
    pub(crate) fn rotate_by_3d(&self, axis: &str, radians: f64) -> PyResult<Self> {
        crate::custom::ensure_authoring_allowed()?;
        self.require_free_position("rotate_by_3d")?;
        free_channel(&self.0, PropertyChannel::Rotation)?;
        self.0
            .clone()
            .rotate_by_3d(axis, radians)
            .map(Self)
            .map_err(|error| PyValueError::new_err(error.to_string()))
    }
    fn with_pivot(&self, x: f64, y: f64) -> PyResult<Self> {
        crate::custom::ensure_authoring_allowed()?;
        Ok(Self(self.0.clone().with_pivot(x, y)))
    }
    fn with_pivot_3d(&self, x: f64, y: f64, z: f64) -> PyResult<Self> {
        crate::custom::ensure_authoring_allowed()?;
        Ok(Self(self.0.clone().with_pivot_3d(x, y, z)))
    }
    fn pivot(&self, x: f64, y: f64) -> PyResult<Self> {
        crate::custom::ensure_authoring_allowed()?;
        Ok(Self(self.0.clone().pivot(x, y)))
    }
    #[pyo3(signature = (reference, direction, spacing=0.24, aligned_edge=None))]
    fn next_to(
        &self,
        reference: &PyDrawable,
        direction: &PyDirection,
        spacing: f64,
        aligned_edge: Option<&PyAnchor>,
    ) -> PyResult<Self> {
        crate::custom::ensure_authoring_allowed()?;
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
        crate::custom::ensure_authoring_allowed()?;
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
    #[pyo3(signature = (direction, buff=0.24))]
    fn to_edge(&self, direction: &PyDirection, buff: f64) -> PyResult<Self> {
        crate::custom::ensure_authoring_allowed()?;
        self.require_free_position("to_edge")?;
        Ok(Self(self.0.clone().to_edge(direction.0, buff)))
    }
    #[pyo3(signature = (corner, buff=0.24))]
    fn to_corner(&self, corner: &PyAnchor, buff: f64) -> PyResult<Self> {
        crate::custom::ensure_authoring_allowed()?;
        self.require_free_position("to_corner")?;
        Ok(Self(self.0.clone().to_corner(corner.0, buff)))
    }
    // -- Reactive methods --

    /// Attach a preset updater that runs every frame.
    fn add_updater(&self, updater: &PyUpdater) -> PyResult<()> {
        crate::custom::ensure_authoring_allowed()?;
        Ok({
            self.0.add_updater(updater.0.clone());
        })
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
        crate::custom::ensure_authoring_allowed()?;
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
    fn remove_updater(&self) -> PyResult<()> {
        crate::custom::ensure_authoring_allowed()?;
        Ok({
            self.0.remove_updater();
        })
    }

    /// Drive a property of this drawable along a sampled `(times, values)`
    /// series, evaluated natively as a pure function of timeline time — no
    /// per-frame Python callbacks, and exact under seeks and paused scrubbing.
    ///
    /// `property` selects the driven channel: `"x"`, `"y"`, `"z"`,
    /// `"rotation"` (Z radians), `"scale"` (uniform), `"opacity"`, or
    /// `"signal"` (the entity's float signal, composable with expressions).
    /// Translation axes and rotation are relative to the authored pose:
    /// `base + offset + scale * sample`. Scale, opacity, and signal are
    /// absolute: `offset + scale * sample`. Samples outside the series are
    /// clamped to its first/last value.
    ///
    /// ```python
    /// times = [i * 0.02 for i in range(len(accel))]
    /// building.drive_from_samples(times, accel, "x", scale=520.0)
    /// ```
    #[pyo3(signature = (times, values, property = "x", *, interpolation = "linear", scale = 1.0, offset = 0.0))]
    fn drive_from_samples(
        &self,
        times: Vec<f64>,
        values: Vec<f64>,
        property: &str,
        interpolation: &str,
        scale: f64,
        offset: f64,
    ) -> PyResult<Self> {
        crate::custom::ensure_authoring_allowed()?;
        let property = parse_sampled_property(property)?;
        let interpolation = parse_sampled_interpolation(interpolation)?;
        self.0
            .drive_from_samples(times, values, property, interpolation, scale, offset)
            .map(|_| self.clone())
            .map_err(|_| {
                PyValueError::new_err(
                    "drive_from_samples requires non-empty matching times/values, finite values, \
                     and non-decreasing times",
                )
            })
    }

    /// Copy the source entity's Y position each frame.
    fn bind_y_from(&self, source: &PyDrawable) -> PyResult<()> {
        crate::custom::ensure_authoring_allowed()?;
        Ok({
            self.0.bind_y_from(&source.0);
        })
    }

    /// Copy the source entity's X position each frame.
    fn bind_x_from(&self, source: &PyDrawable) -> PyResult<()> {
        crate::custom::ensure_authoring_allowed()?;
        Ok({
            self.0.bind_x_from(&source.0);
        })
    }

    /// Keep this drawable centered on ``source`` each frame.
    fn attach_to(&self, source: &PyDrawable) -> PyResult<()> {
        crate::custom::ensure_authoring_allowed()?;
        Ok({
            self.0.attach_to(&source.0);
        })
    }

    /// Follow ``source`` while keeping an ``(x, y)`` scene-space offset.
    fn follow_to(&self, source: &PyDrawable, offset: (f64, f64)) -> PyResult<()> {
        crate::custom::ensure_authoring_allowed()?;
        Ok({
            self.0.follow_to(&source.0, offset.0, offset.1);
        })
    }

    /// Follow any endpoint and return this drawable for fluent chaining.
    #[pyo3(signature = (source, *, offset=(0.0, 0.0), offset_space="world"))]
    fn follow(
        &self,
        source: Bound<'_, PyAny>,
        offset: (f64, f64),
        offset_space: &str,
    ) -> PyResult<Self> {
        crate::custom::ensure_authoring_allowed()?;
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
        crate::custom::ensure_authoring_allowed()?;
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
        crate::custom::ensure_authoring_allowed()?;
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
        crate::custom::ensure_authoring_allowed()?;
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
}
