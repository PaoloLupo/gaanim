//! Text range animator bindings (TX-02) and the text presets built on it:
//! masked reveals (TX-01), blur-in and tracking (TX-05).

use pyo3::exceptions::{PyTypeError, PyValueError};
use pyo3::prelude::*;

use gaanim_api::anim::{AnimationType, DrawOrder};
use gaanim_api::canvas::{SelectorShape, TextAnimator, TextAnimatorError, TextRevealStyle};
use gaanim_text::prelude::TextRevealUnit;

use crate::color::PyColor;
use crate::pydrawable::PyCanvasAnim;
use crate::pytext::PyText;

fn text_error(error: TextAnimatorError) -> PyErr {
    PyTypeError::new_err(error.to_string())
}

pub(crate) fn parse_unit(by: &str) -> PyResult<TextRevealUnit> {
    Ok(match by {
        "grapheme" => TextRevealUnit::Grapheme,
        "word" => TextRevealUnit::Word,
        "line" => TextRevealUnit::Line,
        "part" => TextRevealUnit::Part,
        _ => {
            return Err(PyValueError::new_err(
                "by must be grapheme, word, line, or part",
            ))
        }
    })
}

fn parse_order(order: &str) -> PyResult<DrawOrder> {
    Ok(match order {
        "forward" => DrawOrder::Forward,
        "reverse" => DrawOrder::Reverse,
        "center" => DrawOrder::Center,
        "random" => DrawOrder::Random,
        _ => {
            return Err(PyValueError::new_err(
                "order must be forward, reverse, center, or random",
            ))
        }
    })
}

fn parse_style(style: &str) -> PyResult<TextRevealStyle> {
    TextRevealStyle::parse(style).ok_or_else(|| {
        PyValueError::new_err(format!(
            "unknown reveal style {style:?}; expected {}",
            TextRevealStyle::NAMES
        ))
    })
}

fn require_non_negative(name: &str, value: f64) -> PyResult<()> {
    if value.is_finite() && value >= 0.0 {
        Ok(())
    } else {
        Err(PyValueError::new_err(format!(
            "{name} must be finite and non-negative"
        )))
    }
}

fn require_finite(name: &str, value: f64) -> PyResult<()> {
    if value.is_finite() {
        Ok(())
    } else {
        Err(PyValueError::new_err(format!("{name} must be finite")))
    }
}

/// Text presets replace the proxy's animation, like any other effect.
fn require_text_effect_slot(anim: &PyCanvasAnim, name: &str) -> PyResult<()> {
    crate::custom::ensure_authoring_allowed()?;
    if anim.inner.property_target_is_text_selection()
        || matches!(anim.inner.inner.anim_type, AnimationType::TextSelection { .. })
    {
        return Err(PyTypeError::new_err(format!(
            "{name}() requires a Text animation proxy, e.g. title.animate.{name}()"
        )));
    }
    if matches!(
        anim.inner.inner.anim_type,
        AnimationType::CustomProperties(_)
    ) || !anim.inner.inner.anim_type.is_empty_properties()
    {
        return Err(PyValueError::new_err(format!(
            "{name}() cannot be combined with property targets or another effect in one Anim; combine separate animations with parallel()"
        )));
    }
    Ok(())
}

/// `Anim.reveal(...)` on a whole Text (the selection form lives in
/// `PyCanvasAnim::reveal`).
pub(crate) fn text_reveal(
    anim: &PyCanvasAnim,
    style: &str,
    by: &str,
    mask: bool,
    stagger: f64,
) -> PyResult<PyCanvasAnim> {
    require_text_effect_slot(anim, "reveal")?;
    let style = parse_style(style)?;
    let unit = parse_unit(by)?;
    require_non_negative("stagger", stagger)?;
    anim.inner
        .clone()
        .text_reveal(unit, style, mask, stagger)
        .map(|inner| PyCanvasAnim { inner })
        .map_err(text_error)
}

/// A reusable range selector over the units of one Text.
#[pyclass(name = "TextAnimator", module = "gaanim_core", skip_from_py_object)]
#[derive(Clone)]
pub struct PyTextAnimator {
    inner: TextAnimator,
}

#[pymethods]
impl PyTextAnimator {
    /// Define the out state reached at full influence; returns self.
    #[pyo3(signature = (*, offset=None, opacity=None, scale=None, rotation=None, blur=None, tracking=None, color=None))]
    #[allow(clippy::too_many_arguments)]
    fn set<'py>(
        slf: PyRef<'py, Self>,
        offset: Option<(f64, f64)>,
        opacity: Option<f32>,
        scale: Option<f64>,
        rotation: Option<f64>,
        blur: Option<f64>,
        tracking: Option<f64>,
        color: Option<PyColor>,
    ) -> PyResult<PyRef<'py, Self>> {
        crate::custom::ensure_authoring_allowed()?;
        if let Some((x, y)) = offset {
            require_finite("offset", x)?;
            require_finite("offset", y)?;
        }
        if let Some(opacity) = opacity {
            if !opacity.is_finite() || !(0.0..=1.0).contains(&opacity) {
                return Err(PyValueError::new_err("opacity must be between zero and one"));
            }
        }
        if let Some(scale) = scale {
            require_non_negative("scale", scale)?;
        }
        if let Some(rotation) = rotation {
            require_finite("rotation", rotation)?;
        }
        if let Some(blur) = blur {
            require_non_negative("blur", blur)?;
        }
        if let Some(tracking) = tracking {
            require_finite("tracking", tracking)?;
        }
        slf.inner.set(|out| {
            if let Some((x, y)) = offset {
                out.offset = gaanim_core::glam::DVec2::new(x, y);
            }
            out.opacity = opacity.or(out.opacity);
            out.scale = scale.or(out.scale);
            out.rotation = rotation.or(out.rotation);
            out.blur = blur.or(out.blur);
            out.tracking = tracking.or(out.tracking);
            out.color = color.map(|color| color.0).or(out.color);
        });
        Ok(slf)
    }

    /// Typed proxy whose `sweep()` plays the animator.
    #[getter]
    fn animate(&self) -> PyResult<PyTextAnimatorAnimation> {
        crate::custom::ensure_authoring_allowed()?;
        Ok(PyTextAnimatorAnimation {
            inner: self.inner.clone(),
        })
    }
}

/// Animation proxy of a [`PyTextAnimator`].
#[pyclass(
    name = "TextAnimatorAnimation",
    module = "gaanim_core",
    skip_from_py_object
)]
#[derive(Clone)]
pub struct PyTextAnimatorAnimation {
    inner: TextAnimator,
}

#[pymethods]
impl PyTextAnimatorAnimation {
    /// Move the range from `start` to `end` over the animation.
    #[pyo3(signature = (start=0.0, end=1.0, *, stagger=None))]
    fn sweep(&self, start: f64, end: f64, stagger: Option<f64>) -> PyResult<PyCanvasAnim> {
        crate::custom::ensure_authoring_allowed()?;
        require_finite("start", start)?;
        require_finite("end", end)?;
        if let Some(stagger) = stagger {
            require_non_negative("stagger", stagger)?;
        }
        Ok(PyCanvasAnim {
            inner: self.inner.sweep(start, end, stagger),
        })
    }
}

#[pymethods]
impl PyText {
    /// Create a range animator over this text's units.
    #[pyo3(signature = (by="grapheme", shape="smooth", order="forward", seed=0))]
    fn animator(&self, by: &str, shape: &str, order: &str, seed: u64) -> PyResult<PyTextAnimator> {
        crate::custom::ensure_authoring_allowed()?;
        let unit = parse_unit(by)?;
        let shape = SelectorShape::parse(shape).ok_or_else(|| {
            PyValueError::new_err(format!("shape must be {}", SelectorShape::NAMES))
        })?;
        let order = parse_order(order)?;
        self.handle
            .animator(unit, shape, order, seed)
            .map(|inner| PyTextAnimator { inner })
            .map_err(text_error)
    }

    /// Immediately set the extra spacing between glyphs, in scene units.
    fn tracking<'py>(slf: PyRef<'py, Self>, value: f64) -> PyResult<PyRef<'py, Self>> {
        crate::custom::ensure_authoring_allowed()?;
        require_finite("tracking", value)?;
        slf.handle.clone().tracking(value).map_err(text_error)?;
        Ok(slf)
    }
}

#[pymethods]
impl PyCanvasAnim {
    /// The exit symmetric to `reveal`.
    #[pyo3(signature = (style="slide_up", *, by="line", mask=true, stagger=0.06))]
    fn conceal(&self, style: &str, by: &str, mask: bool, stagger: f64) -> PyResult<Self> {
        require_text_effect_slot(self, "conceal")?;
        let style = parse_style(style)?;
        let unit = parse_unit(by)?;
        require_non_negative("stagger", stagger)?;
        self.inner
            .clone()
            .text_conceal(unit, style, mask, stagger)
            .map(|inner| Self { inner })
            .map_err(text_error)
    }

    /// Bring units in from a transparent blur.
    #[pyo3(signature = (sigma=0.3, *, by="grapheme", stagger=0.02))]
    fn blur_in(&self, sigma: f64, by: &str, stagger: f64) -> PyResult<Self> {
        require_text_effect_slot(self, "blur_in")?;
        require_non_negative("sigma", sigma)?;
        let unit = parse_unit(by)?;
        require_non_negative("stagger", stagger)?;
        self.inner
            .clone()
            .blur_in(sigma, unit, stagger)
            .map(|inner| Self { inner })
            .map_err(text_error)
    }

    /// Animate the extra spacing between glyphs to `value` scene units.
    fn tracking(&self, value: f64) -> PyResult<Self> {
        require_text_effect_slot(self, "tracking")?;
        require_finite("tracking", value)?;
        self.inner
            .clone()
            .tracking(value)
            .map(|inner| Self { inner })
            .map_err(text_error)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gaanim_api::canvas::SceneModel;

    #[test]
    fn presets_build_text_animator_effects() {
        let mut scene = SceneModel::new(640, 360);
        let text = scene.text("uno dos");
        let proxy = || PyCanvasAnim {
            inner: text.animate(),
        };
        for anim in [
            text_reveal(&proxy(), "slide_up", "line", true, 0.06).unwrap(),
            proxy().conceal("fade", "word", true, 0.04).unwrap(),
            proxy().blur_in(0.3, "grapheme", 0.02).unwrap(),
            proxy().tracking(0.4).unwrap(),
        ] {
            assert!(matches!(
                anim.inner.inner.anim_type,
                AnimationType::TextAnimator(_)
            ));
        }
        assert!(text_reveal(&proxy(), "wipe", "line", true, 0.06).is_err());
        assert!(proxy().blur_in(0.3, "sentence", 0.02).is_err());
        assert!(proxy().tracking(f64::NAN).is_err());
        // Presets replace the animation, so they refuse earlier targets.
        let moved = PyCanvasAnim {
            inner: text.animate().opacity(0.5),
        };
        assert!(moved.blur_in(0.3, "grapheme", 0.02).is_err());
    }

    #[test]
    fn presets_reject_non_text_drawables() {
        let mut scene = SceneModel::new(640, 360);
        let circle = scene.circle(1.0);
        let proxy = PyCanvasAnim {
            inner: circle.animate(),
        };
        pyo3::Python::initialize();
        let error = proxy.blur_in(0.3, "grapheme", 0.02).unwrap_err();
        pyo3::Python::attach(|py| assert!(error.is_instance_of::<PyTypeError>(py)));
    }
}
