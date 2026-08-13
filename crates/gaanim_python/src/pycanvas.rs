//! Python scene facade and its visual canvas configuration.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use pyo3::prelude::*;
use pyo3::types::{PyAny, PyDict, PySequence, PyTuple};

use gaanim_api::canvas::{
    AngleDimensionOptions, Axes3DConfig, AxesConfig, CameraConstraintHandle, Canvas as ApiCanvas,
    CanvasEndpoint, CanvasRay, CanvasTheme, CurveControl, CurveElement, DimensionExtensionStyle,
    DimensionOptions, ImageCrop, ImageFit, ImageOptions, LabelMode, PresentationBrand,
    SegmentHandle, ThemeFont,
};
use gaanim_api::export::{
    detect_best_encoder, export_canvas, export_canvas_segment, export_canvas_segments,
    AspectRatioPreset, EncodingSpeed, ExportConfig, QualityPreset, SegmentExportError,
    VideoEncoder,
};

use crate::color::PyColor;
use crate::py3d::{PyMaterial3D, PyPrimitive3D};
use crate::pydrawable::{PyAnchorPoint, PyCanvasAnim, PyDrawable};
use crate::pylayout::{
    column_kind, grid_kind, layout_item_from_python, layout_spec, parse_grid_tracks, row_kind,
    stack_kind, PyAnchor, PyConstraintSet, PyLayout, PyLayoutConstraint, PyLayoutItem,
};
use crate::pystyle::{PyAxesStyle, PyStyle};
use crate::pytext::{build_text_spec, PyText, PyTextFlow, PyTextStyle};
use crate::transition::PyTransitionType;
use crate::visualization::{extract_expr, PyParameter, PyVariable};

#[pyclass(name = "PointRef", module = "gaanim_core", frozen, from_py_object)]
#[derive(Clone, Debug)]
pub struct PyPointRef(pub gaanim_api::canvas::PointRef);

fn reactive_scalar(value: Bound<'_, PyAny>) -> PyResult<(gaanim_api::canvas::DrawableHandle, f64)> {
    if let Ok(parameter) = value.extract::<PyRef<'_, PyParameter>>() {
        Ok((
            parameter.inner.drawable().clone(),
            parameter.inner.current(),
        ))
    } else if let Ok(variable) = value.extract::<PyRef<'_, PyVariable>>() {
        Ok((
            variable.parameter.inner.drawable().clone(),
            variable.parameter.inner.current(),
        ))
    } else {
        Err(pyo3::exceptions::PyTypeError::new_err(
            "tracker must be a Parameter or Variable",
        ))
    }
}

#[pyclass(name = "Dimension", module = "gaanim_core", extends = PyDrawable, from_py_object)]
#[derive(Clone)]
pub struct PyDimension {
    line: PyDrawable,
    extensions: PyDrawable,
    label: Option<PyDrawable>,
    number: Option<PyDrawable>,
    unit: Option<PyDrawable>,
}

#[pyclass(name = "AngleDimension", module = "gaanim_core", extends = PyDrawable, from_py_object)]
#[derive(Clone)]
pub struct PyAngleDimension {
    arc: PyDrawable,
    arrows: PyDrawable,
    extensions: PyDrawable,
    label: Option<PyDrawable>,
    number: Option<PyDrawable>,
    unit: Option<PyDrawable>,
}

impl PyAngleDimension {
    fn initializer(handle: gaanim_api::canvas::AngleDimensionHandle) -> PyClassInitializer<Self> {
        PyClassInitializer::from(PyDrawable(handle.drawable)).add_subclass(Self {
            arc: PyDrawable(handle.arc),
            arrows: PyDrawable(handle.arrows),
            extensions: PyDrawable(handle.extensions),
            label: handle.label.map(PyDrawable),
            number: handle.number.map(PyDrawable),
            unit: handle.unit.map(PyDrawable),
        })
    }
}

#[pymethods]
impl PyAngleDimension {
    #[getter]
    fn arc(&self) -> PyDrawable {
        self.arc.clone()
    }
    #[getter]
    fn arrows(&self) -> PyDrawable {
        self.arrows.clone()
    }
    #[getter]
    fn extensions(&self) -> PyDrawable {
        self.extensions.clone()
    }
    #[getter]
    fn label(&self) -> Option<PyDrawable> {
        self.label.clone()
    }
    #[getter]
    fn number(&self) -> Option<PyDrawable> {
        self.number.clone()
    }
    #[getter]
    fn unit(&self) -> Option<PyDrawable> {
        self.unit.clone()
    }
}

#[pyclass(name = "Support", module = "gaanim_core", extends = PyDrawable, from_py_object)]
#[derive(Clone)]
pub struct PySupport {
    joint: PyDrawable,
    body: PyDrawable,
    ground: PyDrawable,
    rollers: PyDrawable,
    guides: PyDrawable,
    hatching: PyDrawable,
}

#[pyclass(name = "ForceVector", module = "gaanim_core", extends = PyDrawable, from_py_object)]
#[derive(Clone)]
pub struct PyForceVector {
    shaft: PyDrawable,
    head: PyDrawable,
    label: Option<PyDrawable>,
    number: Option<PyDrawable>,
    unit: Option<PyDrawable>,
}

impl PyForceVector {
    fn initializer(handle: gaanim_api::canvas::ForceVectorHandle) -> PyClassInitializer<Self> {
        PyClassInitializer::from(PyDrawable(handle.drawable)).add_subclass(Self {
            shaft: PyDrawable(handle.shaft),
            head: PyDrawable(handle.head),
            label: handle.label.map(PyDrawable),
            number: handle.number.map(PyDrawable),
            unit: handle.unit.map(PyDrawable),
        })
    }
}

#[pymethods]
impl PyForceVector {
    #[getter]
    fn shaft(&self) -> PyDrawable {
        self.shaft.clone()
    }
    #[getter]
    fn head(&self) -> PyDrawable {
        self.head.clone()
    }
    #[getter]
    fn label(&self) -> Option<PyDrawable> {
        self.label.clone()
    }
    #[getter]
    fn number(&self) -> Option<PyDrawable> {
        self.number.clone()
    }
    #[getter]
    fn unit(&self) -> Option<PyDrawable> {
        self.unit.clone()
    }
}

impl PySupport {
    fn initializer(handle: gaanim_api::canvas::SupportHandle) -> PyClassInitializer<Self> {
        PyClassInitializer::from(PyDrawable(handle.drawable)).add_subclass(Self {
            joint: PyDrawable(handle.joint),
            body: PyDrawable(handle.body),
            ground: PyDrawable(handle.ground),
            rollers: PyDrawable(handle.rollers),
            guides: PyDrawable(handle.guides),
            hatching: PyDrawable(handle.hatching),
        })
    }
}

#[pymethods]
impl PySupport {
    #[getter]
    fn joint(&self) -> PyDrawable {
        self.joint.clone()
    }
    #[getter]
    fn body(&self) -> PyDrawable {
        self.body.clone()
    }
    #[getter]
    fn ground(&self) -> PyDrawable {
        self.ground.clone()
    }
    #[getter]
    fn rollers(&self) -> PyDrawable {
        self.rollers.clone()
    }
    #[getter]
    fn guides(&self) -> PyDrawable {
        self.guides.clone()
    }
    #[getter]
    fn hatching(&self) -> PyDrawable {
        self.hatching.clone()
    }
}

impl PyDimension {
    fn initializer(handle: gaanim_api::canvas::DimensionHandle) -> PyClassInitializer<Self> {
        PyClassInitializer::from(PyDrawable(handle.drawable)).add_subclass(Self {
            line: PyDrawable(handle.line),
            extensions: PyDrawable(handle.extensions),
            label: handle.label.map(PyDrawable),
            number: handle.number.map(PyDrawable),
            unit: handle.unit.map(PyDrawable),
        })
    }
}

#[pymethods]
impl PyDimension {
    #[getter]
    fn line(&self) -> PyDrawable {
        self.line.clone()
    }

    #[getter]
    fn extensions(&self) -> PyDrawable {
        self.extensions.clone()
    }

    #[getter]
    fn label(&self) -> Option<PyDrawable> {
        self.label.clone()
    }

    #[getter]
    fn number(&self) -> Option<PyDrawable> {
        self.number.clone()
    }

    #[getter]
    fn unit(&self) -> Option<PyDrawable> {
        self.unit.clone()
    }
}

fn segment_export_error(error: SegmentExportError) -> PyErr {
    match &error {
        SegmentExportError::Export(_) | SegmentExportError::Io(_) => {
            pyo3::exceptions::PyRuntimeError::new_err(error.to_string())
        }
        _ => pyo3::exceptions::PyValueError::new_err(error.to_string()),
    }
}

fn drawable_args(
    first: &PyDrawable,
    others: &Bound<'_, PyTuple>,
) -> PyResult<Vec<gaanim_api::canvas::DrawableHandle>> {
    let mut drawables = vec![first.0.clone()];
    for item in others.iter() {
        let drawable = item.extract::<PyRef<'_, PyDrawable>>()?;
        drawables.push(drawable.0.clone());
    }
    Ok(drawables)
}

fn layout_members(children: &Bound<'_, PyAny>) -> PyResult<Vec<crate::pylayout::LayoutMember>> {
    let children = children.cast::<PySequence>().map_err(|_| {
        pyo3::exceptions::PyTypeError::new_err(
            "children must be a sequence of Drawable, Layout, or LayoutItem values",
        )
    })?;
    children
        .try_iter()?
        .map(|child| PyLayout::member_from_python(&child?))
        .collect()
}

fn parse_quality(value: &str) -> PyResult<QualityPreset> {
    match value.to_ascii_lowercase().as_str() {
        "draft" => Ok(QualityPreset::Draft),
        "standard" => Ok(QualityPreset::Standard),
        "production" => Ok(QualityPreset::Production),
        _ => Err(pyo3::exceptions::PyValueError::new_err(
            "quality must be 'draft', 'standard', or 'production'",
        )),
    }
}

fn parse_aspect_ratio(value: &str) -> PyResult<AspectRatioPreset> {
    match value.to_ascii_lowercase().as_str() {
        "youtube" | "16:9" => Ok(AspectRatioPreset::Youtube),
        "tiktok" | "9:16" => Ok(AspectRatioPreset::TikTok),
        "instagram" | "1:1" | "square" => Ok(AspectRatioPreset::Instagram),
        "custom" => Ok(AspectRatioPreset::Custom),
        _ => Err(pyo3::exceptions::PyValueError::new_err(
            "aspect_ratio must be 'youtube', 'tiktok', 'instagram', or 'custom'",
        )),
    }
}

fn parse_encoder(value: &str) -> PyResult<VideoEncoder> {
    match value.to_ascii_lowercase().as_str() {
        "auto" => Ok(detect_best_encoder()),
        "libx264" | "cpu" => Ok(VideoEncoder::Libx264),
        "h264_nvenc" | "nvenc" => Ok(VideoEncoder::H264Nvenc),
        "h264_amf" | "amf" => Ok(VideoEncoder::H264Amf),
        "h264_qsv" | "qsv" => Ok(VideoEncoder::H264Qsv),
        "h264_vaapi" | "vaapi" => Ok(VideoEncoder::H264Vaapi),
        _ => Err(pyo3::exceptions::PyValueError::new_err(
            "encoder must be 'auto', 'libx264', 'nvenc', 'amf', 'qsv', or 'vaapi'",
        )),
    }
}

fn parse_encoding_speed(value: &str) -> PyResult<EncodingSpeed> {
    match value.to_ascii_lowercase().as_str() {
        "fast" => Ok(EncodingSpeed::Fast),
        "balanced" | "medium" => Ok(EncodingSpeed::Balanced),
        "best" | "slow" => Ok(EncodingSpeed::Best),
        _ => Err(pyo3::exceptions::PyValueError::new_err(
            "speed must be 'fast', 'balanced', or 'best'",
        )),
    }
}

fn parse_curve_elements(commands: &Bound<'_, PyAny>) -> PyResult<Vec<CurveElement>> {
    let mut elements = Vec::new();
    for command in commands.try_iter()? {
        let command = command?;
        let (kind, arguments): (String, Bound<'_, PyAny>) = command.extract()?;
        let arguments: Vec<Bound<'_, PyAny>> = arguments.try_iter()?.collect::<PyResult<_>>()?;
        let (kind, relative) = match kind.as_str() {
            "move" => ("move", false),
            "move_rel" => ("move", true),
            "line" => ("line", false),
            "line_rel" => ("line", true),
            "quad" => ("quad", false),
            "quad_rel" => ("quad", true),
            "cubic" => ("cubic", false),
            "cubic_rel" => ("cubic", true),
            "close" | "close_smooth" => (kind.as_str(), false),
            _ => {
                return Err(pyo3::exceptions::PyValueError::new_err(format!(
                    "unknown path command {kind:?}; expected move, line, quad, cubic, close, or close_smooth (with optional _rel)"
                )));
            }
        };
        let point = |value: &Bound<'_, PyAny>| -> PyResult<(f64, f64)> {
            value.extract().map_err(|_| {
                pyo3::exceptions::PyTypeError::new_err("path points must be (x, y) pairs")
            })
        };
        let control = |value: &Bound<'_, PyAny>| -> PyResult<CurveControl> {
            if value.is_none() {
                Ok(CurveControl::None)
            } else if let Ok(name) = value.extract::<&str>() {
                if name == "auto" {
                    Ok(CurveControl::Auto)
                } else {
                    Err(pyo3::exceptions::PyValueError::new_err(
                        "path controls may only use the string 'auto'",
                    ))
                }
            } else {
                point(value).map(CurveControl::Point)
            }
        };
        let element = match (kind, arguments.as_slice()) {
            ("move", [to]) => CurveElement::Move {
                to: point(to)?,
                relative,
            },
            ("line", [to]) => CurveElement::Line {
                to: point(to)?,
                relative,
            },
            ("quad", [handle, to]) => CurveElement::Quad {
                control: control(handle)?,
                to: point(to)?,
                relative,
            },
            ("cubic", [start, end, to]) => CurveElement::Cubic {
                control_start: control(start)?,
                control_end: control(end)?,
                to: point(to)?,
                relative,
            },
            ("close", []) => CurveElement::Close { smooth: false },
            ("close_smooth", []) => CurveElement::Close { smooth: true },
            _ => {
                return Err(pyo3::exceptions::PyValueError::new_err(format!(
                    "invalid arguments for path command {kind:?}"
                )));
            }
        };
        elements.push(element);
    }
    Ok(elements)
}

fn escape_typst_string(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "")
}

#[derive(Clone, Copy)]
struct ComponentPalette {
    foreground: gaanim_core::peniko::Color,
    muted: gaanim_core::peniko::Color,
    accent: gaanim_core::peniko::Color,
    chart: gaanim_core::peniko::Color,
    panel: gaanim_core::peniko::Color,
    header: gaanim_core::peniko::Color,
    rule: gaanim_core::peniko::Color,
}

fn component_palette(scene: &ApiCanvas) -> ComponentPalette {
    use gaanim_core::peniko::Color;

    if let Some(theme) = &scene.theme_style {
        ComponentPalette {
            foreground: theme.palette.foreground,
            muted: theme.palette.muted,
            accent: theme.palette.accent,
            chart: theme.palette.chart,
            panel: theme.palette.panel,
            header: theme.palette.header,
            rule: theme.palette.rule,
        }
    } else {
        ComponentPalette {
            foreground: Color::from_rgb8(0xE6, 0xED, 0xF5),
            muted: Color::from_rgb8(0x94, 0xA3, 0xB8),
            accent: Color::from_rgb8(0x5B, 0x8F, 0xC9),
            chart: Color::from_rgb8(0x4C, 0x78, 0xA8),
            panel: Color::from_rgb8(0x10, 0x16, 0x20),
            header: Color::from_rgb8(0x16, 0x2B, 0x46),
            rule: Color::from_rgb8(0x5B, 0x70, 0x88),
        }
    }
}

/// Reusable colors, typography, and embedded font files for a scene.
#[pyclass(name = "Theme", module = "gaanim_core", skip_from_py_object)]
#[derive(Clone)]
pub struct PyTheme {
    pub(crate) inner: CanvasTheme,
}

#[pymethods]
impl PyTheme {
    /// Create a theme from scratch, from a built-in scheme name, or from
    /// another Theme. Dictionaries override only the supplied semantic roles.
    #[new]
    #[pyo3(signature = (
        base=None,
        *,
        name=None,
        colors=None,
        fonts=None,
        sizes=None,
        text=None,
        styles=None,
        series=None,
        heatmap=None,
        layout=None,
        font_files=None,
    ))]
    fn new(
        base: Option<&Bound<'_, PyAny>>,
        name: Option<String>,
        colors: Option<HashMap<String, PyColor>>,
        fonts: Option<HashMap<String, String>>,
        sizes: Option<HashMap<String, f64>>,
        text: Option<&Bound<'_, PyDict>>,
        styles: Option<&Bound<'_, PyDict>>,
        series: Option<Vec<PyColor>>,
        heatmap: Option<Vec<PyColor>>,
        layout: Option<HashMap<String, f64>>,
        font_files: Option<HashMap<String, String>>,
    ) -> PyResult<Self> {
        let mut theme = match base {
            Some(base) => {
                if let Ok(name) = base.extract::<String>() {
                    CanvasTheme::builtin(&name).map_err(|error| {
                        pyo3::exceptions::PyValueError::new_err(error.to_string())
                    })?
                } else if let Ok(theme) = base.extract::<PyRef<'_, PyTheme>>() {
                    theme.inner.clone()
                } else {
                    return Err(pyo3::exceptions::PyTypeError::new_err(
                        "Theme base must be a built-in scheme name, another Theme, or None",
                    ));
                }
            }
            None => CanvasTheme::custom(name.clone().unwrap_or_else(|| "custom".into())),
        };
        if let Some(name) = name {
            theme.name = name;
        } else if base.is_some_and(|base| base.extract::<PyRef<'_, PyTheme>>().is_ok()) {
            theme.name = format!("{}-derived", theme.name);
        }
        if let Some(colors) = colors {
            let colors = colors
                .into_iter()
                .map(|(role, color)| (role, color.0))
                .collect();
            theme
                .set_colors(&colors)
                .map_err(pyo3::exceptions::PyValueError::new_err)?;
        }
        if let Some(fonts) = fonts {
            theme
                .set_fonts(&fonts)
                .map_err(pyo3::exceptions::PyValueError::new_err)?;
        }
        if let Some(sizes) = sizes {
            theme
                .set_sizes(&sizes)
                .map_err(pyo3::exceptions::PyValueError::new_err)?;
        }
        if let Some(text) = text {
            let mut overlays = HashMap::new();
            for (role, style) in text.iter() {
                let role_name = role.extract::<String>()?;
                let role = match role_name.as_str() {
                    "title" => gaanim_text::prelude::TextRole::Title,
                    "subtitle" => gaanim_text::prelude::TextRole::Subtitle,
                    "heading" => gaanim_text::prelude::TextRole::Heading,
                    "body" => gaanim_text::prelude::TextRole::Body,
                    "caption" => gaanim_text::prelude::TextRole::Caption,
                    "label" => gaanim_text::prelude::TextRole::Label,
                    "math" => gaanim_text::prelude::TextRole::Math,
                    "code" => gaanim_text::prelude::TextRole::Code,
                    _ => {
                        return Err(pyo3::exceptions::PyValueError::new_err(format!(
                            "unknown theme text role '{role_name}'"
                        )))
                    }
                };
                overlays.insert(role, style.extract::<PyTextStyle>()?.0);
            }
            theme
                .set_text_styles(&overlays)
                .map_err(pyo3::exceptions::PyValueError::new_err)?;
        }
        if let Some(styles) = styles {
            let mut rules = HashMap::new();
            for (selector, style) in styles.iter() {
                let selector = selector.extract::<String>()?;
                if let Ok(style) = style.extract::<PyStyle>() {
                    rules.insert(selector, style.0);
                } else if let Ok(style) = style.extract::<PyAxesStyle>() {
                    rules.extend(style.expand(&selector));
                } else {
                    return Err(pyo3::exceptions::PyTypeError::new_err(format!(
                        "theme style '{selector}' must be Style or AxesStyle"
                    )));
                }
            }
            theme
                .set_styles(&rules)
                .map_err(pyo3::exceptions::PyValueError::new_err)?;
        }
        if let Some(series) = series {
            if series.is_empty() {
                return Err(pyo3::exceptions::PyValueError::new_err(
                    "series must contain at least one color",
                ));
            }
            theme.series = series.into_iter().map(|color| color.0).collect();
        }
        if let Some(heatmap) = heatmap {
            if heatmap.len() < 2 {
                return Err(pyo3::exceptions::PyValueError::new_err(
                    "heatmap must contain at least two colors",
                ));
            }
            theme.heatmap = heatmap.into_iter().map(|color| color.0).collect();
        }
        if let Some(layout) = layout {
            theme
                .layout
                .set(&layout)
                .map_err(pyo3::exceptions::PyValueError::new_err)?;
        }
        if let Some(font_files) = font_files {
            for (family, path) in font_files {
                let bytes = std::fs::read(&path).map_err(|error| {
                    pyo3::exceptions::PyIOError::new_err(format!(
                        "could not read font file '{path}' for '{family}': {error}"
                    ))
                })?;
                theme.fonts.push(ThemeFont {
                    family,
                    bytes: bytes.into(),
                });
            }
        }
        Ok(Self { inner: theme })
    }

    #[getter]
    fn name(&self) -> &str {
        &self.inner.name
    }

    /// Names accepted by `Theme(name)` and `scene.canvas.set_theme(name)`.
    #[staticmethod]
    fn schemes() -> Vec<&'static str> {
        CanvasTheme::BUILTIN_NAMES.to_vec()
    }

    /// Resolve a semantic token for styling manual primitives.
    fn color(&self, role: &str) -> PyResult<PyColor> {
        self.inner
            .color(role)
            .map(PyColor)
            .map_err(pyo3::exceptions::PyValueError::new_err)
    }

    /// Resolve a named spacing/layout token.
    fn layout_token(&self, name: &str) -> PyResult<f64> {
        self.inner
            .layout
            .get(name)
            .map_err(pyo3::exceptions::PyValueError::new_err)
    }

    /// Return contrast and typography warnings. An empty list is presentation-ready.
    fn validate(&self) -> Vec<String> {
        self.inner.validate()
    }

    fn __repr__(&self) -> String {
        format!("Theme({:?})", self.inner.name)
    }
}

/// Visual configuration owned by a [`PyScene`].
///
/// A canvas deliberately has no timeline or mobject factories.  Those belong to
/// `Scene`; this object only controls the viewport shared by that scene.
#[pyclass(name = "Canvas", module = "gaanim_core")]
pub struct PyCanvas {
    inner: Arc<Mutex<ApiCanvas>>,
}

#[pymethods]
impl PyCanvas {
    #[getter]
    fn width(&self) -> u32 {
        self.inner.lock().expect("scene canvas poisoned").width
    }

    #[setter]
    fn set_width(&self, width: u32) {
        self.inner.lock().expect("scene canvas poisoned").width = width;
    }

    #[getter]
    fn height(&self) -> u32 {
        self.inner.lock().expect("scene canvas poisoned").height
    }

    #[setter]
    fn set_height(&self, height: u32) {
        self.inner.lock().expect("scene canvas poisoned").height = height;
    }

    #[getter]
    fn background(&self) -> Option<PyColor> {
        self.inner
            .lock()
            .expect("scene canvas poisoned")
            .background
            .map(PyColor)
    }

    #[setter]
    fn set_background(&self, background: Option<PyColor>) {
        self.inner
            .lock()
            .expect("scene canvas poisoned")
            .set_background(background.map(|c| c.0));
    }

    /// Name of the selected built-in or custom visual theme, if any.
    #[getter]
    fn theme(&self) -> Option<String> {
        self.inner
            .lock()
            .expect("scene canvas poisoned")
            .theme
            .clone()
    }

    /// Apply a built-in visual theme.
    ///
    /// Accepts either a built-in color-scheme name or a reusable `Theme`.
    /// Custom themes can derive a scheme and override semantic colors,
    /// typography, sizes, and embedded font files.
    fn set_theme(&self, theme: &Bound<'_, PyAny>) -> PyResult<()> {
        let mut canvas = self.inner.lock().expect("scene canvas poisoned");
        if let Ok(name) = theme.extract::<String>() {
            canvas
                .set_theme(&name)
                .map_err(|error| pyo3::exceptions::PyValueError::new_err(error.to_string()))
        } else if let Ok(theme) = theme.extract::<PyRef<'_, PyTheme>>() {
            canvas.apply_theme(theme.inner.clone());
            Ok(())
        } else {
            Err(pyo3::exceptions::PyTypeError::new_err(
                "set_theme expects a built-in scheme name or Theme",
            ))
        }
    }

    /// Resolve a semantic token from the active theme.
    fn color(&self, role: &str) -> PyResult<PyColor> {
        self.inner
            .lock()
            .expect("scene canvas poisoned")
            .theme_color(role)
            .map(PyColor)
            .map_err(pyo3::exceptions::PyValueError::new_err)
    }

    /// Resolve a layout token from the active theme or the default scale.
    fn layout_token(&self, name: &str) -> PyResult<f64> {
        self.inner
            .lock()
            .expect("scene canvas poisoned")
            .theme_layout_token(name)
            .map_err(pyo3::exceptions::PyValueError::new_err)
    }

    /// Return readability warnings for the active theme.
    fn validate_theme(&self) -> PyResult<Vec<String>> {
        self.inner
            .lock()
            .expect("scene canvas poisoned")
            .validate_theme()
            .map_err(pyo3::exceptions::PyValueError::new_err)
    }

    /// Set a uniform margin on all four sides. It affects `to_edge` and
    /// `to_corner` layout operations.
    fn set_margin(&self, margin: f64) {
        self.inner.lock().expect("scene canvas poisoned").margin =
            gaanim_api::canvas::Margin::all(margin);
    }

    /// Configure a per-edge safe area in canvas coordinates.
    #[pyo3(signature = (*, top=0.0, right=0.0, bottom=0.0, left=0.0))]
    fn set_safe_area(&self, top: f64, right: f64, bottom: f64, left: f64) -> PyResult<()> {
        for (name, value) in [
            ("top", top),
            ("right", right),
            ("bottom", bottom),
            ("left", left),
        ] {
            if !value.is_finite() || value < 0.0 {
                return Err(pyo3::exceptions::PyValueError::new_err(format!(
                    "{name} safe-area margin must be a finite non-negative number"
                )));
            }
        }
        self.inner.lock().expect("scene canvas poisoned").margin = gaanim_api::canvas::Margin {
            top,
            right,
            bottom,
            left,
        };
        Ok(())
    }

    /// Apply a common output format and its conservative safe area.
    fn set_preset(&self, name: &str) -> PyResult<()> {
        let (width, height, margin) = match name.to_ascii_lowercase().as_str() {
            "widescreen" | "youtube" | "16:9" => {
                (1920, 1080, gaanim_api::canvas::Margin::all(96.0))
            }
            "vertical" | "tiktok" | "9:16" => (
                1080,
                1920,
                gaanim_api::canvas::Margin {
                    top: 192.0,
                    right: 72.0,
                    bottom: 320.0,
                    left: 72.0,
                },
            ),
            "square" | "instagram" | "1:1" => (1080, 1080, gaanim_api::canvas::Margin::all(72.0)),
            _ => {
                return Err(pyo3::exceptions::PyValueError::new_err(
                    "preset must be 'widescreen', 'vertical', or 'square'",
                ));
            }
        };
        let mut canvas = self.inner.lock().expect("scene canvas poisoned");
        canvas.width = width;
        canvas.height = height;
        canvas.margin = margin;
        Ok(())
    }
}

/// Top-level public facade for building a Gaanim animation.
#[pyclass(name = "Scene", module = "gaanim_core")]
pub struct PyScene {
    pub(crate) inner: Arc<Mutex<ApiCanvas>>,
}

/// Semantic camera controller exposed as ``scene.camera``.
#[pyclass(name = "Camera", module = "gaanim_core", skip_from_py_object)]
#[derive(Clone)]
pub struct PyCamera {
    inner: Arc<Mutex<ApiCanvas>>,
}

/// Stable handle for a persistent native camera constraint.
#[pyclass(name = "CameraConstraint", module = "gaanim_core", skip_from_py_object)]
#[derive(Clone)]
pub struct PyCameraConstraint {
    inner: CameraConstraintHandle,
}

#[pymethods]
impl PyCameraConstraint {
    /// Enable this constraint at the current timeline cursor.
    fn enable(&self) {
        self.inner.enable();
    }

    /// Disable this constraint at the current timeline cursor.
    fn disable(&self) {
        self.inner.disable();
    }
}

fn require_finite(value: f64, name: &str) -> PyResult<f64> {
    if value.is_finite() {
        Ok(value)
    } else {
        Err(pyo3::exceptions::PyValueError::new_err(format!(
            "{name} must be finite"
        )))
    }
}

fn require_duration(duration: f64) -> PyResult<f64> {
    if duration.is_finite() && duration >= 0.0 {
        Ok(duration)
    } else {
        Err(pyo3::exceptions::PyValueError::new_err(
            "duration must be finite and non-negative",
        ))
    }
}

fn validate_force_metrics(
    visual_scale: f64,
    label_gap: f64,
    font_size: Option<f64>,
) -> PyResult<()> {
    if !visual_scale.is_finite() || visual_scale <= 0.0 {
        return Err(pyo3::exceptions::PyValueError::new_err(
            "visual_scale must be finite and greater than zero",
        ));
    }
    if !label_gap.is_finite() || label_gap < 0.0 {
        return Err(pyo3::exceptions::PyValueError::new_err(
            "label_gap must be finite and non-negative",
        ));
    }
    if font_size.is_some_and(|size| !size.is_finite() || size <= 0.0) {
        return Err(pyo3::exceptions::PyValueError::new_err(
            "font_size must be finite and greater than zero",
        ));
    }
    Ok(())
}

#[pymethods]
impl PyCamera {
    /// Pan to a world-space point.
    #[pyo3(signature = (target, y=None, duration=1.0))]
    fn pan_to(
        &self,
        target: Bound<'_, PyAny>,
        y: Option<f64>,
        duration: f64,
    ) -> PyResult<PyCanvasAnim> {
        let duration = require_duration(duration)?;
        let inner = if let Some(y) = y {
            let x = target.extract::<f64>().map_err(|_| {
                pyo3::exceptions::PyTypeError::new_err(
                    "the two-coordinate overload requires numeric x and y",
                )
            })?;
            self.inner
                .lock()
                .expect("scene canvas poisoned")
                .camera_pan_to(require_finite(x, "x")?, require_finite(y, "y")?, duration)
        } else {
            let endpoint = resolve_endpoint(&target)?;
            self.inner
                .lock()
                .expect("scene canvas poisoned")
                .camera_pan_to_endpoint(endpoint, duration)
        };
        Ok(PyCanvasAnim { inner })
    }

    /// Set the orthographic zoom. Values above one zoom in.
    #[pyo3(signature = (zoom, duration=1.0))]
    fn zoom_to(&self, zoom: Bound<'_, PyAny>, duration: f64) -> PyResult<PyCanvasAnim> {
        if let Ok(value) = zoom.extract::<f64>() {
            if !value.is_finite() || value <= 0.0 {
                return Err(pyo3::exceptions::PyValueError::new_err(
                    "zoom must be finite and positive",
                ));
            }
        }
        let duration = require_duration(duration)?;
        let zoom = extract_expr(zoom)?;
        let inner = self
            .inner
            .lock()
            .expect("scene canvas poisoned")
            .camera_zoom_to_source(zoom, duration);
        Ok(PyCanvasAnim { inner })
    }

    /// Pan and zoom in parallel so the target fits inside the safe viewport.
    #[pyo3(signature = (targets, margin=None, duration=1.0, *, dynamic=false))]
    fn frame_to(
        &self,
        targets: Bound<'_, PyAny>,
        margin: Option<Bound<'_, PyAny>>,
        duration: f64,
        dynamic: bool,
    ) -> PyResult<PyCanvasAnim> {
        let margins = if let Some(margin) = margin {
            if let Ok(value) = margin.extract::<f64>() {
                [value; 4]
            } else if let Ok((vertical, horizontal)) = margin.extract::<(f64, f64)>() {
                [vertical, horizontal, vertical, horizontal]
            } else if let Ok(values) = margin.extract::<(f64, f64, f64, f64)>() {
                [values.0, values.1, values.2, values.3]
            } else {
                return Err(pyo3::exceptions::PyTypeError::new_err(
                    "margin must be a scalar, (vertical, horizontal), or (top, right, bottom, left)",
                ));
            }
        } else {
            [40.0; 4]
        };
        if margins
            .iter()
            .any(|value| !value.is_finite() || *value < 0.0)
        {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "margin must be finite and non-negative",
            ));
        }
        let targets = if let Ok(drawable) = targets.extract::<PyRef<'_, PyDrawable>>() {
            vec![drawable.0.clone()]
        } else {
            let sequence = targets.cast::<PySequence>().map_err(|_| {
                pyo3::exceptions::PyTypeError::new_err(
                    "targets must be a Drawable or a non-empty sequence of Drawable values",
                )
            })?;
            let mut drawables = Vec::new();
            for item in sequence.try_iter()? {
                let item = item?;
                let drawable = item.extract::<PyRef<'_, PyDrawable>>().map_err(|_| {
                    pyo3::exceptions::PyTypeError::new_err(
                        "every frame_to target must be a Drawable",
                    )
                })?;
                drawables.push(drawable.0.clone());
            }
            drawables
        };
        if targets.is_empty() {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "targets must contain at least one Drawable",
            ));
        }
        let duration = require_duration(duration)?;
        let inner = self
            .inner
            .lock()
            .expect("scene canvas poisoned")
            .camera_frame_many(&targets, margins, dynamic, duration);
        Ok(PyCanvasAnim { inner })
    }

    /// Rotate the camera around the viewport center, in radians.
    #[pyo3(signature = (angle, duration=1.0))]
    fn rotate_to(&self, angle: Bound<'_, PyAny>, duration: f64) -> PyResult<PyCanvasAnim> {
        if let Ok(value) = angle.extract::<f64>() {
            require_finite(value, "angle")?;
        }
        let duration = require_duration(duration)?;
        let angle = extract_expr(angle)?;
        let inner = self
            .inner
            .lock()
            .expect("scene canvas poisoned")
            .camera_rotate_to_source(angle, duration);
        Ok(PyCanvasAnim { inner })
    }

    /// Keep the camera centered on a drawable while it moves.
    #[pyo3(signature = (target, *, offset=(0.0, 0.0), offset_space="world", lag=0.0, duration=1.0))]
    fn follow(
        &self,
        target: Bound<'_, PyAny>,
        offset: (f64, f64),
        offset_space: &str,
        lag: f64,
        duration: f64,
    ) -> PyResult<PyCanvasAnim> {
        if !offset.0.is_finite() || !offset.1.is_finite() {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "offset must be finite",
            ));
        }
        if !lag.is_finite() || lag < 0.0 {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "lag must be finite and non-negative",
            ));
        }
        let offset_space = match offset_space {
            "world" => gaanim_animation::FollowOffsetSpace::World,
            "local" => gaanim_animation::FollowOffsetSpace::Local,
            _ => {
                return Err(pyo3::exceptions::PyValueError::new_err(
                    "offset_space must be 'world' or 'local'",
                ));
            }
        };
        let duration = require_duration(duration)?;
        let inner = self
            .inner
            .lock()
            .expect("scene canvas poisoned")
            .camera_follow_endpoint(
                resolve_endpoint(&target)?,
                gaanim_core::glam::DVec3::new(offset.0, offset.1, 0.0),
                offset_space,
                lag,
                duration,
            );
        Ok(PyCanvasAnim { inner })
    }

    /// Apply a deterministic shake that settles at the original position.
    #[pyo3(signature = (amplitude=12.0, frequency=8.0, duration=0.5))]
    fn shake(&self, amplitude: f64, frequency: f64, duration: f64) -> PyResult<PyCanvasAnim> {
        if !amplitude.is_finite() || amplitude < 0.0 {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "amplitude must be finite and non-negative",
            ));
        }
        if !frequency.is_finite() || frequency < 0.0 {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "frequency must be finite and non-negative",
            ));
        }
        let duration = require_duration(duration)?;
        let inner = self
            .inner
            .lock()
            .expect("scene canvas poisoned")
            .camera_shake(amplitude, frequency, duration);
        Ok(PyCanvasAnim { inner })
    }

    /// Set camera to look at target from eye (3D perspective).
    #[pyo3(signature = (eye, target, up=None, duration=1.0))]
    fn look_at(
        &self,
        eye: Bound<'_, PyAny>,
        target: Bound<'_, PyAny>,
        up: Option<(f64, f64, f64)>,
        duration: f64,
    ) -> PyResult<PyCanvasAnim> {
        if let Some(up) = up {
            if ![up.0, up.1, up.2].iter().all(|v| v.is_finite()) {
                return Err(pyo3::exceptions::PyValueError::new_err("up must be finite"));
            }
        }
        let up_tuple = up.unwrap_or((0.0, 1.0, 0.0));
        let up_vec = gaanim_core::glam::DVec3::new(up_tuple.0, up_tuple.1, up_tuple.2);
        if up_vec.length_squared() <= f64::EPSILON {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "up must be non-zero",
            ));
        }
        if let (Ok(eye_tuple), Ok(target_tuple)) = (
            eye.extract::<(f64, f64, f64)>(),
            target.extract::<(f64, f64, f64)>(),
        ) {
            let eye_vec = gaanim_core::glam::DVec3::new(eye_tuple.0, eye_tuple.1, eye_tuple.2);
            let target_vec =
                gaanim_core::glam::DVec3::new(target_tuple.0, target_tuple.1, target_tuple.2);
            gaanim_math::Camera::validate_look_at(eye_vec, target_vec, up_vec)
                .map_err(|error| pyo3::exceptions::PyValueError::new_err(error.to_string()))?;
        }
        let duration = require_duration(duration)?;
        let inner = self
            .inner
            .lock()
            .expect("scene canvas poisoned")
            .camera_look_at_endpoints(
                resolve_endpoint_3d(&eye)?,
                resolve_endpoint_3d(&target)?,
                up_vec,
                duration,
            );
        Ok(PyCanvasAnim { inner })
    }

    /// Orbit around current target by yaw/pitch (radians).
    #[pyo3(signature = (delta_yaw, delta_pitch, duration=1.0))]
    fn orbit(&self, delta_yaw: f64, delta_pitch: f64, duration: f64) -> PyResult<PyCanvasAnim> {
        if ![delta_yaw, delta_pitch].iter().all(|v| v.is_finite()) {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "delta_yaw/delta_pitch must be finite",
            ));
        }
        let duration = require_duration(duration)?;
        let inner = self
            .inner
            .lock()
            .expect("scene canvas poisoned")
            .camera_orbit(delta_yaw, delta_pitch, duration);
        Ok(PyCanvasAnim { inner })
    }

    /// Animate perspective projection (fov in radians).
    #[pyo3(signature = (fov_y, near=0.1, far=1000.0, duration=1.0))]
    fn perspective(
        &self,
        fov_y: f64,
        near: f64,
        far: f64,
        duration: f64,
    ) -> PyResult<PyCanvasAnim> {
        if ![fov_y, near, far].iter().all(|v| v.is_finite())
            || fov_y <= 0.0
            || fov_y >= std::f64::consts::PI
            || near <= 0.0
            || far <= near
        {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "fov_y/near/far must be finite with 0 < near < far and 0 < fov_y < pi",
            ));
        }
        let duration = require_duration(duration)?;
        let inner = self
            .inner
            .lock()
            .expect("scene canvas poisoned")
            .camera_perspective(fov_y, near, far, duration);
        Ok(PyCanvasAnim { inner })
    }

    /// Select orthographic projection. Values above one zoom in.
    #[pyo3(signature = (zoom=1.0, duration=0.0))]
    fn orthographic(&self, zoom: f64, duration: f64) -> PyResult<PyCanvasAnim> {
        if !zoom.is_finite() || zoom <= 0.0 {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "zoom must be finite and positive",
            ));
        }
        let duration = require_duration(duration)?;
        let inner = self
            .inner
            .lock()
            .expect("scene canvas poisoned")
            .camera_orthographic(zoom, duration);
        Ok(PyCanvasAnim { inner })
    }

    /// Restore the default authored 2D pose and projection.
    #[pyo3(signature = (duration=1.0))]
    fn reset(&self, duration: f64) -> PyResult<PyCanvasAnim> {
        let duration = require_duration(duration)?;
        let inner = self
            .inner
            .lock()
            .expect("scene canvas poisoned")
            .camera_reset(duration);
        Ok(PyCanvasAnim { inner })
    }

    /// Dolly camera toward/away from target (factor <1 closer).
    #[pyo3(signature = (factor, duration=1.0))]
    fn dolly(&self, factor: f64, duration: f64) -> PyResult<PyCanvasAnim> {
        if !factor.is_finite() || factor <= 0.0 {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "factor must be finite and positive",
            ));
        }
        let duration = require_duration(duration)?;
        let inner = self
            .inner
            .lock()
            .expect("scene canvas poisoned")
            .camera_dolly(factor, duration);
        Ok(PyCanvasAnim { inner })
    }

    /// Persistently bind orthographic camera channels to native reactive sources.
    #[pyo3(signature = (*, center=None, zoom=None, rotation=None, influence=None, enabled=true))]
    fn bind_2d(
        &self,
        center: Option<Bound<'_, PyAny>>,
        zoom: Option<Bound<'_, PyAny>>,
        rotation: Option<Bound<'_, PyAny>>,
        influence: Option<Bound<'_, PyAny>>,
        enabled: bool,
    ) -> PyResult<PyCameraConstraint> {
        let center = center.as_ref().map(resolve_endpoint).transpose()?;
        let zoom = zoom.map(extract_expr).transpose()?;
        let rotation = rotation.map(extract_expr).transpose()?;
        let influence = influence
            .map(extract_expr)
            .transpose()?
            .unwrap_or_else(|| gaanim_expr::Expr::constant(1.0));
        let handle = self
            .inner
            .lock()
            .expect("scene canvas poisoned")
            .camera_bind_2d(center, zoom, rotation, influence, enabled)
            .map_err(|error| pyo3::exceptions::PyValueError::new_err(error.to_string()))?;
        Ok(PyCameraConstraint { inner: handle })
    }

    /// Persistently bind perspective camera channels to native reactive sources.
    #[pyo3(signature = (*, eye=None, target=None, fov_y=None, up=(0.0, 1.0, 0.0), influence=None, enabled=true))]
    fn bind_3d(
        &self,
        eye: Option<Bound<'_, PyAny>>,
        target: Option<Bound<'_, PyAny>>,
        fov_y: Option<Bound<'_, PyAny>>,
        up: (f64, f64, f64),
        influence: Option<Bound<'_, PyAny>>,
        enabled: bool,
    ) -> PyResult<PyCameraConstraint> {
        let eye = eye.as_ref().map(resolve_endpoint_3d).transpose()?;
        let target = target.as_ref().map(resolve_endpoint_3d).transpose()?;
        let fov_y = fov_y.map(extract_expr).transpose()?;
        let influence = influence
            .map(extract_expr)
            .transpose()?
            .unwrap_or_else(|| gaanim_expr::Expr::constant(1.0));
        let up = gaanim_core::glam::DVec3::new(up.0, up.1, up.2);
        let handle = self
            .inner
            .lock()
            .expect("scene canvas poisoned")
            .camera_bind_3d(eye, target, fov_y, up, influence, enabled)
            .map_err(|error| pyo3::exceptions::PyValueError::new_err(error.to_string()))?;
        Ok(PyCameraConstraint { inner: handle })
    }
}

/// Stable handle for one authored segment.
#[pyclass(name = "Segment", module = "gaanim_core")]
pub struct PySegment {
    scene: Py<PyScene>,
    template: Option<Py<PyAny>>,
    inner: SegmentHandle,
}

#[pymethods]
impl PySegment {
    /// Bind template slots and return the segment's root Layout.
    #[pyo3(signature = (**slots))]
    fn bind<'py>(
        &self,
        py: Python<'py>,
        slots: Option<&Bound<'py, PyDict>>,
    ) -> PyResult<Py<PyLayout>> {
        let template = self.template.as_ref().ok_or_else(|| {
            pyo3::exceptions::PyValueError::new_err(
                "this segment has no template; pass template= to scene.segment()",
            )
        })?;
        let result = template.bind(py).call((self.scene.bind(py),), slots)?;
        if !result.is_instance_of::<PyLayout>() {
            return Err(pyo3::exceptions::PyTypeError::new_err(
                "a segment template must return Layout",
            ));
        }
        Ok(result.extract::<Py<PyLayout>>()?)
    }
}

#[pymethods]
impl PyScene {
    #[new]
    #[pyo3(signature = (width=1280, height=720, background=None, margin=None, theme=None))]
    fn new(
        width: u32,
        height: u32,
        background: Option<PyColor>,
        margin: Option<f64>,
        theme: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<Self> {
        let mut canvas = ApiCanvas::new(width, height);
        if let Some(theme) = theme {
            if let Ok(name) = theme.extract::<String>() {
                canvas
                    .set_theme(&name)
                    .map_err(|error| pyo3::exceptions::PyValueError::new_err(error.to_string()))?;
            } else if let Ok(theme) = theme.extract::<PyRef<'_, PyTheme>>() {
                canvas.apply_theme(theme.inner.clone());
            } else {
                return Err(pyo3::exceptions::PyTypeError::new_err(
                    "theme must be a built-in scheme name, Theme, or None",
                ));
            }
        }
        if let Some(background) = background {
            canvas.set_background(Some(background.0));
        }
        if let Some(margin) = margin {
            canvas.margin = gaanim_api::canvas::Margin::all(margin);
        }
        Ok(Self {
            inner: Arc::new(Mutex::new(canvas)),
        })
    }

    /// The scene viewport and visual configuration.
    #[getter]
    fn canvas(&self) -> PyCanvas {
        PyCanvas {
            inner: self.inner.clone(),
        }
    }

    /// Editorial camera controller.
    #[getter]
    fn camera(&self) -> PyCamera {
        PyCamera {
            inner: self.inner.clone(),
        }
    }

    /// Configure a reusable logo, footer, rule, and slide numbering treatment.
    #[pyo3(signature = (*, logo=None, footer=None, slide_numbers=true, rule=true, show_on_cover=false, logo_scale=1.0))]
    fn brand(
        &self,
        logo: Option<String>,
        footer: Option<String>,
        slide_numbers: bool,
        rule: bool,
        show_on_cover: bool,
        logo_scale: f64,
    ) -> PyResult<()> {
        if !logo_scale.is_finite() || logo_scale <= 0.0 {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "logo_scale must be finite and positive",
            ));
        }
        self.inner
            .lock()
            .expect("scene canvas poisoned")
            .set_branding(PresentationBrand {
                logo: logo.map(PathBuf::from),
                footer,
                slide_numbers,
                rule,
                show_on_cover,
                logo_scale,
            });
        Ok(())
    }

    /// Horizontal Layout v2 container.
    #[pyo3(signature = (children, *, gap=24.0, padding=None, width=None, height=None, align="center", justify="start", wrap=false, within=None))]
    #[allow(clippy::too_many_arguments)]
    fn row<'py>(
        &self,
        py: Python<'py>,
        children: &Bound<'py, PyAny>,
        gap: f64,
        padding: Option<&Bound<'py, PyAny>>,
        width: Option<&Bound<'py, PyAny>>,
        height: Option<&Bound<'py, PyAny>>,
        align: &str,
        justify: &str,
        wrap: bool,
        within: Option<&str>,
    ) -> PyResult<Py<PyLayout>> {
        Py::new(
            py,
            PyLayout::initializer(
                self.inner.clone(),
                layout_spec(
                    row_kind(wrap),
                    gap,
                    padding,
                    width,
                    height,
                    align,
                    justify,
                    within,
                )?,
                layout_members(children)?,
            )?,
        )
    }

    /// Vertical Layout v2 container.
    #[pyo3(signature = (children, *, gap=24.0, padding=None, width=None, height=None, align="start", justify="start", wrap=false, within=None))]
    #[allow(clippy::too_many_arguments)]
    fn column<'py>(
        &self,
        py: Python<'py>,
        children: &Bound<'py, PyAny>,
        gap: f64,
        padding: Option<&Bound<'py, PyAny>>,
        width: Option<&Bound<'py, PyAny>>,
        height: Option<&Bound<'py, PyAny>>,
        align: &str,
        justify: &str,
        wrap: bool,
        within: Option<&str>,
    ) -> PyResult<Py<PyLayout>> {
        Py::new(
            py,
            PyLayout::initializer(
                self.inner.clone(),
                layout_spec(
                    column_kind(wrap),
                    gap,
                    padding,
                    width,
                    height,
                    align,
                    justify,
                    within,
                )?,
                layout_members(children)?,
            )?,
        )
    }

    /// Grid Layout v2 container with fixed, auto, and fractional tracks.
    #[pyo3(signature = (children, *, rows=None, columns=None, gap=0.0, row_gap=None, column_gap=None, padding=None, width=None, height=None, align="stretch", justify="start", auto_flow="row", within=None))]
    #[allow(clippy::too_many_arguments)]
    fn grid<'py>(
        &self,
        py: Python<'py>,
        children: &Bound<'py, PyAny>,
        rows: Option<&Bound<'py, PyAny>>,
        columns: Option<&Bound<'py, PyAny>>,
        gap: f64,
        row_gap: Option<f64>,
        column_gap: Option<f64>,
        padding: Option<&Bound<'py, PyAny>>,
        width: Option<&Bound<'py, PyAny>>,
        height: Option<&Bound<'py, PyAny>>,
        align: &str,
        justify: &str,
        auto_flow: &str,
        within: Option<&str>,
    ) -> PyResult<Py<PyLayout>> {
        let kind = grid_kind(
            parse_grid_tracks(rows, "rows")?,
            parse_grid_tracks(columns, "columns")?,
            auto_flow,
        )?;
        let mut spec = layout_spec(kind, gap, padding, width, height, align, justify, within)?;
        spec.style.gap =
            gaanim_core::glam::DVec2::new(column_gap.unwrap_or(gap), row_gap.unwrap_or(gap));
        Py::new(
            py,
            PyLayout::initializer(self.inner.clone(), spec, layout_members(children)?)?,
        )
    }

    /// Overlay Layout v2 container.
    #[pyo3(signature = (children, *, padding=None, width=None, height=None, align="center", within=None))]
    fn stack<'py>(
        &self,
        py: Python<'py>,
        children: &Bound<'py, PyAny>,
        padding: Option<&Bound<'py, PyAny>>,
        width: Option<&Bound<'py, PyAny>>,
        height: Option<&Bound<'py, PyAny>>,
        align: &str,
        within: Option<&str>,
    ) -> PyResult<Py<PyLayout>> {
        Py::new(
            py,
            PyLayout::initializer(
                self.inner.clone(),
                layout_spec(
                    stack_kind(),
                    0.0,
                    padding,
                    width,
                    height,
                    align,
                    "start",
                    within,
                )?,
                layout_members(children)?,
            )?,
        )
    }

    /// Adds per-child sizing, grid, fit, absolute, anchor, and offset rules.
    #[pyo3(signature = (child, *, grow=0.0, shrink=1.0, align=None, row=None, column=None, row_span=1, column_span=1, absolute=false, anchor=None, offset=(0.0, 0.0), fit="none"))]
    #[allow(clippy::too_many_arguments)]
    fn item(
        &self,
        child: &Bound<'_, PyAny>,
        grow: f64,
        shrink: f64,
        align: Option<&str>,
        row: Option<usize>,
        column: Option<usize>,
        row_span: usize,
        column_span: usize,
        absolute: bool,
        anchor: Option<&PyAnchor>,
        offset: (f64, f64),
        fit: &str,
    ) -> PyResult<PyLayoutItem> {
        layout_item_from_python(
            child,
            grow,
            shrink,
            align,
            row,
            column,
            row_span,
            column_span,
            absolute,
            anchor,
            offset,
            fit,
        )
    }

    /// Register prioritized linear relations between drawable bounds.
    #[pyo3(signature = (*constraints, animate=None))]
    fn constrain(
        &self,
        constraints: &Bound<'_, PyTuple>,
        animate: Option<f64>,
    ) -> PyResult<PyConstraintSet> {
        let mut parsed = Vec::with_capacity(constraints.len());
        for constraint in constraints.iter() {
            let constraint = constraint
                .extract::<PyRef<'_, PyLayoutConstraint>>()
                .map_err(|_| {
                    pyo3::exceptions::PyTypeError::new_err(
                        "constrain() arguments must be LayoutConstraint values",
                    )
                })?;
            if !self
                .inner
                .lock()
                .expect("scene canvas poisoned")
                .owns_drawable(&constraint.owner)
            {
                return Err(pyo3::exceptions::PyValueError::new_err(
                    "layout constraint belongs to a different Scene",
                ));
            }
            parsed.push(constraint.inner.clone());
        }
        let constraints = parsed;
        if constraints.is_empty() {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "constrain() requires at least one LayoutConstraint",
            ));
        }
        let count = constraints.len();
        self.inner
            .lock()
            .expect("scene canvas poisoned")
            .constrain_layout(constraints, animate)
            .map_err(|error| pyo3::exceptions::PyValueError::new_err(error.to_string()))?;
        Ok(PyConstraintSet { count })
    }

    /// Return weak-constraint diagnostics known before rendering.
    fn check_layout(&self) -> Vec<String> {
        self.inner
            .lock()
            .expect("scene canvas poisoned")
            .check_layout()
    }

    /// Instantiate a typed Python layout template with this scene.
    #[pyo3(signature = (template, **slots))]
    fn template<'py>(
        slf: &Bound<'py, Self>,
        template: &Bound<'py, PyAny>,
        slots: Option<&Bound<'py, PyDict>>,
    ) -> PyResult<Py<PyLayout>> {
        let result = template.call((slf,), slots)?;
        if !result.is_instance_of::<PyLayout>() {
            return Err(pyo3::exceptions::PyTypeError::new_err(
                "a layout template must return Layout",
            ));
        }
        Ok(result.extract::<Py<PyLayout>>()?)
    }

    /// Sets the directory used to resolve relative image and SVG paths.
    fn assets_dir(&self, path: &str) -> PyResult<()> {
        self.inner
            .lock()
            .expect("scene canvas poisoned")
            .set_asset_root(path)
            .map_err(|error| pyo3::exceptions::PyValueError::new_err(error.to_string()))
    }

    /// Resolve and validate raster/SVG assets before the scene is played.
    fn preload(&self, paths: Vec<String>) -> PyResult<()> {
        let paths = paths
            .into_iter()
            .map(std::path::PathBuf::from)
            .collect::<Vec<_>>();
        self.inner
            .lock()
            .expect("scene canvas poisoned")
            .preload(&paths)
            .map_err(|error| pyo3::exceptions::PyRuntimeError::new_err(error.to_string()))
    }

    /// Load the minimal project manifest. It currently accepts one setting:
    /// `assets_dir = "assets"`, resolved relative to the manifest file.
    #[pyo3(signature = (path="gaanim.toml"))]
    fn load_project(&self, path: &str) -> PyResult<()> {
        let manifest = std::path::PathBuf::from(path);
        let source = std::fs::read_to_string(&manifest).map_err(|error| {
            pyo3::exceptions::PyRuntimeError::new_err(format!(
                "could not read project manifest {path:?}: {error}"
            ))
        })?;
        let assets_dir = source
            .lines()
            .find_map(|line| {
                let line = line.trim();
                let value = line
                    .strip_prefix("assets_dir")?
                    .trim_start()
                    .strip_prefix('=')?
                    .trim();
                value
                    .strip_prefix('"')?
                    .strip_suffix('"')
                    .map(str::to_owned)
            })
            .ok_or_else(|| {
                pyo3::exceptions::PyValueError::new_err(
                    "project manifest must declare assets_dir = \"...\"",
                )
            })?;
        let root = manifest
            .parent()
            .unwrap_or_else(|| std::path::Path::new("."))
            .join(assets_dir);
        self.inner
            .lock()
            .expect("scene canvas poisoned")
            .set_asset_root(root)
            .map_err(|error| pyo3::exceptions::PyValueError::new_err(error.to_string()))
    }

    /// Invalidate decoded raster assets so a hot reload reads changed files.
    fn reload_assets(&self) {
        self.inner
            .lock()
            .expect("scene canvas poisoned")
            .reload_assets();
    }

    /// Mix an audio file into MP4/WebM exports. With no explicit `start`, the
    /// file begins at the scene's current timeline cursor.
    #[pyo3(signature = (
        path,
        *,
        start=None,
        duration=None,
        volume=1.0,
        fade_in=0.0,
        fade_out=0.0,
    ))]
    fn audio(
        &self,
        path: &str,
        start: Option<f64>,
        duration: Option<f64>,
        volume: f64,
        fade_in: f64,
        fade_out: f64,
    ) -> PyResult<()> {
        self.inner
            .lock()
            .expect("scene canvas poisoned")
            .audio(path, start, duration, volume, fade_in, fade_out)
            .map_err(|error| pyo3::exceptions::PyValueError::new_err(error.to_string()))
    }

    fn circle(&self, r: f64) -> PyDrawable {
        PyDrawable(self.inner.lock().expect("scene canvas poisoned").circle(r))
    }
    fn rect(&self, w: f64, h: f64) -> PyDrawable {
        PyDrawable(self.inner.lock().expect("scene canvas poisoned").rect(w, h))
    }
    fn rounded_rect(&self, w: f64, h: f64, r: f64) -> PyDrawable {
        PyDrawable(
            self.inner
                .lock()
                .expect("scene canvas poisoned")
                .rounded_rect(w, h, r),
        )
    }
    fn square(&self, s: f64) -> PyDrawable {
        PyDrawable(self.inner.lock().expect("scene canvas poisoned").square(s))
    }
    fn dot(&self, r: f64) -> PyDrawable {
        PyDrawable(self.inner.lock().expect("scene canvas poisoned").dot(r))
    }
    fn ellipse(&self, rx: f64, ry: f64) -> PyDrawable {
        PyDrawable(
            self.inner
                .lock()
                .expect("scene canvas poisoned")
                .ellipse(rx, ry),
        )
    }
    fn line(&self, x1: f64, y1: f64, x2: f64, y2: f64) -> PyDrawable {
        PyDrawable(
            self.inner
                .lock()
                .expect("scene canvas poisoned")
                .line(x1, y1, x2, y2),
        )
    }
    fn arrow(&self, x1: f64, y1: f64, x2: f64, y2: f64) -> PyDrawable {
        PyDrawable(
            self.inner
                .lock()
                .expect("scene canvas poisoned")
                .arrow(x1, y1, x2, y2),
        )
    }
    #[pyo3(signature = (x1, y1, x2, y2, *, dash_length=16.0, gap_length=10.0))]
    fn dashed_line(
        &self,
        x1: f64,
        y1: f64,
        x2: f64,
        y2: f64,
        dash_length: f64,
        gap_length: f64,
    ) -> PyResult<PyDrawable> {
        if !dash_length.is_finite()
            || !gap_length.is_finite()
            || dash_length <= 0.0
            || gap_length <= 0.0
        {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "dash_length and gap_length must be finite positive numbers",
            ));
        }
        Ok(PyDrawable(
            self.inner
                .lock()
                .expect("scene canvas poisoned")
                .dashed_line(x1, y1, x2, y2, dash_length, gap_length),
        ))
    }

    #[pyo3(signature = (x1, y1, x2, y2, *, head_length=None, head_width=None))]
    fn double_arrow(
        &self,
        x1: f64,
        y1: f64,
        x2: f64,
        y2: f64,
        head_length: Option<f64>,
        head_width: Option<f64>,
    ) -> PyResult<PyDrawable> {
        for value in [head_length, head_width].into_iter().flatten() {
            if !value.is_finite() || value <= 0.0 {
                return Err(pyo3::exceptions::PyValueError::new_err(
                    "head_length and head_width must be finite positive numbers",
                ));
            }
        }
        Ok(PyDrawable(
            self.inner
                .lock()
                .expect("scene canvas poisoned")
                .double_arrow(x1, y1, x2, y2, head_length, head_width),
        ))
    }

    fn polygon(&self, points: Vec<(f64, f64)>) -> PyResult<PyDrawable> {
        if points.len() < 3 || points.iter().any(|(x, y)| !x.is_finite() || !y.is_finite()) {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "polygon requires at least three finite points",
            ));
        }
        Ok(PyDrawable(
            self.inner
                .lock()
                .expect("scene canvas poisoned")
                .polygon(points),
        ))
    }

    fn star(&self, points: u32, outer_radius: f64, inner_radius: f64) -> PyResult<PyDrawable> {
        if points < 2
            || !outer_radius.is_finite()
            || !inner_radius.is_finite()
            || outer_radius <= 0.0
            || inner_radius <= 0.0
        {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "star requires at least two points and finite positive radii",
            ));
        }
        Ok(PyDrawable(
            self.inner.lock().expect("scene canvas poisoned").star(
                points,
                outer_radius,
                inner_radius,
            ),
        ))
    }

    fn regular_polygon(&self, sides: u32, radius: f64) -> PyResult<PyDrawable> {
        if sides < 3 || !radius.is_finite() || radius <= 0.0 {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "regular_polygon requires at least three sides and a finite positive radius",
            ));
        }
        Ok(PyDrawable(
            self.inner
                .lock()
                .expect("scene canvas poisoned")
                .regular_polygon(sides, radius),
        ))
    }

    fn sector(
        &self,
        cx: f64,
        cy: f64,
        radius: f64,
        start_angle: f64,
        sweep_angle: f64,
    ) -> PyResult<PyDrawable> {
        if !radius.is_finite()
            || !start_angle.is_finite()
            || !sweep_angle.is_finite()
            || radius <= 0.0
        {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "sector requires a finite positive radius and finite angles",
            ));
        }
        Ok(PyDrawable(
            self.inner.lock().expect("scene canvas poisoned").sector(
                cx,
                cy,
                radius,
                start_angle,
                sweep_angle,
            ),
        ))
    }

    fn annulus(&self, outer_radius: f64, inner_radius: f64) -> PyResult<PyDrawable> {
        if !outer_radius.is_finite()
            || !inner_radius.is_finite()
            || inner_radius <= 0.0
            || outer_radius <= inner_radius
        {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "annulus requires finite radii with outer_radius greater than inner_radius",
            ));
        }
        Ok(PyDrawable(
            self.inner
                .lock()
                .expect("scene canvas poisoned")
                .annulus(outer_radius, inner_radius),
        ))
    }

    fn brace(&self, x1: f64, y1: f64, x2: f64, y2: f64, height: f64) -> PyResult<PyDrawable> {
        if !x1.is_finite()
            || !y1.is_finite()
            || !x2.is_finite()
            || !y2.is_finite()
            || !height.is_finite()
            || height == 0.0
            || (x1 == x2 && y1 == y2)
        {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "brace requires distinct finite endpoints and a non-zero finite height",
            ));
        }
        Ok(PyDrawable(
            self.inner
                .lock()
                .expect("scene canvas poisoned")
                .brace(x1, y1, x2, y2, height),
        ))
    }

    fn checkmark(&self, size: f64) -> PyResult<PyDrawable> {
        if !size.is_finite() || size <= 0.0 {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "checkmark size must be a finite positive number",
            ));
        }
        Ok(PyDrawable(
            self.inner
                .lock()
                .expect("scene canvas poisoned")
                .checkmark(size),
        ))
    }

    fn cross(&self, size: f64) -> PyResult<PyDrawable> {
        if !size.is_finite() || size <= 0.0 {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "cross size must be a finite positive number",
            ));
        }
        Ok(PyDrawable(
            self.inner
                .lock()
                .expect("scene canvas poisoned")
                .cross(size),
        ))
    }

    fn right_angle(&self, arm_length: f64) -> PyResult<PyDrawable> {
        if !arm_length.is_finite() || arm_length <= 0.0 {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "right_angle arm_length must be a finite positive number",
            ));
        }
        Ok(PyDrawable(
            self.inner
                .lock()
                .expect("scene canvas poisoned")
                .right_angle(arm_length),
        ))
    }

    fn arc(&self, cx: f64, cy: f64, radius: f64, start_angle: f64, sweep_angle: f64) -> PyDrawable {
        PyDrawable(self.inner.lock().expect("scene canvas poisoned").arc(
            cx,
            cy,
            radius,
            start_angle,
            sweep_angle,
        ))
    }
    fn curved_arrow(&self, x1: f64, y1: f64, x2: f64, y2: f64, angle: f64) -> PyDrawable {
        PyDrawable(
            self.inner
                .lock()
                .expect("scene canvas poisoned")
                .curved_arrow(x1, y1, x2, y2, angle),
        )
    }
    fn curved_arrow_arc(
        &self,
        cx: f64,
        cy: f64,
        radius: f64,
        start_angle: f64,
        sweep_angle: f64,
    ) -> PyDrawable {
        PyDrawable(
            self.inner
                .lock()
                .expect("scene canvas poisoned")
                .curved_arrow_arc(cx, cy, radius, start_angle, sweep_angle),
        )
    }
    fn dimension(&self, x1: f64, y1: f64, x2: f64, y2: f64, offset: f64) -> PyDrawable {
        PyDrawable(
            self.inner
                .lock()
                .expect("scene canvas poisoned")
                .dimension(x1, y1, x2, y2, offset),
        )
    }

    /// Create an open polyline from points or a composed path from cursor commands.
    ///
    /// This is the primary path entry point. `polyline()` and `curve()` remain
    /// available when the caller prefers an explicit shape kind.
    fn path(&self, definition: Bound<'_, PyAny>) -> PyResult<PyDrawable> {
        if let Ok(points) = definition.extract::<Vec<(f64, f64)>>() {
            if points.len() < 2 {
                return Err(pyo3::exceptions::PyValueError::new_err(
                    "a point path requires at least two points",
                ));
            }
            return Ok(PyDrawable(
                self.inner
                    .lock()
                    .expect("scene canvas poisoned")
                    .polyline(&points),
            ));
        }

        let elements = parse_curve_elements(&definition)?;
        if elements.is_empty() {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "a command path requires at least one command",
            ));
        }
        Ok(PyDrawable(
            self.inner
                .lock()
                .expect("scene canvas poisoned")
                .curve(elements),
        ))
    }

    fn polyline(&self, points: Vec<(f64, f64)>) -> PyDrawable {
        PyDrawable(
            self.inner
                .lock()
                .expect("scene canvas poisoned")
                .polyline(&points),
        )
    }

    fn bezier(
        &self,
        start: (f64, f64),
        controls: Vec<(f64, f64)>,
        end: (f64, f64),
    ) -> PyResult<PyDrawable> {
        if !(1..=2).contains(&controls.len()) {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "controls must contain one point (quadratic) or two points (cubic)",
            ));
        }
        Ok(PyDrawable(
            self.inner
                .lock()
                .expect("scene canvas poisoned")
                .bezier(start, controls, end),
        ))
    }

    /// Create a composed native curve from Typst-inspired cursor commands.
    ///
    /// Each command is `(name, arguments)`. Use `move`, `line`, `quad`, and
    /// `cubic` for absolute coordinates; append `_rel` to make every point an
    /// offset from the current cursor. `quad` takes `(control, endpoint)` and
    /// `cubic` takes `(start_control, end_control, endpoint)`. A control may be
    /// a point, `None` (a collapsed handle), or `"auto"` (a reflected handle).
    /// Finish a subpath with `close` or `close_smooth`, both with no arguments.
    fn curve(&self, commands: Bound<'_, PyAny>) -> PyResult<PyDrawable> {
        let elements = parse_curve_elements(&commands)?;
        Ok(PyDrawable(
            self.inner
                .lock()
                .expect("scene canvas poisoned")
                .curve(elements),
        ))
    }

    #[pyo3(name = "_legacy_function_graph", signature = (function, x, samples=160))]
    fn legacy_function_graph(
        &self,
        function: Bound<'_, PyAny>,
        x: (f64, f64),
        samples: usize,
    ) -> PyResult<PyDrawable> {
        if samples < 2 {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "samples must be at least 2",
            ));
        }
        let mut points = Vec::with_capacity(samples);
        for index in 0..samples {
            let t = index as f64 / (samples - 1) as f64;
            let x_value = x.0 + (x.1 - x.0) * t;
            let y_value: f64 = function.call1((x_value,))?.extract()?;
            points.push((x_value, y_value));
        }
        Ok(PyDrawable(
            self.inner
                .lock()
                .expect("scene canvas poisoned")
                .polyline(&points),
        ))
    }

    #[pyo3(name = "_legacy_parametric_curve", signature = (function, t, samples=240))]
    fn legacy_parametric_curve(
        &self,
        function: Bound<'_, PyAny>,
        t: (f64, f64),
        samples: usize,
    ) -> PyResult<PyDrawable> {
        if samples < 2 {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "samples must be at least 2",
            ));
        }
        let mut points = Vec::with_capacity(samples);
        for index in 0..samples {
            let progress = index as f64 / (samples - 1) as f64;
            let parameter = t.0 + (t.1 - t.0) * progress;
            let point: (f64, f64) = function.call1((parameter,))?.extract()?;
            points.push(point);
        }
        Ok(PyDrawable(
            self.inner
                .lock()
                .expect("scene canvas poisoned")
                .polyline(&points),
        ))
    }
    #[pyo3(name = "_legacy_axes", signature = (
        x=None, y=None, *,
        x_range=None, y_range=None,
        grid=true, ticks=true, numbers=true, labels=true,
        x_axis=true, y_axis=true,
        x_grid=None, y_grid=None,
        x_ticks=None, y_ticks=None,
        x_numbers=None, y_numbers=None,
        x_label=None, y_label=None,
        axis_color=None, grid_color=None, tick_color=None,
        number_color=None, label_color=None,
        axis_width=3.0, grid_width=1.0, tick_width=2.0, tick_length=8.0,
        auto_fit=true, x_length=None, y_length=None, tips=true,
        axis_config=None, x_axis_config=None, y_axis_config=None
    ))]
    fn legacy_axes(
        &self,
        x: Option<Bound<'_, PyAny>>,
        y: Option<Bound<'_, PyAny>>,
        x_range: Option<Bound<'_, PyAny>>,
        y_range: Option<Bound<'_, PyAny>>,
        grid: bool,
        ticks: bool,
        numbers: bool,
        labels: bool,
        x_axis: bool,
        y_axis: bool,
        x_grid: Option<bool>,
        y_grid: Option<bool>,
        x_ticks: Option<bool>,
        y_ticks: Option<bool>,
        x_numbers: Option<bool>,
        y_numbers: Option<bool>,
        x_label: Option<String>,
        y_label: Option<String>,
        axis_color: Option<PyColor>,
        grid_color: Option<PyColor>,
        tick_color: Option<PyColor>,
        number_color: Option<PyColor>,
        label_color: Option<PyColor>,
        axis_width: f64,
        grid_width: f64,
        tick_width: f64,
        tick_length: f64,
        auto_fit: bool,
        x_length: Option<f64>,
        y_length: Option<f64>,
        tips: bool,
        axis_config: Option<Bound<'_, PyAny>>,
        x_axis_config: Option<Bound<'_, PyAny>>,
        y_axis_config: Option<Bound<'_, PyAny>>,
    ) -> PyResult<PyDrawable> {
        // manim compat: x/y can be (min,max) or (min,max,step), and x_range/y_range aliases
        let parse_range =
            |opt: Option<Bound<PyAny>>, default: (f64, f64, f64)| -> PyResult<(f64, f64, f64)> {
                if let Some(b) = opt {
                    if let Ok(v) = b.extract::<(f64, f64, f64)>() {
                        Ok(v)
                    } else if let Ok(v) = b.extract::<(f64, f64)>() {
                        Ok((v.0, v.1, 1.0))
                    } else if let Ok(v) = b.extract::<Vec<f64>>() {
                        if v.len() == 2 {
                            Ok((v[0], v[1], 1.0))
                        } else if v.len() == 3 {
                            Ok((v[0], v[1], v[2]))
                        } else {
                            Err(pyo3::exceptions::PyValueError::new_err(
                                "x_range/y_range must be (min, max) or (min, max, step)",
                            ))
                        }
                    } else {
                        Err(pyo3::exceptions::PyValueError::new_err(
                            "x_range/y_range must be (min, max) or (min, max, step)",
                        ))
                    }
                } else {
                    Ok(default)
                }
            };
        // x takes precedence over x_range if both provided (x is the gaanim name, x_range is manim)
        let x = parse_range(x.or(x_range), (-7.11, 7.11, 1.0))?;
        let y = parse_range(y.or(y_range), (-4.0, 4.0, 1.0))?;
        if !x.0.is_finite()
            || !x.1.is_finite()
            || !x.2.is_finite()
            || !y.0.is_finite()
            || !y.1.is_finite()
            || !y.2.is_finite()
            || x.0 >= x.1
            || y.0 >= y.1
            || x.2 <= 0.0
            || y.2 <= 0.0
        {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "axis ranges must be finite (min, max, step) tuples with min < max and step > 0",
            ));
        }
        if [axis_width, grid_width, tick_width, tick_length]
            .iter()
            .any(|value| !value.is_finite() || *value < 0.0)
        {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "axis, grid, and tick dimensions must be finite and non-negative",
            ));
        }
        if let Some(v) = x_length {
            if !v.is_finite() || v <= 0.0 {
                return Err(pyo3::exceptions::PyValueError::new_err(
                    "x_length must be finite and positive",
                ));
            }
        }
        if let Some(v) = y_length {
            if !v.is_finite() || v <= 0.0 {
                return Err(pyo3::exceptions::PyValueError::new_err(
                    "y_length must be finite and positive",
                ));
            }
        }
        // axis_config dicts kept for manim compat (currently no-op, reserved for NumberLine overrides)
        let _ = (&axis_config, &x_axis_config, &y_axis_config);
        let defaults = AxesConfig::default();
        let axis_color = axis_color
            .map(|color| color.0)
            .unwrap_or(defaults.axis_color);
        let config = AxesConfig {
            grid,
            ticks,
            numbers,
            labels,
            x_axis,
            y_axis,
            x_grid: x_grid.unwrap_or(grid),
            y_grid: y_grid.unwrap_or(grid),
            x_ticks: x_ticks.unwrap_or(ticks),
            y_ticks: y_ticks.unwrap_or(ticks),
            x_numbers: x_numbers.unwrap_or(numbers),
            y_numbers: y_numbers.unwrap_or(numbers),
            x_label,
            y_label,
            axis_color,
            grid_color: grid_color
                .map(|color| color.0)
                .unwrap_or(defaults.grid_color),
            tick_color: tick_color.map(|color| color.0).unwrap_or(axis_color),
            number_color: number_color.map(|color| color.0).unwrap_or(axis_color),
            label_color: label_color.map(|color| color.0).unwrap_or(axis_color),
            number_size: None,
            label_size: None,
            axis_width,
            grid_width,
            tick_width,
            tick_length,
            auto_fit,
            x_length,
            y_length,
            tips,
        };
        Ok(PyDrawable(
            self.inner
                .lock()
                .expect("scene canvas poisoned")
                .axes(x, y, config),
        ))
    }

    #[pyo3(name = "_legacy_axes_3d", signature = (
        x=None, y=None, z=None,
        x_range=None, y_range=None, z_range=None,
        grid=true, ticks=true, numbers=true, labels=true,
        x_axis=true, y_axis=true, z_axis=true,
        xy_grid=None, xz_grid=None, yz_grid=None,
        x_ticks=None, y_ticks=None, z_ticks=None,
        x_numbers=None, y_numbers=None, z_numbers=None,
        x_label=None, y_label=None, z_label=None,
        label_mode="billboard",
        axis_color=None, grid_color=None, tick_color=None,
        number_color=None, label_color=None,
        axis_width=3.0, grid_width=1.0, tick_width=2.0, tick_length=8.0,
        auto_fit=true, x_length=None, y_length=None, z_length=None, tips=true
    ))]
    fn legacy_axes_3d(
        &self,
        x: Option<Bound<'_, PyAny>>,
        y: Option<Bound<'_, PyAny>>,
        z: Option<Bound<'_, PyAny>>,
        x_range: Option<Bound<'_, PyAny>>,
        y_range: Option<Bound<'_, PyAny>>,
        z_range: Option<Bound<'_, PyAny>>,
        grid: bool,
        ticks: bool,
        numbers: bool,
        labels: bool,
        x_axis: bool,
        y_axis: bool,
        z_axis: bool,
        xy_grid: Option<bool>,
        xz_grid: Option<bool>,
        yz_grid: Option<bool>,
        x_ticks: Option<bool>,
        y_ticks: Option<bool>,
        z_ticks: Option<bool>,
        x_numbers: Option<bool>,
        y_numbers: Option<bool>,
        z_numbers: Option<bool>,
        x_label: Option<String>,
        y_label: Option<String>,
        z_label: Option<String>,
        label_mode: &str,
        axis_color: Option<PyColor>,
        grid_color: Option<PyColor>,
        tick_color: Option<PyColor>,
        number_color: Option<PyColor>,
        label_color: Option<PyColor>,
        axis_width: f64,
        grid_width: f64,
        tick_width: f64,
        tick_length: f64,
        auto_fit: bool,
        x_length: Option<f64>,
        y_length: Option<f64>,
        z_length: Option<f64>,
        tips: bool,
    ) -> PyResult<PyDrawable> {
        let parse_range =
            |opt: Option<Bound<PyAny>>, default: (f64, f64, f64)| -> PyResult<(f64, f64, f64)> {
                if let Some(b) = opt {
                    if let Ok(v) = b.extract::<(f64, f64, f64)>() {
                        Ok(v)
                    } else if let Ok(v) = b.extract::<(f64, f64)>() {
                        Ok((v.0, v.1, 1.0))
                    } else if let Ok(v) = b.extract::<Vec<f64>>() {
                        if v.len() == 2 {
                            Ok((v[0], v[1], 1.0))
                        } else if v.len() == 3 {
                            Ok((v[0], v[1], v[2]))
                        } else {
                            Err(pyo3::exceptions::PyValueError::new_err(
                                "range must be (min, max) or (min, max, step)",
                            ))
                        }
                    } else {
                        Err(pyo3::exceptions::PyValueError::new_err(
                            "range must be (min, max) or (min, max, step)",
                        ))
                    }
                } else {
                    Ok(default)
                }
            };
        let xr = parse_range(x.or(x_range), (-5.0, 5.0, 1.0))?;
        let yr = parse_range(y.or(y_range), (-5.0, 5.0, 1.0))?;
        let zr = parse_range(z.or(z_range), (-3.0, 3.0, 1.0))?;
        if ![xr.0, xr.1, xr.2, yr.0, yr.1, yr.2, zr.0, zr.1, zr.2]
            .iter()
            .all(|v| v.is_finite())
            || xr.0 >= xr.1
            || yr.0 >= yr.1
            || zr.0 >= zr.1
            || xr.2 <= 0.0
            || yr.2 <= 0.0
            || zr.2 <= 0.0
        {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "axis ranges must be finite (min, max, step) with min < max and step > 0",
            ));
        }
        if [axis_width, grid_width, tick_width, tick_length]
            .iter()
            .any(|v| !v.is_finite() || *v < 0.0)
        {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "axis, grid, and tick dimensions must be finite and non-negative",
            ));
        }
        for v in [x_length, y_length, z_length].into_iter().flatten() {
            if !v.is_finite() || v <= 0.0 {
                return Err(pyo3::exceptions::PyValueError::new_err(
                    "length must be finite and positive",
                ));
            }
        }
        let label_mode_parsed = match label_mode.to_lowercase().as_str() {
            "billboard" => LabelMode::Billboard,
            "hud" => LabelMode::Hud,
            _ => {
                return Err(pyo3::exceptions::PyValueError::new_err(
                    "label_mode must be 'billboard' or 'hud'",
                ));
            }
        };
        let defaults = Axes3DConfig::default();
        let axis_color = axis_color.map(|c| c.0).unwrap_or(defaults.axis_color);
        let config = Axes3DConfig {
            grid,
            ticks,
            numbers,
            labels,
            x_axis,
            y_axis,
            z_axis,
            xy_grid: xy_grid.unwrap_or(grid),
            xz_grid: xz_grid.unwrap_or(grid),
            yz_grid: yz_grid.unwrap_or(grid),
            x_ticks: x_ticks.unwrap_or(ticks),
            y_ticks: y_ticks.unwrap_or(ticks),
            z_ticks: z_ticks.unwrap_or(ticks),
            x_numbers: x_numbers.unwrap_or(numbers),
            y_numbers: y_numbers.unwrap_or(numbers),
            z_numbers: z_numbers.unwrap_or(numbers),
            x_label,
            y_label,
            z_label,
            label_mode: label_mode_parsed,
            axis_color,
            grid_color: grid_color.map(|c| c.0).unwrap_or(defaults.grid_color),
            tick_color: tick_color.map(|c| c.0).unwrap_or(axis_color),
            number_color: number_color.map(|c| c.0).unwrap_or(axis_color),
            label_color: label_color.map(|c| c.0).unwrap_or(axis_color),
            axis_width,
            grid_width,
            tick_width,
            tick_length,
            auto_fit,
            x_length,
            y_length,
            z_length,
            tips,
        };
        Ok(PyDrawable(
            self.inner
                .lock()
                .expect("scene canvas poisoned")
                .axes_3d(xr, yr, zr, config),
        ))
    }

    #[pyo3(name = "_legacy_plot", signature = (axes, function, x, samples=160))]
    fn legacy_plot(
        &self,
        axes: &PyDrawable,
        function: Bound<'_, PyAny>,
        x: (f64, f64),
        samples: usize,
    ) -> PyResult<PyDrawable> {
        let Some(((x_min, x_max, _), (y_min, y_max, _), config)) = axes.0.axes_info() else {
            return Err(pyo3::exceptions::PyTypeError::new_err(
                "plot() first argument must be an axes drawable",
            ));
        };
        if samples < 2 {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "samples must be at least 2",
            ));
        }
        if !x.0.is_finite() || !x.1.is_finite() || x.0 >= x.1 {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "x range must be finite with min < max",
            ));
        }
        // Compute same scale as in compile.rs styled_axes (manim x_length/y_length or auto_fit)
        let canvas = self.inner.lock().expect("scene canvas poisoned");
        let avail_w = if canvas.width == 0 {
            800.0
        } else {
            canvas.width as f64 - canvas.margin.left - canvas.margin.right
        };
        let avail_h = if canvas.height == 0 {
            480.0
        } else {
            canvas.height as f64 - canvas.margin.top - canvas.margin.bottom
        };
        drop(canvas);
        let data_w = (x_max - x_min).max(1e-9);
        let data_h = (y_max - y_min).max(1e-9);
        let manim_frame_w: f64 = 14.222222222222221;
        let manim_frame_h: f64 = 8.0;
        let (scale_x, scale_y) = match (config.x_length, config.y_length) {
            (Some(xl), Some(yl)) => (
                xl * avail_w / manim_frame_w / data_w,
                yl * avail_h / manim_frame_h / data_h,
            ),
            (Some(xl), None) => {
                let s = xl * avail_w / manim_frame_w / data_w;
                (s, s)
            }
            (None, Some(yl)) => {
                let s = yl * avail_h / manim_frame_h / data_h;
                (s, s)
            }
            (None, None) if config.auto_fit => {
                let s = (avail_w / data_w).min(avail_h / data_h);
                (s, s)
            }
            (None, None) => (1.0, 1.0),
        };
        let x_center = (x_min + x_max) * 0.5;
        let y_center = (y_min + y_max) * 0.5;
        let mut points = Vec::with_capacity(samples);
        for i in 0..samples {
            let t = i as f64 / (samples - 1) as f64;
            let xv = x.0 + (x.1 - x.0) * t;
            let yv: f64 = function.call1((xv,))?.extract()?;
            let sx = (xv - x_center) * scale_x;
            let sy = (yv - y_center) * scale_y;
            // When neither auto_fit nor explicit length, keep raw data coords (manim-like no scaling)
            let (sx, sy) =
                if config.auto_fit || config.x_length.is_some() || config.y_length.is_some() {
                    (sx, sy)
                } else {
                    (xv, yv)
                };
            points.push((sx, sy));
        }
        Ok(PyDrawable(
            self.inner
                .lock()
                .expect("scene canvas poisoned")
                .polyline(&points),
        ))
    }

    /// manim `get_graph` — alias to `plot`
    #[pyo3(name = "_legacy_get_graph", signature = (axes, function, x, samples=160))]
    fn legacy_get_graph(
        &self,
        axes: &PyDrawable,
        function: Bound<'_, PyAny>,
        x: (f64, f64),
        samples: usize,
    ) -> PyResult<PyDrawable> {
        self.legacy_plot(axes, function, x, samples)
    }

    /// manim `plot_parametric_curve` — (t -> (x,y)) with t_range, respects auto_fit/x_length
    #[pyo3(name = "_legacy_plot_parametric_curve", signature = (axes, function, t, samples=160))]
    fn legacy_plot_parametric_curve(
        &self,
        axes: &PyDrawable,
        function: Bound<'_, PyAny>,
        t: (f64, f64),
        samples: usize,
    ) -> PyResult<PyDrawable> {
        let Some(((x_min, x_max, _), (y_min, y_max, _), config)) = axes.0.axes_info() else {
            return Err(pyo3::exceptions::PyTypeError::new_err(
                "plot_parametric_curve() first argument must be an axes drawable",
            ));
        };
        if samples < 2 {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "samples must be at least 2",
            ));
        }
        let canvas = self.inner.lock().expect("scene canvas poisoned");
        let avail_w = if canvas.width == 0 {
            800.0
        } else {
            canvas.width as f64 - canvas.margin.left - canvas.margin.right
        };
        let avail_h = if canvas.height == 0 {
            480.0
        } else {
            canvas.height as f64 - canvas.margin.top - canvas.margin.bottom
        };
        drop(canvas);
        let data_w = (x_max - x_min).max(1e-9);
        let data_h = (y_max - y_min).max(1e-9);
        let manim_frame_w: f64 = 14.222222222222221;
        let manim_frame_h: f64 = 8.0;
        let (scale_x, scale_y) = match (config.x_length, config.y_length) {
            (Some(xl), Some(yl)) => (
                xl * avail_w / manim_frame_w / data_w,
                yl * avail_h / manim_frame_h / data_h,
            ),
            (Some(xl), None) => {
                let s = xl * avail_w / manim_frame_w / data_w;
                (s, s)
            }
            (None, Some(yl)) => {
                let s = yl * avail_h / manim_frame_h / data_h;
                (s, s)
            }
            (None, None) if config.auto_fit => {
                let s = (avail_w / data_w).min(avail_h / data_h);
                (s, s)
            }
            _ => (1.0, 1.0),
        };
        let x_center = (x_min + x_max) * 0.5;
        let y_center = (y_min + y_max) * 0.5;
        let mut points = Vec::with_capacity(samples);
        for i in 0..samples {
            let tt = i as f64 / (samples - 1) as f64;
            let tv = t.0 + (t.1 - t.0) * tt;
            let (xv, yv): (f64, f64) = function.call1((tv,))?.extract()?;
            let (sx, sy) =
                if config.auto_fit || config.x_length.is_some() || config.y_length.is_some() {
                    ((xv - x_center) * scale_x, (yv - y_center) * scale_y)
                } else {
                    (xv, yv)
                };
            points.push((sx, sy));
        }
        Ok(PyDrawable(
            self.inner
                .lock()
                .expect("scene canvas poisoned")
                .polyline(&points),
        ))
    }

    #[pyo3(signature = (*content, role=None, style=None, flow=None, font=None, math_font=None, size=None, weight=None, italic=None, color=None, opacity=None, letter_spacing=None, word_spacing=None, baseline=None, wrap=None, text_align=None, line_spacing=None, max_lines=None, overflow=None, direction=None, hyphenate=None))]
    #[allow(clippy::too_many_arguments)]
    fn text<'py>(
        &self,
        py: Python<'py>,
        content: &Bound<'py, PyTuple>,
        role: Option<&str>,
        style: Option<PyTextStyle>,
        flow: Option<PyTextFlow>,
        font: Option<String>,
        math_font: Option<String>,
        size: Option<f64>,
        weight: Option<u16>,
        italic: Option<bool>,
        color: Option<PyColor>,
        opacity: Option<f32>,
        letter_spacing: Option<f64>,
        word_spacing: Option<f64>,
        baseline: Option<f64>,
        wrap: Option<&Bound<'py, PyAny>>,
        text_align: Option<&str>,
        line_spacing: Option<f64>,
        max_lines: Option<usize>,
        overflow: Option<&str>,
        direction: Option<&str>,
        hyphenate: Option<bool>,
    ) -> PyResult<Py<PyText>> {
        let spec = build_text_spec(
            content,
            role,
            style,
            flow,
            font,
            math_font,
            size,
            weight,
            italic,
            color,
            opacity,
            letter_spacing,
            word_spacing,
            baseline,
            wrap,
            text_align,
            line_spacing,
            max_lines,
            overflow,
            direction,
            hyphenate,
        )?;
        let handle = self
            .inner
            .lock()
            .expect("scene canvas poisoned")
            .text_spec(spec.clone());
        Py::new(py, PyText::initializer(handle, spec))
    }

    #[pyo3(signature = (size=2.0, *, material=None))]
    fn cube<'py>(
        &self,
        py: Python<'py>,
        size: f64,
        material: Option<PyMaterial3D>,
    ) -> PyResult<Py<PyPrimitive3D>> {
        let handle = self
            .inner
            .lock()
            .expect("scene canvas poisoned")
            .cube(size, material.map(|value| value.0).unwrap_or_default())
            .map_err(|error| pyo3::exceptions::PyValueError::new_err(error.to_string()))?;
        Py::new(py, PyPrimitive3D::initializer(handle))
    }

    #[pyo3(signature = (radius=1.0, *, segments=32, rings=16, material=None))]
    fn sphere<'py>(
        &self,
        py: Python<'py>,
        radius: f64,
        segments: u32,
        rings: u32,
        material: Option<PyMaterial3D>,
    ) -> PyResult<Py<PyPrimitive3D>> {
        let handle = self
            .inner
            .lock()
            .expect("scene canvas poisoned")
            .sphere(
                radius,
                segments,
                rings,
                material.map(|value| value.0).unwrap_or_default(),
            )
            .map_err(|error| pyo3::exceptions::PyValueError::new_err(error.to_string()))?;
        Py::new(py, PyPrimitive3D::initializer(handle))
    }

    #[pyo3(signature = (radius=1.0, height=2.0, *, segments=32, caps=true, material=None))]
    fn cylinder<'py>(
        &self,
        py: Python<'py>,
        radius: f64,
        height: f64,
        segments: u32,
        caps: bool,
        material: Option<PyMaterial3D>,
    ) -> PyResult<Py<PyPrimitive3D>> {
        let handle = self
            .inner
            .lock()
            .expect("scene canvas poisoned")
            .cylinder(
                radius,
                height,
                segments,
                caps,
                material.map(|value| value.0).unwrap_or_default(),
            )
            .map_err(|error| pyo3::exceptions::PyValueError::new_err(error.to_string()))?;
        Py::new(py, PyPrimitive3D::initializer(handle))
    }

    #[pyo3(signature = (radius=1.0, height=2.0, *, segments=32, cap=true, material=None))]
    fn cone<'py>(
        &self,
        py: Python<'py>,
        radius: f64,
        height: f64,
        segments: u32,
        cap: bool,
        material: Option<PyMaterial3D>,
    ) -> PyResult<Py<PyPrimitive3D>> {
        let handle = self
            .inner
            .lock()
            .expect("scene canvas poisoned")
            .cone(
                radius,
                height,
                segments,
                cap,
                material.map(|value| value.0).unwrap_or_default(),
            )
            .map_err(|error| pyo3::exceptions::PyValueError::new_err(error.to_string()))?;
        Py::new(py, PyPrimitive3D::initializer(handle))
    }

    #[pyo3(signature = (width=2.0, height=2.0, *, subdivisions=(1, 1), material=None))]
    fn plane<'py>(
        &self,
        py: Python<'py>,
        width: f64,
        height: f64,
        subdivisions: (u32, u32),
        material: Option<PyMaterial3D>,
    ) -> PyResult<Py<PyPrimitive3D>> {
        let handle = self
            .inner
            .lock()
            .expect("scene canvas poisoned")
            .plane(
                width,
                height,
                subdivisions,
                material.map(|value| value.0).unwrap_or_default(),
            )
            .map_err(|error| pyo3::exceptions::PyValueError::new_err(error.to_string()))?;
        Py::new(py, PyPrimitive3D::initializer(handle))
    }

    #[pyo3(signature = (preset="studio", intensity=1.0, shadows=true))]
    fn lighting_3d(&self, preset: &str, intensity: f32, shadows: bool) -> PyResult<()> {
        if !intensity.is_finite() || intensity < 0.0 {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "intensity must be finite and non-negative",
            ));
        }
        let enabled = match preset {
            "studio" => true,
            "none" => false,
            _ => {
                return Err(pyo3::exceptions::PyValueError::new_err(
                    "preset must be 'studio' or 'none'",
                ));
            }
        };
        self.inner
            .lock()
            .expect("scene canvas poisoned")
            .lighting_3d(enabled, intensity, shadows);
        Ok(())
    }

    #[pyo3(signature = (function, x_range=(-5.0, 5.0), y_range=(-5.0, 5.0), x_samples=20, y_samples=20, color=None))]
    fn surface(
        &self,
        function: Bound<'_, PyAny>,
        x_range: (f64, f64),
        y_range: (f64, f64),
        x_samples: usize,
        y_samples: usize,
        color: Option<PyColor>,
    ) -> PyResult<PyDrawable> {
        if ![x_range.0, x_range.1, y_range.0, y_range.1]
            .iter()
            .all(|v| v.is_finite())
            || x_range.0 >= x_range.1
            || y_range.0 >= y_range.1
        {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "x_range/y_range must be finite with min < max",
            ));
        }
        if x_samples < 2 || y_samples < 2 || x_samples > 200 || y_samples > 200 {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "x_samples/y_samples must be between 2 and 200",
            ));
        }
        let mut vertices: Vec<[f32; 3]> = Vec::with_capacity(x_samples * y_samples);
        for j in 0..y_samples {
            let y = y_range.0 + (y_range.1 - y_range.0) * (j as f64 / (y_samples - 1) as f64);
            for i in 0..x_samples {
                let x = x_range.0 + (x_range.1 - x_range.0) * (i as f64 / (x_samples - 1) as f64);
                let z: f64 = function.call1((x, y))?.extract()?;
                if !z.is_finite() {
                    return Err(pyo3::exceptions::PyValueError::new_err(
                        "surface function must return finite z",
                    ));
                }
                vertices.push([x as f32, y as f32, z as f32]);
            }
        }
        let mut indices: Vec<u32> = Vec::with_capacity((x_samples - 1) * (y_samples - 1) * 6);
        for j in 0..(y_samples - 1) {
            for i in 0..(x_samples - 1) {
                let a = (j * x_samples + i) as u32;
                let b = (j * x_samples + i + 1) as u32;
                let c = ((j + 1) * x_samples + i) as u32;
                let d = ((j + 1) * x_samples + i + 1) as u32;
                indices.extend_from_slice(&[a, b, d, a, d, c]);
            }
        }
        let color = color.map(|c| c.0);
        Ok(PyDrawable(
            self.inner
                .lock()
                .expect("scene canvas poisoned")
                .surface_mesh(vertices, indices, color),
        ))
    }

    #[pyo3(signature = (points, color=None, *, colors=None, colormap=None))]
    fn polyline_3d(
        &self,
        points: Vec<(f64, f64, f64)>,
        color: Option<PyColor>,
        colors: Option<Vec<PyColor>>,
        colormap: Option<String>,
    ) -> PyResult<PyDrawable> {
        if points.len() < 2 {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "polyline_3d requires at least 2 points",
            ));
        }
        for (x, y, z) in &points {
            if ![x, y, z].iter().all(|v| v.is_finite()) {
                return Err(pyo3::exceptions::PyValueError::new_err(
                    "all points must be finite",
                ));
            }
        }
        let verts: Vec<[f32; 3]> = points
            .iter()
            .map(|(x, y, z)| [*x as f32, *y as f32, *z as f32])
            .collect();

        // Resolve per-vertex colors: explicit list > colormap > uniform color
        let per_vertex: Option<Vec<gaanim_core::peniko::Color>> = if let Some(cols) = colors {
            if cols.len() != points.len() {
                return Err(pyo3::exceptions::PyValueError::new_err(format!(
                    "colors length {} must match points length {}",
                    cols.len(),
                    points.len()
                )));
            }
            Some(cols.into_iter().map(|c| c.0).collect())
        } else if let Some(name) = colormap {
            let name = name.to_lowercase();
            // Supported colormaps: inferno (Makie default), viridis, plasma
            let palette: Vec<(u8, u8, u8)> = match name.as_str() {
                "inferno" => vec![
                    (0, 0, 4),
                    (31, 12, 72),
                    (85, 15, 109),
                    (136, 34, 106),
                    (168, 50, 88),
                    (210, 72, 55),
                    (233, 100, 28),
                    (249, 157, 87),
                    (247, 209, 61),
                    (252, 255, 164),
                ],
                "viridis" => vec![
                    (68, 1, 84),
                    (59, 82, 139),
                    (33, 144, 140),
                    (94, 201, 98),
                    (253, 231, 37),
                ],
                "plasma" => vec![
                    (13, 8, 135),
                    (126, 3, 168),
                    (203, 70, 121),
                    (248, 149, 64),
                    (240, 249, 33),
                ],
                _ => {
                    return Err(pyo3::exceptions::PyValueError::new_err(
                        "colormap must be 'inferno', 'viridis' or 'plasma'",
                    ));
                }
            };
            let n = points.len();
            let mut out = Vec::with_capacity(n);
            for i in 0..n {
                let t = if n > 1 {
                    i as f32 / (n - 1) as f32
                } else {
                    0.0
                };
                let scaled = t * (palette.len() - 1) as f32;
                let idx = scaled.floor() as usize;
                let f = scaled - idx as f32;
                let (r, g, b) = if idx >= palette.len() - 1 {
                    palette[palette.len() - 1]
                } else {
                    let (r0, g0, b0) = palette[idx];
                    let (r1, g1, b1) = palette[idx + 1];
                    (
                        (r0 as f32 + (r1 as f32 - r0 as f32) * f) as u8,
                        (g0 as f32 + (g1 as f32 - g0 as f32) * f) as u8,
                        (b0 as f32 + (b1 as f32 - b0 as f32) * f) as u8,
                    )
                };
                out.push(gaanim_core::peniko::Color::from_rgb8(r, g, b));
            }
            Some(out)
        } else {
            None
        };

        let handle = if let Some(cols) = per_vertex {
            self.inner
                .lock()
                .expect("scene canvas poisoned")
                .polyline_3d_with_colors(verts, cols)
        } else {
            let verts2 = verts.clone();
            let mut h = self
                .inner
                .lock()
                .expect("scene canvas poisoned")
                .polyline_3d(verts2);
            if let Some(c) = color.clone() {
                h = h.fill(c.0);
            }
            // If we have uniform color via fill fallback, the per-vertex path is not needed.
            // We already handled colormap case above, so just return uniform.
            return Ok(PyDrawable(h));
        };
        // For per-vertex case, uniform `color` is still allowed as tint? ignore, vertex colors dominate.
        // But we allow `color` to override base if provided and no vertex? already handled.
        // If both per-vertex and uniform color provided, we respect per-vertex.
        Ok(PyDrawable(handle))
    }

    /// Compile full Typst markup into a vector drawable with optional custom page width (e.g. `width="16cm"` or `width=800`).
    #[pyo3(signature = (source, *, width=None))]
    fn typst(
        &self,
        py: pyo3::Python<'_>,
        source: &str,
        width: Option<pyo3::Py<pyo3::PyAny>>,
    ) -> PyResult<PyDrawable> {
        if source.trim().is_empty() {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "Typst source must not be empty",
            ));
        }
        let handle = if let Some(w) = width {
            // ponytail: String first, then f64 (covers int) — i64 branch is dead code
            let width_str = if let Ok(s) = w.extract::<String>(py) {
                s
            } else if let Ok(f) = w.extract::<f64>(py) {
                if f.is_finite() {
                    if f.fract() == 0.0 {
                        format!("{}pt", f as i64)
                    } else {
                        format!("{}pt", f)
                    }
                } else {
                    return Err(pyo3::exceptions::PyValueError::new_err(
                        "width must be a finite number",
                    ));
                }
            } else {
                return Err(pyo3::exceptions::PyTypeError::new_err(
                    "width must be a string (e.g. '16cm', '800pt') or a number",
                ));
            };
            if width_str.trim().is_empty() || width_str.contains(['\n', '\r', ';', '"', '\'']) {
                return Err(pyo3::exceptions::PyValueError::new_err(
                    "width must be a valid Typst length like '16cm' or '800pt'",
                ));
            }
            self.inner
                .lock()
                .expect("scene canvas poisoned")
                .typst_with_width(source, &width_str)
        } else {
            self.inner
                .lock()
                .expect("scene canvas poisoned")
                .typst(source)
        };

        Ok(PyDrawable(handle))
    }
    /// Auto-match by shape geometry — improved TransformMatchingShapes.
    ///
    /// Matches submobjects between `source` and `target` using Hungarian algorithm,
    /// normalized shape hashing, position, and color cost. Matched source submobjects
    /// morph into target submobjects, surplus source elements fade out, and new target
    /// elements fade in.
    #[pyo3(signature = (source, target, *, duration=1.0))]
    fn transform_matching_shapes(
        &self,
        source: &PyDrawable,
        target: &PyDrawable,
        duration: f64,
    ) -> PyResult<()> {
        if !duration.is_finite() || duration <= 0.0 {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "duration must be a finite positive number",
            ));
        }
        self.inner
            .lock()
            .expect("scene canvas poisoned")
            .transform_matching_shapes(&source.0, &target.0, duration);
        Ok(())
    }
    /// Auto-match by character/tex — improved TransformMatchingTex.
    ///
    /// Matches submobjects (glyphs/letters) between text/math equations using an
    /// order-preserving Longest Common Subsequence (LCS) algorithm on character keys,
    /// combined with Hungarian assignment for remaining elements.
    /// Alias for transform_matching_tex — manim TransformMatchingText compatibility.
    /// Generic auto-matching morph. `mode` is "shapes" or "tex".
    ///
    /// Performs auto-matching transform between `source` and `target` using the specified `mode`.
    #[pyo3(signature = (source, target, *, mode="shapes", duration=1.0))]
    fn transform_matching(
        &self,
        source: &PyDrawable,
        target: &PyDrawable,
        mode: &str,
        duration: f64,
    ) -> PyResult<()> {
        if !duration.is_finite() || duration <= 0.0 {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "duration must be a finite positive number",
            ));
        }
        self.inner
            .lock()
            .expect("scene canvas poisoned")
            .transform_matching(&source.0, &target.0, mode, duration);
        Ok(())
    }
    /// Dim an equation except for the requested semantic tags, then pulse them.
    /// Load a PNG, JPEG, or WebP image with optional size, fit mode, and crop.
    /// `crop` is `(x, y, width, height)` in source pixels, from the top-left.
    #[pyo3(signature = (path, *, width=None, height=None, fit="contain", crop=None))]
    fn image(
        &self,
        path: &str,
        width: Option<f64>,
        height: Option<f64>,
        fit: &str,
        crop: Option<(f64, f64, f64, f64)>,
    ) -> PyResult<PyDrawable> {
        let fit = match fit {
            "contain" => ImageFit::Contain,
            "cover" => ImageFit::Cover,
            "stretch" => ImageFit::Stretch,
            _ => {
                return Err(pyo3::exceptions::PyValueError::new_err(
                    "fit must be 'contain', 'cover', or 'stretch'",
                ));
            }
        };
        let options = ImageOptions {
            width,
            height,
            fit,
            crop: crop.map(|(x, y, width, height)| ImageCrop {
                x,
                y,
                width,
                height,
            }),
        };
        self.inner
            .lock()
            .expect("scene canvas poisoned")
            .image_with_options(path, options)
            .map(PyDrawable)
            .map_err(|error| match error {
                gaanim_api::canvas::ImageLoadError::Options(error) => {
                    pyo3::exceptions::PyValueError::new_err(error.to_string())
                }
                error => pyo3::exceptions::PyRuntimeError::new_err(error.to_string()),
            })
    }

    /// Load an SVG as an animatable group of vector paths.
    fn svg(&self, path: &str) -> PyResult<PyDrawable> {
        self.inner
            .lock()
            .expect("scene canvas poisoned")
            .svg(path)
            .map(PyDrawable)
            .map_err(|error| pyo3::exceptions::PyRuntimeError::new_err(error.to_string()))
    }

    /// Load a local glTF 2.0 model, selecting a scene by name or index.
    #[pyo3(signature = (path, *, scene=None))]
    fn gltf(&self, path: &str, scene: Option<&Bound<'_, PyAny>>) -> PyResult<PyDrawable> {
        let selector = match scene {
            None => gaanim_objects::prelude::GltfSceneSelector::Default,
            Some(value) => {
                if let Ok(index) = value.extract::<usize>() {
                    gaanim_objects::prelude::GltfSceneSelector::Index(index)
                } else if let Ok(name) = value.extract::<String>() {
                    gaanim_objects::prelude::GltfSceneSelector::Name(name)
                } else {
                    return Err(pyo3::exceptions::PyTypeError::new_err(
                        "scene must be a string, integer, or None",
                    ));
                }
            }
        };
        self.inner
            .lock()
            .expect("scene canvas poisoned")
            .gltf_scene(path, selector)
            .map(PyDrawable)
            .map_err(|error| pyo3::exceptions::PyValueError::new_err(error.to_string()))
    }

    fn group(&self, members: Vec<PyDrawable>) -> PyDrawable {
        let refs: Vec<&gaanim_api::canvas::DrawableHandle> = members.iter().map(|m| &m.0).collect();
        PyDrawable(
            self.inner
                .lock()
                .expect("scene canvas poisoned")
                .group(&refs),
        )
    }

    /// Create a labeled card with a native connector that follows `target`.
    #[pyo3(signature = (
        text,
        target,
        *,
        offset=(160.0, 96.0),
        width=240.0,
        height=72.0,
        background=None,
        color=None,
    ))]
    fn callout(
        &self,
        text: String,
        target: &PyDrawable,
        offset: (f64, f64),
        width: f64,
        height: f64,
        background: Option<PyColor>,
        color: Option<PyColor>,
    ) -> PyResult<PyDrawable> {
        if text.trim().is_empty() {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "callout text must not be empty",
            ));
        }
        if !offset.0.is_finite()
            || !offset.1.is_finite()
            || !width.is_finite()
            || !height.is_finite()
            || width <= 0.0
            || height <= 0.0
        {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "offset must be finite and width and height must be finite positive numbers",
            ));
        }

        let palette = {
            let scene = self.inner.lock().expect("scene canvas poisoned");
            component_palette(&scene)
        };
        let background = background.map(|color| color.0).unwrap_or(palette.panel);
        let color = color.map(|color| color.0).unwrap_or(palette.foreground);
        let mut scene = self.inner.lock().expect("scene canvas poisoned");
        let card = scene
            .rounded_rect(width, height, 12.0)
            .fill(background)
            .stroke(color, 2.0);
        card.follow_to(&target.0, offset.0, offset.1);
        let label = scene.text(&text).fill(color);
        label.follow_to(&target.0, offset.0, offset.1);
        let connector = scene
            .tracking_line(
                CanvasEndpoint::Entity(target.0.id),
                CanvasEndpoint::Entity(card.id),
            )
            .no_fill()
            .stroke(color, 2.0)
            .z_index(-1);
        Ok(PyDrawable(scene.group(&[&connector, &card, &label])))
    }

    /// Create a caption card positioned at the top or bottom safe edge.
    #[pyo3(signature = (
        text,
        *,
        position="bottom",
        width=720.0,
        height=92.0,
        margin=32.0,
        background=None,
        color=None,
    ))]
    fn caption(
        &self,
        text: String,
        position: &str,
        width: f64,
        height: f64,
        margin: f64,
        background: Option<PyColor>,
        color: Option<PyColor>,
    ) -> PyResult<PyDrawable> {
        if text.trim().is_empty() {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "caption text must not be empty",
            ));
        }
        if !width.is_finite()
            || !height.is_finite()
            || !margin.is_finite()
            || width <= 0.0
            || height <= 0.0
            || margin < 0.0
        {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "width and height must be finite positive numbers and margin must be non-negative",
            ));
        }
        let direction = match position.to_ascii_lowercase().as_str() {
            "bottom" => gaanim_api::canvas::Direction::Down,
            "top" => gaanim_api::canvas::Direction::Up,
            _ => {
                return Err(pyo3::exceptions::PyValueError::new_err(
                    "position must be 'top' or 'bottom'",
                ));
            }
        };
        let palette = {
            let scene = self.inner.lock().expect("scene canvas poisoned");
            component_palette(&scene)
        };
        let background = background.map(|color| color.0).unwrap_or(palette.panel);
        let color = color.map(|color| color.0).unwrap_or(palette.foreground);
        let mut scene = self.inner.lock().expect("scene canvas poisoned");
        let card = scene
            .rounded_rect(width, height, 14.0)
            .fill(background)
            .no_stroke();
        let label_spec = gaanim_text::prelude::TextSpec::new(
            vec![text.into()],
            Some(gaanim_text::prelude::TextRole::Caption),
            gaanim_text::prelude::TextStyle::default(),
            gaanim_text::prelude::TextFlow {
                wrap: gaanim_text::prelude::TextWrap::Width(width - 48.0),
                align: gaanim_text::prelude::TextAlign::Center,
                max_lines: Some(2),
                ..Default::default()
            },
        )
        .expect("caption card text is validated by its public arguments");
        let label = scene.text_spec(label_spec).fill(color);
        let caption = scene.group(&[&card, &label]).to_edge(direction, margin);
        Ok(PyDrawable(caption))
    }

    /// Create a centered title card with an optional subtitle and accent rule.
    #[pyo3(signature = (
        title,
        subtitle=None,
        *,
        width=760.0,
        height=320.0,
        panel=false,
        background=None,
        color=None,
        accent=None,
    ))]
    fn title_card(
        &self,
        title: String,
        subtitle: Option<String>,
        width: f64,
        height: f64,
        panel: bool,
        background: Option<PyColor>,
        color: Option<PyColor>,
        accent: Option<PyColor>,
    ) -> PyResult<PyDrawable> {
        if title.trim().is_empty() {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "title must not be empty",
            ));
        }
        if !width.is_finite() || !height.is_finite() || width <= 0.0 || height <= 0.0 {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "width and height must be finite positive numbers",
            ));
        }
        if subtitle
            .as_ref()
            .is_some_and(|subtitle| subtitle.trim().is_empty())
        {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "subtitle must not be empty when provided",
            ));
        }

        let palette = {
            let scene = self.inner.lock().expect("scene canvas poisoned");
            component_palette(&scene)
        };
        let background = background.map(|color| color.0).unwrap_or(palette.panel);
        let color = color.map(|color| color.0).unwrap_or(palette.foreground);
        let accent = accent.map(|color| color.0).unwrap_or(palette.accent);
        let mut scene = self.inner.lock().expect("scene canvas poisoned");
        let title_y = if subtitle.is_some() { 44.0 } else { 0.0 };
        let title_spec = gaanim_text::prelude::TextSpec::new(
            vec![title.into()],
            Some(gaanim_text::prelude::TextRole::Title),
            gaanim_text::prelude::TextStyle::default(),
            gaanim_text::prelude::TextFlow::default(),
        )
        .expect("title card validates title text");
        let title = scene.text_spec(title_spec).fill(color).at(0.0, title_y);
        let rule = scene
            .line(-width * 0.28, -12.0, width * 0.28, -12.0)
            .stroke(accent, 5.0);
        let mut members = Vec::new();
        if panel {
            members.push(
                scene
                    .rounded_rect(width, height, 24.0)
                    .fill(background)
                    .stroke(accent, 3.0),
            );
        }
        members.push(title);
        members.push(rule);
        if let Some(subtitle) = subtitle {
            let subtitle_spec = gaanim_text::prelude::TextSpec::new(
                vec![subtitle.into()],
                Some(gaanim_text::prelude::TextRole::Subtitle),
                gaanim_text::prelude::TextStyle::default(),
                gaanim_text::prelude::TextFlow::default(),
            )
            .expect("title card validates subtitle text");
            members.push(scene.text_spec(subtitle_spec).fill(color).at(0.0, -64.0));
        }
        let refs: Vec<&gaanim_api::canvas::DrawableHandle> = members.iter().collect();
        Ok(PyDrawable(scene.group(&refs)))
    }

    /// Create a vertically arranged editorial bullet list.
    #[pyo3(signature = (
        items,
        *,
        width=720.0,
        gap=68.0,
        bullet_radius=8.0,
        bullet_color=None,
        color=None,
    ))]
    fn bullets(
        &self,
        items: Vec<String>,
        width: f64,
        gap: f64,
        bullet_radius: f64,
        bullet_color: Option<PyColor>,
        color: Option<PyColor>,
    ) -> PyResult<PyDrawable> {
        if items.is_empty() || items.iter().any(|item| item.trim().is_empty()) {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "items must contain at least one non-empty string",
            ));
        }
        if !width.is_finite()
            || !gap.is_finite()
            || !bullet_radius.is_finite()
            || width <= 0.0
            || gap <= 0.0
            || bullet_radius <= 0.0
        {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "width, gap, and bullet_radius must be finite positive numbers",
            ));
        }
        let palette = {
            let scene = self.inner.lock().expect("scene canvas poisoned");
            component_palette(&scene)
        };
        let bullet_color = bullet_color.map(|color| color.0).unwrap_or(palette.accent);
        let color = color.map(|color| color.0).unwrap_or(palette.foreground);
        let mut scene = self.inner.lock().expect("scene canvas poisoned");
        let start_y = (items.len().saturating_sub(1) as f64 * gap) * 0.5;
        let bullet_x = -width * 0.5;
        let text_left = bullet_x + bullet_radius * 4.0;
        let text_width = (width - bullet_radius * 4.0).max(1.0);
        let label_x = text_left + text_width * 0.5;
        let mut members = Vec::with_capacity(items.len() * 2);
        for (index, item) in items.iter().enumerate() {
            let y = start_y - index as f64 * gap;
            members.push(scene.dot(bullet_radius).fill(bullet_color).at(bullet_x, y));
            let label_spec = gaanim_text::prelude::TextSpec::new(
                vec![item.clone().into()],
                Some(gaanim_text::prelude::TextRole::Body),
                gaanim_text::prelude::TextStyle::default(),
                gaanim_text::prelude::TextFlow {
                    wrap: gaanim_text::prelude::TextWrap::Width(text_width),
                    ..Default::default()
                },
            )
            .expect("bullet text is validated by its public arguments");
            members.push(scene.text_spec(label_spec).fill(color).at(label_x, y));
        }
        let refs: Vec<&gaanim_api::canvas::DrawableHandle> = members.iter().collect();
        Ok(PyDrawable(scene.group(&refs)))
    }

    /// Create a labeled bar chart for finite non-negative values.
    #[pyo3(name = "_legacy_bar_chart", signature = (values, *, labels=None, width=640.0, height=320.0, gap=20.0, color=None))]
    fn legacy_bar_chart(
        &self,
        values: Vec<f64>,
        labels: Option<Vec<String>>,
        width: f64,
        height: f64,
        gap: f64,
        color: Option<PyColor>,
    ) -> PyResult<PyDrawable> {
        if values.is_empty()
            || values
                .iter()
                .any(|value| !value.is_finite() || *value < 0.0)
        {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "values must contain finite non-negative numbers",
            ));
        }
        if !width.is_finite()
            || !height.is_finite()
            || !gap.is_finite()
            || width <= 0.0
            || height <= 0.0
            || gap < 0.0
        {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "width and height must be finite positive numbers and gap must be non-negative",
            ));
        }
        let labels =
            labels.unwrap_or_else(|| (1..=values.len()).map(|index| index.to_string()).collect());
        if labels.len() != values.len() || labels.iter().any(|label| label.trim().is_empty()) {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "labels must contain one non-empty label per value",
            ));
        }
        let available_width = width - gap * (values.len() as f64 + 1.0);
        if available_width <= 0.0 {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "width is too small for the requested number of bars and gap",
            ));
        }
        let max_value = values.iter().copied().fold(0.0_f64, f64::max).max(1.0);
        let bar_width = available_width / values.len() as f64;
        // Reserve room above the bars for value labels and below for category
        // labels, keeping both inside the requested chart bounds.
        let chart_height = height - 92.0;
        if chart_height <= 0.0 {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "height must be greater than 92",
            ));
        }
        let palette = {
            let scene = self.inner.lock().expect("scene canvas poisoned");
            component_palette(&scene)
        };
        let bar_color = color.map(|color| color.0).unwrap_or(palette.chart);
        let mut scene = self.inner.lock().expect("scene canvas poisoned");
        let baseline_y = -height * 0.5 + 28.0;
        let mut members = Vec::with_capacity(values.len() * 3 + 1);
        members.push(
            scene
                .line(-width * 0.5, baseline_y, width * 0.5, baseline_y)
                .stroke(palette.rule, 2.0),
        );
        for (index, (value, label)) in values.iter().zip(labels.iter()).enumerate() {
            let bar_height = chart_height * (*value / max_value);
            let x = -width * 0.5 + gap + bar_width * (index as f64 + 0.5) + gap * index as f64;
            members.push(
                scene
                    .rounded_rect(bar_width, bar_height.max(1.0), 6.0)
                    .fill(bar_color)
                    .stroke(palette.rule, 1.5)
                    .at(x, baseline_y + bar_height * 0.5),
            );
            let value_label = if value.fract().abs() < 1e-9 {
                format!("{value:.0}")
            } else {
                format!("{value:.1}")
            };
            members.push(
                scene
                    .text(&value_label)
                    .fill(palette.foreground)
                    .at(x, baseline_y + bar_height + 24.0),
            );
            members.push(
                scene
                    .text(label)
                    .fill(palette.muted)
                    .at(x, baseline_y - 28.0),
            );
        }
        let refs: Vec<&gaanim_api::canvas::DrawableHandle> = members.iter().collect();
        Ok(PyDrawable(scene.group(&refs)))
    }

    /// Create a compact technical table with a muted header and construction rules.
    #[pyo3(signature = (
        headers,
        rows,
        *,
        width=760.0,
        row_height=58.0,
        header_background=None,
        rule_color=None,
        color=None,
    ))]
    fn table(
        &self,
        headers: Vec<String>,
        rows: Vec<Vec<String>>,
        width: f64,
        row_height: f64,
        header_background: Option<PyColor>,
        rule_color: Option<PyColor>,
        color: Option<PyColor>,
    ) -> PyResult<PyDrawable> {
        if headers.is_empty() || headers.iter().any(|header| header.trim().is_empty()) {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "headers must contain at least one non-empty string",
            ));
        }
        if rows.is_empty()
            || rows.iter().any(|row| {
                row.len() != headers.len() || row.iter().any(|cell| cell.trim().is_empty())
            })
        {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "rows must contain at least one complete row of non-empty cells",
            ));
        }
        if !width.is_finite() || !row_height.is_finite() || width <= 0.0 || row_height <= 0.0 {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "width and row_height must be finite positive numbers",
            ));
        }

        let palette = {
            let scene = self.inner.lock().expect("scene canvas poisoned");
            component_palette(&scene)
        };
        let header_background = header_background
            .map(|color| color.0)
            .unwrap_or(palette.header);
        let rule_color = rule_color.map(|color| color.0).unwrap_or(palette.rule);
        let color = color.map(|color| color.0).unwrap_or(palette.foreground);
        let columns = headers.len() as f64;
        let total_height = row_height * (rows.len() as f64 + 1.0);
        let cell_width = width / columns;
        let top_y = total_height * 0.5;
        let mut scene = self.inner.lock().expect("scene canvas poisoned");
        let mut members = Vec::with_capacity((rows.len() + 1) * headers.len() + rows.len() + 3);

        members.push(
            scene
                .rounded_rect(width, row_height, 6.0)
                .fill(header_background)
                .at(0.0, top_y - row_height * 0.5),
        );
        for column in 1..headers.len() {
            let x = -width * 0.5 + cell_width * column as f64;
            members.push(
                scene
                    .line(x, -total_height * 0.5, x, total_height * 0.5)
                    .stroke(rule_color, 1.0),
            );
        }
        for row in 0..=(rows.len() + 1) {
            let y = top_y - row_height * row as f64;
            members.push(
                scene
                    .line(-width * 0.5, y, width * 0.5, y)
                    .stroke(rule_color, if row == 0 { 2.0 } else { 1.0 }),
            );
        }
        for (column, header) in headers.iter().enumerate() {
            let x = -width * 0.5 + cell_width * (column as f64 + 0.5);
            members.push(
                scene
                    .text(header)
                    .fill(color)
                    .at(x, top_y - row_height * 0.5),
            );
        }
        for (row_index, row) in rows.iter().enumerate() {
            let y = top_y - row_height * (row_index as f64 + 1.5);
            for (column, cell) in row.iter().enumerate() {
                let x = -width * 0.5 + cell_width * (column as f64 + 0.5);
                members.push(scene.text(cell).fill(color).at(x, y));
            }
        }
        let refs: Vec<&gaanim_api::canvas::DrawableHandle> = members.iter().collect();
        Ok(PyDrawable(scene.group(&refs)))
    }

    /// Create a restrained monospaced code block backed by Typst vector text.
    #[pyo3(signature = (
        source,
        *,
        language="text",
        width=760.0,
        height=300.0,
        font_size=20.0,
        background=None,
        color=None,
        accent=None,
    ))]
    fn code(
        &self,
        source: &str,
        language: &str,
        width: f64,
        height: f64,
        font_size: f64,
        background: Option<PyColor>,
        color: Option<PyColor>,
        accent: Option<PyColor>,
    ) -> PyResult<PyDrawable> {
        if source.trim().is_empty() {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "source must not be empty",
            ));
        }
        if language.trim().is_empty()
            || !language.chars().all(|character| {
                character.is_ascii_alphanumeric() || character == '-' || character == '_'
            })
        {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "language must contain only ASCII letters, digits, '-' or '_'",
            ));
        }
        if !width.is_finite()
            || !height.is_finite()
            || !font_size.is_finite()
            || width <= 0.0
            || height <= 0.0
            || font_size <= 0.0
        {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "width, height, and font_size must be finite positive numbers",
            ));
        }
        let palette = {
            let scene = self.inner.lock().expect("scene canvas poisoned");
            component_palette(&scene)
        };
        let background = background.map(|color| color.0).unwrap_or(palette.panel);
        let color = color.map(|color| color.0).unwrap_or(palette.foreground);
        let accent = accent.map(|color| color.0).unwrap_or(palette.accent);
        let content_width = (width - 64.0).max(1.0);
        let typst_source = format!(
            r#"#set text(font: "Consolas", size: {font_size}pt)
#block(width: {content_width}pt)[#raw("{}", block: true, lang: "{}")]"#,
            escape_typst_string(source),
            escape_typst_string(language),
        );
        let mut scene = self.inner.lock().expect("scene canvas poisoned");
        let panel = scene
            .rounded_rect(width, height, 10.0)
            .fill(background)
            .stroke(accent, 1.5);
        let rule = scene
            .line(
                -width * 0.5 + 24.0,
                height * 0.5 - 40.0,
                width * 0.5 - 24.0,
                height * 0.5 - 40.0,
            )
            .stroke(accent, 1.0);
        let label = scene
            .text(&language.to_ascii_uppercase())
            .fill(accent)
            .at(-width * 0.5 + 90.0, height * 0.5 - 20.0);
        // Typst hierarchies are centered on their visual bounds. Shift the
        // resulting raw block into the panel's reading column.
        let body = scene
            .typst(&typst_source)
            .fill(color)
            .at(-width * 0.25, -18.0);
        Ok(PyDrawable(scene.group(&[&panel, &rule, &label, &body])))
    }

    #[pyo3(signature = (name, transition=None, *, notes=None, template=None))]
    fn segment<'py>(
        slf: &Bound<'py, Self>,
        name: String,
        transition: Option<&PyTransitionType>,
        notes: Option<String>,
        template: Option<&Bound<'py, PyAny>>,
    ) -> PyResult<PySegment> {
        let template_name = template.and_then(|template| {
            template
                .getattr("__name__")
                .ok()
                .and_then(|name| name.extract::<String>().ok())
        });
        let scene = slf.borrow();
        let handle = scene
            .inner
            .lock()
            .expect("scene canvas poisoned")
            .segment_with(
                name,
                transition.map(|transition| transition.0.clone()),
                notes,
                template_name,
            )
            .map_err(|error| pyo3::exceptions::PyValueError::new_err(error.to_string()))?;
        drop(scene);
        Ok(PySegment {
            scene: slf.clone().unbind(),
            template: template.map(|template| template.clone().unbind()),
            inner: handle,
        })
    }

    fn link(
        &self,
        from: &PySegment,
        to: &PySegment,
        transition: &PyTransitionType,
    ) -> PyResult<()> {
        self.inner
            .lock()
            .expect("scene canvas poisoned")
            .link(&from.inner, &to.inner, transition.0.clone())
            .map_err(|error| pyo3::exceptions::PyValueError::new_err(error.to_string()))
    }

    /// Reuse one or more drawables in the active segment at the current cursor.
    #[pyo3(signature = (object, *others))]
    fn reuse(&self, object: &PyDrawable, others: &Bound<'_, PyTuple>) -> PyResult<()> {
        let drawables = drawable_args(object, others)?;
        self.inner
            .lock()
            .expect("scene canvas poisoned")
            .reuse_many(&drawables)
            .map_err(|error| pyo3::exceptions::PyValueError::new_err(error.to_string()))
    }

    /// Keep one or more drawables available across future segments.
    #[pyo3(signature = (object, *others))]
    fn persist(&self, object: &PyDrawable, others: &Bound<'_, PyTuple>) -> PyResult<()> {
        let drawables = drawable_args(object, others)?;
        self.inner
            .lock()
            .expect("scene canvas poisoned")
            .persist_many(&drawables)
            .map_err(|error| pyo3::exceptions::PyValueError::new_err(error.to_string()))
    }

    /// Stop persistence and attach one or more drawables to the active segment.
    #[pyo3(signature = (object, *others))]
    fn release(&self, object: &PyDrawable, others: &Bound<'_, PyTuple>) -> PyResult<()> {
        let drawables = drawable_args(object, others)?;
        self.inner
            .lock()
            .expect("scene canvas poisoned")
            .release_many(&drawables)
            .map_err(|error| pyo3::exceptions::PyValueError::new_err(error.to_string()))
    }

    fn wait(&self, duration: f64) {
        self.inner
            .lock()
            .expect("scene canvas poisoned")
            .wait(duration);
    }

    /// Insert an explicit zero-duration interactive stop.
    #[pyo3(signature = (name=None))]
    fn stop(&self, name: Option<String>) -> PyResult<()> {
        self.inner
            .lock()
            .expect("scene canvas poisoned")
            .stop(name)
            .map_err(|error| pyo3::exceptions::PyValueError::new_err(error.to_string()))
    }

    #[pyo3(signature = (anims, *, lag=None))]
    fn play(&self, anims: Vec<PyCanvasAnim>, lag: Option<f64>) {
        let anims = anims.into_iter().map(|anim| anim.inner).collect();
        let mut scene = self.inner.lock().expect("scene canvas poisoned");
        if let Some(lag) = lag {
            scene.play_with_lag(anims, lag);
        } else {
            scene.play(anims);
        }
    }

    fn fade_out_all(&self, duration: f64) {
        self.inner
            .lock()
            .expect("scene canvas poisoned")
            .fade_out_all(duration);
    }
    fn render(&self) -> PyResult<()> {
        if self
            .inner
            .lock()
            .expect("scene canvas poisoned")
            .clone()
            .render()
        {
            Ok(())
        } else {
            Err(pyo3::exceptions::PyRuntimeError::new_err(
                "Gaanim scenes can only be rendered inside the Gaanim application. \
                 Run your script with:  gaanim <script.py>",
            ))
        }
    }
    #[pyo3(signature = (
        path,
        fps=None,
        *,
        transparent=None,
        quality=None,
        aspect_ratio=None,
        width=None,
        height=None,
        start_time=None,
        end_time=None,
        segment=None,
        crf=None,
        encoder="auto",
        speed=None,
    ))]
    fn export(
        &self,
        path: &str,
        fps: Option<u32>,
        transparent: Option<bool>,
        quality: Option<&str>,
        aspect_ratio: Option<&str>,
        width: Option<u32>,
        height: Option<u32>,
        start_time: Option<f64>,
        end_time: Option<f64>,
        segment: Option<&str>,
        crf: Option<u32>,
        encoder: &str,
        speed: Option<&str>,
    ) -> PyResult<()> {
        let canvas = self.inner.lock().expect("scene canvas poisoned").clone();
        let mut config = ExportConfig::new(path);
        config.width = canvas.width;
        config.height = canvas.height;
        config.aspect_ratio = AspectRatioPreset::Custom;
        config.headless = true;

        if let Some(quality) = quality {
            config.quality = parse_quality(quality)?;
        }
        if let Some(aspect_ratio) = aspect_ratio {
            config.aspect_ratio = parse_aspect_ratio(aspect_ratio)?;
        }
        if quality.is_some() || aspect_ratio.is_some() {
            config = config.apply_presets();
        }

        if let Some(fps) = fps {
            if fps == 0 {
                return Err(pyo3::exceptions::PyValueError::new_err(
                    "fps must be greater than zero",
                ));
            }
            config.fps = fps;
        }
        if let Some(width) = width {
            if width == 0 {
                return Err(pyo3::exceptions::PyValueError::new_err(
                    "width must be greater than zero",
                ));
            }
            config.width = width;
            config.aspect_ratio = AspectRatioPreset::Custom;
        }
        if let Some(height) = height {
            if height == 0 {
                return Err(pyo3::exceptions::PyValueError::new_err(
                    "height must be greater than zero",
                ));
            }
            config.height = height;
            config.aspect_ratio = AspectRatioPreset::Custom;
        }
        if let Some(transparent) = transparent {
            config.transparent = transparent;
        }
        if let Some(crf) = crf {
            if crf > 51 {
                return Err(pyo3::exceptions::PyValueError::new_err(
                    "crf must be between 0 and 51",
                ));
            }
            config.crf = crf;
        }
        if let Some(start_time) = start_time {
            if !start_time.is_finite() || start_time < 0.0 {
                return Err(pyo3::exceptions::PyValueError::new_err(
                    "start_time must be a finite non-negative number",
                ));
            }
            config.start_time = Some(start_time);
        }
        if let Some(end_time) = end_time {
            if !end_time.is_finite() || end_time <= 0.0 {
                return Err(pyo3::exceptions::PyValueError::new_err(
                    "end_time must be a finite positive number",
                ));
            }
            config.end_time = Some(end_time);
        }
        if let (Some(start_time), Some(end_time)) = (config.start_time, config.end_time) {
            if end_time <= start_time {
                return Err(pyo3::exceptions::PyValueError::new_err(
                    "end_time must be greater than start_time",
                ));
            }
        }
        if segment.is_some() && (start_time.is_some() || end_time.is_some()) {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "segment cannot be combined with start_time or end_time",
            ));
        }

        config.video_encoder = parse_encoder(encoder)?;
        if let Some(speed) = speed {
            config.encoding_speed = parse_encoding_speed(speed)?;
        }

        match segment {
            Some("*") => export_canvas_segments(canvas, path, config)
                .map(|_| ())
                .map_err(segment_export_error),
            Some(segment_name) => {
                export_canvas_segment(canvas, segment_name, config).map_err(segment_export_error)
            }
            None => export_canvas(canvas, config)
                .map_err(|error| pyo3::exceptions::PyRuntimeError::new_err(error.to_string())),
        }
    }

    /// Render exact timeline seeks into PNG snapshots and a comparison manifest.
    fn snapshots(&self, directory: &str, times: Vec<f64>) -> PyResult<usize> {
        let scene = self.inner.lock().expect("scene canvas poisoned").clone();
        gaanim_diff::capture_canvas(scene, directory, &times)
            .map(|manifest| manifest.snapshots.len())
            .map_err(|error| pyo3::exceptions::PyRuntimeError::new_err(error.to_string()))
    }

    // -- Reactive objects --

    /// Create a dot positioned at the normalized arc-length of a sampled curve.
    fn point_on_curve(
        &self,
        curve: &PyDrawable,
        tracker: Bound<'_, PyAny>,
    ) -> PyResult<PyDrawable> {
        let (tracker, _) = reactive_scalar(tracker)?;
        Ok(PyDrawable(
            self.inner
                .lock()
                .expect("scene canvas poisoned")
                .point_on_curve(&curve.0, &tracker),
        ))
    }

    #[pyo3(signature = (curve, tracker, length=80.0))]
    fn tangent_on_curve(
        &self,
        curve: &PyDrawable,
        tracker: Bound<'_, PyAny>,
        length: f64,
    ) -> PyResult<PyDrawable> {
        let (tracker, _) = reactive_scalar(tracker)?;
        Ok(PyDrawable(
            self.inner
                .lock()
                .expect("scene canvas poisoned")
                .tangent_on_curve(&curve.0, &tracker, length),
        ))
    }

    #[pyo3(signature = (curve, tracker, length=80.0))]
    fn normal_on_curve(
        &self,
        curve: &PyDrawable,
        tracker: Bound<'_, PyAny>,
        length: f64,
    ) -> PyResult<PyDrawable> {
        let (tracker, _) = reactive_scalar(tracker)?;
        Ok(PyDrawable(
            self.inner
                .lock()
                .expect("scene canvas poisoned")
                .normal_on_curve(&curve.0, &tracker, length),
        ))
    }

    #[pyo3(signature = (curve, tracker, window=0.02))]
    fn curvature_on_curve(
        &self,
        curve: &PyDrawable,
        tracker: Bound<'_, PyAny>,
        window: f64,
    ) -> PyResult<PyDrawable> {
        let (tracker, _) = reactive_scalar(tracker)?;
        Ok(PyDrawable(
            self.inner
                .lock()
                .expect("scene canvas poisoned")
                .curvature_on_curve(&curve.0, &tracker, window),
        ))
    }

    #[pyo3(signature = (tracker, cx, cy, radius, start_angle, sweep_scale=1.0, sweep_offset=0.0))]
    fn always_redraw_arc(
        &self,
        tracker: Bound<'_, PyAny>,
        cx: f64,
        cy: f64,
        radius: f64,
        start_angle: f64,
        sweep_scale: f64,
        sweep_offset: f64,
    ) -> PyResult<PyDrawable> {
        let (tracker, current) = reactive_scalar(tracker)?;
        Ok(PyDrawable(
            self.inner
                .lock()
                .expect("scene canvas poisoned")
                .always_redraw_arc(
                    &tracker,
                    cx,
                    cy,
                    radius,
                    start_angle,
                    current,
                    sweep_scale,
                    sweep_offset,
                ),
        ))
    }

    #[pyo3(signature = (source, *, dissipating_time=None, max_points=None, min_distance=1.0))]
    fn traced_path(
        &self,
        source: &PyDrawable,
        dissipating_time: Option<f64>,
        max_points: Option<usize>,
        min_distance: f64,
    ) -> PyResult<PyDrawable> {
        if !min_distance.is_finite() || min_distance < 0.0 {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "min_distance must be finite and non-negative",
            ));
        }
        if let Some(n) = max_points {
            if n == 0 {
                return Err(pyo3::exceptions::PyValueError::new_err(
                    "max_points must be positive when provided",
                ));
            }
        }
        if let Some(duration) = dissipating_time {
            if !duration.is_finite() || duration <= 0.0 {
                return Err(pyo3::exceptions::PyValueError::new_err(
                    "dissipating_time must be finite and greater than zero",
                ));
            }
        }
        Ok(PyDrawable(
            self.inner
                .lock()
                .expect("scene canvas poisoned")
                .traced_path_with_options(&source.0, dissipating_time, max_points, min_distance),
        ))
    }

    /// 3D traced path — accumulates the 3D world position of `source` as a `LineList`.
    /// `colormap` may be "inferno", "viridis" or "plasma" to color by time (like Makie).
    #[pyo3(signature = (source, *, colormap=None, dissipating_time=None, max_points=None, min_distance=0.1))]
    fn traced_path_3d(
        &self,
        source: &PyDrawable,
        colormap: Option<String>,
        dissipating_time: Option<f64>,
        max_points: Option<usize>,
        min_distance: f64,
    ) -> PyResult<PyDrawable> {
        if !min_distance.is_finite() || min_distance < 0.0 {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "min_distance must be finite and non-negative",
            ));
        }
        if let Some(n) = max_points {
            if n == 0 {
                return Err(pyo3::exceptions::PyValueError::new_err(
                    "max_points must be positive when provided",
                ));
            }
        }
        if let Some(duration) = dissipating_time {
            if !duration.is_finite() || duration <= 0.0 {
                return Err(pyo3::exceptions::PyValueError::new_err(
                    "dissipating_time must be finite and greater than zero",
                ));
            }
        }
        if let Some(ref name) = colormap {
            let low = name.to_lowercase();
            if !["inferno", "viridis", "plasma"].contains(&low.as_str()) {
                return Err(pyo3::exceptions::PyValueError::new_err(
                    "colormap must be 'inferno', 'viridis' or 'plasma'",
                ));
            }
        }
        Ok(PyDrawable(
            self.inner
                .lock()
                .expect("scene canvas poisoned")
                .traced_path_3d_with_options(
                    &source.0,
                    colormap,
                    max_points,
                    min_distance,
                    dissipating_time,
                ),
        ))
    }

    fn tracking_line(&self, from: Bound<'_, PyAny>, to: Bound<'_, PyAny>) -> PyResult<PyDrawable> {
        let from = resolve_endpoint(&from)?;
        let to = resolve_endpoint(&to)?;
        Ok(PyDrawable(
            self.inner
                .lock()
                .expect("scene canvas poisoned")
                .tracking_line(from, to),
        ))
    }

    fn point_ref(&self, x: Bound<'_, PyAny>, y: Bound<'_, PyAny>) -> PyResult<PyPointRef> {
        let point = self
            .inner
            .lock()
            .expect("scene canvas poisoned")
            .point_ref(extract_expr(x)?, extract_expr(y)?);
        Ok(PyPointRef(point))
    }

    fn offset_point(
        &self,
        origin: Bound<'_, PyAny>,
        dx: Bound<'_, PyAny>,
        dy: Bound<'_, PyAny>,
    ) -> PyResult<PyPointRef> {
        let point = self
            .inner
            .lock()
            .expect("scene canvas poisoned")
            .offset_point(
                resolve_endpoint(&origin)?,
                extract_expr(dx)?,
                extract_expr(dy)?,
            );
        Ok(PyPointRef(point))
    }

    #[pyo3(signature = (from, to, *, alpha=0.5, offset=(0.0, 0.0)))]
    fn point_between(
        &self,
        from: Bound<'_, PyAny>,
        to: Bound<'_, PyAny>,
        alpha: f64,
        offset: (f64, f64),
    ) -> PyResult<PyPointRef> {
        if !alpha.is_finite() || !offset.0.is_finite() || !offset.1.is_finite() {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "alpha and offset must be finite",
            ));
        }
        let point = self
            .inner
            .lock()
            .expect("scene canvas poisoned")
            .point_between(
                resolve_endpoint(&from)?,
                resolve_endpoint(&to)?,
                alpha,
                gaanim_core::glam::DVec3::new(offset.0, offset.1, 0.0),
            );
        Ok(PyPointRef(point))
    }

    fn polar_point(
        &self,
        origin: Bound<'_, PyAny>,
        radius: Bound<'_, PyAny>,
        angle: Bound<'_, PyAny>,
    ) -> PyResult<PyPointRef> {
        let point = self
            .inner
            .lock()
            .expect("scene canvas poisoned")
            .polar_point(
                resolve_endpoint(&origin)?,
                extract_expr(radius)?,
                extract_expr(angle)?,
            );
        Ok(PyPointRef(point))
    }

    #[pyo3(signature = (from, to, *, width=8.0))]
    fn bar_between(
        &self,
        from: Bound<'_, PyAny>,
        to: Bound<'_, PyAny>,
        width: f64,
    ) -> PyResult<PyDrawable> {
        if !width.is_finite() || width <= 0.0 {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "width must be finite and greater than zero",
            ));
        }
        let from = resolve_endpoint(&from)?;
        let to = resolve_endpoint(&to)?;
        Ok(PyDrawable(
            self.inner
                .lock()
                .expect("scene canvas poisoned")
                .bar_between(from, to, width),
        ))
    }

    #[pyo3(signature = (from, to, coils=8, amplitude=12.0, crossing=0.0, start_straight=12.0, end_straight=12.0))]
    fn spring_between(
        &self,
        from: Bound<'_, PyAny>,
        to: Bound<'_, PyAny>,
        coils: usize,
        amplitude: f64,
        crossing: f64,
        start_straight: f64,
        end_straight: f64,
    ) -> PyResult<PyDrawable> {
        if !start_straight.is_finite() || start_straight < 0.0 {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "start_straight must be finite and non-negative",
            ));
        }
        if !end_straight.is_finite() || end_straight < 0.0 {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "end_straight must be finite and non-negative",
            ));
        }
        let from = resolve_endpoint(&from)?;
        let to = resolve_endpoint(&to)?;
        Ok(PyDrawable(
            self.inner
                .lock()
                .expect("scene canvas poisoned")
                .spring_between_with_options(
                    from,
                    to,
                    coils,
                    amplitude,
                    crossing,
                    start_straight,
                    end_straight,
                ),
        ))
    }

    #[pyo3(signature = (from, to, offset, *, label=None, show_value=false, value=None, format=".2f", unit=None, scale=1.0, label_gap=10.0, label_orientation="upright", font_size=None, color=None, line_width=3.0, extension_style="solid", dash_length=12.0, gap_length=8.0))]
    #[allow(clippy::too_many_arguments)]
    fn dimension_between<'py>(
        &self,
        py: Python<'py>,
        from: Bound<'_, PyAny>,
        to: Bound<'_, PyAny>,
        offset: f64,
        label: Option<String>,
        show_value: bool,
        value: Option<Bound<'py, PyAny>>,
        format: &str,
        unit: Option<String>,
        scale: f64,
        label_gap: f64,
        label_orientation: &str,
        font_size: Option<f64>,
        color: Option<PyColor>,
        line_width: f64,
        extension_style: &str,
        dash_length: f64,
        gap_length: f64,
    ) -> PyResult<Py<PyDimension>> {
        if !offset.is_finite() {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "offset must be finite",
            ));
        }
        if !scale.is_finite() || scale <= 0.0 {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "scale must be finite and greater than zero",
            ));
        }
        if !label_gap.is_finite() || label_gap < 0.0 {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "label_gap must be finite and non-negative",
            ));
        }
        if font_size.is_some_and(|size| !size.is_finite() || size <= 0.0) {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "font_size must be finite and greater than zero",
            ));
        }
        if !line_width.is_finite() || line_width <= 0.0 {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "line_width must be finite and greater than zero",
            ));
        }
        if !dash_length.is_finite()
            || dash_length <= 0.0
            || !gap_length.is_finite()
            || gap_length <= 0.0
        {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "dash_length and gap_length must be finite and greater than zero",
            ));
        }
        let extension_style = match extension_style {
            "solid" => DimensionExtensionStyle::Solid,
            "dashed" => DimensionExtensionStyle::Dashed,
            _ => {
                return Err(pyo3::exceptions::PyValueError::new_err(
                    "extension_style must be 'solid' or 'dashed'",
                ))
            }
        };
        let orientation = match label_orientation {
            "upright" => gaanim_animation::DimensionLabelOrientation::Upright,
            "aligned" => gaanim_animation::DimensionLabelOrientation::Aligned,
            _ => {
                return Err(pyo3::exceptions::PyValueError::new_err(
                    "label_orientation must be 'upright' or 'aligned'",
                ));
            }
        };
        let from = resolve_endpoint(&from)?;
        let to = resolve_endpoint(&to)?;
        let value = value.map(extract_expr).transpose()?;
        let handle = self
            .inner
            .lock()
            .expect("scene canvas poisoned")
            .dimension_between_with_options(
                from,
                to,
                offset,
                DimensionOptions {
                    label,
                    show_value,
                    value,
                    format: format.to_owned(),
                    unit,
                    scale,
                    label_gap,
                    label_orientation: orientation,
                    font_size,
                    color: color.map(|value| value.0),
                    line_width,
                    extension_style,
                    dash_length,
                    gap_length,
                },
            )
            .map_err(|error| pyo3::exceptions::PyValueError::new_err(error.to_string()))?;
        Py::new(py, PyDimension::initializer(handle))
    }

    #[pyo3(signature = (vertex, from, to, *, radius=64.0, label=None, show_value=false, format=".1f", unit="deg", sweep="minor", arrowheads="both", label_gap=12.0, label_orientation="upright", show_extensions=true, font_size=None, color=None))]
    #[allow(clippy::too_many_arguments)]
    fn angle_between<'py>(
        &self,
        py: Python<'py>,
        vertex: Bound<'_, PyAny>,
        from: Bound<'_, PyAny>,
        to: Bound<'_, PyAny>,
        radius: f64,
        label: Option<String>,
        show_value: bool,
        format: &str,
        unit: &str,
        sweep: &str,
        arrowheads: &str,
        label_gap: f64,
        label_orientation: &str,
        show_extensions: bool,
        font_size: Option<f64>,
        color: Option<PyColor>,
    ) -> PyResult<Py<PyAngleDimension>> {
        if !radius.is_finite() || radius <= 0.0 {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "radius must be finite and positive",
            ));
        }
        if !label_gap.is_finite() || label_gap < 0.0 {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "label_gap must be finite and non-negative",
            ));
        }
        if !matches!(unit, "deg" | "rad") {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "unit must be 'deg' or 'rad'",
            ));
        }
        let sweep = match sweep {
            "minor" => gaanim_animation::AngleSweep::Minor,
            "major" => gaanim_animation::AngleSweep::Major,
            "cw" => gaanim_animation::AngleSweep::Clockwise,
            "ccw" => gaanim_animation::AngleSweep::CounterClockwise,
            _ => {
                return Err(pyo3::exceptions::PyValueError::new_err(
                    "sweep must be 'minor', 'major', 'cw', or 'ccw'",
                ))
            }
        };
        let arrowheads = match arrowheads {
            "none" => gaanim_animation::AngleArrowheads::None,
            "start" => gaanim_animation::AngleArrowheads::Start,
            "end" => gaanim_animation::AngleArrowheads::End,
            "both" => gaanim_animation::AngleArrowheads::Both,
            _ => {
                return Err(pyo3::exceptions::PyValueError::new_err(
                    "arrowheads must be 'none', 'start', 'end', or 'both'",
                ))
            }
        };
        let orientation = match label_orientation {
            "upright" => gaanim_animation::DimensionLabelOrientation::Upright,
            "aligned" => gaanim_animation::DimensionLabelOrientation::Aligned,
            _ => {
                return Err(pyo3::exceptions::PyValueError::new_err(
                    "label_orientation must be 'upright' or 'aligned'",
                ))
            }
        };
        let handle = self
            .inner
            .lock()
            .expect("scene canvas poisoned")
            .angle_between_with_options(
                resolve_endpoint(&vertex)?,
                resolve_ray(&from)?,
                resolve_ray(&to)?,
                radius,
                AngleDimensionOptions {
                    label,
                    show_value,
                    format: format.into(),
                    unit: unit.into(),
                    sweep,
                    arrowheads,
                    label_gap,
                    label_orientation: orientation,
                    show_extensions,
                    font_size,
                    color: color.map(|value| value.0),
                },
            )
            .map_err(|error| pyo3::exceptions::PyValueError::new_err(error.to_string()))?;
        Py::new(py, PyAngleDimension::initializer(handle))
    }

    #[pyo3(signature = (from, to, *, label=None, show_value=false, format=".1f", unit=None, scale=1.0, label_gap=14.0, font_size=None, color=None))]
    #[allow(clippy::too_many_arguments)]
    fn vector_between(
        &self,
        py: Python<'_>,
        from: Bound<'_, PyAny>,
        to: Bound<'_, PyAny>,
        label: Option<String>,
        show_value: bool,
        format: &str,
        unit: Option<String>,
        scale: f64,
        label_gap: f64,
        font_size: Option<f64>,
        color: Option<PyColor>,
    ) -> PyResult<Py<PyForceVector>> {
        if !scale.is_finite() || scale <= 0.0 {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "scale must be finite and greater than zero",
            ));
        }
        if !label_gap.is_finite() || label_gap < 0.0 {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "label_gap must be finite and non-negative",
            ));
        }
        let handle = self
            .inner
            .lock()
            .expect("scene canvas poisoned")
            .vector_between_with_parts(
                resolve_endpoint(&from)?,
                resolve_endpoint(&to)?,
                label,
                show_value,
                format.into(),
                unit,
                scale,
                label_gap,
                font_size,
                color.map(|value| value.0),
            )
            .map_err(|error| pyo3::exceptions::PyValueError::new_err(error.to_string()))?;
        Py::new(py, PyForceVector::initializer(handle))
    }

    #[pyo3(signature = (origin, magnitude, *, direction=None, visual_scale=1.0, label=None, show_value=false, format=".1f", unit="N", label_gap=14.0, font_size=None, color=None))]
    #[allow(clippy::too_many_arguments)]
    fn force_at(
        &self,
        py: Python<'_>,
        origin: Bound<'_, PyAny>,
        magnitude: Bound<'_, PyAny>,
        direction: Option<Bound<'_, PyAny>>,
        visual_scale: f64,
        label: Option<String>,
        show_value: bool,
        format: &str,
        unit: &str,
        label_gap: f64,
        font_size: Option<f64>,
        color: Option<PyColor>,
    ) -> PyResult<Py<PyForceVector>> {
        validate_force_metrics(visual_scale, label_gap, font_size)?;
        let handle = self
            .inner
            .lock()
            .expect("scene canvas poisoned")
            .force_at(
                resolve_endpoint(&origin)?,
                extract_expr(magnitude)?,
                direction
                    .map(extract_expr)
                    .transpose()?
                    .unwrap_or_else(|| gaanim_expr::Expr::constant(0.0)),
                visual_scale,
                label,
                show_value,
                format.into(),
                Some(unit.to_owned()),
                label_gap,
                font_size,
                color.map(|value| value.0),
            )
            .map_err(|error| pyo3::exceptions::PyValueError::new_err(error.to_string()))?;
        Py::new(py, PyForceVector::initializer(handle))
    }

    #[pyo3(signature = (origin, fx, fy, *, visual_scale=1.0, label=None, show_value=false, format=".1f", unit="N", label_gap=14.0, font_size=None, color=None))]
    #[allow(clippy::too_many_arguments)]
    fn force_from_components(
        &self,
        py: Python<'_>,
        origin: Bound<'_, PyAny>,
        fx: Bound<'_, PyAny>,
        fy: Bound<'_, PyAny>,
        visual_scale: f64,
        label: Option<String>,
        show_value: bool,
        format: &str,
        unit: &str,
        label_gap: f64,
        font_size: Option<f64>,
        color: Option<PyColor>,
    ) -> PyResult<Py<PyForceVector>> {
        validate_force_metrics(visual_scale, label_gap, font_size)?;
        let handle = self
            .inner
            .lock()
            .expect("scene canvas poisoned")
            .force_from_components(
                resolve_endpoint(&origin)?,
                extract_expr(fx)?,
                extract_expr(fy)?,
                visual_scale,
                label,
                show_value,
                format.into(),
                Some(unit.to_owned()),
                label_gap,
                font_size,
                color.map(|value| value.0),
            )
            .map_err(|error| pyo3::exceptions::PyValueError::new_err(error.to_string()))?;
        Py::new(py, PyForceVector::initializer(handle))
    }

    #[pyo3(signature = (point, *, kind="pin", direction=None, size=48.0, ground_length=70.0, color=None))]
    fn support_at<'py>(
        &self,
        py: Python<'py>,
        point: Bound<'_, PyAny>,
        kind: &str,
        direction: Option<crate::pylayout::PyDirection>,
        size: f64,
        ground_length: f64,
        color: Option<PyColor>,
    ) -> PyResult<Py<PySupport>> {
        if !matches!(
            kind,
            "fixed" | "pin" | "roller" | "simple" | "guided" | "prismatic" | "cable" | "spring"
        ) {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "unknown support kind",
            ));
        }
        if !size.is_finite() || size <= 0.0 || !ground_length.is_finite() || ground_length <= 0.0 {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "size and ground_length must be finite and positive",
            ));
        }
        let direction = direction
            .map(|value| value.0.to_vector())
            .unwrap_or(gaanim_core::glam::DVec3::Y);
        if direction.length_squared() <= 1e-12 {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "direction cannot be zero",
            ));
        }
        let handle = self
            .inner
            .lock()
            .expect("scene canvas poisoned")
            .support_at(
                resolve_endpoint(&point)?,
                kind,
                direction,
                size,
                ground_length,
                color.map(|value| value.0),
            );
        Py::new(py, PySupport::initializer(handle))
    }

    #[pyo3(signature = (point, *, direction=None, size=48.0, ground_length=70.0, color=None))]
    fn fixed_support<'py>(
        &self,
        py: Python<'py>,
        point: Bound<'_, PyAny>,
        direction: Option<crate::pylayout::PyDirection>,
        size: f64,
        ground_length: f64,
        color: Option<PyColor>,
    ) -> PyResult<Py<PySupport>> {
        self.support_at(py, point, "fixed", direction, size, ground_length, color)
    }

    #[pyo3(signature = (point, *, direction=None, size=48.0, ground_length=70.0, color=None))]
    fn pin_support<'py>(
        &self,
        py: Python<'py>,
        point: Bound<'_, PyAny>,
        direction: Option<crate::pylayout::PyDirection>,
        size: f64,
        ground_length: f64,
        color: Option<PyColor>,
    ) -> PyResult<Py<PySupport>> {
        self.support_at(py, point, "pin", direction, size, ground_length, color)
    }

    #[pyo3(signature = (point, *, direction=None, size=48.0, ground_length=70.0, color=None))]
    fn roller_support<'py>(
        &self,
        py: Python<'py>,
        point: Bound<'_, PyAny>,
        direction: Option<crate::pylayout::PyDirection>,
        size: f64,
        ground_length: f64,
        color: Option<PyColor>,
    ) -> PyResult<Py<PySupport>> {
        self.support_at(py, point, "roller", direction, size, ground_length, color)
    }

    #[pyo3(signature = (point, *, direction=None, size=48.0, ground_length=70.0, color=None))]
    fn guided_support<'py>(
        &self,
        py: Python<'py>,
        point: Bound<'_, PyAny>,
        direction: Option<crate::pylayout::PyDirection>,
        size: f64,
        ground_length: f64,
        color: Option<PyColor>,
    ) -> PyResult<Py<PySupport>> {
        self.support_at(py, point, "guided", direction, size, ground_length, color)
    }

    #[pyo3(signature = (point, *, kind="revolute", axis=None, size=36.0, color=None))]
    fn joint_at(
        &self,
        point: Bound<'_, PyAny>,
        kind: &str,
        axis: Option<crate::pylayout::PyDirection>,
        size: f64,
        color: Option<PyColor>,
    ) -> PyResult<PyDrawable> {
        if !matches!(kind, "revolute" | "prismatic") {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "kind must be 'revolute' or 'prismatic'",
            ));
        }
        let axis = axis
            .map(|value| value.0.to_vector())
            .unwrap_or(gaanim_core::glam::DVec3::X);
        Ok(PyDrawable(
            self.inner.lock().expect("scene canvas poisoned").joint_at(
                resolve_endpoint(&point)?,
                kind,
                axis,
                size,
                color.map(|value| value.0),
            ),
        ))
    }

    #[pyo3(signature = (radius, teeth, *, bore_radius=8.0, color=None))]
    fn gear(
        &self,
        radius: f64,
        teeth: usize,
        bore_radius: f64,
        color: Option<PyColor>,
    ) -> PyResult<PyDrawable> {
        if !radius.is_finite()
            || radius <= 0.0
            || teeth < 4
            || !bore_radius.is_finite()
            || bore_radius < 0.0
        {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "radius must be positive, teeth >= 4, and bore_radius non-negative",
            ));
        }
        Ok(PyDrawable(
            self.inner.lock().expect("scene canvas poisoned").gear(
                radius,
                teeth,
                bore_radius,
                color.map(|value| value.0),
            ),
        ))
    }

    #[pyo3(signature = (length, teeth, *, color=None))]
    fn rack(&self, length: f64, teeth: usize, color: Option<PyColor>) -> PyResult<PyDrawable> {
        if !length.is_finite() || length <= 0.0 || teeth == 0 {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "length and teeth must be positive",
            ));
        }
        Ok(PyDrawable(
            self.inner.lock().expect("scene canvas poisoned").rack(
                length,
                teeth,
                color.map(|value| value.0),
            ),
        ))
    }

    #[pyo3(signature = (samples, *, bore_radius=8.0, color=None))]
    fn cam_profile(
        &self,
        samples: Vec<(f64, f64)>,
        bore_radius: f64,
        color: Option<PyColor>,
    ) -> PyResult<PyDrawable> {
        if samples.len() < 3
            || samples
                .iter()
                .any(|(angle, radius)| !angle.is_finite() || !radius.is_finite() || *radius <= 0.0)
        {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "samples need at least three finite positive radii",
            ));
        }
        Ok(PyDrawable(
            self.inner
                .lock()
                .expect("scene canvas poisoned")
                .cam_profile(&samples, bore_radius, color.map(|value| value.0)),
        ))
    }

    #[pyo3(signature = (curve, tracker, *, tangent_length=80.0, normal_length=80.0))]
    fn contact_on_curve(
        &self,
        curve: &PyDrawable,
        tracker: Bound<'_, PyAny>,
        tangent_length: f64,
        normal_length: f64,
    ) -> PyResult<PyDrawable> {
        let (tracker, _) = reactive_scalar(tracker)?;
        Ok(PyDrawable(
            self.inner
                .lock()
                .expect("scene canvas poisoned")
                .contact_on_curve(&curve.0, &tracker, tangent_length, normal_length),
        ))
    }

    #[pyo3(signature = (center, radius, *, direction="ccw", label=None, color=None))]
    fn moment_about(
        &self,
        center: Bound<'_, PyAny>,
        radius: f64,
        direction: &str,
        label: Option<String>,
        color: Option<PyColor>,
    ) -> PyResult<PyDrawable> {
        let ccw = match direction {
            "ccw" => true,
            "cw" => false,
            _ => {
                return Err(pyo3::exceptions::PyValueError::new_err(
                    "direction must be 'cw' or 'ccw'",
                ))
            }
        };
        let handle = self
            .inner
            .lock()
            .expect("scene canvas poisoned")
            .moment_about(
                resolve_endpoint(&center)?,
                radius,
                ccw,
                label,
                color.map(|value| value.0),
            )
            .map_err(|error| pyo3::exceptions::PyValueError::new_err(error.to_string()))?;
        Ok(PyDrawable(handle))
    }

    #[pyo3(signature = (origin, x_direction, *, length=70.0, labels=None, color=None))]
    fn coordinate_frame_at(
        &self,
        origin: Bound<'_, PyAny>,
        x_direction: crate::pylayout::PyDirection,
        length: f64,
        labels: Option<(String, String)>,
        color: Option<PyColor>,
    ) -> PyResult<PyDrawable> {
        let handle = self
            .inner
            .lock()
            .expect("scene canvas poisoned")
            .coordinate_frame_at(
                resolve_endpoint(&origin)?,
                x_direction.0.to_vector(),
                length,
                labels,
                color.map(|value| value.0),
            )
            .map_err(|error| pyo3::exceptions::PyValueError::new_err(error.to_string()))?;
        Ok(PyDrawable(handle))
    }
}

/// Resolve a Python object into a CanvasEndpoint.
/// Accepts a drawable, anchor, point reference, or static 2D/3D tuple.
pub(crate) fn resolve_endpoint(obj: &Bound<'_, PyAny>) -> PyResult<CanvasEndpoint> {
    if let Ok(drawable) = obj.extract::<PyRef<PyDrawable>>() {
        Ok(CanvasEndpoint::Entity(drawable.0.id))
    } else if let Ok(anchor) = obj.extract::<PyRef<PyAnchorPoint>>() {
        Ok(CanvasEndpoint::Anchor(anchor.0))
    } else if let Ok(point) = obj.extract::<PyRef<PyPointRef>>() {
        Ok(point.0 .0.clone())
    } else if let Ok(tuple) = obj.extract::<(f64, f64, f64)>() {
        Ok(CanvasEndpoint::Static(gaanim_core::glam::DVec3::new(
            tuple.0, tuple.1, tuple.2,
        )))
    } else if let Ok(tuple) = obj.extract::<(f64, f64)>() {
        Ok(CanvasEndpoint::Static(gaanim_core::glam::DVec3::new(
            tuple.0, tuple.1, 0.0,
        )))
    } else {
        Err(pyo3::exceptions::PyTypeError::new_err(
            "Endpoint must be a Drawable, AnchorPoint, PointRef, or a 2D/3D tuple",
        ))
    }
}

/// Resolve a 3D camera endpoint. Static tuples must declare all three axes;
/// drawable, anchor, and PointRef endpoints retain their native world Z.
fn resolve_endpoint_3d(obj: &Bound<'_, PyAny>) -> PyResult<CanvasEndpoint> {
    if let Ok(drawable) = obj.extract::<PyRef<PyDrawable>>() {
        Ok(CanvasEndpoint::Entity(drawable.0.id))
    } else if let Ok(anchor) = obj.extract::<PyRef<PyAnchorPoint>>() {
        Ok(CanvasEndpoint::Anchor(anchor.0))
    } else if let Ok(point) = obj.extract::<PyRef<PyPointRef>>() {
        Ok(point.0 .0.clone())
    } else if let Ok(tuple) = obj.extract::<(f64, f64, f64)>() {
        Ok(CanvasEndpoint::Static(gaanim_core::glam::DVec3::new(
            tuple.0, tuple.1, tuple.2,
        )))
    } else {
        Err(pyo3::exceptions::PyTypeError::new_err(
            "3D Endpoint must be a Drawable, AnchorPoint, PointRef, or a (x, y, z) tuple",
        ))
    }
}

fn resolve_ray(obj: &Bound<'_, PyAny>) -> PyResult<CanvasRay> {
    if let Ok(direction) = obj.extract::<PyRef<crate::pylayout::PyDirection>>() {
        let vector = direction.0.to_vector();
        if vector.length_squared() <= 1e-12 {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "angle direction cannot be zero",
            ));
        }
        Ok(CanvasRay::Direction(vector))
    } else {
        resolve_endpoint(obj).map(CanvasRay::Endpoint)
    }
}
