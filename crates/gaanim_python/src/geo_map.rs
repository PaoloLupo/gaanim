//! Python bindings for maps (`scene.viz.map`).

use std::sync::{Arc, Mutex};

use gaanim_api::canvas::{
    DrawableHandle, GeoMapError, GeoMapHandle, GraticuleOptions, Longitudes, MapSide, MapView,
    Projection, SceneModel as ApiCanvas,
};
use gaanim_core::glam::DVec2;
use gaanim_core::kurbo::Rect;
use pyo3::exceptions::{
    PyFileNotFoundError, PyIndexError, PyKeyError, PyOSError, PyTypeError, PyValueError,
};
use pyo3::prelude::*;
use pyo3::pyclass_init::PyClassInitializer;
use pyo3::types::PyString;

use crate::pycanvas::PyVisualization;
use crate::pydrawable::PyDrawable;
use crate::pylayout::PyAnchor;

fn value_error(error: impl ToString) -> PyErr {
    PyValueError::new_err(error.to_string())
}

fn map_error(error: GeoMapError) -> PyErr {
    match error {
        GeoMapError::Read { ref source, .. } if source.kind() == std::io::ErrorKind::NotFound => {
            PyFileNotFoundError::new_err(error.to_string())
        }
        GeoMapError::Read { .. } => PyOSError::new_err(error.to_string()),
        error => value_error(error),
    }
}

/// A map: longitude and latitude laid on a rectangle of the scene.
#[pyclass(name = "GeoMap", module = "gaanim_core", frozen)]
pub struct PyGeoMap {
    inner: GeoMapHandle,
    canvas: Arc<Mutex<ApiCanvas>>,
}

/// The graticule of a map: its lines and its labels.
#[pyclass(name = "Graticule", module = "gaanim_core", extends = PyDrawable)]
pub struct PyGraticule {
    lines: DrawableHandle,
    labels: DrawableHandle,
}

/// The features of a GeoJSON document drawn on a map.
#[pyclass(name = "GeoFeatures", module = "gaanim_core", extends = PyDrawable)]
pub struct PyGeoFeatures {
    features: Vec<DrawableHandle>,
    properties: Vec<String>,
    keys: Option<Vec<String>>,
}

/// A number for both directions, or one for longitude and one for latitude.
fn degree_pair(value: &Bound<'_, PyAny>, name: &str) -> PyResult<(f64, f64)> {
    if let Ok(both) = value.extract::<f64>() {
        return Ok((both, both));
    }
    value.extract::<(f64, f64)>().map_err(|_| {
        PyTypeError::new_err(format!(
            "{name} must be a number of degrees or a (longitude, latitude) pair"
        ))
    })
}

fn sides(labels: &Bound<'_, PyAny>) -> PyResult<Vec<MapSide>> {
    let names: Vec<String> = if let Ok(text) = labels.cast::<PyString>() {
        text.to_str()?
            .split(',')
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .map(str::to_owned)
            .collect()
    } else {
        labels.extract().map_err(|_| {
            PyTypeError::new_err(
                "labels must be a side, a comma-separated str or a sequence of sides",
            )
        })?
    };
    names
        .iter()
        .map(|name| {
            MapSide::from_name(name).ok_or_else(|| {
                value_error(format!(
                    "unknown side {name:?}; use 'left', 'right', 'top' or 'bottom'"
                ))
            })
        })
        .collect()
}

/// `rings` as one ring of points or as several.
fn rings(rings: &Bound<'_, PyAny>) -> PyResult<Vec<Vec<(f64, f64)>>> {
    if let Ok(ring) = rings.extract::<Vec<(f64, f64)>>() {
        return Ok(vec![ring]);
    }
    rings.extract::<Vec<Vec<(f64, f64)>>>().map_err(|_| {
        PyTypeError::new_err(
            "rings must be a sequence of (lon, lat) points or a sequence of such rings",
        )
    })
}

#[pymethods]
impl PyGeoMap {
    /// The projection: ``"mercator"`` or ``"equirectangular"``.
    #[getter]
    fn projection(&self) -> &'static str {
        self.inner.view().projection().name()
    }

    /// The west and east edges, in degrees.
    #[getter]
    fn lon(&self) -> (f64, f64) {
        self.inner.view().longitudes()
    }

    /// The south and north edges, in degrees.
    #[getter]
    fn lat(&self) -> (f64, f64) {
        self.inner.view().latitudes()
    }

    /// Scene units per degree of longitude.
    #[getter]
    fn scale(&self) -> f64 {
        self.inner.view().scale()
    }

    /// The invisible rectangle where the map lies, which clips its drawings.
    #[getter]
    fn frame(&self) -> PyResult<PyDrawable> {
        crate::custom::ensure_authoring_allowed()?;
        Ok(PyDrawable(self.inner.frame().clone()))
    }

    /// Where `(lon, lat)` is drawn, in scene units.
    fn point(&self, lon: f64, lat: f64) -> PyResult<(f64, f64)> {
        let point = self.inner.point(lon, lat).map_err(value_error)?;
        Ok((point.x, point.y))
    }

    /// Move `drawable` to `(lon, lat)` and return it.
    #[pyo3(signature = (drawable, lon, lat, *, anchor=None, offset=None))]
    fn place<'py>(
        &self,
        drawable: &Bound<'py, PyAny>,
        lon: f64,
        lat: f64,
        anchor: Option<PyAnchor>,
        offset: Option<(f64, f64)>,
    ) -> PyResult<Bound<'py, PyAny>> {
        crate::custom::ensure_authoring_allowed()?;
        let handle = crate::pyzones::handle_of(drawable)?;
        if !self
            .canvas
            .lock()
            .expect("scene canvas poisoned")
            .owns_drawable(&handle)
        {
            return Err(value_error("place() takes a drawable of the map's Scene"));
        }
        PyDrawable(handle.clone()).require_free_position("place")?;
        let offset = offset.unwrap_or_default();
        self.inner
            .place(
                handle,
                lon,
                lat,
                anchor.map(|anchor| anchor.0),
                DVec2::new(offset.0, offset.1),
            )
            .map_err(value_error)?;
        Ok(drawable.clone())
    }

    /// Fill a polygon of `(lon, lat)` rings.
    fn polygon(&self, rings: &Bound<'_, PyAny>) -> PyResult<PyDrawable> {
        crate::custom::ensure_authoring_allowed()?;
        let rings = self::rings(rings)?;
        self.canvas
            .lock()
            .expect("scene canvas poisoned")
            .map_polygon(&self.inner, &[rings])
            .map(PyDrawable)
            .map_err(value_error)
    }

    /// Draw a line through `(lon, lat)` points.
    #[pyo3(signature = (points, *, closed=false))]
    fn polyline(&self, points: Vec<(f64, f64)>, closed: bool) -> PyResult<PyDrawable> {
        crate::custom::ensure_authoring_allowed()?;
        self.canvas
            .lock()
            .expect("scene canvas poisoned")
            .map_polyline(&self.inner, &[points], closed)
            .map(PyDrawable)
            .map_err(value_error)
    }

    /// Draw the features of a GeoJSON file or mapping.
    #[pyo3(signature = (source, *, key=None))]
    fn geojson(
        &self,
        py: Python<'_>,
        source: &Bound<'_, PyAny>,
        key: Option<String>,
    ) -> PyResult<Py<PyGeoFeatures>> {
        crate::custom::ensure_authoring_allowed()?;
        let os = py.import("os")?;
        let is_path =
            source.is_instance_of::<PyString>() || source.is_instance(&os.getattr("PathLike")?)?;
        let features = if is_path {
            let path: std::path::PathBuf = os.call_method1("fspath", (source,))?.extract()?;
            self.canvas
                .lock()
                .expect("scene canvas poisoned")
                .map_geojson_file(&self.inner, path)
        } else {
            // Shapely geometries and GeoPandas frames describe themselves
            // as GeoJSON mappings.
            let mapping = if source.hasattr("__geo_interface__")? {
                source.getattr("__geo_interface__")?
            } else {
                source.clone()
            };
            let text: String = py
                .import("json")?
                .call_method1("dumps", (mapping,))
                .map_err(|_| {
                    PyTypeError::new_err(
                        "geojson() takes a path, a GeoJSON mapping or an object with __geo_interface__",
                    )
                })?
                .extract()?;
            self.canvas
                .lock()
                .expect("scene canvas poisoned")
                .map_geojson(&self.inner, &text)
        }
        .map_err(map_error)?;
        let keys = key
            .map(|key| features.keys(&key))
            .transpose()
            .map_err(map_error)?;
        let properties = features
            .features
            .iter()
            .map(|feature| feature.properties_json())
            .collect();
        Py::new(
            py,
            PyClassInitializer::from(PyDrawable(features.group.clone())).add_subclass(
                PyGeoFeatures {
                    features: features
                        .features
                        .into_iter()
                        .map(|feature| feature.drawable)
                        .collect(),
                    properties,
                    keys,
                },
            ),
        )
    }

    /// Draw meridians and parallels, and label them.
    #[pyo3(signature = (step=None, *, every=None, labels=None, hemispheres="NSEW"))]
    fn graticule(
        &self,
        py: Python<'_>,
        step: Option<&Bound<'_, PyAny>>,
        every: Option<&Bound<'_, PyAny>>,
        labels: Option<&Bound<'_, PyAny>>,
        hemispheres: &str,
    ) -> PyResult<Py<PyGraticule>> {
        crate::custom::ensure_authoring_allowed()?;
        let step = match step {
            Some(step) => degree_pair(step, "step")?,
            None => {
                let step = self.inner.view().graticule_step();
                (step, step)
            }
        };
        let mut options = GraticuleOptions::new(step);
        if let Some(every) = every {
            options.every = degree_pair(every, "every")?;
        }
        if let Some(labels) = labels {
            options.sides = sides(labels)?;
        }
        options.hemispheres =
            GraticuleOptions::parse_hemispheres(hemispheres).map_err(value_error)?;
        let graticule = self
            .canvas
            .lock()
            .expect("scene canvas poisoned")
            .map_graticule(&self.inner, &options)
            .map_err(value_error)?;
        Py::new(
            py,
            PyClassInitializer::from(PyDrawable(graticule.group)).add_subclass(PyGraticule {
                lines: graticule.lines,
                labels: graticule.labels,
            }),
        )
    }
}

#[pymethods]
impl PyGraticule {
    /// The meridians and parallels, as one path.
    #[getter]
    fn lines(&self) -> PyResult<PyDrawable> {
        crate::custom::ensure_authoring_allowed()?;
        Ok(PyDrawable(self.lines.clone()))
    }

    /// The labels, as a group of texts.
    #[getter]
    fn labels(&self) -> PyResult<PyDrawable> {
        crate::custom::ensure_authoring_allowed()?;
        Ok(PyDrawable(self.labels.clone()))
    }
}

impl PyGeoFeatures {
    fn index(&self, key: &Bound<'_, PyAny>) -> PyResult<usize> {
        if let Ok(index) = key.extract::<isize>() {
            let count = self.features.len() as isize;
            let index = if index < 0 { count + index } else { index };
            return usize::try_from(index)
                .ok()
                .filter(|index| *index < self.features.len())
                .ok_or_else(|| PyIndexError::new_err("feature index out of range"));
        }
        let name: String = key
            .extract()
            .map_err(|_| PyTypeError::new_err("features are indexed by position or by key"))?;
        let Some(keys) = &self.keys else {
            return Err(PyKeyError::new_err(format!(
                "{name:?}: give geojson() a key= to find features by name"
            )));
        };
        keys.iter()
            .position(|known| *known == name)
            .ok_or_else(|| PyKeyError::new_err(name))
    }
}

#[pymethods]
impl PyGeoFeatures {
    fn __len__(&self) -> usize {
        self.features.len()
    }

    fn __getitem__(&self, key: &Bound<'_, PyAny>) -> PyResult<PyDrawable> {
        crate::custom::ensure_authoring_allowed()?;
        let index = self.index(key)?;
        Ok(PyDrawable(self.features[index].clone()))
    }

    fn __contains__(&self, key: &Bound<'_, PyAny>) -> bool {
        self.index(key).is_ok()
    }

    fn __iter__(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        crate::custom::ensure_authoring_allowed()?;
        let features: Vec<PyDrawable> = self.features.iter().cloned().map(PyDrawable).collect();
        Ok(pyo3::types::PyList::new(py, features)?
            .as_any()
            .try_iter()?
            .into_any()
            .unbind())
    }

    /// Each feature's key, in order, or ``None`` without ``key=``.
    #[getter]
    fn keys(&self) -> Option<Vec<String>> {
        self.keys.clone()
    }

    /// Each feature's properties, in order, as new dicts.
    #[getter]
    fn properties<'py>(&self, py: Python<'py>) -> PyResult<Vec<Bound<'py, PyAny>>> {
        let json = py.import("json")?;
        self.properties
            .iter()
            .map(|properties| json.call_method1("loads", (properties,)))
            .collect()
    }
}

#[pymethods]
impl PyVisualization {
    /// A map of the latitudes `lat` in `within` (the safe area by default).
    #[pyo3(signature = (lat, lon=None, *, center_lon=None, projection="mercator", within=None))]
    fn map(
        &self,
        lat: (f64, f64),
        lon: Option<(f64, f64)>,
        center_lon: Option<f64>,
        projection: &str,
        within: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<PyGeoMap> {
        crate::custom::ensure_authoring_allowed()?;
        let projection = Projection::from_name(projection).ok_or_else(|| {
            value_error(format!(
                "unknown projection {projection:?}; use 'mercator' or 'equirectangular'"
            ))
        })?;
        let lon = match (lon, center_lon) {
            (Some(_), Some(_)) => {
                return Err(value_error(
                    "give lon=(west, east) or center_lon=, not both",
                ));
            }
            (Some((west, east)), None) => Longitudes::Range(west, east),
            (None, center) => Longitudes::Center(center.unwrap_or(0.0)),
        };
        let region = crate::pycanvas::within_region(&self.inner, within)?;
        let region = Rect::new(region.min.x, region.min.y, region.max.x, region.max.y);
        let view = MapView::fit(projection, lat, lon, region).map_err(value_error)?;
        let inner = self
            .inner
            .lock()
            .expect("scene canvas poisoned")
            .geo_map(view);
        Ok(PyGeoMap {
            inner,
            canvas: self.inner.clone(),
        })
    }
}
