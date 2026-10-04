use gaanim_core::glam::DVec2;
use gaanim_timeline::sound::SoundCue;
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

/// Validate the optional `sound=` file of a transition constructor.
fn sound_cue(sound: Option<String>) -> PyResult<Option<SoundCue>> {
    sound
        .map(SoundCue::new)
        .transpose()
        .map_err(|error| PyValueError::new_err(error.to_string()))
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
    /// Attach the optional easing, overlay and sound shared by every constructor.
    fn styled(
        transition: TransitionType,
        easing: Option<PyEasing>,
        overlay: Option<PyOverlay>,
        sound: Option<SoundCue>,
    ) -> Self {
        let mut transition = transition;
        if let Some(easing) = easing {
            transition = transition.with_easing(easing.inner);
        }
        if let Some(overlay) = overlay {
            transition = transition.with_overlay(overlay.0);
        }
        if let Some(sound) = sound {
            transition = transition.with_sound(sound);
        }
        Self(transition)
    }
}

#[pymethods]
impl PyTransitionType {
    /// Instant cut (no transition), optionally decorated by an overlay.
    #[staticmethod]
    #[pyo3(signature = (*, overlay=None, sound=None))]
    fn cut(overlay: Option<PyOverlay>, sound: Option<String>) -> PyResult<Self> {
        Ok(Self::styled(
            TransitionType::Cut,
            None,
            overlay,
            sound_cue(sound)?,
        ))
    }

    /// Cross-fade: outgoing scene fades out, incoming scene fades in.
    #[staticmethod]
    #[pyo3(signature = (duration, *, easing=None, overlay=None, sound=None))]
    fn cross_fade(
        duration: f64,
        easing: Option<PyEasing>,
        overlay: Option<PyOverlay>,
        sound: Option<String>,
    ) -> PyResult<Self> {
        Ok(Self::styled(
            TransitionType::CrossFade { duration },
            easing,
            overlay,
            sound_cue(sound)?,
        ))
    }

    /// Fade to a color, then fade in from that color.
    #[staticmethod]
    #[pyo3(signature = (duration, color, *, easing=None, overlay=None, sound=None))]
    fn fade_through(
        duration: f64,
        color: super::color::PyColor,
        easing: Option<PyEasing>,
        overlay: Option<PyOverlay>,
        sound: Option<String>,
    ) -> PyResult<Self> {
        Ok(Self::styled(
            TransitionType::FadeThrough {
                duration,
                fade_color: color.0,
            },
            easing,
            overlay,
            sound_cue(sound)?,
        ))
    }

    /// The incoming scene slides in over the outgoing one ("left", "right", "up", "down").
    #[staticmethod]
    #[pyo3(signature = (duration, direction, *, easing=None, overlay=None, sound=None))]
    fn slide(
        duration: f64,
        direction: &str,
        easing: Option<PyEasing>,
        overlay: Option<PyOverlay>,
        sound: Option<String>,
    ) -> PyResult<Self> {
        let direction = slide_direction(direction)?;
        Ok(Self::styled(
            TransitionType::Slide {
                duration,
                direction,
            },
            easing,
            overlay,
            sound_cue(sound)?,
        ))
    }

    /// Zoom through a point in the outgoing scene before revealing the next one.
    #[staticmethod]
    #[pyo3(signature = (duration, *, center=(0.0, 0.0), max_zoom=4.0, easing=None, overlay=None, sound=None))]
    fn zoom_through(
        duration: f64,
        center: (f64, f64),
        max_zoom: f64,
        easing: Option<PyEasing>,
        overlay: Option<PyOverlay>,
        sound: Option<String>,
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
            sound_cue(sound)?,
        ))
    }

    /// Morph paired drawables of the outgoing segment into the incoming one.
    #[staticmethod]
    #[pyo3(signature = (duration, *, pairs=Vec::new(), easing=None, overlay=None, sound=None))]
    fn morph(
        duration: f64,
        pairs: Vec<(
            PyRef<'_, crate::pydrawable::PyDrawable>,
            PyRef<'_, crate::pydrawable::PyDrawable>,
        )>,
        easing: Option<PyEasing>,
        overlay: Option<PyOverlay>,
        sound: Option<String>,
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
            sound_cue(sound)?,
        ))
    }

    /// Keyed morph: drawables of both segments that share a key morph into
    /// each other, the rest cross-fade. `key` is "name" or "id".
    #[staticmethod]
    #[pyo3(signature = (duration, *, key="name", easing=None, overlay=None, sound=None))]
    fn magic_move(
        duration: f64,
        key: &str,
        easing: Option<PyEasing>,
        overlay: Option<PyOverlay>,
        sound: Option<String>,
    ) -> PyResult<Self> {
        positive_duration(duration)?;
        let key = crate::magic_move::parse_key(key)?;
        Ok(Self::styled(
            TransitionType::magic_move(duration, key),
            easing,
            overlay,
            sound_cue(sound)?,
        ))
    }

    /// A straight edge travels across the frame in `direction`, revealing the next segment.
    #[staticmethod]
    #[pyo3(signature = (duration, direction="left", feather=0.1, *, easing=None, overlay=None, sound=None))]
    fn wipe(
        duration: f64,
        direction: &str,
        feather: f64,
        easing: Option<PyEasing>,
        overlay: Option<PyOverlay>,
        sound: Option<String>,
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
            sound_cue(sound)?,
        ))
    }

    /// A clock hand sweeps clockwise from `start_angle` degrees (90 = twelve o'clock).
    #[staticmethod]
    #[pyo3(signature = (duration, start_angle=90.0, *, easing=None, overlay=None, sound=None))]
    fn clock_wipe(
        duration: f64,
        start_angle: f64,
        easing: Option<PyEasing>,
        overlay: Option<PyOverlay>,
        sound: Option<String>,
    ) -> PyResult<Self> {
        positive_duration(duration)?;
        finite("start_angle", start_angle)?;
        Ok(Self::styled(
            TransitionType::clock_wipe(duration, start_angle),
            easing,
            overlay,
            sound_cue(sound)?,
        ))
    }

    /// A shape opens from `center` until the next segment covers the frame.
    #[staticmethod]
    #[pyo3(signature = (duration, center=(0.0, 0.0), shape=None, *, easing=None, overlay=None, sound=None))]
    fn iris(
        duration: f64,
        center: (f64, f64),
        shape: Option<&Bound<'_, PyAny>>,
        easing: Option<PyEasing>,
        overlay: Option<PyOverlay>,
        sound: Option<String>,
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
            sound_cue(sound)?,
        ))
    }

    /// `count` parallel slats open together; `angle` tilts the slats in degrees.
    #[staticmethod]
    #[pyo3(signature = (duration, count=8, angle=0.0, *, easing=None, overlay=None, sound=None))]
    fn blinds(
        duration: f64,
        count: u32,
        angle: f64,
        easing: Option<PyEasing>,
        overlay: Option<PyOverlay>,
        sound: Option<String>,
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
            sound_cue(sound)?,
        ))
    }

    /// The incoming segment pushes the outgoing one out of the frame.
    #[staticmethod]
    #[pyo3(signature = (duration, direction="up", *, easing=None, overlay=None, sound=None))]
    fn push(
        duration: f64,
        direction: &str,
        easing: Option<PyEasing>,
        overlay: Option<PyOverlay>,
        sound: Option<String>,
    ) -> PyResult<Self> {
        positive_duration(duration)?;
        let direction = slide_direction(direction)?;
        Ok(Self::styled(
            TransitionType::push(duration, direction),
            easing,
            overlay,
            sound_cue(sound)?,
        ))
    }

    /// A WGSL transition: `source` (inline text or an os.PathLike `.wgsl`
    /// file) defines `fn transition(uv: vec2<f32>) -> vec4<f32>` over
    /// `gaanim_from(uv)`, `gaanim_to(uv)` and `progress`.
    #[staticmethod]
    #[pyo3(signature = (source, duration, *, uniforms=None, easing=None, overlay=None, sound=None))]
    fn shader(
        source: &Bound<'_, PyAny>,
        duration: f64,
        uniforms: Option<&Bound<'_, pyo3::types::PyMapping>>,
        easing: Option<PyEasing>,
        overlay: Option<PyOverlay>,
        sound: Option<String>,
    ) -> PyResult<Self> {
        positive_duration(duration)?;
        let source = if let Ok(source) = source.extract::<String>() {
            source
        } else {
            let path = source.extract::<std::path::PathBuf>().map_err(|_| {
                pyo3::exceptions::PyTypeError::new_err(
                    "source must be inline WGSL text or an os.PathLike .wgsl file",
                )
            })?;
            std::fs::read_to_string(&path).map_err(|error| {
                PyValueError::new_err(format!(
                    "could not read the transition WGSL {}: {error}",
                    path.display()
                ))
            })?
        };
        let mut names = Vec::new();
        let mut values = Vec::new();
        let mut sources = Vec::new();
        let mut reactive = false;
        if let Some(uniforms) = uniforms {
            for item in uniforms.items()?.iter() {
                let (name, value) = item.extract::<(Bound<'_, PyAny>, Bound<'_, PyAny>)>()?;
                names.push(name.extract::<String>().map_err(|_| {
                    pyo3::exceptions::PyTypeError::new_err("uniform names must be strings")
                })?);
                if let Ok(number) = value.extract::<f64>() {
                    finite("uniform values", number)?;
                    values.push(number as f32);
                    sources.push(gaanim_animation::ScalarSource::constant(number));
                } else {
                    let source = crate::visualization::extract_deferred_scalar(value).map_err(|_| {
                        pyo3::exceptions::PyTypeError::new_err(
                            "uniform values must be numbers, Parameters, Variables, Computeds or scene.time",
                        )
                    })?;
                    values.push(0.0);
                    sources.push(source.source);
                    reactive = true;
                }
            }
        }
        let uniform_sources: Vec<gaanim_animation::ResolvedScalarSource> = if reactive {
            sources
                .into_iter()
                .map(|source| gaanim_animation::ResolvedScalarSource {
                    source,
                    parameters: Vec::new(),
                })
                .collect()
        } else {
            Vec::new()
        };
        gaanim_api::canvas::PostProcessShader::transition(source.as_str(), &names, None)
            .map_err(|error| PyValueError::new_err(error.to_string()))?;
        let shader = gaanim_scene::TransitionShader {
            source,
            uniforms: names,
            values,
            data: None,
        };
        Ok(Self::styled(
            TransitionType::Shader {
                duration,
                shader: std::sync::Arc::new(shader),
                uniforms: uniform_sources,
            },
            easing,
            overlay,
            sound_cue(sound)?,
        ))
    }

    /// A built-in shader transition: `cross_zoom`, `directional_warp`,
    /// `ripple`, `glitch_displace` or `luma`, tuned by keyword settings.
    #[staticmethod]
    #[pyo3(signature = (name, duration, *, image=None, invert=false, easing=None, overlay=None, sound=None, **settings))]
    #[allow(clippy::too_many_arguments)]
    fn preset(
        name: &str,
        duration: f64,
        image: Option<std::path::PathBuf>,
        invert: bool,
        easing: Option<PyEasing>,
        overlay: Option<PyOverlay>,
        sound: Option<String>,
        settings: Option<&Bound<'_, pyo3::types::PyDict>>,
    ) -> PyResult<Self> {
        use gaanim_api::canvas::TransitionPreset;
        positive_duration(duration)?;
        let preset = TransitionPreset::from_name(name).ok_or_else(|| {
            let names: Vec<&str> = TransitionPreset::ALL.iter().map(|p| p.name()).collect();
            PyValueError::new_err(format!(
                "unknown transition preset {name:?}; expected one of {}",
                names.join(", ")
            ))
        })?;
        let mut values = Vec::new();
        if let Some(settings) = settings {
            for (key, value) in settings.iter() {
                let key = key.extract::<String>()?;
                let value: f64 = value.extract().map_err(|_| {
                    pyo3::exceptions::PyTypeError::new_err(format!("{key} must be a number"))
                })?;
                finite(&key, value)?;
                values.push((key, value as f32));
            }
        }
        let map = match (preset, image) {
            (TransitionPreset::Luma, Some(path)) => Some(
                gaanim_api::canvas::luma_map_from_file(&path, invert)
                    .map_err(PyValueError::new_err)?,
            ),
            (TransitionPreset::Luma, None) => {
                return Err(PyValueError::new_err(
                    "the luma transition needs image=, a grayscale reveal map",
                ));
            }
            (_, Some(_)) => {
                return Err(PyValueError::new_err(
                    "image= only applies to the luma transition",
                ));
            }
            (_, None) => None,
        };
        let values: Vec<(&str, f32)> = values
            .iter()
            .map(|(key, value)| (key.as_str(), *value))
            .collect();
        let shader = preset.shader(&values, map).map_err(PyValueError::new_err)?;
        Ok(Self::styled(
            TransitionType::Shader {
                duration,
                shader: std::sync::Arc::new(shader),
                uniforms: Vec::new(),
            },
            easing,
            overlay,
            sound_cue(sound)?,
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
            TransitionType::MagicMove { duration, key } => {
                format!("Transition.magic_move({}, key={:?}", duration, key.as_str())
            }
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
            TransitionType::Shader { duration, .. } => {
                format!("Transition.shader({}", duration)
            }
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
        if let Some(sound) = self.0.sound() {
            extras.push_str(&format!(", sound={:?}", sound.path.display().to_string()));
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
