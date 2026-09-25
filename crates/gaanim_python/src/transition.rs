use gaanim_core::glam::DVec2;
use gaanim_timeline::transition::{
    IrisShape, MorphMapping, MorphProperty, SlideDirection, TransitionOverlay, TransitionType,
};
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;

use crate::easing::PyEasing;

/// Python wrapper for scene transition types.
#[pyclass(name = "Transition", module = "gaanim_core", frozen, from_py_object)]
#[derive(Debug, Clone)]
pub struct PyTransitionType(pub TransitionType);

/// Python wrapper for overlays drawn above a transition.
#[pyclass(name = "Overlay", module = "gaanim_core", frozen, from_py_object)]
#[derive(Debug, Clone)]
pub struct PyOverlay(pub TransitionOverlay);

fn positive_duration(duration: f64) -> PyResult<()> {
    if !duration.is_finite() || duration <= 0.0 {
        return Err(PyValueError::new_err(
            "duration must be a finite positive number",
        ));
    }
    Ok(())
}

fn finite(name: &str, value: f64) -> PyResult<()> {
    if !value.is_finite() {
        return Err(PyValueError::new_err(format!("{name} must be finite")));
    }
    Ok(())
}

fn slide_direction(direction: &str) -> PyResult<SlideDirection> {
    match direction.to_lowercase().as_str() {
        "left" => Ok(SlideDirection::Left),
        "right" => Ok(SlideDirection::Right),
        "up" => Ok(SlideDirection::Up),
        "down" => Ok(SlideDirection::Down),
        _ => Err(PyValueError::new_err(format!(
            "Invalid slide direction: '{}'. Use left, right, up, or down",
            direction
        ))),
    }
}

fn wipe_direction(direction: &str) -> PyResult<DVec2> {
    let normalized = direction.to_lowercase().replace(['-', ' '], "_");
    let vector = match normalized.as_str() {
        "left" => DVec2::NEG_X,
        "right" => DVec2::X,
        "up" => DVec2::Y,
        "down" => DVec2::NEG_Y,
        "up_left" => DVec2::new(-1.0, 1.0),
        "up_right" => DVec2::new(1.0, 1.0),
        "down_left" => DVec2::new(-1.0, -1.0),
        "down_right" => DVec2::new(1.0, -1.0),
        _ => {
            return Err(PyValueError::new_err(format!(
                "Invalid wipe direction: '{direction}'. Use left, right, up, down, \
                 up_left, up_right, down_left or down_right"
            )));
        }
    };
    Ok(vector.normalize())
}

impl PyTransitionType {
    /// Attach the optional easing and overlay shared by every constructor.
    fn styled(
        transition: TransitionType,
        easing: Option<PyEasing>,
        overlay: Option<PyOverlay>,
    ) -> Self {
        let mut transition = transition;
        if let Some(easing) = easing {
            transition = transition.with_easing(easing.inner);
        }
        if let Some(overlay) = overlay {
            transition = transition.with_overlay(overlay.0);
        }
        Self(transition)
    }
}

#[pymethods]
impl PyTransitionType {
    /// Instant cut (no transition), optionally decorated by an overlay.
    #[staticmethod]
    #[pyo3(signature = (*, overlay=None))]
    fn cut(overlay: Option<PyOverlay>) -> Self {
        Self::styled(TransitionType::Cut, None, overlay)
    }

    /// Cross-fade: outgoing scene fades out, incoming scene fades in.
    #[staticmethod]
    #[pyo3(signature = (duration, *, easing=None, overlay=None))]
    fn cross_fade(duration: f64, easing: Option<PyEasing>, overlay: Option<PyOverlay>) -> Self {
        Self::styled(TransitionType::CrossFade { duration }, easing, overlay)
    }

    /// Fade to a color, then fade in from that color.
    #[staticmethod]
    #[pyo3(signature = (duration, color, *, easing=None, overlay=None))]
    fn fade_through(
        duration: f64,
        color: super::color::PyColor,
        easing: Option<PyEasing>,
        overlay: Option<PyOverlay>,
    ) -> Self {
        Self::styled(
            TransitionType::FadeThrough {
                duration,
                fade_color: color.0,
            },
            easing,
            overlay,
        )
    }

    /// The incoming scene slides in over the outgoing one ("left", "right", "up", "down").
    #[staticmethod]
    #[pyo3(signature = (duration, direction, *, easing=None, overlay=None))]
    fn slide(
        duration: f64,
        direction: &str,
        easing: Option<PyEasing>,
        overlay: Option<PyOverlay>,
    ) -> PyResult<Self> {
        let direction = slide_direction(direction)?;
        Ok(Self::styled(
            TransitionType::Slide {
                duration,
                direction,
            },
            easing,
            overlay,
        ))
    }

    /// Zoom through a point in the outgoing scene before revealing the next one.
    #[staticmethod]
    #[pyo3(signature = (duration, *, center=(0.0, 0.0), max_zoom=4.0, easing=None, overlay=None))]
    fn zoom_through(
        duration: f64,
        center: (f64, f64),
        max_zoom: f64,
        easing: Option<PyEasing>,
        overlay: Option<PyOverlay>,
    ) -> PyResult<Self> {
        positive_duration(duration)?;
        if !center.0.is_finite() || !center.1.is_finite() {
            return Err(PyValueError::new_err("center coordinates must be finite"));
        }
        if !max_zoom.is_finite() || max_zoom <= 0.0 {
            return Err(PyValueError::new_err(
                "max_zoom must be a finite positive number",
            ));
        }
        Ok(Self::styled(
            TransitionType::ZoomThrough {
                duration,
                center: DVec2::new(center.0, center.1),
                max_zoom,
            },
            easing,
            overlay,
        ))
    }

    /// Morph paired drawables of the outgoing segment into the incoming one.
    #[staticmethod]
    #[pyo3(signature = (duration, *, pairs=Vec::new(), easing=None, overlay=None))]
    fn morph(
        duration: f64,
        pairs: Vec<(
            PyRef<'_, crate::pydrawable::PyDrawable>,
            PyRef<'_, crate::pydrawable::PyDrawable>,
        )>,
        easing: Option<PyEasing>,
        overlay: Option<PyOverlay>,
    ) -> PyResult<Self> {
        positive_duration(duration)?;
        let mut sources = std::collections::HashSet::new();
        let mut targets = std::collections::HashSet::new();
        let mut mappings = Vec::with_capacity(pairs.len());
        for (source, target) in &pairs {
            let (source, target) = (source.0.id, target.0.id);
            if source == target {
                return Err(PyValueError::new_err(
                    "a morph pair needs two different drawables",
                ));
            }
            if !sources.insert(source) || !targets.insert(target) {
                return Err(PyValueError::new_err(
                    "each drawable can appear in at most one morph pair per side",
                ));
            }
            mappings.push(MorphMapping {
                source,
                target,
                property: MorphProperty::All,
            });
        }
        Ok(Self::styled(
            TransitionType::Morph { duration, mappings },
            easing,
            overlay,
        ))
    }

    /// A straight edge travels across the frame in `direction`, revealing the next segment.
    #[staticmethod]
    #[pyo3(signature = (duration, direction="left", feather=0.1, *, easing=None, overlay=None))]
    fn wipe(
        duration: f64,
        direction: &str,
        feather: f64,
        easing: Option<PyEasing>,
        overlay: Option<PyOverlay>,
    ) -> PyResult<Self> {
        positive_duration(duration)?;
        let direction = wipe_direction(direction)?;
        if !feather.is_finite() || !(0.0..=1.0).contains(&feather) {
            return Err(PyValueError::new_err("feather must be between 0 and 1"));
        }
        Ok(Self::styled(
            TransitionType::wipe(duration, direction, feather),
            easing,
            overlay,
        ))
    }

    /// A clock hand sweeps clockwise from `start_angle` degrees (90 = twelve o'clock).
    #[staticmethod]
    #[pyo3(signature = (duration, start_angle=90.0, *, easing=None, overlay=None))]
    fn clock_wipe(
        duration: f64,
        start_angle: f64,
        easing: Option<PyEasing>,
        overlay: Option<PyOverlay>,
    ) -> PyResult<Self> {
        positive_duration(duration)?;
        finite("start_angle", start_angle)?;
        Ok(Self::styled(
            TransitionType::clock_wipe(duration, start_angle),
            easing,
            overlay,
        ))
    }

    /// A shape opens from `center` until the next segment covers the frame.
    #[staticmethod]
    #[pyo3(signature = (duration, center=(0.0, 0.0), shape=None, *, easing=None, overlay=None))]
    fn iris(
        duration: f64,
        center: (f64, f64),
        shape: Option<&Bound<'_, PyAny>>,
        easing: Option<PyEasing>,
        overlay: Option<PyOverlay>,
    ) -> PyResult<Self> {
        positive_duration(duration)?;
        finite("center x", center.0)?;
        finite("center y", center.1)?;
        let shape = match shape {
            None => IrisShape::Circle,
            Some(shape) => {
                if let Ok(name) = shape.extract::<String>() {
                    match name.to_lowercase().as_str() {
                        "circle" => IrisShape::Circle,
                        "diamond" => IrisShape::Diamond,
                        "square" => IrisShape::Square,
                        "star" => IrisShape::Star,
                        _ => {
                            return Err(PyValueError::new_err(format!(
                                "Invalid iris shape: '{name}'. Use circle, diamond, square, \
                                 star or a Drawable"
                            )));
                        }
                    }
                } else if let Ok(drawable) =
                    shape.extract::<PyRef<'_, crate::pydrawable::PyDrawable>>()
                {
                    IrisShape::Drawable(drawable.0.id)
                } else {
                    return Err(pyo3::exceptions::PyTypeError::new_err(
                        "shape must be a shape name or a Drawable",
                    ));
                }
            }
        };
        Ok(Self::styled(
            TransitionType::iris(duration, DVec2::new(center.0, center.1), shape),
            easing,
            overlay,
        ))
    }

    /// `count` parallel slats open together; `angle` tilts the slats in degrees.
    #[staticmethod]
    #[pyo3(signature = (duration, count=8, angle=0.0, *, easing=None, overlay=None))]
    fn blinds(
        duration: f64,
        count: u32,
        angle: f64,
        easing: Option<PyEasing>,
        overlay: Option<PyOverlay>,
    ) -> PyResult<Self> {
        positive_duration(duration)?;
        if !(1..=512).contains(&count) {
            return Err(PyValueError::new_err("count must be between 1 and 512"));
        }
        finite("angle", angle)?;
        Ok(Self::styled(
            TransitionType::blinds(duration, count, angle),
            easing,
            overlay,
        ))
    }

    /// The incoming segment pushes the outgoing one out of the frame.
    #[staticmethod]
    #[pyo3(signature = (duration, direction="up", *, easing=None, overlay=None))]
    fn push(
        duration: f64,
        direction: &str,
        easing: Option<PyEasing>,
        overlay: Option<PyOverlay>,
    ) -> PyResult<Self> {
        positive_duration(duration)?;
        let direction = slide_direction(direction)?;
        Ok(Self::styled(
            TransitionType::push(duration, direction),
            easing,
            overlay,
        ))
    }

    fn __repr__(&self) -> String {
        let base = match self.0.base() {
            TransitionType::Cut => "Transition.cut(".to_string(),
            TransitionType::CrossFade { duration } => {
                format!("Transition.cross_fade({}", duration)
            }
            TransitionType::FadeThrough { duration, .. } => {
                format!("Transition.fade_through({}", duration)
            }
            TransitionType::Slide {
                duration,
                direction,
            } => format!("Transition.slide({:?}, {}", direction, duration),
            TransitionType::ZoomThrough { duration, .. } => {
                format!("Transition.zoom_through({}", duration)
            }
            TransitionType::Morph { duration, mappings } => format!(
                "Transition.morph({}, pairs=<{} pairs>",
                duration,
                mappings.len()
            ),
            TransitionType::Wipe {
                duration, feather, ..
            } => format!("Transition.wipe({}, feather={}", duration, feather),
            TransitionType::ClockWipe {
                duration,
                start_angle,
            } => format!(
                "Transition.clock_wipe({}, start_angle={}",
                duration, start_angle
            ),
            TransitionType::Iris {
                duration, shape, ..
            } => format!("Transition.iris({}, shape={:?}", duration, shape),
            TransitionType::Blinds {
                duration,
                count,
                angle,
            } => format!(
                "Transition.blinds({}, count={}, angle={}",
                duration, count, angle
            ),
            TransitionType::Push {
                duration,
                direction,
            } => format!("Transition.push({}, {:?}", duration, direction),
            TransitionType::Styled { .. } => "Transition(".to_string(),
        };
        let mut extras = String::new();
        if matches!(
            &self.0,
            TransitionType::Styled {
                easing: Some(_),
                ..
            }
        ) {
            extras.push_str(", easing=...");
        }
        if let Some(overlay) = self.0.overlay() {
            extras.push_str(&format!(", overlay={}", overlay_repr(overlay)));
        }
        if base.ends_with('(') {
            format!("{}{})", base, extras.trim_start_matches(", "))
        } else {
            format!("{}{})", base, extras)
        }
    }
}

fn overlay_repr(overlay: &TransitionOverlay) -> String {
    match overlay {
        TransitionOverlay::Flash { duration, .. } => format!("Overlay.flash(duration={duration})"),
        TransitionOverlay::LightLeak {
            seed,
            hue,
            duration,
            intensity,
        } => format!(
            "Overlay.light_leak(seed={seed}, hue={hue}, duration={duration}, intensity={intensity})"
        ),
    }
}

#[pymethods]
impl PyOverlay {
    /// A full-frame flash of `color` peaking at the cut (default white).
    #[staticmethod]
    #[pyo3(signature = (color=None, duration=0.2))]
    fn flash(color: Option<super::color::PyColor>, duration: f64) -> PyResult<Self> {
        positive_duration(duration)?;
        let color = color.map_or(gaanim_core::peniko::Color::WHITE, |color| color.0);
        Ok(Self(TransitionOverlay::flash(color, duration)))
    }

    /// Deterministic warm light blobs drifting across the frame around the cut.
    #[staticmethod]
    #[pyo3(signature = (seed=0, hue=0.1, duration=0.8, intensity=0.8))]
    fn light_leak(seed: u64, hue: f64, duration: f64, intensity: f64) -> PyResult<Self> {
        positive_duration(duration)?;
        finite("hue", hue)?;
        if !intensity.is_finite() || !(0.0..=4.0).contains(&intensity) {
            return Err(PyValueError::new_err("intensity must be between 0 and 4"));
        }
        Ok(Self(TransitionOverlay::light_leak(
            seed, hue, duration, intensity,
        )))
    }

    fn __repr__(&self) -> String {
        overlay_repr(&self.0)
    }
}
