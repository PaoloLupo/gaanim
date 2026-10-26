//! Treemaps: `scene.viz.treemap` returns a `Treemap`.
use std::sync::{Arc, Mutex};

use gaanim_api::canvas::{SceneModel, TreemapError, TreemapHandle, TreemapOptions};
use gaanim_visualization::{Column, DataTable as NativeDataTable};
use pyo3::{
    exceptions::{PyKeyError, PyTypeError, PyValueError},
    prelude::*,
    pyclass_init::PyClassInitializer,
};

use crate::color::PyColor;
use crate::pycanvas::PyVisualization;
use crate::pydrawable::{PyCanvasAnim, PyDrawable};
use crate::visualization::{PyDataSource, PyDataTable, table_from_python};

/// A treemap from `scene.viz.treemap`: a `Drawable` of named cells.
#[pyclass(name = "Treemap", module = "gaanim_core", extends = PyDrawable)]
pub struct PyTreemap {
    handle: TreemapHandle,
    columns: Arc<TreemapColumns>,
    scene: Arc<Mutex<SceneModel>>,
}

/// Animation proxy of a `Treemap`: an `Anim` with `to`.
#[pyclass(name = "TreemapAnimation", module = "gaanim_core", extends = PyCanvasAnim)]
pub struct PyTreemapAnimation {
    handle: TreemapHandle,
    columns: Arc<TreemapColumns>,
    scene: Arc<Mutex<SceneModel>>,
}

/// The columns a treemap reads its rows from.
struct TreemapColumns {
    path: Vec<String>,
    value: String,
}

/// A table from a `DataTable`, a `DataSource` (its current rows), a mapping
/// of columns or a data frame.
pub(crate) fn data_table(data: &Bound<'_, PyAny>) -> PyResult<NativeDataTable> {
    if let Ok(source) = data.extract::<PyRef<'_, PyDataSource>>() {
        Ok(source.inner.snapshot())
    } else if let Ok(table) = data.extract::<PyRef<'_, PyDataTable>>() {
        Ok(table.0.clone())
    } else {
        table_from_python(data)
    }
}

/// The text of `column` at `row`: a name, or a number written out.
pub(crate) fn cell_text(table: &NativeDataTable, column: &str, row: usize) -> PyResult<String> {
    let found = match table
        .column(column)
        .map_err(|error| PyValueError::new_err(error.to_string()))?
    {
        Column::Text(values) => values[row].clone(),
        Column::Numeric(values) => values[row].map(|value| {
            if value.fract() == 0.0 && value.abs() < 1e15 {
                format!("{}", value as i64)
            } else {
                value.to_string()
            }
        }),
    };
    found.ok_or_else(|| {
        PyValueError::new_err(format!("row {row} has no value in column {column:?}"))
    })
}

/// The number in `column` at `row`; a missing value counts as 0.
pub(crate) fn cell_number(table: &NativeDataTable, column: &str, row: usize) -> PyResult<f64> {
    let values = table
        .numeric_column(column)
        .map_err(|_| PyValueError::new_err(format!("column {column:?} must hold numbers")))?;
    Ok(values[row].unwrap_or(0.0))
}

impl TreemapColumns {
    fn rows(&self, data: &Bound<'_, PyAny>) -> PyResult<Vec<(Vec<String>, f64)>> {
        let table = data_table(data)?;
        (0..table.len())
            .map(|row| {
                let path = self
                    .path
                    .iter()
                    .map(|column| cell_text(&table, column, row))
                    .collect::<PyResult<Vec<String>>>()?;
                Ok((path, cell_number(&table, &self.value, row)?))
            })
            .collect()
    }
}

fn treemap_error(error: TreemapError) -> PyErr {
    PyValueError::new_err(error.to_string())
}

#[pymethods]
impl PyVisualization {
    /// A treemap of `data`: one cell per row, nested by the `path` columns.
    #[pyo3(signature = (data, *, path, value, within=None, labels=true, gap=0.04, padding=0.06, colors=None))]
    #[allow(clippy::too_many_arguments)]
    fn treemap(
        &self,
        py: Python<'_>,
        data: &Bound<'_, PyAny>,
        path: &Bound<'_, PyAny>,
        value: String,
        within: Option<&Bound<'_, PyAny>>,
        labels: bool,
        gap: f64,
        padding: f64,
        colors: Option<Vec<PyColor>>,
    ) -> PyResult<Py<PyTreemap>> {
        crate::custom::ensure_authoring_allowed()?;
        let path: Vec<String> = match path.extract::<String>() {
            Ok(column) => vec![column],
            Err(_) => path.extract::<Vec<String>>().map_err(|_| {
                PyTypeError::new_err("path must be a column name or a sequence of them")
            })?,
        };
        if path.is_empty() {
            return Err(PyValueError::new_err("path needs at least one column"));
        }
        if !(gap.is_finite() && gap >= 0.0 && padding.is_finite() && padding >= 0.0) {
            return Err(PyValueError::new_err(
                "gap and padding must be finite and not negative",
            ));
        }
        let columns = Arc::new(TreemapColumns { path, value });
        let rows = columns.rows(data)?;
        let region = crate::pycanvas::within_region(&self.inner, within)?;
        let region =
            gaanim_core::kurbo::Rect::new(region.min.x, region.min.y, region.max.x, region.max.y);
        let options = TreemapOptions {
            gap,
            padding,
            labels,
            colors: colors.map(|colors| colors.into_iter().map(|color| color.0).collect()),
        };
        let handle = self
            .inner
            .lock()
            .expect("scene canvas poisoned")
            .treemap(&rows, region, options)
            .map_err(treemap_error)?;
        let drawable = PyDrawable(handle.group.clone());
        Py::new(
            py,
            PyClassInitializer::from(drawable).add_subclass(PyTreemap {
                handle,
                columns,
                scene: self.inner.clone(),
            }),
        )
    }
}

#[pymethods]
impl PyTreemap {
    /// The keys of the cells, parents before their children.
    #[getter]
    fn keys(&self) -> PyResult<Vec<String>> {
        crate::custom::ensure_authoring_allowed()?;
        Ok(self.handle.keys())
    }

    /// The rectangle of the cell `key`.
    fn cell(&self, key: &str) -> PyResult<PyDrawable> {
        crate::custom::ensure_authoring_allowed()?;
        self.handle
            .cell(key)
            .map(PyDrawable)
            .ok_or_else(|| PyKeyError::new_err(key.to_owned()))
    }

    fn __getitem__(&self, key: &str) -> PyResult<PyDrawable> {
        self.cell(key)
    }

    /// Animation proxy: `Drawable.animate` plus `to`.
    #[getter]
    fn animate(&self, py: Python<'_>) -> PyResult<Py<PyTreemapAnimation>> {
        crate::custom::ensure_authoring_allowed()?;
        let base = PyCanvasAnim {
            inner: self.handle.group.animate(),
        };
        Py::new(
            py,
            PyClassInitializer::from(base).add_subclass(PyTreemapAnimation {
                handle: self.handle.clone(),
                columns: self.columns.clone(),
                scene: self.scene.clone(),
            }),
        )
    }
}

#[pymethods]
impl PyTreemapAnimation {
    /// Change to `data`: cells that keep their key move, the others fade.
    fn to(&self, data: &Bound<'_, PyAny>) -> PyResult<PyCanvasAnim> {
        crate::custom::ensure_authoring_allowed()?;
        let rows = self.columns.rows(data)?;
        let mut scene = self.scene.lock().expect("scene canvas poisoned");
        self.handle
            .to_anim(&mut scene, &rows, None)
            .map(|inner| PyCanvasAnim { inner })
            .map_err(treemap_error)
    }
}
