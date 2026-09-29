//! Zones: named regions of the frame, defined once and reused anywhere, that
//! place drawables without owning them.

use std::sync::{Arc, Mutex};

use gaanim_api::canvas::{DrawableHandle, SceneModel as ApiCanvas};
use gaanim_core::glam::{DVec2, DVec3};
use gaanim_layout::{
    Align, AutoFlow, BoxConstraints, FitMode, Insets, IntrinsicMeasure, LayoutChild, LayoutError,
    LayoutId, LayoutItemStyle, LayoutNode, LayoutNodeKind, SizeRule, Track,
};
use gaanim_math::Bounds3D;
use pyo3::prelude::*;
use pyo3::types::{PyAny, PyString, PyTuple};

use crate::pydrawable::{PyCanvasAnim, PyDrawable};
use crate::pylayout::{Units, parse_align, parse_anchor, parse_fit, parse_justify};

/// How a zones template divides its region.
#[derive(Clone)]
enum ZonesKind {
    Rows,
    Columns,
    Grid,
}

/// A reusable division of a region into named zones, like a CSS grid
/// template: `Zones.rows(...)`, `Zones.columns(...)` or `Zones.grid(...)`.
/// It holds no drawables and belongs to no scene, so one template serves
/// the whole frame, a zone of another template or the box of an object.
#[pyclass(name = "Zones", module = "gaanim_core", frozen, skip_from_py_object)]
pub struct PyZones {
    kind: ZonesKind,
    rows: Option<Py<PyAny>>,
    columns: Option<Py<PyAny>>,
    gap: Option<Py<PyAny>>,
    row_gap: Option<Py<PyAny>>,
    column_gap: Option<Py<PyAny>>,
    padding: Option<Py<PyAny>>,
    names: Vec<String>,
}

fn names_of(names: Option<Vec<String>>) -> PyResult<Vec<String>> {
    let names = names.unwrap_or_default();
    let mut seen = std::collections::BTreeSet::new();
    for name in &names {
        if name.trim().is_empty() || !seen.insert(name.as_str()) {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "zone names must be non-empty and unique",
            ));
        }
    }
    Ok(names)
}

#[pymethods]
impl PyZones {
    /// Stacked bands, top to bottom: one per track (`"64px"`, `"1fr"`,
    /// `"auto"`, `"20%"` or scene units).
    #[staticmethod]
    #[pyo3(signature = (tracks, *, names=None, gap=None, padding=None))]
    fn rows(
        tracks: Py<PyAny>,
        names: Option<Vec<String>>,
        gap: Option<Py<PyAny>>,
        padding: Option<Py<PyAny>>,
    ) -> PyResult<Self> {
        Ok(Self {
            kind: ZonesKind::Rows,
            rows: Some(tracks),
            columns: None,
            gap,
            row_gap: None,
            column_gap: None,
            padding,
            names: names_of(names)?,
        })
    }

    /// Side-by-side bands, left to right: one per track.
    #[staticmethod]
    #[pyo3(signature = (tracks, *, names=None, gap=None, padding=None))]
    fn columns(
        tracks: Py<PyAny>,
        names: Option<Vec<String>>,
        gap: Option<Py<PyAny>>,
        padding: Option<Py<PyAny>>,
    ) -> PyResult<Self> {
        Ok(Self {
            kind: ZonesKind::Columns,
            rows: None,
            columns: Some(tracks),
            gap,
            row_gap: None,
            column_gap: None,
            padding,
            names: names_of(names)?,
        })
    }

    /// Cells of a grid, row by row. `rows` and `columns` are a count of equal
    /// tracks or a list of tracks.
    #[staticmethod]
    #[pyo3(signature = (*, rows, columns, names=None, gap=None, row_gap=None, column_gap=None, padding=None))]
    #[allow(clippy::too_many_arguments)]
    fn grid(
        rows: Py<PyAny>,
        columns: Py<PyAny>,
        names: Option<Vec<String>>,
        gap: Option<Py<PyAny>>,
        row_gap: Option<Py<PyAny>>,
        column_gap: Option<Py<PyAny>>,
        padding: Option<Py<PyAny>>,
    ) -> PyResult<Self> {
        Ok(Self {
            kind: ZonesKind::Grid,
            rows: Some(rows),
            columns: Some(columns),
            gap,
            row_gap,
            column_gap,
            padding,
            names: names_of(names)?,
        })
    }

    #[getter]
    fn names(&self) -> Vec<String> {
        self.names.clone()
    }

    fn __repr__(&self) -> String {
        let kind = match self.kind {
            ZonesKind::Rows => "rows",
            ZonesKind::Columns => "columns",
            ZonesKind::Grid => "grid",
        };
        format!("Zones.{kind}(names={:?})", self.names)
    }
}

/// Layout leaves with no content: zones take their size from their tracks.
struct EmptyMeasure;

impl IntrinsicMeasure for EmptyMeasure {
    fn measure(&self, _: LayoutId, constraints: BoxConstraints) -> Result<DVec2, LayoutError> {
        Ok(constraints.constrain(DVec2::ZERO))
    }
}

impl PyZones {
    /// Divide `region` into this template's zones.
    fn resolve(&self, py: Python<'_>, region: Bounds3D, units: &Units) -> PyResult<Vec<Bounds3D>> {
        let tracks = |value: &Option<Py<PyAny>>, name: &str| -> PyResult<Vec<Track>> {
            match value {
                Some(value) => units.tracks(value.bind(py), name),
                None => Ok(vec![Track::Fraction(1.0)]),
            }
        };
        let rows = tracks(&self.rows, "rows")?;
        let columns = tracks(&self.columns, "columns")?;
        let count = match self.kind {
            ZonesKind::Rows => rows.len(),
            ZonesKind::Columns => columns.len(),
            ZonesKind::Grid => rows.len() * columns.len(),
        };
        if !self.names.is_empty() && self.names.len() != count {
            return Err(pyo3::exceptions::PyValueError::new_err(format!(
                "{} names for {count} zones",
                self.names.len()
            )));
        }
        let length = |value: &Option<Py<PyAny>>, name: &str| -> PyResult<Option<f64>> {
            value
                .as_ref()
                .map(|value| units.length(value.bind(py), name))
                .transpose()
        };
        let gap = length(&self.gap, "gap")?.unwrap_or(0.0);
        let mut style = gaanim_layout::LayoutStyle {
            width: SizeRule::Fixed(region.width().max(0.0)),
            height: SizeRule::Fixed(region.height().max(0.0)),
            gap: DVec2::new(
                length(&self.column_gap, "column_gap")?.unwrap_or(gap),
                length(&self.row_gap, "row_gap")?.unwrap_or(gap),
            )
            .max(DVec2::ZERO),
            align: Align::Stretch,
            ..gaanim_layout::LayoutStyle::default()
        };
        if let Some(padding) = &self.padding {
            style.padding = units.insets(padding.bind(py), "padding", false)?;
        }
        let children = (0..count)
            .map(|index| {
                let (row, column) = match self.kind {
                    ZonesKind::Rows => (index, 0),
                    ZonesKind::Columns => (0, index),
                    ZonesKind::Grid => (index / columns.len(), index % columns.len()),
                };
                LayoutChild {
                    node: Box::new(LayoutNode::leaf(LayoutId(index as u64 + 1))),
                    style: LayoutItemStyle {
                        row: Some(row),
                        column: Some(column),
                        align: Some(Align::Stretch),
                        ..LayoutItemStyle::default()
                    },
                }
            })
            .collect();
        let mut root = LayoutNode::container(
            LayoutId(0),
            LayoutNodeKind::Grid {
                rows,
                columns,
                auto_flow: AutoFlow::Row,
            },
            children,
        );
        root.style = style;
        let resolved = gaanim_layout::resolve_layout(&root, region, &EmptyMeasure, &[])
            .map_err(|error| pyo3::exceptions::PyValueError::new_err(error.to_string()))?;
        Ok((0..count)
            .map(|index| resolved.boxes[&LayoutId(index as u64 + 1)].bounds)
            .collect())
    }
}

/// The zones of a template applied to a region.
#[pyclass(name = "ZoneSet", module = "gaanim_core", frozen, skip_from_py_object)]
pub struct PyZoneSet {
    canvas: Arc<Mutex<ApiCanvas>>,
    zones: Vec<Bounds3D>,
    names: Vec<String>,
}

impl PyZoneSet {
    pub(crate) fn resolve(
        py: Python<'_>,
        canvas: Arc<Mutex<ApiCanvas>>,
        template: &PyZones,
        region: Bounds3D,
    ) -> PyResult<Self> {
        let units = Units::new(canvas.clone());
        let zones = template.resolve(py, region, &units)?;
        Ok(Self {
            canvas,
            zones,
            names: template.names.clone(),
        })
    }

    fn zone(&self, index: usize) -> PyZone {
        PyZone {
            canvas: self.canvas.clone(),
            bounds: self.zones[index],
        }
    }
}

#[pymethods]
impl PyZoneSet {
    fn __len__(&self) -> usize {
        self.zones.len()
    }

    /// A zone by name or index.
    fn __getitem__(&self, key: &Bound<'_, PyAny>) -> PyResult<PyZone> {
        if let Ok(name) = key.cast::<PyString>() {
            let name = name.to_str()?;
            return self
                .names
                .iter()
                .position(|candidate| candidate == name)
                .map(|index| self.zone(index))
                .ok_or_else(|| pyo3::exceptions::PyKeyError::new_err(name.to_owned()));
        }
        let index = key.extract::<isize>()?;
        let len = self.zones.len() as isize;
        let index = if index < 0 { len + index } else { index };
        usize::try_from(index)
            .ok()
            .filter(|index| *index < self.zones.len())
            .map(|index| self.zone(index))
            .ok_or_else(|| pyo3::exceptions::PyIndexError::new_err("zone index out of range"))
    }

    fn __iter__(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        let zones = (0..self.zones.len())
            .map(|index| Py::new(py, self.zone(index)))
            .collect::<PyResult<Vec<_>>>()?;
        Ok(pyo3::types::PyList::new(py, zones)?
            .as_any()
            .try_iter()?
            .into_any()
            .unbind())
    }

    #[getter]
    fn names(&self) -> Vec<String> {
        self.names.clone()
    }

    /// Place each object in the zone of the same position, in order.
    #[pyo3(signature = (*objects, anchor=None, fit=None, padding=None))]
    fn fill(
        &self,
        objects: &Bound<'_, PyTuple>,
        anchor: Option<&Bound<'_, PyAny>>,
        fit: Option<&str>,
        padding: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<()> {
        crate::custom::ensure_authoring_allowed()?;
        let objects = flatten(objects)?;
        if objects.len() > self.zones.len() {
            return Err(pyo3::exceptions::PyValueError::new_err(format!(
                "{} objects for {} zones",
                objects.len(),
                self.zones.len()
            )));
        }
        for (index, object) in objects.iter().enumerate() {
            let options = Placement::parse(&self.canvas, anchor, fit, padding, None)?;
            options.place(&handle_of(object)?, self.zones[index])?;
        }
        Ok(())
    }

    fn __repr__(&self) -> String {
        format!(
            "ZoneSet({} zones, names={:?})",
            self.zones.len(),
            self.names
        )
    }
}

/// A rectangle of the frame that places drawables without owning them.
#[pyclass(name = "Zone", module = "gaanim_core", frozen, skip_from_py_object)]
#[derive(Clone)]
pub struct PyZone {
    canvas: Arc<Mutex<ApiCanvas>>,
    pub(crate) bounds: Bounds3D,
}

impl PyZone {
    pub(crate) fn new(canvas: Arc<Mutex<ApiCanvas>>, bounds: Bounds3D) -> Self {
        Self { canvas, bounds }
    }
}

#[pymethods]
impl PyZone {
    #[getter]
    fn left(&self) -> f64 {
        self.bounds.min.x
    }
    #[getter]
    fn right(&self) -> f64 {
        self.bounds.max.x
    }
    #[getter]
    fn top(&self) -> f64 {
        self.bounds.max.y
    }
    #[getter]
    fn bottom(&self) -> f64 {
        self.bounds.min.y
    }
    #[getter]
    fn width(&self) -> f64 {
        self.bounds.width()
    }
    #[getter]
    fn height(&self) -> f64 {
        self.bounds.height()
    }
    #[getter]
    fn center(&self) -> (f64, f64) {
        (self.bounds.center().x, self.bounds.center().y)
    }

    /// The point at `anchor` (`"top_left"`, `"center"`…) of the zone.
    #[pyo3(signature = (anchor=None))]
    fn point(&self, anchor: Option<&Bound<'_, PyAny>>) -> PyResult<(f64, f64)> {
        let anchor = anchor
            .map(parse_anchor)
            .transpose()?
            .unwrap_or(gaanim_layout::Anchor::Center);
        let point = anchor_point(self.bounds, anchor);
        Ok((point.x, point.y))
    }

    /// This zone shrunk by `padding` (CSS shorthand; lengths accept `px`).
    fn inset(&self, padding: &Bound<'_, PyAny>) -> PyResult<Self> {
        let units = Units::new(self.canvas.clone());
        Ok(Self {
            canvas: self.canvas.clone(),
            bounds: inset(self.bounds, units.insets(padding, "padding", true)?),
        })
    }

    /// Divide this zone with a `Zones` template.
    fn split(&self, py: Python<'_>, template: PyRef<'_, PyZones>) -> PyResult<PyZoneSet> {
        PyZoneSet::resolve(py, self.canvas.clone(), &template, self.bounds)
    }

    /// Lay `objects` out in this zone like a row or column box, then leave
    /// them free: each is only moved (and scaled with `fit`), never owned.
    #[pyo3(signature = (*objects, direction="row", gap=None, align="center", justify="center", padding=None))]
    fn arrange(
        &self,
        objects: &Bound<'_, PyTuple>,
        direction: &str,
        gap: Option<&Bound<'_, PyAny>>,
        align: &str,
        justify: &str,
        padding: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<()> {
        crate::custom::ensure_authoring_allowed()?;
        let objects = flatten(objects)?;
        let handles = objects
            .iter()
            .map(|object| handle_of(object))
            .collect::<PyResult<Vec<_>>>()?;
        let units = Units::new(self.canvas.clone());
        let region = match padding {
            Some(padding) => inset(self.bounds, units.insets(padding, "padding", false)?),
            None => self.bounds,
        };
        let kind = match direction {
            "row" => LayoutNodeKind::Row { wrap: false },
            "column" => LayoutNodeKind::Column { wrap: false },
            "wrap" => LayoutNodeKind::Row { wrap: true },
            _ => {
                return Err(pyo3::exceptions::PyValueError::new_err(
                    "direction must be 'row', 'column' or 'wrap'",
                ));
            }
        };
        let sizes = handles
            .iter()
            .map(|handle| {
                handle
                    .bounds()
                    .map(|bounds| DVec2::new(bounds.width(), bounds.height()))
                    .map_err(|error| pyo3::exceptions::PyValueError::new_err(error.to_string()))
            })
            .collect::<PyResult<Vec<_>>>()?;
        let gap = gap
            .map(|gap| units.length(gap, "gap"))
            .transpose()?
            .unwrap_or(0.0)
            .max(0.0);
        let children = (0..handles.len())
            .map(|index| LayoutChild {
                node: Box::new(LayoutNode::leaf(LayoutId(index as u64 + 1))),
                style: LayoutItemStyle::default(),
            })
            .collect();
        let mut root = LayoutNode::container(LayoutId(0), kind, children);
        root.style = gaanim_layout::LayoutStyle {
            width: SizeRule::Fixed(region.width().max(0.0)),
            height: SizeRule::Fixed(region.height().max(0.0)),
            gap: DVec2::splat(gap),
            align: parse_align(align)?,
            justify: parse_justify(justify)?,
            ..gaanim_layout::LayoutStyle::default()
        };
        struct Sizes(Vec<DVec2>);
        impl IntrinsicMeasure for Sizes {
            fn measure(
                &self,
                id: LayoutId,
                constraints: BoxConstraints,
            ) -> Result<DVec2, LayoutError> {
                let _ = constraints;
                Ok(self.0[(id.0 - 1) as usize])
            }
        }
        let resolved = gaanim_layout::resolve_layout(&root, region, &Sizes(sizes), &[])
            .map_err(|error| pyo3::exceptions::PyValueError::new_err(error.to_string()))?;
        for (index, handle) in handles.iter().enumerate() {
            let cell = resolved.boxes[&LayoutId(index as u64 + 1)].bounds;
            PyDrawable(handle.clone())
                .move_to_point(cell.center(), gaanim_layout::Anchor::Center)?;
        }
        Ok(())
    }

    fn __repr__(&self) -> String {
        format!(
            "Zone(left={:.3}, bottom={:.3}, width={:.3}, height={:.3})",
            self.bounds.min.x,
            self.bounds.min.y,
            self.bounds.width(),
            self.bounds.height()
        )
    }
}

fn flatten<'py>(objects: &Bound<'py, PyTuple>) -> PyResult<Vec<Bound<'py, PyAny>>> {
    let mut flat = Vec::new();
    for object in objects.iter() {
        if object.is_instance_of::<pyo3::types::PyList>() || object.is_instance_of::<PyTuple>() {
            for item in object.try_iter()? {
                flat.push(item?);
            }
        } else {
            flat.push(object);
        }
    }
    Ok(flat)
}

fn handle_of(object: &Bound<'_, PyAny>) -> PyResult<DrawableHandle> {
    Ok(object
        .extract::<PyRef<'_, PyDrawable>>()
        .map_err(|_| pyo3::exceptions::PyTypeError::new_err("expected a Drawable"))?
        .0
        .clone())
}

fn anchor_point(bounds: Bounds3D, anchor: gaanim_layout::Anchor) -> DVec3 {
    let offset = anchor.to_offset();
    let center = bounds.center();
    DVec3::new(
        center.x + offset.x * bounds.width() * 0.5,
        center.y + offset.y * bounds.height() * 0.5,
        0.0,
    )
}

fn inset(bounds: Bounds3D, insets: Insets) -> Bounds3D {
    let min_x = bounds.min.x + insets.left;
    let max_x = (bounds.max.x - insets.right).max(min_x);
    let min_y = bounds.min.y + insets.bottom;
    let max_y = (bounds.max.y - insets.top).max(min_y);
    Bounds3D::new_2d(min_x, min_y, max_x, max_y)
}

/// Where to put a drawable: which of its anchors meets the same anchor of the
/// target, how it scales to fit, and how far it keeps from the edges.
pub(crate) struct Placement {
    anchor: gaanim_layout::Anchor,
    fit: FitMode,
    padding: Insets,
    offset: DVec3,
}

impl Placement {
    pub(crate) fn parse(
        canvas: &Arc<Mutex<ApiCanvas>>,
        anchor: Option<&Bound<'_, PyAny>>,
        fit: Option<&str>,
        padding: Option<&Bound<'_, PyAny>>,
        offset: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<Self> {
        let units = Units::new(canvas.clone());
        Ok(Self {
            anchor: anchor
                .map(parse_anchor)
                .transpose()?
                .unwrap_or(gaanim_layout::Anchor::Center),
            fit: fit.map(parse_fit).transpose()?.unwrap_or(FitMode::None),
            padding: padding
                .map(|padding| units.insets(padding, "padding", true))
                .transpose()?
                .unwrap_or_default(),
            offset: offset
                .map(|offset| units.point(offset, "offset"))
                .transpose()?
                .unwrap_or(DVec3::ZERO),
        })
    }

    /// The scale the fit mode applies to an object of `size` in `region`.
    fn scale(&self, size: DVec2, region: Bounds3D) -> DVec3 {
        let sx = region.width() / size.x.max(1.0e-9);
        let sy = region.height() / size.y.max(1.0e-9);
        match self.fit {
            FitMode::None => DVec3::ONE,
            FitMode::Contain => DVec3::splat(sx.min(sy)),
            FitMode::Cover => DVec3::splat(sx.max(sy)),
            FitMode::Stretch => DVec3::new(sx, sy, 1.0),
            FitMode::ScaleDown => DVec3::splat(sx.min(sy).min(1.0)),
        }
    }

    fn region(&self, target: Bounds3D) -> Bounds3D {
        inset(target, self.padding)
    }

    fn point(&self, target: Bounds3D) -> DVec3 {
        anchor_point(self.region(target), self.anchor) + self.offset
    }

    /// Move (and scale) `handle` into `target` now.
    pub(crate) fn place(&self, handle: &DrawableHandle, target: Bounds3D) -> PyResult<()> {
        let region = self.region(target);
        if self.fit != FitMode::None {
            let bounds = handle
                .bounds()
                .map_err(|error| pyo3::exceptions::PyValueError::new_err(error.to_string()))?;
            let scale = self.scale(DVec2::new(bounds.width(), bounds.height()), region);
            let factor = scale.x.min(scale.y);
            if self.fit == FitMode::Stretch {
                handle.clone().scale_by_3d(scale.x, scale.y, 1.0);
            } else {
                handle.clone().scale_by(factor);
            }
        }
        PyDrawable(handle.clone()).move_to_point(self.point(target), self.anchor)
    }

    /// The same placement as an animation.
    pub(crate) fn animate(
        &self,
        anim: gaanim_api::canvas::Anim,
        target: Bounds3D,
    ) -> PyResult<gaanim_api::canvas::Anim> {
        let region = self.region(target);
        let mut anim = anim;
        if self.fit != FitMode::None {
            let bounds = anim
                .target_bounds()
                .map_err(|error| pyo3::exceptions::PyValueError::new_err(error.to_string()))?;
            let scale = self.scale(DVec2::new(bounds.width(), bounds.height()), region);
            anim = anim.scale_by(scale.x.min(scale.y));
        }
        let point = self.point(target);
        Ok(anim.move_to_anchor(point.x, point.y, self.anchor))
    }
}

/// The rectangle a `place` target stands for: a zone, or a drawable's box.
pub(crate) fn target_bounds(target: &Bound<'_, PyAny>) -> PyResult<Bounds3D> {
    if let Ok(zone) = target.extract::<PyRef<'_, PyZone>>() {
        return Ok(zone.bounds);
    }
    handle_of(target)
        .map_err(|_| pyo3::exceptions::PyTypeError::new_err("place() needs a Zone or a Drawable"))?
        .bounds()
        .map_err(|error| pyo3::exceptions::PyValueError::new_err(error.to_string()))
}

/// Extension used by `Drawable.place` and `Anim.place`.
pub(crate) fn canvas_of(handle: &DrawableHandle) -> PyResult<Arc<Mutex<ApiCanvas>>> {
    handle.scene().ok_or_else(|| {
        pyo3::exceptions::PyRuntimeError::new_err("this drawable's Scene no longer exists")
    })
}

#[pymethods]
impl PyCanvasAnim {
    /// Animate a move (and scale, with `fit`) into a zone or onto a drawable.
    #[pyo3(signature = (target, *, anchor=None, fit=None, padding=None, offset=None))]
    fn place(
        &self,
        target: &Bound<'_, PyAny>,
        anchor: Option<&Bound<'_, PyAny>>,
        fit: Option<&str>,
        padding: Option<&Bound<'_, PyAny>>,
        offset: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<Self> {
        crate::custom::ensure_authoring_allowed()?;
        let canvas = match target.extract::<PyRef<'_, PyZone>>() {
            Ok(zone) => zone.canvas.clone(),
            Err(_) => canvas_of(&handle_of(target)?)?,
        };
        let placement = Placement::parse(&canvas, anchor, fit, padding, offset)?;
        Ok(Self {
            inner: placement.animate(self.inner.clone(), target_bounds(target)?)?,
        })
    }
}
