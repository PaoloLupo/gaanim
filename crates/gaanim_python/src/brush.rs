use gaanim_core::peniko::{self, Brush, Extend, Gradient};
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use std::path::PathBuf;

use crate::color::PyColor;

/// Full scene-bounds paint, including timeline-driven custom WGSL shaders.
#[pyclass(name = "Background", module = "gaanim_core", skip_from_py_object)]
#[derive(Clone, Debug)]
pub struct PyBackground(pub gaanim_api::canvas::BackgroundPaint);

/// Accept a Background, Brush, or any value accepted by Color.
#[derive(Clone, Debug)]
pub struct PyBackgroundInput(pub gaanim_api::canvas::BackgroundPaint);

impl<'a, 'py> FromPyObject<'a, 'py> for PyBackgroundInput {
    type Error = PyErr;

    fn extract(obj: pyo3::Borrowed<'a, 'py, PyAny>) -> Result<Self, Self::Error> {
        if let Ok(background) = obj.cast::<PyBackground>() {
            return Ok(Self(background.borrow().0.clone()));
        }
        PyPaint::extract(obj).map(|paint| Self(gaanim_api::canvas::BackgroundPaint::Brush(paint.0)))
    }
}

#[pymethods]
impl PyBackground {
    #[new]
    fn new(paint: PyPaint) -> Self {
        Self(gaanim_api::canvas::BackgroundPaint::Brush(paint.0))
    }

    /// Build a WGSL background evaluated with exact timeline time, with
    /// optional uniforms and an audio track's analysis.
    #[staticmethod]
    #[pyo3(signature = (source, *, fallback=None, uniforms=None, audio=None))]
    fn shader(
        py: Python<'_>,
        source: &Bound<'_, PyAny>,
        fallback: Option<PyColor>,
        uniforms: Option<&Bound<'_, pyo3::types::PyDict>>,
        audio: Option<PyRef<'_, crate::pycanvas::PyAudio>>,
    ) -> PyResult<Self> {
        let fallback = fallback.map_or(peniko::Color::BLACK, |color| color.0);
        let mut text = if let Ok(source) = source.extract::<String>() {
            source
        } else {
            let path = source.extract::<PathBuf>().map_err(|_| {
                pyo3::exceptions::PyTypeError::new_err(
                    "source must be inline WGSL text or an os.PathLike .wgsl asset",
                )
            })?;
            std::fs::read_to_string(&path).map_err(|error| {
                pyo3::exceptions::PyRuntimeError::new_err(
                    gaanim_api::canvas::ShaderBackgroundError::ReadSource {
                        path: path.clone(),
                        message: error.to_string(),
                    }
                    .to_string(),
                )
            })?
        };
        let mut named = Vec::new();
        if let Some(uniforms) = uniforms {
            for (name, value) in uniforms.iter() {
                let name = name.extract::<String>().map_err(|_| {
                    pyo3::exceptions::PyTypeError::new_err("uniform names must be strings")
                })?;
                named.push((
                    name,
                    crate::visualization::extract_deferred_scalar(value)?.source,
                ));
            }
        }
        let mut data = None;
        if let Some(audio) = audio {
            let name = gaanim_api::canvas::AUDIO_TIME_UNIFORM;
            if named.iter().any(|(existing, _)| existing == name) {
                return Err(PyValueError::new_err(format!(
                    "the uniform {name:?} is reserved for audio="
                )));
            }
            let (audio_data, time) = audio.shader_input(py)?;
            named.push((name.to_string(), time.source));
            data = Some(audio_data);
            text = format!("{}\n{text}", gaanim_api::canvas::AUDIO_SHADER_FUNCTIONS);
        }
        gaanim_api::canvas::ShaderBackground::with_uniforms(text, fallback, named, data)
            .map(gaanim_api::canvas::BackgroundPaint::Shader)
            .map(Self)
            .map_err(|error| PyValueError::new_err(error.to_string()))
    }

    /// Soft color blobs drifting on seeded orbits, blended like a mesh gradient.
    #[staticmethod]
    #[pyo3(signature = (colors, *, speed=0.1, seed=0))]
    fn mesh_gradient(colors: Vec<PyColor>, speed: f64, seed: u32) -> PyResult<Self> {
        living_background(gaanim_api::canvas::background_presets::mesh_gradient(
            &colors_of(colors),
            speed,
            seed,
        ))
    }

    /// Domain-warped noise mapped across a color ramp, like a Stripe gradient.
    #[staticmethod]
    #[pyo3(signature = (colors, *, scale=1.5, speed=0.05, seed=0))]
    fn noise_gradient(colors: Vec<PyColor>, scale: f64, speed: f64, seed: u32) -> PyResult<Self> {
        living_background(gaanim_api::canvas::background_presets::noise_gradient(
            &colors_of(colors),
            scale,
            speed,
            seed,
        ))
    }

    /// Northern-lights curtains, one per color, over a dark `sky`.
    #[staticmethod]
    #[pyo3(signature = (colors, *, sky=None, speed=0.2, seed=0))]
    fn aurora(colors: Vec<PyColor>, sky: Option<PyColor>, speed: f64, seed: u32) -> PyResult<Self> {
        let sky = sky.map_or(peniko::Color::from_rgb8(0x05, 0x08, 0x14), |sky| sky.0);
        living_background(gaanim_api::canvas::background_presets::aurora(
            &colors_of(colors),
            sky,
            speed,
            seed,
        ))
    }

    /// Dots every `spacing` scene units, drifting by `drift` units per second.
    #[staticmethod]
    #[pyo3(signature = (spacing=0.4, *, radius=0.03, color=None, background=None, drift=(0.0, 0.0)))]
    fn dot_grid(
        spacing: f64,
        radius: f64,
        color: Option<PyColor>,
        background: Option<PyColor>,
        drift: (f64, f64),
    ) -> PyResult<Self> {
        let color = color.map_or(peniko::Color::from_rgba8(0xff, 0xff, 0xff, 0x40), |c| c.0);
        let background = background
            .map_or(peniko::Color::from_rgb8(0x0b, 0x10, 0x20), |background| {
                background.0
            });
        living_background(gaanim_api::canvas::background_presets::dot_grid(
            spacing, radius, color, background, drift,
        ))
    }

    #[getter]
    fn fallback(&self) -> PyColor {
        PyColor(self.0.fallback_color())
    }

    fn __repr__(&self) -> &'static str {
        match &self.0 {
            gaanim_api::canvas::BackgroundPaint::Brush(_) => "Background(...)",
            gaanim_api::canvas::BackgroundPaint::Shader(_) => "Background.shader(...)",
        }
    }
}

fn colors_of(colors: Vec<PyColor>) -> Vec<peniko::Color> {
    colors.into_iter().map(|color| color.0).collect()
}

fn living_background(
    shader: Result<
        gaanim_api::canvas::ShaderBackground,
        gaanim_api::canvas::background_presets::BackgroundPresetError,
    >,
) -> PyResult<PyBackground> {
    shader
        .map(|shader| PyBackground(gaanim_api::canvas::BackgroundPaint::Shader(shader)))
        .map_err(|error| PyValueError::new_err(error.to_string()))
}

/// Custom WGSL post-processing of the rendered 2D scene inside the camera frame.
#[pyclass(name = "PostProcess", module = "gaanim_core", skip_from_py_object)]
#[derive(Clone)]
pub struct PyPostProcess {
    pub(crate) shader: gaanim_api::canvas::PostProcessShader,
    /// Value of each declared uniform, validated when a Scene uses the pass.
    pub(crate) uniforms: Vec<crate::visualization::DeferredScalar>,
}

impl PyPostProcess {
    pub(crate) fn new(
        shader: gaanim_api::canvas::PostProcessShader,
        uniforms: Vec<crate::visualization::DeferredScalar>,
    ) -> Self {
        Self { shader, uniforms }
    }

    /// The pass for `canvas`, rejecting uniforms read from another Scene.
    pub(crate) fn pass(
        &self,
        canvas: &std::sync::Arc<std::sync::Mutex<gaanim_api::canvas::SceneModel>>,
    ) -> PyResult<gaanim_api::canvas::PostProcessPass> {
        for uniform in &self.uniforms {
            uniform.validate(canvas)?;
        }
        gaanim_api::canvas::PostProcessPass::new(
            self.shader.clone(),
            self.uniforms
                .iter()
                .map(|uniform| uniform.source.clone())
                .collect(),
        )
        .map_err(post_process_error)
    }
}

pub(crate) fn post_process_error(error: gaanim_api::canvas::PostProcessError) -> PyErr {
    match error {
        gaanim_api::canvas::PostProcessError::ReadSource { .. } => {
            pyo3::exceptions::PyRuntimeError::new_err(error.to_string())
        }
        _ => PyValueError::new_err(error.to_string()),
    }
}

#[pymethods]
impl PyPostProcess {
    /// Build a post-process from inline WGSL or an os.PathLike asset, with
    /// optional named uniforms read as `gaanim_uniforms.<name>`.
    #[staticmethod]
    #[pyo3(signature = (source, *, uniforms=None, audio=None))]
    fn shader(
        py: Python<'_>,
        source: &Bound<'_, PyAny>,
        uniforms: Option<&Bound<'_, pyo3::types::PyDict>>,
        audio: Option<PyRef<'_, crate::pycanvas::PyAudio>>,
    ) -> PyResult<Self> {
        let mut names = Vec::new();
        let mut values = Vec::new();
        if let Some(uniforms) = uniforms {
            for (name, value) in uniforms.iter() {
                names.push(name.extract::<String>().map_err(|_| {
                    pyo3::exceptions::PyTypeError::new_err("uniform names must be strings")
                })?);
                values.push(crate::visualization::extract_deferred_scalar(value)?);
            }
        }
        if let Some(audio) = audio {
            let name = gaanim_api::canvas::AUDIO_TIME_UNIFORM;
            if names.iter().any(|existing| existing == name) {
                return Err(PyValueError::new_err(format!(
                    "the uniform {name:?} is reserved for audio="
                )));
            }
            let text = match source.extract::<String>() {
                Ok(text) => text,
                Err(_) => {
                    let path = source.extract::<PathBuf>().map_err(|_| {
                        pyo3::exceptions::PyTypeError::new_err(
                            "source must be inline WGSL text or an os.PathLike .wgsl asset",
                        )
                    })?;
                    std::fs::read_to_string(&path).map_err(|error| {
                        pyo3::exceptions::PyRuntimeError::new_err(format!(
                            "could not read {}: {error}",
                            path.display()
                        ))
                    })?
                }
            };
            let (data, time) = audio.shader_input(py)?;
            names.push(name.to_string());
            values.push(time);
            let source = format!("{}\n{text}", gaanim_api::canvas::AUDIO_SHADER_FUNCTIONS);
            return gaanim_api::canvas::PostProcessShader::with_data(source, &names, data)
                .map(|shader| Self::new(shader, values))
                .map_err(post_process_error);
        }
        let shader = if let Ok(source) = source.extract::<String>() {
            gaanim_api::canvas::PostProcessShader::with_uniforms(source, &names)
        } else {
            let path = source.extract::<PathBuf>().map_err(|_| {
                pyo3::exceptions::PyTypeError::new_err(
                    "source must be inline WGSL text or an os.PathLike .wgsl asset",
                )
            })?;
            gaanim_api::canvas::PostProcessShader::from_file_with_uniforms(path, &names)
        };
        shader
            .map(|shader| Self::new(shader, values))
            .map_err(post_process_error)
    }

    /// Film grain: hashed noise per cell of `size` pixels (at 1080p).
    #[staticmethod]
    #[pyo3(signature = (amount=None, size=None, animated=true))]
    fn grain(
        amount: Option<&Bound<'_, PyAny>>,
        size: Option<&Bound<'_, PyAny>>,
        animated: bool,
    ) -> PyResult<Self> {
        preset(
            gaanim_api::canvas::PostPreset::Grain,
            vec![
                preset_value("amount", amount, 0.06, 0.0..=f64::INFINITY)?,
                preset_value("size", size, 1.0, 0.25..=f64::INFINITY)?,
                crate::visualization::DeferredScalar::validated(
                    gaanim_animation::ScalarSource::constant(if animated { 1.0 } else { 0.0 }),
                ),
            ],
        )
    }

    /// Darken the frame toward its corners.
    #[staticmethod]
    #[pyo3(signature = (strength=None, softness=None))]
    fn vignette(
        strength: Option<&Bound<'_, PyAny>>,
        softness: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<Self> {
        preset(
            gaanim_api::canvas::PostPreset::Vignette,
            vec![
                preset_value("strength", strength, 0.35, 0.0..=1.0)?,
                preset_value("softness", softness, 0.6, 0.0..=1.0)?,
            ],
        )
    }

    /// Split red and blue toward the frame corners.
    #[staticmethod]
    #[pyo3(signature = (amount=None))]
    fn chromatic_aberration(amount: Option<&Bound<'_, PyAny>>) -> PyResult<Self> {
        preset(
            gaanim_api::canvas::PostPreset::ChromaticAberration,
            vec![preset_value("amount", amount, 0.004, 0.0..=0.5)?],
        )
    }

    /// Exposure (stops), contrast, saturation and white balance.
    #[staticmethod]
    #[pyo3(signature = (exposure=None, contrast=None, saturation=None, temperature=None))]
    fn color_grade(
        exposure: Option<&Bound<'_, PyAny>>,
        contrast: Option<&Bound<'_, PyAny>>,
        saturation: Option<&Bound<'_, PyAny>>,
        temperature: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<Self> {
        preset(
            gaanim_api::canvas::PostPreset::ColorGrade,
            vec![
                preset_value("exposure", exposure, 0.0, -10.0..=10.0)?,
                preset_value("contrast", contrast, 1.0, 0.0..=f64::INFINITY)?,
                preset_value("saturation", saturation, 1.0, 0.0..=f64::INFINITY)?,
                preset_value("temperature", temperature, 0.0, -1.0..=1.0)?,
            ],
        )
    }

    /// Grade through a 3D lookup table from a `.cube` file.
    #[staticmethod]
    #[pyo3(signature = (path, strength=None))]
    fn lut(path: PathBuf, strength: Option<&Bound<'_, PyAny>>) -> PyResult<Self> {
        let lut = gaanim_api::canvas::CubeLut::from_file(path).map_err(post_process_error)?;
        let shader = lut.shader().map_err(post_process_error)?;
        Ok(Self::new(
            shader,
            vec![
                crate::visualization::DeferredScalar::validated(
                    gaanim_animation::ScalarSource::constant(lut.size as f64),
                ),
                preset_value("strength", strength, 1.0, 0.0..=1.0)?,
            ],
        ))
    }

    /// Multipass bloom: pixels brighter than `threshold` glow by `intensity`,
    /// spreading by `radius` (0..1).
    #[staticmethod]
    #[pyo3(signature = (threshold=None, intensity=None, radius=None))]
    fn bloom(
        threshold: Option<&Bound<'_, PyAny>>,
        intensity: Option<&Bound<'_, PyAny>>,
        radius: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<Self> {
        preset(
            gaanim_api::canvas::PostPreset::Bloom,
            vec![
                preset_value("threshold", threshold, 0.8, 0.0..=1.0)?,
                preset_value("intensity", intensity, 0.6, 0.0..=f64::INFINITY)?,
                preset_value("radius", radius, 0.5, 0.0..=1.0)?,
            ],
        )
    }

    /// Print-style dots whose size follows the brightness of each cell.
    #[staticmethod]
    #[pyo3(signature = (dot=None))]
    fn halftone(dot: Option<&Bound<'_, PyAny>>) -> PyResult<Self> {
        preset(
            gaanim_api::canvas::PostPreset::Halftone,
            vec![preset_value("dot", dot, 6.0, 1.0..=f64::INFINITY)?],
        )
    }

    /// Ordered (Bayer) dithering to `levels` values per channel.
    #[staticmethod]
    #[pyo3(signature = (levels=None))]
    fn dither(levels: Option<&Bound<'_, PyAny>>) -> PyResult<Self> {
        preset(
            gaanim_api::canvas::PostPreset::Dither,
            vec![preset_value("levels", levels, 4.0, 2.0..=256.0)?],
        )
    }

    /// Curved screen, scanlines and an RGB mask.
    #[staticmethod]
    #[pyo3(signature = (strength=None))]
    fn crt(strength: Option<&Bound<'_, PyAny>>) -> PyResult<Self> {
        preset(
            gaanim_api::canvas::PostPreset::Crt,
            vec![preset_value("strength", strength, 1.0, 0.0..=1.0)?],
        )
    }

    /// Blocks of `size` pixels (at 1080p).
    #[staticmethod]
    #[pyo3(signature = (size=None))]
    fn pixelate(size: Option<&Bound<'_, PyAny>>) -> PyResult<Self> {
        preset(
            gaanim_api::canvas::PostPreset::Pixelate,
            vec![preset_value("size", size, 8.0, 1.0..=f64::INFINITY)?],
        )
    }

    /// Bands that jump sideways with an RGB split, seeded and deterministic.
    #[staticmethod]
    #[pyo3(signature = (intensity=None, seed=0))]
    fn glitch(intensity: Option<&Bound<'_, PyAny>>, seed: u32) -> PyResult<Self> {
        preset(
            gaanim_api::canvas::PostPreset::Glitch,
            vec![
                preset_value("intensity", intensity, 0.5, 0.0..=1.0)?,
                crate::visualization::DeferredScalar::validated(
                    gaanim_animation::ScalarSource::constant(f64::from(seed % 10_000)),
                ),
            ],
        )
    }

    /// Complete WGSL source of the post-process function.
    #[getter]
    fn source(&self) -> &str {
        self.shader.source()
    }

    /// Names of the declared uniforms, in declaration order.
    #[getter]
    fn uniforms(&self) -> Vec<String> {
        self.shader
            .uniforms()
            .iter()
            .map(|name| name.to_string())
            .collect()
    }

    fn __repr__(&self) -> &'static str {
        "PostProcess.shader(...)"
    }
}

fn preset(
    preset: gaanim_api::canvas::PostPreset,
    values: Vec<crate::visualization::DeferredScalar>,
) -> PyResult<PyPostProcess> {
    Ok(PyPostProcess::new(preset.shader(), values))
}

/// A preset amount: `default` when omitted, a number within `range`, or a
/// reactive value (clamped by the shader instead).
fn preset_value(
    name: &str,
    value: Option<&Bound<'_, PyAny>>,
    default: f64,
    range: std::ops::RangeInclusive<f64>,
) -> PyResult<crate::visualization::DeferredScalar> {
    let Some(value) = value.filter(|value| !value.is_none()) else {
        return Ok(crate::visualization::DeferredScalar::validated(
            gaanim_animation::ScalarSource::constant(default),
        ));
    };
    let scalar = crate::visualization::extract_deferred_scalar(value.clone())?;
    let out_of_range = scalar
        .source
        .constant_value()
        .filter(|number| !range.contains(number));
    if let Some(number) = out_of_range {
        let upper = if range.end().is_infinite() {
            String::new()
        } else {
            format!(" and at most {}", range.end())
        };
        return Err(PyValueError::new_err(format!(
            "{name} must be at least {}{upper}, got {number}",
            range.start()
        )));
    }
    Ok(scalar)
}

/// Read `None`, one PostProcess or a sequence of them as a pass chain.
pub fn post_process_passes(
    post: Option<&Bound<'_, PyAny>>,
    canvas: &std::sync::Arc<std::sync::Mutex<gaanim_api::canvas::SceneModel>>,
) -> PyResult<Vec<gaanim_api::canvas::PostProcessPass>> {
    let Some(post) = post.filter(|post| !post.is_none()) else {
        return Ok(Vec::new());
    };
    if let Ok(post) = post.cast::<PyPostProcess>() {
        return Ok(vec![post.borrow().pass(canvas)?]);
    }
    let not_a_chain = || {
        pyo3::exceptions::PyTypeError::new_err(
            "post must be a PostProcess, a sequence of PostProcess, or None",
        )
    };
    if post.is_instance_of::<pyo3::types::PyString>() {
        return Err(not_a_chain());
    }
    let items = post.try_iter().map_err(|_| not_a_chain())?;
    items
        .map(|item| {
            let item = item?;
            let post = item.cast::<PyPostProcess>().map_err(|_| not_a_chain())?;
            post.borrow().pass(canvas)
        })
        .collect()
}

/// The chain as Python values: None, one PostProcess, or a list of them.
pub fn post_process_value(
    py: Python<'_>,
    passes: &[gaanim_api::canvas::PostProcessPass],
) -> PyResult<Py<PyAny>> {
    let post = |pass: &gaanim_api::canvas::PostProcessPass| PyPostProcess {
        shader: pass.shader.clone(),
        uniforms: pass
            .values
            .iter()
            .cloned()
            .map(crate::visualization::DeferredScalar::validated)
            .collect(),
    };
    Ok(match passes {
        [] => py.None(),
        [pass] => Py::new(py, post(pass))?.into_any(),
        passes => pyo3::types::PyList::new(
            py,
            passes
                .iter()
                .map(|pass| Py::new(py, post(pass)))
                .collect::<PyResult<Vec<_>>>()?,
        )?
        .into_any()
        .unbind(),
    })
}

/// Accept `None` (inherit), `False` (disable), or a PostProcess or sequence
/// of them for a segment.
pub fn segment_post_process(
    post: Option<&Bound<'_, PyAny>>,
    canvas: &std::sync::Arc<std::sync::Mutex<gaanim_api::canvas::SceneModel>>,
) -> PyResult<gaanim_api::canvas::PostProcessOverride> {
    let Some(post) = post.filter(|post| !post.is_none()) else {
        return Ok(gaanim_api::canvas::PostProcessOverride::Inherit);
    };
    if post.is_instance_of::<pyo3::types::PyBool>() {
        return if post.extract::<bool>()? {
            Err(pyo3::exceptions::PyTypeError::new_err(
                "segment post must be a PostProcess, a sequence of them, False to disable it, or None to inherit",
            ))
        } else {
            Ok(gaanim_api::canvas::PostProcessOverride::Disabled)
        };
    }
    let passes = post_process_passes(Some(post), canvas).map_err(|error| {
        if error.is_instance_of::<pyo3::exceptions::PyTypeError>(post.py()) {
            pyo3::exceptions::PyTypeError::new_err(
                "segment post must be a PostProcess, a sequence of them, False to disable it, or None to inherit",
            )
        } else {
            error
        }
    })?;
    Ok(if passes.is_empty() {
        gaanim_api::canvas::PostProcessOverride::Disabled
    } else {
        gaanim_api::canvas::PostProcessOverride::Passes(passes)
    })
}

/// A reusable solid or gradient paint accepted by drawables and scene backgrounds.
#[pyclass(name = "Brush", module = "gaanim_core", skip_from_py_object)]
#[derive(Clone, Debug)]
pub struct PyBrush(pub Brush);

/// Accept either a Brush or any value accepted by Color.
#[derive(Clone, Debug)]
pub struct PyPaint(pub Brush);

impl<'a, 'py> FromPyObject<'a, 'py> for PyPaint {
    type Error = PyErr;

    fn extract(obj: pyo3::Borrowed<'a, 'py, PyAny>) -> Result<Self, Self::Error> {
        if let Ok(brush) = obj.cast::<PyBrush>() {
            return Ok(Self(brush.borrow().0.clone()));
        }
        PyColor::extract(obj).map(|color| Self(Brush::Solid(color.0)))
    }
}

#[pymethods]
impl PyBrush {
    #[staticmethod]
    fn solid(color: PyColor) -> Self {
        Self(Brush::Solid(color.0))
    }

    /// Linear gradient in the drawable's local coordinates.
    #[staticmethod]
    #[pyo3(signature = (colors, *, start, end, extend="pad"))]
    fn linear(
        colors: Vec<PyColor>,
        start: (f64, f64),
        end: (f64, f64),
        extend: &str,
    ) -> PyResult<Self> {
        validate_point("start", start)?;
        validate_point("end", end)?;
        if start == end {
            return Err(PyValueError::new_err(
                "linear gradient start and end must differ",
            ));
        }
        let stops = uniform_stops(colors)?;
        let gradient = Gradient::new_linear(start, end)
            .with_extend(parse_extend(extend)?)
            .with_stops(stops.as_slice());
        Ok(Self(Brush::Gradient(gradient)))
    }

    /// Radial gradient in the drawable's local coordinates.
    #[staticmethod]
    #[pyo3(signature = (colors, *, center=(0.0, 0.0), radius, extend="pad"))]
    fn radial(
        colors: Vec<PyColor>,
        center: (f64, f64),
        radius: f64,
        extend: &str,
    ) -> PyResult<Self> {
        validate_point("center", center)?;
        if !radius.is_finite() || radius <= 0.0 || radius > f32::MAX as f64 {
            return Err(PyValueError::new_err(
                "radial gradient radius must be finite and positive",
            ));
        }
        let stops = uniform_stops(colors)?;
        let gradient = Gradient::new_radial(center, radius as f32)
            .with_extend(parse_extend(extend)?)
            .with_stops(stops.as_slice());
        Ok(Self(Brush::Gradient(gradient)))
    }

    /// Angular gradient. Public angles are degrees for presentation ergonomics.
    #[staticmethod]
    #[pyo3(signature = (colors, *, center=(0.0, 0.0), start_angle=0.0, end_angle=360.0, extend="pad"))]
    fn sweep(
        colors: Vec<PyColor>,
        center: (f64, f64),
        start_angle: f64,
        end_angle: f64,
        extend: &str,
    ) -> PyResult<Self> {
        validate_point("center", center)?;
        if !start_angle.is_finite() || !end_angle.is_finite() || start_angle == end_angle {
            return Err(PyValueError::new_err(
                "sweep gradient angles must be finite and differ",
            ));
        }
        let stops = uniform_stops(colors)?;
        let gradient = Gradient::new_sweep(
            center,
            start_angle.to_radians() as f32,
            end_angle.to_radians() as f32,
        )
        .with_extend(parse_extend(extend)?)
        .with_stops(stops.as_slice());
        Ok(Self(Brush::Gradient(gradient)))
    }

    fn __repr__(&self) -> &'static str {
        match &self.0 {
            Brush::Solid(_) => "Brush.solid(...)",
            Brush::Gradient(_) => "Brush.gradient(...)",
            Brush::Image(_) => "Brush.image(...)",
        }
    }
}

fn uniform_stops(colors: Vec<PyColor>) -> PyResult<Vec<(f32, peniko::Color)>> {
    if colors.len() < 2 {
        return Err(PyValueError::new_err(
            "a gradient requires at least two colors",
        ));
    }
    let denominator = (colors.len() - 1) as f32;
    Ok(colors
        .into_iter()
        .enumerate()
        .map(|(index, color)| (index as f32 / denominator, color.0))
        .collect())
}

fn parse_extend(value: &str) -> PyResult<Extend> {
    match value {
        "pad" => Ok(Extend::Pad),
        "repeat" => Ok(Extend::Repeat),
        "reflect" => Ok(Extend::Reflect),
        _ => Err(PyValueError::new_err(
            "gradient extend must be 'pad', 'repeat', or 'reflect'",
        )),
    }
}

fn validate_point(name: &str, point: (f64, f64)) -> PyResult<()> {
    if point.0.is_finite() && point.1.is_finite() {
        Ok(())
    } else {
        Err(PyValueError::new_err(format!(
            "{name} coordinates must be finite"
        )))
    }
}
