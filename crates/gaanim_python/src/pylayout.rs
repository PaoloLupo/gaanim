use std::sync::{Arc, Mutex, Weak};

use gaanim_api::canvas::{
    Anchor, Direction, DrawableHandle, LayoutMemberSpec, LayoutSpec, LayoutWithin,
    SceneModel as ApiCanvas,
};
use gaanim_core::glam::{DVec2, DVec3};
use gaanim_layout::{
    Align, AutoFlow, ConstraintRelation, ConstraintStrength, FitMode, Insets, Justify,
    LayoutAttribute, LayoutConstraint, LayoutExpression, LayoutItemStyle, LayoutNodeKind,
    LayoutStyle, SizeRule, Track,
};
use pyo3::prelude::*;
use pyo3::types::{PyAny, PySequence, PyString, PyTuple};

use crate::pydrawable::PyDrawable;

/// A linear expression over drawable layout attributes.
#[pyclass(
    name = "LayoutExpression",
    module = "gaanim_core",
    frozen,
    from_py_object
)]
#[derive(Clone)]
pub struct PyLayoutExpression {
    pub(crate) inner: LayoutExpression,
    pub(crate) owner: DrawableHandle,
}

impl PyLayoutExpression {
    fn operand(
        &self,
        value: &Bound<'_, PyAny>,
    ) -> PyResult<(LayoutExpression, Option<DrawableHandle>)> {
        if let Ok(expression) = value.extract::<PyRef<'_, Self>>() {
            if !self.owner.same_canvas(&expression.owner) {
                return Err(pyo3::exceptions::PyValueError::new_err(
                    "layout expressions cannot reference drawables from different Scenes",
                ));
            }
            return Ok((expression.inner.clone(), Some(expression.owner.clone())));
        }
        if let Ok(value) = value.extract::<f64>() {
            if value.is_finite() {
                return Ok((value.into(), None));
            }
        }
        Err(pyo3::exceptions::PyTypeError::new_err(
            "layout expressions only support finite scalars and other linear expressions",
        ))
    }

    fn relation(
        &self,
        rhs: &Bound<'_, PyAny>,
        relation: ConstraintRelation,
    ) -> PyResult<PyLayoutConstraint> {
        let (rhs, _) = self.operand(rhs)?;
        Ok(PyLayoutConstraint {
            inner: LayoutConstraint {
                lhs: self.inner.clone(),
                relation,
                rhs,
                strength: ConstraintStrength::Required,
                label: None,
            },
            owner: self.owner.clone(),
        })
    }
}

#[pymethods]
impl PyLayoutExpression {
    fn __add__(&self, rhs: &Bound<'_, PyAny>) -> PyResult<Self> {
        let (rhs, _) = self.operand(rhs)?;
        Ok(Self {
            inner: self.inner.clone() + rhs,
            owner: self.owner.clone(),
        })
    }

    fn __radd__(&self, lhs: &Bound<'_, PyAny>) -> PyResult<Self> {
        let (lhs, _) = self.operand(lhs)?;
        Ok(Self {
            inner: lhs + self.inner.clone(),
            owner: self.owner.clone(),
        })
    }

    fn __sub__(&self, rhs: &Bound<'_, PyAny>) -> PyResult<Self> {
        let (rhs, _) = self.operand(rhs)?;
        Ok(Self {
            inner: self.inner.clone() - rhs,
            owner: self.owner.clone(),
        })
    }

    fn __rsub__(&self, lhs: &Bound<'_, PyAny>) -> PyResult<Self> {
        let (lhs, _) = self.operand(lhs)?;
        Ok(Self {
            inner: lhs - self.inner.clone(),
            owner: self.owner.clone(),
        })
    }

    fn __mul__(&self, scalar: f64) -> PyResult<Self> {
        if !scalar.is_finite() {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "layout scalar must be finite",
            ));
        }
        Ok(Self {
            inner: self.inner.clone() * scalar,
            owner: self.owner.clone(),
        })
    }

    fn __rmul__(&self, scalar: f64) -> PyResult<Self> {
        self.__mul__(scalar)
    }

    fn __truediv__(&self, scalar: f64) -> PyResult<Self> {
        if !scalar.is_finite() || scalar == 0.0 {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "layout divisor must be finite and non-zero",
            ));
        }
        Ok(Self {
            inner: self.inner.clone() / scalar,
            owner: self.owner.clone(),
        })
    }

    fn __neg__(&self) -> Self {
        Self {
            inner: -self.inner.clone(),
            owner: self.owner.clone(),
        }
    }

    fn __eq__(&self, rhs: &Bound<'_, PyAny>) -> PyResult<PyLayoutConstraint> {
        self.relation(rhs, ConstraintRelation::Equal)
    }

    fn __le__(&self, rhs: &Bound<'_, PyAny>) -> PyResult<PyLayoutConstraint> {
        self.relation(rhs, ConstraintRelation::LessOrEqual)
    }

    fn __ge__(&self, rhs: &Bound<'_, PyAny>) -> PyResult<PyLayoutConstraint> {
        self.relation(rhs, ConstraintRelation::GreaterOrEqual)
    }
}

/// One prioritized linear relation in Layout v2.
#[pyclass(
    name = "LayoutConstraint",
    module = "gaanim_core",
    frozen,
    from_py_object
)]
#[derive(Clone)]
pub struct PyLayoutConstraint {
    pub(crate) inner: LayoutConstraint,
    pub(crate) owner: DrawableHandle,
}

#[pymethods]
impl PyLayoutConstraint {
    fn strong(&self) -> Self {
        let mut inner = self.inner.clone();
        inner.strength = ConstraintStrength::Strong;
        Self {
            inner,
            owner: self.owner.clone(),
        }
    }

    fn medium(&self) -> Self {
        let mut inner = self.inner.clone();
        inner.strength = ConstraintStrength::Medium;
        Self {
            inner,
            owner: self.owner.clone(),
        }
    }

    fn weak(&self) -> Self {
        let mut inner = self.inner.clone();
        inner.strength = ConstraintStrength::Weak;
        Self {
            inner,
            owner: self.owner.clone(),
        }
    }

    fn named(&self, label: String) -> Self {
        let mut inner = self.inner.clone();
        inner.label = Some(label);
        Self {
            inner,
            owner: self.owner.clone(),
        }
    }
}

/// Handle returned by `Scene.constrain`.
#[pyclass(
    name = "ConstraintSet",
    module = "gaanim_core",
    frozen,
    skip_from_py_object
)]
#[derive(Clone)]
pub struct PyConstraintSet {
    #[pyo3(get)]
    pub count: usize,
}

pub(crate) fn expression_for(
    drawable: &DrawableHandle,
    attribute: LayoutAttribute,
) -> PyLayoutExpression {
    PyLayoutExpression {
        inner: LayoutExpression::variable(gaanim_layout::LayoutId(drawable.id.as_raw()), attribute),
        owner: drawable.clone(),
    }
}

#[derive(Clone)]
pub(crate) struct LayoutMember {
    pub handle: DrawableHandle,
    pub style: LayoutItemStyle,
    child_layout: Option<Arc<Mutex<LayoutState>>>,
}

#[derive(Clone)]
struct LayoutState {
    canvas: Arc<Mutex<ApiCanvas>>,
    spec: LayoutSpec,
    members: Vec<LayoutMember>,
    root: DrawableHandle,
    version: u64,
    parents: Vec<Weak<Mutex<LayoutState>>>,
}

/// Per-child sizing and placement metadata used by Layout v2.
#[pyclass(
    name = "LayoutItem",
    module = "gaanim_core",
    frozen,
    skip_from_py_object
)]
#[derive(Clone)]
pub struct PyLayoutItem {
    pub(crate) member: LayoutMember,
}

#[pymethods]
impl PyLayoutItem {
    fn __repr__(&self) -> String {
        format!("LayoutItem(drawable={:?})", self.member.handle.id)
    }
}

/// Persistent, nestable Layout v2 container. Layout extends Drawable, so it
/// participates in styling and animations without an intermediate `.drawable`.
#[pyclass(name = "Layout", module = "gaanim_core", extends = PyDrawable, skip_from_py_object)]
#[derive(Clone)]
pub struct PyLayout {
    inner: Arc<Mutex<LayoutState>>,
}

impl PyLayout {
    pub(crate) fn initializer(
        canvas: Arc<Mutex<ApiCanvas>>,
        spec: LayoutSpec,
        members: Vec<LayoutMember>,
    ) -> PyResult<PyClassInitializer<Self>> {
        let refs: Vec<_> = members.iter().map(|member| &member.handle).collect();
        let root = canvas.lock().expect("scene canvas poisoned").group(&refs);
        for member in &members {
            member.handle.claim_layout(&root).map_err(layout_error)?;
        }
        let inner = Arc::new(Mutex::new(LayoutState {
            canvas,
            spec,
            members,
            root: root.clone(),
            version: 0,
            parents: Vec::new(),
        }));
        {
            let state = inner.lock().expect("layout poisoned");
            for member in &state.members {
                if let Some(child) = &member.child_layout {
                    child
                        .lock()
                        .expect("layout poisoned")
                        .parents
                        .push(Arc::downgrade(&inner));
                }
            }
        }
        Self::reflow_inner(&inner, None, None, None);
        Ok(PyClassInitializer::from(PyDrawable(root)).add_subclass(Self { inner }))
    }

    pub(crate) fn member_from_python(child: &Bound<'_, PyAny>) -> PyResult<LayoutMember> {
        if let Ok(item) = child.extract::<PyRef<'_, PyLayoutItem>>() {
            return Ok(item.member.clone());
        }
        if let Ok(layout) = child.extract::<PyRef<'_, PyLayout>>() {
            let state = layout.inner.lock().expect("layout poisoned");
            return Ok(LayoutMember {
                handle: state.root.clone(),
                style: LayoutItemStyle::default(),
                child_layout: Some(layout.inner.clone()),
            });
        }
        if let Ok(drawable) = child.extract::<PyRef<'_, PyDrawable>>() {
            return Ok(LayoutMember {
                handle: drawable.0.clone(),
                style: LayoutItemStyle::default(),
                child_layout: None,
            });
        }
        Err(pyo3::exceptions::PyTypeError::new_err(
            "layout children must be Drawable, Layout, or LayoutItem",
        ))
    }

    fn reflow_inner(
        inner: &Arc<Mutex<LayoutState>>,
        duration: Option<f64>,
        entering: Option<DrawableHandle>,
        leaving: Option<DrawableHandle>,
    ) {
        let (canvas, spec, root, members, parents, version) = {
            let mut state = inner.lock().expect("layout poisoned");
            state.version = state.version.saturating_add(1);
            (
                state.canvas.clone(),
                state.spec.clone(),
                state.root.clone(),
                state.members.clone(),
                state.parents.clone(),
                state.version,
            )
        };
        let refs: Vec<_> = members.iter().map(|member| &member.handle).collect();
        let snapshots = members
            .iter()
            .map(|member| LayoutMemberSpec {
                id: member.handle.id,
                style: member.style.clone(),
            })
            .collect();
        let mut canvas = canvas.lock().expect("scene canvas poisoned");
        canvas.set_group_members(&root, &refs);
        canvas.reflow_layout(
            &root,
            snapshots,
            spec,
            version,
            duration,
            entering.as_ref(),
            leaving.as_ref(),
        );
        drop(canvas);
        for parent in parents.into_iter().filter_map(|parent| parent.upgrade()) {
            Self::reflow_inner(&parent, duration, None, None);
        }
    }

    fn direct_member(child: &Bound<'_, PyAny>) -> PyResult<DrawableHandle> {
        Ok(Self::member_from_python(child)?.handle)
    }
}

#[pymethods]
impl PyLayout {
    #[getter]
    fn animate(&self) -> crate::pydrawable::PyCanvasAnim {
        let root = self.inner.lock().expect("layout poisoned").root.clone();
        crate::pydrawable::PyCanvasAnim {
            inner: root.animate(),
        }
    }

    #[getter]
    fn count(&self) -> usize {
        self.inner.lock().expect("layout poisoned").members.len()
    }

    #[pyo3(signature = (child, *, at=None))]
    fn add(&self, child: &Bound<'_, PyAny>, at: Option<usize>) -> PyResult<PyDrawable> {
        let member = Self::member_from_python(child)?;
        let handle = member.handle.clone();
        {
            let mut state = self.inner.lock().expect("layout poisoned");
            let index = at.unwrap_or(state.members.len());
            if index > state.members.len() {
                return Err(pyo3::exceptions::PyIndexError::new_err(
                    "layout insertion index is out of bounds",
                ));
            }
            if handle.id == state.root.id {
                return Err(pyo3::exceptions::PyValueError::new_err(
                    "a Layout cannot contain itself",
                ));
            }
            handle.claim_layout(&state.root).map_err(layout_error)?;
            if let Some(child_layout) = &member.child_layout {
                child_layout
                    .lock()
                    .expect("layout poisoned")
                    .parents
                    .push(Arc::downgrade(&self.inner));
            }
            state.members.insert(index, member);
        }
        Self::reflow_inner(&self.inner, None, Some(handle.clone()), None);
        Ok(PyDrawable(handle))
    }

    fn remove(&self, child: &Bound<'_, PyAny>) -> PyResult<()> {
        let handle = Self::direct_member(child)?;
        let removed = {
            let mut state = self.inner.lock().expect("layout poisoned");
            let index = state
                .members
                .iter()
                .position(|member| member.handle.id == handle.id)
                .ok_or_else(|| {
                    pyo3::exceptions::PyValueError::new_err(
                        "child is not a direct member of this Layout",
                    )
                })?;
            let removed = state.members.remove(index);
            removed.handle.release_layout(&state.root);
            removed
        };
        Self::reflow_inner(&self.inner, None, None, Some(removed.handle));
        Ok(())
    }

    /// Detach a direct child without hiding it, releasing positional ownership.
    fn detach(&self, child: &Bound<'_, PyAny>) -> PyResult<()> {
        let handle = Self::direct_member(child)?;
        {
            let mut state = self.inner.lock().expect("layout poisoned");
            let index = state
                .members
                .iter()
                .position(|member| member.handle.id == handle.id)
                .ok_or_else(|| {
                    pyo3::exceptions::PyValueError::new_err(
                        "child is not a direct member of this Layout",
                    )
                })?;
            let detached = state.members.remove(index);
            detached.handle.release_layout(&state.root);
        }
        Self::reflow_inner(&self.inner, None, None, None);
        Ok(())
    }

    #[pyo3(signature = (old, new))]
    fn replace(&self, old: &Bound<'_, PyAny>, new: &Bound<'_, PyAny>) -> PyResult<PyDrawable> {
        let old = Self::direct_member(old)?;
        let replacement = Self::member_from_python(new)?;
        let replacement_handle = replacement.handle.clone();
        {
            let mut state = self.inner.lock().expect("layout poisoned");
            let index = state
                .members
                .iter()
                .position(|member| member.handle.id == old.id)
                .ok_or_else(|| {
                    pyo3::exceptions::PyValueError::new_err(
                        "old is not a direct member of this Layout",
                    )
                })?;
            replacement_handle
                .claim_layout(&state.root)
                .map_err(layout_error)?;
            old.release_layout(&state.root);
            state.members[index] = replacement;
        }
        Self::reflow_inner(
            &self.inner,
            None,
            Some(replacement_handle.clone()),
            Some(old),
        );
        Ok(PyDrawable(replacement_handle))
    }

    #[pyo3(signature = (*, gap=None, padding=None, width=None, height=None, min_width=None, max_width=None, min_height=None, max_height=None, aspect_ratio=None, align=None, justify=None, wrap=None, within=None))]
    #[allow(clippy::too_many_arguments)]
    fn configure(
        &self,
        gap: Option<f64>,
        padding: Option<&Bound<'_, PyAny>>,
        width: Option<&Bound<'_, PyAny>>,
        height: Option<&Bound<'_, PyAny>>,
        min_width: Option<f64>,
        max_width: Option<f64>,
        min_height: Option<f64>,
        max_height: Option<f64>,
        aspect_ratio: Option<f64>,
        align: Option<&str>,
        justify: Option<&str>,
        wrap: Option<bool>,
        within: Option<&str>,
    ) -> PyResult<()> {
        {
            let mut state = self.inner.lock().expect("layout poisoned");
            if let Some(gap) = gap {
                finite_non_negative(gap, "gap")?;
                state.spec.style.gap = DVec2::splat(gap);
            }
            if let Some(padding) = padding {
                state.spec.style.padding = parse_padding(padding)?;
            }
            if let Some(width) = width {
                state.spec.style.width = parse_size(width, "width")?;
            }
            if let Some(height) = height {
                state.spec.style.height = parse_size(height, "height")?;
            }
            if let Some(value) = min_width {
                finite_non_negative(value, "min_width")?;
                state.spec.style.min_width = Some(value);
            }
            if let Some(value) = max_width {
                finite_non_negative(value, "max_width")?;
                state.spec.style.max_width = Some(value);
            }
            if let Some(value) = min_height {
                finite_non_negative(value, "min_height")?;
                state.spec.style.min_height = Some(value);
            }
            if let Some(value) = max_height {
                finite_non_negative(value, "max_height")?;
                state.spec.style.max_height = Some(value);
            }
            if let Some(value) = aspect_ratio {
                if !value.is_finite() || value <= 0.0 {
                    return Err(pyo3::exceptions::PyValueError::new_err(
                        "aspect_ratio must be a finite positive number",
                    ));
                }
                state.spec.style.aspect_ratio = Some(value);
            }
            if let Some(align) = align {
                state.spec.style.align = parse_align(align)?;
            }
            if let Some(justify) = justify {
                state.spec.style.justify = parse_justify(justify)?;
            }
            if let Some(wrap) = wrap {
                match &mut state.spec.kind {
                    LayoutNodeKind::Row { wrap: current }
                    | LayoutNodeKind::Column { wrap: current } => *current = wrap,
                    _ => {
                        return Err(pyo3::exceptions::PyValueError::new_err(
                            "wrap is only valid for row and column layouts",
                        ));
                    }
                }
            }
            if let Some(within) = within {
                state.spec.within = parse_within(Some(within))?;
            }
        }
        Self::reflow_inner(&self.inner, None, None, None);
        Ok(())
    }

    #[pyo3(signature = (child, *, grow=None, shrink=None, align=None, row=None, column=None, row_span=None, column_span=None, absolute=None, anchor=None, offset=None, fit=None))]
    #[allow(clippy::too_many_arguments)]
    fn configure_item(
        &self,
        child: &Bound<'_, PyAny>,
        grow: Option<f64>,
        shrink: Option<f64>,
        align: Option<&str>,
        row: Option<usize>,
        column: Option<usize>,
        row_span: Option<usize>,
        column_span: Option<usize>,
        absolute: Option<bool>,
        anchor: Option<&PyAnchor>,
        offset: Option<(f64, f64)>,
        fit: Option<&str>,
    ) -> PyResult<()> {
        let handle = Self::direct_member(child)?;
        {
            let mut state = self.inner.lock().expect("layout poisoned");
            let member = state
                .members
                .iter_mut()
                .find(|member| member.handle.id == handle.id)
                .ok_or_else(|| {
                    pyo3::exceptions::PyValueError::new_err(
                        "child is not a direct member of this Layout",
                    )
                })?;
            if let Some(grow) = grow {
                finite_non_negative(grow, "grow")?;
                member.style.grow = grow;
            }
            if let Some(shrink) = shrink {
                finite_non_negative(shrink, "shrink")?;
                member.style.shrink = shrink;
            }
            if let Some(align) = align {
                member.style.align = Some(parse_align(align)?);
            }
            if row.is_some() {
                member.style.row = row;
            }
            if column.is_some() {
                member.style.column = column;
            }
            if let Some(row_span) = row_span {
                member.style.row_span = row_span.max(1);
            }
            if let Some(column_span) = column_span {
                member.style.column_span = column_span.max(1);
            }
            if let Some(absolute) = absolute {
                member.style.absolute = absolute;
            }
            if let Some(anchor) = anchor {
                member.style.anchor = anchor.0;
            }
            if let Some((x, y)) = offset {
                member.style.offset = DVec3::new(x, y, 0.0);
            }
            if let Some(fit) = fit {
                member.style.fit = parse_fit(fit)?;
            }
        }
        Self::reflow_inner(&self.inner, None, None, None);
        Ok(())
    }

    fn reflow(&self) {
        Self::reflow_inner(&self.inner, None, None, None);
    }

    fn diagnostics(&self) -> Vec<String> {
        let (canvas, root) = {
            let state = self.inner.lock().expect("layout poisoned");
            (state.canvas.clone(), state.root.clone())
        };
        let diagnostics = canvas
            .lock()
            .expect("scene canvas poisoned")
            .layout_diagnostics(&root);
        diagnostics
    }
}

pub(crate) fn layout_item_from_python(
    child: &Bound<'_, PyAny>,
    grow: f64,
    shrink: f64,
    align: Option<&str>,
    row: Option<usize>,
    column: Option<usize>,
    row_span: usize,
    column_span: usize,
    absolute: bool,
    anchor: Option<&PyAnchor>,
    offset: (f64, f64),
    fit: &str,
) -> PyResult<PyLayoutItem> {
    finite_non_negative(grow, "grow")?;
    finite_non_negative(shrink, "shrink")?;
    let mut member = PyLayout::member_from_python(child)?;
    member.style = LayoutItemStyle {
        grow,
        shrink,
        align: align.map(parse_align).transpose()?,
        row,
        column,
        row_span: row_span.max(1),
        column_span: column_span.max(1),
        absolute,
        anchor: anchor.map(|anchor| anchor.0).unwrap_or(Anchor::Center),
        offset: DVec3::new(offset.0, offset.1, 0.0),
        fit: parse_fit(fit)?,
    };
    Ok(PyLayoutItem { member })
}

pub(crate) fn layout_spec(
    kind: LayoutNodeKind,
    gap: f64,
    padding: Option<&Bound<'_, PyAny>>,
    width: Option<&Bound<'_, PyAny>>,
    height: Option<&Bound<'_, PyAny>>,
    align: &str,
    justify: &str,
    within: Option<&str>,
) -> PyResult<LayoutSpec> {
    finite_non_negative(gap, "gap")?;
    Ok(LayoutSpec {
        kind,
        style: LayoutStyle {
            width: width.map_or(Ok(SizeRule::Hug), |value| parse_size(value, "width"))?,
            height: height.map_or(Ok(SizeRule::Hug), |value| parse_size(value, "height"))?,
            padding: padding.map_or(Ok(Insets::default()), parse_padding)?,
            gap: DVec2::splat(gap),
            align: parse_align(align)?,
            justify: parse_justify(justify)?,
            ..LayoutStyle::default()
        },
        within: parse_within(within)?,
    })
}

pub(crate) fn parse_grid_tracks(
    value: Option<&Bound<'_, PyAny>>,
    axis: &str,
) -> PyResult<Vec<Track>> {
    let Some(value) = value else {
        return Ok(vec![Track::Fraction(1.0)]);
    };
    if let Ok(count) = value.extract::<usize>() {
        if count == 0 {
            return Err(pyo3::exceptions::PyValueError::new_err(format!(
                "{axis} count must be greater than zero"
            )));
        }
        return Ok(vec![Track::Fraction(1.0); count]);
    }
    let sequence = value.cast::<PySequence>().map_err(|_| {
        pyo3::exceptions::PyTypeError::new_err(format!(
            "{axis} must be an integer or sequence of fixed numbers, 'auto', or '<weight>fr'"
        ))
    })?;
    let mut tracks = Vec::with_capacity(sequence.len()? as usize);
    for item in sequence.try_iter()? {
        let item = item?;
        if let Ok(value) = item.extract::<f64>() {
            finite_non_negative(value, axis)?;
            tracks.push(Track::Fixed(value));
            continue;
        }
        let value = item.cast::<PyString>()?.to_str()?;
        if value == "auto" {
            tracks.push(Track::Auto);
            continue;
        }
        let weight = value
            .strip_suffix("fr")
            .and_then(|value| value.trim().parse::<f64>().ok())
            .filter(|value| value.is_finite() && *value > 0.0)
            .ok_or_else(|| {
                pyo3::exceptions::PyValueError::new_err(format!("invalid {axis} track {value:?}"))
            })?;
        tracks.push(Track::Fraction(weight));
    }
    if tracks.is_empty() {
        return Err(pyo3::exceptions::PyValueError::new_err(format!(
            "{axis} tracks cannot be empty"
        )));
    }
    Ok(tracks)
}

fn parse_size(value: &Bound<'_, PyAny>, name: &str) -> PyResult<SizeRule> {
    if let Ok(value) = value.extract::<f64>() {
        finite_non_negative(value, name)?;
        return Ok(SizeRule::Fixed(value));
    }
    let value = value.extract::<String>().map_err(|_| {
        pyo3::exceptions::PyTypeError::new_err(format!(
            "{name} must be a non-negative number, 'hug', or 'fill'"
        ))
    })?;
    match value.as_str() {
        "hug" => Ok(SizeRule::Hug),
        "fill" => Ok(SizeRule::Fill(1.0)),
        _ => Err(pyo3::exceptions::PyValueError::new_err(format!(
            "{name} must be a non-negative number, 'hug', or 'fill'"
        ))),
    }
}

fn parse_padding(value: &Bound<'_, PyAny>) -> PyResult<Insets> {
    if let Ok(value) = value.extract::<f64>() {
        finite_non_negative(value, "padding")?;
        return Ok(Insets::all(value));
    }
    let tuple = value.cast::<PyTuple>().map_err(|_| {
        pyo3::exceptions::PyTypeError::new_err(
            "padding must be a number, (vertical, horizontal), or (top, right, bottom, left)",
        )
    })?;
    let values: Vec<f64> = tuple
        .iter()
        .map(|item| item.extract::<f64>())
        .collect::<PyResult<_>>()?;
    for value in &values {
        finite_non_negative(*value, "padding")?;
    }
    match values.as_slice() {
        [vertical, horizontal] => Ok(Insets::symmetric(*vertical, *horizontal)),
        [top, right, bottom, left] => Ok(Insets {
            top: *top,
            right: *right,
            bottom: *bottom,
            left: *left,
        }),
        _ => Err(pyo3::exceptions::PyValueError::new_err(
            "padding tuple must contain two or four values",
        )),
    }
}

fn parse_align(value: &str) -> PyResult<Align> {
    match value {
        "start" => Ok(Align::Start),
        "center" => Ok(Align::Center),
        "end" => Ok(Align::End),
        "stretch" => Ok(Align::Stretch),
        _ => Err(pyo3::exceptions::PyValueError::new_err(
            "align must be 'start', 'center', 'end', or 'stretch'",
        )),
    }
}

fn parse_justify(value: &str) -> PyResult<Justify> {
    match value {
        "start" => Ok(Justify::Start),
        "center" => Ok(Justify::Center),
        "end" => Ok(Justify::End),
        "between" => Ok(Justify::Between),
        "around" => Ok(Justify::Around),
        "evenly" => Ok(Justify::Evenly),
        _ => Err(pyo3::exceptions::PyValueError::new_err(
            "justify must be 'start', 'center', 'end', 'between', 'around', or 'evenly'",
        )),
    }
}

fn parse_fit(value: &str) -> PyResult<FitMode> {
    match value {
        "none" => Ok(FitMode::None),
        "contain" => Ok(FitMode::Contain),
        "cover" => Ok(FitMode::Cover),
        "stretch" => Ok(FitMode::Stretch),
        "scale_down" => Ok(FitMode::ScaleDown),
        _ => Err(pyo3::exceptions::PyValueError::new_err(
            "fit must be 'none', 'contain', 'cover', 'stretch', or 'scale_down'",
        )),
    }
}

fn parse_within(value: Option<&str>) -> PyResult<LayoutWithin> {
    match value {
        None => Ok(LayoutWithin::Intrinsic),
        Some("safe") => Ok(LayoutWithin::Safe),
        Some("frame") => Ok(LayoutWithin::Frame),
        Some(_) => Err(pyo3::exceptions::PyValueError::new_err(
            "within must be None, 'safe', or 'frame'",
        )),
    }
}

fn finite_non_negative(value: f64, name: &str) -> PyResult<()> {
    if value.is_finite() && value >= 0.0 {
        Ok(())
    } else {
        Err(pyo3::exceptions::PyValueError::new_err(format!(
            "{name} must be a finite non-negative number"
        )))
    }
}

fn layout_error(error: gaanim_api::canvas::LayoutOwnershipError) -> PyErr {
    crate::LayoutOwnershipError::new_err(error.to_string())
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
    #[pyo3(signature = (x, y, z=0.0))]
    fn custom(x: f64, y: f64, z: f64) -> Self {
        Self(Direction::Custom(DVec3::new(x, y, z)))
    }
}

pub(crate) fn row_kind(wrap: bool) -> LayoutNodeKind {
    LayoutNodeKind::Row { wrap }
}

pub(crate) fn column_kind(wrap: bool) -> LayoutNodeKind {
    LayoutNodeKind::Column { wrap }
}

pub(crate) fn stack_kind() -> LayoutNodeKind {
    LayoutNodeKind::Stack
}

pub(crate) fn grid_kind(
    rows: Vec<Track>,
    columns: Vec<Track>,
    auto_flow: &str,
) -> PyResult<LayoutNodeKind> {
    let auto_flow = match auto_flow {
        "row" => AutoFlow::Row,
        "column" => AutoFlow::Column,
        _ => {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "auto_flow must be 'row' or 'column'",
            ));
        }
    };
    Ok(LayoutNodeKind::Grid {
        rows,
        columns,
        auto_flow,
    })
}
