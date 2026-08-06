//! Python scene facade and its visual canvas configuration.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use pyo3::prelude::*;

use gaanim_api::canvas::{
    Anchor, AxesConfig, Canvas as ApiCanvas, CanvasEndpoint, CanvasTheme, CurveControl,
    CurveElement, ImageCrop, ImageFit, ImageOptions, ParagraphOptions, PresentationBrand, SlideId,
    SlideTemplate, TextAlign, ThemeFont,
};
use gaanim_api::export::{
    detect_best_encoder, export_canvas, export_canvas_slide, export_canvas_slides,
    AspectRatioPreset, EncodingSpeed, ExportConfig, QualityPreset, SlideExportError, VideoEncoder,
};

use crate::color::PyColor;
use crate::pydrawable::{PyCanvasAnim, PyDrawable};
use crate::pylayout::{PyFlow, PyFrameLayout, PyLayout, PyLayoutRegion};
use crate::transition::PyTransitionType;
use crate::value_tracker::PyValueTracker;

fn slide_export_error(error: SlideExportError) -> PyErr {
    match &error {
        SlideExportError::Export(_) | SlideExportError::Io(_) => {
            pyo3::exceptions::PyRuntimeError::new_err(error.to_string())
        }
        _ => pyo3::exceptions::PyValueError::new_err(error.to_string()),
    }
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
        font_files=None,
    ))]
    fn new(
        base: Option<&Bound<'_, PyAny>>,
        name: Option<String>,
        colors: Option<HashMap<String, PyColor>>,
        fonts: Option<HashMap<String, String>>,
        sizes: Option<HashMap<String, f64>>,
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
        self.inner.lock().expect("scene canvas poisoned").background = background.map(|c| c.0);
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

    /// The region available after the configured safe-area margins.
    fn safe_area(&self) -> PyLayoutRegion {
        PyLayoutRegion {
            region: self
                .inner
                .lock()
                .expect("scene canvas poisoned")
                .safe_area(),
            canvas: self.inner.clone(),
        }
    }
}

/// Top-level public facade for building a Gaanim animation.
#[pyclass(name = "Scene", module = "gaanim_core")]
pub struct PyScene {
    inner: Arc<Mutex<ApiCanvas>>,
}

/// Reusable institutional thesis identity and cover composition.
#[pyclass(name = "ThesisTemplate", module = "gaanim_core", skip_from_py_object)]
pub struct PyThesisTemplate {
    inner: Arc<Mutex<ApiCanvas>>,
    logo: Option<String>,
    institution: String,
    faculty: String,
    school: String,
}

fn resolve_tw_cen_font(font_path: Option<String>) -> PyResult<PathBuf> {
    let mut candidates = Vec::new();
    if let Some(path) = font_path {
        candidates.push(PathBuf::from(path));
    }
    if let Ok(path) = std::env::var("GAANIM_TW_CEN_FONT") {
        candidates.push(PathBuf::from(path));
    }
    if let Ok(windir) = std::env::var("WINDIR") {
        candidates.push(PathBuf::from(windir).join("Fonts").join("TCM_____.TTF"));
    }
    candidates.extend([
        PathBuf::from("assets").join("TwCenMT.ttf"),
        PathBuf::from("assets").join("TCM_____.TTF"),
    ]);

    if let Some(path) = candidates.iter().find(|path| path.is_file()) {
        return Ok(path.clone());
    }
    let searched = candidates
        .iter()
        .map(|path| path.display().to_string())
        .collect::<Vec<_>>()
        .join(", ");
    Err(pyo3::exceptions::PyFileNotFoundError::new_err(format!(
        "Tw Cen MT is required by ThesisTemplate. Pass font_path=..., set \
         GAANIM_TW_CEN_FONT, or copy the licensed font to assets/TwCenMT.ttf. \
         Searched: {searched}"
    )))
}

#[pymethods]
impl PyThesisTemplate {
    #[new]
    #[pyo3(signature = (
        scene,
        *,
        font_path=None,
        logo=None,
        background=None,
        institution="UNIVERSIDAD CATÓLICA SAN PABLO",
        faculty="FACULTAD DE ARQUITECTURA, COMPUTACIÓN E INGENIERÍAS",
        school="ESCUELA PROFESIONAL DE INGENIERÍA CIVIL",
    ))]
    fn new(
        scene: &PyScene,
        font_path: Option<String>,
        logo: Option<String>,
        background: Option<PyColor>,
        institution: &str,
        faculty: &str,
        school: &str,
    ) -> PyResult<Self> {
        let font_path = resolve_tw_cen_font(font_path)?;
        let bytes = std::fs::read(&font_path).map_err(|error| {
            pyo3::exceptions::PyIOError::new_err(format!(
                "could not read Tw Cen MT font '{}': {error}",
                font_path.display()
            ))
        })?;
        let white = gaanim_core::peniko::Color::WHITE;
        let mut theme = CanvasTheme::builtin("presentation")
            .expect("the built-in presentation theme must exist");
        theme.name = "ucsp-thesis".into();
        theme.palette.background = background
            .map(|color| color.0)
            .unwrap_or_else(|| gaanim_core::peniko::Color::from_rgb8(0x16, 0x01, 0xFC));
        theme.palette.foreground = white;
        theme.palette.muted = white;
        theme.palette.title = white;
        theme.palette.accent = white;
        theme.palette.chart = white;
        theme.palette.panel = gaanim_core::peniko::Color::from_rgb8(0x24, 0x12, 0xE8);
        theme.palette.header = theme.palette.panel;
        theme.palette.rule = white;
        theme.sync_text_colors();
        theme
            .set_fonts(&HashMap::from([("text".into(), "Tw Cen MT".into())]))
            .map_err(pyo3::exceptions::PyValueError::new_err)?;
        theme
            .set_sizes(&HashMap::from([
                ("title".into(), 72.0),
                ("subtitle".into(), 40.0),
                ("body".into(), 34.0),
                ("caption".into(), 25.0),
            ]))
            .map_err(pyo3::exceptions::PyValueError::new_err)?;
        theme.fonts.push(ThemeFont {
            family: "Tw Cen MT".into(),
            bytes: bytes.into(),
        });
        scene
            .inner
            .lock()
            .expect("scene canvas poisoned")
            .apply_theme(theme);

        Ok(Self {
            inner: scene.inner.clone(),
            logo,
            institution: institution.into(),
            faculty: faculty.into(),
            school: school.into(),
        })
    }

    /// Create the institutional cover shown in the reference design.
    #[pyo3(signature = (title, authors, date, *, name="Portada", notes=None))]
    fn cover(
        &self,
        title: &str,
        authors: &str,
        date: &str,
        name: &str,
        notes: Option<String>,
    ) -> PyResult<PySlide> {
        let template = SlideTemplate::Blank;
        let id = {
            let mut canvas = self.inner.lock().expect("scene canvas poisoned");
            canvas
                .slide(name, notes, template)
                .map_err(|error| pyo3::exceptions::PyValueError::new_err(error.to_string()))?
        };
        // Keep the semantic boundary and its composition as separate canvas
        // mutations, mirroring `scene.slide(...); scene.text(...)`.
        let mut canvas = self.inner.lock().expect("scene canvas poisoned");
        let white = gaanim_core::peniko::Color::WHITE;

        let paragraph = |width, font_size, line_spacing| ParagraphOptions {
            width,
            align: TextAlign::Center,
            line_spacing,
            font_size: Some(font_size),
            font_family: Some("Tw Cen MT".into()),
            max_lines: None,
            overflow: gaanim_api::canvas::ParagraphOverflow::Clip,
        };
        canvas
            .paragraph(title, paragraph(1580.0, 82.0, 1.02))
            .at(0.0, 20.0);
        canvas
            .paragraph(
                &format!("{}\n{}\n{}", self.institution, self.faculty, self.school),
                paragraph(1280.0, 34.0, 1.12),
            )
            .at(0.0, 315.0);
        if let Some(logo) = &self.logo {
            canvas
                .svg(logo)
                .map_err(|error| pyo3::exceptions::PyRuntimeError::new_err(error.to_string()))?
                .scaled(0.82)
                .at(0.0, 430.0);
        } else {
            canvas
                .rounded_rect(78.0, 98.0, 18.0)
                .no_fill()
                .stroke(white, 4.0)
                .at(0.0, 430.0);
            canvas.line(0.0, 394.0, 0.0, 466.0).stroke(white, 4.0);
            canvas.line(-19.0, 430.0, 19.0, 430.0).stroke(white, 4.0);
        }
        canvas
            .paragraph(authors, paragraph(1320.0, 31.0, 1.2))
            .at(0.0, -350.0);
        canvas
            .paragraph(date, paragraph(900.0, 30.0, 1.2))
            .at(0.0, -430.0);
        drop(canvas);

        Ok(PySlide {
            inner: self.inner.clone(),
            id,
            template,
        })
    }

    /// Place a consistently styled title in any semantic slide.
    fn title(&self, slide: &PySlide, text: &str) -> PyResult<PyDrawable> {
        let mut canvas = self.inner.lock().expect("scene canvas poisoned");
        let region = canvas
            .slide_region(slide.template, "title")
            .map_err(|error| pyo3::exceptions::PyValueError::new_err(error.to_string()))?;
        let title = canvas.title(text);
        Ok(PyDrawable(region.place(title, Anchor::Center)))
    }
}

/// Semantic camera controller exposed as ``scene.camera``.
#[pyclass(name = "Camera", module = "gaanim_core", skip_from_py_object)]
#[derive(Clone)]
pub struct PyCamera {
    inner: Arc<Mutex<ApiCanvas>>,
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

#[pymethods]
impl PyCamera {
    /// Pan to a world-space point.
    #[pyo3(signature = (x, y, duration=1.0))]
    fn pan_to(&self, x: f64, y: f64, duration: f64) -> PyResult<()> {
        let (x, y, duration) = (
            require_finite(x, "x")?,
            require_finite(y, "y")?,
            require_duration(duration)?,
        );
        self.inner
            .lock()
            .expect("scene canvas poisoned")
            .camera_pan_to(x, y, duration);
        Ok(())
    }

    /// Set the orthographic zoom. Values above one zoom in.
    #[pyo3(signature = (zoom, duration=1.0))]
    fn zoom_to(&self, zoom: f64, duration: f64) -> PyResult<()> {
        if !zoom.is_finite() || zoom <= 0.0 {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "zoom must be finite and positive",
            ));
        }
        let duration = require_duration(duration)?;
        self.inner
            .lock()
            .expect("scene canvas poisoned")
            .camera_zoom_to(zoom, duration);
        Ok(())
    }

    /// Pan and zoom in parallel so the target fits inside the safe viewport.
    #[pyo3(signature = (target, margin=40.0, duration=1.0))]
    fn frame_to(&self, target: &PyDrawable, margin: f64, duration: f64) -> PyResult<()> {
        if !margin.is_finite() || margin < 0.0 {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "margin must be finite and non-negative",
            ));
        }
        let duration = require_duration(duration)?;
        self.inner
            .lock()
            .expect("scene canvas poisoned")
            .camera_frame_to(&target.0, margin, duration);
        Ok(())
    }

    /// Rotate the camera around the viewport center, in radians.
    #[pyo3(signature = (angle, duration=1.0))]
    fn rotate_to(&self, angle: f64, duration: f64) -> PyResult<()> {
        let (angle, duration) = (require_finite(angle, "angle")?, require_duration(duration)?);
        self.inner
            .lock()
            .expect("scene canvas poisoned")
            .camera_rotate_to(angle, duration);
        Ok(())
    }

    /// Keep the camera centered on a drawable while it moves.
    #[pyo3(signature = (target, duration=1.0))]
    fn follow(&self, target: &PyDrawable, duration: f64) -> PyResult<()> {
        let duration = require_duration(duration)?;
        self.inner
            .lock()
            .expect("scene canvas poisoned")
            .camera_follow(&target.0, duration);
        Ok(())
    }

    /// Apply a deterministic shake that settles at the original position.
    #[pyo3(signature = (amplitude=12.0, frequency=8.0, duration=0.5))]
    fn shake(&self, amplitude: f64, frequency: f64, duration: f64) -> PyResult<()> {
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
        self.inner
            .lock()
            .expect("scene canvas poisoned")
            .camera_shake(amplitude, frequency, duration);
        Ok(())
    }
}

/// Handle for adding reveal pauses to one semantic presentation slide.
#[pyclass(name = "Slide", module = "gaanim_core")]
pub struct PySlide {
    inner: Arc<Mutex<ApiCanvas>>,
    id: SlideId,
    template: SlideTemplate,
}

#[pymethods]
impl PySlide {
    #[pyo3(signature = (name=None))]
    fn step(&self, name: Option<String>) -> PyResult<()> {
        self.inner
            .lock()
            .expect("scene canvas poisoned")
            .slide_step(self.id, name)
            .map_err(|error| pyo3::exceptions::PyValueError::new_err(error.to_string()))
    }

    /// Resolve a named region supplied by this slide's template.
    fn region(&self, name: &str) -> PyResult<PyLayoutRegion> {
        let region = self
            .inner
            .lock()
            .expect("scene canvas poisoned")
            .slide_region(self.template, name)
            .map_err(|error| pyo3::exceptions::PyValueError::new_err(error.to_string()))?;
        Ok(PyLayoutRegion {
            region,
            canvas: self.inner.clone(),
        })
    }
}

#[pymethods]
impl PyScene {
    #[new]
    #[pyo3(signature = (width=1280, height=720, background=None, margin=None))]
    fn new(width: u32, height: u32, background: Option<PyColor>, margin: Option<f64>) -> Self {
        let mut canvas = ApiCanvas::new(width, height);
        if let Some(background) = background {
            canvas.background = Some(background.0);
        }
        if let Some(margin) = margin {
            canvas.margin = gaanim_api::canvas::Margin::all(margin);
        }
        Self {
            inner: Arc::new(Mutex::new(canvas)),
        }
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

    /// Defines reusable `header`, `content`, and `footer` safe areas.
    /// Use `region.place(drawable, Anchor.TOP_LEFT)` to place a drawable.
    #[pyo3(signature = (header=0.0, footer=0.0, gap=24.0))]
    fn frame_layout(&self, header: f64, footer: f64, gap: f64) -> PyFrameLayout {
        PyFrameLayout {
            layout: self
                .inner
                .lock()
                .expect("scene canvas poisoned")
                .layout(header, footer, gap),
            canvas: self.inner.clone(),
        }
    }

    /// Creates the one persistent container API for presentation layouts.
    /// Layouts accept drawables and other layouts through `.add(...)`.
    #[pyo3(signature = (kind="column", *, gap=24.0, columns=2, width=None, height=None, fit="none", wrap=false, justify="center"))]
    fn layout(
        &self,
        kind: &str,
        gap: f64,
        columns: usize,
        width: Option<f64>,
        height: Option<f64>,
        fit: &str,
        wrap: bool,
        justify: &str,
    ) -> PyResult<PyLayout> {
        let kind = match kind {
            "row" => gaanim_api::canvas::LayoutKind::Row,
            "column" => gaanim_api::canvas::LayoutKind::Column,
            "grid" => gaanim_api::canvas::LayoutKind::Grid {
                columns: columns.max(1),
            },
            _ => {
                return Err(pyo3::exceptions::PyValueError::new_err(
                    "kind must be 'row', 'column', or 'grid'",
                ))
            }
        };
        for (name, value) in [("width", width), ("height", height)] {
            if let Some(value) = value {
                if !value.is_finite() || value <= 0.0 {
                    return Err(pyo3::exceptions::PyValueError::new_err(format!(
                        "{name} must be a finite positive number"
                    )));
                }
            }
        }
        let shrink_to_fit = match fit {
            "none" => false,
            "shrink" => true,
            _ => {
                return Err(pyo3::exceptions::PyValueError::new_err(
                    "fit must be 'none' or 'shrink'",
                ))
            }
        };
        if !matches!(justify, "start" | "center" | "end" | "between") {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "justify must be 'start', 'center', 'end', or 'between'",
            ));
        }
        Ok(PyLayout::new(
            self.inner.clone(),
            kind,
            gap,
            width,
            height,
            shrink_to_fit,
            None,
            wrap,
            justify,
        ))
    }

    /// Creates an editorial layout preset that scales with the safe frame.
    fn layout_preset(&self, name: &str) -> PyResult<PyFrameLayout> {
        let preset = match name {
            "lecture" => gaanim_api::canvas::LayoutPreset::Lecture,
            "comparison" => gaanim_api::canvas::LayoutPreset::Comparison,
            "vertical_short" => gaanim_api::canvas::LayoutPreset::VerticalShort,
            "minimal" => gaanim_api::canvas::LayoutPreset::Minimal,
            _ => {
                return Err(pyo3::exceptions::PyValueError::new_err(
                    "preset must be 'lecture', 'comparison', 'vertical_short', or 'minimal'",
                ))
            }
        };
        Ok(PyFrameLayout {
            layout: self
                .inner
                .lock()
                .expect("scene canvas poisoned")
                .layout_preset(preset),
            canvas: self.inner.clone(),
        })
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

    /// Starts a deferred vertical or horizontal sequence of drawables.
    #[pyo3(signature = (direction="vertical", gap=24.0, align=None))]
    fn flow(
        &self,
        direction: &str,
        gap: f64,
        align: Option<&crate::pylayout::PyAnchor>,
    ) -> PyResult<PyFlow> {
        let direction = match direction {
            "vertical" => gaanim_api::canvas::Direction::Down,
            "horizontal" => gaanim_api::canvas::Direction::Right,
            _ => {
                return Err(pyo3::exceptions::PyValueError::new_err(
                    "direction must be 'vertical' or 'horizontal'",
                ))
            }
        };
        let default_align = match direction {
            gaanim_api::canvas::Direction::Down => gaanim_api::canvas::Anchor::Left,
            _ => gaanim_api::canvas::Anchor::Bottom,
        };
        Ok(PyFlow::new(
            self.inner.clone(),
            direction,
            gap,
            align.map(|anchor| anchor.0).unwrap_or(default_align),
        ))
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

    #[pyo3(signature = (function, x, samples=160))]
    fn function_graph(
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

    #[pyo3(signature = (function, t, samples=240))]
    fn parametric_curve(
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
    #[pyo3(signature = (
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
    fn axes(
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
        let parse_range = |opt: Option<Bound<PyAny>>, default: (f64, f64, f64)| -> PyResult<(f64, f64, f64)> {
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

    #[pyo3(signature = (axes, function, x, samples=160))]
    fn plot(
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
        let avail_w = if canvas.width == 0 { 800.0 } else { canvas.width as f64 - canvas.margin.left - canvas.margin.right };
        let avail_h = if canvas.height == 0 { 480.0 } else { canvas.height as f64 - canvas.margin.top - canvas.margin.bottom };
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
            let (sx, sy) = if config.auto_fit || config.x_length.is_some() || config.y_length.is_some() {
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
    #[pyo3(signature = (axes, function, x, samples=160))]
    fn get_graph(
        &self,
        axes: &PyDrawable,
        function: Bound<'_, PyAny>,
        x: (f64, f64),
        samples: usize,
    ) -> PyResult<PyDrawable> {
        self.plot(axes, function, x, samples)
    }

    /// manim `plot_parametric_curve` — (t -> (x,y)) with t_range, respects auto_fit/x_length
    #[pyo3(signature = (axes, function, t, samples=160))]
    fn plot_parametric_curve(
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
        let avail_w = if canvas.width == 0 { 800.0 } else { canvas.width as f64 - canvas.margin.left - canvas.margin.right };
        let avail_h = if canvas.height == 0 { 480.0 } else { canvas.height as f64 - canvas.margin.top - canvas.margin.bottom };
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
            let (sx, sy) = if config.auto_fit || config.x_length.is_some() || config.y_length.is_some() {
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

    fn text(&self, s: &str) -> PyDrawable {
        PyDrawable(self.inner.lock().expect("scene canvas poisoned").text(s))
    }
    /// Multi-line vector text constrained to a width.
    #[pyo3(signature = (s, width, *, align="left", line_spacing=1.2, font_size=None, font_family=None, max_lines=None, overflow="clip"))]
    fn paragraph(
        &self,
        s: &str,
        width: f64,
        align: &str,
        line_spacing: f64,
        font_size: Option<f64>,
        font_family: Option<String>,
        max_lines: Option<usize>,
        overflow: &str,
    ) -> PyResult<PyDrawable> {
        let align = match align {
            "left" => TextAlign::Left,
            "center" => TextAlign::Center,
            "right" => TextAlign::Right,
            "justify" => TextAlign::Justify,
            _ => {
                return Err(pyo3::exceptions::PyValueError::new_err(
                    "align must be 'left', 'center', 'right', or 'justify'",
                ));
            }
        };
        if !width.is_finite() || width <= 0.0 {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "width must be a finite positive number",
            ));
        }
        if !line_spacing.is_finite() || line_spacing < 1.0 {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "line_spacing must be finite and at least 1.0",
            ));
        }
        if font_size.is_some_and(|size| !size.is_finite() || size <= 0.0) {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "font_size must be a finite positive number",
            ));
        }
        if max_lines == Some(0) {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "max_lines must be at least 1 when provided",
            ));
        }
        let overflow = match overflow {
            "visible" => gaanim_api::canvas::ParagraphOverflow::Visible,
            "clip" => gaanim_api::canvas::ParagraphOverflow::Clip,
            _ => {
                return Err(pyo3::exceptions::PyValueError::new_err(
                    "overflow must be 'visible' or 'clip'",
                ))
            }
        };
        let options = ParagraphOptions {
            width,
            align,
            line_spacing,
            font_size,
            font_family,
            max_lines,
            overflow,
        };
        Ok(PyDrawable(
            self.inner
                .lock()
                .expect("scene canvas poisoned")
                .paragraph(s, options),
        ))
    }
    fn title(&self, s: &str) -> PyDrawable {
        PyDrawable(self.inner.lock().expect("scene canvas poisoned").title(s))
    }
    fn subtitle(&self, s: &str) -> PyDrawable {
        PyDrawable(
            self.inner
                .lock()
                .expect("scene canvas poisoned")
                .subtitle(s),
        )
    }
    #[pyo3(signature = (s, *, tags=None))]
    fn equation(&self, s: &str, tags: Option<HashMap<String, String>>) -> PyResult<PyDrawable> {
        let mut equation = self
            .inner
            .lock()
            .expect("scene canvas poisoned")
            .equation(s);
        for (name, fragment) in tags.unwrap_or_default() {
            if name.trim().is_empty() || fragment.trim().is_empty() {
                return Err(pyo3::exceptions::PyValueError::new_err(
                    "tag names and fragments must not be empty",
                ));
            }
            equation = equation.define_tag(name, fragment, None);
        }
        Ok(PyDrawable(equation))
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
    /// Morph the semantic tags shared by two equations in parallel.
    #[pyo3(signature = (source, target, *, tags=None, duration=1.0))]
    fn transform_equation(
        &self,
        source: &PyDrawable,
        target: &PyDrawable,
        tags: Option<Vec<String>>,
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
            .transform_equation_tags(&source.0, &target.0, tags, duration);
        Ok(())
    }
    /// Replace an equation while expanding around one persistent semantic tag.
    #[pyo3(signature = (source, target, *, tag, duration=1.0))]
    fn expand_equation(
        &self,
        source: &PyDrawable,
        target: &PyDrawable,
        tag: String,
        duration: f64,
    ) -> PyResult<()> {
        if tag.trim().is_empty() {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "tag must not be empty",
            ));
        }
        if !duration.is_finite() || duration <= 0.0 {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "duration must be a finite positive number",
            ));
        }
        self.inner
            .lock()
            .expect("scene canvas poisoned")
            .expand_equation_tag(&source.0, &target.0, &tag, duration);
        Ok(())
    }
    /// Replace one tagged term while keeping the unchanged equation glyphs in place.
    #[pyo3(signature = (source, target, *, tag, target_tag=None, duration=1.0))]
    fn replace_term(
        &self,
        source: &PyDrawable,
        target: &PyDrawable,
        tag: String,
        target_tag: Option<String>,
        duration: f64,
    ) -> PyResult<()> {
        if tag.trim().is_empty()
            || target_tag
                .as_deref()
                .is_some_and(|target_tag| target_tag.trim().is_empty())
        {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "tag names must not be empty",
            ));
        }
        if !duration.is_finite() || duration <= 0.0 {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "duration must be a finite positive number",
            ));
        }
        let target_tag = target_tag.as_deref().unwrap_or(&tag);
        self.inner
            .lock()
            .expect("scene canvas poisoned")
            .replace_equation_term(&source.0, &target.0, &tag, target_tag, duration);
        Ok(())
    }
    /// Transition between two equation steps by moving their common glyphs.
    #[pyo3(signature = (source, target, *, duration=1.0))]
    fn step_equation(
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
            .step_equation(&source.0, &target.0, duration);
        Ok(())
    }
    /// Dim an equation except for the requested semantic tags, then pulse them.
    #[pyo3(signature = (equation, tags, *, duration=1.0, dim_opacity=0.25))]
    fn focus_equation(
        &self,
        equation: &PyDrawable,
        tags: Vec<String>,
        duration: f64,
        dim_opacity: f32,
    ) -> PyResult<()> {
        if tags.is_empty() || tags.iter().any(|tag| tag.trim().is_empty()) {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "tags must contain at least one non-empty tag",
            ));
        }
        if !duration.is_finite() || duration <= 0.0 {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "duration must be a finite positive number",
            ));
        }
        if !dim_opacity.is_finite() || !(0.0..=1.0).contains(&dim_opacity) {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "dim_opacity must be between 0 and 1",
            ));
        }
        self.inner
            .lock()
            .expect("scene canvas poisoned")
            .focus_equation(&equation.0, tags, dim_opacity, duration);
        Ok(())
    }
    #[pyo3(signature = (equation, tag, label, *, above=false, duration=0.6))]
    fn brace_label(
        &self,
        equation: &PyDrawable,
        tag: &str,
        label: String,
        above: bool,
        duration: f64,
    ) -> PyResult<()> {
        if tag.trim().is_empty()
            || label.trim().is_empty()
            || !duration.is_finite()
            || duration <= 0.0
        {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "tag, label, and positive duration are required",
            ));
        }
        self.inner
            .lock()
            .expect("scene canvas poisoned")
            .brace_label(&equation.0, tag, label, above, duration);
        Ok(())
    }
    #[pyo3(signature = (equation, tag, label, *, offset=(120.0, 80.0), duration=0.6))]
    fn annotate_tag(
        &self,
        equation: &PyDrawable,
        tag: &str,
        label: String,
        offset: (f64, f64),
        duration: f64,
    ) -> PyResult<()> {
        if tag.trim().is_empty()
            || label.trim().is_empty()
            || !duration.is_finite()
            || duration <= 0.0
        {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "tag, label, and positive duration are required",
            ));
        }
        self.inner
            .lock()
            .expect("scene canvas poisoned")
            .annotate_tag(
                &equation.0,
                tag,
                label,
                gaanim_core::glam::DVec3::new(offset.0, offset.1, 0.0),
                duration,
            );
        Ok(())
    }
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
        let mut options = ParagraphOptions::new(width - 48.0);
        options.align = TextAlign::Center;
        options.max_lines = Some(2);
        let label = scene.paragraph(&text, options).fill(color);
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
        let title = scene.title(&title).fill(color).at(0.0, title_y);
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
            members.push(scene.subtitle(&subtitle).fill(color).at(0.0, -64.0));
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
            members.push(
                scene
                    .paragraph(item, ParagraphOptions::new(text_width))
                    .fill(color)
                    .at(label_x, y),
            );
        }
        let refs: Vec<&gaanim_api::canvas::DrawableHandle> = members.iter().collect();
        Ok(PyDrawable(scene.group(&refs)))
    }

    /// Create a labeled bar chart for finite non-negative values.
    #[pyo3(signature = (values, *, labels=None, width=640.0, height=320.0, gap=20.0, color=None))]
    fn bar_chart(
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

    #[pyo3(signature = (x, y, duration=1.0))]
    fn camera_pan_to(&self, x: f64, y: f64, duration: f64) {
        self.inner
            .lock()
            .expect("scene canvas poisoned")
            .camera_pan_to(x, y, duration);
    }

    #[pyo3(signature = (zoom, duration=1.0))]
    fn camera_zoom_to(&self, zoom: f64, duration: f64) {
        self.inner
            .lock()
            .expect("scene canvas poisoned")
            .camera_zoom_to(zoom, duration);
    }

    #[pyo3(signature = (target, margin=40.0, duration=1.0))]
    fn camera_frame_to(&self, target: &PyDrawable, margin: f64, duration: f64) {
        self.inner
            .lock()
            .expect("scene canvas poisoned")
            .camera_frame_to(&target.0, margin, duration);
    }

    #[pyo3(signature = (angle, duration=1.0))]
    fn camera_rotate_to(&self, angle: f64, duration: f64) {
        self.inner
            .lock()
            .expect("scene canvas poisoned")
            .camera_rotate_to(angle, duration);
    }

    #[pyo3(signature = (target, duration=1.0))]
    fn camera_follow(&self, target: &PyDrawable, duration: f64) {
        self.inner
            .lock()
            .expect("scene canvas poisoned")
            .camera_follow(&target.0, duration);
    }

    #[pyo3(signature = (amplitude=12.0, frequency=8.0, duration=0.5))]
    fn camera_shake(&self, amplitude: f64, frequency: f64, duration: f64) {
        self.inner
            .lock()
            .expect("scene canvas poisoned")
            .camera_shake(amplitude, frequency, duration);
    }

    #[pyo3(signature = (name, transition=None))]
    fn segment(&self, name: &str, transition: Option<&PyTransitionType>) -> usize {
        self.inner
            .lock()
            .expect("scene canvas poisoned")
            .segment(name, transition.map(|t| t.0.clone()))
    }

    fn link(&self, from: usize, to: usize, transition: &PyTransitionType) {
        self.inner
            .lock()
            .expect("scene canvas poisoned")
            .link(from, to, transition.0.clone());
    }

    fn wait(&self, duration: f64) {
        self.inner
            .lock()
            .expect("scene canvas poisoned")
            .wait(duration);
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

    #[pyo3(signature = (name, *, notes=None, layout="blank"))]
    fn slide(&self, name: String, notes: Option<String>, layout: &str) -> PyResult<PySlide> {
        let template = SlideTemplate::parse(layout)
            .map_err(|error| pyo3::exceptions::PyValueError::new_err(error.to_string()))?;
        let id = self
            .inner
            .lock()
            .expect("scene canvas poisoned")
            .slide(name, notes, template)
            .map_err(|error| pyo3::exceptions::PyValueError::new_err(error.to_string()))?;
        Ok(PySlide {
            inner: self.inner.clone(),
            id,
            template,
        })
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
        slide=None,
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
        slide: Option<&str>,
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
        if slide.is_some() && (start_time.is_some() || end_time.is_some()) {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "slide cannot be combined with start_time or end_time",
            ));
        }

        config.video_encoder = parse_encoder(encoder)?;
        if let Some(speed) = speed {
            config.encoding_speed = parse_encoding_speed(speed)?;
        }

        match slide {
            Some("*") => export_canvas_slides(canvas, path, config)
                .map(|_| ())
                .map_err(slide_export_error),
            Some(slide_name) => {
                export_canvas_slide(canvas, slide_name, config).map_err(slide_export_error)
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

    fn value_tracker(&self, initial: f64) -> PyValueTracker {
        let handle = self
            .inner
            .lock()
            .expect("scene canvas poisoned")
            .value_tracker(initial);
        PyValueTracker::new(handle, initial)
    }

    /// Create a dot positioned at the normalized arc-length of a sampled curve.
    fn point_on_curve(&self, curve: &PyDrawable, tracker: &PyValueTracker) -> PyDrawable {
        PyDrawable(
            self.inner
                .lock()
                .expect("scene canvas poisoned")
                .point_on_curve(&curve.0, &tracker.inner),
        )
    }

    #[pyo3(signature = (curve, tracker, length=80.0))]
    fn tangent_on_curve(
        &self,
        curve: &PyDrawable,
        tracker: &PyValueTracker,
        length: f64,
    ) -> PyDrawable {
        PyDrawable(
            self.inner
                .lock()
                .expect("scene canvas poisoned")
                .tangent_on_curve(&curve.0, &tracker.inner, length),
        )
    }

    #[pyo3(signature = (curve, tracker, length=80.0))]
    fn normal_on_curve(
        &self,
        curve: &PyDrawable,
        tracker: &PyValueTracker,
        length: f64,
    ) -> PyDrawable {
        PyDrawable(
            self.inner
                .lock()
                .expect("scene canvas poisoned")
                .normal_on_curve(&curve.0, &tracker.inner, length),
        )
    }

    #[pyo3(signature = (curve, tracker, window=0.02))]
    fn curvature_on_curve(
        &self,
        curve: &PyDrawable,
        tracker: &PyValueTracker,
        window: f64,
    ) -> PyDrawable {
        PyDrawable(
            self.inner
                .lock()
                .expect("scene canvas poisoned")
                .curvature_on_curve(&curve.0, &tracker.inner, window),
        )
    }

    #[pyo3(signature = (tracker, cx, cy, radius, start_angle, sweep_scale=1.0, sweep_offset=0.0))]
    fn always_redraw_arc(
        &self,
        tracker: &PyValueTracker,
        cx: f64,
        cy: f64,
        radius: f64,
        start_angle: f64,
        sweep_scale: f64,
        sweep_offset: f64,
    ) -> PyDrawable {
        PyDrawable(
            self.inner
                .lock()
                .expect("scene canvas poisoned")
                .always_redraw_arc(
                    &tracker.inner,
                    cx,
                    cy,
                    radius,
                    start_angle,
                    tracker.current_value(),
                    sweep_scale,
                    sweep_offset,
                ),
        )
    }

    fn traced_path(&self, source: &PyDrawable) -> PyDrawable {
        PyDrawable(
            self.inner
                .lock()
                .expect("scene canvas poisoned")
                .traced_path(&source.0),
        )
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

    #[pyo3(signature = (from, to, coils=8, amplitude=12.0))]
    fn spring_between(
        &self,
        from: Bound<'_, PyAny>,
        to: Bound<'_, PyAny>,
        coils: usize,
        amplitude: f64,
    ) -> PyResult<PyDrawable> {
        let from = resolve_endpoint(&from)?;
        let to = resolve_endpoint(&to)?;
        Ok(PyDrawable(
            self.inner
                .lock()
                .expect("scene canvas poisoned")
                .spring_between(from, to, coils, amplitude),
        ))
    }

    fn dimension_between(
        &self,
        from: Bound<'_, PyAny>,
        to: Bound<'_, PyAny>,
        offset: f64,
    ) -> PyResult<PyDrawable> {
        let from = resolve_endpoint(&from)?;
        let to = resolve_endpoint(&to)?;
        Ok(PyDrawable(
            self.inner
                .lock()
                .expect("scene canvas poisoned")
                .dimension_between(from, to, offset),
        ))
    }
}

/// Resolve a Python object into a CanvasEndpoint.
/// Accepts a PyDrawable (entity) or a tuple (x, y) (static position).
fn resolve_endpoint(obj: &Bound<'_, PyAny>) -> PyResult<CanvasEndpoint> {
    if let Ok(drawable) = obj.extract::<PyRef<PyDrawable>>() {
        Ok(CanvasEndpoint::Entity(drawable.0.id))
    } else if let Ok(tuple) = obj.extract::<(f64, f64)>() {
        Ok(CanvasEndpoint::Static(gaanim_core::glam::DVec3::new(
            tuple.0, tuple.1, 0.0,
        )))
    } else {
        Err(pyo3::exceptions::PyTypeError::new_err(
            "Endpoint must be a Drawable or a (x, y) tuple",
        ))
    }
}
