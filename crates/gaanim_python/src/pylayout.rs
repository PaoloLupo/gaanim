use std::sync::{Arc, Mutex, Weak};

use gaanim_api::canvas::{
    Anchor, Canvas as ApiCanvas, Direction, FrameLayout, GridLayout, GridTrack, LayoutKind,
    LayoutRegion,
};
use gaanim_core::glam::DVec3;
use pyo3::prelude::*;

use crate::pydrawable::PyDrawable;

#[derive(Clone)]
struct LayoutState {
    canvas: Arc<Mutex<ApiCanvas>>,
    kind: LayoutKind,
    gap: f64,
    members: Vec<gaanim_api::canvas::DrawableHandle>,
    root: Option<gaanim_api::canvas::DrawableHandle>,
    parents: Vec<Weak<Mutex<LayoutState>>>,
}

/// A persistent, nestable layout container. It accepts drawables and other
/// layouts; changing it recalculates its children and can animate the reflow.
#[pyclass(name = "Layout", module = "gaanim_core", skip_from_py_object)]
#[derive(Clone)]
pub struct PyLayout {
    inner: Arc<Mutex<LayoutState>>,
}

impl PyLayout {
    pub fn new(canvas: Arc<Mutex<ApiCanvas>>, kind: LayoutKind, gap: f64) -> Self {
        Self {
            inner: Arc::new(Mutex::new(LayoutState {
                canvas,
                kind,
                gap: gap.max(0.0),
                members: Vec::new(),
                root: None,
                parents: Vec::new(),
            })),
        }
    }

    fn root(&self) -> Option<gaanim_api::canvas::DrawableHandle> {
        self.inner.lock().expect("layout poisoned").root.clone()
    }

    fn reflow_inner(
        inner: &Arc<Mutex<LayoutState>>,
        duration: Option<f64>,
        entering: Option<gaanim_api::canvas::DrawableHandle>,
    ) {
        let (canvas, kind, gap, root, members, parents) = {
            let state = inner.lock().expect("layout poisoned");
            (
                state.canvas.clone(),
                state.kind,
                state.gap,
                state.root.clone(),
                state.members.clone(),
                state.parents.clone(),
            )
        };
        let Some(root) = root else { return };
        let refs: Vec<_> = members.iter().collect();
        let mut canvas = canvas.lock().expect("scene canvas poisoned");
        canvas.set_group_members(&root, &refs);
        canvas.reflow_layout(&root, &refs, kind, gap, duration, entering.as_ref());
        drop(canvas);
        for parent in parents.into_iter().filter_map(|parent| parent.upgrade()) {
            Self::reflow_inner(&parent, duration, None);
        }
    }

    fn add_member(
        &self,
        member: gaanim_api::canvas::DrawableHandle,
        at: Option<usize>,
        animate: Option<f64>,
    ) -> PyResult<PyDrawable> {
        {
            let mut state = self.inner.lock().expect("layout poisoned");
            let index = at.unwrap_or(state.members.len());
            if index > state.members.len() {
                return Err(pyo3::exceptions::PyIndexError::new_err(
                    "layout insertion index is out of bounds",
                ));
            }
            state.members.insert(index, member.clone());
            if state.root.is_none() {
                let refs: Vec<_> = state.members.iter().collect();
                let canvas = state.canvas.clone();
                let root = canvas.lock().expect("scene canvas poisoned").group(&refs);
                state.root = Some(root);
            }
        }
        Self::reflow_inner(&self.inner, animate, Some(member.clone()));
        Ok(PyDrawable(member))
    }
}

#[pymethods]
impl PyLayout {
    /// Adds a drawable or another Layout. `animate` smoothly moves the items
    /// displaced by the insertion and fades the new item in.
    #[pyo3(signature = (child, *, at=None, animate=None))]
    fn add(
        &self,
        child: &Bound<'_, PyAny>,
        at: Option<usize>,
        animate: Option<f64>,
    ) -> PyResult<PyDrawable> {
        if let Ok(drawable) = child.extract::<PyRef<'_, PyDrawable>>() {
            return self.add_member(drawable.0.clone(), at, animate);
        }
        if let Ok(layout) = child.extract::<PyRef<'_, PyLayout>>() {
            if Arc::ptr_eq(&self.inner, &layout.inner) {
                return Err(pyo3::exceptions::PyValueError::new_err(
                    "a Layout cannot contain itself",
                ));
            }
            let root = layout.root().ok_or_else(|| {
                pyo3::exceptions::PyValueError::new_err(
                    "cannot add an empty Layout; add a drawable to it first",
                )
            })?;
            layout
                .inner
                .lock()
                .expect("layout poisoned")
                .parents
                .push(Arc::downgrade(&self.inner));
            return self.add_member(root, at, animate);
        }
        Err(pyo3::exceptions::PyTypeError::new_err(
            "Layout.add expects a Drawable or Layout",
        ))
    }

    /// Recalculate this container after external changes to its children.
    #[pyo3(signature = (*, animate=None))]
    fn reflow(&self, animate: Option<f64>) {
        Self::reflow_inner(&self.inner, animate, None);
    }

    #[getter]
    fn count(&self) -> usize {
        self.inner.lock().expect("layout poisoned").members.len()
    }

    /// The backing group, for placing the completed layout with existing APIs.
    #[getter]
    fn drawable(&self) -> PyResult<PyDrawable> {
        self.root().map(PyDrawable).ok_or_else(|| {
            pyo3::exceptions::PyValueError::new_err("an empty Layout has no drawable yet")
        })
    }
}

fn parse_tracks(values: &Bound<'_, PyAny>, axis: &str) -> PyResult<Vec<GridTrack>> {
    let mut tracks = Vec::new();
    for value in values.try_iter()? {
        let value = value?;
        if let Ok(size) = value.extract::<f64>() {
            if !size.is_finite() || size < 0.0 {
                return Err(pyo3::exceptions::PyValueError::new_err(format!(
                    "{axis} fixed tracks must be finite non-negative numbers"
                )));
            }
            tracks.push(GridTrack::Fixed(size));
            continue;
        }
        let spec = value.extract::<String>().map_err(|_| {
            pyo3::exceptions::PyTypeError::new_err(format!(
                "{axis} tracks must be numbers or strings such as '1fr'"
            ))
        })?;
        let fraction = spec
            .strip_suffix("fr")
            .and_then(|weight| weight.trim().parse::<f64>().ok())
            .filter(|weight| weight.is_finite() && *weight >= 0.0)
            .ok_or_else(|| {
                pyo3::exceptions::PyValueError::new_err(format!(
                    "invalid {axis} track {spec:?}; expected a non-negative number or '<weight>fr'"
                ))
            })?;
        tracks.push(GridTrack::Fraction(fraction));
    }
    if tracks.is_empty() {
        return Err(pyo3::exceptions::PyValueError::new_err(format!(
            "{axis} tracks cannot be empty"
        )));
    }
    Ok(tracks)
}

/// Deferred sequence of drawables that becomes one arranged group on `build`.
#[pyclass(name = "Flow", module = "gaanim_core", skip_from_py_object)]
pub struct PyFlow {
    canvas: Arc<Mutex<ApiCanvas>>,
    members: Vec<gaanim_api::canvas::DrawableHandle>,
    direction: Direction,
    gap: f64,
    align: Anchor,
    built: Option<gaanim_api::canvas::DrawableHandle>,
}

impl PyFlow {
    pub fn new(
        canvas: Arc<Mutex<ApiCanvas>>,
        direction: Direction,
        gap: f64,
        align: Anchor,
    ) -> Self {
        Self {
            canvas,
            members: Vec::new(),
            direction,
            gap: gap.max(0.0),
            align,
            built: None,
        }
    }
}

#[pymethods]
impl PyFlow {
    /// Appends a drawable. A flow cannot be changed after it has been built.
    fn add(&mut self, drawable: &PyDrawable) -> PyResult<()> {
        if self.built.is_some() {
            return Err(pyo3::exceptions::PyRuntimeError::new_err(
                "a Flow cannot be changed after build(); create a new flow",
            ));
        }
        self.members.push(drawable.0.clone());
        Ok(())
    }

    #[getter]
    fn count(&self) -> usize {
        self.members.len()
    }

    /// Creates the arranged group. Repeated calls return the same drawable.
    fn build(&mut self) -> PyResult<PyDrawable> {
        if let Some(built) = &self.built {
            return Ok(PyDrawable(built.clone()));
        }
        if self.members.is_empty() {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "a Flow needs at least one drawable before build()",
            ));
        }
        let refs: Vec<_> = self.members.iter().collect();
        let group = self
            .canvas
            .lock()
            .expect("scene canvas poisoned")
            .group(&refs)
            .arrange(self.direction, self.gap, self.align);
        self.built = Some(group.clone());
        Ok(PyDrawable(group))
    }
}

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

    /// Creates a grid with fixed numeric tracks and fractional strings (`"1fr"`).
    #[pyo3(signature = (rows, columns, row_gap=0.0, column_gap=0.0))]
    fn grid_tracks(
        &self,
        rows: Bound<'_, PyAny>,
        columns: Bound<'_, PyAny>,
        row_gap: f64,
        column_gap: f64,
    ) -> PyResult<PyGridLayout> {
        Ok(PyGridLayout(self.0.grid_with_tracks(
            parse_tracks(&rows, "row")?,
            parse_tracks(&columns, "column")?,
            row_gap,
            column_gap,
        )))
    }
}

#[pyclass(
    name = "GridLayout",
    module = "gaanim_core",
    frozen,
    skip_from_py_object
)]
#[derive(Clone, Debug)]
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
