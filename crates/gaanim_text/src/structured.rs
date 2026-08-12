//! Structured, role-aware text authoring shared by the Rust and Python APIs.
//!
//! This module deliberately contains no box layout concepts. `TextFlow` only
//! describes typographic composition; `gaanim_layout::BoxConstraints` remains
//! the authority for the outer size offered to a text leaf.

use std::collections::{HashMap, HashSet};

use gaanim_core::peniko::Color;
use unicode_segmentation::UnicodeSegmentation;

use crate::config::TextRole;

#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum TextWrap {
    Auto,
    NoWrap,
    Width(f64),
}

impl Default for TextWrap {
    fn default() -> Self {
        Self::Auto
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum TextAlign {
    #[default]
    Left,
    Center,
    Right,
    Justify,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum TextOverflow {
    Visible,
    #[default]
    Clip,
    Ellipsis,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum TextDirection {
    #[default]
    Auto,
    Ltr,
    Rtl,
}

/// Visual and metric text properties. All fields are optional so a reusable
/// style can overlay the active semantic role without copying theme defaults.
#[derive(Debug, Clone, PartialEq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct TextStyle {
    pub font: Option<String>,
    pub math_font: Option<String>,
    pub fallbacks: Vec<String>,
    pub size: Option<f64>,
    pub weight: Option<u16>,
    pub italic: Option<bool>,
    pub color: Option<Color>,
    pub stroke_color: Option<Color>,
    pub stroke_width: Option<f64>,
    pub opacity: Option<f32>,
    pub letter_spacing: Option<f64>,
    pub word_spacing: Option<f64>,
    pub decorations: Vec<String>,
    pub baseline: Option<f64>,
}

/// Internal typographic flow. It intentionally excludes padding, box size,
/// columns, growth, fitting, and vertical alignment.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct TextFlow {
    pub wrap: TextWrap,
    pub align: TextAlign,
    pub line_spacing: f64,
    pub max_lines: Option<usize>,
    pub overflow: TextOverflow,
    pub direction: TextDirection,
    pub hyphenate: bool,
}

impl Default for TextFlow {
    fn default() -> Self {
        Self {
            wrap: TextWrap::Auto,
            align: TextAlign::Left,
            line_spacing: 1.2,
            max_lines: None,
            overflow: TextOverflow::Clip,
            direction: TextDirection::Auto,
            hyphenate: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum TextContent {
    Literal(String),
    Part(TextPart),
}

impl From<&str> for TextContent {
    fn from(value: &str) -> Self {
        Self::Literal(value.to_string())
    }
}

impl From<String> for TextContent {
    fn from(value: String) -> Self {
        Self::Literal(value)
    }
}

impl From<TextPart> for TextContent {
    fn from(value: TextPart) -> Self {
        Self::Part(value)
    }
}

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct TextPart {
    pub name: String,
    pub content: Vec<TextContent>,
    pub style: TextStyle,
}

impl TextPart {
    pub fn new(name: impl Into<String>, content: Vec<TextContent>, style: TextStyle) -> Self {
        Self {
            name: name.into(),
            content,
            style,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct TextSpec {
    pub content: Vec<TextContent>,
    pub role: TextRole,
    pub style: TextStyle,
    pub flow: TextFlow,
    pub version: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TextSpecError {
    #[error("text content must not be empty")]
    Empty,
    #[error("text part name must not be empty")]
    EmptyPartName,
    #[error("duplicate text part '{name}' below '{parent}'")]
    DuplicatePart { name: String, parent: String },
    #[error("unbalanced '$' math delimiter; escape a literal dollar as '\\$'")]
    UnbalancedMath,
    #[error("unbalanced '{delimiter}' text markup delimiter; escape it as '\\{delimiter}'")]
    UnbalancedMarkup { delimiter: char },
    #[error("misnested '{delimiter}' text markup delimiter")]
    MisnestedMarkup { delimiter: char },
    #[error("text size must be finite and greater than zero")]
    InvalidSize,
    #[error("text weight must be between 1 and 1000")]
    InvalidWeight,
    #[error("text opacity must be finite and between zero and one")]
    InvalidOpacity,
    #[error("text stroke width and spacing values must be finite and non-negative")]
    InvalidMetric,
    #[error("text flow width must be finite and greater than zero")]
    InvalidWrap,
    #[error("line_spacing must be finite and greater than zero")]
    InvalidLineSpacing,
    #[error("max_lines must be at least one")]
    InvalidMaxLines,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TextPartInfo {
    pub path: Vec<String>,
    pub text: String,
    pub occurrence: usize,
    pub style: TextStyle,
}

impl TextSpec {
    pub fn new(
        content: Vec<TextContent>,
        role: Option<TextRole>,
        style: TextStyle,
        flow: TextFlow,
    ) -> Result<Self, TextSpecError> {
        validate_content(&content, "<root>")?;
        validate_style(&style)?;
        validate_flow(&flow)?;
        let plain = flatten_content(&content);
        if plain.is_empty() {
            return Err(TextSpecError::Empty);
        }
        let parsed = parse_inline_math(&plain)?;
        let mut markup = InlineMarkupParser::new();
        markup.push(&plain)?;
        markup.finish()?;
        let inferred = if parsed
            .iter()
            .all(|segment| segment.math || segment.text.trim().is_empty())
            && parsed
                .iter()
                .any(|segment| segment.math && !segment.text.trim().is_empty())
        {
            TextRole::Math
        } else {
            TextRole::Body
        };
        Ok(Self {
            content,
            role: role.unwrap_or(inferred),
            style,
            flow,
            version: 0,
        })
    }

    pub fn plain_text(&self) -> String {
        flatten_content(&self.content)
    }

    pub fn graphemes(&self) -> Vec<String> {
        rendered_text(&self.plain_text())
            .graphemes(true)
            .map(str::to_string)
            .collect()
    }

    pub fn words(&self) -> Vec<String> {
        rendered_text(&self.plain_text())
            .unicode_words()
            .map(str::to_string)
            .collect()
    }

    pub fn explicit_lines(&self) -> Vec<String> {
        rendered_text(&self.plain_text())
            .split('\n')
            .map(str::to_string)
            .collect()
    }

    pub fn parts(&self) -> Vec<TextPartInfo> {
        let mut raw = Vec::new();
        collect_parts(&self.content, &mut Vec::new(), &mut raw);
        let rendered = rendered_text(&self.plain_text());
        let mut seen: HashMap<String, usize> = HashMap::new();
        raw.into_iter()
            .map(|(path, text, style)| {
                let text = rendered_text(&text);
                let occurrence = *seen.entry(text.clone()).or_insert(0);
                *seen.get_mut(&text).expect("part occurrence inserted") += 1;
                // Keep occurrence deterministic even if the part's exact text is
                // repeated elsewhere outside semantic parts.
                let occurrence = rendered
                    .match_indices(&text)
                    .nth(occurrence)
                    .map(|(offset, _)| rendered[..offset].matches(&text).count())
                    .unwrap_or(occurrence);
                TextPartInfo {
                    path,
                    text,
                    occurrence,
                    style,
                }
            })
            .collect()
    }

    pub fn bumped(mut self) -> Self {
        self.version = self.version.saturating_add(1);
        self
    }
}

fn validate_style(style: &TextStyle) -> Result<(), TextSpecError> {
    if style
        .size
        .is_some_and(|size| !size.is_finite() || size <= 0.0)
    {
        return Err(TextSpecError::InvalidSize);
    }
    if style
        .weight
        .is_some_and(|weight| !(1..=1000).contains(&weight))
    {
        return Err(TextSpecError::InvalidWeight);
    }
    if style
        .opacity
        .is_some_and(|opacity| !opacity.is_finite() || !(0.0..=1.0).contains(&opacity))
    {
        return Err(TextSpecError::InvalidOpacity);
    }
    if [style.stroke_width, style.letter_spacing, style.word_spacing]
        .into_iter()
        .flatten()
        .any(|value| !value.is_finite() || value < 0.0)
    {
        return Err(TextSpecError::InvalidMetric);
    }
    if style.baseline.is_some_and(|value| !value.is_finite()) {
        return Err(TextSpecError::InvalidMetric);
    }
    Ok(())
}

fn validate_flow(flow: &TextFlow) -> Result<(), TextSpecError> {
    if matches!(flow.wrap, TextWrap::Width(width) if !width.is_finite() || width <= 0.0) {
        return Err(TextSpecError::InvalidWrap);
    }
    if !flow.line_spacing.is_finite() || flow.line_spacing <= 0.0 {
        return Err(TextSpecError::InvalidLineSpacing);
    }
    if flow.max_lines == Some(0) {
        return Err(TextSpecError::InvalidMaxLines);
    }
    Ok(())
}

fn validate_content(content: &[TextContent], parent: &str) -> Result<(), TextSpecError> {
    let mut names = HashSet::new();
    for node in content {
        if let TextContent::Part(part) = node {
            if part.name.trim().is_empty() {
                return Err(TextSpecError::EmptyPartName);
            }
            if !names.insert(part.name.clone()) {
                return Err(TextSpecError::DuplicatePart {
                    name: part.name.clone(),
                    parent: parent.to_string(),
                });
            }
            validate_style(&part.style)?;
            validate_content(&part.content, &part.name)?;
        }
    }
    Ok(())
}

pub fn flatten_content(content: &[TextContent]) -> String {
    let mut out = String::new();
    for node in content {
        match node {
            TextContent::Literal(text) => out.push_str(text),
            TextContent::Part(part) => out.push_str(&flatten_content(&part.content)),
        }
    }
    out
}

fn collect_parts(
    content: &[TextContent],
    path: &mut Vec<String>,
    out: &mut Vec<(Vec<String>, String, TextStyle)>,
) {
    for node in content {
        if let TextContent::Part(part) = node {
            path.push(part.name.clone());
            out.push((
                path.clone(),
                flatten_content(&part.content),
                part.style.clone(),
            ));
            collect_parts(&part.content, path, out);
            path.pop();
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InlineSegment {
    pub math: bool,
    pub text: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum InlineMarkupKind {
    Strong,
    Emphasis,
}

impl InlineMarkupKind {
    fn delimiter(self) -> char {
        match self {
            Self::Strong => '*',
            Self::Emphasis => '_',
        }
    }
}

/// A rendered text fragment and the semantic inline emphasis active on it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InlineMarkupSegment {
    pub text: String,
    pub strong: bool,
    pub emphasis: bool,
}

/// Incremental parser for Typst-inspired `*strong*` and `_emphasis_` markup.
///
/// The parser is incremental so delimiters and `$...$` math may span semantic
/// `TextPart` boundaries without turning those boundaries into shaping breaks.
#[derive(Debug, Clone, Default)]
pub struct InlineMarkupParser {
    stack: Vec<InlineMarkupKind>,
    in_math: bool,
    previous: Option<char>,
}

impl InlineMarkupParser {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, text: &str) -> Result<Vec<InlineMarkupSegment>, TextSpecError> {
        let chars = text.chars().collect::<Vec<_>>();
        let mut segments = Vec::<InlineMarkupSegment>::new();
        let mut index = 0;
        while index < chars.len() {
            let character = chars[index];
            let next = chars.get(index + 1).copied();

            if character == '\\' && !self.in_math && matches!(next, Some('*' | '_')) {
                let literal = next.expect("escaped markup delimiter");
                self.push_literal(&mut segments, literal);
                self.previous = Some(literal);
                index += 2;
                continue;
            }
            if character == '\\' && next == Some('$') {
                self.push_literal(&mut segments, character);
                self.push_literal(&mut segments, '$');
                self.previous = Some('$');
                index += 2;
                continue;
            }
            if character == '$' {
                self.push_literal(&mut segments, character);
                self.in_math = !self.in_math;
                self.previous = Some(character);
                index += 1;
                continue;
            }
            if !self.in_math && matches!(character, '*' | '_') {
                let kind = if character == '*' {
                    InlineMarkupKind::Strong
                } else {
                    InlineMarkupKind::Emphasis
                };
                let previous = self.previous;
                let repeated = previous == Some(character) || next == Some(character);
                let closes = self.stack.last() == Some(&kind)
                    && previous.is_some_and(|value| !value.is_whitespace())
                    && !repeated;
                if closes {
                    self.stack.pop();
                    self.previous = Some(character);
                    index += 1;
                    continue;
                }
                if self.stack.contains(&kind)
                    && previous.is_some_and(|value| !value.is_whitespace())
                {
                    return Err(TextSpecError::MisnestedMarkup {
                        delimiter: character,
                    });
                }
                let inside_word = character == '_'
                    && previous.is_some_and(char::is_alphanumeric)
                    && next.is_some_and(char::is_alphanumeric);
                let opens =
                    next.is_some_and(|value| !value.is_whitespace()) && !inside_word && !repeated;
                if opens {
                    self.stack.push(kind);
                    self.previous = Some(character);
                    index += 1;
                    continue;
                }
            }

            self.push_literal(&mut segments, character);
            self.previous = Some(character);
            index += 1;
        }
        Ok(segments)
    }

    pub fn finish(self) -> Result<(), TextSpecError> {
        if let Some(kind) = self.stack.last().copied() {
            return Err(TextSpecError::UnbalancedMarkup {
                delimiter: kind.delimiter(),
            });
        }
        Ok(())
    }

    fn push_literal(&self, segments: &mut Vec<InlineMarkupSegment>, character: char) {
        let strong = self.stack.contains(&InlineMarkupKind::Strong);
        let emphasis = self.stack.contains(&InlineMarkupKind::Emphasis);
        if let Some(segment) = segments.last_mut()
            && segment.strong == strong
            && segment.emphasis == emphasis
        {
            segment.text.push(character);
        } else {
            segments.push(InlineMarkupSegment {
                text: character.to_string(),
                strong,
                emphasis,
            });
        }
    }
}

/// Parse `$...$` while preserving escaped dollar signs. Both `$` and `$$`
/// use the same inline/display vector math compositor in scene text.
pub fn parse_inline_math(text: &str) -> Result<Vec<InlineSegment>, TextSpecError> {
    let mut segments = Vec::new();
    let mut buffer = String::new();
    let mut in_math = false;
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\\' && chars.peek() == Some(&'$') {
            chars.next();
            buffer.push('$');
        } else if ch == '$' {
            if chars.peek() == Some(&'$') {
                chars.next();
            }
            segments.push(InlineSegment {
                math: in_math,
                text: std::mem::take(&mut buffer),
            });
            in_math = !in_math;
        } else {
            buffer.push(ch);
        }
    }
    if in_math {
        return Err(TextSpecError::UnbalancedMath);
    }
    if !buffer.is_empty() || segments.is_empty() {
        segments.push(InlineSegment {
            math: false,
            text: buffer,
        });
    }
    Ok(segments)
}

pub fn rendered_text(text: &str) -> String {
    let mut markup = InlineMarkupParser::new();
    let Ok(markup_segments) = markup.push(text) else {
        return text.to_string();
    };
    if markup.finish().is_err() {
        return text.to_string();
    }
    let text = markup_segments
        .into_iter()
        .map(|segment| segment.text)
        .collect::<String>();
    parse_inline_math(&text)
        .map(|segments| segments.into_iter().map(|segment| segment.text).collect())
        .unwrap_or(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nested_parts_keep_semantic_paths() {
        let spec = TextSpec::new(
            vec![
                TextPart::new(
                    "formula",
                    vec![
                        "$E = ".into(),
                        TextPart::new("mass", vec!["m".into()], TextStyle::default()).into(),
                        " c^2$".into(),
                    ],
                    TextStyle::default(),
                )
                .into(),
            ],
            None,
            TextStyle::default(),
            TextFlow::default(),
        )
        .unwrap();
        assert_eq!(spec.role, TextRole::Math);
        assert_eq!(spec.parts()[1].path, ["formula", "mass"]);
        assert_eq!(
            spec.graphemes(),
            ["E", " ", "=", " ", "m", " ", "c", "^", "2"]
        );
    }

    #[test]
    fn rejects_unbalanced_math_and_duplicate_siblings() {
        assert_eq!(
            TextSpec::new(
                vec!["$x".into()],
                None,
                TextStyle::default(),
                TextFlow::default(),
            ),
            Err(TextSpecError::UnbalancedMath)
        );
        assert!(matches!(
            TextSpec::new(
                vec![
                    TextPart::new("x", vec!["a".into()], TextStyle::default()).into(),
                    TextPart::new("x", vec!["b".into()], TextStyle::default()).into(),
                ],
                None,
                TextStyle::default(),
                TextFlow::default(),
            ),
            Err(TextSpecError::DuplicatePart { .. })
        ));
    }

    #[test]
    fn inline_markup_is_rendered_and_ignores_math_and_common_literals() {
        let spec = TextSpec::new(
            vec!["Texto _enfatizado_, *fuerte*, snake_case, __init__, 5 * 4 y $x_1 * 5$.".into()],
            None,
            TextStyle::default(),
            TextFlow::default(),
        )
        .unwrap();
        assert_eq!(
            rendered_text(&spec.plain_text()),
            "Texto enfatizado, fuerte, snake_case, __init__, 5 * 4 y x_1 * 5."
        );
    }

    #[test]
    fn inline_markup_supports_nesting_escaping_and_part_boundaries() {
        let spec = TextSpec::new(
            vec![
                "*_fuerte ".into(),
                TextPart::new("word", vec!["enfatizado".into()], TextStyle::default()).into(),
                "_* y \\*literal\\*".into(),
            ],
            None,
            TextStyle::default(),
            TextFlow::default(),
        )
        .unwrap();
        assert_eq!(
            rendered_text(&spec.plain_text()),
            "fuerte enfatizado y *literal*"
        );
        assert_eq!(spec.parts()[0].text, "enfatizado");
    }

    #[test]
    fn rejects_unbalanced_or_misnested_inline_markup() {
        assert!(matches!(
            TextSpec::new(
                vec!["*fuerte".into()],
                None,
                TextStyle::default(),
                TextFlow::default(),
            ),
            Err(TextSpecError::UnbalancedMarkup { delimiter: '*' })
        ));
        assert!(matches!(
            TextSpec::new(
                vec!["*_cruzado*_".into()],
                None,
                TextStyle::default(),
                TextFlow::default(),
            ),
            Err(TextSpecError::MisnestedMarkup { delimiter: '*' })
        ));
    }
}
