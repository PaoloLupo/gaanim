//! Unified structured text bindings.

use pyo3::exceptions::{PyIndexError, PyKeyError, PyTypeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyAny, PyDict, PySlice, PyTuple};

use gaanim_text::prelude::{
    flatten_content, TextAlign, TextAnchor, TextContent, TextDirection, TextFlow, TextOverflow,
    TextPart, TextRole, TextSpec, TextStyle, TextWrap,
};

use crate::brush::PyPaint;
use crate::color::PyColor;
use crate::pydrawable::{resolve_at_target, PyAtTarget, PyCanvasAnim, PyDrawable};
use crate::pylayout::{PyAnchor, PyDirection};

#[pyclass(name = "TextAnchor", module = "gaanim_core", frozen, from_py_object)]
#[derive(Clone, Copy, Debug)]
pub struct PyTextAnchor(pub TextAnchor);

#[pymethods]
#[allow(non_snake_case)]
impl PyTextAnchor {
    #[classattr]
    fn BASELINE_LEFT() -> Self {
        Self(TextAnchor::BaselineLeft)
    }

    #[classattr]
    fn BASELINE_CENTER() -> Self {
        Self(TextAnchor::BaselineCenter)
    }

    #[classattr]
    fn BASELINE_RIGHT() -> Self {
        Self(TextAnchor::BaselineRight)
    }
}

enum ResolvedTextAnchor {
    Geometric(gaanim_api::canvas::Anchor),
    Typographic(TextAnchor),
}

fn resolve_text_anchor(value: &Bound<'_, PyAny>) -> PyResult<ResolvedTextAnchor> {
    if let Ok(anchor) = value.extract::<PyRef<'_, PyAnchor>>() {
        return Ok(ResolvedTextAnchor::Geometric(anchor.0));
    }
    if let Ok(anchor) = value.extract::<PyRef<'_, PyTextAnchor>>() {
        return Ok(ResolvedTextAnchor::Typographic(anchor.0));
    }
    Err(PyTypeError::new_err(
        "anchor must be an Anchor or TextAnchor",
    ))
}

pub(crate) fn parse_role(value: Option<&str>) -> PyResult<Option<TextRole>> {
    value
        .map(|value| match value {
            "title" => Ok(TextRole::Title),
            "subtitle" => Ok(TextRole::Subtitle),
            "kicker" => Ok(TextRole::Kicker),
            "heading" => Ok(TextRole::Heading),
            "body" => Ok(TextRole::Body),
            "caption" => Ok(TextRole::Caption),
            "label" => Ok(TextRole::Label),
            "code" => Ok(TextRole::Code),
            "math" => Ok(TextRole::Math),
            _ => Err(PyValueError::new_err(
                "role must be title, subtitle, kicker, heading, body, caption, label, code, or math",
            )),
        })
        .transpose()
}

fn parse_wrap(value: &Bound<'_, PyAny>) -> PyResult<TextWrap> {
    if let Ok(value) = value.extract::<bool>() {
        return if value {
            Err(PyValueError::new_err("wrap=True is ambiguous; use 'auto'"))
        } else {
            Ok(TextWrap::NoWrap)
        };
    }
    if let Ok(value) = value.extract::<String>() {
        return if value == "auto" {
            Ok(TextWrap::Auto)
        } else {
            Err(PyValueError::new_err(
                "wrap must be 'auto', False, or a positive number",
            ))
        };
    }
    if let Ok(value) = value.extract::<f64>() {
        if value.is_finite() && value > 0.0 {
            return Ok(TextWrap::Width(value));
        }
    }
    Err(PyValueError::new_err(
        "wrap must be 'auto', False, or a finite positive number",
    ))
}

fn parse_align(value: &str) -> PyResult<TextAlign> {
    match value {
        "left" => Ok(TextAlign::Left),
        "center" => Ok(TextAlign::Center),
        "right" => Ok(TextAlign::Right),
        "justify" => Ok(TextAlign::Justify),
        _ => Err(PyValueError::new_err(
            "align must be 'left', 'center', 'right', or 'justify'",
        )),
    }
}

fn parse_overflow(value: &str) -> PyResult<TextOverflow> {
    match value {
        "visible" => Ok(TextOverflow::Visible),
        "clip" => Ok(TextOverflow::Clip),
        "ellipsis" => Ok(TextOverflow::Ellipsis),
        _ => Err(PyValueError::new_err(
            "overflow must be 'visible', 'clip', or 'ellipsis'",
        )),
    }
}

fn parse_direction(value: &str) -> PyResult<TextDirection> {
    match value {
        "auto" => Ok(TextDirection::Auto),
        "ltr" => Ok(TextDirection::Ltr),
        "rtl" => Ok(TextDirection::Rtl),
        _ => Err(PyValueError::new_err(
            "direction must be 'auto', 'ltr', or 'rtl'",
        )),
    }
}

#[pyclass(name = "TextStyle", module = "gaanim_core", frozen, from_py_object)]
#[derive(Clone)]
pub struct PyTextStyle(pub TextStyle);

#[pymethods]
impl PyTextStyle {
    #[new]
    #[pyo3(signature = (*, font=None, math_font=None, fallbacks=Vec::new(), size=None, weight=None, italic=None, color=None, stroke=None, stroke_width=None, opacity=None, letter_spacing=None, word_spacing=None, decorations=Vec::new(), baseline=None))]
    #[allow(clippy::too_many_arguments)]
    fn new(
        font: Option<String>,
        math_font: Option<String>,
        fallbacks: Vec<String>,
        size: Option<f64>,
        weight: Option<u16>,
        italic: Option<bool>,
        color: Option<PyColor>,
        stroke: Option<PyColor>,
        stroke_width: Option<f64>,
        opacity: Option<f32>,
        letter_spacing: Option<f64>,
        word_spacing: Option<f64>,
        decorations: Vec<String>,
        baseline: Option<f64>,
    ) -> PyResult<Self> {
        let style = TextStyle {
            font,
            math_font,
            fallbacks,
            size,
            weight,
            italic,
            color: color.map(|color| color.0),
            stroke_color: stroke.map(|color| color.0),
            stroke_width,
            opacity,
            letter_spacing,
            word_spacing,
            decorations,
            baseline,
        };
        // Reuse TextSpec's public validation so constructors and scene.text
        // report identical errors.
        TextSpec::new(
            vec!["x".into()],
            Some(TextRole::Body),
            style.clone(),
            TextFlow::default(),
        )
        .map_err(|error| PyValueError::new_err(error.to_string()))?;
        Ok(Self(style))
    }
}

#[pyclass(name = "TextFlow", module = "gaanim_core", frozen, from_py_object)]
#[derive(Clone)]
pub struct PyTextFlow(pub TextFlow);

#[pymethods]
impl PyTextFlow {
    #[new]
    #[pyo3(signature = (*, wrap=None, align="left", line_spacing=1.2, max_lines=None, overflow="clip", direction="auto", hyphenate=false))]
    fn new(
        wrap: Option<&Bound<'_, PyAny>>,
        align: &str,
        line_spacing: f64,
        max_lines: Option<usize>,
        overflow: &str,
        direction: &str,
        hyphenate: bool,
    ) -> PyResult<Self> {
        let flow = TextFlow {
            wrap: wrap.map(parse_wrap).transpose()?.unwrap_or(TextWrap::Auto),
            align: parse_align(align)?,
            line_spacing,
            max_lines,
            overflow: parse_overflow(overflow)?,
            direction: parse_direction(direction)?,
            hyphenate,
        };
        TextSpec::new(
            vec!["x".into()],
            Some(TextRole::Body),
            TextStyle::default(),
            flow.clone(),
        )
        .map_err(|error| PyValueError::new_err(error.to_string()))?;
        Ok(Self(flow))
    }
}

#[pyclass(name = "TextPart", module = "gaanim_core", frozen, from_py_object)]
#[derive(Clone)]
pub struct PyTextPart(pub TextPart);

#[pyclass(name = "TextParts", module = "gaanim_core", frozen, from_py_object)]
#[derive(Clone)]
/// Ordered plain semantic parts produced by [`text_parts`].
pub struct PyTextParts(pub Vec<TextPart>);

fn content_from_tuple(content: &Bound<'_, PyTuple>) -> PyResult<Vec<TextContent>> {
    content.iter().try_fold(Vec::new(), |mut content, value| {
        if let Ok(text) = value.extract::<String>() {
            content.push(TextContent::Literal(text));
        } else if let Ok(part) = value.extract::<PyRef<'_, PyTextPart>>() {
            content.push(TextContent::Part(part.0.clone()));
        } else if let Ok(parts) = value.extract::<PyRef<'_, PyTextParts>>() {
            content.extend(parts.0.iter().cloned().map(TextContent::Part));
        } else {
            return Err(PyTypeError::new_err(
                "text content must contain only str, TextPart, or TextParts values",
            ));
        }
        Ok(content)
    })
}

fn overlay_style(
    mut style: TextStyle,
    font: Option<String>,
    math_font: Option<String>,
    size: Option<f64>,
    weight: Option<u16>,
    italic: Option<bool>,
    color: Option<PyColor>,
    opacity: Option<f32>,
    letter_spacing: Option<f64>,
    word_spacing: Option<f64>,
    baseline: Option<f64>,
) -> TextStyle {
    if let Some(value) = font {
        style.font = Some(value);
    }
    if let Some(value) = math_font {
        style.math_font = Some(value);
    }
    if let Some(value) = size {
        style.size = Some(value);
    }
    if let Some(value) = weight {
        style.weight = Some(value);
    }
    if let Some(value) = italic {
        style.italic = Some(value);
    }
    if let Some(value) = color {
        style.color = Some(value.0);
    }
    if let Some(value) = opacity {
        style.opacity = Some(value);
    }
    if let Some(value) = letter_spacing {
        style.letter_spacing = Some(value);
    }
    if let Some(value) = word_spacing {
        style.word_spacing = Some(value);
    }
    if let Some(value) = baseline {
        style.baseline = Some(value);
    }
    style
}

#[pyfunction(name = "part")]
#[pyo3(signature = (name, *content, style=None, font=None, math_font=None, size=None, weight=None, italic=None, color=None, opacity=None, letter_spacing=None, word_spacing=None, baseline=None))]
#[allow(clippy::too_many_arguments)]
pub fn text_part(
    name: String,
    content: &Bound<'_, PyTuple>,
    style: Option<PyTextStyle>,
    font: Option<String>,
    math_font: Option<String>,
    size: Option<f64>,
    weight: Option<u16>,
    italic: Option<bool>,
    color: Option<PyColor>,
    opacity: Option<f32>,
    letter_spacing: Option<f64>,
    word_spacing: Option<f64>,
    baseline: Option<f64>,
) -> PyResult<PyTextPart> {
    let style = overlay_style(
        style.map(|style| style.0).unwrap_or_default(),
        font,
        math_font,
        size,
        weight,
        italic,
        color,
        opacity,
        letter_spacing,
        word_spacing,
        baseline,
    );
    let part = TextPart::new(name, content_from_tuple(content)?, style);
    TextSpec::new(
        vec![part.clone().into()],
        None,
        TextStyle::default(),
        TextFlow::default(),
    )
    .map_err(|error| PyValueError::new_err(error.to_string()))?;
    Ok(PyTextPart(part))
}

#[pyfunction(name = "parts")]
#[pyo3(signature = (**entries))]
/// Build an ordered group of plain semantic text parts from keyword entries.
pub fn text_parts(entries: Option<&Bound<'_, PyDict>>) -> PyResult<PyTextParts> {
    let entries = entries
        .ok_or_else(|| PyValueError::new_err("parts() requires at least one named text part"))?;
    if entries.is_empty() {
        return Err(PyValueError::new_err(
            "parts() requires at least one named text part",
        ));
    }
    let mut result = Vec::with_capacity(entries.len());
    for (name, value) in entries.iter() {
        let name = name.extract::<String>()?;
        let text = value
            .extract::<String>()
            .map_err(|_| PyTypeError::new_err("parts() values must be strings"))?;
        result.push(TextPart::new(
            name,
            vec![TextContent::Literal(text)],
            TextStyle::default(),
        ));
    }
    TextSpec::new(
        result.iter().cloned().map(TextContent::Part).collect(),
        None,
        TextStyle::default(),
        TextFlow::default(),
    )
    .map_err(|error| PyValueError::new_err(error.to_string()))?;
    Ok(PyTextParts(result))
}

#[derive(Clone, Copy)]
enum QueryKind {
    Grapheme,
    Word,
    Line,
    Part,
}

#[pyclass(
    name = "TextQuery",
    module = "gaanim_core",
    frozen,
    skip_from_py_object
)]
#[derive(Clone)]
pub struct PyTextQuery {
    handle: gaanim_api::canvas::DrawableHandle,
    spec: TextSpec,
    kind: QueryKind,
}

impl PyTextQuery {
    fn values(&self) -> Vec<(String, Option<usize>, Vec<String>)> {
        match self.kind {
            QueryKind::Grapheme => occurrences(self.spec.graphemes()),
            QueryKind::Word => occurrences(self.spec.words()),
            QueryKind::Line => occurrences(self.spec.explicit_lines()),
            QueryKind::Part => self
                .spec
                .parts()
                .into_iter()
                .map(|part| (part.text, Some(part.occurrence), part.path))
                .collect(),
        }
    }

    fn selection(&self, index: isize) -> PyResult<PyTextSelection> {
        let values = self.values();
        let normalized = if index < 0 {
            values.len() as isize + index
        } else {
            index
        };
        let Some((fragment, occurrence, path)) = values.get(normalized as usize) else {
            return Err(PyIndexError::new_err("text selection index out of range"));
        };
        Ok(PyTextSelection {
            handle: self.handle.clone(),
            spec: self.spec.clone(),
            path: path.clone(),
            fragment: fragment.clone(),
            occurrence: *occurrence,
        })
    }

    fn slice_selection(&self, slice: &Bound<'_, PySlice>) -> PyResult<PyTextSelection> {
        let values = self.values();
        let indices = slice.indices(values.len() as isize)?;
        if indices.step != 1 {
            return Err(PyValueError::new_err(
                "text selection slices require a step of 1",
            ));
        }
        if indices.slicelength == 0 {
            return Err(PyIndexError::new_err("text selection slice is empty"));
        }
        let selected = &values[indices.start as usize..indices.stop as usize];
        let separator = match self.kind {
            QueryKind::Word => " ",
            QueryKind::Line => "\n",
            _ => "",
        };
        let fragment = selected
            .iter()
            .map(|(value, _, _)| value.as_str())
            .collect::<Vec<_>>()
            .join(separator);
        let rendered = gaanim_text::prelude::rendered_text(&self.spec.plain_text());
        let occurrence = rendered
            .match_indices(&fragment)
            .position(|_| true)
            .unwrap_or(0);
        Ok(PyTextSelection {
            handle: self.handle.clone(),
            spec: self.spec.clone(),
            path: Vec::new(),
            fragment,
            occurrence: Some(occurrence),
        })
    }

    fn contains_name(&self, name: &str) -> bool {
        match self.kind {
            QueryKind::Part => self.spec.parts().into_iter().any(|part| {
                part.path.join(".") == name
                    || part.path.last().is_some_and(|part_name| part_name == name)
            }),
            _ => self.values().iter().any(|(value, _, _)| value == name),
        }
    }
}

fn occurrences(values: Vec<String>) -> Vec<(String, Option<usize>, Vec<String>)> {
    use std::collections::HashMap;
    let mut seen = HashMap::<String, usize>::new();
    values
        .into_iter()
        .map(|value| {
            let occurrence = seen.entry(value.clone()).or_default();
            let current = *occurrence;
            *occurrence += 1;
            (value, Some(current), Vec::new())
        })
        .collect()
}

#[pymethods]
impl PyTextQuery {
    fn __len__(&self) -> usize {
        self.values().len()
    }

    fn __getitem__(&self, key: &Bound<'_, PyAny>) -> PyResult<PyTextSelection> {
        if let Ok(index) = key.extract::<isize>() {
            self.selection(index)
        } else if let Ok(slice) = key.cast::<PySlice>() {
            self.slice_selection(slice)
        } else {
            Err(PyTypeError::new_err(
                "text query indices must be int or slice",
            ))
        }
    }

    fn __contains__(&self, value: &str) -> bool {
        self.contains_name(value)
    }
}

#[pyclass(name = "TextSelection", module = "gaanim_core", from_py_object)]
#[derive(Clone)]
pub struct PyTextSelection {
    handle: gaanim_api::canvas::DrawableHandle,
    spec: TextSpec,
    path: Vec<String>,
    fragment: String,
    occurrence: Option<usize>,
}

impl PyTextSelection {
    fn inner(&self) -> gaanim_api::canvas::FragmentSelection {
        match self.occurrence {
            Some(occurrence) => self.handle.select_nth(self.fragment.clone(), occurrence),
            None => self.handle.select(self.fragment.clone()),
        }
    }
}

#[pymethods]
impl PyTextSelection {
    fn __getitem__(&self, name: &str) -> PyResult<Self> {
        let mut path = self.path.clone();
        path.push(name.to_string());
        let Some(part) = self.spec.parts().into_iter().find(|part| part.path == path) else {
            return Err(PyKeyError::new_err(name.to_string()));
        };
        Ok(Self {
            handle: self.handle.clone(),
            spec: self.spec.clone(),
            path,
            fragment: part.text,
            occurrence: Some(part.occurrence),
        })
    }

    fn fill(&self, color: PyColor) -> Self {
        if !self.handle.fill_text_part(&self.path, color.0) {
            self.inner().fill(color.0);
        }
        self.clone()
    }

    /// Start a compound fill/opacity animation scoped to this selection.
    fn animate(&self) -> PyCanvasAnim {
        PyCanvasAnim {
            inner: self.inner().animate_properties(),
        }
    }

    #[pyo3(signature = (color, duration=None))]
    fn color_to(&self, color: PyColor, duration: Option<f64>) -> PyCanvasAnim {
        PyCanvasAnim {
            inner: self.inner().color_to(color.0, duration),
        }
    }

    #[pyo3(signature = (opacity, duration=None))]
    fn opacity_to(&self, opacity: f32, duration: Option<f64>) -> PyResult<PyCanvasAnim> {
        if !opacity.is_finite() || !(0.0..=1.0).contains(&opacity) {
            return Err(PyValueError::new_err(
                "opacity must be finite and within 0..1",
            ));
        }
        Ok(PyCanvasAnim {
            inner: self.inner().opacity_to(opacity, duration),
        })
    }

    #[pyo3(signature = (duration=None))]
    fn indicate(&self, duration: Option<f64>) -> PyCanvasAnim {
        PyCanvasAnim {
            inner: self.inner().indicate(duration),
        }
    }

    #[pyo3(signature = (duration=None))]
    fn pulse(&self, duration: Option<f64>) -> PyCanvasAnim {
        PyCanvasAnim {
            inner: self.inner().pulse(duration),
        }
    }

    #[pyo3(signature = (duration=None))]
    fn wiggle(&self, duration: Option<f64>) -> PyCanvasAnim {
        PyCanvasAnim {
            inner: self.inner().wiggle(duration),
        }
    }

    #[pyo3(signature = (duration=None))]
    fn wave(&self, duration: Option<f64>) -> PyCanvasAnim {
        PyCanvasAnim {
            inner: self.inner().wave(duration),
        }
    }

    #[pyo3(signature = (duration=None))]
    fn highlight(&self, duration: Option<f64>) -> PyCanvasAnim {
        PyCanvasAnim {
            inner: self.inner().highlight(duration),
        }
    }

    #[pyo3(signature = (duration=None))]
    fn focus(&self, duration: Option<f64>) -> PyCanvasAnim {
        PyCanvasAnim {
            inner: self.inner().focus(duration),
        }
    }

    #[pyo3(signature = (duration=None))]
    fn cancel(&self, duration: Option<f64>) -> PyCanvasAnim {
        PyCanvasAnim {
            inner: self.inner().cancel(duration),
        }
    }

    #[pyo3(signature = (*, style="fade", duration=None))]
    fn reveal(&self, style: &str, duration: Option<f64>) -> PyResult<PyCanvasAnim> {
        let style = match style {
            "fade" => gaanim_api::canvas::FragmentRevealStyle::Fade,
            "wipe" => gaanim_api::canvas::FragmentRevealStyle::Wipe,
            "from_below" => gaanim_api::canvas::FragmentRevealStyle::FromBelow,
            _ => {
                return Err(PyValueError::new_err(
                    "style must be fade, wipe, or from_below",
                ))
            }
        };
        Ok(PyCanvasAnim {
            inner: self.inner().reveal(style, duration),
        })
    }

    #[pyo3(signature = (target, *, duration=None))]
    fn morph_to(&self, target: &PyTextSelection, duration: Option<f64>) -> PyResult<PyCanvasAnim> {
        Ok(PyCanvasAnim {
            inner: self
                .inner()
                .morph_to(&target.inner(), duration)
                .map_err(text_transition_error)?,
        })
    }

    #[pyo3(signature = (target, *, duration=None))]
    fn copy_to(&self, target: &PyTextSelection, duration: Option<f64>) -> PyResult<PyCanvasAnim> {
        Ok(PyCanvasAnim {
            inner: self
                .inner()
                .copy_to(&target.inner(), duration)
                .map_err(text_transition_error)?,
        })
    }

    #[pyo3(signature = (label, *, above=false, duration=None))]
    fn brace(&self, label: String, above: bool, duration: Option<f64>) -> PyCanvasAnim {
        PyCanvasAnim {
            inner: self.inner().brace(label, above, duration),
        }
    }

    #[pyo3(signature = (label, *, offset=(120.0, 80.0), duration=None))]
    fn annotate(&self, label: String, offset: (f64, f64), duration: Option<f64>) -> PyCanvasAnim {
        PyCanvasAnim {
            inner: self.inner().annotate(
                label,
                gaanim_core::glam::DVec3::new(offset.0, offset.1, 0.0),
                duration,
            ),
        }
    }
}

#[pyclass(name = "Text", module = "gaanim_core", extends = PyDrawable, skip_from_py_object)]
#[derive(Clone)]
pub struct PyText {
    handle: gaanim_api::canvas::DrawableHandle,
    spec: TextSpec,
}

impl PyText {
    pub(crate) fn initializer(
        handle: gaanim_api::canvas::DrawableHandle,
        spec: TextSpec,
    ) -> PyClassInitializer<Self> {
        PyClassInitializer::from(PyDrawable(handle.clone())).add_subclass(Self { handle, spec })
    }

    fn named(&self, name: &str) -> PyResult<PyTextSelection> {
        let path = vec![name.to_string()];
        let Some(part) = self.spec.parts().into_iter().find(|part| part.path == path) else {
            return Err(PyKeyError::new_err(name.to_string()));
        };
        Ok(PyTextSelection {
            handle: self.handle.clone(),
            spec: self.spec.clone(),
            path,
            fragment: part.text,
            occurrence: Some(part.occurrence),
        })
    }

    fn whole(&self) -> PyTextSelection {
        PyTextSelection {
            handle: self.handle.clone(),
            spec: self.spec.clone(),
            path: Vec::new(),
            fragment: gaanim_text::prelude::rendered_text(&self.spec.plain_text()),
            occurrence: None,
        }
    }

    fn require_free_position(&self, operation: &str) -> PyResult<()> {
        if self.handle.layout_owner().is_some() {
            Err(crate::LayoutOwnershipError::new_err(format!(
                "layout owns this Text's translation; use scene.item(..., offset=...) or layout.configure_item(...). Operation: {operation}"
            )))
        } else {
            Ok(())
        }
    }
}

#[pymethods]
impl PyText {
    /// Apply a fill while preserving the specialized Text handle.
    fn fill(slf: PyRef<'_, Self>, paint: PyPaint) -> PyRef<'_, Self> {
        slf.handle.clone().fill_brush(paint.0);
        slf
    }

    fn no_fill(slf: PyRef<'_, Self>) -> PyRef<'_, Self> {
        slf.handle.clone().no_fill();
        slf
    }

    fn stroke(slf: PyRef<'_, Self>, paint: PyPaint, width: f64) -> PyRef<'_, Self> {
        slf.handle.clone().stroke_brush(paint.0, width);
        slf
    }

    fn no_stroke(slf: PyRef<'_, Self>) -> PyRef<'_, Self> {
        slf.handle.clone().no_stroke();
        slf
    }

    fn opacity(slf: PyRef<'_, Self>, value: f32) -> PyRef<'_, Self> {
        slf.handle.clone().opacity(value);
        slf
    }

    fn z_index(slf: PyRef<'_, Self>, value: i32) -> PyRef<'_, Self> {
        slf.handle.clone().z_index(value);
        slf
    }

    #[pyo3(signature = (x, y=None, anchor=None))]
    fn at<'py>(
        slf: PyRef<'py, Self>,
        x: &Bound<'_, PyAny>,
        y: Option<f64>,
        anchor: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<PyRef<'py, Self>> {
        slf.require_free_position("at")?;
        match resolve_at_target("at", x, y, anchor.is_some())? {
            PyAtTarget::Coordinates { x, y } => {
                match anchor.map(resolve_text_anchor).transpose()? {
                    Some(ResolvedTextAnchor::Geometric(anchor)) => {
                        slf.handle.clone().at_anchor(x, y, anchor);
                    }
                    Some(ResolvedTextAnchor::Typographic(anchor)) => {
                        slf.handle.clone().at_text_anchor(x, y, anchor);
                    }
                    None => {
                        slf.handle.clone().at_text_default(x, y);
                    }
                }
            }
            PyAtTarget::Drawable(reference) => {
                slf.handle.clone().align_to(
                    &reference,
                    gaanim_api::canvas::Anchor::Center,
                    gaanim_api::canvas::Anchor::Center,
                );
            }
            PyAtTarget::AnchorPoint(point) => {
                slf.handle.clone().at_anchor_point(point);
            }
        }
        Ok(slf)
    }

    fn at_3d(slf: PyRef<'_, Self>, x: f64, y: f64, z: f64) -> PyResult<PyRef<'_, Self>> {
        slf.require_free_position("at_3d")?;
        slf.handle.clone().at_3d(x, y, z);
        Ok(slf)
    }

    fn billboard(slf: PyRef<'_, Self>) -> PyRef<'_, Self> {
        slf.handle.clone().billboard();
        slf
    }

    fn hud(slf: PyRef<'_, Self>) -> PyRef<'_, Self> {
        slf.handle.clone().hud();
        slf
    }

    fn scaled(slf: PyRef<'_, Self>, factor: f64) -> PyRef<'_, Self> {
        slf.handle.clone().scaled(factor);
        slf
    }

    fn scaled_3d(slf: PyRef<'_, Self>, x: f64, y: f64, z: f64) -> PyRef<'_, Self> {
        slf.handle.clone().scaled_3d(x, y, z);
        slf
    }

    fn rotated(slf: PyRef<'_, Self>, radians: f64) -> PyRef<'_, Self> {
        slf.handle.clone().rotated(radians);
        slf
    }

    fn rotated_3d(slf: PyRef<'_, Self>, x: f64, y: f64, z: f64) -> PyRef<'_, Self> {
        slf.handle.clone().rotated_3d(x, y, z);
        slf
    }

    fn with_pivot(slf: PyRef<'_, Self>, x: f64, y: f64) -> PyRef<'_, Self> {
        slf.handle.clone().with_pivot(x, y);
        slf
    }

    fn with_pivot_3d(slf: PyRef<'_, Self>, x: f64, y: f64, z: f64) -> PyRef<'_, Self> {
        slf.handle.clone().with_pivot_3d(x, y, z);
        slf
    }

    fn pivot(slf: PyRef<'_, Self>, x: f64, y: f64) -> PyRef<'_, Self> {
        slf.handle.clone().pivot(x, y);
        slf
    }

    fn at_anchor<'py>(
        slf: PyRef<'py, Self>,
        x: f64,
        y: f64,
        anchor: &Bound<'_, PyAny>,
    ) -> PyResult<PyRef<'py, Self>> {
        slf.require_free_position("at_anchor")?;
        match resolve_text_anchor(anchor)? {
            ResolvedTextAnchor::Geometric(anchor) => {
                slf.handle.clone().at_anchor(x, y, anchor);
            }
            ResolvedTextAnchor::Typographic(anchor) => {
                slf.handle.clone().at_text_anchor(x, y, anchor);
            }
        }
        Ok(slf)
    }

    #[pyo3(signature = (reference, direction, spacing=24.0, aligned_edge=None))]
    fn next_to<'py>(
        slf: PyRef<'py, Self>,
        reference: &PyDrawable,
        direction: &PyDirection,
        spacing: f64,
        aligned_edge: Option<&PyAnchor>,
    ) -> PyResult<PyRef<'py, Self>> {
        slf.require_free_position("next_to")?;
        let aligned_edge = aligned_edge
            .map(|anchor| anchor.0)
            .unwrap_or(gaanim_api::canvas::Anchor::Center);
        slf.handle
            .clone()
            .next_to_aligned(&reference.0, direction.0, spacing, aligned_edge);
        Ok(slf)
    }

    #[pyo3(signature = (reference, target_anchor, reference_anchor=None))]
    fn align_to<'py>(
        slf: PyRef<'py, Self>,
        reference: &PyDrawable,
        target_anchor: &PyAnchor,
        reference_anchor: Option<&PyAnchor>,
    ) -> PyResult<PyRef<'py, Self>> {
        slf.require_free_position("align_to")?;
        let reference_anchor = reference_anchor
            .map(|anchor| anchor.0)
            .unwrap_or(target_anchor.0);
        slf.handle
            .clone()
            .align_to(&reference.0, target_anchor.0, reference_anchor);
        Ok(slf)
    }

    #[pyo3(signature = (direction, buff=24.0))]
    fn to_edge<'py>(
        slf: PyRef<'py, Self>,
        direction: &PyDirection,
        buff: f64,
    ) -> PyResult<PyRef<'py, Self>> {
        slf.require_free_position("to_edge")?;
        slf.handle.clone().to_edge(direction.0, buff);
        Ok(slf)
    }

    #[pyo3(signature = (corner, buff=24.0))]
    fn to_corner<'py>(
        slf: PyRef<'py, Self>,
        corner: &PyAnchor,
        buff: f64,
    ) -> PyResult<PyRef<'py, Self>> {
        slf.require_free_position("to_corner")?;
        slf.handle.clone().to_corner(corner.0, buff);
        Ok(slf)
    }

    fn __getitem__(&self, key: &Bound<'_, PyAny>) -> PyResult<PyTextSelection> {
        if let Ok(name) = key.extract::<String>() {
            self.named(&name)
        } else if let Ok(index) = key.extract::<isize>() {
            PyTextQuery {
                handle: self.handle.clone(),
                spec: self.spec.clone(),
                kind: QueryKind::Grapheme,
            }
            .selection(index)
        } else if let Ok(slice) = key.cast::<PySlice>() {
            PyTextQuery {
                handle: self.handle.clone(),
                spec: self.spec.clone(),
                kind: QueryKind::Grapheme,
            }
            .slice_selection(slice)
        } else {
            Err(PyTypeError::new_err(
                "Text indices must be a part name or grapheme index",
            ))
        }
    }

    #[getter]
    fn graphemes(&self) -> PyTextQuery {
        PyTextQuery {
            handle: self.handle.clone(),
            spec: self.spec.clone(),
            kind: QueryKind::Grapheme,
        }
    }
    #[getter]
    fn words(&self) -> PyTextQuery {
        PyTextQuery {
            handle: self.handle.clone(),
            spec: self.spec.clone(),
            kind: QueryKind::Word,
        }
    }
    #[getter]
    fn lines(&self) -> PyTextQuery {
        PyTextQuery {
            handle: self.handle.clone(),
            spec: self.spec.clone(),
            kind: QueryKind::Line,
        }
    }
    #[getter]
    fn parts(&self) -> PyTextQuery {
        PyTextQuery {
            handle: self.handle.clone(),
            spec: self.spec.clone(),
            kind: QueryKind::Part,
        }
    }

    #[pyo3(signature = (duration=None, *, by="grapheme", order="forward", stagger=0.0))]
    fn write(
        &self,
        duration: Option<f64>,
        by: &str,
        order: &str,
        stagger: f64,
    ) -> PyResult<PyCanvasAnim> {
        validate_grouping(by, order, stagger)?;
        if by == "part" {
            self.handle.write_by_parts(None, duration.unwrap_or(1.0));
        }
        Ok(PyCanvasAnim {
            inner: self.handle.write(duration),
        })
    }

    #[pyo3(signature = (duration=None, *, by="grapheme", order="forward", stagger=0.04))]
    fn type_in(
        &self,
        duration: Option<f64>,
        by: &str,
        order: &str,
        stagger: f64,
    ) -> PyResult<PyCanvasAnim> {
        self.write(duration, by, order, stagger)
    }

    #[pyo3(signature = (duration=None, *, by="grapheme", order="forward", stagger=0.0))]
    fn reveal(
        &self,
        duration: Option<f64>,
        by: &str,
        order: &str,
        stagger: f64,
    ) -> PyResult<PyCanvasAnim> {
        self.write(duration, by, order, stagger)
    }

    #[pyo3(signature = (duration=None))]
    fn fade_in(&self, duration: Option<f64>) -> PyCanvasAnim {
        PyCanvasAnim {
            inner: self.handle.fade_in(duration),
        }
    }

    #[pyo3(signature = (direction="up", *, distance=24.0, duration=None))]
    fn slide_in(
        &self,
        direction: &str,
        distance: f64,
        duration: Option<f64>,
    ) -> PyResult<PyCanvasAnim> {
        let direction = match direction {
            "up" => gaanim_layout::Direction::Up,
            "down" => gaanim_layout::Direction::Down,
            "left" => gaanim_layout::Direction::Left,
            "right" => gaanim_layout::Direction::Right,
            _ => {
                return Err(PyValueError::new_err(
                    "direction must be up, down, left, or right",
                ))
            }
        };
        Ok(PyCanvasAnim {
            inner: self.handle.fade_in_from(direction, distance, duration),
        })
    }

    #[pyo3(signature = (duration=None))]
    fn unwrite(&self, duration: Option<f64>) -> PyCanvasAnim {
        PyCanvasAnim {
            inner: self.handle.unwrite(duration),
        }
    }
    #[pyo3(signature = (duration=None))]
    fn erase(&self, duration: Option<f64>) -> PyCanvasAnim {
        self.unwrite(duration)
    }
    #[pyo3(signature = (duration=None))]
    fn fade_out(&self, duration: Option<f64>) -> PyCanvasAnim {
        PyCanvasAnim {
            inner: self.handle.fade_out(duration),
        }
    }
    #[pyo3(signature = (direction="down", *, distance=24.0, duration=None))]
    fn slide_out(
        &self,
        direction: &str,
        distance: f64,
        duration: Option<f64>,
    ) -> PyResult<PyCanvasAnim> {
        if !matches!(direction, "up" | "down" | "left" | "right") {
            return Err(PyValueError::new_err(
                "direction must be up, down, left, or right",
            ));
        }
        if !distance.is_finite() || distance < 0.0 {
            return Err(PyValueError::new_err(
                "distance must be finite and non-negative",
            ));
        }
        Ok(self.fade_out(duration))
    }
    #[pyo3(signature = (duration=None))]
    fn indicate(&self, duration: Option<f64>) -> PyCanvasAnim {
        PyCanvasAnim {
            inner: self.handle.indicate(duration),
        }
    }
    #[pyo3(signature = (duration=None))]
    fn pulse(&self, duration: Option<f64>) -> PyCanvasAnim {
        self.indicate(duration)
    }
    #[pyo3(signature = (duration=None))]
    fn wiggle(&self, duration: Option<f64>) -> PyCanvasAnim {
        PyCanvasAnim {
            inner: self.handle.wiggle(duration),
        }
    }
    #[pyo3(signature = (duration=None))]
    fn wave(&self, duration: Option<f64>) -> PyCanvasAnim {
        self.wiggle(duration)
    }
    #[pyo3(signature = (duration=None))]
    fn highlight(&self, duration: Option<f64>) -> PyCanvasAnim {
        PyCanvasAnim {
            inner: self.handle.circumscribe(duration),
        }
    }
    #[pyo3(signature = (duration=None))]
    fn focus(&self, duration: Option<f64>) -> PyCanvasAnim {
        self.indicate(duration)
    }
    #[pyo3(signature = (duration=None))]
    fn cancel(&self, duration: Option<f64>) -> PyCanvasAnim {
        let selection = self.whole();
        selection.cancel(duration)
    }
    #[pyo3(signature = (label, *, above=false, duration=None))]
    fn brace(&self, label: String, above: bool, duration: Option<f64>) -> PyCanvasAnim {
        let selection = self.whole();
        selection.brace(label, above, duration)
    }
    #[pyo3(signature = (label, *, offset=(120.0, 80.0), duration=None))]
    fn annotate(&self, label: String, offset: (f64, f64), duration: Option<f64>) -> PyCanvasAnim {
        let selection = self.whole();
        selection.annotate(label, offset, duration)
    }

    #[pyo3(signature = (target, *, r#match="auto", duration=1.0))]
    fn morph_to(&self, target: &PyText, r#match: &str, duration: f64) -> PyResult<PyCanvasAnim> {
        if !matches!(r#match, "auto" | "semantic" | "grapheme" | "shape") {
            return Err(PyValueError::new_err(
                "match must be 'auto', 'semantic', 'grapheme', or 'shape'",
            ));
        }
        validate_duration(duration)?;
        Ok(PyCanvasAnim {
            inner: self
                .handle
                .morph_to(&target.handle, duration)
                .map_err(text_transition_error)?,
        })
    }

    #[pyo3(signature = (target, *, matches=None, duration=1.0))]
    fn step_to(
        &self,
        target: &PyText,
        matches: Option<&Bound<'_, PyAny>>,
        duration: f64,
    ) -> PyResult<PyCanvasAnim> {
        validate_duration(duration)?;
        Ok(PyCanvasAnim {
            inner: self
                .handle
                .step_to(&target.handle, parse_matches(matches)?, duration)
                .map_err(text_transition_error)?,
        })
    }

    #[pyo3(signature = (target, *, anchor="part", duration=1.0))]
    fn expand_to(&self, target: &PyText, anchor: &str, duration: f64) -> PyResult<PyCanvasAnim> {
        validate_duration(duration)?;
        let anchor = if anchor == "part" {
            self.spec
                .parts()
                .into_iter()
                .find(|source| {
                    target
                        .spec
                        .parts()
                        .iter()
                        .any(|candidate| candidate.path == source.path)
                })
                .map(|part| part.path.join("."))
                .ok_or_else(|| PyKeyError::new_err("no shared semantic part to use as anchor"))?
        } else {
            anchor.to_string()
        };
        Ok(PyCanvasAnim {
            inner: self
                .handle
                .expand_to(&target.handle, &anchor, duration)
                .map_err(text_transition_error)?,
        })
    }

    #[pyo3(signature = (*content, role=None, style=None, flow=None, duration=1.0))]
    fn r#become(
        &mut self,
        content: &Bound<'_, PyTuple>,
        role: Option<&str>,
        style: Option<PyTextStyle>,
        flow: Option<PyTextFlow>,
        duration: f64,
    ) -> PyResult<()> {
        validate_duration(duration)?;
        let mut spec = TextSpec::new(
            content_from_tuple(content)?,
            parse_role(role)?,
            style
                .map(|style| style.0)
                .unwrap_or_else(|| self.spec.style.clone()),
            flow.map(|flow| flow.0)
                .unwrap_or_else(|| self.spec.flow.clone()),
        )
        .map_err(|error| PyValueError::new_err(error.to_string()))?;
        spec.version = self.spec.version.saturating_add(1);
        self.handle.r#become(spec.clone(), Some(duration));
        self.spec = spec;
        Ok(())
    }
}

fn validate_duration(duration: f64) -> PyResult<()> {
    if duration.is_finite() && duration > 0.0 {
        Ok(())
    } else {
        Err(PyValueError::new_err(
            "duration must be a finite positive number",
        ))
    }
}

fn text_transition_error(error: gaanim_api::canvas::LayoutOwnershipError) -> PyErr {
    crate::LayoutOwnershipError::new_err(error.to_string())
}

fn parse_matches(matches: Option<&Bound<'_, PyAny>>) -> PyResult<Option<Vec<(String, String)>>> {
    let Some(matches) = matches else {
        return Ok(None);
    };
    if let Ok(mapping) = matches.cast::<PyDict>() {
        return mapping
            .iter()
            .map(|(source, target)| Ok((source.extract()?, target.extract()?)))
            .collect::<PyResult<Vec<_>>>()
            .map(Some);
    }
    matches
        .extract::<Vec<(String, String)>>()
        .map(Some)
        .map_err(|_| {
            PyTypeError::new_err(
                "matches must be a mapping or a sequence of (source_part, target_part) pairs",
            )
        })
}

fn validate_grouping(by: &str, order: &str, stagger: f64) -> PyResult<()> {
    if !matches!(by, "grapheme" | "word" | "line" | "part") {
        return Err(PyValueError::new_err(
            "by must be grapheme, word, line, or part",
        ));
    }
    if !matches!(order, "forward" | "reverse" | "center" | "random") {
        return Err(PyValueError::new_err(
            "order must be forward, reverse, center, or random",
        ));
    }
    if !stagger.is_finite() || stagger < 0.0 {
        return Err(PyValueError::new_err(
            "stagger must be finite and non-negative",
        ));
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn build_text_spec(
    content: &Bound<'_, PyTuple>,
    equation: bool,
    role_name: Option<&str>,
    style: Option<PyTextStyle>,
    flow: Option<PyTextFlow>,
    font: Option<String>,
    math_font: Option<String>,
    size: Option<f64>,
    weight: Option<u16>,
    italic: Option<bool>,
    color: Option<PyColor>,
    opacity: Option<f32>,
    letter_spacing: Option<f64>,
    word_spacing: Option<f64>,
    baseline: Option<f64>,
    wrap: Option<&Bound<'_, PyAny>>,
    text_align: Option<&str>,
    line_spacing: Option<f64>,
    max_lines: Option<usize>,
    overflow: Option<&str>,
    direction: Option<&str>,
    hyphenate: Option<bool>,
) -> PyResult<TextSpec> {
    let style = overlay_style(
        style.map(|style| style.0).unwrap_or_default(),
        font,
        math_font,
        size,
        weight,
        italic,
        color,
        opacity,
        letter_spacing,
        word_spacing,
        baseline,
    );
    let mut flow = flow.map(|flow| flow.0).unwrap_or_default();
    if let Some(value) = wrap {
        flow.wrap = parse_wrap(value)?;
    }
    if let Some(value) = text_align {
        flow.align = parse_align(value)?;
    }
    if let Some(value) = line_spacing {
        flow.line_spacing = value;
    }
    if max_lines.is_some() {
        flow.max_lines = max_lines;
    }
    if let Some(value) = overflow {
        flow.overflow = parse_overflow(value)?;
    }
    if let Some(value) = direction {
        flow.direction = parse_direction(value)?;
    }
    if let Some(value) = hyphenate {
        flow.hyphenate = value;
    }
    let mut content = content_from_tuple(content)?;
    if equation {
        if flatten_content(&content).is_empty() {
            return Err(PyValueError::new_err("equation content must not be empty"));
        }
        content.insert(0, TextContent::Literal("$ ".to_owned()));
        content.push(TextContent::Literal(" $".to_owned()));
    }
    TextSpec::new(content, parse_role(role_name)?, style, flow)
        .map_err(|error| PyValueError::new_err(error.to_string()))
}
