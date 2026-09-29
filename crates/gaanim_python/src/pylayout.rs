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
use pyo3::types::{PyAny, PyDict, PySequence, PyString, PyTuple};

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
        if let Ok(value) = value.extract::<f64>()
            && value.is_finite()
        {
            return Ok((value.into(), None));
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

// ---------------------------------------------------------------------------
// Boxes: CSS-style containers (flexbox, grid, overlay) with a box model.
// ---------------------------------------------------------------------------

/// Converts layout lengths to scene units: numbers are scene units,
/// `"16px"` design pixels, `"50%"` a share of the parent (sizes only) and
/// any other name a theme layout token.
pub(crate) struct Units {
    canvas: Arc<Mutex<ApiCanvas>>,
}

impl Units {
    pub(crate) fn new(canvas: Arc<Mutex<ApiCanvas>>) -> Self {
        Self { canvas }
    }

    /// Units of the scene `drawable` belongs to.
    pub(crate) fn of(drawable: &DrawableHandle) -> PyResult<Self> {
        drawable.scene().map(Self::new).ok_or_else(|| {
            pyo3::exceptions::PyRuntimeError::new_err("this drawable's Scene no longer exists")
        })
    }

    fn number(value: &Bound<'_, PyAny>, name: &str) -> PyResult<Option<f64>> {
        if value.is_instance_of::<pyo3::types::PyBool>() {
            return Err(pyo3::exceptions::PyTypeError::new_err(format!(
                "{name} must be a length, not a bool"
            )));
        }
        match value.extract::<f64>() {
            Ok(number) if number.is_finite() => Ok(Some(number)),
            Ok(_) => Err(pyo3::exceptions::PyValueError::new_err(format!(
                "{name} must be finite"
            ))),
            Err(_) => Ok(None),
        }
    }

    fn text(value: &Bound<'_, PyAny>, name: &str, expected: &str) -> PyResult<String> {
        value
            .extract::<String>()
            .map(|text| text.trim().to_owned())
            .map_err(|_| {
                pyo3::exceptions::PyTypeError::new_err(format!("{name} must be {expected}"))
            })
    }

    fn suffixed(text: &str, suffix: &str) -> Option<f64> {
        text.strip_suffix(suffix)
            .and_then(|number| number.trim().parse::<f64>().ok())
            .filter(|number| number.is_finite())
    }

    /// A length: scene units, `"Npx"` or a theme token.
    pub(crate) fn length(&self, value: &Bound<'_, PyAny>, name: &str) -> PyResult<f64> {
        if let Some(number) = Self::number(value, name)? {
            return Ok(number);
        }
        let text = Self::text(value, name, "a number, 'Npx' or a theme token")?;
        if let Some(pixels) = Self::suffixed(&text, "px") {
            let unit = self
                .canvas
                .lock()
                .expect("scene canvas poisoned")
                .pixel_unit();
            return Ok(pixels * unit);
        }
        if let Ok(number) = text.parse::<f64>()
            && number.is_finite()
        {
            return Ok(number);
        }
        self.canvas
            .lock()
            .expect("scene canvas poisoned")
            .theme_layout_token(&text)
            .map_err(|_| {
                pyo3::exceptions::PyValueError::new_err(format!(
                    "{name} {text:?} is not a number, 'Npx' length or theme layout token"
                ))
            })
    }

    fn non_negative(&self, value: &Bound<'_, PyAny>, name: &str) -> PyResult<f64> {
        let length = self.length(value, name)?;
        finite_non_negative(length, name)?;
        Ok(length)
    }

    /// A box size: a length, `"hug"`, `"fill"`, `"Nfr"` or `"N%"`.
    pub(crate) fn size(&self, value: &Bound<'_, PyAny>, name: &str) -> PyResult<SizeRule> {
        if let Some(number) = Self::number(value, name)? {
            finite_non_negative(number, name)?;
            return Ok(SizeRule::Fixed(number));
        }
        let text = Self::text(value, name, "a length, 'hug', 'fill', 'Nfr' or 'N%'")?;
        match text.as_str() {
            "hug" | "auto" => return Ok(SizeRule::Hug),
            "fill" => return Ok(SizeRule::Fill(1.0)),
            _ => {}
        }
        if let Some(weight) = Self::suffixed(&text, "fr") {
            return if weight > 0.0 {
                Ok(SizeRule::Fill(weight))
            } else {
                Err(pyo3::exceptions::PyValueError::new_err(format!(
                    "{name} fraction must be positive"
                )))
            };
        }
        if let Some(percent) = Self::suffixed(&text, "%") {
            finite_non_negative(percent, name)?;
            return Ok(SizeRule::Percent(percent));
        }
        Ok(SizeRule::Fixed(self.non_negative(value, name)?))
    }

    /// A grid track: a length, `"auto"`, `"Nfr"` or `"N%"`.
    fn track(&self, value: &Bound<'_, PyAny>, name: &str) -> PyResult<Track> {
        if let Some(number) = Self::number(value, name)? {
            finite_non_negative(number, name)?;
            return Ok(Track::Fixed(number));
        }
        let text = Self::text(value, name, "a length, 'auto', 'Nfr' or 'N%'")?;
        if text == "auto" {
            return Ok(Track::Auto);
        }
        if let Some(weight) = Self::suffixed(&text, "fr").filter(|weight| *weight > 0.0) {
            return Ok(Track::Fraction(weight));
        }
        if let Some(percent) = Self::suffixed(&text, "%") {
            finite_non_negative(percent, name)?;
            return Ok(Track::Percent(percent));
        }
        Ok(Track::Fixed(self.non_negative(value, name)?))
    }

    /// Grid tracks: a count of equal `1fr` tracks or a sequence of tracks.
    pub(crate) fn tracks(&self, value: &Bound<'_, PyAny>, name: &str) -> PyResult<Vec<Track>> {
        if let Ok(count) = value.extract::<usize>()
            && !value.is_instance_of::<pyo3::types::PyBool>()
        {
            if count == 0 {
                return Err(pyo3::exceptions::PyValueError::new_err(format!(
                    "{name} needs at least one track"
                )));
            }
            return Ok(vec![Track::Fraction(1.0); count]);
        }
        let sequence = value.cast::<PySequence>().map_err(|_| {
            pyo3::exceptions::PyTypeError::new_err(format!(
                "{name} must be a track count or a sequence of tracks"
            ))
        })?;
        let tracks = sequence
            .try_iter()?
            .map(|item| self.track(&item?, name))
            .collect::<PyResult<Vec<_>>>()?;
        if tracks.is_empty() {
            return Err(pyo3::exceptions::PyValueError::new_err(format!(
                "{name} needs at least one track"
            )));
        }
        Ok(tracks)
    }

    /// CSS shorthand: one value, (vertical, horizontal), (top, horizontal,
    /// bottom) or (top, right, bottom, left).
    /// Margins: like insets, but negative lengths and `"auto"` (take the
    /// free space, stored as an infinite length) are allowed.
    pub(crate) fn margins(&self, value: &Bound<'_, PyAny>) -> PyResult<Insets> {
        let is_auto =
            |value: &Bound<'_, PyAny>| value.extract::<String>().is_ok_and(|text| text == "auto");
        let side = |value: &Bound<'_, PyAny>| {
            if is_auto(value) {
                Ok(f64::INFINITY)
            } else {
                self.length(value, "margin")
            }
        };
        let values = match value.cast::<PyTuple>() {
            Ok(tuple) => tuple
                .iter()
                .map(|item| side(&item))
                .collect::<PyResult<Vec<_>>>()?,
            Err(_) => vec![side(value)?],
        };
        Self::sides(&values, "margin")
    }

    pub(crate) fn insets(
        &self,
        value: &Bound<'_, PyAny>,
        name: &str,
        negative: bool,
    ) -> PyResult<Insets> {
        let side = |value: &Bound<'_, PyAny>| {
            if negative {
                self.length(value, name)
            } else {
                self.non_negative(value, name)
            }
        };
        let Ok(tuple) = value.cast::<PyTuple>() else {
            return Ok(Insets::all(side(value)?));
        };
        let values = tuple
            .iter()
            .map(|item| side(&item))
            .collect::<PyResult<Vec<_>>>()?;
        Self::sides(&values, name)
    }

    /// CSS shorthand: all, (vertical, horizontal), (top, horizontal,
    /// bottom) or (top, right, bottom, left).
    fn sides(values: &[f64], name: &str) -> PyResult<Insets> {
        match values {
            [all] => Ok(Insets::all(*all)),
            [vertical, horizontal] => Ok(Insets::symmetric(*vertical, *horizontal)),
            [top, horizontal, bottom] => Ok(Insets {
                top: *top,
                right: *horizontal,
                bottom: *bottom,
                left: *horizontal,
            }),
            [top, right, bottom, left] => Ok(Insets {
                top: *top,
                right: *right,
                bottom: *bottom,
                left: *left,
            }),
            _ => Err(pyo3::exceptions::PyValueError::new_err(format!(
                "{name} takes one to four values"
            ))),
        }
    }

    pub(crate) fn point(&self, value: &Bound<'_, PyAny>, name: &str) -> PyResult<DVec3> {
        let tuple = value.cast::<PyTuple>().map_err(|_| {
            pyo3::exceptions::PyTypeError::new_err(format!("{name} must be an (x, y) pair"))
        })?;
        if tuple.len() != 2 {
            return Err(pyo3::exceptions::PyValueError::new_err(format!(
                "{name} must be an (x, y) pair"
            )));
        }
        Ok(DVec3::new(
            self.length(&tuple.get_item(0)?, name)?,
            self.length(&tuple.get_item(1)?, name)?,
            0.0,
        ))
    }
}

pub(crate) fn parse_anchor(value: &Bound<'_, PyAny>) -> PyResult<Anchor> {
    if let Ok(anchor) = value.extract::<PyAnchor>() {
        return Ok(anchor.0);
    }
    let name = value.extract::<String>().map_err(|_| {
        pyo3::exceptions::PyTypeError::new_err("anchor must be an Anchor or its name")
    })?;
    Ok(
        match name.trim().to_ascii_lowercase().replace('-', "_").as_str() {
            "center" => Anchor::Center,
            "top" => Anchor::Top,
            "bottom" => Anchor::Bottom,
            "left" => Anchor::Left,
            "right" => Anchor::Right,
            "top_left" => Anchor::TopLeft,
            "top_right" => Anchor::TopRight,
            "bottom_left" => Anchor::BottomLeft,
            "bottom_right" => Anchor::BottomRight,
            _ => {
                return Err(pyo3::exceptions::PyValueError::new_err(
                    "anchor must be center, top, bottom, left, right, top_left, top_right, bottom_left or bottom_right",
                ));
            }
        },
    )
}

/// Box properties that describe the container itself.
const CONTAINER_KEYS: &[&str] = &[
    "direction",
    "gap",
    "row_gap",
    "column_gap",
    "padding",
    "width",
    "height",
    "min_width",
    "max_width",
    "min_height",
    "max_height",
    "aspect_ratio",
    "align",
    "justify",
    "wrap",
    "columns",
    "rows",
    "auto_flow",
    "within",
];
/// Box properties that describe how a child sits in its parent.
pub(crate) const ITEM_KEYS: &[&str] = &[
    "grow",
    "shrink",
    "basis",
    "align_self",
    "row",
    "column",
    "row_span",
    "column_span",
    "margin",
    "fit",
    "anchor",
    "absolute",
    "offset",
];
/// Box properties drawn by its background.
const DECORATION_KEYS: &[&str] = &[
    "background",
    "border",
    "border_width",
    "radius",
    "shadow",
    "clip",
];
/// Typography passed to the text a box creates from `str` children.
const TEXT_KEYS: &[&str] = &[
    "color",
    "font",
    "font_size",
    "weight",
    "italic",
    "role",
    "text_align",
    "line_spacing",
    "letter_spacing",
    "max_lines",
    "overflow",
    "markup",
];

/// Named box styles of each scene, used by `class_=`.
fn classes()
-> &'static Mutex<std::collections::HashMap<usize, std::collections::HashMap<String, Py<PyDict>>>> {
    static CLASSES: std::sync::OnceLock<
        Mutex<std::collections::HashMap<usize, std::collections::HashMap<String, Py<PyDict>>>>,
    > = std::sync::OnceLock::new();
    CLASSES.get_or_init(Default::default)
}

fn scene_key(canvas: &Arc<Mutex<ApiCanvas>>) -> usize {
    Arc::as_ptr(canvas).cast::<()>() as usize
}

/// Register named styles for `class_=` in this scene.
pub(crate) fn define_classes(
    py: Python<'_>,
    canvas: &Arc<Mutex<ApiCanvas>>,
    named: &Bound<'_, PyDict>,
) -> PyResult<()> {
    let mut parsed = Vec::new();
    for (name, style) in named.iter() {
        let name = name.extract::<String>()?;
        if name.trim().is_empty() || name.contains(char::is_whitespace) {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "class names cannot be empty or contain spaces",
            ));
        }
        parsed.push((name, merged_props(py, None, Some(&style), None)?.unbind()));
    }
    let mut registry = classes().lock().expect("class registry poisoned");
    let scene = registry.entry(scene_key(canvas)).or_default();
    scene.extend(parsed);
    Ok(())
}

/// Resolve `class_` ("card elevated" or a list) against the scene's classes.
fn class_props<'py>(
    py: Python<'py>,
    canvas: &Arc<Mutex<ApiCanvas>>,
    names: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyDict>> {
    let names: Vec<String> = if let Ok(text) = names.extract::<String>() {
        text.split_whitespace().map(str::to_owned).collect()
    } else {
        names.extract::<Vec<String>>().map_err(|_| {
            pyo3::exceptions::PyTypeError::new_err("class_ must be a string or a list of names")
        })?
    };
    let merged = PyDict::new(py);
    let registry = classes().lock().expect("class registry poisoned");
    let scene = registry.get(&scene_key(canvas));
    for name in names {
        let style = scene.and_then(|scene| scene.get(&name)).ok_or_else(|| {
            pyo3::exceptions::PyValueError::new_err(format!(
                "unknown class {name:?}; define it with scene.layout.classes({name}=BoxStyle(...))"
            ))
        })?;
        merged.update(style.bind(py).as_mapping())?;
    }
    Ok(merged)
}

/// Merge the scene classes named by `class_`, then `style` (a mapping or a
/// `BoxStyle`), then the inline properties into one mapping, rejecting
/// unknown names. Later sources win, like CSS specificity.
fn merged_props<'py>(
    py: Python<'py>,
    canvas: Option<&Arc<Mutex<ApiCanvas>>>,
    style: Option<&Bound<'py, PyAny>>,
    props: Option<&Bound<'py, PyDict>>,
) -> PyResult<Bound<'py, PyDict>> {
    let merged = PyDict::new(py);
    let class_names = props
        .map(|props| props.get_item("class_"))
        .transpose()?
        .flatten()
        .filter(|value| !value.is_none());
    if let (Some(canvas), Some(names)) = (canvas, &class_names) {
        merged.update(class_props(py, canvas, names)?.as_mapping())?;
    } else if class_names.is_some() {
        return Err(pyo3::exceptions::PyTypeError::new_err(
            "class_ is resolved when a box is created; pass it to the box",
        ));
    }
    if let Some(style) = style {
        let mapping = if let Ok(style) = style.extract::<PyRef<'_, PyBoxStyle>>() {
            style.props.bind(py).clone()
        } else {
            style.cast::<PyDict>().cloned().map_err(|_| {
                pyo3::exceptions::PyTypeError::new_err("style must be a BoxStyle or a dict")
            })?
        };
        merged.update(mapping.as_mapping())?;
    }
    if let Some(props) = props {
        merged.update(props.as_mapping())?;
    }
    if merged.contains("class_")? {
        merged.del_item("class_")?;
    }
    for key in merged.keys() {
        let key = key.extract::<String>()?;
        let known = [CONTAINER_KEYS, ITEM_KEYS, DECORATION_KEYS, TEXT_KEYS]
            .iter()
            .any(|keys| keys.contains(&key.as_str()));
        if !known {
            return Err(pyo3::exceptions::PyTypeError::new_err(format!(
                "unknown box property {key:?}"
            )));
        }
    }
    Ok(merged)
}

fn prop<'py>(props: &Bound<'py, PyDict>, key: &str) -> PyResult<Option<Bound<'py, PyAny>>> {
    Ok(props.get_item(key)?.filter(|value| !value.is_none()))
}

fn apply_container(
    spec: &mut LayoutSpec,
    props: &Bound<'_, PyDict>,
    units: &Units,
) -> PyResult<()> {
    if let Some(value) = prop(props, "direction")? {
        let direction = value.extract::<String>()?;
        let wrap = matches!(
            spec.kind,
            LayoutNodeKind::Row { wrap: true } | LayoutNodeKind::Column { wrap: true }
        );
        spec.kind = match direction.as_str() {
            "row" => LayoutNodeKind::Row { wrap },
            "column" => LayoutNodeKind::Column { wrap },
            "stack" => LayoutNodeKind::Stack,
            "grid" => match &spec.kind {
                grid @ LayoutNodeKind::Grid { .. } => grid.clone(),
                _ => LayoutNodeKind::Grid {
                    rows: vec![Track::Auto],
                    columns: vec![Track::Fraction(1.0)],
                    auto_flow: AutoFlow::Row,
                },
            },
            _ => {
                return Err(pyo3::exceptions::PyValueError::new_err(
                    "direction must be 'row', 'column', 'grid' or 'stack'",
                ));
            }
        };
    }
    let style = &mut spec.style;
    if let Some(value) = prop(props, "gap")? {
        style.gap = DVec2::splat(units.non_negative(&value, "gap")?);
    }
    if let Some(value) = prop(props, "column_gap")? {
        style.gap.x = units.non_negative(&value, "column_gap")?;
    }
    if let Some(value) = prop(props, "row_gap")? {
        style.gap.y = units.non_negative(&value, "row_gap")?;
    }
    if let Some(value) = prop(props, "padding")? {
        style.padding = units.insets(&value, "padding", false)?;
    }
    if let Some(value) = prop(props, "width")? {
        style.width = units.size(&value, "width")?;
    }
    if let Some(value) = prop(props, "height")? {
        style.height = units.size(&value, "height")?;
    }
    for (key, slot) in [
        ("min_width", &mut style.min_width),
        ("max_width", &mut style.max_width),
        ("min_height", &mut style.min_height),
        ("max_height", &mut style.max_height),
    ] {
        if let Some(value) = prop(props, key)? {
            *slot = Some(units.non_negative(&value, key)?);
        }
    }
    if let Some(value) = prop(props, "aspect_ratio")? {
        let ratio = value.extract::<f64>()?;
        if !ratio.is_finite() || ratio <= 0.0 {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "aspect_ratio must be a finite positive number",
            ));
        }
        style.aspect_ratio = Some(ratio);
    }
    if let Some(value) = prop(props, "align")? {
        style.align = parse_align(&value.extract::<String>()?)?;
    }
    if let Some(value) = prop(props, "justify")? {
        style.justify = parse_justify(&value.extract::<String>()?)?;
    }
    if let Some(value) = prop(props, "wrap")? {
        let wrap = value.extract::<bool>()?;
        match &mut spec.kind {
            LayoutNodeKind::Row { wrap: current } | LayoutNodeKind::Column { wrap: current } => {
                *current = wrap
            }
            _ => {
                return Err(pyo3::exceptions::PyValueError::new_err(
                    "wrap applies to rows and columns",
                ));
            }
        }
    }
    let grid_value = |key: &str| -> PyResult<Option<Vec<Track>>> {
        prop(props, key)?
            .map(|value| units.tracks(&value, key))
            .transpose()
    };
    let (columns, rows) = (grid_value("columns")?, grid_value("rows")?);
    let auto_flow = prop(props, "auto_flow")?
        .map(|value| -> PyResult<AutoFlow> {
            match value.extract::<String>()?.as_str() {
                "row" => Ok(AutoFlow::Row),
                "column" => Ok(AutoFlow::Column),
                _ => Err(pyo3::exceptions::PyValueError::new_err(
                    "auto_flow must be 'row' or 'column'",
                )),
            }
        })
        .transpose()?;
    if columns.is_some() || rows.is_some() || auto_flow.is_some() {
        let LayoutNodeKind::Grid {
            rows: current_rows,
            columns: current_columns,
            auto_flow: current_flow,
        } = &mut spec.kind
        else {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "columns, rows and auto_flow apply to grids",
            ));
        };
        if let Some(columns) = columns {
            *current_columns = columns;
        }
        if let Some(rows) = rows {
            *current_rows = rows;
        }
        if let Some(flow) = auto_flow {
            *current_flow = flow;
        }
    }
    if let Some(value) = prop(props, "within")? {
        spec.within = parse_within(Some(&value.extract::<String>()?))?;
    }
    Ok(())
}

/// Keys `Drawable.item` accepts: the item properties plus a layout size
/// set from outside.
pub(crate) const DRAWABLE_ITEM_KEYS: &[&str] = &["width", "height"];

pub(crate) fn apply_item_size(
    item: &mut LayoutItemStyle,
    props: &Bound<'_, PyDict>,
    units: &Units,
) -> PyResult<()> {
    if let Some(value) = prop(props, "width")? {
        item.width = Some(units.size(&value, "width")?);
    }
    if let Some(value) = prop(props, "height")? {
        item.height = Some(units.size(&value, "height")?);
    }
    Ok(())
}

pub(crate) fn apply_item(
    item: &mut LayoutItemStyle,
    props: &Bound<'_, PyDict>,
    units: &Units,
) -> PyResult<()> {
    if let Some(value) = prop(props, "grow")? {
        item.grow = value.extract::<f64>()?;
        finite_non_negative(item.grow, "grow")?;
    }
    if let Some(value) = prop(props, "shrink")? {
        item.shrink = value.extract::<f64>()?;
        finite_non_negative(item.shrink, "shrink")?;
    }
    if let Some(value) = prop(props, "basis")? {
        item.basis = Some(units.non_negative(&value, "basis")?);
    }
    if let Some(value) = prop(props, "align_self")? {
        item.align = Some(parse_align(&value.extract::<String>()?)?);
    }
    if let Some(value) = prop(props, "row")? {
        item.row = Some(value.extract::<usize>()?);
    }
    if let Some(value) = prop(props, "column")? {
        item.column = Some(value.extract::<usize>()?);
    }
    if let Some(value) = prop(props, "row_span")? {
        item.row_span = value.extract::<usize>()?.max(1);
    }
    if let Some(value) = prop(props, "column_span")? {
        item.column_span = value.extract::<usize>()?.max(1);
    }
    if let Some(value) = prop(props, "margin")? {
        item.margin = units.margins(&value)?;
    }
    if let Some(value) = prop(props, "fit")? {
        item.fit = parse_fit(&value.extract::<String>()?)?;
    }
    if let Some(value) = prop(props, "anchor")? {
        item.anchor = parse_anchor(&value)?;
    }
    if let Some(value) = prop(props, "absolute")? {
        item.absolute = value.extract::<bool>()?;
    }
    if let Some(value) = prop(props, "offset")? {
        item.offset = units.point(&value, "offset")?;
    }
    Ok(())
}

/// Fill, border and corner radius drawn behind a box.
#[derive(Clone, Default)]
struct Decoration {
    background: Option<gaanim_core::peniko::Brush>,
    border: Option<gaanim_core::peniko::Brush>,
    border_width: Option<f64>,
    radius: Option<f64>,
}

impl Decoration {
    fn is_empty(&self) -> bool {
        self.background.is_none() && self.border.is_none() && self.radius.is_none()
    }
}

fn parse_decoration(props: &Bound<'_, PyDict>, units: &Units) -> PyResult<Decoration> {
    let paint = |key: &str| -> PyResult<Option<gaanim_core::peniko::Brush>> {
        prop(props, key)?
            .map(|value| {
                value
                    .extract::<crate::brush::PyPaint>()
                    .map(|paint| paint.0)
            })
            .transpose()
    };
    let radius = prop(props, "radius")?
        .map(|value| -> PyResult<f64> {
            if value
                .extract::<String>()
                .is_ok_and(|text| text.trim() == "full")
            {
                // Clamped to half the shorter side when drawn: a pill.
                return Ok(FULL_RADIUS);
            }
            units.non_negative(&value, "radius")
        })
        .transpose()?;
    Ok(Decoration {
        background: paint("background")?,
        border: paint("border")?,
        border_width: prop(props, "border_width")?
            .map(|value| units.non_negative(&value, "border_width"))
            .transpose()?,
        radius,
    })
}

/// Typography keyword arguments for text created from `str` children;
/// lengths (font size, letter spacing) accept the box units.
fn text_kwargs<'py>(
    py: Python<'py>,
    props: &Bound<'py, PyDict>,
    units: &Units,
) -> PyResult<Bound<'py, PyDict>> {
    let kwargs = PyDict::new(py);
    // Interface text is literal: "$3.1k" is money, not math.
    kwargs.set_item("markup", false)?;
    for key in TEXT_KEYS {
        if let Some(value) = prop(props, key)? {
            match *key {
                "font_size" => kwargs.set_item("size", units.non_negative(&value, key)?)?,
                "letter_spacing" => kwargs.set_item(*key, units.length(&value, key)?)?,
                _ => kwargs.set_item(*key, value)?,
            }
        }
    }
    Ok(kwargs)
}

#[derive(Clone)]
pub(crate) struct LayoutMember {
    pub handle: DrawableHandle,
    /// The Python object the author passed (or the text a `str` created),
    /// returned by `children` with its own class.
    object: Arc<Py<PyAny>>,
    child_layout: Option<Arc<Mutex<LayoutState>>>,
}

pub(crate) struct LayoutState {
    canvas: Arc<Mutex<ApiCanvas>>,
    scene: Py<PyAny>,
    spec: LayoutSpec,
    members: Vec<LayoutMember>,
    root: DrawableHandle,
    version: u64,
    parents: Vec<Weak<Mutex<LayoutState>>>,
    background: Option<DrawableHandle>,
    text: Py<PyDict>,
    /// Lowest z-index used by backgrounds in this box's subtree.
    floor_z: i32,
}

/// Boxes by their root drawable, so a child's `item(...)` can reflow the
/// box that holds it.
fn registry() -> &'static Mutex<std::collections::HashMap<u64, Weak<Mutex<LayoutState>>>> {
    static REGISTRY: std::sync::OnceLock<
        Mutex<std::collections::HashMap<u64, Weak<Mutex<LayoutState>>>>,
    > = std::sync::OnceLock::new();
    REGISTRY.get_or_init(Default::default)
}

/// Reflow the box holding `child`, if any, after its item properties
/// change, then advance past the change as `scene.play` does when asked.
pub(crate) fn reflow_owner_and_advance(
    child: &DrawableHandle,
    duration: Option<f64>,
    advance: bool,
) {
    let owner = child.layout_owner().and_then(|owner| {
        registry()
            .lock()
            .expect("box registry poisoned")
            .get(&owner.as_raw())
            .and_then(Weak::upgrade)
    });
    reflow_owner_of(child, duration);
    if let Some(owner) = owner {
        PyBox::finish(&owner, duration, advance);
    }
}

/// Reflow the box holding `child`, if any, after its item properties change.
pub(crate) fn reflow_owner_of(child: &DrawableHandle, duration: Option<f64>) {
    let Some(owner) = child.layout_owner() else {
        return;
    };
    let state = registry()
        .lock()
        .expect("box registry poisoned")
        .get(&owner.as_raw())
        .and_then(Weak::upgrade);
    if let Some(state) = state {
        let same_scene = state
            .lock()
            .expect("layout poisoned")
            .root
            .same_canvas(child);
        if same_scene {
            PyBox::reflow_inner(&state, duration, None, None);
        }
    }
}

/// A reusable set of box properties, like a CSS class.
#[pyclass(name = "BoxStyle", module = "gaanim_core", frozen, skip_from_py_object)]
pub struct PyBoxStyle {
    props: Py<PyDict>,
}

#[pymethods]
impl PyBoxStyle {
    #[new]
    #[pyo3(signature = (**props))]
    fn new(py: Python<'_>, props: Option<&Bound<'_, PyDict>>) -> PyResult<Self> {
        let props = merged_props(py, None, None, props)?;
        Ok(Self {
            props: props.unbind(),
        })
    }

    /// A copy with some properties replaced, like a modifier class.
    #[pyo3(signature = (**props))]
    fn but(&self, py: Python<'_>, props: Option<&Bound<'_, PyDict>>) -> PyResult<Self> {
        let merged = merged_props(py, None, Some(self.props.bind(py).as_any()), props)?;
        Ok(Self {
            props: merged.unbind(),
        })
    }

    /// The properties as a new dict.
    fn to_dict<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyDict>> {
        self.props.bind(py).copy()
    }

    fn __repr__(&self, py: Python<'_>) -> PyResult<String> {
        Ok(format!("BoxStyle({})", self.props.bind(py).repr()?))
    }
}

/// A box: a container that places its children like CSS flexbox or grid,
/// with padding, gap, background, border, radius and shadow. It is a
/// Drawable, so it moves, fades and animates as one object.
#[pyclass(name = "Box", module = "gaanim_core", extends = PyDrawable, skip_from_py_object)]
#[derive(Clone)]
pub struct PyBox {
    inner: Arc<Mutex<LayoutState>>,
}

impl PyBox {
    /// Build a box from Python children and properties.
    pub(crate) fn create<'py>(
        py: Python<'py>,
        canvas: Arc<Mutex<ApiCanvas>>,
        scene: Py<PyAny>,
        kind: LayoutNodeKind,
        defaults: LayoutStyle,
        children: &Bound<'py, PyTuple>,
        style: Option<&Bound<'py, PyAny>>,
        props: Option<&Bound<'py, PyDict>>,
    ) -> PyResult<Py<Self>> {
        let props = merged_props(py, Some(&canvas), style, props)?;
        let units = Units::new(canvas.clone());
        let mut spec = LayoutSpec {
            kind,
            style: defaults,
            within: LayoutWithin::Intrinsic,
        };
        apply_container(&mut spec, &props, &units)?;
        let text = text_kwargs(py, &props, &units)?;
        let members = Self::members(py, &scene, &text, children)?;
        let decoration = parse_decoration(&props, &units)?;

        let refs: Vec<_> = members.iter().map(|member| &member.handle).collect();
        let root = canvas.lock().expect("scene canvas poisoned").group(&refs);
        for member in &members {
            member.handle.claim_layout(&root).map_err(layout_error)?;
        }
        let mut item = LayoutItemStyle::default();
        apply_item(&mut item, &props, &units)?;
        root.set_layout_item(item);

        let inner = Arc::new(Mutex::new(LayoutState {
            canvas,
            scene,
            spec,
            members,
            root: root.clone(),
            version: 0,
            parents: Vec::new(),
            background: None,
            text: text.unbind(),
            floor_z: 0,
        }));
        registry()
            .lock()
            .expect("box registry poisoned")
            .insert(root.id.as_raw(), Arc::downgrade(&inner));
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
        let boxed = Py::new(
            py,
            PyClassInitializer::from(PyDrawable(root)).add_subclass(Self {
                inner: inner.clone(),
            }),
        )?;
        Self::decorate(py, &inner, decoration, &props)?;
        Self::restack(&inner);
        Self::reflow_inner(&inner, None, None, None);
        Ok(boxed)
    }

    fn members<'py>(
        py: Python<'py>,
        scene: &Py<PyAny>,
        text: &Bound<'py, PyDict>,
        children: &Bound<'py, PyTuple>,
    ) -> PyResult<Vec<LayoutMember>> {
        let mut members = Vec::new();
        for child in children.iter() {
            Self::collect(py, scene, text, &child, &mut members)?;
        }
        Ok(members)
    }

    fn collect<'py>(
        py: Python<'py>,
        scene: &Py<PyAny>,
        text: &Bound<'py, PyDict>,
        child: &Bound<'py, PyAny>,
        members: &mut Vec<LayoutMember>,
    ) -> PyResult<()> {
        if child.is_none() {
            return Ok(());
        }
        if (child.is_instance_of::<pyo3::types::PyList>() || child.is_instance_of::<PyTuple>())
            && !child.is_instance_of::<PyString>()
        {
            for item in child.try_iter()? {
                Self::collect(py, scene, text, &item?, members)?;
            }
            return Ok(());
        }
        members.push(Self::member(py, scene, text, child)?);
        Ok(())
    }

    fn member<'py>(
        py: Python<'py>,
        scene: &Py<PyAny>,
        text: &Bound<'py, PyDict>,
        child: &Bound<'py, PyAny>,
    ) -> PyResult<LayoutMember> {
        let object = if let Ok(content) = child.cast::<PyString>() {
            let markup = text
                .get_item("markup")?
                .is_some_and(|value| value.is_truthy().unwrap_or(false));
            let content = content.to_str()?;
            // Literal interface text: a `$` is a dollar sign, not math.
            let content = if markup {
                content.to_owned()
            } else {
                content
                    .replace("\\$", "\u{0}")
                    .replace('$', "\\$")
                    .replace('\u{0}', "\\$")
            };
            scene
                .bind(py)
                .getattr("text")?
                .call((content,), Some(text))?
        } else {
            child.clone()
        };
        if let Ok(boxed) = object.extract::<PyRef<'_, PyBox>>() {
            let handle = boxed.inner.lock().expect("layout poisoned").root.clone();
            return Ok(LayoutMember {
                handle,
                object: Arc::new(object.clone().unbind()),
                child_layout: Some(boxed.inner.clone()),
            });
        }
        if let Ok(drawable) = object.extract::<PyRef<'_, PyDrawable>>() {
            return Ok(LayoutMember {
                handle: drawable.0.clone(),
                object: Arc::new(object.clone().unbind()),
                child_layout: None,
            });
        }
        Err(pyo3::exceptions::PyTypeError::new_err(
            "box children must be Drawables, boxes, strings or lists of them",
        ))
    }

    /// Add or update the background: fill, border, radius, shadow and clip.
    fn decorate(
        py: Python<'_>,
        inner: &Arc<Mutex<LayoutState>>,
        decoration: Decoration,
        props: &Bound<'_, PyDict>,
    ) -> PyResult<()> {
        let shadow = prop(props, "shadow")?;
        let clip = prop(props, "clip")?
            .map(|value| value.extract::<bool>())
            .transpose()?;
        if decoration.is_empty() && shadow.is_none() && clip.is_none() {
            return Ok(());
        }
        let (canvas, root, existing) = {
            let state = inner.lock().expect("layout poisoned");
            (
                state.canvas.clone(),
                state.root.clone(),
                state.background.clone(),
            )
        };
        let background = match existing {
            Some(background) => {
                if decoration.radius.is_some() {
                    return Err(pyo3::exceptions::PyValueError::new_err(
                        "a box's radius is fixed when it is created",
                    ));
                }
                let mut background = background;
                if let Some(fill) = decoration.background {
                    background = background.fill_brush(fill);
                }
                match (decoration.border, decoration.border_width) {
                    (Some(paint), width) => {
                        background = background.stroke_with_style(
                            paint,
                            gaanim_core::kurbo::Stroke::new(width.unwrap_or(DEFAULT_BORDER_WIDTH)),
                        );
                    }
                    (None, Some(_)) => {
                        return Err(pyo3::exceptions::PyValueError::new_err(
                            "set border together with border_width",
                        ));
                    }
                    (None, None) => {}
                }
                background
            }
            None => {
                let border = decoration.border.map(|paint| {
                    (
                        paint,
                        decoration.border_width.unwrap_or(DEFAULT_BORDER_WIDTH),
                    )
                });
                let background = canvas
                    .lock()
                    .expect("scene canvas poisoned")
                    .decorate_layout(
                        &root,
                        decoration.background,
                        border,
                        decoration.radius.unwrap_or(0.0),
                    )
                    .map_err(pyo3::exceptions::PyValueError::new_err)?;
                inner.lock().expect("layout poisoned").background = Some(background.clone());
                background
            }
        };
        let background_object = Py::new(py, PyDrawable(background.clone()))?.into_any();
        if let Some(shadow) = shadow {
            let bound = background_object.bind(py);
            if shadow.extract::<bool>().is_ok_and(|enabled| enabled) {
                bound.call_method1("shadow", (DEFAULT_SHADOW_COLOR,))?;
            } else if let Ok(settings) = shadow.cast::<PyDict>() {
                let color = settings
                    .get_item("color")?
                    .map(|color| color.unbind())
                    .unwrap_or_else(|| PyString::new(py, DEFAULT_SHADOW_COLOR).into_any().unbind());
                let kwargs = PyDict::new(py);
                let units = Units::new(canvas.clone());
                for key in ["x", "y", "blur"] {
                    if let Some(value) = settings.get_item(key)? {
                        kwargs.set_item(key, units.length(&value, key)?)?;
                    }
                }
                bound.call_method("shadow", (color,), Some(&kwargs))?;
            } else if !shadow.extract::<bool>().is_ok_and(|enabled| !enabled) {
                return Err(pyo3::exceptions::PyTypeError::new_err(
                    "shadow must be True, False or a dict with color, x, y and blur",
                ));
            }
        }
        if clip == Some(true) {
            let root_object = Py::new(py, PyDrawable(root))?.into_any();
            root_object
                .bind(py)
                .call_method1("clip", (background_object,))?;
        }
        Ok(())
    }

    /// Draw this box's background beneath the backgrounds of the boxes it
    /// contains, however they were created, and repeat for its parents.
    fn restack(inner: &Arc<Mutex<LayoutState>>) {
        let (children, background, parents) = {
            let state = inner.lock().expect("layout poisoned");
            (
                state
                    .members
                    .iter()
                    .filter_map(|member| member.child_layout.clone())
                    .collect::<Vec<_>>(),
                state.background.clone(),
                state.parents.clone(),
            )
        };
        let floor = children
            .iter()
            .map(|child| child.lock().expect("layout poisoned").floor_z)
            .min()
            .unwrap_or(0)
            .min(0);
        let own = match background {
            Some(background) => {
                background.z_index(floor - 1);
                floor - 1
            }
            None => floor,
        };
        inner.lock().expect("layout poisoned").floor_z = own;
        for parent in parents.into_iter().filter_map(|parent| parent.upgrade()) {
            Self::restack(&parent);
        }
    }

    /// Advance the scene past an animated change, as `scene.play` does,
    /// unless `advance` is false.
    fn finish(inner: &Arc<Mutex<LayoutState>>, duration: Option<f64>, advance: bool) {
        if let (Some(duration), true) = (duration, advance) {
            let canvas = inner.lock().expect("layout poisoned").canvas.clone();
            canvas.lock().expect("scene canvas poisoned").wait(duration);
        }
    }

    /// Record this box's new snapshot and its ancestors', then resolve the
    /// whole tree once from the outermost box: one transition animates every
    /// child, however deep the change was.
    pub(crate) fn reflow_inner(
        inner: &Arc<Mutex<LayoutState>>,
        duration: Option<f64>,
        entering: Option<DrawableHandle>,
        leaving: Option<DrawableHandle>,
    ) {
        let mut chain = vec![inner.clone()];
        let mut index = 0;
        while index < chain.len() {
            let parents = chain[index]
                .lock()
                .expect("layout poisoned")
                .parents
                .clone();
            for parent in parents.into_iter().filter_map(|parent| parent.upgrade()) {
                if !chain.iter().any(|seen| Arc::ptr_eq(seen, &parent)) {
                    chain.push(parent);
                }
            }
            index += 1;
        }
        let last = chain.len() - 1;
        for (position, state) in chain.iter().enumerate() {
            let (canvas, spec, root, members, version) = {
                let mut state = state.lock().expect("layout poisoned");
                state.version = state.version.saturating_add(1);
                (
                    state.canvas.clone(),
                    state.spec.clone(),
                    state.root.clone(),
                    state.members.clone(),
                    state.version,
                )
            };
            let refs: Vec<_> = members.iter().map(|member| &member.handle).collect();
            let snapshots = members
                .iter()
                .map(|member| LayoutMemberSpec {
                    id: member.handle.id,
                    style: member.handle.layout_item(),
                })
                .collect();
            let mut canvas = canvas.lock().expect("scene canvas poisoned");
            canvas.set_group_members(&root, &refs);
            if position == last {
                canvas.reflow_layout(
                    &root,
                    snapshots,
                    spec,
                    version,
                    duration,
                    entering.as_ref(),
                    leaving.as_ref(),
                );
            } else {
                canvas.record_layout(&root, snapshots, spec, version);
            }
        }
    }

    fn position(&self, child: &Bound<'_, PyAny>) -> PyResult<usize> {
        let handle = if let Ok(boxed) = child.extract::<PyRef<'_, PyBox>>() {
            boxed.inner.lock().expect("layout poisoned").root.clone()
        } else {
            child
                .extract::<PyRef<'_, PyDrawable>>()
                .map_err(|_| pyo3::exceptions::PyTypeError::new_err("expected a Drawable"))?
                .0
                .clone()
        };
        self.inner
            .lock()
            .expect("layout poisoned")
            .members
            .iter()
            .position(|member| member.handle.id == handle.id)
            .ok_or_else(|| {
                pyo3::exceptions::PyValueError::new_err("the object is not a child of this box")
            })
    }

    fn adopt(&self, member: &LayoutMember) -> PyResult<()> {
        let state = self.inner.lock().expect("layout poisoned");
        if member.handle.id == state.root.id {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "a box cannot contain itself",
            ));
        }
        member
            .handle
            .claim_layout(&state.root)
            .map_err(layout_error)?;
        if let Some(child) = &member.child_layout {
            child
                .lock()
                .expect("layout poisoned")
                .parents
                .push(Arc::downgrade(&self.inner));
        }
        Ok(())
    }

    /// Forget this box as the parent of a child box that leaves it, so the
    /// child's later changes no longer reflow this box.
    fn orphan(&self, member: &LayoutMember) {
        if let Some(child) = &member.child_layout {
            child
                .lock()
                .expect("layout poisoned")
                .parents
                .retain(|parent| !std::ptr::eq(parent.as_ptr(), Arc::as_ptr(&self.inner)));
        }
    }

    fn restacked(&self) {
        Self::restack(&self.inner);
    }
}

impl PyBox {
    fn handle(&self) -> DrawableHandle {
        self.inner.lock().expect("layout poisoned").root.clone()
    }
}

// Fluent setters inherited from Drawable, returning this Box so chains keep
// its methods (`box.move_to(0, 0).set(gap=0.2)`).
#[pymethods]
impl PyBox {
    fn opacity<'py>(slf: PyRef<'py, Self>, op: &Bound<'_, PyAny>) -> PyResult<PyRef<'py, Self>> {
        crate::pydrawable::PyDrawable(slf.handle()).opacity(op)?;
        Ok(slf)
    }

    fn z_index<'py>(slf: PyRef<'py, Self>, z: i32) -> PyResult<PyRef<'py, Self>> {
        crate::pydrawable::PyDrawable(slf.handle()).z_index(z)?;
        Ok(slf)
    }
    #[pyo3(signature = (x, y=None, anchor=None))]
    fn move_to<'py>(
        slf: PyRef<'py, Self>,
        x: &Bound<'_, PyAny>,
        y: Option<&Bound<'_, PyAny>>,
        anchor: Option<&PyAnchor>,
    ) -> PyResult<PyRef<'py, Self>> {
        crate::pydrawable::PyDrawable(slf.handle()).move_to(x, y, anchor)?;
        Ok(slf)
    }

    fn move_to_3d<'py>(
        slf: PyRef<'py, Self>,
        x: &Bound<'_, PyAny>,
        y: &Bound<'_, PyAny>,
        z: &Bound<'_, PyAny>,
    ) -> PyResult<PyRef<'py, Self>> {
        crate::pydrawable::PyDrawable(slf.handle()).move_to_3d(x, y, z)?;
        Ok(slf)
    }

    fn shift_by<'py>(slf: PyRef<'py, Self>, dx: f64, dy: f64) -> PyResult<PyRef<'py, Self>> {
        crate::pydrawable::PyDrawable(slf.handle()).shift_by(dx, dy)?;
        Ok(slf)
    }

    fn shift_by_3d<'py>(
        slf: PyRef<'py, Self>,
        dx: f64,
        dy: f64,
        dz: f64,
    ) -> PyResult<PyRef<'py, Self>> {
        crate::pydrawable::PyDrawable(slf.handle()).shift_by_3d(dx, dy, dz)?;
        Ok(slf)
    }

    fn billboard<'py>(slf: PyRef<'py, Self>) -> PyResult<PyRef<'py, Self>> {
        crate::pydrawable::PyDrawable(slf.handle()).billboard()?;
        Ok(slf)
    }

    fn hud<'py>(slf: PyRef<'py, Self>) -> PyResult<PyRef<'py, Self>> {
        crate::pydrawable::PyDrawable(slf.handle()).hud()?;
        Ok(slf)
    }

    fn scale_to<'py>(
        slf: PyRef<'py, Self>,
        factor: &Bound<'_, PyAny>,
    ) -> PyResult<PyRef<'py, Self>> {
        crate::pydrawable::PyDrawable(slf.handle()).scale_to(factor)?;
        Ok(slf)
    }

    fn scale_to_3d<'py>(
        slf: PyRef<'py, Self>,
        x: &Bound<'_, PyAny>,
        y: &Bound<'_, PyAny>,
        z: &Bound<'_, PyAny>,
    ) -> PyResult<PyRef<'py, Self>> {
        crate::pydrawable::PyDrawable(slf.handle()).scale_to_3d(x, y, z)?;
        Ok(slf)
    }

    fn scale_by<'py>(slf: PyRef<'py, Self>, factor: f64) -> PyResult<PyRef<'py, Self>> {
        crate::pydrawable::PyDrawable(slf.handle()).scale_by(factor)?;
        Ok(slf)
    }

    fn scale_by_3d<'py>(
        slf: PyRef<'py, Self>,
        x: f64,
        y: f64,
        z: f64,
    ) -> PyResult<PyRef<'py, Self>> {
        crate::pydrawable::PyDrawable(slf.handle()).scale_by_3d(x, y, z)?;
        Ok(slf)
    }

    fn rotate_to<'py>(
        slf: PyRef<'py, Self>,
        radians: &Bound<'_, PyAny>,
    ) -> PyResult<PyRef<'py, Self>> {
        crate::pydrawable::PyDrawable(slf.handle()).rotate_to(radians)?;
        Ok(slf)
    }

    fn rotate_to_3d<'py>(
        slf: PyRef<'py, Self>,
        x: &Bound<'_, PyAny>,
        y: &Bound<'_, PyAny>,
        z: &Bound<'_, PyAny>,
    ) -> PyResult<PyRef<'py, Self>> {
        crate::pydrawable::PyDrawable(slf.handle()).rotate_to_3d(x, y, z)?;
        Ok(slf)
    }

    fn rotate_by<'py>(slf: PyRef<'py, Self>, radians: f64) -> PyResult<PyRef<'py, Self>> {
        crate::pydrawable::PyDrawable(slf.handle()).rotate_by(radians)?;
        Ok(slf)
    }

    fn skew_to<'py>(slf: PyRef<'py, Self>, x: f64, y: f64) -> PyResult<PyRef<'py, Self>> {
        crate::pydrawable::PyDrawable(slf.handle()).skew_to(x, y)?;
        Ok(slf)
    }

    fn matrix_to<'py>(
        slf: PyRef<'py, Self>,
        matrix: ((f64, f64), (f64, f64)),
    ) -> PyResult<PyRef<'py, Self>> {
        crate::pydrawable::PyDrawable(slf.handle()).matrix_to(matrix)?;
        Ok(slf)
    }

    fn rotate_by_3d<'py>(
        slf: PyRef<'py, Self>,
        axis: &str,
        radians: f64,
    ) -> PyResult<PyRef<'py, Self>> {
        crate::pydrawable::PyDrawable(slf.handle()).rotate_by_3d(axis, radians)?;
        Ok(slf)
    }

    fn with_pivot<'py>(slf: PyRef<'py, Self>, x: f64, y: f64) -> PyResult<PyRef<'py, Self>> {
        crate::pydrawable::PyDrawable(slf.handle()).with_pivot(x, y)?;
        Ok(slf)
    }

    fn with_pivot_3d<'py>(
        slf: PyRef<'py, Self>,
        x: f64,
        y: f64,
        z: f64,
    ) -> PyResult<PyRef<'py, Self>> {
        crate::pydrawable::PyDrawable(slf.handle()).with_pivot_3d(x, y, z)?;
        Ok(slf)
    }

    fn pivot<'py>(slf: PyRef<'py, Self>, x: f64, y: f64) -> PyResult<PyRef<'py, Self>> {
        crate::pydrawable::PyDrawable(slf.handle()).pivot(x, y)?;
        Ok(slf)
    }
    #[pyo3(signature = (reference, direction, spacing=0.24, aligned_edge=None))]
    fn next_to<'py>(
        slf: PyRef<'py, Self>,
        reference: &PyDrawable,
        direction: &PyDirection,
        spacing: f64,
        aligned_edge: Option<&PyAnchor>,
    ) -> PyResult<PyRef<'py, Self>> {
        crate::pydrawable::PyDrawable(slf.handle()).next_to(
            reference,
            direction,
            spacing,
            aligned_edge,
        )?;
        Ok(slf)
    }
    #[pyo3(signature = (reference, target_anchor, reference_anchor=None))]
    fn align_to<'py>(
        slf: PyRef<'py, Self>,
        reference: &PyDrawable,
        target_anchor: &PyAnchor,
        reference_anchor: Option<&PyAnchor>,
    ) -> PyResult<PyRef<'py, Self>> {
        crate::pydrawable::PyDrawable(slf.handle()).align_to(
            reference,
            target_anchor,
            reference_anchor,
        )?;
        Ok(slf)
    }
    #[pyo3(signature = (direction, buff=0.24))]
    fn to_edge<'py>(
        slf: PyRef<'py, Self>,
        direction: &PyDirection,
        buff: f64,
    ) -> PyResult<PyRef<'py, Self>> {
        crate::pydrawable::PyDrawable(slf.handle()).to_edge(direction, buff)?;
        Ok(slf)
    }
    #[pyo3(signature = (corner, buff=0.24))]
    fn to_corner<'py>(
        slf: PyRef<'py, Self>,
        corner: &PyAnchor,
        buff: f64,
    ) -> PyResult<PyRef<'py, Self>> {
        crate::pydrawable::PyDrawable(slf.handle()).to_corner(corner, buff)?;
        Ok(slf)
    }
}

const DEFAULT_BORDER_WIDTH: f64 = 0.025;
/// `radius="full"`: the background clamps it to half the shorter side.
const FULL_RADIUS: f64 = 1.0e9;
const DEFAULT_SHADOW_COLOR: &str = "#00000055";

fn duration_value(duration: Option<f64>) -> PyResult<Option<f64>> {
    match duration {
        Some(value) if !value.is_finite() || value < 0.0 => Err(
            pyo3::exceptions::PyValueError::new_err("duration must be finite and non-negative"),
        ),
        Some(value) if value == 0.0 => Ok(None),
        other => Ok(other),
    }
}

#[pymethods]
impl PyBox {
    /// The children in order, as the objects that were passed in.
    #[getter]
    fn children(&self, py: Python<'_>) -> PyResult<Vec<Py<PyAny>>> {
        crate::custom::ensure_authoring_allowed()?;
        Ok(self
            .inner
            .lock()
            .expect("layout poisoned")
            .members
            .iter()
            .map(|member| member.object.clone_ref(py))
            .collect())
    }

    fn __len__(&self) -> usize {
        self.inner.lock().expect("layout poisoned").members.len()
    }

    fn __getitem__(&self, py: Python<'_>, index: isize) -> PyResult<Py<PyAny>> {
        let state = self.inner.lock().expect("layout poisoned");
        let len = state.members.len() as isize;
        let index = if index < 0 { len + index } else { index };
        state
            .members
            .get(usize::try_from(index).unwrap_or(usize::MAX))
            .map(|member| member.object.clone_ref(py))
            .ok_or_else(|| pyo3::exceptions::PyIndexError::new_err("box child index out of range"))
    }

    fn __iter__(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        let children = self.children(py)?;
        Ok(pyo3::types::PyList::new(py, children)?
            .as_any()
            .try_iter()?
            .into_any()
            .unbind())
    }

    /// The drawable behind the box (fill, border, radius), if any.
    #[getter]
    fn background(&self) -> PyResult<Option<PyDrawable>> {
        crate::custom::ensure_authoring_allowed()?;
        Ok(self
            .inner
            .lock()
            .expect("layout poisoned")
            .background
            .clone()
            .map(PyDrawable))
    }

    /// Insert a child (a Drawable, box or string) at `at`, or at the end.
    /// With `duration`, the others slide to make room and it fades in.
    #[pyo3(signature = (child, *, at=None, duration=None, advance=true))]
    fn add(
        &self,
        py: Python<'_>,
        child: &Bound<'_, PyAny>,
        at: Option<usize>,
        duration: Option<f64>,
        advance: bool,
    ) -> PyResult<Py<PyAny>> {
        crate::custom::ensure_authoring_allowed()?;
        let duration = duration_value(duration)?;
        let (scene, text) = {
            let state = self.inner.lock().expect("layout poisoned");
            (state.scene.clone_ref(py), state.text.clone_ref(py))
        };
        let member = Self::member(py, &scene, text.bind(py), child)?;
        self.adopt(&member)?;
        let (handle, object) = (member.handle.clone(), member.object.clone_ref(py));
        {
            let mut state = self.inner.lock().expect("layout poisoned");
            let index = at.unwrap_or(state.members.len());
            if index > state.members.len() {
                handle.release_layout(&state.root);
                self.orphan(&member);
                return Err(pyo3::exceptions::PyIndexError::new_err(
                    "box insertion index is out of range",
                ));
            }
            state.members.insert(index, member);
        }
        self.restacked();
        Self::reflow_inner(&self.inner, duration, Some(handle), None);
        Self::finish(&self.inner, duration, advance);
        Ok(object)
    }

    /// Remove a child: it fades out while the others close the gap.
    #[pyo3(signature = (child, *, duration=None, advance=true))]
    fn remove(
        &self,
        child: &Bound<'_, PyAny>,
        duration: Option<f64>,
        advance: bool,
    ) -> PyResult<()> {
        crate::custom::ensure_authoring_allowed()?;
        let duration = duration_value(duration)?;
        let index = self.position(child)?;
        let removed = {
            let mut state = self.inner.lock().expect("layout poisoned");
            let removed = state.members.remove(index);
            removed.handle.release_layout(&state.root);
            removed
        };
        self.orphan(&removed);
        Self::reflow_inner(&self.inner, duration, None, Some(removed.handle));
        Self::finish(&self.inner, duration, advance);
        Ok(())
    }

    /// Take a child out of the box without hiding it: it stays where it is
    /// and can be moved freely again.
    #[pyo3(signature = (child, *, duration=None, advance=true))]
    fn detach(
        &self,
        child: &Bound<'_, PyAny>,
        duration: Option<f64>,
        advance: bool,
    ) -> PyResult<()> {
        crate::custom::ensure_authoring_allowed()?;
        let duration = duration_value(duration)?;
        let index = self.position(child)?;
        let detached = {
            let mut state = self.inner.lock().expect("layout poisoned");
            let detached = state.members.remove(index);
            detached.handle.release_layout(&state.root);
            detached
        };
        self.orphan(&detached);
        Self::reflow_inner(&self.inner, duration, None, None);
        Self::finish(&self.inner, duration, advance);
        Ok(())
    }

    /// Put `new` where `old` was.
    #[pyo3(signature = (old, new, *, duration=None, advance=true))]
    fn replace(
        &self,
        py: Python<'_>,
        old: &Bound<'_, PyAny>,
        new: &Bound<'_, PyAny>,
        duration: Option<f64>,
        advance: bool,
    ) -> PyResult<Py<PyAny>> {
        crate::custom::ensure_authoring_allowed()?;
        let duration = duration_value(duration)?;
        let index = self.position(old)?;
        let (scene, text) = {
            let state = self.inner.lock().expect("layout poisoned");
            (state.scene.clone_ref(py), state.text.clone_ref(py))
        };
        let member = Self::member(py, &scene, text.bind(py), new)?;
        self.adopt(&member)?;
        let (handle, object) = (member.handle.clone(), member.object.clone_ref(py));
        // The newcomer takes the old child's place: its cell, grow, margin…
        if handle.explicit_layout_item().is_none() {
            let previous = self.inner.lock().expect("layout poisoned").members[index]
                .handle
                .layout_item();
            handle.set_layout_item(previous);
        }
        let old = {
            let mut state = self.inner.lock().expect("layout poisoned");
            let old = std::mem::replace(&mut state.members[index], member);
            old.handle.release_layout(&state.root);
            old
        };
        self.orphan(&old);
        let old = old.handle;
        self.restacked();
        Self::reflow_inner(&self.inner, duration, Some(handle), Some(old));
        Self::finish(&self.inner, duration, advance);
        Ok(object)
    }

    /// Change box properties (the same names as when creating it). With
    /// `duration`, the children move to their new places over that time.
    #[pyo3(signature = (*, duration=None, advance=true, style=None, **props))]
    fn set<'py>(
        slf: PyRef<'py, Self>,
        py: Python<'py>,
        duration: Option<f64>,
        advance: bool,
        style: Option<&Bound<'py, PyAny>>,
        props: Option<&Bound<'py, PyDict>>,
    ) -> PyResult<PyRef<'py, Self>> {
        crate::custom::ensure_authoring_allowed()?;
        let duration = duration_value(duration)?;
        let canvas = slf.inner.lock().expect("layout poisoned").canvas.clone();
        let props = merged_props(py, Some(&canvas), style, props)?;
        let (canvas, root) = {
            let state = slf.inner.lock().expect("layout poisoned");
            (state.canvas.clone(), state.root.clone())
        };
        let units = Units::new(canvas);
        {
            let mut state = slf.inner.lock().expect("layout poisoned");
            apply_container(&mut state.spec, &props, &units)?;
            let text = state.text.bind(py).clone();
            text.update(text_kwargs(py, &props, &units)?.as_mapping())?;
        }
        let mut item = root.layout_item();
        apply_item(&mut item, &props, &units)?;
        root.set_layout_item(item);
        Self::decorate(py, &slf.inner, parse_decoration(&props, &units)?, &props)?;
        Self::restack(&slf.inner);
        // The chain up to the outermost box re-reads this box's item
        // properties, so one transition covers both kinds of change.
        Self::reflow_inner(&slf.inner, duration, None, None);
        Self::finish(&slf.inner, duration, advance);
        Ok(slf)
    }

    /// Recompute the box, for example after a child changed size.
    #[pyo3(signature = (*, duration=None, advance=true))]
    fn reflow(&self, duration: Option<f64>, advance: bool) -> PyResult<()> {
        crate::custom::ensure_authoring_allowed()?;
        let duration = duration_value(duration)?;
        Self::reflow_inner(&self.inner, duration, None, None);
        Self::finish(&self.inner, duration, advance);
        Ok(())
    }

    /// Layout problems found while resolving this box.
    fn diagnostics(&self) -> PyResult<Vec<String>> {
        crate::custom::ensure_authoring_allowed()?;
        let (canvas, root) = {
            let state = self.inner.lock().expect("layout poisoned");
            (state.canvas.clone(), state.root.clone())
        };
        Ok(canvas
            .lock()
            .expect("scene canvas poisoned")
            .layout_diagnostics(&root))
    }

    fn __repr__(&self) -> String {
        let state = self.inner.lock().expect("layout poisoned");
        let kind = match state.spec.kind {
            LayoutNodeKind::Row { .. } => "row",
            LayoutNodeKind::Column { .. } => "column",
            LayoutNodeKind::Grid { .. } => "grid",
            LayoutNodeKind::Stack => "stack",
            LayoutNodeKind::Leaf => "leaf",
        };
        format!("Box({kind}, {} children)", state.members.len())
    }
}

pub(crate) fn parse_align(value: &str) -> PyResult<Align> {
    match value {
        "start" => Ok(Align::Start),
        "center" => Ok(Align::Center),
        "end" => Ok(Align::End),
        "stretch" => Ok(Align::Stretch),
        "baseline" => Ok(Align::Baseline),
        _ => Err(pyo3::exceptions::PyValueError::new_err(
            "align must be 'start', 'center', 'end', 'stretch' or 'baseline'",
        )),
    }
}

pub(crate) fn parse_justify(value: &str) -> PyResult<Justify> {
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

pub(crate) fn parse_fit(value: &str) -> PyResult<FitMode> {
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
