use gaanim_api::canvas::{Anchor, Direction, FrameLayout, GridLayout, LayoutRegion};
use gaanim_core::glam::DVec3;
use pyo3::prelude::*;

use crate::pydrawable::PyDrawable;

/// A rectangular safe area produced by [`PyVideoLayout`].
#[pyclass(
    name = "LayoutRegion",
    module = "gaanim_core",
    frozen,
    skip_from_py_object
)]
#[derive(Clone, Copy, Debug)]
pub struct PyLayoutRegion(pub LayoutRegion);

#[pymethods]
impl PyLayoutRegion {
    #[getter]
    fn width(&self) -> f64 {
        self.0.width()
    }

    #[getter]
    fn height(&self) -> f64 {
        self.0.height()
    }

    /// Pins the chosen anchor of a drawable to the same anchor in this region.
    fn place(&self, drawable: &PyDrawable, anchor: &PyAnchor) -> PyDrawable {
        PyDrawable(self.0.place(drawable.0.clone(), anchor.0))
    }

    /// Coordinates for an anchor in this region; useful for custom placement.
    fn point(&self, anchor: &PyAnchor) -> (f64, f64) {
        let point = self.0.anchor_point(anchor.0);
        (point.x, point.y)
    }

    /// Returns a region inset by CSS-style top/right/bottom/left values.
    #[pyo3(signature = (value, right=None, bottom=None, left=None))]
    fn inset(
        &self,
        value: f64,
        right: Option<f64>,
        bottom: Option<f64>,
        left: Option<f64>,
    ) -> Self {
        let right = right.unwrap_or(value);
        let bottom = bottom.unwrap_or(value);
        let left = left.unwrap_or(right);
        Self(self.0.inset(value, right, bottom, left))
    }

    #[pyo3(signature = (rows=1, columns=1, row_gap=0.0, column_gap=0.0))]
    fn grid(&self, rows: usize, columns: usize, row_gap: f64, column_gap: f64) -> PyGridLayout {
        PyGridLayout(self.0.grid(rows, columns, row_gap, column_gap))
    }
}

#[pyclass(
    name = "GridLayout",
    module = "gaanim_core",
    frozen,
    skip_from_py_object
)]
#[derive(Clone, Copy, Debug)]
pub struct PyGridLayout(pub GridLayout);

#[pymethods]
impl PyGridLayout {
    #[getter]
    fn rows(&self) -> usize {
        self.0.rows
    }

    #[getter]
    fn columns(&self) -> usize {
        self.0.columns
    }

    fn cell(&self, row: usize, column: usize) -> PyResult<PyLayoutRegion> {
        self.area(row, column, 1, 1)
    }

    #[pyo3(signature = (row, column, row_span=1, column_span=1))]
    fn area(
        &self,
        row: usize,
        column: usize,
        row_span: usize,
        column_span: usize,
    ) -> PyResult<PyLayoutRegion> {
        self.0
            .area(row, column, row_span, column_span)
            .map(PyLayoutRegion)
            .ok_or_else(|| pyo3::exceptions::PyIndexError::new_err("grid area is out of bounds"))
    }
}

/// Standard regions for a title/content/footer video composition.
#[pyclass(
    name = "FrameLayout",
    module = "gaanim_core",
    frozen,
    skip_from_py_object
)]
#[derive(Clone, Copy, Debug)]
pub struct PyFrameLayout(pub FrameLayout);

#[pymethods]
impl PyFrameLayout {
    #[getter]
    fn frame(&self) -> PyLayoutRegion {
        PyLayoutRegion(self.0.frame)
    }
    #[getter]
    fn header(&self) -> PyLayoutRegion {
        PyLayoutRegion(self.0.header)
    }
    #[getter]
    fn content(&self) -> PyLayoutRegion {
        PyLayoutRegion(self.0.content)
    }
    #[getter]
    fn footer(&self) -> PyLayoutRegion {
        PyLayoutRegion(self.0.footer)
    }

    /// Convenience accessor for a column spanning the whole content area.
    #[pyo3(signature = (index, count=2, gap=24.0))]
    fn column(&self, index: usize, count: usize, gap: f64) -> PyResult<PyLayoutRegion> {
        self.0
            .content
            .grid(1, count, 0.0, gap)
            .cell(0, index)
            .map(PyLayoutRegion)
            .ok_or_else(|| {
                pyo3::exceptions::PyIndexError::new_err("column index must be smaller than count")
            })
    }
}

#[pyclass(name = "Anchor", module = "gaanim_core", frozen, from_py_object)]
#[derive(Clone, Copy, Debug)]
pub struct PyAnchor(pub Anchor);

#[pymethods]
#[allow(non_snake_case)]
impl PyAnchor {
    #[classattr]
    fn CENTER() -> Self {
        Self(Anchor::Center)
    }

    #[classattr]
    fn TOP() -> Self {
        Self(Anchor::Top)
    }

    #[classattr]
    fn BOTTOM() -> Self {
        Self(Anchor::Bottom)
    }

    #[classattr]
    fn LEFT() -> Self {
        Self(Anchor::Left)
    }

    #[classattr]
    fn RIGHT() -> Self {
        Self(Anchor::Right)
    }

    #[classattr]
    fn TOP_LEFT() -> Self {
        Self(Anchor::TopLeft)
    }

    #[classattr]
    fn TOP_RIGHT() -> Self {
        Self(Anchor::TopRight)
    }

    #[classattr]
    fn BOTTOM_LEFT() -> Self {
        Self(Anchor::BottomLeft)
    }

    #[classattr]
    fn BOTTOM_RIGHT() -> Self {
        Self(Anchor::BottomRight)
    }

    fn __repr__(&self) -> &'static str {
        match self.0 {
            Anchor::Center => "Anchor.CENTER",
            Anchor::Top => "Anchor.TOP",
            Anchor::Bottom => "Anchor.BOTTOM",
            Anchor::Left => "Anchor.LEFT",
            Anchor::Right => "Anchor.RIGHT",
            Anchor::TopLeft => "Anchor.TOP_LEFT",
            Anchor::TopRight => "Anchor.TOP_RIGHT",
            Anchor::BottomLeft => "Anchor.BOTTOM_LEFT",
            Anchor::BottomRight => "Anchor.BOTTOM_RIGHT",
        }
    }
}

#[pyclass(name = "Direction", module = "gaanim_core", frozen, from_py_object)]
#[derive(Clone, Copy, Debug)]
pub struct PyDirection(pub Direction);

#[pymethods]
#[allow(non_snake_case)]
impl PyDirection {
    #[classattr]
    fn UP() -> Self {
        Self(Direction::Up)
    }

    #[classattr]
    fn DOWN() -> Self {
        Self(Direction::Down)
    }

    #[classattr]
    fn LEFT() -> Self {
        Self(Direction::Left)
    }

    #[classattr]
    fn RIGHT() -> Self {
        Self(Direction::Right)
    }

    #[classattr]
    fn UP_LEFT() -> Self {
        Self(Direction::UpLeft)
    }

    #[classattr]
    fn UP_RIGHT() -> Self {
        Self(Direction::UpRight)
    }

    #[classattr]
    fn DOWN_LEFT() -> Self {
        Self(Direction::DownLeft)
    }

    #[classattr]
    fn DOWN_RIGHT() -> Self {
        Self(Direction::DownRight)
    }

    #[staticmethod]
    #[pyo3(signature = (x, y, z = 0.0))]
    fn custom(x: f64, y: f64, z: f64) -> Self {
        Self(Direction::Custom(DVec3::new(x, y, z)))
    }

    fn __repr__(&self) -> String {
        match self.0 {
            Direction::Up => "Direction.UP".to_string(),
            Direction::Down => "Direction.DOWN".to_string(),
            Direction::Left => "Direction.LEFT".to_string(),
            Direction::Right => "Direction.RIGHT".to_string(),
            Direction::UpLeft => "Direction.UP_LEFT".to_string(),
            Direction::UpRight => "Direction.UP_RIGHT".to_string(),
            Direction::DownLeft => "Direction.DOWN_LEFT".to_string(),
            Direction::DownRight => "Direction.DOWN_RIGHT".to_string(),
            Direction::Custom(v) => {
                format!("Direction.custom({}, {}, {})", v.x, v.y, v.z)
            }
        }
    }
}
