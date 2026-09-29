//! Python bindings for camera views and camera insets.

use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;

use gaanim_api::canvas::{
    CameraInsetOptions, CameraInsetPlacement, CameraInsetShape, CameraViewBackground,
    CameraViewError, CameraViewFit, CameraViewHandle, CameraViewOptions, CameraViewZoom,
    DrawableHandle,
};

use crate::color::PyColor;
use crate::pydrawable::{CameraViewBackgroundArg, PyCanvasAnim, PyDrawable};
use crate::pylayout::PyAnchor;
use crate::visualization::{PyComputed, PyParameter, extract_scalar_source_for_drawable};

fn view_error(error: CameraViewError) -> PyErr {
    PyValueError::new_err(error.to_string())
}

fn parse_fit(fit: &str) -> PyResult<CameraViewFit> {
    match fit {
        "contain" => Ok(CameraViewFit::Contain),
        "cover" => Ok(CameraViewFit::Cover),
        "stretch" => Ok(CameraViewFit::Stretch),
        other => Err(PyValueError::new_err(format!(
            "fit must be 'contain', 'cover', or 'stretch', got {other:?}"
        ))),
    }
}

fn parse_background(background: CameraViewBackgroundArg) -> CameraViewBackground {
    match background {
        CameraViewBackgroundArg::Canvas => CameraViewBackground::Canvas,
        CameraViewBackgroundArg::None => CameraViewBackground::None,
        CameraViewBackgroundArg::Paint(brush) => CameraViewBackground::Brush(brush),
    }
}

/// A number, a `Parameter` the view animates, or another reactive source the
/// view follows.
fn parse_zoom(zoom: &Bound<'_, PyAny>, owner: &DrawableHandle) -> PyResult<CameraViewZoom> {
    if let Ok(parameter) = zoom.extract::<PyRef<'_, PyParameter>>() {
        return Ok(CameraViewZoom::Parameter(parameter.inner.clone()));
    }
    if let Ok(value) = zoom.extract::<f64>() {
        return Ok(CameraViewZoom::Value(value));
    }
    extract_scalar_source_for_drawable(zoom.clone(), owner).map(CameraViewZoom::Source)
}

/// Split an `(x, y)` tuple into the two coordinates `move_to` expects, and
/// pass anything else through.
fn coordinates<'py>(
    x: &Bound<'py, PyAny>,
    y: Option<&Bound<'py, PyAny>>,
) -> PyResult<(Bound<'py, PyAny>, Option<Bound<'py, PyAny>>)> {
    if y.is_none()
        && let Ok((px, py)) = x.extract::<(f64, f64)>()
    {
        let python = x.py();
        return Ok((
            px.into_pyobject(python)?.into_any(),
            Some(py.into_pyobject(python)?.into_any()),
        ));
    }
    Ok((x.clone(), y.cloned()))
}

/// Create the view behind `Drawable.camera_view`.
#[allow(clippy::too_many_arguments)]
pub(crate) fn camera_view(
    screen: &DrawableHandle,
    frame: Option<&PyDrawable>,
    center: Option<&Bound<'_, PyAny>>,
    zoom: Option<&Bound<'_, PyAny>>,
    fit: &str,
    background: CameraViewBackgroundArg,
    exclude: Vec<PyDrawable>,
    layers: Vec<String>,
) -> PyResult<PyCameraView> {
    if frame.is_some() && center.is_some() {
        return Err(PyValueError::new_err(
            "pass either a frame or a center: a frame's own center is where the camera looks",
        ));
    }
    let options = CameraViewOptions {
        fit: parse_fit(fit)?,
        background: parse_background(background),
        exclude: exclude.into_iter().map(|drawable| drawable.0).collect(),
        layers,
        zoom: zoom.map(|zoom| parse_zoom(zoom, screen)).transpose()?,
    };
    let view = screen
        .clone()
        .camera_view_with(frame.map(|frame| &frame.0), options)
        .map_err(view_error)?;
    if let Some(center) = center {
        let (x, y) = coordinates(center, None)?;
        PyDrawable(view.frame().clone()).move_to_impl(&x, y.as_ref(), None)?;
    }
    Ok(PyCameraView { inner: view })
}

fn parse_placement(at: &Bound<'_, PyAny>) -> PyResult<CameraInsetPlacement> {
    if let Ok(anchor) = at.extract::<PyRef<'_, PyAnchor>>() {
        return Ok(CameraInsetPlacement::Anchor(anchor.0));
    }
    if let Ok((x, y)) = at.extract::<(f64, f64)>()
        && x.is_finite()
        && y.is_finite()
    {
        return Ok(CameraInsetPlacement::Point(x, y));
    }
    Err(PyValueError::new_err(
        "at must be an Anchor or a finite (x, y) point",
    ))
}

fn parse_shape(shape: &str) -> PyResult<CameraInsetShape> {
    match shape {
        "rect" => Ok(CameraInsetShape::Rect),
        "rounded" => Ok(CameraInsetShape::Rounded),
        "circle" => Ok(CameraInsetShape::Circle),
        other => Err(PyValueError::new_err(format!(
            "shape must be 'rect', 'rounded', or 'circle', got {other:?}"
        ))),
    }
}

/// Create the inset behind `Camera.inset`.
#[allow(clippy::too_many_arguments)]
pub(crate) fn camera_inset(
    canvas: &mut gaanim_api::canvas::SceneModel,
    target: &Bound<'_, PyAny>,
    zoom: &Bound<'_, PyAny>,
    at: &Bound<'_, PyAny>,
    size: Option<&Bound<'_, PyAny>>,
    aspect: Option<f64>,
    shape: &str,
    follow: bool,
    connectors: bool,
    color: Option<PyColor>,
    fixed: bool,
    background: CameraViewBackgroundArg,
    exclude: Vec<PyDrawable>,
    layers: Vec<String>,
) -> PyResult<PyCameraView> {
    let target = crate::pycanvas::resolve_endpoint(target)?;
    let zoom = if let Ok(parameter) = zoom.extract::<PyRef<'_, PyParameter>>() {
        CameraViewZoom::Parameter(parameter.inner.clone())
    } else if let Ok(value) = zoom.extract::<f64>() {
        CameraViewZoom::Value(value)
    } else {
        return Err(PyValueError::new_err(
            "an inset zoom must be a number or a Parameter",
        ));
    };
    if exclude.iter().any(|drawable| !canvas.owns(&drawable.0)) {
        return Err(view_error(CameraViewError::ForeignScene));
    }
    // `size=(w, h)` fixes both sides; a number is the width.
    let (size, aspect) = match size {
        None => (None, aspect),
        Some(size) => {
            if let Ok((width, height)) = size.extract::<(f64, f64)>() {
                if aspect.is_some() {
                    return Err(PyValueError::new_err(
                        "pass either size=(width, height) or aspect=, not both",
                    ));
                }
                if !height.is_finite() || height <= 0.0 {
                    return Err(view_error(CameraViewError::InvalidSize));
                }
                (Some(width), Some(width / height))
            } else if let Ok(width) = size.extract::<f64>() {
                (Some(width), aspect)
            } else {
                return Err(pyo3::exceptions::PyTypeError::new_err(
                    "an inset size must be a number or a (width, height) tuple",
                ));
            }
        }
    };
    let options = CameraInsetOptions {
        zoom,
        placement: parse_placement(at)?,
        size,
        aspect,
        shape: parse_shape(shape)?,
        follow,
        connectors,
        color: color.map(|color| color.0),
        fixed,
        background: parse_background(background),
        exclude: exclude.into_iter().map(|drawable| drawable.0).collect(),
        layers,
    };
    canvas
        .camera_inset(target, options)
        .map(|inner| PyCameraView { inner })
        .map_err(view_error)
}

/// A second camera shown inside a screen drawable.
#[pyclass(name = "CameraView", module = "gaanim_core", skip_from_py_object)]
#[derive(Clone)]
pub struct PyCameraView {
    inner: CameraViewHandle,
}

#[pymethods]
impl PyCameraView {
    /// The drawable that shows the view.
    #[getter]
    fn screen(&self) -> PyDrawable {
        PyDrawable(self.inner.screen().clone())
    }

    /// The drawable that plays the camera.
    #[getter]
    fn frame(&self) -> PyDrawable {
        PyDrawable(self.inner.frame().clone())
    }

    /// Lines joining the frame to the screen of an inset.
    #[getter]
    fn connectors(&self) -> Vec<PyDrawable> {
        self.inner
            .connectors()
            .iter()
            .cloned()
            .map(PyDrawable)
            .collect()
    }

    /// The zoom as a reactive value for readouts and bindings.
    #[getter]
    fn zoom(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        crate::custom::ensure_authoring_allowed()?;
        if let Some(parameter) = self.inner.zoom_parameter() {
            return Ok(Py::new(
                py,
                PyParameter {
                    inner: parameter.clone(),
                },
            )?
            .into_any());
        }
        let Some(source) = self.inner.zoom_source() else {
            return Err(PyValueError::new_err(
                "this view's frame size sets its zoom; create the view with zoom= to read it",
            ));
        };
        let owner = self.inner.zoom_owner().cloned();
        Ok(Py::new(py, PyComputed::native_source(source, owner))?.into_any())
    }

    /// Pure animation proxy for the camera.
    #[getter]
    fn animate(&self) -> PyCameraViewAnimation {
        PyCameraViewAnimation {
            inner: self.inner.clone(),
        }
    }

    /// Point the camera at `(x, y)`, a drawable or an anchor point.
    #[pyo3(signature = (x, y=None))]
    fn pan_to<'py>(
        slf: PyRef<'py, Self>,
        x: &Bound<'_, PyAny>,
        y: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<PyRef<'py, Self>> {
        let (x, y) = coordinates(x, y)?;
        PyDrawable(slf.inner.frame().clone()).move_to_impl(&x, y.as_ref(), None)?;
        Ok(slf)
    }

    /// Set the zoom at the current cursor.
    fn zoom_to(slf: PyRef<'_, Self>, zoom: f64) -> PyResult<PyRef<'_, Self>> {
        crate::custom::ensure_authoring_allowed()?;
        slf.inner.zoom_to(zoom).map_err(view_error)?;
        Ok(slf)
    }

    /// Turn the camera to `radians`; the view turns the other way.
    fn rotate_to<'py>(
        slf: PyRef<'py, Self>,
        radians: &Bound<'_, PyAny>,
    ) -> PyResult<PyRef<'py, Self>> {
        PyDrawable(slf.inner.frame().clone()).rotate_to_impl(radians)?;
        Ok(slf)
    }

    /// Keep the camera centered on an endpoint from now on.
    #[pyo3(signature = (target, *, offset=(0.0, 0.0)))]
    fn follow<'py>(
        slf: PyRef<'py, Self>,
        target: &Bound<'_, PyAny>,
        offset: (f64, f64),
    ) -> PyResult<PyRef<'py, Self>> {
        crate::custom::ensure_authoring_allowed()?;
        if !offset.0.is_finite() || !offset.1.is_finite() {
            return Err(PyValueError::new_err("offset must be finite"));
        }
        let endpoint = crate::pycanvas::resolve_endpoint(target)?;
        slf.inner.follow(
            endpoint,
            gaanim_core::glam::DVec3::new(offset.0, offset.1, 0.0),
        );
        Ok(slf)
    }
}

/// Animations of a camera view, each returned as an `Anim`.
#[pyclass(
    name = "CameraViewAnimation",
    module = "gaanim_core",
    skip_from_py_object
)]
#[derive(Clone)]
pub struct PyCameraViewAnimation {
    inner: CameraViewHandle,
}

#[pymethods]
impl PyCameraViewAnimation {
    /// Pan the camera to `(x, y)`, a drawable or an anchor point.
    #[pyo3(signature = (x, y=None))]
    fn pan_to(&self, x: &Bound<'_, PyAny>, y: Option<&Bound<'_, PyAny>>) -> PyResult<PyCanvasAnim> {
        let (x, y) = coordinates(x, y)?;
        PyCanvasAnim {
            inner: self.inner.frame().animate(),
        }
        .move_to(&x, y.as_ref(), None)
    }

    /// Zoom to `zoom`.
    fn zoom_to(&self, zoom: f64) -> PyResult<PyCanvasAnim> {
        crate::custom::ensure_authoring_allowed()?;
        self.inner
            .animate_zoom_to(zoom)
            .map(|inner| PyCanvasAnim { inner })
            .map_err(view_error)
    }

    /// Turn the camera to `radians`.
    fn rotate_to(&self, radians: &Bound<'_, PyAny>) -> PyResult<PyCanvasAnim> {
        PyCanvasAnim {
            inner: self.inner.frame().animate(),
        }
        .rotate_to(radians)
    }

    /// Grow the screen out of the region the camera sees into its place.
    fn pop_out(&self) -> PyResult<PyCanvasAnim> {
        crate::custom::ensure_authoring_allowed()?;
        Ok(PyCanvasAnim {
            inner: self.inner.pop_out(),
        })
    }

    /// Shrink the screen back into the region the camera sees.
    fn pop_in(&self) -> PyResult<PyCanvasAnim> {
        crate::custom::ensure_authoring_allowed()?;
        Ok(PyCanvasAnim {
            inner: self.inner.pop_in(),
        })
    }
}
