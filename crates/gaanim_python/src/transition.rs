use gaanim_core::glam::DVec2;
use gaanim_timeline::transition::{MorphMapping, MorphProperty, SlideDirection, TransitionType};
use pyo3::prelude::*;

/// Python wrapper for scene transition types.
#[pyclass(name = "Transition", module = "gaanim_core", frozen, from_py_object)]
#[derive(Debug, Clone)]
pub struct PyTransitionType(pub TransitionType);

#[pymethods]
impl PyTransitionType {
    /// Instant cut (no transition).
    #[staticmethod]
    fn cut() -> Self {
        Self(TransitionType::Cut)
    }

    /// Cross-fade: outgoing scene fades out, incoming scene fades in.
    #[staticmethod]
    fn cross_fade(duration: f64) -> Self {
        Self(TransitionType::CrossFade { duration })
    }

    /// Fade to a color, then fade in from that color.
    #[staticmethod]
    fn fade_through(duration: f64, color: super::color::PyColor) -> Self {
        Self(TransitionType::FadeThrough {
            duration,
            fade_color: color.0,
        })
    }

    /// Slide transition in a given direction ("left", "right", "up", "down").
    #[staticmethod]
    fn slide(duration: f64, direction: &str) -> PyResult<Self> {
        let dir = match direction.to_lowercase().as_str() {
            "left" => SlideDirection::Left,
            "right" => SlideDirection::Right,
            "up" => SlideDirection::Up,
            "down" => SlideDirection::Down,
            _ => {
                return Err(pyo3::exceptions::PyValueError::new_err(format!(
                    "Invalid slide direction: '{}'. Use left, right, up, or down",
                    direction
                )))
            }
        };
        Ok(Self(TransitionType::Slide {
            duration,
            direction: dir,
        }))
    }

    /// Zoom through a point in the outgoing scene before revealing the next one.
    #[staticmethod]
    #[pyo3(signature = (duration, *, center=(0.0, 0.0), max_zoom=4.0))]
    fn zoom_through(duration: f64, center: (f64, f64), max_zoom: f64) -> PyResult<Self> {
        if !duration.is_finite() || duration <= 0.0 {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "duration must be a finite positive number",
            ));
        }
        if !center.0.is_finite() || !center.1.is_finite() {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "center coordinates must be finite",
            ));
        }
        if !max_zoom.is_finite() || max_zoom <= 0.0 {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "max_zoom must be a finite positive number",
            ));
        }
        Ok(Self(TransitionType::ZoomThrough {
            duration,
            center: DVec2::new(center.0, center.1),
            max_zoom,
        }))
    }

    /// Morph paired drawables of the outgoing segment into the incoming one.
    #[staticmethod]
    #[pyo3(signature = (duration, *, pairs=Vec::new()))]
    fn morph(
        duration: f64,
        pairs: Vec<(
            PyRef<'_, crate::pydrawable::PyDrawable>,
            PyRef<'_, crate::pydrawable::PyDrawable>,
        )>,
    ) -> PyResult<Self> {
        if !duration.is_finite() || duration <= 0.0 {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "duration must be a finite positive number",
            ));
        }
        let mut sources = std::collections::HashSet::new();
        let mut targets = std::collections::HashSet::new();
        let mut mappings = Vec::with_capacity(pairs.len());
        for (source, target) in &pairs {
            let (source, target) = (source.0.id, target.0.id);
            if source == target {
                return Err(pyo3::exceptions::PyValueError::new_err(
                    "a morph pair needs two different drawables",
                ));
            }
            if !sources.insert(source) || !targets.insert(target) {
                return Err(pyo3::exceptions::PyValueError::new_err(
                    "each drawable can appear in at most one morph pair per side",
                ));
            }
            mappings.push(MorphMapping {
                source,
                target,
                property: MorphProperty::All,
            });
        }
        Ok(Self(TransitionType::Morph { duration, mappings }))
    }

    fn __repr__(&self) -> String {
        match &self.0 {
            TransitionType::Cut => "Transition.cut()".to_string(),
            TransitionType::CrossFade { duration } => {
                format!("Transition.cross_fade({})", duration)
            }
            TransitionType::FadeThrough { duration, .. } => {
                format!("Transition.fade_through({})", duration)
            }
            TransitionType::Slide {
                duration,
                direction,
            } => format!("Transition.slide({:?}, {})", direction, duration),
            TransitionType::ZoomThrough { duration, .. } => {
                format!("Transition.zoom_through({})", duration)
            }
            TransitionType::Morph { duration, mappings } => format!(
                "Transition.morph({}, pairs=<{} pairs>)",
                duration,
                mappings.len()
            ),
        }
    }
}
