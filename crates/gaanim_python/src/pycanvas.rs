//! Python scene facade and its visual canvas configuration.

use std::sync::{Arc, Mutex};

use pyo3::prelude::*;

use gaanim_api::canvas::{
    AxesConfig, Canvas as ApiCanvas, CanvasEndpoint, ImageCrop, ImageFit, ImageOptions,
};

use crate::color::PyColor;
use crate::pydrawable::{PyCanvasAnim, PyDrawable};
use crate::transition::PyTransitionType;
use crate::value_tracker::PyValueTracker;

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
    fn equation(&self, s: &str) -> PyDrawable {
        PyDrawable(
            self.inner
                .lock()
                .expect("scene canvas poisoned")
                .equation(s),
        )
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
    fn export(&self, path: &str, fps: Option<u32>) -> PyResult<()> {
        self.inner
            .lock()
            .expect("scene canvas poisoned")
            .clone()
            .export(path, fps, None, None)
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
