//! Ruler-and-compass constructions on `scene.geometry`: reactive points
//! (`intersect`, `circumcenter`...), whole lines (`perpendicular`,
//! `parallel`, bisectors, `tangents`) and circles (`circle_through`) that
//! follow the points they are built from. See `gaanim_core.pyi`.

use gaanim_api::canvas::{CanvasEndpoint, CanvasRadius, PointRef};
use gaanim_math::Construction;
use pyo3::{exceptions::PyTypeError, exceptions::PyValueError, prelude::*, types::PyTuple};

use crate::pycanvas::{PyGeometry, PyPointRef, resolve_endpoint};
use crate::pydrawable::PyDrawable;

/// A whole line from a construction: a `Drawable` that remembers the two
/// points it runs through, so other constructions can use it.
#[pyclass(name = "ConstructedLine", module = "gaanim_core", extends = PyDrawable)]
pub struct PyConstructedLine {
    through: CanvasEndpoint,
    toward: CanvasEndpoint,
}

/// A circle from a construction: a `Drawable` that remembers its center and
/// a point on it.
#[pyclass(name = "ConstructedCircle", module = "gaanim_core", extends = PyDrawable)]
pub struct PyConstructedCircle {
    center: CanvasEndpoint,
    on: CanvasEndpoint,
}

#[pymethods]
impl PyConstructedLine {
    /// A point the line runs through.
    #[getter]
    fn through(&self) -> PyPointRef {
        PyPointRef(PointRef(self.through.clone()))
    }

    /// A second point on the line, one unit or more from `through`.
    #[getter]
    fn toward(&self) -> PyPointRef {
        PyPointRef(PointRef(self.toward.clone()))
    }
}

#[pymethods]
impl PyConstructedCircle {
    /// The center, recomputed every frame.
    #[getter]
    fn center(&self) -> PyPointRef {
        PyPointRef(PointRef(self.center.clone()))
    }

    /// A point on the circle.
    #[getter]
    fn on(&self) -> PyPointRef {
        PyPointRef(PointRef(self.on.clone()))
    }
}

/// A line argument: a constructed line, or two endpoints it runs through.
fn line_of(value: &Bound<'_, PyAny>) -> PyResult<(CanvasEndpoint, CanvasEndpoint)> {
    if let Ok(line) = value.cast::<PyConstructedLine>() {
        let line = line.borrow();
        return Ok((line.through.clone(), line.toward.clone()));
    }
    if let Ok(pair) = value.cast::<PyTuple>()
        && pair.len() == 2
        && pair.get_item(0)?.extract::<f64>().is_err()
    {
        return Ok((
            resolve_endpoint(&pair.get_item(0)?)?,
            resolve_endpoint(&pair.get_item(1)?)?,
        ));
    }
    Err(PyTypeError::new_err(
        "a line is a line from scene.geometry (perpendicular, line_through...) or a pair of points (A, B)",
    ))
}

/// A circle argument: a constructed circle.
fn circle_of(value: &Bound<'_, PyAny>) -> Option<(CanvasEndpoint, CanvasEndpoint)> {
    value.cast::<PyConstructedCircle>().ok().map(|circle| {
        let circle = circle.borrow();
        (circle.center.clone(), circle.on.clone())
    })
}

fn which_of(which: u8) -> PyResult<u8> {
    if which > 1 {
        return Err(PyValueError::new_err("which is 0 or 1"));
    }
    Ok(which)
}

fn constructed(kind: Construction, inputs: Vec<CanvasEndpoint>) -> PyPointRef {
    PyPointRef(PointRef(CanvasEndpoint::Construction { kind, inputs }))
}

impl PyGeometry {
    fn new_line(
        &self,
        py: Python<'_>,
        through: CanvasEndpoint,
        toward: CanvasEndpoint,
    ) -> PyResult<Py<PyConstructedLine>> {
        let handle = self
            .inner
            .lock()
            .expect("scene canvas poisoned")
            .construction_line(through.clone(), toward.clone());
        Py::new(
            py,
            PyClassInitializer::from(PyDrawable(handle))
                .add_subclass(PyConstructedLine { through, toward }),
        )
    }

    fn new_circle(
        &self,
        py: Python<'_>,
        center: CanvasEndpoint,
        on: CanvasEndpoint,
    ) -> PyResult<Py<PyConstructedCircle>> {
        let handle = self
            .inner
            .lock()
            .expect("scene canvas poisoned")
            .tracking_circle(center.clone(), CanvasRadius::Through(on.clone()));
        Py::new(
            py,
            PyClassInitializer::from(PyDrawable(handle))
                .add_subclass(PyConstructedCircle { center, on }),
        )
    }
}

#[pymethods]
impl PyGeometry {
    /// Where two lines or circles meet; see `gaanim_core.pyi`.
    #[pyo3(signature = (a, b, *, which=0))]
    fn intersect(
        &self,
        a: &Bound<'_, PyAny>,
        b: &Bound<'_, PyAny>,
        which: u8,
    ) -> PyResult<PyPointRef> {
        crate::custom::ensure_authoring_allowed()?;
        let which = which_of(which)?;
        Ok(match (circle_of(a), circle_of(b)) {
            (Some((c1, on1)), Some((c2, on2))) => {
                constructed(Construction::CircleCircle { which }, vec![c1, on1, c2, on2])
            }
            (Some((center, on)), None) => {
                let (p, q) = line_of(b)?;
                constructed(Construction::LineCircle { which }, vec![p, q, center, on])
            }
            (None, Some((center, on))) => {
                let (p, q) = line_of(a)?;
                constructed(Construction::LineCircle { which }, vec![p, q, center, on])
            }
            (None, None) => {
                let (p, q) = line_of(a)?;
                let (r, s) = line_of(b)?;
                constructed(Construction::LineLine, vec![p, q, r, s])
            }
        })
    }

    /// The whole line through two points.
    fn line_through(
        &self,
        py: Python<'_>,
        a: &Bound<'_, PyAny>,
        b: &Bound<'_, PyAny>,
    ) -> PyResult<Py<PyConstructedLine>> {
        crate::custom::ensure_authoring_allowed()?;
        self.new_line(py, resolve_endpoint(a)?, resolve_endpoint(b)?)
    }

    /// The perpendicular to `line` through a point.
    #[pyo3(signature = (line, *, through))]
    fn perpendicular(
        &self,
        py: Python<'_>,
        line: &Bound<'_, PyAny>,
        through: &Bound<'_, PyAny>,
    ) -> PyResult<Py<PyConstructedLine>> {
        crate::custom::ensure_authoring_allowed()?;
        let (a, b) = line_of(line)?;
        let through = resolve_endpoint(through)?;
        let toward = constructed(Construction::Perpendicular, vec![a, b, through.clone()])
            .0
            .0;
        self.new_line(py, through, toward)
    }

    /// The parallel to `line` through a point.
    #[pyo3(signature = (line, *, through))]
    fn parallel(
        &self,
        py: Python<'_>,
        line: &Bound<'_, PyAny>,
        through: &Bound<'_, PyAny>,
    ) -> PyResult<Py<PyConstructedLine>> {
        crate::custom::ensure_authoring_allowed()?;
        let (a, b) = line_of(line)?;
        let through = resolve_endpoint(through)?;
        let toward = constructed(Construction::Parallel, vec![a, b, through.clone()])
            .0
            .0;
        self.new_line(py, through, toward)
    }

    /// The perpendicular bisector of the segment from `a` to `b`.
    fn perpendicular_bisector(
        &self,
        py: Python<'_>,
        a: &Bound<'_, PyAny>,
        b: &Bound<'_, PyAny>,
    ) -> PyResult<Py<PyConstructedLine>> {
        crate::custom::ensure_authoring_allowed()?;
        let (a, b) = (resolve_endpoint(a)?, resolve_endpoint(b)?);
        let middle = CanvasEndpoint::Between {
            from: Box::new(a.clone()),
            to: Box::new(b.clone()),
            alpha: 0.5.into(),
            offset: gaanim_core::glam::DVec3::ZERO,
        };
        let toward = constructed(Construction::Perpendicular, vec![a, b, middle.clone()])
            .0
            .0;
        self.new_line(py, middle, toward)
    }

    /// The bisector of the angle at `vertex` between `a` and `b`.
    fn angle_bisector(
        &self,
        py: Python<'_>,
        a: &Bound<'_, PyAny>,
        vertex: &Bound<'_, PyAny>,
        b: &Bound<'_, PyAny>,
    ) -> PyResult<Py<PyConstructedLine>> {
        crate::custom::ensure_authoring_allowed()?;
        let vertex = resolve_endpoint(vertex)?;
        let toward = constructed(
            Construction::AngleBisector,
            vec![resolve_endpoint(a)?, vertex.clone(), resolve_endpoint(b)?],
        )
        .0
        .0;
        self.new_line(py, vertex, toward)
    }

    /// The two tangents to `circle` from a point outside it.
    #[pyo3(signature = (circle, *, from_))]
    fn tangents(
        &self,
        py: Python<'_>,
        circle: &Bound<'_, PyAny>,
        from_: &Bound<'_, PyAny>,
    ) -> PyResult<(Py<PyConstructedLine>, Py<PyConstructedLine>)> {
        crate::custom::ensure_authoring_allowed()?;
        let (center, on) = circle_of(circle).ok_or_else(|| {
            PyTypeError::new_err(
                "tangents take a circle from scene.geometry (circle_through, circle_around)",
            )
        })?;
        let from = resolve_endpoint(from_)?;
        let line = |which| {
            let touch = constructed(
                Construction::Tangent { which },
                vec![center.clone(), on.clone(), from.clone()],
            )
            .0
            .0;
            self.new_line(py, from.clone(), touch)
        };
        Ok((line(0)?, line(1)?))
    }

    /// The circle through three points.
    fn circle_through(
        &self,
        py: Python<'_>,
        a: &Bound<'_, PyAny>,
        b: &Bound<'_, PyAny>,
        c: &Bound<'_, PyAny>,
    ) -> PyResult<Py<PyConstructedCircle>> {
        crate::custom::ensure_authoring_allowed()?;
        let a = resolve_endpoint(a)?;
        let center = constructed(
            Construction::Circumcenter,
            vec![a.clone(), resolve_endpoint(b)?, resolve_endpoint(c)?],
        )
        .0
        .0;
        self.new_circle(py, center, a)
    }

    /// The circle around `center` through a point, or of a radius.
    fn circle_around(
        &self,
        py: Python<'_>,
        center: &Bound<'_, PyAny>,
        through: &Bound<'_, PyAny>,
    ) -> PyResult<Py<PyConstructedCircle>> {
        crate::custom::ensure_authoring_allowed()?;
        let center = resolve_endpoint(center)?;
        let on = match through.extract::<f64>() {
            Ok(radius) if radius.is_finite() && radius > 0.0 => CanvasEndpoint::Offset {
                origin: Box::new(center.clone()),
                dx: radius.into(),
                dy: 0.0.into(),
            },
            Ok(_) => return Err(PyValueError::new_err("a radius must be a positive number")),
            Err(_) => resolve_endpoint(through)?,
        };
        self.new_circle(py, center, on)
    }

    /// The center of the circle through three points.
    fn circumcenter(
        &self,
        a: &Bound<'_, PyAny>,
        b: &Bound<'_, PyAny>,
        c: &Bound<'_, PyAny>,
    ) -> PyResult<PyPointRef> {
        crate::custom::ensure_authoring_allowed()?;
        triangle(Construction::Circumcenter, a, b, c)
    }

    /// The center of the circle inside a triangle that touches its sides.
    fn incenter(
        &self,
        a: &Bound<'_, PyAny>,
        b: &Bound<'_, PyAny>,
        c: &Bound<'_, PyAny>,
    ) -> PyResult<PyPointRef> {
        crate::custom::ensure_authoring_allowed()?;
        triangle(Construction::Incenter, a, b, c)
    }

    /// Where a triangle's medians meet.
    fn centroid(
        &self,
        a: &Bound<'_, PyAny>,
        b: &Bound<'_, PyAny>,
        c: &Bound<'_, PyAny>,
    ) -> PyResult<PyPointRef> {
        crate::custom::ensure_authoring_allowed()?;
        triangle(Construction::Centroid, a, b, c)
    }

    /// The foot of the perpendicular from `point` to `line`.
    fn foot(&self, point: &Bound<'_, PyAny>, line: &Bound<'_, PyAny>) -> PyResult<PyPointRef> {
        crate::custom::ensure_authoring_allowed()?;
        let (a, b) = line_of(line)?;
        Ok(constructed(
            Construction::Foot,
            vec![a, b, resolve_endpoint(point)?],
        ))
    }

    /// A dot that follows a point and hides where it has no position.
    #[pyo3(signature = (point, *, radius=0.07))]
    fn marker(&self, point: &Bound<'_, PyAny>, radius: f64) -> PyResult<PyDrawable> {
        crate::custom::ensure_authoring_allowed()?;
        if !radius.is_finite() || radius <= 0.0 {
            return Err(PyValueError::new_err("radius must be a positive number"));
        }
        let center = resolve_endpoint(point)?;
        let mut canvas = self.inner.lock().expect("scene canvas poisoned");
        let color = canvas
            .theme_color("foreground")
            .unwrap_or(gaanim_core::peniko::Color::WHITE);
        let handle = canvas
            .tracking_circle(center, CanvasRadius::Fixed(radius))
            .fill(color)
            .no_stroke();
        Ok(PyDrawable(handle))
    }
}

fn triangle(
    kind: Construction,
    a: &Bound<'_, PyAny>,
    b: &Bound<'_, PyAny>,
    c: &Bound<'_, PyAny>,
) -> PyResult<PyPointRef> {
    Ok(constructed(
        kind,
        vec![
            resolve_endpoint(a)?,
            resolve_endpoint(b)?,
            resolve_endpoint(c)?,
        ],
    ))
}
