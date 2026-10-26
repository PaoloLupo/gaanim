//! Graphs and trees: `scene.viz.graph` returns a `Graph`.
use std::sync::{Arc, Mutex};

use gaanim_api::canvas::{
    Composition, GraphError, GraphHandle, GraphLayout, GraphOptions, SceneModel,
};
use pyo3::{
    exceptions::{PyKeyError, PyTypeError, PyValueError},
    prelude::*,
    pyclass_init::PyClassInitializer,
};

use crate::composition::PyComposition;
use crate::pycanvas::PyVisualization;
use crate::pydrawable::{PyCanvasAnim, PyDrawable};

/// A graph from `scene.viz.graph`: a `Drawable` of named nodes and edges.
#[pyclass(name = "Graph", module = "gaanim_core", extends = PyDrawable)]
pub struct PyGraph {
    handle: GraphHandle,
    scene: Arc<Mutex<SceneModel>>,
}

/// Animation proxy of a `Graph`: an `Anim` with changes of structure.
#[pyclass(name = "GraphAnimation", module = "gaanim_core", extends = PyCanvasAnim)]
pub struct PyGraphAnimation {
    handle: GraphHandle,
    scene: Arc<Mutex<SceneModel>>,
}

fn graph_error(error: GraphError) -> PyErr {
    PyValueError::new_err(error.to_string())
}

/// A missing node or edge to remove is a missing key.
fn removal_error(error: GraphError) -> PyErr {
    match error {
        GraphError::UnknownNode(_) | GraphError::UnknownEdge(..) => {
            PyKeyError::new_err(error.to_string())
        }
        error => graph_error(error),
    }
}

fn node_names(nodes: &Bound<'_, PyAny>) -> PyResult<Vec<String>> {
    if nodes.extract::<String>().is_ok() {
        return Err(PyTypeError::new_err("nodes must be a sequence of names"));
    }
    nodes
        .extract()
        .map_err(|_| PyTypeError::new_err("nodes must be a sequence of names"))
}

fn edge_pairs(edges: Option<&Bound<'_, PyAny>>) -> PyResult<Vec<(String, String)>> {
    let Some(edges) = edges else {
        return Ok(Vec::new());
    };
    edges.extract().map_err(|_| {
        PyTypeError::new_err("edges must be a sequence of (from, to) pairs of node names")
    })
}

fn check_duration(duration: Option<f64>) -> PyResult<()> {
    if duration.is_some_and(|seconds| !(seconds.is_finite() && seconds > 0.0)) {
        return Err(PyValueError::new_err(
            "duration must be a finite positive number of seconds",
        ));
    }
    Ok(())
}

#[pymethods]
impl PyVisualization {
    /// A graph of `nodes` joined by `edges`, laid out in `within`.
    #[pyo3(signature = (
        nodes=None,
        edges=None,
        *,
        layout="layered",
        directed=true,
        seed=0,
        node_radius=0.3,
        labels=true,
        within=None,
    ))]
    #[allow(clippy::too_many_arguments)]
    fn graph(
        &self,
        py: Python<'_>,
        nodes: Option<&Bound<'_, PyAny>>,
        edges: Option<&Bound<'_, PyAny>>,
        layout: &str,
        directed: bool,
        seed: u64,
        node_radius: f64,
        labels: bool,
        within: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<Py<PyGraph>> {
        crate::custom::ensure_authoring_allowed()?;
        let layout = GraphLayout::parse(layout, seed).ok_or_else(|| {
            PyValueError::new_err("layout must be 'tree', 'layered', 'circular' or 'force'")
        })?;
        let edges = edge_pairs(edges)?;
        // Without nodes, the graph has the nodes its edges name, in order.
        let nodes = match nodes {
            Some(nodes) => node_names(nodes)?,
            None => {
                let mut names: Vec<String> = Vec::new();
                for name in edges.iter().flat_map(|(from, to)| [from, to]) {
                    if !names.contains(name) {
                        names.push(name.clone());
                    }
                }
                names
            }
        };
        let region = crate::pycanvas::within_region(&self.inner, within)?;
        let region =
            gaanim_core::kurbo::Rect::new(region.min.x, region.min.y, region.max.x, region.max.y);
        let options = GraphOptions {
            layout,
            directed,
            node_radius,
            labels,
        };
        let handle = self
            .inner
            .lock()
            .expect("scene canvas poisoned")
            .graph(&nodes, &edges, region, options)
            .map_err(graph_error)?;
        let drawable = PyDrawable(handle.group.clone());
        Py::new(
            py,
            PyClassInitializer::from(drawable).add_subclass(PyGraph {
                handle,
                scene: self.inner.clone(),
            }),
        )
    }
}

#[pymethods]
impl PyGraph {
    /// The node names, in order.
    #[getter]
    fn nodes(&self) -> PyResult<Vec<String>> {
        crate::custom::ensure_authoring_allowed()?;
        Ok(self.handle.nodes())
    }

    /// The edges, as `(from, to)` pairs, in order.
    #[getter]
    fn edges(&self) -> PyResult<Vec<(String, String)>> {
        crate::custom::ensure_authoring_allowed()?;
        Ok(self.handle.edges())
    }

    /// The node `name`: its circle and its name.
    fn node(&self, name: &str) -> PyResult<PyDrawable> {
        crate::custom::ensure_authoring_allowed()?;
        self.handle
            .node(name)
            .map(PyDrawable)
            .ok_or_else(|| PyKeyError::new_err(name.to_owned()))
    }

    fn __getitem__(&self, name: &str) -> PyResult<PyDrawable> {
        self.node(name)
    }

    /// The edge from `source` to `target` (either way without direction).
    fn edge(&self, source: &str, target: &str) -> PyResult<PyDrawable> {
        crate::custom::ensure_authoring_allowed()?;
        self.handle
            .edge(source, target)
            .map(PyDrawable)
            .ok_or_else(|| PyKeyError::new_err(format!("{source} → {target}")))
    }

    /// Animation proxy: `Drawable.animate` plus changes of structure.
    #[getter]
    fn animate(&self, py: Python<'_>) -> PyResult<Py<PyGraphAnimation>> {
        crate::custom::ensure_authoring_allowed()?;
        let base = PyCanvasAnim {
            inner: self.handle.group.animate(),
        };
        Py::new(
            py,
            PyClassInitializer::from(base).add_subclass(PyGraphAnimation {
                handle: self.handle.clone(),
                scene: self.scene.clone(),
            }),
        )
    }
}

impl PyGraphAnimation {
    fn change(
        &self,
        duration: Option<f64>,
        change: impl FnOnce(&GraphHandle, &mut SceneModel) -> PyResult<Composition>,
    ) -> PyResult<PyComposition> {
        crate::custom::ensure_authoring_allowed()?;
        check_duration(duration)?;
        let mut scene = self.scene.lock().expect("scene canvas poisoned");
        change(&self.handle, &mut scene).map(|inner| PyComposition { inner })
    }
}

#[pymethods]
impl PyGraphAnimation {
    /// Change to `nodes` and `edges`: kept nodes move, the rest fade.
    #[pyo3(signature = (nodes, edges=None, *, duration=None))]
    fn to(
        &self,
        nodes: &Bound<'_, PyAny>,
        edges: Option<&Bound<'_, PyAny>>,
        duration: Option<f64>,
    ) -> PyResult<PyComposition> {
        let nodes = node_names(nodes)?;
        let edges = edge_pairs(edges)?;
        self.change(duration, |handle, scene| {
            handle
                .to(scene, &nodes, &edges, duration)
                .map_err(graph_error)
        })
    }

    /// Add the node `name`, joined by `edges`, and lay the graph out again.
    #[pyo3(signature = (name, edges=None, *, duration=None))]
    fn add_node(
        &self,
        name: &str,
        edges: Option<&Bound<'_, PyAny>>,
        duration: Option<f64>,
    ) -> PyResult<PyComposition> {
        let edges = edge_pairs(edges)?;
        self.change(duration, |handle, scene| {
            handle
                .add_node(scene, name, &edges, duration)
                .map_err(graph_error)
        })
    }

    /// Remove the node `name` and its edges, and lay the graph out again.
    #[pyo3(signature = (name, *, duration=None))]
    fn remove_node(&self, name: &str, duration: Option<f64>) -> PyResult<PyComposition> {
        self.change(duration, |handle, scene| {
            handle
                .remove_node(scene, name, duration)
                .map_err(removal_error)
        })
    }

    /// Add an edge from `source` to `target`, and lay the graph out again.
    #[pyo3(signature = (source, target, *, duration=None))]
    fn add_edge(
        &self,
        source: &str,
        target: &str,
        duration: Option<f64>,
    ) -> PyResult<PyComposition> {
        self.change(duration, |handle, scene| {
            handle
                .add_edge(scene, source, target, duration)
                .map_err(graph_error)
        })
    }

    /// Remove the edge from `source` to `target`, and lay the graph out again.
    #[pyo3(signature = (source, target, *, duration=None))]
    fn remove_edge(
        &self,
        source: &str,
        target: &str,
        duration: Option<f64>,
    ) -> PyResult<PyComposition> {
        self.change(duration, |handle, scene| {
            handle
                .remove_edge(scene, source, target, duration)
                .map_err(removal_error)
        })
    }
}
