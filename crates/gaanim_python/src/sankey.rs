//! Sankey diagrams: `scene.viz.sankey` returns a `Sankey`.
use std::sync::{Arc, Mutex};

use gaanim_api::canvas::{
    SankeyAlign, SankeyChartOptions, SankeyError, SankeyHandle, SankeyOptions, SceneModel,
};
use pyo3::{
    exceptions::{PyKeyError, PyValueError},
    prelude::*,
    pyclass_init::PyClassInitializer,
};

use crate::color::PyColor;
use crate::pycanvas::PyVisualization;
use crate::pydrawable::{PyCanvasAnim, PyDrawable};
use crate::treemap::{cell_number, cell_text, data_table};

/// A sankey diagram from `scene.viz.sankey`: a `Drawable` of named bars and
/// ribbons.
#[pyclass(name = "Sankey", module = "gaanim_core", extends = PyDrawable)]
pub struct PySankey {
    handle: SankeyHandle,
    columns: Arc<FlowColumns>,
    scene: Arc<Mutex<SceneModel>>,
}

/// Animation proxy of a `Sankey`: an `Anim` with `to`.
#[pyclass(name = "SankeyAnimation", module = "gaanim_core", extends = PyCanvasAnim)]
pub struct PySankeyAnimation {
    handle: SankeyHandle,
    columns: Arc<FlowColumns>,
    scene: Arc<Mutex<SceneModel>>,
}

/// The columns a sankey diagram reads its flows from.
struct FlowColumns {
    source: String,
    target: String,
    value: String,
}

impl FlowColumns {
    fn flows(&self, data: &Bound<'_, PyAny>) -> PyResult<Vec<(String, String, f64)>> {
        let table = data_table(data)?;
        (0..table.len())
            .map(|row| {
                Ok((
                    cell_text(&table, &self.source, row)?,
                    cell_text(&table, &self.target, row)?,
                    cell_number(&table, &self.value, row)?,
                ))
            })
            .collect()
    }
}

fn sankey_error(error: SankeyError) -> PyErr {
    PyValueError::new_err(error.to_string())
}

#[pymethods]
impl PyVisualization {
    /// A sankey diagram of `flows`: one ribbon per row, between the nodes
    /// its `source` and `target` columns name.
    #[pyo3(signature = (
        flows,
        *,
        source="source",
        target="target",
        value="value",
        within=None,
        node_width=0.24,
        node_gap=0.3,
        align="justify",
        labels=true,
        colors=None,
        flow_opacity=0.45,
    ))]
    #[allow(clippy::too_many_arguments)]
    fn sankey(
        &self,
        py: Python<'_>,
        flows: &Bound<'_, PyAny>,
        source: &str,
        target: &str,
        value: &str,
        within: Option<&Bound<'_, PyAny>>,
        node_width: f64,
        node_gap: f64,
        align: &str,
        labels: bool,
        colors: Option<Vec<PyColor>>,
        flow_opacity: f32,
    ) -> PyResult<Py<PySankey>> {
        crate::custom::ensure_authoring_allowed()?;
        let align = SankeyAlign::parse(align)
            .ok_or_else(|| PyValueError::new_err("align must be 'justify', 'left' or 'right'"))?;
        if !(flow_opacity.is_finite() && (0.0..=1.0).contains(&flow_opacity)) {
            return Err(PyValueError::new_err(
                "flow_opacity must be between 0 and 1",
            ));
        }
        let columns = Arc::new(FlowColumns {
            source: source.to_owned(),
            target: target.to_owned(),
            value: value.to_owned(),
        });
        let rows = columns.flows(flows)?;
        let region = crate::pycanvas::within_region(&self.inner, within)?;
        let region =
            gaanim_core::kurbo::Rect::new(region.min.x, region.min.y, region.max.x, region.max.y);
        let options = SankeyChartOptions {
            layout: SankeyOptions {
                node_width,
                node_gap,
                align,
                ..SankeyOptions::default()
            },
            labels,
            colors: colors.map(|colors| colors.into_iter().map(|color| color.0).collect()),
            flow_opacity,
        };
        let handle = self
            .inner
            .lock()
            .expect("scene canvas poisoned")
            .sankey(&rows, region, options)
            .map_err(sankey_error)?;
        let drawable = PyDrawable(handle.group.clone());
        Py::new(
            py,
            PyClassInitializer::from(drawable).add_subclass(PySankey {
                handle,
                columns,
                scene: self.inner.clone(),
            }),
        )
    }
}

#[pymethods]
impl PySankey {
    /// The node names, in the order they first appear in the flows.
    #[getter]
    fn nodes(&self) -> PyResult<Vec<String>> {
        crate::custom::ensure_authoring_allowed()?;
        Ok(self
            .handle
            .nodes()
            .into_iter()
            .map(|node| node.name)
            .collect())
    }

    /// The bar of the node `name`.
    fn node(&self, name: &str) -> PyResult<PyDrawable> {
        crate::custom::ensure_authoring_allowed()?;
        self.handle
            .node(name)
            .map(PyDrawable)
            .ok_or_else(|| PyKeyError::new_err(name.to_owned()))
    }

    /// The ribbon of the flow from `source` to `target`.
    fn flow(&self, source: &str, target: &str) -> PyResult<PyDrawable> {
        crate::custom::ensure_authoring_allowed()?;
        self.handle
            .flow(source, target)
            .map(PyDrawable)
            .ok_or_else(|| PyKeyError::new_err(format!("{source} → {target}")))
    }

    /// Animation proxy: `Drawable.animate` plus `to`.
    #[getter]
    fn animate(&self, py: Python<'_>) -> PyResult<Py<PySankeyAnimation>> {
        crate::custom::ensure_authoring_allowed()?;
        let base = PyCanvasAnim {
            inner: self.handle.group.animate(),
        };
        Py::new(
            py,
            PyClassInitializer::from(base).add_subclass(PySankeyAnimation {
                handle: self.handle.clone(),
                columns: self.columns.clone(),
                scene: self.scene.clone(),
            }),
        )
    }
}

#[pymethods]
impl PySankeyAnimation {
    /// Change to `flows`: parts that keep their name move, the others fade.
    fn to(&self, flows: &Bound<'_, PyAny>) -> PyResult<PyCanvasAnim> {
        crate::custom::ensure_authoring_allowed()?;
        let rows = self.columns.flows(flows)?;
        let mut scene = self.scene.lock().expect("scene canvas poisoned");
        self.handle
            .to_anim(&mut scene, &rows, None)
            .map(|inner| PyCanvasAnim { inner })
            .map_err(sankey_error)
    }
}
