//! Python scene facade and its visual canvas configuration.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use pyo3::prelude::*;

use gaanim_api::canvas::{
    AxesConfig, Canvas as ApiCanvas, CanvasEndpoint, CurveControl, CurveElement, ImageCrop,
    ImageFit, ImageOptions, ParagraphOptions, TextAlign,
};
use gaanim_api::export::{
    detect_best_encoder, export_canvas, AspectRatioPreset, EncodingSpeed, ExportConfig,
    QualityPreset, VideoEncoder,
};

use crate::color::PyColor;
use crate::pydrawable::{PyCanvasAnim, PyDrawable};
use crate::pylayout::{PyFlow, PyFrameLayout, PyLayoutRegion};
use crate::transition::PyTransitionType;
use crate::value_tracker::PyValueTracker;

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
        PyLayoutRegion(
            self.inner
                .lock()
                .expect("scene canvas poisoned")
                .safe_area(),
        )
    }
}

/// Top-level public facade for building a Gaanim animation.
#[pyclass(name = "Scene", module = "gaanim_core")]
pub struct PyScene {
    inner: Arc<Mutex<ApiCanvas>>,
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

    /// Defines reusable `header`, `content`, and `footer` safe areas.
    /// Use `region.place(drawable, Anchor.TOP_LEFT)` to place a drawable.
    #[pyo3(signature = (header=0.0, footer=0.0, gap=24.0))]
    fn layout(&self, header: f64, footer: f64, gap: f64) -> PyFrameLayout {
        PyFrameLayout(
            self.inner
                .lock()
                .expect("scene canvas poisoned")
                .layout(header, footer, gap),
        )
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
        Ok(PyFrameLayout(
            self.inner
                .lock()
                .expect("scene canvas poisoned")
                .layout_preset(preset),
        ))
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
        let mut elements = Vec::new();
        for command in commands.try_iter()? {
            let command = command?;
            let (kind, arguments): (String, Bound<'_, PyAny>) = command.extract()?;
            let arguments: Vec<Bound<'_, PyAny>> =
                arguments.try_iter()?.collect::<PyResult<_>>()?;
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
                        "unknown curve command {kind:?}; expected move, line, quad, cubic, close, or close_smooth (with optional _rel)"
                    )));
                }
            };
            let point = |value: &Bound<'_, PyAny>| -> PyResult<(f64, f64)> {
                value.extract().map_err(|_| {
                    pyo3::exceptions::PyTypeError::new_err("curve points must be (x, y) pairs")
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
                            "curve controls may only use the string 'auto'",
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
                        "invalid arguments for curve command {kind:?}"
                    )))
                }
            };
            elements.push(element);
        }
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
    #[pyo3(signature = (x, y, *, grid=true, ticks=true, numbers=true, axis_color=None, grid_color=None, axis_width=3.0, grid_width=1.0))]
    fn axes(
        &self,
        x: (f64, f64, f64),
        y: (f64, f64, f64),
        grid: bool,
        ticks: bool,
        numbers: bool,
        axis_color: Option<PyColor>,
        grid_color: Option<PyColor>,
        axis_width: f64,
        grid_width: f64,
    ) -> PyDrawable {
        let config = AxesConfig {
            grid,
            ticks,
            numbers,
            axis_color: axis_color
                .map(|color| color.0)
                .unwrap_or_else(|| AxesConfig::default().axis_color),
            grid_color: grid_color
                .map(|color| color.0)
                .unwrap_or_else(|| AxesConfig::default().grid_color),
            axis_width: axis_width.max(0.0),
            grid_width: grid_width.max(0.0),
        };
        PyDrawable(
            self.inner
                .lock()
                .expect("scene canvas poisoned")
                .axes(x, y, config),
        )
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

        let background = background
            .map(|color| color.0)
            .unwrap_or_else(|| gaanim_core::peniko::Color::from_rgb8(0x1B, 0x1F, 0x3B));
        let color = color
            .map(|color| color.0)
            .unwrap_or(gaanim_core::peniko::Color::WHITE);
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
        let background = background
            .map(|color| color.0)
            .unwrap_or_else(|| gaanim_core::peniko::Color::from_rgba8(0x0F, 0x17, 0x2A, 0xE8));
        let color = color
            .map(|color| color.0)
            .unwrap_or(gaanim_core::peniko::Color::WHITE);
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

        let background = background
            .map(|color| color.0)
            .unwrap_or_else(|| gaanim_core::peniko::Color::from_rgb8(0x1B, 0x1F, 0x3B));
        let color = color
            .map(|color| color.0)
            .unwrap_or(gaanim_core::peniko::Color::WHITE);
        let accent = accent
            .map(|color| color.0)
            .unwrap_or_else(|| gaanim_core::peniko::Color::from_rgb8(0xFF, 0xD7, 0x00));
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
        let bullet_color = bullet_color
            .map(|color| color.0)
            .unwrap_or_else(|| gaanim_core::peniko::Color::from_rgb8(0xFF, 0xD7, 0x00));
        let color = color
            .map(|color| color.0)
            .unwrap_or(gaanim_core::peniko::Color::WHITE);
        let mut scene = self.inner.lock().expect("scene canvas poisoned");
        let start_y = (items.len().saturating_sub(1) as f64 * gap) * 0.5;
        let bullet_x = -width * 0.5;
        let label_x = bullet_x + bullet_radius * 4.0;
        let mut members = Vec::with_capacity(items.len() * 2);
        for (index, item) in items.iter().enumerate() {
            let y = start_y - index as f64 * gap;
            members.push(scene.dot(bullet_radius).fill(bullet_color).at(bullet_x, y));
            members.push(scene.text(item).fill(color).at(label_x, y));
        }
        let refs: Vec<&gaanim_api::canvas::DrawableHandle> = members.iter().collect();
        Ok(PyDrawable(scene.group(&refs)))
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

    fn slide(&self) {
        self.inner.lock().expect("scene canvas poisoned").slide();
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

        config.video_encoder = parse_encoder(encoder)?;
        if let Some(speed) = speed {
            config.encoding_speed = parse_encoding_speed(speed)?;
        }

        export_canvas(canvas, config)
            .map_err(|error| pyo3::exceptions::PyRuntimeError::new_err(error.to_string()))
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
