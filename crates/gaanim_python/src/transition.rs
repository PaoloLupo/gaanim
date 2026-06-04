use pyo3::prelude::*;
use gaanim_timeline::transition::{TransitionType, SlideDirection};

/// Python wrapper for scene transition types.
#[pyclass(name = "Transition", module = "gaanim_core", frozen)]
#[derive(Debug, Clone)]
pub struct PyTransitionType(pub TransitionType);

impl PyTransitionType {
    /// Instant cut (no transition) — Rust-internal constructor.
    pub fn cut_transition() -> Self {
        Self(TransitionType::Cut)
    }
}

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
    fn fade_through(duration: f64, color: &super::color::PyColor) -> Self {
        Self(TransitionType::FadeThrough { duration, fade_color: color.0 })
    }

    /// Slide transition in a given direction ("left", "right", "up", "down").
    #[staticmethod]
    fn slide(duration: f64, direction: &str) -> PyResult<Self> {
        let dir = match direction.to_lowercase().as_str() {
            "left" => SlideDirection::Left,
            "right" => SlideDirection::Right,
            "up" => SlideDirection::Up,
            "down" => SlideDirection::Down,
            _ => return Err(pyo3::exceptions::PyValueError::new_err(
                format!("Invalid slide direction: '{}'. Use left, right, up, or down", direction)
            )),
        };
        Ok(Self(TransitionType::Slide { duration, direction: dir }))
    }

    fn __repr__(&self) -> String {
        match &self.0 {
            TransitionType::Cut => "Transition.cut()".to_string(),
            TransitionType::CrossFade { duration } => format!("Transition.cross_fade({})", duration),
            TransitionType::FadeThrough { duration, .. } => format!("Transition.fade_through({})", duration),
            TransitionType::Slide { duration, direction } => format!("Transition.slide({:?}, {})", direction, duration),
            TransitionType::ZoomThrough { duration, .. } => format!("Transition.zoom_through({})", duration),
            TransitionType::Morph { duration, .. } => format!("Transition.morph({})", duration),
        }
    }
}
