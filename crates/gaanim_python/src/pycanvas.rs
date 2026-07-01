//! Thin PyCanvas wrapper over ganim_api Canvas.

use pyo3::prelude::*;

use gaanim_api::canvas::{Canvas, CanvasEndpoint};

use crate::color::PyColor;
use crate::pydrawable::{PyCanvasAnim, PyDrawable};
use crate::transition::PyTransitionType;

#[pyclass(name = "Canvas", module = "gaanim_core")]
pub struct PyCanvas {
    pub inner: Canvas,
}

#[pymethods]
impl PyCanvas {
    #[new]
    #[pyo3(signature = (width=1280, height=720, background=None, margin=None))]
    fn new(width: u32, height: u32, background: Option<&PyColor>, margin: Option<f64>) -> Self {
        let mut c = Canvas::new(width, height);
        if let Some(bg) = background {
            c.background = Some(bg.0);
        }
        if let Some(m) = margin {
            c.margin = gaanim_api::canvas::Margin::all(m);
        }
        Self { inner: c }
    }

    /// Set uniform margin on all four sides (affects to_edge / to_corner).
    fn set_margin(&mut self, v: f64) {
        self.inner.margin = gaanim_api::canvas::Margin::all(v);
    }
    fn circle(&mut self, r: f64) -> PyDrawable {
        PyDrawable(self.inner.circle(r))
    }
    fn rect(&mut self, w: f64, h: f64) -> PyDrawable {
        PyDrawable(self.inner.rect(w, h))
    }
    fn rounded_rect(&mut self, w: f64, h: f64, r: f64) -> PyDrawable {
        PyDrawable(self.inner.rounded_rect(w, h, r))
    }
    fn square(&mut self, s: f64) -> PyDrawable {
        PyDrawable(self.inner.square(s))
    }
    fn dot(&mut self, r: f64) -> PyDrawable {
        PyDrawable(self.inner.dot(r))
    }
    fn ellipse(&mut self, rx: f64, ry: f64) -> PyDrawable {
        PyDrawable(self.inner.ellipse(rx, ry))
    }
    fn line(&mut self, x1: f64, y1: f64, x2: f64, y2: f64) -> PyDrawable {
        PyDrawable(self.inner.line(x1, y1, x2, y2))
    }
    fn arrow(&mut self, x1: f64, y1: f64, x2: f64, y2: f64) -> PyDrawable {
        PyDrawable(self.inner.arrow(x1, y1, x2, y2))
    }
    fn text(&mut self, s: &str) -> PyDrawable {
        PyDrawable(self.inner.text(s))
    }
    fn title(&mut self, s: &str) -> PyDrawable {
        PyDrawable(self.inner.title(s))
    }
    fn subtitle(&mut self, s: &str) -> PyDrawable {
        PyDrawable(self.inner.subtitle(s))
    }
    fn equation(&mut self, s: &str) -> PyDrawable {
        PyDrawable(self.inner.equation(s))
    }

    fn group(&mut self, members: Vec<PyDrawable>) -> PyDrawable {
        let refs: Vec<&gaanim_api::canvas::DrawableHandle> = members.iter().map(|m| &m.0).collect();
        PyDrawable(self.inner.group(&refs))
    }

    #[pyo3(signature = (name, transition=None))]
    fn segment(&mut self, name: &str, transition: Option<&PyTransitionType>) -> usize {
        self.inner.segment(name, transition.map(|t| t.0.clone()))
    }

    fn link(&mut self, from: usize, to: usize, transition: &PyTransitionType) {
        self.inner.link(from, to, transition.0.clone());
    }

    fn wait(&mut self, d: f64) {
        self.inner.wait(d);
    }
    #[pyo3(signature = (anims, *, lag=None))]
    fn play(&mut self, anims: Vec<PyCanvasAnim>, lag: Option<f64>) {
        let a = anims.into_iter().map(|a| a.inner).collect();
        if let Some(lag) = lag {
            self.inner.play_with_lag(a, lag);
        } else {
            self.inner.play(a);
        }
    }
    fn slide(&mut self) {
        self.inner.slide();
    }
    fn fade_out_all(&mut self, d: f64) {
        self.inner.fade_out_all(d);
    }
    fn render(&self) -> PyResult<()> {
        if self.inner.render() {
            Ok(())
        } else {
            Err(pyo3::exceptions::PyRuntimeError::new_err(
                "Gaanim canvases can only be rendered inside the Gaanim application. \
                 Run your script with:  gaanim <script.py>",
            ))
        }
    }
    fn export(&self, path: &str, fps: Option<u32>) -> PyResult<()> {
        self.inner
            .export(path, fps, None, None)
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))
    }

    // -- Reactive objects --

    /// Create a value tracker — a reactive float signal that can be animated.
    fn value_tracker(&mut self, initial: f64) -> PyDrawable {
        PyDrawable(self.inner.value_tracker(initial))
    }

    /// Create a traced path that accumulates the trajectory of `source`.
    fn traced_path(&mut self, source: &PyDrawable) -> PyDrawable {
        PyDrawable(self.inner.traced_path(&source.0))
    }

    /// Create a tracking line — a reactive line between two endpoints.
    ///
    /// Endpoints can be:
    /// - A `Drawable` (tracks entity position)
    /// - A tuple `(x, y)` (static position)
    fn tracking_line(
        &mut self,
        from: Bound<'_, PyAny>,
        to: Bound<'_, PyAny>,
    ) -> PyResult<PyDrawable> {
        let from_ep = resolve_endpoint(&from)?;
        let to_ep = resolve_endpoint(&to)?;
        Ok(PyDrawable(self.inner.tracking_line(from_ep, to_ep)))
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
