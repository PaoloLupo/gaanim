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
use crate::pydrawable::{
    resolve_at_target, validate_at_target_owner, PyAtTarget, PyCanvasAnim, PyDrawable,
};
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

#[pyclass(
    name = "TextSelectionAnimation",
    module = "gaanim_core",
    skip_from_py_object
)]
#[derive(Clone)]
pub struct PyTextSelectionAnimation {
    source: gaanim_api::canvas::FragmentSelection,
}

#[pymethods]
impl PyTextSelectionAnimation {
    fn fill(&self, color: PyColor) -> PyResult<PyCanvasAnim> {
        crate::custom::ensure_authoring_allowed()?;
        Ok(PyCanvasAnim {
            inner: self.source.clone().animate_properties().fill(color.0),
        })
    }

    fn opacity(&self, value: f32) -> PyResult<PyCanvasAnim> {
        crate::custom::ensure_authoring_allowed()?;
        Ok(PyCanvasAnim {
            inner: self.source.clone().animate_properties().opacity(value),
        })
    }

    fn indicate(&self) -> PyResult<PyCanvasAnim> {
        crate::custom::ensure_authoring_allowed()?;
        Ok(PyCanvasAnim {
            inner: self.source.clone().animate_properties().indicate(),
        })
    }

    fn wiggle(&self) -> PyResult<PyCanvasAnim> {
        crate::custom::ensure_authoring_allowed()?;
        Ok(PyCanvasAnim {
            inner: self.source.clone().animate_properties().wiggle(),
        })
    }

    fn pulse(&self) -> PyResult<PyCanvasAnim> {
        crate::custom::ensure_authoring_allowed()?;
        Ok(PyCanvasAnim {
            inner: self.source.clone().animate_properties().pulse(),
        })
    }

    fn wave(&self) -> PyResult<PyCanvasAnim> {
        crate::custom::ensure_authoring_allowed()?;
        Ok(PyCanvasAnim {
            inner: self.source.clone().animate_properties().wave(),
        })
    }

    fn highlight(&self) -> PyResult<PyCanvasAnim> {
        crate::custom::ensure_authoring_allowed()?;
        Ok(PyCanvasAnim {
            inner: self.source.clone().animate_properties().highlight(),
        })
    }

    fn focus(&self) -> PyResult<PyCanvasAnim> {
        crate::custom::ensure_authoring_allowed()?;
        Ok(PyCanvasAnim {
            inner: self.source.clone().animate_properties().focus(),
        })
    }

    fn cancel(&self) -> PyResult<PyCanvasAnim> {
        crate::custom::ensure_authoring_allowed()?;
        Ok(PyCanvasAnim {
            inner: self.source.clone().animate_properties().cancel(),
        })
    }

    fn morph_to(&self, target: &PyTextSelection) -> PyResult<PyCanvasAnim> {
        crate::custom::ensure_authoring_allowed()?;
        self.source
            .clone()
            .morph_to(&target.inner(), None)
            .map(|inner| PyCanvasAnim { inner })
            .map_err(|error| crate::LayoutOwnershipError::new_err(error.to_string()))
    }

    fn copy_to(&self, target: &PyTextSelection) -> PyResult<PyCanvasAnim> {
        crate::custom::ensure_authoring_allowed()?;
        self.source
            .clone()
            .copy_to(&target.inner(), None)
            .map(|inner| PyCanvasAnim { inner })
            .map_err(|error| crate::LayoutOwnershipError::new_err(error.to_string()))
    }
}

impl PyTextSelection {
    fn inner(&self) -> gaanim_api::canvas::FragmentSelection {
        match self.occurrence {
            Some(occurrence) => self.handle.select_nth(self.fragment.clone(), occurrence),
            None => self.handle.select(self.fragment.clone()),
        }
    }

    pub(crate) fn bounds_target(&self) -> gaanim_api::canvas::BoundsTarget {
        self.inner().bounds_target()
    }

    pub(crate) fn owner(&self) -> &gaanim_api::canvas::DrawableHandle {
        &self.handle
    }
}

#[pymethods]
impl PyTextSelection {
    fn __getitem__(&self, name: &str) -> PyResult<Self> {
        crate::custom::ensure_authoring_allowed()?;
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

    fn fill(&self, color: PyColor) -> PyResult<Self> {
        crate::custom::ensure_authoring_allowed()?;
        Ok({
            if !self.handle.fill_text_part(&self.path, color.0) {
                self.inner().fill(color.0);
            }
            self.clone()
        })
    }

    /// Start a compound fill/opacity animation scoped to this selection.
    #[getter]
    fn animate(&self) -> PyResult<PyTextSelectionAnimation> {
        crate::custom::ensure_authoring_allowed()?;
        Ok(PyTextSelectionAnimation {
            source: self.inner(),
        })
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn visual_effects_preserve_text_handle_and_typographic_placement() {
        Python::initialize();
        Python::attach(|py| -> PyResult<()> {
            let module = PyModule::new(py, "gaanim_core")?;
            crate::gaanim_core(py, &module)?;
            let scene = module.getattr("Scene")?.call0()?;
            let color = module.getattr("GOLD")?;

            for (method, args) in [
                ("glow", PyTuple::new(py, [&color])?),
                ("blur", PyTuple::empty(py)),
                ("shadow", PyTuple::new(py, [&color])?),
                ("no_effects", PyTuple::empty(py)),
            ] {
                let text = scene.call_method1("text", ("Dinámica de estructuras",))?;
                let styled = text.call_method(method, args, None)?;
                let text_type = module.getattr("Text")?;
                assert!(
                    styled.is_instance(&text_type)?,
                    "{method} must preserve the specialized Text handle"
                );
                styled.call_method1("move_to", (0.0, 0.0))?;
            }
            Ok(())
        })
        .unwrap();
    }
}

#[pymethods]
impl PyText {
    /// Apply a fill while preserving the specialized Text handle.
    fn fill(slf: PyRef<'_, Self>, paint: PyPaint) -> PyResult<PyRef<'_, Self>> {
        crate::custom::ensure_authoring_allowed()?;
        Ok({
            slf.handle.clone().fill_brush(paint.0);
            slf
        })
    }

    fn no_fill(slf: PyRef<'_, Self>) -> PyResult<PyRef<'_, Self>> {
        crate::custom::ensure_authoring_allowed()?;
        Ok({
            slf.handle.clone().no_fill();
            slf
        })
    }

    fn stroke(slf: PyRef<'_, Self>, paint: PyPaint, width: f64) -> PyResult<PyRef<'_, Self>> {
        crate::custom::ensure_authoring_allowed()?;
        Ok({
            slf.handle.clone().stroke_brush(paint.0, width);
            slf
        })
    }

    fn no_stroke(slf: PyRef<'_, Self>) -> PyResult<PyRef<'_, Self>> {
        crate::custom::ensure_authoring_allowed()?;
        Ok({
            slf.handle.clone().no_stroke();
            slf
        })
    }

    #[pyo3(signature = (color, radius=16.0, intensity=1.0))]
    fn glow(
        slf: PyRef<'_, Self>,
        color: PyColor,
        radius: f64,
        intensity: f32,
    ) -> PyResult<PyRef<'_, Self>> {
        crate::custom::ensure_authoring_allowed()?;
        if !radius.is_finite() || radius <= 0.0 {
            return Err(PyValueError::new_err("radius must be finite and positive"));
        }
        if !intensity.is_finite() || intensity <= 0.0 {
            return Err(PyValueError::new_err(
                "intensity must be finite and positive",
            ));
        }
        slf.handle.clone().glow(color.0, radius, intensity);
        Ok(slf)
    }

    #[pyo3(signature = (sigma=4.0))]
    fn blur(slf: PyRef<'_, Self>, sigma: f64) -> PyResult<PyRef<'_, Self>> {
        crate::custom::ensure_authoring_allowed()?;
        if !sigma.is_finite() || sigma <= 0.0 {
            return Err(PyValueError::new_err("sigma must be finite and positive"));
        }
        slf.handle.clone().blur(sigma);
        Ok(slf)
    }

    #[pyo3(signature = (color, x=8.0, y=-8.0, blur=6.0))]
    fn shadow(
        slf: PyRef<'_, Self>,
        color: PyColor,
        x: f64,
        y: f64,
        blur: f64,
    ) -> PyResult<PyRef<'_, Self>> {
        crate::custom::ensure_authoring_allowed()?;
        if !x.is_finite() || !y.is_finite() {
            return Err(PyValueError::new_err("shadow offset must be finite"));
        }
        if !blur.is_finite() || blur < 0.0 {
            return Err(PyValueError::new_err(
                "shadow blur must be finite and non-negative",
            ));
        }
        slf.handle
            .clone()
            .shadow(color.0, gaanim_core::glam::DVec2::new(x, y), blur);
        Ok(slf)
    }

    fn no_effects(slf: PyRef<'_, Self>) -> PyResult<PyRef<'_, Self>> {
        crate::custom::ensure_authoring_allowed()?;
        Ok({
            slf.handle.clone().no_effects();
            slf
        })
    }

    fn opacity<'py>(slf: PyRef<'py, Self>, value: &Bound<'_, PyAny>) -> PyResult<PyRef<'py, Self>> {
        crate::custom::ensure_authoring_allowed()?;
        PyDrawable(slf.handle.clone()).opacity(value)?;
        Ok(slf)
    }

    fn z_index(slf: PyRef<'_, Self>, value: i32) -> PyResult<PyRef<'_, Self>> {
        crate::custom::ensure_authoring_allowed()?;
        Ok({
            slf.handle.clone().z_index(value);
            slf
        })
    }

    #[pyo3(signature = (x, y=None, anchor=None))]
    fn move_to<'py>(
        slf: PyRef<'py, Self>,
        x: &Bound<'_, PyAny>,
        y: Option<&Bound<'_, PyAny>>,
        anchor: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<PyRef<'py, Self>> {
        crate::custom::ensure_authoring_allowed()?;
        slf.require_free_position("move_to")?;
        let mut numeric_y = None;
        if let Some(y) = y {
            let sx =
                crate::visualization::extract_scalar_source_for_drawable(x.clone(), &slf.handle)?;
            let sy =
                crate::visualization::extract_scalar_source_for_drawable(y.clone(), &slf.handle)?;
            if let (Some(_), Some(y)) = (sx.constant_value(), sy.constant_value()) {
                numeric_y = Some(y);
            } else {
                let values = [sx, sy, gaanim_animation::ScalarSource::Constant(0.0)];
                match anchor.map(resolve_text_anchor).transpose()? {
                    Some(ResolvedTextAnchor::Geometric(anchor)) => {
                        PyDrawable(slf.handle.clone()).move_to(
                            x,
                            Some(y),
                            Some(&PyAnchor(anchor)),
                        )?;
                    }
                    Some(ResolvedTextAnchor::Typographic(anchor)) => {
                        slf.handle
                            .clone()
                            .bind_text_position(values, anchor, false)
                            .map_err(PyValueError::new_err)?;
                    }
                    None => {
                        slf.handle
                            .clone()
                            .bind_text_position(values, TextAnchor::BaselineCenter, true)
                            .map_err(PyValueError::new_err)?;
                    }
                }
                return Ok(slf);
            }
        }
        let target = resolve_at_target("move_to", x, numeric_y, anchor.is_some())?;
        validate_at_target_owner(&target, &slf.handle)?;
        match target {
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

    fn move_to_3d<'py>(
        slf: PyRef<'py, Self>,
        x: &Bound<'_, PyAny>,
        y: &Bound<'_, PyAny>,
        z: &Bound<'_, PyAny>,
    ) -> PyResult<PyRef<'py, Self>> {
        crate::custom::ensure_authoring_allowed()?;
        PyDrawable(slf.handle.clone()).move_to_3d(x, y, z)?;
        Ok(slf)
    }

    fn shift_by<'py>(slf: PyRef<'py, Self>, dx: f64, dy: f64) -> PyResult<PyRef<'py, Self>> {
        crate::custom::ensure_authoring_allowed()?;
        PyDrawable(slf.handle.clone()).shift_by(dx, dy)?;
        Ok(slf)
    }

    fn shift_by_3d<'py>(
        slf: PyRef<'py, Self>,
        dx: f64,
        dy: f64,
        dz: f64,
    ) -> PyResult<PyRef<'py, Self>> {
        crate::custom::ensure_authoring_allowed()?;
        PyDrawable(slf.handle.clone()).shift_by_3d(dx, dy, dz)?;
        Ok(slf)
    }

    fn billboard(slf: PyRef<'_, Self>) -> PyResult<PyRef<'_, Self>> {
        crate::custom::ensure_authoring_allowed()?;
        Ok({
            slf.handle.clone().billboard();
            slf
        })
    }

    fn hud(slf: PyRef<'_, Self>) -> PyResult<PyRef<'_, Self>> {
        crate::custom::ensure_authoring_allowed()?;
        Ok({
            slf.handle.clone().hud();
            slf
        })
    }

    fn scale_to<'py>(
        slf: PyRef<'py, Self>,
        factor: &Bound<'_, PyAny>,
    ) -> PyResult<PyRef<'py, Self>> {
        crate::custom::ensure_authoring_allowed()?;
        PyDrawable(slf.handle.clone()).scale_to(factor)?;
        Ok(slf)
    }

    fn scale_by<'py>(slf: PyRef<'py, Self>, factor: f64) -> PyResult<PyRef<'py, Self>> {
        crate::custom::ensure_authoring_allowed()?;
        PyDrawable(slf.handle.clone()).scale_by(factor)?;
        Ok(slf)
    }

    fn scale_to_3d<'py>(
        slf: PyRef<'py, Self>,
        x: &Bound<'_, PyAny>,
        y: &Bound<'_, PyAny>,
        z: &Bound<'_, PyAny>,
    ) -> PyResult<PyRef<'py, Self>> {
        crate::custom::ensure_authoring_allowed()?;
        PyDrawable(slf.handle.clone()).scale_to_3d(x, y, z)?;
        Ok(slf)
    }

    fn scale_by_3d<'py>(
        slf: PyRef<'py, Self>,
        x: f64,
        y: f64,
        z: f64,
    ) -> PyResult<PyRef<'py, Self>> {
        crate::custom::ensure_authoring_allowed()?;
        PyDrawable(slf.handle.clone()).scale_by_3d(x, y, z)?;
        Ok(slf)
    }

    fn rotate_to<'py>(
        slf: PyRef<'py, Self>,
        radians: &Bound<'_, PyAny>,
    ) -> PyResult<PyRef<'py, Self>> {
        crate::custom::ensure_authoring_allowed()?;
        PyDrawable(slf.handle.clone()).rotate_to(radians)?;
        Ok(slf)
    }

    fn rotate_by<'py>(slf: PyRef<'py, Self>, radians: f64) -> PyResult<PyRef<'py, Self>> {
        crate::custom::ensure_authoring_allowed()?;
        PyDrawable(slf.handle.clone()).rotate_by(radians)?;
        Ok(slf)
    }

    fn rotate_to_3d<'py>(
        slf: PyRef<'py, Self>,
        x: &Bound<'_, PyAny>,
        y: &Bound<'_, PyAny>,
        z: &Bound<'_, PyAny>,
    ) -> PyResult<PyRef<'py, Self>> {
        crate::custom::ensure_authoring_allowed()?;
        PyDrawable(slf.handle.clone()).rotate_to_3d(x, y, z)?;
        Ok(slf)
    }

    fn rotate_by_3d<'py>(
        slf: PyRef<'py, Self>,
        axis: &str,
        radians: f64,
    ) -> PyResult<PyRef<'py, Self>> {
        crate::custom::ensure_authoring_allowed()?;
        PyDrawable(slf.handle.clone()).rotate_by_3d(axis, radians)?;
        Ok(slf)
    }

    fn with_pivot(slf: PyRef<'_, Self>, x: f64, y: f64) -> PyResult<PyRef<'_, Self>> {
        crate::custom::ensure_authoring_allowed()?;
        Ok({
            slf.handle.clone().with_pivot(x, y);
            slf
        })
    }

    fn with_pivot_3d(slf: PyRef<'_, Self>, x: f64, y: f64, z: f64) -> PyResult<PyRef<'_, Self>> {
        crate::custom::ensure_authoring_allowed()?;
        Ok({
            slf.handle.clone().with_pivot_3d(x, y, z);
            slf
        })
    }

    fn pivot(slf: PyRef<'_, Self>, x: f64, y: f64) -> PyResult<PyRef<'_, Self>> {
        crate::custom::ensure_authoring_allowed()?;
        Ok({
            slf.handle.clone().pivot(x, y);
            slf
        })
    }

    #[pyo3(signature = (reference, direction, spacing=24.0, aligned_edge=None))]
    fn next_to<'py>(
        slf: PyRef<'py, Self>,
        reference: &PyDrawable,
        direction: &PyDirection,
        spacing: f64,
        aligned_edge: Option<&PyAnchor>,
    ) -> PyResult<PyRef<'py, Self>> {
        crate::custom::ensure_authoring_allowed()?;
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
        crate::custom::ensure_authoring_allowed()?;
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
        crate::custom::ensure_authoring_allowed()?;
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
        crate::custom::ensure_authoring_allowed()?;
        slf.require_free_position("to_corner")?;
        slf.handle.clone().to_corner(corner.0, buff);
        Ok(slf)
    }

    fn __getitem__(&self, key: &Bound<'_, PyAny>) -> PyResult<PyTextSelection> {
        crate::custom::ensure_authoring_allowed()?;
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
    fn graphemes(&self) -> PyResult<PyTextQuery> {
        crate::custom::ensure_authoring_allowed()?;
        Ok(PyTextQuery {
            handle: self.handle.clone(),
            spec: self.spec.clone(),
            kind: QueryKind::Grapheme,
        })
    }
    #[getter]
    fn words(&self) -> PyResult<PyTextQuery> {
        crate::custom::ensure_authoring_allowed()?;
        Ok(PyTextQuery {
            handle: self.handle.clone(),
            spec: self.spec.clone(),
            kind: QueryKind::Word,
        })
    }
    #[getter]
    fn lines(&self) -> PyResult<PyTextQuery> {
        crate::custom::ensure_authoring_allowed()?;
        Ok(PyTextQuery {
            handle: self.handle.clone(),
            spec: self.spec.clone(),
            kind: QueryKind::Line,
        })
    }
    #[getter]
    fn parts(&self) -> PyResult<PyTextQuery> {
        crate::custom::ensure_authoring_allowed()?;
        Ok(PyTextQuery {
            handle: self.handle.clone(),
            spec: self.spec.clone(),
            kind: QueryKind::Part,
        })
    }

    #[pyo3(signature = (*content, role=None, style=None, flow=None))]
    fn r#become(
        &mut self,
        content: &Bound<'_, PyTuple>,
        role: Option<&str>,
        style: Option<PyTextStyle>,
        flow: Option<PyTextFlow>,
    ) -> PyResult<()> {
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
        self.handle.r#become(spec.clone(), None);
        self.spec = spec;
        Ok(())
    }
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
