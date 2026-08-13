//! Compile — replay Canvas ops into SceneBuilder/Bevy.

use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::{Arc, Mutex};

use bevy::prelude::*;
use gaanim_core::ObjectId;
use gaanim_core::glam::{DVec2, DVec3};
use gaanim_core::kurbo::{BezPath, Point, Rect, Shape, Vec2};
use gaanim_core::peniko::Color as PenikoColor;
use gaanim_math::{Bounds3D, GlobalSpatialTransform};
use gaanim_scene::{
    FillBrush, GlobalOpacity, GltfModelRoot, GltfNodeBinding, GltfNodeWrapper, GroupMarker,
    LocalBounds, MobjectId, ObjectTag, Opacity, RenderLayer, RenderOrder, StrokeBrush, Visible,
    WorldBounds,
};
use gaanim_timeline::clip::SceneId;
use gaanim_timeline::timeline::{SegmentMetadata, SegmentStop, Timeline};

use crate::anim::{AnimationBuilder, AnimationType};
use crate::builder::{
    EquationTransitionMode, MobjectRef, MobjectState, MobjectStateMap, SceneBuilder,
};
use crate::canvas::canvas_impl::Canvas;
use crate::canvas::ops::{
    CanvasCameraBindingKind, CanvasEndpoint, CanvasRay, FragmentRevealStyle, Op, Segment,
};
use crate::canvas::types::{
    AxesConfig, LayoutOp, LayoutTreeSnapshot, LayoutWithin, ObjectSpec, SpawnKind,
};
use gaanim_text::prelude::{
    TextContent as StructuredTextContent, TextDirection as StructuredTextDirection,
    TextOverflow as StructuredTextOverflow, TextSpec as StructuredTextSpec,
    TextStyle as StructuredTextStyle, TextWrap as StructuredTextWrap,
};

use gaanim_animation::{
    AngleLabelPlacement, CurvatureOnCurve, DimensionLabelPlacement, EndpointAngle,
    EndpointDistance, EndpointFollow, NormalOnCurve, PointOnCurve, PositionBinding,
    RotationBinding, RotationTranslationBinding, TangentOnCurve, TracedPath, TrackingAngle,
    TrackingAnglePart, TrackingEndpoint, TrackingLine, TrackingRay, TrackingScalar,
    TrackingVectorHead, Updater,
};
use gaanim_math::{RateFunc, SpatialTransform};

fn sampled_expression_path(
    map: &gaanim_visualization::CoordinateMap2D,
    expression: &gaanim_expr::Expr,
    variable: &str,
    domain: (f64, f64),
    reveal: Option<&gaanim_expr::Expr>,
    sampling: gaanim_visualization::Sampling,
    context: &gaanim_expr::EvalContext,
) -> BezPath {
    let sampled_domain = if let Some(reveal) = reveal {
        let Ok(end) = reveal.eval(context) else {
            return BezPath::new();
        };
        let end = end.clamp(domain.0, domain.1);
        if end <= domain.0 {
            return BezPath::new();
        }
        (domain.0, end)
    } else {
        domain
    };
    gaanim_visualization::sample_expression(
        map,
        expression,
        variable,
        sampled_domain,
        sampling,
        context,
    )
    .map(|sampled| sampled.to_bez_path())
    .unwrap_or_default()
}

fn compile_tracking_scalar(
    expression: &gaanim_expr::Expr,
    id_map: &HashMap<ObjectId, ObjectId>,
    states: &MobjectStateMap,
) -> TrackingScalar {
    let parameters = expression
        .parameter_ids()
        .into_iter()
        .filter_map(|id| {
            id_map
                .get(&id)
                .and_then(|runtime| states.get(*runtime))
                .map(|state| (id, state.entity))
        })
        .collect();
    TrackingScalar {
        expression: expression.clone(),
        parameters,
    }
}

fn compile_tracking_endpoint(
    endpoint: &CanvasEndpoint,
    id_map: &HashMap<ObjectId, ObjectId>,
    states: &MobjectStateMap,
) -> TrackingEndpoint {
    match endpoint {
        CanvasEndpoint::Static(position) => TrackingEndpoint::Static(*position),
        CanvasEndpoint::Entity(id) => id_map
            .get(id)
            .and_then(|runtime| states.get(*runtime))
            .map(|state| TrackingEndpoint::Entity(state.entity))
            .unwrap_or(TrackingEndpoint::Static(DVec3::ZERO)),
        CanvasEndpoint::Anchor(anchor) => id_map
            .get(&anchor.object)
            .and_then(|runtime| states.get(*runtime))
            .map(|state| TrackingEndpoint::EntityAnchor {
                entity: state.entity,
                normalized: anchor.normalized,
                offset: anchor.offset,
            })
            .unwrap_or(TrackingEndpoint::Static(DVec3::ZERO)),
        CanvasEndpoint::Expression { x, y } => TrackingEndpoint::Expression {
            x: compile_tracking_scalar(x, id_map, states),
            y: compile_tracking_scalar(y, id_map, states),
        },
        CanvasEndpoint::LocalExpression { space, x, y, z } => id_map
            .get(space)
            .and_then(|runtime| states.get(*runtime))
            .map(|state| TrackingEndpoint::LocalExpression {
                space: state.entity,
                x: compile_tracking_scalar(x, id_map, states),
                y: compile_tracking_scalar(y, id_map, states),
                z: compile_tracking_scalar(z, id_map, states),
            })
            .unwrap_or(TrackingEndpoint::Static(DVec3::ZERO)),
        CanvasEndpoint::Offset { origin, dx, dy } => TrackingEndpoint::Offset {
            origin: Box::new(compile_tracking_endpoint(origin, id_map, states)),
            dx: compile_tracking_scalar(dx, id_map, states),
            dy: compile_tracking_scalar(dy, id_map, states),
        },
        CanvasEndpoint::Between {
            from,
            to,
            alpha,
            offset,
        } => TrackingEndpoint::Between {
            from: Box::new(compile_tracking_endpoint(from, id_map, states)),
            to: Box::new(compile_tracking_endpoint(to, id_map, states)),
            alpha: *alpha,
            offset: *offset,
        },
        CanvasEndpoint::Polar {
            origin,
            radius,
            angle,
        } => TrackingEndpoint::Polar {
            origin: Box::new(compile_tracking_endpoint(origin, id_map, states)),
            radius: compile_tracking_scalar(radius, id_map, states),
            angle: compile_tracking_scalar(angle, id_map, states),
        },
    }
}

fn compile_tracking_ray(
    ray: &CanvasRay,
    id_map: &HashMap<ObjectId, ObjectId>,
    states: &MobjectStateMap,
) -> TrackingRay {
    match ray {
        CanvasRay::Direction(direction) => TrackingRay::Direction(*direction),
        CanvasRay::Endpoint(endpoint) => TrackingRay::Endpoint(Box::new(
            compile_tracking_endpoint(endpoint, id_map, states),
        )),
    }
}

#[derive(Clone)]
struct CompiledTextMeasure {
    spec: StructuredTextSpec,
    font_size: f64,
    font_family: String,
    math_font: String,
    color: PenikoColor,
}

struct CompiledLayoutMeasure<'a> {
    fixed: BTreeMap<gaanim_layout::LayoutId, DVec2>,
    texts: BTreeMap<gaanim_layout::LayoutId, CompiledTextMeasure>,
    text_composition_widths: RefCell<BTreeMap<gaanim_layout::LayoutId, f64>>,
    font_registry: &'a gaanim_text::font::FontRegistry,
}

impl gaanim_layout::IntrinsicMeasure for CompiledLayoutMeasure<'_> {
    fn measure(
        &self,
        id: gaanim_layout::LayoutId,
        constraints: gaanim_layout::BoxConstraints,
    ) -> Result<DVec2, gaanim_layout::LayoutError> {
        let Some(text) = self.texts.get(&id) else {
            return Ok(constraints.constrain(*self.fixed.get(&id).unwrap_or(&DVec2::ZERO)));
        };
        let offered_width = if constraints.max.x.is_finite() {
            constraints.max.x.max(1.0)
        } else {
            640.0
        };
        let composition_width = match text.spec.flow.wrap {
            StructuredTextWrap::NoWrap => None,
            StructuredTextWrap::Auto => Some(offered_width),
            StructuredTextWrap::Width(limit) => Some(offered_width.min(limit).max(1.0)),
        };
        if let Some(width) = composition_width {
            self.text_composition_widths.borrow_mut().insert(id, width);
        }
        let source = structured_text_typst_source(
            &text.spec,
            Some(offered_width),
            text.font_size,
            &text.font_family,
            text.color,
        );
        let bounds = gaanim_text::prelude::measure_typst(
            self.font_registry,
            &source,
            false,
            Some(&text.font_family),
            Some(&text.math_font),
            Some(text.font_size),
            None,
            Some(gaanim_core::peniko::Brush::Solid(text.color)),
            StrokeBrush::transparent(),
        )
        .map_err(|errors| gaanim_layout::LayoutError::Measure {
            id,
            message: errors.join("; "),
        })?;
        Ok(constraints.constrain(DVec2::new(
            bounds.width().max(0.0),
            bounds.height().max(0.0),
        )))
    }

    fn is_width_sensitive(&self, id: gaanim_layout::LayoutId) -> bool {
        self.texts
            .get(&id)
            .is_some_and(|text| !matches!(text.spec.flow.wrap, StructuredTextWrap::NoWrap))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CompiledObjectScope {
    Segment(SceneId),
    Persistent,
}

#[derive(Clone, Copy, Debug)]
enum SceneObjectScopeAction {
    Reuse,
    Persist,
    Release,
}

fn escape_typst_string(text: &str) -> String {
    text.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "")
}

fn typst_foreground_for_background(background: gaanim_core::peniko::Color) -> &'static str {
    let rgba = background.to_rgba8();
    let luminance =
        (0.2126 * f64::from(rgba.r) + 0.7152 * f64::from(rgba.g) + 0.0722 * f64::from(rgba.b))
            / 255.0;
    if luminance > 0.5 { "000000" } else { "ffffff" }
}

pub(crate) fn split_text_math(text: &str) -> Vec<(bool, String)> {
    let mut segments: Vec<(bool, String)> = Vec::new();
    let mut buf = String::new();
    let mut in_math = false;
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\\' {
            if let Some(&next) = chars.peek() {
                if next == '$' {
                    chars.next();
                    buf.push('$');
                } else if next == '\\' {
                    chars.next();
                    buf.push('\\');
                } else {
                    buf.push(c);
                }
            } else {
                buf.push(c);
            }
        } else if c == '$' {
            let is_double = chars.peek() == Some(&'$');
            if is_double {
                chars.next();
            }
            if !in_math {
                if !buf.is_empty() {
                    segments.push((false, std::mem::take(&mut buf)));
                }
                in_math = true;
            } else {
                if !buf.is_empty() {
                    segments.push((true, std::mem::take(&mut buf)));
                } else {
                    segments.push((true, String::new()));
                }
                in_math = false;
            }
        } else {
            buf.push(c);
        }
    }
    if !buf.is_empty() {
        if in_math {
            // Unclosed `$` — treat the opening delimiter and trailing content as literal text.
            segments.push((false, format!("${buf}")));
        } else {
            segments.push((false, buf));
        }
    }
    if segments.is_empty() {
        segments.push((false, String::new()));
    }
    segments
}

pub(crate) fn typst_inline_content(text: &str) -> String {
    if !text.contains('$') {
        return format!("#text(\"{}\")", escape_typst_string(text));
    }
    let segments = split_text_math(text);
    let has_math = segments.iter().any(|(is_math, _)| *is_math);
    if !has_math {
        return format!("#text(\"{}\")", escape_typst_string(text));
    }
    let mut parts = Vec::new();
    for (is_math, content) in segments {
        if content.is_empty() {
            continue;
        }
        if is_math {
            let trimmed = content.trim();
            if trimmed.is_empty() {
                continue;
            }
            parts.push(format!("${}$", trimmed));
        } else {
            parts.push(format!("#text(\"{}\")", escape_typst_string(&content)));
        }
    }
    if parts.is_empty() {
        return format!("#text(\"{}\")", escape_typst_string(text));
    }
    parts.join("")
}

fn merge_text_style(
    base: &StructuredTextStyle,
    overlay: &StructuredTextStyle,
) -> StructuredTextStyle {
    StructuredTextStyle {
        font: overlay.font.clone().or_else(|| base.font.clone()),
        math_font: overlay.math_font.clone().or_else(|| base.math_font.clone()),
        fallbacks: if overlay.fallbacks.is_empty() {
            base.fallbacks.clone()
        } else {
            overlay.fallbacks.clone()
        },
        size: overlay.size.or(base.size),
        weight: overlay.weight.or(base.weight),
        italic: overlay.italic.or(base.italic),
        color: overlay.color.or(base.color),
        stroke_color: overlay.stroke_color.or(base.stroke_color),
        stroke_width: overlay.stroke_width.or(base.stroke_width),
        opacity: overlay.opacity.or(base.opacity),
        letter_spacing: overlay.letter_spacing.or(base.letter_spacing),
        word_spacing: overlay.word_spacing.or(base.word_spacing),
        decorations: if overlay.decorations.is_empty() {
            base.decorations.clone()
        } else {
            overlay.decorations.clone()
        },
        baseline: overlay.baseline.or(base.baseline),
    }
}

fn collect_styled_text_leaves(
    content: &[StructuredTextContent],
    inherited: &StructuredTextStyle,
    leaves: &mut Vec<(String, StructuredTextStyle)>,
) {
    for node in content {
        match node {
            StructuredTextContent::Literal(text) => leaves.push((text.clone(), inherited.clone())),
            StructuredTextContent::Part(part) => {
                let style = merge_text_style(inherited, &part.style);
                collect_styled_text_leaves(&part.content, &style, leaves);
            }
        }
    }
}

fn typst_style_arguments(style: &StructuredTextStyle) -> Vec<String> {
    let mut args = Vec::new();
    if let Some(font) = &style.font {
        let font = escape_typst_string(font);
        args.push(format!("font: \"{font}\""));
    }
    if let Some(size) = style.size {
        args.push(format!("size: {size}pt"));
    }
    if let Some(weight) = style.weight {
        args.push(format!("weight: {weight}"));
    }
    if style.italic == Some(true) {
        args.push("style: \"italic\"".to_string());
    }
    if let Some(color) = style.color {
        args.push(format!("fill: rgb(\"{}\")", color_to_hex(color)));
    }
    if let Some(spacing) = style.letter_spacing {
        args.push(format!("tracking: {spacing}pt"));
    }
    if let Some(spacing) = style.word_spacing {
        args.push(format!("spacing: {spacing}pt"));
    }
    args
}

fn styled_typst_chunk(text: &str, math: bool, style: &StructuredTextStyle) -> String {
    if text.is_empty() {
        return String::new();
    }
    let mut wrapper_style = style.clone();
    let content = if math {
        let text = text.trim();
        if text.is_empty() {
            return String::new();
        }
        // Math glyphs do not inherit an outer `#text(fill: ...)` wrapper.
        // Typst's math-mode `text` function accepts mathematical content and
        // applies its paint to every resulting glyph and shape.
        let text = if let Some(color) = style.color {
            wrapper_style.color = None;
            format!("text(fill: #rgb(\"{}\"), {text})", color_to_hex(color))
        } else {
            text.to_owned()
        };
        format!("${text}$")
    } else {
        format!("#text(\"{}\")", escape_typst_string(text))
    };
    let args = typst_style_arguments(&wrapper_style);
    let mut content = if args.is_empty() {
        content
    } else {
        format!("#text({})[{content}]", args.join(", "))
    };
    for decoration in &style.decorations {
        content = match decoration.as_str() {
            "underline" => format!("#underline[{content}]"),
            "strike" | "strikethrough" => format!("#strike[{content}]"),
            _ => content,
        };
    }
    if let Some(baseline) = style.baseline.filter(|value| *value != 0.0) {
        content = format!("#move(dy: {}pt)[{content}]", -baseline);
    }
    content
}

/// Compile structured leaves without discarding the semantic style stack.
/// Math delimiters are tracked across part boundaries, so a styled nested
/// part may safely live inside a `$...$` expression.
fn structured_typst_content(spec: &StructuredTextSpec, font_size: f64) -> String {
    let mut raw_leaves = Vec::new();
    collect_styled_text_leaves(
        &spec.content,
        &StructuredTextStyle::default(),
        &mut raw_leaves,
    );
    let mut markup = gaanim_text::structured::InlineMarkupParser::new();
    let mut marked_leaves = Vec::new();
    for (text, inherited_style) in raw_leaves {
        for segment in markup
            .push(&text)
            .expect("TextSpec validates inline markup before compilation")
        {
            if segment.text.is_empty() {
                continue;
            }
            let mut style = inherited_style.clone();
            if segment.strong {
                style.weight = Some(style.weight.unwrap_or(400).max(700));
            }
            if segment.emphasis {
                style.italic = Some(true);
            }
            marked_leaves.push((segment.text, style));
        }
    }
    markup
        .finish()
        .expect("TextSpec validates inline markup before compilation");
    let mut raw_leaves = marked_leaves;
    // Typst ignores ordinary spaces inside math. At compositional boundaries,
    // however, users naturally write `part("x"), " dot ..."` and expect that
    // leading/trailing space to remain visible. Record those boundaries so we
    // can emit a non-weak gap between inline math runs. Keeping the gap outside
    // each run prevents Typst from discarding it at the edge of a locally
    // styled part. Use an absolute length derived from the resolved font size:
    // markup outside math would otherwise resolve `em` against Typst's default
    // text size instead of this Text's configured size.
    let mut gap_before = vec![false; raw_leaves.len()];
    let mut in_math = false;
    let mut escaped = false;
    for index in 0..raw_leaves.len().saturating_sub(1) {
        for character in raw_leaves[index].0.chars() {
            if escaped {
                escaped = false;
            } else if character == '\\' {
                escaped = true;
            } else if character == '$' {
                in_math = !in_math;
            }
        }
        if !in_math {
            continue;
        }
        let left_has_space = raw_leaves[index]
            .0
            .ends_with(|character| character == ' ' || character == '\t');
        let right_has_space = raw_leaves[index + 1]
            .0
            .starts_with(|character| character == ' ' || character == '\t');
        if left_has_space || right_has_space {
            let trimmed_left_len = raw_leaves[index].0.trim_end_matches([' ', '\t']).len();
            raw_leaves[index].0.truncate(trimmed_left_len);
            raw_leaves[index + 1].0 = raw_leaves[index + 1]
                .0
                .trim_start_matches([' ', '\t'])
                .to_string();
            gap_before[index + 1] = true;
        }
    }
    // A semantic part must not become a typographic boundary. Coalesce
    // adjacent leaves with the same resolved style so a structured equation
    // is shaped exactly like the equivalent single string, including math
    // operator spacing and kerning.
    let mut leaves: Vec<(String, StructuredTextStyle, bool)> = Vec::new();
    for ((text, style), gap_before) in raw_leaves.into_iter().zip(gap_before) {
        if let Some((previous, previous_style, _)) = leaves.last_mut()
            && !gap_before
            && *previous_style == style
        {
            previous.push_str(&text);
        } else {
            leaves.push((text, style, gap_before));
        }
    }
    let mut output = String::new();
    let mut in_math = false;
    let append_chunk = |output: &mut String, chunk: String| {
        if chunk.is_empty() {
            return;
        }
        // Two adjacent inline-math fragments would produce `$$`, which Typst
        // parses as block math. A zero-width markup separator preserves the
        // inline flow and keeps semantic part boundaries measurable.
        if output.ends_with('$') && chunk.starts_with('$') {
            output.push_str("#h(0pt)");
        }
        output.push_str(&chunk);
    };
    for (text, style, gap_before) in leaves {
        if gap_before && in_math {
            output.push_str(&format!("#h({}pt, weak: false)", font_size * 0.28));
        }
        let mut chunk = String::new();
        let mut escaped = false;
        for character in text.chars() {
            if escaped {
                if character == '$' {
                    chunk.push('$');
                } else {
                    chunk.push('\\');
                    chunk.push(character);
                }
                escaped = false;
                continue;
            }
            if character == '\\' {
                escaped = true;
                continue;
            }
            if character == '$' {
                append_chunk(&mut output, styled_typst_chunk(&chunk, in_math, &style));
                chunk.clear();
                in_math = !in_math;
            } else {
                chunk.push(character);
            }
        }
        if escaped {
            chunk.push('\\');
        }
        append_chunk(&mut output, styled_typst_chunk(&chunk, in_math, &style));
    }
    output
}

fn color_to_hex(color: gaanim_core::peniko::Color) -> String {
    let rgba = color.to_rgba8();
    format!("{:02x}{:02x}{:02x}", rgba.r, rgba.g, rgba.b)
}

pub(crate) fn text_inline_typst_source(text: &str, color: gaanim_core::peniko::Color) -> String {
    let content = typst_inline_content(text);
    let hex = color_to_hex(color);
    format!(
        "#set page(width: auto, height: auto, margin: 0pt)\n\
         #set text(fill: rgb(\"{hex}\"))\n\
         #align(left)[{content}]"
    )
}

/// Compose every structured text role through one Typst vector pipeline. The
/// optional width is the offer from Layout v2 (or the scene safe frame for a
/// free text object); it is never stored as an outer text box.
fn structured_text_typst_source(
    spec: &StructuredTextSpec,
    offered_width: Option<f64>,
    font_size: f64,
    font_family: &str,
    color: gaanim_core::peniko::Color,
) -> String {
    let width = match spec.flow.wrap {
        StructuredTextWrap::NoWrap => None,
        StructuredTextWrap::Auto => offered_width.map(|width| width.max(1.0)),
        StructuredTextWrap::Width(limit) => Some(
            offered_width
                .map(|width| width.min(limit))
                .unwrap_or(limit)
                .max(1.0),
        ),
    };
    let page_width = width
        .map(|width| format!("{width}pt"))
        .unwrap_or_else(|| "auto".to_string());
    let leading = font_size * (spec.flow.line_spacing.max(0.1) - 1.0);
    let (alignment, justify) = match spec.flow.align {
        gaanim_text::prelude::TextAlign::Left => ("left", false),
        gaanim_text::prelude::TextAlign::Center => ("center", false),
        gaanim_text::prelude::TextAlign::Right => ("right", false),
        gaanim_text::prelude::TextAlign::Justify => ("left", true),
    };
    let direction = match spec.flow.direction {
        StructuredTextDirection::Auto => "auto",
        StructuredTextDirection::Ltr => "ltr",
        StructuredTextDirection::Rtl => "rtl",
    };
    let weight = spec
        .style
        .weight
        .map(|weight| format!(", weight: {weight}"))
        .unwrap_or_default();
    let italic = if spec.style.italic == Some(true) {
        ", style: \"italic\""
    } else {
        ""
    };
    let tracking = spec
        .style
        .letter_spacing
        .map(|spacing| format!(", tracking: {spacing}pt"))
        .unwrap_or_default();
    let family = spec.style.font.as_deref().unwrap_or(font_family);
    let font = if spec.style.fallbacks.is_empty() {
        format!("font: \"{}\", ", escape_typst_string(family))
    } else {
        let families = std::iter::once(family)
            .chain(spec.style.fallbacks.iter().map(String::as_str))
            .map(|family| format!("\"{}\"", escape_typst_string(family)))
            .collect::<Vec<_>>()
            .join(", ");
        format!("font: ({families}), ")
    };
    let hex = color_to_hex(color);
    let content = structured_typst_content(spec, font_size);
    let content = format!("#align({alignment})[{content}]");
    let content = if let Some(max_lines) = spec.flow.max_lines {
        let height = font_size * spec.flow.line_spacing.max(0.1) * max_lines as f64;
        let clip = !matches!(spec.flow.overflow, StructuredTextOverflow::Visible);
        // Typst currently supplies the clip for both clip and ellipsis. The
        // structured overflow value remains distinct in the cache/spec so a
        // renderer-level ellipsis marker can be added without an API change.
        format!("#block(width: 100%, height: {height}pt, clip: {clip})[{content}]")
    } else {
        content
    };
    format!(
        "#set page(width: {page_width}, height: auto, margin: 0pt)\n\
         #set text({font}fill: rgb(\"{hex}\"), dir: {direction}, hyphenate: {}{weight}{italic}{tracking})\n\
         #set par(justify: {justify}, leading: {leading}pt)\n\
         {content}",
        spec.flow.hyphenate,
    )
}

fn compiled_text_measure(
    object: &ObjectSpec,
    text_config: &gaanim_text::prelude::TextConfig,
) -> Option<CompiledTextMeasure> {
    let SpawnKind::Text(spec) = &object.kind else {
        return None;
    };
    let role = &text_config.roles[&spec.role];
    let math = &text_config.roles[&gaanim_text::prelude::TextRole::Math];
    let color = match &object.fill {
        Some(gaanim_core::peniko::Brush::Solid(color)) => *color,
        _ => spec.style.color.unwrap_or(role.fill_color),
    };
    Some(CompiledTextMeasure {
        spec: spec.clone(),
        font_size: spec.style.size.unwrap_or(role.size).max(1.0),
        font_family: spec
            .style
            .font
            .clone()
            .unwrap_or_else(|| role.font_family.clone()),
        math_font: spec
            .style
            .math_font
            .clone()
            .unwrap_or_else(|| math.font_family.clone()),
        color,
    })
}

struct CompiledLayoutTree {
    root: gaanim_layout::LayoutNode,
    source_by_id: BTreeMap<gaanim_layout::LayoutId, ObjectId>,
    parent_by_id: BTreeMap<gaanim_layout::LayoutId, gaanim_layout::LayoutId>,
    item_style_by_id: BTreeMap<gaanim_layout::LayoutId, gaanim_layout::LayoutItemStyle>,
    children_by_id: BTreeMap<gaanim_layout::LayoutId, Vec<gaanim_layout::LayoutId>>,
    fixed: BTreeMap<gaanim_layout::LayoutId, DVec2>,
    texts: BTreeMap<gaanim_layout::LayoutId, CompiledTextMeasure>,
}

fn collect_compiled_layout_node(
    source: ObjectId,
    snapshots: &HashMap<ObjectId, LayoutTreeSnapshot>,
    id_map: &HashMap<ObjectId, ObjectId>,
    states: &MobjectStateMap,
    object_specs: &HashMap<ObjectId, ObjectSpec>,
    text_config: &gaanim_text::prelude::TextConfig,
    source_by_id: &mut BTreeMap<gaanim_layout::LayoutId, ObjectId>,
    parent_by_id: &mut BTreeMap<gaanim_layout::LayoutId, gaanim_layout::LayoutId>,
    item_style_by_id: &mut BTreeMap<gaanim_layout::LayoutId, gaanim_layout::LayoutItemStyle>,
    children_by_id: &mut BTreeMap<gaanim_layout::LayoutId, Vec<gaanim_layout::LayoutId>>,
    fixed: &mut BTreeMap<gaanim_layout::LayoutId, DVec2>,
    texts: &mut BTreeMap<gaanim_layout::LayoutId, CompiledTextMeasure>,
    visiting: &mut HashSet<ObjectId>,
) -> Option<gaanim_layout::LayoutNode> {
    assert!(
        visiting.insert(source),
        "layout ownership cycle involving {source:?}"
    );
    let Some(actual) = id_map.get(&source).copied() else {
        visiting.remove(&source);
        return None;
    };
    let Some(state) = states.get(actual) else {
        visiting.remove(&source);
        return None;
    };
    let id = gaanim_layout::LayoutId(actual.as_raw());
    source_by_id.insert(id, source);

    let node = if let Some(snapshot) = snapshots.get(&source) {
        let mut children = Vec::new();
        let mut child_ids = Vec::new();
        for member in &snapshot.members {
            let Some(child) = collect_compiled_layout_node(
                member.id,
                snapshots,
                id_map,
                states,
                object_specs,
                text_config,
                source_by_id,
                parent_by_id,
                item_style_by_id,
                children_by_id,
                fixed,
                texts,
                visiting,
            ) else {
                continue;
            };
            parent_by_id.insert(child.id, id);
            item_style_by_id.insert(child.id, member.style.clone());
            child_ids.push(child.id);
            children.push(gaanim_layout::LayoutChild {
                node: Box::new(child),
                style: member.style.clone(),
            });
        }
        children_by_id.insert(id, child_ids);
        let mut node =
            gaanim_layout::LayoutNode::container(id, snapshot.spec.kind.clone(), children);
        node.style = snapshot.spec.style.clone();
        node
    } else {
        let mut transform = state.transform;
        transform.translation = DVec3::ZERO;
        let bounds = gaanim_layout::transform_bounds(state.bounds, &transform);
        fixed.insert(
            id,
            DVec2::new(bounds.width().max(0.0), bounds.height().max(0.0)),
        );
        if let Some(text) = object_specs
            .get(&source)
            .and_then(|spec| compiled_text_measure(spec, text_config))
        {
            texts.insert(id, text);
        }
        gaanim_layout::LayoutNode::leaf(id)
    };
    visiting.remove(&source);
    Some(node)
}

fn compile_layout_tree(
    root_source: ObjectId,
    snapshots: &HashMap<ObjectId, LayoutTreeSnapshot>,
    id_map: &HashMap<ObjectId, ObjectId>,
    states: &MobjectStateMap,
    object_specs: &HashMap<ObjectId, ObjectSpec>,
    text_config: &gaanim_text::prelude::TextConfig,
) -> Option<CompiledLayoutTree> {
    let mut source_by_id = BTreeMap::new();
    let mut parent_by_id = BTreeMap::new();
    let mut item_style_by_id = BTreeMap::new();
    let mut children_by_id = BTreeMap::new();
    let mut fixed = BTreeMap::new();
    let mut texts = BTreeMap::new();
    let root = collect_compiled_layout_node(
        root_source,
        snapshots,
        id_map,
        states,
        object_specs,
        text_config,
        &mut source_by_id,
        &mut parent_by_id,
        &mut item_style_by_id,
        &mut children_by_id,
        &mut fixed,
        &mut texts,
        &mut HashSet::new(),
    )?;
    Some(CompiledLayoutTree {
        root,
        source_by_id,
        parent_by_id,
        item_style_by_id,
        children_by_id,
        fixed,
        texts,
    })
}

fn outermost_layout_source(
    source: ObjectId,
    snapshots: &HashMap<ObjectId, LayoutTreeSnapshot>,
    object_specs: &HashMap<ObjectId, ObjectSpec>,
) -> ObjectId {
    let mut current = source;
    let mut visited = HashSet::from([source]);
    while let Some(owner) = object_specs
        .get(&current)
        .and_then(|spec| spec.layout_owner)
        .filter(|owner| snapshots.contains_key(owner))
    {
        assert!(
            visited.insert(owner),
            "layout ownership cycle involving {owner:?}"
        );
        current = owner;
    }
    current
}

impl Canvas {
    fn axis_path(
        builder: &mut SceneBuilder<'_, '_, '_>,
        path: gaanim_core::kurbo::BezPath,
        bounds: Bounds3D,
        color: PenikoColor,
        width: f64,
        tag: &str,
    ) -> MobjectRef {
        let path = gaanim_objects::prelude::SvgPath {
            id: tag.to_owned(),
            path,
            bounds,
            fill: None,
            stroke: StrokeBrush::new(color, width),
        };
        builder.svg_path(&path).spawn()
    }

    fn axis_text(
        builder: &mut SceneBuilder<'_, '_, '_>,
        text: &str,
        x: f64,
        y: f64,
        color: PenikoColor,
        size: Option<f64>,
    ) -> MobjectRef {
        let role = gaanim_text::prelude::TextRole::Body;
        let label = builder.spawn_text(text, role);
        let base_size = builder.text_config.roles[&role].size.max(1.0);
        if let Some(state) = builder.states.get_mut(label.id) {
            state.transform = state.transform.shift_2d(x, y);
            if let Some(size) = size {
                state.transform.scale *= size / base_size;
            }
            builder
                .commands
                .entity(state.entity)
                .insert(state.transform);
        }
        builder.select(label, text).set_fill(color);
        label
    }

    fn styled_axes(
        builder: &mut SceneBuilder<'_, '_, '_>,
        x_range: (f64, f64, f64),
        y_range: (f64, f64, f64),
        config: &AxesConfig,
        frame_bounds: Bounds3D,
    ) -> MobjectRef {
        let (x_min, x_max, x_step) = x_range;
        let (y_min, y_max, y_step) = y_range;
        // Manim-compatible sizing: x_length/y_length override auto_fit, otherwise map to safe_frame.
        // x_length/y_length are manim units (default frame 14.222x8), convert to scene units via avail_w/h
        let avail_w = frame_bounds.width().max(1.0);
        let avail_h = frame_bounds.height().max(1.0);
        let manim_frame_w: f64 = 14.222222222222221;
        let manim_frame_h: f64 = 8.0;
        let (scale_x, scale_y, x_center, y_center) = match (config.x_length, config.y_length) {
            (Some(xl), Some(yl)) => {
                let scene_xl = xl * avail_w / manim_frame_w;
                let scene_yl = yl * avail_h / manim_frame_h;
                let sx = scene_xl / (x_max - x_min).max(1e-9);
                let sy = scene_yl / (y_max - y_min).max(1e-9);
                (sx, sy, (x_min + x_max) * 0.5, (y_min + y_max) * 0.5)
            }
            (Some(xl), None) => {
                let scene_xl = xl * avail_w / manim_frame_w;
                let s = scene_xl / (x_max - x_min).max(1e-9);
                (s, s, (x_min + x_max) * 0.5, (y_min + y_max) * 0.5)
            }
            (None, Some(yl)) => {
                let scene_yl = yl * avail_h / manim_frame_h;
                let s = scene_yl / (y_max - y_min).max(1e-9);
                (s, s, (x_min + x_max) * 0.5, (y_min + y_max) * 0.5)
            }
            (None, None) if config.auto_fit => {
                let data_w = (x_max - x_min).max(1e-9);
                let data_h = (y_max - y_min).max(1e-9);
                let s = (avail_w / data_w).min(avail_h / data_h);
                (s, s, (x_min + x_max) * 0.5, (y_min + y_max) * 0.5)
            }
            (None, None) => (1.0, 1.0, 0.0, 0.0),
        };
        let sx = |x: f64| (x - x_center) * scale_x;
        let sy = |y: f64| (y - y_center) * scale_y;
        let bounds = if config.auto_fit || config.x_length.is_some() || config.y_length.is_some() {
            Bounds3D::new_2d(sx(x_min), sy(y_min), sx(x_max), sy(y_max))
        } else {
            Bounds3D::new_2d(x_min, y_min, x_max, y_max)
        };
        let mut children = Vec::new();
        let x_axis_in_range = y_min <= 0.0 && y_max >= 0.0;
        let y_axis_in_range = x_min <= 0.0 && x_max >= 0.0;

        let mut grid = gaanim_core::kurbo::BezPath::new();
        if config.grid && config.x_grid {
            let mut x = (x_min / x_step).ceil() * x_step;
            while x <= x_max + 1e-9 {
                if x.abs() > 1e-9 {
                    grid.move_to(Point::new(sx(x), sy(y_min)));
                    grid.line_to(Point::new(sx(x), sy(y_max)));
                }
                x += x_step;
            }
        }
        if config.grid && config.y_grid {
            let mut y = (y_min / y_step).ceil() * y_step;
            while y <= y_max + 1e-9 {
                if y.abs() > 1e-9 {
                    grid.move_to(Point::new(sx(x_min), sy(y)));
                    grid.line_to(Point::new(sx(x_max), sy(y)));
                }
                y += y_step;
            }
        }
        if !grid.elements().is_empty() {
            children.push(Self::axis_path(
                builder,
                grid,
                bounds,
                config.grid_color,
                config.grid_width,
                "AxesGrid",
            ));
        }

        let mut axes = gaanim_core::kurbo::BezPath::new();
        if config.x_axis && x_axis_in_range {
            axes.move_to(Point::new(sx(x_min), sy(0.0)));
            axes.line_to(Point::new(sx(x_max), sy(0.0)));
        }
        if config.y_axis && y_axis_in_range {
            axes.move_to(Point::new(sx(0.0), sy(y_min)));
            axes.line_to(Point::new(sx(0.0), sy(y_max)));
        }
        if !axes.elements().is_empty() {
            children.push(Self::axis_path(
                builder,
                axes,
                bounds,
                config.axis_color,
                config.axis_width,
                "AxesLines",
            ));
        }
        // Manim-like arrow tips at positive ends
        if config.tips {
            let tip_len: f64 = 10.0;
            let tip_half_w: f64 = 5.0;
            if config.x_axis && x_axis_in_range {
                let mut tip = gaanim_core::kurbo::BezPath::new();
                let tx = sx(x_max);
                let ty = sy(0.0);
                tip.move_to(Point::new(tx + tip_len, ty));
                tip.line_to(Point::new(tx - tip_len * 0.3, ty - tip_half_w));
                tip.line_to(Point::new(tx - tip_len * 0.3, ty + tip_half_w));
                tip.close_path();
                let tip_path = gaanim_objects::prelude::SvgPath {
                    id: "AxesTips".to_string(),
                    path: tip,
                    bounds,
                    fill: Some(gaanim_core::peniko::Brush::Solid(config.axis_color)),
                    stroke: StrokeBrush::transparent(),
                };
                children.push(builder.svg_path(&tip_path).spawn());
            }
            if config.y_axis && y_axis_in_range {
                let mut tip = gaanim_core::kurbo::BezPath::new();
                let tx = sx(0.0);
                let ty = sy(y_max);
                tip.move_to(Point::new(tx, ty + tip_len));
                tip.line_to(Point::new(tx - tip_half_w, ty - tip_len * 0.3));
                tip.line_to(Point::new(tx + tip_half_w, ty - tip_len * 0.3));
                tip.close_path();
                let tip_path = gaanim_objects::prelude::SvgPath {
                    id: "AxesTips".to_string(),
                    path: tip,
                    bounds,
                    fill: Some(gaanim_core::peniko::Brush::Solid(config.axis_color)),
                    stroke: StrokeBrush::transparent(),
                };
                children.push(builder.svg_path(&tip_path).spawn());
            }
        }

        let tick_half = config.tick_length * 0.5;
        let mut ticks = gaanim_core::kurbo::BezPath::new();
        if config.ticks && config.x_ticks && x_axis_in_range {
            let mut x = (x_min / x_step).ceil() * x_step;
            while x <= x_max + 1e-9 {
                ticks.move_to(Point::new(sx(x), sy(0.0) - tick_half));
                ticks.line_to(Point::new(sx(x), sy(0.0) + tick_half));
                x += x_step;
            }
        }
        if config.ticks && config.y_ticks && y_axis_in_range {
            let mut y = (y_min / y_step).ceil() * y_step;
            while y <= y_max + 1e-9 {
                ticks.move_to(Point::new(sx(0.0) - tick_half, sy(y)));
                ticks.line_to(Point::new(sx(0.0) + tick_half, sy(y)));
                y += y_step;
            }
        }
        if !ticks.elements().is_empty() {
            children.push(Self::axis_path(
                builder,
                ticks,
                bounds,
                config.tick_color,
                config.tick_width,
                "AxesTicks",
            ));
        }

        if config.numbers && config.x_numbers && x_axis_in_range {
            let mut x = (x_min / x_step).ceil() * x_step;
            while x <= x_max + 1e-9 {
                let value = if x.abs() < 1e-9 { 0.0 } else { x };
                let text = format!("{value}");
                children.push(Self::axis_text(
                    builder,
                    &text,
                    sx(x),
                    sy(0.0) - tick_half - 14.0,
                    config.number_color,
                    config.number_size,
                ));
                x += x_step;
            }
        }
        if config.numbers && config.y_numbers && y_axis_in_range {
            let mut y = (y_min / y_step).ceil() * y_step;
            while y <= y_max + 1e-9 {
                if y.abs() > 1e-9 || !config.x_numbers {
                    let value = if y.abs() < 1e-9 { 0.0 } else { y };
                    let text = format!("{value}");
                    let estimated_width = text.chars().count() as f64 * 9.0;
                    children.push(Self::axis_text(
                        builder,
                        &text,
                        sx(0.0) - tick_half - 8.0 - estimated_width * 0.5,
                        sy(y),
                        config.number_color,
                        config.number_size,
                    ));
                }
                y += y_step;
            }
        }

        if config.labels {
            if let Some(label) = config.x_label.as_deref().filter(|_| x_axis_in_range) {
                children.push(Self::axis_text(
                    builder,
                    label,
                    sx(x_max) + 18.0,
                    sy(0.0) - tick_half - 2.0,
                    config.label_color,
                    config.label_size,
                ));
            }
            if let Some(label) = config.y_label.as_deref().filter(|_| y_axis_in_range) {
                children.push(Self::axis_text(
                    builder,
                    label,
                    sx(0.0) + tick_half + 12.0,
                    sy(y_max) + 12.0,
                    config.label_color,
                    config.label_size,
                ));
            }
        }

        builder.group(&children)
    }

    fn styled_axes_3d(
        builder: &mut SceneBuilder<'_, '_, '_>,
        x_range: (f64, f64, f64),
        y_range: (f64, f64, f64),
        z_range: (f64, f64, f64),
        config: &crate::canvas::types::Axes3DConfig,
        _frame_bounds: Bounds3D,
    ) -> MobjectRef {
        let (x_min, x_max, x_step) = x_range;
        let (y_min, y_max, y_step) = y_range;
        let (z_min, z_max, z_step) = z_range;
        let (scale_x, scale_y, scale_z, x_center, y_center, z_center) =
            match (config.x_length, config.y_length, config.z_length) {
                (Some(xl), Some(yl), Some(zl)) => {
                    let sx = xl / (x_max - x_min).max(1e-9);
                    let sy = yl / (y_max - y_min).max(1e-9);
                    let sz = zl / (z_max - z_min).max(1e-9);
                    (
                        sx,
                        sy,
                        sz,
                        (x_min + x_max) * 0.5,
                        (y_min + y_max) * 0.5,
                        (z_min + z_max) * 0.5,
                    )
                }
                _ => (1.0, 1.0, 1.0, 0.0, 0.0, 0.0),
            };
        let sx = |x: f64| (x - x_center) * scale_x;
        let sy = |y: f64| (y - y_center) * scale_y;
        let sz = |z: f64| (z - z_center) * scale_z;
        let bounds = Bounds3D::new_3d(
            sx(x_min),
            sy(y_min),
            sz(z_min),
            sx(x_max),
            sy(y_max),
            sz(z_max),
        );
        let mut children = Vec::new();

        // Helper to push a 3D line list
        let mut push_line_list = |points: Vec<[f32; 3]>, color: peniko::Color, _tag: &str| {
            if points.len() < 2 {
                return;
            }
            let mref = builder.spawn_line_list(points, color);
            // Tag for debugging
            if let Some(state) = builder.states.get_mut(mref.id) {
                state.bounds = bounds;
            }
            children.push(mref);
        };

        // 3 grid planes
        if config.grid {
            // XY plane at z=0 (if z range includes 0)
            if config.xy_grid && z_min <= 0.0 && z_max >= 0.0 {
                let z0 = sz(0.0) as f32;
                {
                    let mut x = (x_min / x_step).ceil() * x_step;
                    while x <= x_max + 1e-9 {
                        if x.abs() > 1e-9 {
                            let fx = sx(x) as f32;
                            let y0 = sy(y_min) as f32;
                            let y1 = sy(y_max) as f32;
                            let points = vec![[fx, y0, z0], [fx, y1, z0]];
                            push_line_list(points, config.grid_color, "Axes3DGridXY");
                        }
                        x += x_step;
                    }
                }
                {
                    let mut y = (y_min / y_step).ceil() * y_step;
                    while y <= y_max + 1e-9 {
                        if y.abs() > 1e-9 {
                            let fy = sy(y) as f32;
                            let x0 = sx(x_min) as f32;
                            let x1 = sx(x_max) as f32;
                            let points = vec![[x0, fy, z0], [x1, fy, z0]];
                            push_line_list(points, config.grid_color, "Axes3DGridXY");
                        }
                        y += y_step;
                    }
                }
            }
            // XZ plane at y=0
            if config.xz_grid && y_min <= 0.0 && y_max >= 0.0 {
                let y0 = sy(0.0) as f32;
                {
                    let mut x = (x_min / x_step).ceil() * x_step;
                    while x <= x_max + 1e-9 {
                        if x.abs() > 1e-9 {
                            let fx = sx(x) as f32;
                            let z0 = sz(z_min) as f32;
                            let z1 = sz(z_max) as f32;
                            let points = vec![[fx, y0, z0], [fx, y0, z1]];
                            push_line_list(points, config.grid_color, "Axes3DGridXZ");
                        }
                        x += x_step;
                    }
                }
                {
                    // reuse z step for grid density in XZ
                    let mut z = (z_min / z_step).ceil() * z_step;
                    while z <= z_max + 1e-9 {
                        if z.abs() > 1e-9 {
                            let fz = sz(z) as f32;
                            let x0 = sx(x_min) as f32;
                            let x1 = sx(x_max) as f32;
                            let points = vec![[x0, y0, fz], [x1, y0, fz]];
                            push_line_list(points, config.grid_color, "Axes3DGridXZ");
                        }
                        z += z_step;
                    }
                }
            }
            // YZ plane at x=0
            if config.yz_grid && x_min <= 0.0 && x_max >= 0.0 {
                let x0 = sx(0.0) as f32;
                {
                    let mut y = (y_min / y_step).ceil() * y_step;
                    while y <= y_max + 1e-9 {
                        if y.abs() > 1e-9 {
                            let fy = sy(y) as f32;
                            let z0 = sz(z_min) as f32;
                            let z1 = sz(z_max) as f32;
                            let points = vec![[x0, fy, z0], [x0, fy, z1]];
                            push_line_list(points, config.grid_color, "Axes3DGridYZ");
                        }
                        y += y_step;
                    }
                }
                {
                    let mut z = (z_min / z_step).ceil() * z_step;
                    while z <= z_max + 1e-9 {
                        if z.abs() > 1e-9 {
                            let fz = sz(z) as f32;
                            let y0 = sy(y_min) as f32;
                            let y1 = sy(y_max) as f32;
                            let points = vec![[x0, y0, fz], [x0, y1, fz]];
                            push_line_list(points, config.grid_color, "Axes3DGridYZ");
                        }
                        z += z_step;
                    }
                }
            }
        }

        // Axes lines (3)
        {
            let mut axes_points = Vec::new();
            if config.x_axis && y_min <= 0.0 && y_max >= 0.0 && z_min <= 0.0 && z_max >= 0.0 {
                axes_points.push([sx(x_min) as f32, sy(0.0) as f32, sz(0.0) as f32]);
                axes_points.push([sx(x_max) as f32, sy(0.0) as f32, sz(0.0) as f32]);
            }
            if config.y_axis && x_min <= 0.0 && x_max >= 0.0 && z_min <= 0.0 && z_max >= 0.0 {
                axes_points.push([sx(0.0) as f32, sy(y_min) as f32, sz(0.0) as f32]);
                axes_points.push([sx(0.0) as f32, sy(y_max) as f32, sz(0.0) as f32]);
            }
            if config.z_axis && x_min <= 0.0 && x_max >= 0.0 && y_min <= 0.0 && y_max >= 0.0 {
                axes_points.push([sx(0.0) as f32, sy(0.0) as f32, sz(z_min) as f32]);
                axes_points.push([sx(0.0) as f32, sy(0.0) as f32, sz(z_max) as f32]);
            }
            if !axes_points.is_empty() {
                // Each pair is a segment, need to split into separate line lists per axis to avoid connecting
                for chunk in axes_points.chunks(2) {
                    if chunk.len() == 2 {
                        push_line_list(chunk.to_vec(), config.axis_color, "Axes3DLines");
                    }
                }
            }
        }

        // Ticks (short segments perpendicular)
        let tick_half = (config.tick_length * 0.5) as f32;
        if config.ticks {
            if config.x_ticks && y_min <= 0.0 && y_max >= 0.0 && z_min <= 0.0 && z_max >= 0.0 {
                let mut x = (x_min / x_step).ceil() * x_step;
                while x <= x_max + 1e-9 {
                    let fx = sx(x) as f32;
                    let y0 = sy(0.0) as f32;
                    let z0 = sz(0.0) as f32;
                    let points = vec![[fx, y0 - tick_half, z0], [fx, y0 + tick_half, z0]];
                    push_line_list(points, config.tick_color, "Axes3DTicks");
                    x += x_step;
                }
            }
            if config.y_ticks && x_min <= 0.0 && x_max >= 0.0 && z_min <= 0.0 && z_max >= 0.0 {
                let mut y = (y_min / y_step).ceil() * y_step;
                while y <= y_max + 1e-9 {
                    let fy = sy(y) as f32;
                    let x0 = sx(0.0) as f32;
                    let z0 = sz(0.0) as f32;
                    let points = vec![[x0 - tick_half, fy, z0], [x0 + tick_half, fy, z0]];
                    push_line_list(points, config.tick_color, "Axes3DTicks");
                    y += y_step;
                }
            }
            if config.z_ticks && x_min <= 0.0 && x_max >= 0.0 && y_min <= 0.0 && y_max >= 0.0 {
                let mut z = (z_min / z_step).ceil() * z_step;
                while z <= z_max + 1e-9 {
                    let fz = sz(z) as f32;
                    let x0 = sx(0.0) as f32;
                    let y0 = sy(0.0) as f32;
                    let points = vec![[x0 - tick_half, y0, fz], [x0 + tick_half, y0, fz]];
                    push_line_list(points, config.tick_color, "Axes3DTicks");
                    z += z_step;
                }
            }
        }

        // Numbers and labels as billboarded text
        let mut add_text = |text: &str, x: f64, y: f64, z: f64, color: peniko::Color| {
            let label = builder.spawn_text(text, gaanim_text::prelude::TextRole::Body);
            // Clone child info before mutable borrow ends
            let child_entities: Vec<bevy::prelude::Entity> = builder
                .states
                .get(label.id)
                .map(|s| s.child_spans.iter().map(|c| c.entity).collect())
                .unwrap_or_default();
            if let Some(state) = builder.states.get_mut(label.id) {
                // Position in 3D
                state.transform = state
                    .transform
                    .shift_3d(gaanim_core::glam::DVec3::new(x, y, z));
                builder
                    .commands
                    .entity(state.entity)
                    .insert(state.transform);
                // Billboard handling — applied to parent mobject entity so it acts as 3D anchor
                if config.label_mode == crate::canvas::types::LabelMode::Billboard {
                    builder
                        .commands
                        .entity(state.entity)
                        .insert(gaanim_scene::Billboard);
                    builder
                        .commands
                        .entity(state.entity)
                        .insert(gaanim_scene::Mesh3DMarker);
                } else {
                    // HUD labels are fixed screen-space; keep as Vello2D with high z
                    // so they render on top of 3D after the perspective composition.
                    builder
                        .commands
                        .entity(state.entity)
                        .insert(gaanim_scene::HudOverlay);
                    builder
                        .commands
                        .entity(state.entity)
                        .insert(gaanim_scene::RenderOrder {
                            z_index: 1000,
                            ..Default::default()
                        });
                    for child in &child_entities {
                        builder
                            .commands
                            .entity(*child)
                            .insert(gaanim_scene::HudOverlay);
                        builder
                            .commands
                            .entity(*child)
                            .insert(gaanim_scene::RenderOrder {
                                z_index: 1000,
                                ..Default::default()
                            });
                    }
                }
            }
            builder.select(label, text).set_fill(color);
            children.push(label);
        };

        if config.numbers {
            if config.x_numbers && y_min <= 0.0 && y_max >= 0.0 && z_min <= 0.0 && z_max >= 0.0 {
                let mut x = (x_min / x_step).ceil() * x_step;
                while x <= x_max + 1e-9 {
                    let value = if x.abs() < 1e-9 { 0.0 } else { x };
                    let text = format!("{value}");
                    add_text(
                        &text,
                        sx(x),
                        sy(0.0) - config.tick_length * 0.5 - 0.25,
                        sz(0.0),
                        config.number_color,
                    );
                    x += x_step;
                }
            }
            if config.y_numbers && x_min <= 0.0 && x_max >= 0.0 && z_min <= 0.0 && z_max >= 0.0 {
                let mut y = (y_min / y_step).ceil() * y_step;
                while y <= y_max + 1e-9 {
                    if y.abs() > 1e-9 || !config.x_numbers {
                        let value = if y.abs() < 1e-9 { 0.0 } else { y };
                        let text = format!("{value}");
                        add_text(
                            &text,
                            sx(0.0) - config.tick_length * 0.5 - 0.35,
                            sy(y),
                            sz(0.0),
                            config.number_color,
                        );
                    }
                    y += y_step;
                }
            }
            if config.z_numbers && x_min <= 0.0 && x_max >= 0.0 && y_min <= 0.0 && y_max >= 0.0 {
                let mut z = (z_min / z_step).ceil() * z_step;
                while z <= z_max + 1e-9 {
                    if z.abs() > 1e-9 {
                        let value = if z.abs() < 1e-9 { 0.0 } else { z };
                        let text = format!("{value}");
                        add_text(&text, sx(0.0) - 0.35, sy(0.0), sz(z), config.number_color);
                    }
                    z += z_step;
                }
            }
        }

        if config.labels {
            if let Some(label) = config
                .x_label
                .as_deref()
                .filter(|_| y_min <= 0.0 && y_max >= 0.0 && z_min <= 0.0 && z_max >= 0.0)
            {
                add_text(label, sx(x_max) + 0.4, sy(0.0), sz(0.0), config.label_color);
            }
            if let Some(label) = config
                .y_label
                .as_deref()
                .filter(|_| x_min <= 0.0 && x_max >= 0.0 && z_min <= 0.0 && z_max >= 0.0)
            {
                add_text(label, sx(0.0), sy(y_max) + 0.4, sz(0.0), config.label_color);
            }
            if let Some(label) = config
                .z_label
                .as_deref()
                .filter(|_| x_min <= 0.0 && x_max >= 0.0 && y_min <= 0.0 && y_max >= 0.0)
            {
                add_text(label, sx(0.0), sy(0.0), sz(z_max) + 0.4, config.label_color);
            }
        }

        // Ensure a light exists for PBR visibility (once per scene)
        // Use a unique check to avoid spawning many lights if axes_3d is called multiple times
        // For now, spawn a single directional light at world origin
        builder.commands.spawn((
            bevy::prelude::DirectionalLight {
                illuminance: 10000.0,
                shadows_enabled: false,
                ..Default::default()
            },
            bevy::prelude::Transform::from_xyz(4.0, 8.0, 4.0)
                .looking_at(bevy::prelude::Vec3::ZERO, bevy::prelude::Vec3::Y),
        ));

        // Create a 3D-aware group that has both SpatialTransform and Bevy Transform
        // to avoid B0004 hierarchy warnings for mesh children
        let group_id = builder.next_id();
        let group_entity = builder
            .commands
            .spawn((
                GroupMarker,
                MobjectId(group_id),
                SpatialTransform::default(),
                GlobalSpatialTransform::default(),
                bevy::prelude::Transform::default(),
                bevy::prelude::GlobalTransform::default(),
                bevy::prelude::Visibility::default(),
                Opacity(1.0),
                GlobalOpacity(1.0),
                LocalBounds(bounds),
                WorldBounds(bounds),
                RenderOrder::default(),
                Visible,
                ObjectTag("Axes3D".to_string()),
            ))
            .id();
        builder.tag_entity(group_entity);
        let state = MobjectState {
            path: std::sync::Arc::new(gaanim_core::kurbo::BezPath::new()),
            bounds,
            transform: SpatialTransform::default(),
            opacity: 1.0,
            fill: None,
            stroke: StrokeBrush::transparent(),
            entity: group_entity,
            child_spans: Vec::new(),
            children: children.iter().map(|c| c.id).collect(),
            parent: None,
            exclude_from_parent_draw: false,
        };
        builder.states.insert(group_id, state);
        for child in &children {
            if let Some(child_state) = builder.states.get_mut(child.id) {
                child_state.parent = Some(group_id);
                builder
                    .commands
                    .entity(child_state.entity)
                    .set_parent_in_place(group_entity);
            }
        }
        builder.ensure_track(group_id);
        for child in &children {
            builder.ensure_track(child.id);
        }
        MobjectRef { id: group_id }
    }

    fn visual_leaf_ids(builder: &SceneBuilder<'_, '_, '_>, root: ObjectId) -> Vec<ObjectId> {
        let mut leaves = Vec::new();
        let mut stack = vec![root];
        let mut visited = HashSet::new();
        while let Some(id) = stack.pop() {
            if !visited.insert(id) {
                continue;
            }
            let Some(state) = builder.states.get(id) else {
                continue;
            };
            let mut children = state.children.clone();
            children.extend(state.child_spans.iter().map(|child| child.id));
            if children.is_empty() {
                leaves.push(id);
            } else {
                stack.extend(children);
            }
        }
        leaves
    }

    fn mask_path_in_world(
        builder: &SceneBuilder<'_, '_, '_>,
        root: ObjectId,
    ) -> gaanim_core::kurbo::BezPath {
        let mut result = gaanim_core::kurbo::BezPath::new();
        for id in Self::visual_leaf_ids(builder, root) {
            let Some(state) = builder.states.get(id) else {
                continue;
            };
            let mut path = (*state.path).clone();
            path.apply_affine(builder.get_world_transform(id).to_affine_2d());
            result.extend(path);
        }
        result
    }

    pub fn compile_into<'w, 's>(
        &self,
        commands: &mut Commands<'w, 's>,
        timeline: &mut Timeline,
        font_registry: &gaanim_text::font::FontRegistry,
        text_config: &gaanim_text::prelude::TextConfig,
    ) {
        self.state
            .lock()
            .expect("canvas state poisoned")
            .layout_diagnostics
            .clear();
        let manifest = self.segment_manifest();
        timeline.set_segments(
            manifest
                .segments
                .into_iter()
                .map(|segment| SegmentMetadata {
                    id: segment.id.raw(),
                    name: segment.name,
                    notes: segment.notes,
                    start_time: segment.start_time,
                    end_time: segment.end_time,
                    stops: segment
                        .stops
                        .into_iter()
                        .map(|stop| SegmentStop {
                            name: stop.name,
                            time: stop.time,
                        })
                        .collect(),
                })
                .collect(),
        );
        let segments = self
            .state
            .lock()
            .expect("canvas state poisoned")
            .segments
            .clone();
        let mut builder = SceneBuilder::new(commands, timeline, font_registry, text_config);
        let mut scene_ids: Vec<SceneId> = Vec::new();
        let mut id_map: HashMap<ObjectId, ObjectId> = HashMap::new();
        let mut object_specs: HashMap<ObjectId, ObjectSpec> = HashMap::new();
        let mut responsive_text_widths: HashMap<ObjectId, f64> = HashMap::new();
        let mut layout_versions: HashMap<ObjectId, u64> = HashMap::new();
        let mut layout_snapshots: HashMap<ObjectId, LayoutTreeSnapshot> = HashMap::new();
        let mut object_scopes: HashMap<ObjectId, CompiledObjectScope> = HashMap::new();
        let mut camera_position = DVec3::ZERO;
        let mut camera_zoom = 1.0;
        let mut camera_rotation = gaanim_core::glam::DQuat::IDENTITY;
        let mut camera_target = DVec3::ZERO;
        let mut camera_up = DVec3::Y;
        let mut camera_fov: Option<(f64, f64, f64)> = None; // (fov_y, near, far) if perspective
        let mut cancellation_marks: HashMap<ObjectId, Vec<ObjectId>> = HashMap::new();
        let mut canceled_term_children: HashMap<ObjectId, Vec<ObjectId>> = HashMap::new();
        let mut deferred_visibility: HashSet<ObjectId> = HashSet::new();
        let mut revealed_deferred: HashSet<ObjectId> = HashSet::new();
        // Raw bounds for the canvas background (visual, no margin).
        let raw_bounds = self.units.frame_bounds(self.width, self.height);
        // Inset bounds for layout operations (to_edge, to_corner respect margin).
        let m = &self.margin;
        let frame_bounds = Bounds3D::new_2d(
            raw_bounds.min.x + m.left,
            raw_bounds.min.y + m.bottom,
            raw_bounds.max.x - m.right,
            raw_bounds.max.y - m.top,
        );
        let bg_color = self.background.unwrap_or(gaanim_core::peniko::Color::WHITE);

        for seg in &segments {
            let previous_scene = seg
                .prev_segment
                .and_then(|index| scene_ids.get(index).copied());
            let scene_id = builder.begin_scene(&seg.name);
            scene_ids.push(scene_id);
            Self::replay_seg(
                &mut builder,
                seg,
                scene_id,
                previous_scene,
                &mut id_map,
                &mut object_specs,
                &mut responsive_text_widths,
                &mut layout_versions,
                &mut layout_snapshots,
                &mut object_scopes,
                frame_bounds,
                raw_bounds,
                text_config,
                self.theme_style.as_ref(),
                bg_color,
                &mut camera_position,
                &mut camera_zoom,
                &mut camera_rotation,
                &mut camera_target,
                &mut camera_up,
                &mut camera_fov,
                &mut cancellation_marks,
                &mut canceled_term_children,
                &mut deferred_visibility,
                &mut revealed_deferred,
                &self.state,
            );
            builder.end_scene();
        }

        for (i, seg) in segments.iter().enumerate() {
            if let Some(prev) = seg.prev_segment
                && prev < i
                && i < scene_ids.len()
                && prev < scene_ids.len()
                && let Some(tr) = &seg.transition
            {
                builder
                    .timeline
                    .connect(scene_ids[prev], scene_ids[i], tr.clone());
            }
        }

        // Insert canvas background resource so the renderer draws a visible
        // canvas boundary, distinguishing the canvas area from the window.
        // Uses raw_bounds (no margin) — the visual background covers the full canvas.
        builder
            .commands
            .insert_resource(gaanim_renderer::pipeline::CanvasBackground {
                color: bg_color,
                bounds: raw_bounds,
            });

        // Clear with the canvas color as well. The drawable background is
        // world-space geometry and can be rotated by the camera; using the
        // same clear color prevents Bevy's window clear color from showing
        // through at the viewport edges during that rotation.
        let rgba = bg_color.to_rgba8();
        builder
            .commands
            .insert_resource(ClearColor(Color::srgba_u8(rgba.r, rgba.g, rgba.b, rgba.a)));
        builder.commands.insert_resource(self.lighting_3d);
    }

    pub fn compile(&self, world: &mut World) {
        let mut timeline = world
            .remove_resource::<Timeline>()
            .expect("Timeline missing");
        let mut font_registry = world
            .remove_resource::<gaanim_text::font::FontRegistry>()
            .expect("FontRegistry missing");
        let mut text_config = world
            .remove_resource::<gaanim_text::prelude::TextConfig>()
            .expect("TextConfig missing");
        if self.theme.is_some() {
            text_config = self.themed_text_config();
        } else {
            let bg_color = self.background.unwrap_or(gaanim_core::peniko::Color::WHITE);
            let default_fg = if typst_foreground_for_background(bg_color) == "000000" {
                gaanim_core::peniko::Color::BLACK
            } else {
                gaanim_core::peniko::Color::WHITE
            };
            for role_style in text_config.roles.values_mut() {
                role_style.fill_color = default_fg;
            }
        }
        self.register_theme_fonts(&mut font_registry);
        let mut commands = world.commands();
        self.compile_into(&mut commands, &mut timeline, &font_registry, &text_config);
        world.insert_resource(timeline);
        world.insert_resource(font_registry);
        world.insert_resource(text_config);
    }

    fn replay_seg(
        builder: &mut SceneBuilder,
        seg: &Segment,
        scene_id: SceneId,
        previous_scene: Option<SceneId>,
        id_map: &mut HashMap<ObjectId, ObjectId>,
        object_specs: &mut HashMap<ObjectId, ObjectSpec>,
        responsive_text_widths: &mut HashMap<ObjectId, f64>,
        layout_versions: &mut HashMap<ObjectId, u64>,
        layout_snapshots: &mut HashMap<ObjectId, LayoutTreeSnapshot>,
        object_scopes: &mut HashMap<ObjectId, CompiledObjectScope>,
        frame_bounds: Bounds3D,
        raw_frame_bounds: Bounds3D,
        text_config: &gaanim_text::prelude::TextConfig,
        theme: Option<&crate::canvas::CanvasTheme>,
        scene_background: gaanim_core::peniko::Color,
        camera_position: &mut DVec3,
        camera_zoom: &mut f64,
        camera_rotation: &mut gaanim_core::glam::DQuat,
        camera_target: &mut DVec3,
        camera_up: &mut DVec3,
        camera_fov: &mut Option<(f64, f64, f64)>,
        cancellation_marks: &mut HashMap<ObjectId, Vec<ObjectId>>,
        canceled_term_children: &mut HashMap<ObjectId, Vec<ObjectId>>,
        deferred_visibility: &mut HashSet<ObjectId>,
        revealed_deferred: &mut HashSet<ObjectId>,
        diagnostic_state: &crate::canvas::ops::SharedCanvasState,
    ) {
        let scene_start = builder.current_time;
        let transform_targets = Self::transform_targets(&seg.ops);
        for op in &seg.ops {
            match op {
                Op::Spawn(spec) => {
                    let authored = spec.lock().expect("object spec poisoned").clone();
                    let spec = theme
                        .map(|theme| theme.resolve_object(&authored))
                        .transpose()
                        .unwrap_or_else(|error| panic!("invalid theme cascade: {error}"))
                        .unwrap_or(authored);
                    object_specs.insert(spec.id, spec.clone());
                    if matches!(
                        &spec.kind,
                        SpawnKind::Text(text) if !matches!(text.flow.wrap, StructuredTextWrap::NoWrap)
                    ) {
                        responsive_text_widths.insert(spec.id, frame_bounds.width().max(1.0));
                    }
                    if spec.defer_visibility_until_play {
                        deferred_visibility.insert(spec.id);
                    }
                    let actual = Self::spawn_one(
                        builder,
                        &spec,
                        id_map,
                        frame_bounds,
                        text_config,
                        scene_background,
                    );
                    if spec.exclude_from_parent_draw {
                        if let Some(state) = builder.states.get_mut(actual.id) {
                            state.exclude_from_parent_draw = true;
                        }
                    }
                    id_map.insert(spec.id, actual.id);
                    object_scopes.insert(spec.id, CompiledObjectScope::Segment(scene_id));
                    // Compilation creates every entity up front so arbitrary timeline seeks
                    // remain possible. An object declared after earlier animations must still
                    // stay hidden until the playhead reaches its declaration point.
                    if spec.defer_visibility_until_play {
                        if let Some(state) = builder.states.get(actual.id).cloned() {
                            builder.hide_visuals_now(&state);
                        }
                    } else if !transform_targets.contains(&spec.id)
                        && builder.current_time > scene_start + 1e-9
                        && let Some(state) = builder.states.get(actual.id).cloned()
                    {
                        builder.hide_visuals_now(&state);
                        builder.schedule_show_now(actual.id);
                    }
                    if transform_targets.contains(&spec.id) {
                        if let Some(state) = builder.states.get(actual.id).cloned() {
                            builder.hide_visuals_now(&state);
                            builder.schedule_hide_hierarchy(actual.id);
                        }
                    }
                }
                Op::SpawnCameraBinding(spec) => {
                    let spec = spec.lock().expect("camera binding poisoned").clone();
                    let kind = match &spec.kind {
                        CanvasCameraBindingKind::TwoD {
                            center,
                            zoom,
                            rotation,
                        } => gaanim_animation::CameraBindingKind::TwoD {
                            center: center.as_ref().map(|endpoint| {
                                compile_tracking_endpoint(endpoint, id_map, &builder.states)
                            }),
                            zoom: zoom.as_ref().map(|expression| {
                                compile_tracking_scalar(expression, id_map, &builder.states)
                            }),
                            rotation: rotation.as_ref().map(|expression| {
                                compile_tracking_scalar(expression, id_map, &builder.states)
                            }),
                        },
                        CanvasCameraBindingKind::ThreeD {
                            eye,
                            target,
                            fov_y,
                            up,
                        } => gaanim_animation::CameraBindingKind::ThreeD {
                            eye: eye.as_ref().map(|endpoint| {
                                compile_tracking_endpoint(endpoint, id_map, &builder.states)
                            }),
                            target: target.as_ref().map(|endpoint| {
                                compile_tracking_endpoint(endpoint, id_map, &builder.states)
                            }),
                            fov_y: fov_y.as_ref().map(|expression| {
                                compile_tracking_scalar(expression, id_map, &builder.states)
                            }),
                            up: *up,
                        },
                    };
                    builder.commands.spawn(gaanim_animation::CameraBinding {
                        order: spec.order,
                        kind,
                        influence: compile_tracking_scalar(
                            &spec.influence,
                            id_map,
                            &builder.states,
                        ),
                        windows: spec
                            .windows
                            .into_iter()
                            .map(|window| gaanim_animation::CameraBindingWindow {
                                start: window.start,
                                end: window.end,
                            })
                            .collect(),
                    });
                }
                Op::Animate { anim, active } => {
                    if *active {
                        Self::reveal_deferred_on_play(
                            builder,
                            deferred_visibility,
                            revealed_deferred,
                            anim,
                            id_map,
                        );
                        if let Some(anim) = Self::remap_anim(anim, id_map) {
                            if anim.anim_type.is_camera() {
                                let start = builder.current_time;
                                Self::schedule_camera_animation(
                                    builder,
                                    frame_bounds,
                                    id_map,
                                    camera_position,
                                    camera_zoom,
                                    camera_rotation,
                                    camera_target,
                                    camera_up,
                                    camera_fov,
                                    &anim,
                                    start,
                                );
                                builder.wait(anim.delay.max(0.0) + anim.duration.max(0.0));
                            } else {
                                builder.play(anim);
                            }
                        }
                    }
                }
                Op::Play(anims) => {
                    for anim in anims {
                        Self::reveal_deferred_on_play(
                            builder,
                            deferred_visibility,
                            revealed_deferred,
                            anim,
                            id_map,
                        );
                    }
                    let remapped: Vec<AnimationBuilder> = anims
                        .iter()
                        .filter_map(|anim| Self::remap_anim(anim, id_map))
                        .collect();
                    let start = builder.current_time;
                    let max_duration = remapped
                        .iter()
                        .map(|anim| anim.delay.max(0.0) + anim.duration.max(0.0))
                        .fold(0.0, f64::max);
                    for anim in remapped {
                        if anim.anim_type.is_camera() {
                            Self::schedule_camera_animation(
                                builder,
                                frame_bounds,
                                id_map,
                                camera_position,
                                camera_zoom,
                                camera_rotation,
                                camera_target,
                                camera_up,
                                camera_fov,
                                &anim,
                                start,
                            );
                        } else {
                            builder.play_at_current_time(anim);
                        }
                    }
                    builder.current_time = start + max_duration;
                }
                Op::FragmentFill {
                    target,
                    fragment,
                    occurrence,
                    color,
                } => {
                    if let Some(&target) = id_map.get(target) {
                        builder
                            .select_occurrence(MobjectRef { id: target }, fragment, *occurrence)
                            .set_fill(*color);
                    }
                }
                Op::FragmentIndicate {
                    target,
                    fragment,
                    occurrence,
                    color,
                    duration,
                } => {
                    let children =
                        Self::fragment_child_ids(builder, id_map, *target, fragment, *occurrence);
                    canceled_term_children
                        .entry(*target)
                        .or_default()
                        .extend(children.iter().copied());
                    let anims = children
                        .into_iter()
                        .map(|target| AnimationBuilder {
                            target,
                            anim_type: AnimationType::Indicate {
                                color: *color,
                                scale_factor: 1.1,
                            },
                            duration: *duration,
                            rate_func: RateFunc::ThereAndBack,
                            delay: 0.0,
                        })
                        .collect();
                    builder.play_parallel(anims);
                }
                Op::FragmentEmphasis {
                    target,
                    fragment,
                    occurrence,
                    kind,
                    duration,
                } => {
                    let children =
                        Self::fragment_child_ids(builder, id_map, *target, fragment, *occurrence);
                    let count = children.len();
                    let anims = children
                        .into_iter()
                        .enumerate()
                        .map(|(index, target)| AnimationBuilder {
                            target,
                            anim_type: match kind.as_str() {
                                "wiggle" | "wave" => AnimationType::Wiggle,
                                "highlight" => AnimationType::Circumscribe { color: None },
                                _ => AnimationType::Indicate {
                                    color: None,
                                    scale_factor: if kind == "pulse" { 1.16 } else { 1.1 },
                                },
                            },
                            duration: *duration,
                            rate_func: RateFunc::ThereAndBack,
                            delay: if kind == "wave" && count > 1 {
                                index as f64 * duration * 0.35 / (count - 1) as f64
                            } else {
                                0.0
                            },
                        })
                        .collect();
                    builder.play_parallel(anims);
                }
                Op::FragmentReveal {
                    target,
                    fragment,
                    occurrence,
                    style,
                    duration,
                } => {
                    let children =
                        Self::fragment_child_ids(builder, id_map, *target, fragment, *occurrence);
                    let anims = children
                        .into_iter()
                        .map(|target| AnimationBuilder {
                            target,
                            anim_type: match style {
                                FragmentRevealStyle::Fade => AnimationType::FadeIn,
                                FragmentRevealStyle::Wipe => AnimationType::Write {
                                    config: Default::default(),
                                },
                                FragmentRevealStyle::FromBelow => AnimationType::FadeInFrom {
                                    offset: DVec3::new(0.0, -24.0, 0.0),
                                },
                            },
                            duration: *duration,
                            rate_func: RateFunc::Smooth,
                            delay: 0.0,
                        })
                        .collect();
                    builder.play_parallel(anims);
                }
                Op::CancelFragment {
                    target,
                    fragment,
                    occurrence,
                    duration,
                } => {
                    let children =
                        Self::fragment_child_ids(builder, id_map, *target, fragment, *occurrence);
                    let parent_transform = id_map
                        .get(target)
                        .and_then(|parent| builder.states.get(*parent))
                        .map(|state| state.transform);
                    let strike_color = children
                        .iter()
                        .find_map(|child| {
                            builder
                                .states
                                .get(*child)
                                .and_then(|state| match &state.fill {
                                    Some(gaanim_core::peniko::Brush::Solid(color)) => Some(*color),
                                    _ => None,
                                })
                        })
                        .unwrap_or(PenikoColor::WHITE);
                    let bounds = children
                        .iter()
                        .filter_map(|child| {
                            let state = builder.states.get(*child)?;
                            // Textual child bounds have already been centered
                            // into their parent's local coordinate system by
                            // the shaper. Applying the child's transform here
                            // would subtract that center a second time and put
                            // the strike near the canvas corner.
                            let bounds = state.bounds;
                            Some(match parent_transform {
                                Some(parent) => bounds.transform_2d(&parent.to_affine_2d()),
                                None => bounds,
                            })
                        })
                        .reduce(|bounds, next| bounds.union(&next));
                    if let Some(bounds) = bounds {
                        let pad = (bounds.width() * 0.08).max(3.0);
                        let strike = builder
                            .line(
                                Point::new(bounds.min.x - pad, bounds.min.y - pad * 0.25),
                                Point::new(bounds.max.x + pad, bounds.max.y + pad * 0.25),
                            )
                            .no_fill()
                            .stroke(strike_color, 3.0)
                            .spawn();
                        cancellation_marks
                            .entry(*target)
                            .or_default()
                            .push(strike.id);
                        builder.play(AnimationBuilder {
                            target: strike.id,
                            anim_type: AnimationType::Create {
                                config: Default::default(),
                            },
                            duration: *duration,
                            rate_func: RateFunc::Smooth,
                            delay: 0.0,
                        });
                    } else {
                        builder.wait(*duration);
                    }
                }
                Op::BraceLabel {
                    target,
                    fragment,
                    occurrence,
                    label,
                    above,
                    duration,
                } => {
                    let children =
                        Self::fragment_child_ids(builder, id_map, *target, fragment, *occurrence);
                    let parent = id_map
                        .get(target)
                        .and_then(|id| builder.states.get(*id))
                        .map(|s| s.transform);
                    let bounds = children
                        .iter()
                        .filter_map(|id| builder.states.get(*id).map(|s| s.bounds))
                        .map(|b| {
                            parent
                                .map(|p| b.transform_2d(&p.to_affine_2d()))
                                .unwrap_or(b)
                        })
                        .reduce(|a, b| a.union(&b));
                    if let Some(bounds) = bounds {
                        let color = children
                            .iter()
                            .find_map(|id| {
                                builder.states.get(*id).and_then(|s| match &s.fill {
                                    Some(gaanim_core::peniko::Brush::Solid(c)) => Some(*c),
                                    _ => None,
                                })
                            })
                            .unwrap_or(PenikoColor::WHITE);
                        let side = if *above { 1.0 } else { -1.0 };
                        let y = if *above {
                            bounds.max.y + 12.0
                        } else {
                            bounds.min.y - 12.0
                        };
                        let brace = builder
                            .brace(
                                Point::new(bounds.min.x, y),
                                Point::new(bounds.max.x, y),
                                -side * 10.0,
                            )
                            .no_fill()
                            .stroke(color, 2.0)
                            .spawn();
                        let style = &text_config.roles[&gaanim_text::prelude::TextRole::Body];
                        let label_ref = builder.text(label, &style.font_family, style.size);
                        if let Some(state) = builder.states.get_mut(label_ref.id) {
                            state.transform.translation =
                                DVec3::new(bounds.center().x, y + side * 25.0, 0.0);
                            builder
                                .commands
                                .entity(state.entity)
                                .insert(state.transform);
                        }
                        builder.play_parallel(vec![
                            AnimationBuilder {
                                target: brace.id,
                                anim_type: AnimationType::Create {
                                    config: Default::default(),
                                },
                                duration: *duration,
                                rate_func: RateFunc::Smooth,
                                delay: 0.0,
                            },
                            AnimationBuilder {
                                target: label_ref.id,
                                anim_type: AnimationType::FadeIn,
                                duration: *duration,
                                rate_func: RateFunc::Smooth,
                                delay: 0.0,
                            },
                        ]);
                    }
                }
                Op::AnnotateFragment {
                    target,
                    fragment,
                    occurrence,
                    label,
                    offset,
                    duration,
                } => {
                    let children =
                        Self::fragment_child_ids(builder, id_map, *target, fragment, *occurrence);
                    let parent = id_map
                        .get(target)
                        .and_then(|id| builder.states.get(*id))
                        .map(|s| s.transform);
                    let bounds = children
                        .iter()
                        .filter_map(|id| builder.states.get(*id).map(|s| s.bounds))
                        .map(|b| {
                            parent
                                .map(|p| b.transform_2d(&p.to_affine_2d()))
                                .unwrap_or(b)
                        })
                        .reduce(|a, b| a.union(&b));
                    if let Some(bounds) = bounds {
                        let position = bounds.center() + *offset;
                        let style = &text_config.roles[&gaanim_text::prelude::TextRole::Body];
                        let label_ref = builder.text(label, &style.font_family, style.size);
                        let label_size = builder
                            .states
                            .get(label_ref.id)
                            .map(|state| state.bounds.size())
                            .unwrap_or(DVec3::new(80.0, 24.0, 0.0));
                        let toward_label_x = if offset.x >= 0.0 { -1.0 } else { 1.0 };
                        let toward_label_y = if offset.y >= 0.0 { -1.0 } else { 1.0 };
                        // Attach at the label corner nearest the term, so the
                        // leader line never crosses the annotation text.
                        let label_anchor = position
                            + DVec3::new(
                                toward_label_x * label_size.x * 0.46,
                                toward_label_y * label_size.y * 0.42,
                                0.0,
                            );
                        if let Some(state) = builder.states.get_mut(label_ref.id) {
                            state.transform.translation = position;
                            builder
                                .commands
                                .entity(state.entity)
                                .insert(state.transform);
                        }
                        let line = builder
                            .line(
                                Point::new(bounds.center().x, bounds.center().y),
                                Point::new(label_anchor.x, label_anchor.y),
                            )
                            .no_fill()
                            .stroke(PenikoColor::WHITE, 2.0)
                            .spawn();
                        // Text glyph transforms are local to their equation.
                        // Use an invisible scene-space proxy so the leader
                        // starts at the tag rather than a canvas corner.
                        let proxy = builder.dot(0.01).no_fill().no_stroke().spawn();
                        if let Some(proxy_state) = builder.states.get_mut(proxy.id) {
                            proxy_state.transform.translation = bounds.center();
                            builder
                                .commands
                                .entity(proxy_state.entity)
                                .insert(proxy_state.transform);
                        }
                        if let (Some(line_state), Some(proxy_state)) =
                            (builder.states.get(line.id), builder.states.get(proxy.id))
                        {
                            if let Some(parent_id) = id_map.get(target).copied()
                                && let Some(parent_state) = builder.states.get(parent_id)
                            {
                                builder.commands.entity(proxy_state.entity).insert(
                                    PositionBinding::with_offset(
                                        parent_state.entity,
                                        gaanim_animation::AxisMask::XY,
                                        bounds.center() - parent_state.transform.translation,
                                    ),
                                );
                            }
                            builder
                                .commands
                                .entity(line_state.entity)
                                .insert(TrackingLine::new(
                                    TrackingEndpoint::Entity(proxy_state.entity),
                                    TrackingEndpoint::Static(label_anchor),
                                ));
                        }
                        builder.play_parallel(vec![
                            AnimationBuilder {
                                target: line.id,
                                // TrackingLine regenerates its path each
                                // frame, so a path-draw clip cannot hide it
                                // before its scheduled start. FadeIn keeps it
                                // invisible until the annotation begins.
                                anim_type: AnimationType::FadeIn,
                                duration: *duration,
                                rate_func: RateFunc::Smooth,
                                delay: 0.0,
                            },
                            AnimationBuilder {
                                target: label_ref.id,
                                anim_type: AnimationType::FadeIn,
                                duration: *duration,
                                rate_func: RateFunc::Smooth,
                                delay: 0.0,
                            },
                        ]);
                    }
                }
                Op::WriteTerms {
                    target,
                    terms,
                    duration,
                } => {
                    let term_duration = *duration / terms.len() as f64;
                    for (fragment, occurrence) in terms {
                        let anims = Self::fragment_child_ids(
                            builder,
                            id_map,
                            *target,
                            fragment,
                            *occurrence,
                        )
                        .into_iter()
                        .map(|target| AnimationBuilder {
                            target,
                            anim_type: AnimationType::Write {
                                config: Default::default(),
                            },
                            duration: term_duration,
                            rate_func: RateFunc::Smooth,
                            delay: 0.0,
                        })
                        .collect();
                        builder.play_parallel(anims);
                    }
                }
                Op::FocusEquation {
                    target,
                    terms,
                    dim_opacity,
                    duration,
                } => {
                    let focused = terms
                        .iter()
                        .flat_map(|(fragment, occurrence)| {
                            Self::fragment_child_ids(
                                builder,
                                id_map,
                                *target,
                                fragment,
                                *occurrence,
                            )
                        })
                        .collect::<std::collections::HashSet<_>>();
                    let all = id_map
                        .get(target)
                        .and_then(|target| builder.states.get(*target))
                        .map(|state| state.children.clone())
                        .unwrap_or_default();
                    let mut anims = Vec::with_capacity(all.len() + focused.len());
                    for child in all {
                        if focused.contains(&child) {
                            anims.push(AnimationBuilder {
                                target: child,
                                anim_type: AnimationType::Indicate {
                                    color: None,
                                    scale_factor: 1.12,
                                },
                                duration: *duration,
                                rate_func: RateFunc::ThereAndBack,
                                delay: 0.0,
                            });
                        } else {
                            anims.push(AnimationBuilder {
                                target: child,
                                anim_type: AnimationType::FadeTo { to: *dim_opacity },
                                duration: *duration,
                                rate_func: RateFunc::Smooth,
                                delay: 0.0,
                            });
                        }
                    }
                    builder.play_parallel(anims);
                }
                Op::FragmentFillTo {
                    target,
                    fragment,
                    occurrence,
                    color,
                    duration,
                } => {
                    let children =
                        Self::fragment_child_ids(builder, id_map, *target, fragment, *occurrence);
                    let anims = children
                        .into_iter()
                        .map(|target| AnimationBuilder {
                            target,
                            anim_type: AnimationType::FillColorTo { to: *color },
                            duration: *duration,
                            rate_func: RateFunc::Smooth,
                            delay: 0.0,
                        })
                        .collect();
                    builder.play_parallel(anims);
                }
                Op::FragmentTransform {
                    source,
                    source_fragment,
                    source_occurrence,
                    target,
                    target_fragment,
                    target_occurrence,
                    duration,
                } => {
                    let sources = Self::fragment_child_ids(
                        builder,
                        id_map,
                        *source,
                        source_fragment,
                        *source_occurrence,
                    );
                    let targets = Self::fragment_child_ids(
                        builder,
                        id_map,
                        *target,
                        target_fragment,
                        *target_occurrence,
                    );
                    if sources.is_empty() || targets.is_empty() {
                        bevy::prelude::warn!(
                            "equation fragment transform could not resolve '{source_fragment}' -> '{target_fragment}'"
                        );
                    } else if let (Some(&source_parent), Some(&target_parent)) =
                        (id_map.get(source), id_map.get(target))
                    {
                        builder.play_equation_transition(
                            source_parent,
                            target_parent,
                            vec![(sources, targets)],
                            *duration,
                            EquationTransitionMode::Copy,
                            false,
                        );
                    }
                }
                Op::TaggedTransform {
                    source,
                    target,
                    pairs,
                    duration,
                } => {
                    let mut semantic_groups = Vec::new();
                    for (source_fragment, source_occurrence, target_fragment, target_occurrence) in
                        pairs
                    {
                        let sources = Self::fragment_child_ids(
                            builder,
                            id_map,
                            *source,
                            source_fragment,
                            *source_occurrence,
                        );
                        let targets = Self::fragment_child_ids(
                            builder,
                            id_map,
                            *target,
                            target_fragment,
                            *target_occurrence,
                        );
                        if sources.is_empty() || targets.is_empty() {
                            bevy::prelude::warn!(
                                "equation tag transform could not resolve '{source_fragment}' -> '{target_fragment}'"
                            );
                        } else {
                            semantic_groups.push((sources, targets));
                        }
                    }
                    if let (Some(&source_parent), Some(&target_parent)) =
                        (id_map.get(source), id_map.get(target))
                    {
                        builder.play_equation_transition(
                            source_parent,
                            target_parent,
                            semantic_groups,
                            *duration,
                            EquationTransitionMode::Copy,
                            false,
                        );
                    }
                }
                Op::ExpandEquation {
                    source,
                    target,
                    source_fragment,
                    source_occurrence,
                    target_fragment,
                    target_occurrence,
                    duration,
                } => {
                    Self::fade_cancellation_marks(builder, cancellation_marks, *source, *duration);
                    Self::fade_canceled_term_children(
                        builder,
                        canceled_term_children,
                        *source,
                        *duration,
                    );
                    let sources = Self::fragment_child_ids(
                        builder,
                        id_map,
                        *source,
                        source_fragment,
                        *source_occurrence,
                    );
                    let targets = Self::fragment_child_ids(
                        builder,
                        id_map,
                        *target,
                        target_fragment,
                        *target_occurrence,
                    );
                    if let (Some(&source_parent), Some(&target_parent)) =
                        (id_map.get(source), id_map.get(target))
                    {
                        if sources.is_empty() || targets.is_empty() {
                            bevy::prelude::warn!(
                                "equation expansion could not resolve '{source_fragment}' -> '{target_fragment}'"
                            );
                        } else {
                            builder.play_equation_transition(
                                source_parent,
                                target_parent,
                                vec![(sources, targets)],
                                *duration,
                                EquationTransitionMode::Replace,
                                true,
                            );
                        }
                    }
                }
                Op::StepEquation {
                    source,
                    target,
                    pairs,
                    duration,
                } => {
                    Self::fade_cancellation_marks(builder, cancellation_marks, *source, *duration);
                    Self::fade_canceled_term_children(
                        builder,
                        canceled_term_children,
                        *source,
                        *duration,
                    );
                    if let (Some(&source_parent), Some(&target_parent)) =
                        (id_map.get(source), id_map.get(target))
                    {
                        let semantic_groups = pairs
                            .iter()
                            .filter_map(
                                |(
                                    source_fragment,
                                    source_occurrence,
                                    target_fragment,
                                    target_occurrence,
                                )| {
                                    let sources = Self::fragment_child_ids(
                                        builder,
                                        id_map,
                                        *source,
                                        source_fragment,
                                        *source_occurrence,
                                    );
                                    let targets = Self::fragment_child_ids(
                                        builder,
                                        id_map,
                                        *target,
                                        target_fragment,
                                        *target_occurrence,
                                    );
                                    if sources.is_empty() || targets.is_empty() {
                                        bevy::prelude::warn!(
                                            "equation step could not resolve semantic match '{source_fragment}' -> '{target_fragment}'"
                                        );
                                        None
                                    } else {
                                        Some((sources, targets))
                                    }
                                },
                            )
                            .collect();
                        builder.play_equation_transition(
                            source_parent,
                            target_parent,
                            semantic_groups,
                            *duration,
                            EquationTransitionMode::Replace,
                            true,
                        );
                    }
                }
                Op::TransformMatching {
                    source,
                    target,
                    mode,
                    semantic_pairs,
                    duration,
                } => {
                    Self::fade_cancellation_marks(builder, cancellation_marks, *source, *duration);
                    Self::fade_canceled_term_children(
                        builder,
                        canceled_term_children,
                        *source,
                        *duration,
                    );
                    if let (Some(&src), Some(&dst)) = (id_map.get(source), id_map.get(target)) {
                        if mode == "tex" {
                            let semantic_groups = semantic_pairs
                                .iter()
                                .filter_map(
                                    |(
                                        source_fragment,
                                        source_occurrence,
                                        target_fragment,
                                        target_occurrence,
                                    )| {
                                        let sources = Self::fragment_child_ids(
                                            builder,
                                            id_map,
                                            *source,
                                            source_fragment,
                                            *source_occurrence,
                                        );
                                        let targets = Self::fragment_child_ids(
                                            builder,
                                            id_map,
                                            *target,
                                            target_fragment,
                                            *target_occurrence,
                                        );
                                        (!sources.is_empty() && !targets.is_empty())
                                            .then_some((sources, targets))
                                    },
                                )
                                .collect();
                            builder.play_equation_transition(
                                src,
                                dst,
                                semantic_groups,
                                *duration,
                                EquationTransitionMode::Replace,
                                true,
                            );
                        } else {
                            builder.play_transform_matching(
                                src,
                                dst,
                                gaanim_math::matching::MatchingMode::Shapes,
                                *duration,
                                RateFunc::Smooth,
                            );
                        }
                    }
                }
                Op::LayoutTransition {
                    from_version,
                    to,
                    duration,
                    entering,
                    leaving,
                } => {
                    if let Some(expected) = from_version {
                        let actual = layout_versions.get(&to.container).copied();
                        assert_eq!(
                            actual,
                            Some(*expected),
                            "layout snapshot chain for {:?} expected version {}, found {:?}",
                            to.container,
                            expected,
                            actual
                        );
                    }
                    layout_versions.insert(to.container, to.version);
                    layout_snapshots.insert(to.container, to.clone());
                    let root_source =
                        outermost_layout_source(to.container, layout_snapshots, object_specs);
                    let Some(root_snapshot) = layout_snapshots.get(&root_source) else {
                        continue;
                    };
                    let Some(tree) = compile_layout_tree(
                        root_source,
                        layout_snapshots,
                        id_map,
                        &builder.states,
                        object_specs,
                        text_config,
                    ) else {
                        continue;
                    };
                    let root_id = tree.root.id;
                    let Some(container) = id_map.get(&root_source).copied() else {
                        continue;
                    };
                    let before: HashMap<ObjectId, SpatialTransform> = tree
                        .source_by_id
                        .iter()
                        .filter(|(id, _)| **id != root_id)
                        .filter_map(|(_, source)| {
                            let actual = id_map.get(source).copied()?;
                            builder
                                .states
                                .get(actual)
                                .map(|state| (actual, state.transform))
                        })
                        .collect();
                    let viewport = match root_snapshot.spec.within {
                        LayoutWithin::Safe => frame_bounds,
                        LayoutWithin::Frame => raw_frame_bounds,
                        LayoutWithin::Intrinsic => frame_bounds,
                    };
                    let measurer = CompiledLayoutMeasure {
                        fixed: tree.fixed.clone(),
                        texts: tree.texts.clone(),
                        text_composition_widths: RefCell::default(),
                        font_registry: builder.font_registry,
                    };
                    let resolved =
                        match gaanim_layout::resolve_layout(&tree.root, viewport, &measurer, &[]) {
                            Ok(resolved) => resolved,
                            Err(error) => {
                                let message =
                                    format!("layout {} resolution failed: {error}", to.version);
                                eprintln!("{message}");
                                diagnostic_state
                                    .lock()
                                    .expect("canvas state poisoned")
                                    .layout_diagnostics
                                    .push((Some(to.container), message));
                                continue;
                            }
                        };
                    let text_composition_widths = measurer.text_composition_widths.into_inner();
                    if !resolved.diagnostics.is_empty() {
                        let mut state = diagnostic_state.lock().expect("canvas state poisoned");
                        state
                            .layout_diagnostics
                            .extend(resolved.diagnostics.iter().map(|diagnostic| {
                                (
                                    Some(to.container),
                                    format!(
                                        "layout {} constraint #{}: {} (residual {:.6})",
                                        to.version,
                                        diagnostic.constraint,
                                        diagnostic.message,
                                        diagnostic.residual
                                    ),
                                )
                            }));
                    }
                    let mut materialized_by_id: BTreeMap<gaanim_layout::LayoutId, ObjectId> = tree
                        .source_by_id
                        .iter()
                        .filter_map(|(id, source)| {
                            id_map.get(source).copied().map(|actual| (*id, actual))
                        })
                        .collect();
                    let mut text_crossfades = Vec::new();
                    for (layout_id, source) in &tree.source_by_id {
                        if !tree.texts.contains_key(layout_id) {
                            continue;
                        }
                        let Some(text_spec) = object_specs
                            .get(source)
                            .filter(|spec| {
                                compiled_text_measure(spec, text_config).is_some_and(|text| {
                                    !matches!(text.spec.flow.wrap, StructuredTextWrap::NoWrap)
                                })
                            })
                            .cloned()
                        else {
                            continue;
                        };
                        let Some(member) = materialized_by_id.get(layout_id).copied() else {
                            continue;
                        };
                        let Some(target_box) = resolved.boxes.get(layout_id).copied() else {
                            continue;
                        };
                        let width = text_composition_widths
                            .get(layout_id)
                            .copied()
                            .unwrap_or_else(|| target_box.bounds.width())
                            .max(1.0);
                        let current_width = responsive_text_widths
                            .get(source)
                            .copied()
                            .unwrap_or_else(|| frame_bounds.width().max(1.0));
                        if (width - current_width).abs() <= 1.0e-6 {
                            continue;
                        }
                        let mut materialized = text_spec;
                        let SpawnKind::Text(text) = &mut materialized.kind else {
                            continue;
                        };
                        text.flow.wrap = StructuredTextWrap::Width(match text.flow.wrap {
                            StructuredTextWrap::Width(limit) => limit.min(width),
                            StructuredTextWrap::Auto => width,
                            StructuredTextWrap::NoWrap => continue,
                        });
                        let replacement = Self::spawn_one(
                            builder,
                            &materialized,
                            id_map,
                            frame_bounds,
                            text_config,
                            scene_background,
                        );
                        text_crossfades.push((member, replacement.id));
                        materialized_by_id.insert(*layout_id, replacement.id);
                        id_map.insert(*source, replacement.id);
                        responsive_text_widths.insert(*source, width);
                    }

                    // Rebuild every direct group edge in the current tree. A
                    // nested layout is resolved as part of its outermost owner,
                    // so width-dependent leaves see the box offered by the
                    // complete hierarchy rather than the safe frame.
                    for (parent_id, child_ids) in &tree.children_by_id {
                        let Some(parent) = materialized_by_id.get(parent_id).copied() else {
                            continue;
                        };
                        let children: Vec<_> = child_ids
                            .iter()
                            .filter_map(|id| materialized_by_id.get(id).copied())
                            .collect();
                        let removed_children: Vec<_> = builder
                            .states
                            .get(parent)
                            .map(|state| {
                                state
                                    .children
                                    .iter()
                                    .copied()
                                    .filter(|child| !children.contains(child))
                                    .collect()
                            })
                            .unwrap_or_default();
                        for child in removed_children {
                            builder.remove_from_group(
                                MobjectRef { id: parent },
                                MobjectRef { id: child },
                            );
                        }
                        for child in &children {
                            let current_parent =
                                builder.states.get(*child).and_then(|state| state.parent);
                            if current_parent != Some(parent) {
                                builder.add_to_group(
                                    MobjectRef { id: parent },
                                    MobjectRef { id: *child },
                                );
                            }
                        }
                        if let Some(state) = builder.states.get_mut(parent) {
                            state.children = children;
                        }
                    }

                    let root_box = resolved.boxes.get(&root_id).copied().unwrap_or(
                        gaanim_layout::ResolvedBox {
                            bounds: Bounds3D::new_2d(0.0, 0.0, 0.0, 0.0),
                            clip: None,
                            scale: DVec3::ONE,
                        },
                    );
                    let root_center = root_box.bounds.center();
                    let local_root_bounds = Bounds3D::new_2d(
                        -root_box.bounds.width() * 0.5,
                        -root_box.bounds.height() * 0.5,
                        root_box.bounds.width() * 0.5,
                        root_box.bounds.height() * 0.5,
                    );
                    if let Some(state) = builder.states.get_mut(container) {
                        state.bounds = local_root_bounds;
                        state.transform.translation = root_center;
                        builder
                            .commands
                            .entity(state.entity)
                            .insert((LocalBounds(local_root_bounds), state.transform));
                    }

                    let mut targets = Vec::new();
                    let mut cover_clips = Vec::new();
                    for (layout_id, member) in &materialized_by_id {
                        if *layout_id == root_id {
                            continue;
                        }
                        let Some(target_box) = resolved.boxes.get(layout_id).copied() else {
                            continue;
                        };
                        let Some(parent_id) = tree.parent_by_id.get(layout_id) else {
                            continue;
                        };
                        let Some(parent_box) = resolved.boxes.get(parent_id).copied() else {
                            continue;
                        };
                        let parent_center = parent_box.bounds.center();
                        if tree.children_by_id.contains_key(layout_id) {
                            let local_bounds = Bounds3D::new_2d(
                                -target_box.bounds.width() * 0.5,
                                -target_box.bounds.height() * 0.5,
                                target_box.bounds.width() * 0.5,
                                target_box.bounds.height() * 0.5,
                            );
                            let Some(state) = builder.states.get_mut(*member) else {
                                continue;
                            };
                            let mut target = state.transform;
                            target.translation = target_box.bounds.center() - parent_center;
                            state.bounds = local_bounds;
                            state.transform = target;
                            builder
                                .commands
                                .entity(state.entity)
                                .insert((LocalBounds(local_bounds), target));
                            targets.push((*member, target));
                            continue;
                        }
                        let Some(state) = builder.states.get_mut(*member) else {
                            continue;
                        };
                        let mut zero_translation = state.transform;
                        zero_translation.translation = DVec3::ZERO;
                        let intrinsic =
                            gaanim_layout::transform_bounds(state.bounds, &zero_translation);
                        let target_center = target_box.bounds.center() - parent_center;
                        let intrinsic_center = intrinsic.center();
                        let mut target = state.transform;
                        target.translation = target_center - intrinsic_center;
                        let sx = target_box.bounds.width() / intrinsic.width().max(1.0e-9);
                        let sy = target_box.bounds.height() / intrinsic.height().max(1.0e-9);
                        let item_style = tree
                            .item_style_by_id
                            .get(layout_id)
                            .cloned()
                            .unwrap_or_default();
                        let fit = match item_style.fit {
                            gaanim_layout::FitMode::None => DVec3::ONE,
                            gaanim_layout::FitMode::Contain => DVec3::splat(sx.min(sy)),
                            gaanim_layout::FitMode::Cover => DVec3::splat(sx.max(sy)),
                            gaanim_layout::FitMode::Stretch => DVec3::new(sx, sy, 1.0),
                            gaanim_layout::FitMode::ScaleDown => DVec3::splat(sx.min(sy).min(1.0)),
                        };
                        target.scale *= fit;
                        if matches!(item_style.fit, gaanim_layout::FitMode::Cover) {
                            cover_clips.push((*member, target_box.bounds));
                        }
                        targets.push((*member, target));
                        state.transform = target;
                        builder.commands.entity(state.entity).insert(target);
                    }
                    for (member, clip_bounds) in cover_clips {
                        let world_path = Rect::new(
                            clip_bounds.min.x,
                            clip_bounds.min.y,
                            clip_bounds.max.x,
                            clip_bounds.max.y,
                        )
                        .to_path(0.1);
                        for leaf in Self::visual_leaf_ids(builder, member) {
                            let Some(state) = builder.states.get(leaf) else {
                                continue;
                            };
                            let mut local_path = world_path.clone();
                            local_path.apply_affine(
                                builder.get_world_transform(leaf).to_affine_2d().inverse(),
                            );
                            builder.commands.entity(state.entity).insert(
                                gaanim_renderer::effects::ClipMask {
                                    path: local_path,
                                    rule: gaanim_core::peniko::Fill::NonZero,
                                },
                            );
                        }
                    }
                    // Arrangement writes the final transforms. Restore the
                    // layout visible at the current timeline cursor, then let
                    // the regular animation machinery interpolate to the new
                    // arrangement and advance its cursor.
                    if duration.is_some() {
                        for (member, transform) in before {
                            if let Some(state) = builder.states.get_mut(member) {
                                state.transform = transform;
                                builder.commands.entity(state.entity).insert(transform);
                            }
                        }
                    }
                    let transition_duration = (*duration).unwrap_or(0.0);
                    let mut animations: Vec<AnimationBuilder> = if duration.is_some() {
                        targets
                            .into_iter()
                            .flat_map(|(target, to)| {
                                [
                                    AnimationBuilder {
                                        target,
                                        anim_type: AnimationType::TranslateTo {
                                            to: to.translation,
                                        },
                                        duration: transition_duration,
                                        rate_func: RateFunc::Smooth,
                                        delay: 0.0,
                                    },
                                    AnimationBuilder {
                                        target,
                                        anim_type: AnimationType::ScaleTo { to: to.scale },
                                        duration: transition_duration,
                                        rate_func: RateFunc::Smooth,
                                        delay: 0.0,
                                    },
                                ]
                            })
                            .collect()
                    } else {
                        Vec::new()
                    };
                    for (old, new) in text_crossfades {
                        animations.push(AnimationBuilder {
                            target: old,
                            anim_type: AnimationType::FadeOut,
                            duration: transition_duration,
                            rate_func: RateFunc::Smooth,
                            delay: 0.0,
                        });
                        animations.push(AnimationBuilder {
                            target: new,
                            anim_type: AnimationType::FadeIn,
                            duration: transition_duration,
                            rate_func: RateFunc::Smooth,
                            delay: 0.0,
                        });
                    }
                    if let Some(entering) = entering
                        .as_ref()
                        .and_then(|member| id_map.get(member))
                        .copied()
                    {
                        animations.push(AnimationBuilder {
                            target: entering,
                            anim_type: AnimationType::FadeIn,
                            duration: transition_duration,
                            rate_func: RateFunc::Smooth,
                            delay: 0.0,
                        });
                    }
                    if let Some(leaving) = leaving
                        .as_ref()
                        .and_then(|member| id_map.get(member))
                        .copied()
                    {
                        animations.push(AnimationBuilder {
                            target: leaving,
                            anim_type: AnimationType::FadeOut,
                            duration: transition_duration,
                            rate_func: RateFunc::Smooth,
                            delay: 0.0,
                        });
                    }
                    if !animations.is_empty() {
                        builder.play_parallel(animations);
                    }
                }
                Op::LayoutConstraints {
                    constraints,
                    duration,
                } => {
                    let remap_expression = |expression: &gaanim_layout::LayoutExpression| {
                        let mut mapped = gaanim_layout::LayoutExpression::from(expression.constant);
                        for (variable, coefficient) in &expression.terms {
                            let source = ObjectId::from_raw(variable.node.0);
                            let Some(target) = id_map.get(&source).copied() else {
                                continue;
                            };
                            mapped = mapped
                                + gaanim_layout::LayoutExpression::variable(
                                    gaanim_layout::LayoutId(target.as_raw()),
                                    variable.attribute,
                                ) * *coefficient;
                        }
                        mapped
                    };
                    let mapped: Vec<_> = constraints
                        .iter()
                        .map(|constraint| gaanim_layout::LayoutConstraint {
                            lhs: remap_expression(&constraint.lhs),
                            relation: constraint.relation,
                            rhs: remap_expression(&constraint.rhs),
                            strength: constraint.strength,
                            label: constraint.label.clone(),
                        })
                        .collect();
                    let referenced: std::collections::BTreeSet<_> = mapped
                        .iter()
                        .flat_map(|constraint| {
                            constraint
                                .lhs
                                .terms
                                .keys()
                                .chain(constraint.rhs.terms.keys())
                                .map(|variable| variable.node)
                        })
                        .collect();
                    let mut resolved = gaanim_layout::ResolvedLayout::default();
                    for layout_id in &referenced {
                        let object = ObjectId::from_raw(layout_id.0);
                        let Some(state) = builder.states.get(object) else {
                            continue;
                        };
                        let world_transform = builder.get_world_transform(object);
                        resolved.boxes.insert(
                            *layout_id,
                            gaanim_layout::ResolvedBox {
                                bounds: gaanim_layout::transform_bounds(
                                    state.bounds,
                                    &world_transform,
                                ),
                                clip: None,
                                scale: DVec3::ONE,
                            },
                        );
                    }
                    gaanim_layout::solve_constraints(&mut resolved, &mapped).unwrap_or_else(
                        |error| panic!("layout constraint resolution failed: {error}"),
                    );
                    if !resolved.diagnostics.is_empty() {
                        let mut state = diagnostic_state.lock().expect("canvas state poisoned");
                        state
                            .layout_diagnostics
                            .extend(resolved.diagnostics.iter().map(|diagnostic| {
                                (
                                    None,
                                    format!(
                                        "constraint #{}: {} (residual {:.6})",
                                        diagnostic.constraint,
                                        diagnostic.message,
                                        diagnostic.residual
                                    ),
                                )
                            }));
                    }

                    let mut targets = Vec::new();
                    for layout_id in referenced {
                        let object = ObjectId::from_raw(layout_id.0);
                        let Some(target_box) = resolved.boxes.get(&layout_id).copied() else {
                            continue;
                        };
                        let Some(state) = builder.states.get(object) else {
                            continue;
                        };
                        let world_transform = builder.get_world_transform(object);
                        let current =
                            gaanim_layout::transform_bounds(state.bounds, &world_transform);
                        let sx = target_box.bounds.width() / current.width().max(1.0e-9);
                        let sy = target_box.bounds.height() / current.height().max(1.0e-9);
                        let current_center = current.center();
                        let target_center = target_box.bounds.center();
                        let desired_world = gaanim_core::kurbo::Affine::translate((
                            target_center.x,
                            target_center.y,
                        )) * gaanim_core::kurbo::Affine::scale_non_uniform(
                            sx, sy,
                        ) * gaanim_core::kurbo::Affine::translate((
                            -current_center.x,
                            -current_center.y,
                        )) * world_transform.to_affine_2d();
                        let parent_world = state
                            .parent
                            .map(|parent| builder.get_world_transform(parent).to_affine_2d())
                            .unwrap_or(gaanim_core::kurbo::Affine::IDENTITY);
                        let target = SpatialTransform::from_affine_2d(
                            &(parent_world.inverse() * desired_world),
                        );
                        targets.push((object, state.transform, target));
                    }
                    for (object, _, target) in &targets {
                        let Some(state) = builder.states.get_mut(*object) else {
                            continue;
                        };
                        state.transform = *target;
                        builder.commands.entity(state.entity).insert(*target);
                    }
                    let Some(duration) = duration else {
                        continue;
                    };
                    let mut animations = Vec::new();
                    for (object, before, target) in targets {
                        if let Some(state) = builder.states.get_mut(object) {
                            state.transform = before;
                            builder.commands.entity(state.entity).insert(before);
                        }
                        animations.push(AnimationBuilder {
                            target: object,
                            anim_type: AnimationType::TranslateTo {
                                to: target.translation,
                            },
                            duration: *duration,
                            rate_func: RateFunc::Smooth,
                            delay: 0.0,
                        });
                        animations.push(AnimationBuilder {
                            target: object,
                            anim_type: AnimationType::ScaleTo { to: target.scale },
                            duration: *duration,
                            rate_func: RateFunc::Smooth,
                            delay: 0.0,
                        });
                    }
                    builder.play_parallel(animations);
                }
                Op::Wait(d) => builder.wait(*d),
                Op::CameraPosition { to, duration, .. } => {
                    builder.timeline.add_clip(
                        builder.default_track,
                        builder.current_time,
                        *duration,
                        gaanim_timeline::clip::ClipPayload::Animation(
                            gaanim_timeline::clip::AnimationSpec {
                                target: gaanim_core::ObjectId::from_parts(0, 1),
                                lens: gaanim_timeline::clip::PropertyLensSpec::CameraPosition {
                                    from: *camera_position,
                                    to: *to,
                                },
                                rate_func: gaanim_math::RateFunc::Smooth,
                                delay: 0.0,
                                label: None,
                            },
                        ),
                    );
                    *camera_position = *to;
                    builder.wait(*duration);
                }
                Op::CameraZoom { to, duration, .. } => {
                    builder.timeline.add_clip(
                        builder.default_track,
                        builder.current_time,
                        *duration,
                        gaanim_timeline::clip::ClipPayload::Animation(
                            gaanim_timeline::clip::AnimationSpec {
                                target: gaanim_core::ObjectId::from_parts(0, 1),
                                lens: gaanim_timeline::clip::PropertyLensSpec::CameraZoom {
                                    from: *camera_zoom,
                                    to: *to,
                                },
                                rate_func: gaanim_math::RateFunc::Smooth,
                                delay: 0.0,
                                label: None,
                            },
                        ),
                    );
                    *camera_zoom = *to;
                    builder.wait(*duration);
                }
                Op::CameraRotation { to, duration } => {
                    builder.timeline.add_clip(
                        builder.default_track,
                        builder.current_time,
                        *duration,
                        gaanim_timeline::clip::ClipPayload::Animation(
                            gaanim_timeline::clip::AnimationSpec {
                                target: gaanim_core::ObjectId::from_parts(0, 1),
                                lens: gaanim_timeline::clip::PropertyLensSpec::CameraRotation {
                                    from: *camera_rotation,
                                    to: *to,
                                },
                                rate_func: gaanim_math::RateFunc::Smooth,
                                delay: 0.0,
                                label: None,
                            },
                        ),
                    );
                    *camera_rotation = *to;
                    builder.wait(*duration);
                }
                Op::CameraFrame {
                    target,
                    margin,
                    duration,
                } => {
                    let Some(actual) = id_map.get(target).copied() else {
                        continue;
                    };
                    let Some(state) = builder.states.get(actual) else {
                        continue;
                    };
                    let bounds = state
                        .bounds
                        .transform_2d(&builder.get_world_transform(actual).to_affine_2d());
                    let width = (bounds.width() + margin * 2.0).max(1.0);
                    let height = (bounds.height() + margin * 2.0).max(1.0);
                    let zoom = (frame_bounds.width() / width)
                        .min(frame_bounds.height() / height)
                        .max(0.01);
                    let center = bounds.center();
                    for lens in [
                        gaanim_timeline::clip::PropertyLensSpec::CameraPosition {
                            from: *camera_position,
                            to: center,
                        },
                        gaanim_timeline::clip::PropertyLensSpec::CameraZoom {
                            from: *camera_zoom,
                            to: zoom,
                        },
                    ] {
                        builder.timeline.add_clip(
                            builder.default_track,
                            builder.current_time,
                            *duration,
                            gaanim_timeline::clip::ClipPayload::Animation(
                                gaanim_timeline::clip::AnimationSpec {
                                    target: gaanim_core::ObjectId::from_parts(0, 1),
                                    lens,
                                    rate_func: gaanim_math::RateFunc::Smooth,
                                    delay: 0.0,
                                    label: None,
                                },
                            ),
                        );
                    }
                    *camera_position = center;
                    *camera_zoom = zoom;
                    builder.wait(*duration);
                }
                Op::CameraFollow { target, duration } => {
                    let Some(actual) = id_map.get(target).copied() else {
                        continue;
                    };
                    builder.timeline.add_clip(
                        builder.default_track,
                        builder.current_time,
                        *duration,
                        gaanim_timeline::clip::ClipPayload::Animation(
                            gaanim_timeline::clip::AnimationSpec {
                                target: gaanim_core::ObjectId::from_parts(0, 1),
                                lens: gaanim_timeline::clip::PropertyLensSpec::CameraFollow {
                                    target: actual,
                                },
                                rate_func: gaanim_math::RateFunc::Linear,
                                delay: 0.0,
                                label: None,
                            },
                        ),
                    );
                    if let Some(state) = builder.states.get(actual) {
                        camera_position.x = state.transform.translation.x;
                        camera_position.y = state.transform.translation.y;
                    }
                    builder.wait(*duration);
                }
                Op::CameraShake {
                    amplitude,
                    frequency,
                    duration,
                } => {
                    builder.timeline.add_clip(
                        builder.default_track,
                        builder.current_time,
                        *duration,
                        gaanim_timeline::clip::ClipPayload::Animation(
                            gaanim_timeline::clip::AnimationSpec {
                                target: gaanim_core::ObjectId::from_parts(0, 1),
                                lens: gaanim_timeline::clip::PropertyLensSpec::CameraShake {
                                    origin: *camera_position,
                                    amplitude: *amplitude,
                                    frequency: *frequency,
                                },
                                rate_func: gaanim_math::RateFunc::Linear,
                                delay: 0.0,
                                label: None,
                            },
                        ),
                    );
                    builder.wait(*duration);
                }
                Op::CameraLookAt {
                    eye,
                    target,
                    up,
                    duration,
                } => {
                    let from_eye = *camera_position;
                    let to_eye = *eye;
                    let from_target = *camera_target;
                    let to_target = *target;
                    // Compute rotations via look_at
                    let from_rot = *camera_rotation;
                    let view_to = gaanim_core::glam::DMat4::look_at_rh(to_eye, to_target, *up);
                    let to_rot = view_to.inverse().to_scale_rotation_translation().1;
                    // Position clip
                    builder.timeline.add_clip(
                        builder.default_track,
                        builder.current_time,
                        *duration,
                        gaanim_timeline::clip::ClipPayload::Animation(
                            gaanim_timeline::clip::AnimationSpec {
                                target: gaanim_core::ObjectId::from_parts(0, 1),
                                lens: gaanim_timeline::clip::PropertyLensSpec::CameraPosition {
                                    from: from_eye,
                                    to: to_eye,
                                },
                                rate_func: gaanim_math::RateFunc::Smooth,
                                delay: 0.0,
                                label: None,
                            },
                        ),
                    );
                    // Target clip
                    builder.timeline.add_clip(
                        builder.default_track,
                        builder.current_time,
                        *duration,
                        gaanim_timeline::clip::ClipPayload::Animation(
                            gaanim_timeline::clip::AnimationSpec {
                                target: gaanim_core::ObjectId::from_parts(0, 1),
                                lens: gaanim_timeline::clip::PropertyLensSpec::CameraTarget {
                                    from: from_target,
                                    to: to_target,
                                },
                                rate_func: gaanim_math::RateFunc::Smooth,
                                delay: 0.0,
                                label: None,
                            },
                        ),
                    );
                    // Rotation clip
                    builder.timeline.add_clip(
                        builder.default_track,
                        builder.current_time,
                        *duration,
                        gaanim_timeline::clip::ClipPayload::Animation(
                            gaanim_timeline::clip::AnimationSpec {
                                target: gaanim_core::ObjectId::from_parts(0, 1),
                                lens: gaanim_timeline::clip::PropertyLensSpec::CameraRotation {
                                    from: from_rot,
                                    to: to_rot,
                                },
                                rate_func: gaanim_math::RateFunc::Smooth,
                                delay: 0.0,
                                label: None,
                            },
                        ),
                    );
                    *camera_position = to_eye;
                    *camera_target = to_target;
                    *camera_rotation = to_rot;
                    builder.wait(*duration);
                }
                Op::CameraOrbit {
                    delta_yaw,
                    delta_pitch,
                    duration,
                } => {
                    // Compute destination via spherical orbit around target
                    let mut temp_cam = gaanim_math::Camera::ortho_2d(1, 1);
                    temp_cam.position = *camera_position;
                    temp_cam.target = *camera_target;
                    temp_cam.rotation = *camera_rotation;
                    temp_cam.up = gaanim_core::glam::DVec3::Y;
                    temp_cam.projection = if let Some((fov, near, far)) = *camera_fov {
                        gaanim_math::Projection::Perspective {
                            fov_y: fov,
                            near,
                            far,
                        }
                    } else {
                        gaanim_math::Projection::Orthographic { zoom: *camera_zoom }
                    };
                    let from_pos = *camera_position;
                    let from_rot = *camera_rotation;
                    temp_cam
                        .orbit_around_target(*delta_yaw, *delta_pitch)
                        .expect("camera orbit requires a finite, non-degenerate pose");
                    let to_pos = temp_cam.position;
                    let to_rot = temp_cam.rotation;
                    builder.timeline.add_clip(
                        builder.default_track,
                        builder.current_time,
                        *duration,
                        gaanim_timeline::clip::ClipPayload::Animation(
                            gaanim_timeline::clip::AnimationSpec {
                                target: gaanim_core::ObjectId::from_parts(0, 1),
                                lens: gaanim_timeline::clip::PropertyLensSpec::CameraPosition {
                                    from: from_pos,
                                    to: to_pos,
                                },
                                rate_func: gaanim_math::RateFunc::Smooth,
                                delay: 0.0,
                                label: None,
                            },
                        ),
                    );
                    builder.timeline.add_clip(
                        builder.default_track,
                        builder.current_time,
                        *duration,
                        gaanim_timeline::clip::ClipPayload::Animation(
                            gaanim_timeline::clip::AnimationSpec {
                                target: gaanim_core::ObjectId::from_parts(0, 1),
                                lens: gaanim_timeline::clip::PropertyLensSpec::CameraRotation {
                                    from: from_rot,
                                    to: to_rot,
                                },
                                rate_func: gaanim_math::RateFunc::Smooth,
                                delay: 0.0,
                                label: None,
                            },
                        ),
                    );
                    *camera_position = to_pos;
                    *camera_rotation = to_rot;
                    builder.wait(*duration);
                }
                Op::CameraPerspective {
                    fov_y,
                    near,
                    far,
                    duration,
                } => {
                    let (from_fov, from_near, from_far) =
                        (*camera_fov).unwrap_or((std::f64::consts::FRAC_PI_4, 0.1, 1000.0));
                    builder.timeline.add_clip(
                        builder.default_track,
                        builder.current_time,
                        *duration,
                        gaanim_timeline::clip::ClipPayload::Animation(
                            gaanim_timeline::clip::AnimationSpec {
                                target: gaanim_core::ObjectId::from_parts(0, 1),
                                lens: gaanim_timeline::clip::PropertyLensSpec::CameraPerspective {
                                    from_fov,
                                    to_fov: *fov_y,
                                    from_near,
                                    to_near: *near,
                                    from_far,
                                    to_far: *far,
                                },
                                rate_func: gaanim_math::RateFunc::Smooth,
                                delay: 0.0,
                                label: None,
                            },
                        ),
                    );
                    *camera_fov = Some((*fov_y, *near, *far));
                    builder.wait(*duration);
                }
                Op::CameraDolly { factor, duration } => {
                    let from_pos = *camera_position;
                    let dir = from_pos - *camera_target;
                    let to_pos = *camera_target + dir * *factor;
                    builder.timeline.add_clip(
                        builder.default_track,
                        builder.current_time,
                        *duration,
                        gaanim_timeline::clip::ClipPayload::Animation(
                            gaanim_timeline::clip::AnimationSpec {
                                target: gaanim_core::ObjectId::from_parts(0, 1),
                                lens: gaanim_timeline::clip::PropertyLensSpec::CameraPosition {
                                    from: from_pos,
                                    to: to_pos,
                                },
                                rate_func: gaanim_math::RateFunc::Smooth,
                                delay: 0.0,
                                label: None,
                            },
                        ),
                    );
                    *camera_position = to_pos;
                    builder.wait(*duration);
                }
                Op::SetClip { target, mask, rule } => {
                    let Some(target) = id_map.get(target).copied() else {
                        continue;
                    };
                    let target_leaves = Self::visual_leaf_ids(builder, target);
                    if let Some(mask) = mask {
                        let Some(mask) = id_map.get(mask).copied() else {
                            continue;
                        };
                        let mask_world = Self::mask_path_in_world(builder, mask);
                        for leaf in target_leaves {
                            let Some(state) = builder.states.get(leaf) else {
                                continue;
                            };
                            let mut local_path = mask_world.clone();
                            local_path.apply_affine(
                                builder.get_world_transform(leaf).to_affine_2d().inverse(),
                            );
                            builder.commands.entity(state.entity).insert(
                                gaanim_renderer::effects::ClipMask {
                                    path: local_path,
                                    rule: *rule,
                                },
                            );
                        }
                    } else {
                        for leaf in target_leaves {
                            if let Some(state) = builder.states.get(leaf) {
                                builder
                                    .commands
                                    .entity(state.entity)
                                    .remove::<gaanim_renderer::effects::ClipMask>();
                            }
                        }
                    }
                }
                Op::Stop => builder.stop(),
                Op::Show(id) => {
                    if let Some(id) = id_map.get(id).copied()
                        && let Some(st) = builder.states.get_mut(id)
                    {
                        builder.commands.entity(st.entity).insert(Visible);
                    }
                }
                Op::Hide(id) => {
                    if let Some(id) = id_map.get(id).copied()
                        && let Some(st) = builder.states.get_mut(id)
                    {
                        builder.commands.entity(st.entity).remove::<Visible>();
                    }
                }
                Op::Remove(id) => {
                    if let Some(id) = id_map.get(id).copied()
                        && let Some(st) = builder.states.get(id)
                    {
                        builder.commands.entity(st.entity).despawn();
                    }
                }
                Op::AttachToGroup { group, child } => {
                    if let (Some(group), Some(child)) =
                        (id_map.get(group).copied(), id_map.get(child).copied())
                        && builder.states.get(group).is_some()
                        && builder.states.get(child).is_some()
                    {
                        builder.add_to_group(MobjectRef { id: group }, MobjectRef { id: child });
                    }
                }
                Op::PlaceAtCoordinate {
                    space,
                    target,
                    local,
                } => {
                    if let (Some(space), Some(target)) =
                        (id_map.get(space).copied(), id_map.get(target).copied())
                        && builder.states.get(space).is_some()
                        && builder.states.get(target).is_some()
                    {
                        builder.add_to_group(MobjectRef { id: space }, MobjectRef { id: target });
                        let view_local = builder
                            .states
                            .get(space)
                            .map(|s| s.transform)
                            .unwrap_or_default();
                        let inv = view_local.to_affine_2d().inverse();
                        let desired_point = Point::new(local.x, local.y);
                        let local_point = inv * desired_point;
                        if let Some(state) = builder.states.get_mut(target) {
                            state.transform.translation =
                                DVec3::new(local_point.x, local_point.y, local.z);
                            // Preserve any existing rotation/scale from add_to_group (which is identity for dots)
                            // but ensure translation is correct.
                            builder
                                .commands
                                .entity(state.entity)
                                .insert(state.transform);
                        }
                    }
                }
                Op::Reuse(target) => Self::apply_scene_object_scope(
                    builder,
                    *target,
                    SceneObjectScopeAction::Reuse,
                    scene_id,
                    previous_scene,
                    seg.transition.as_ref(),
                    scene_start,
                    id_map,
                    object_scopes,
                ),
                Op::Persist(target) => Self::apply_scene_object_scope(
                    builder,
                    *target,
                    SceneObjectScopeAction::Persist,
                    scene_id,
                    previous_scene,
                    seg.transition.as_ref(),
                    scene_start,
                    id_map,
                    object_scopes,
                ),
                Op::Release(target) => Self::apply_scene_object_scope(
                    builder,
                    *target,
                    SceneObjectScopeAction::Release,
                    scene_id,
                    previous_scene,
                    seg.transition.as_ref(),
                    scene_start,
                    id_map,
                    object_scopes,
                ),

                // -- Reactive ops --
                Op::AttachUpdater { target, preset } => {
                    if let Some(target_id) = id_map.get(target).copied()
                        && let Some(st) = builder.states.get(target_id)
                    {
                        let updater: Updater = preset
                            .clone()
                            .into_updater()
                            .starting_at(builder.current_time);
                        builder.commands.entity(st.entity).insert(updater);
                    }
                }

                Op::RemoveUpdater(target) => {
                    if let Some(target_id) = id_map.get(target).copied() {
                        builder.schedule_remove_updater(target_id);
                    }
                }

                Op::AttachTracedPath {
                    target,
                    source,
                    min_distance,
                    max_points,
                    dissipating_time,
                } => {
                    if let Some(target_id) = id_map.get(target).copied()
                        && let Some(source_id) = id_map.get(source).copied()
                        && let Some(target_st) = builder.states.get(target_id)
                        && let Some(source_st) = builder.states.get(source_id)
                    {
                        let traced = TracedPath::new(source_st.entity, *min_distance, *max_points)
                            .starting_at(builder.current_time)
                            .with_dissipating_time(*dissipating_time);
                        builder.commands.entity(target_st.entity).insert(traced);
                    }
                }

                Op::AttachPositionBinding {
                    target,
                    source,
                    axes,
                } => {
                    if let Some(target_id) = id_map.get(target).copied()
                        && let Some(source_id) = id_map.get(source).copied()
                        && let Some(target_st) = builder.states.get(target_id)
                        && let Some(source_st) = builder.states.get(source_id)
                    {
                        let binding = PositionBinding::new(source_st.entity, *axes);
                        builder.commands.entity(target_st.entity).insert(binding);
                    }
                }

                Op::AttachPositionFollow {
                    target,
                    source,
                    offset,
                } => {
                    if let Some(target_id) = id_map.get(target).copied()
                        && let Some(source_id) = id_map.get(source).copied()
                        && let Some(target_st) = builder.states.get(target_id)
                        && let Some(source_st) = builder.states.get(source_id)
                    {
                        builder.commands.entity(target_st.entity).insert(
                            PositionBinding::with_offset(
                                source_st.entity,
                                gaanim_animation::AxisMask::XY,
                                *offset,
                            ),
                        );
                    }
                }

                Op::AttachEndpointFollow {
                    target,
                    endpoint,
                    offset,
                    offset_space,
                } => {
                    if let Some(target_id) = id_map.get(target).copied()
                        && let Some(target_state) = builder.states.get(target_id)
                    {
                        builder
                            .commands
                            .entity(target_state.entity)
                            .insert(EndpointFollow {
                                endpoint: compile_tracking_endpoint(
                                    endpoint,
                                    &id_map,
                                    &builder.states,
                                ),
                                offset: *offset,
                                offset_space: *offset_space,
                            });
                    }
                }

                Op::AttachTrackingLine { target, from, to } => {
                    if let Some(target_id) = id_map.get(target).copied()
                        && let Some(st) = builder.states.get(target_id)
                    {
                        let line = TrackingLine::new(
                            compile_tracking_endpoint(from, &id_map, &builder.states),
                            compile_tracking_endpoint(to, &id_map, &builder.states),
                        );
                        builder.commands.entity(st.entity).insert(line);
                    }
                }

                Op::AttachTrackingSpring {
                    target,
                    from,
                    to,
                    coils,
                    amplitude,
                    crossing,
                    start_straight,
                    end_straight,
                } => {
                    if let Some(target_id) = id_map.get(target).copied()
                        && let Some(st) = builder.states.get(target_id)
                    {
                        let from = compile_tracking_endpoint(from, &id_map, &builder.states);
                        let to = compile_tracking_endpoint(to, &id_map, &builder.states);
                        let coils = *coils;
                        let amplitude = *amplitude;
                        let crossing = *crossing;
                        let start_straight = *start_straight;
                        let end_straight = *end_straight;
                        let target_entity = st.entity;
                        let redraw = gaanim_animation::AlwaysRedrawRegen::new(move |world| {
                            let endpoint_position = |endpoint: &TrackingEndpoint| match endpoint {
                                TrackingEndpoint::Static(position) => *position,
                                _ => gaanim_animation::resolve_tracking_endpoint(endpoint, world)
                                    .unwrap_or(DVec3::ZERO),
                            };
                            let from = gaanim_animation::tracking_world_to_local(
                                target_entity,
                                endpoint_position(&from),
                                world,
                            );
                            let to = gaanim_animation::tracking_world_to_local(
                                target_entity,
                                endpoint_position(&to),
                                world,
                            );
                            // Rebuild the projected helix every frame so an animated
                            // endpoint changes the spring pitch in lockstep.
                            gaanim_objects::primitives::spring_path(
                                Point::new(from.x, from.y),
                                Point::new(to.x, to.y),
                                coils,
                                amplitude,
                                crossing,
                                start_straight,
                                end_straight,
                            )
                        });
                        builder.commands.entity(st.entity).insert(redraw);
                    }
                }

                Op::AttachTrackingDimension {
                    line,
                    extensions,
                    from,
                    to,
                    offset,
                    line_width,
                    extension_dash,
                } => {
                    let from = compile_tracking_endpoint(from, &id_map, &builder.states);
                    let to = compile_tracking_endpoint(to, &id_map, &builder.states);
                    let offset = *offset;
                    let line_width = *line_width;
                    let extension_dash = *extension_dash;
                    for (target, is_extensions) in [(line, false), (extensions, true)] {
                        if let Some(target_id) = id_map.get(target).copied()
                            && let Some(st) = builder.states.get(target_id)
                        {
                            let from = from.clone();
                            let to = to.clone();
                            let target_entity = st.entity;
                            let redraw = gaanim_animation::AlwaysRedrawRegen::new(move |world| {
                                let endpoint_position = |endpoint: &TrackingEndpoint| match endpoint
                                {
                                    TrackingEndpoint::Static(position) => *position,
                                    _ => {
                                        gaanim_animation::resolve_tracking_endpoint(endpoint, world)
                                            .unwrap_or(DVec3::ZERO)
                                    }
                                };
                                let from = gaanim_animation::tracking_world_to_local(
                                    target_entity,
                                    endpoint_position(&from),
                                    world,
                                );
                                let to = gaanim_animation::tracking_world_to_local(
                                    target_entity,
                                    endpoint_position(&to),
                                    world,
                                );
                                let start = Point::new(from.x, from.y);
                                let end = Point::new(to.x, to.y);
                                if is_extensions {
                                    gaanim_objects::primitives::dimension_extensions_path(
                                        start,
                                        end,
                                        offset,
                                        line_width,
                                        extension_dash,
                                    )
                                } else {
                                    gaanim_objects::primitives::dimension_measure_path(
                                        start, end, offset, line_width,
                                    )
                                }
                            });
                            builder.commands.entity(st.entity).insert(redraw);
                        }
                    }
                }

                Op::AttachEndpointDistance {
                    target,
                    from,
                    to,
                    scale,
                } => {
                    if let Some(target_id) = id_map.get(target).copied()
                        && let Some(st) = builder.states.get(target_id)
                    {
                        builder.commands.entity(st.entity).insert(EndpointDistance {
                            from: compile_tracking_endpoint(from, &id_map, &builder.states),
                            to: compile_tracking_endpoint(to, &id_map, &builder.states),
                            scale: *scale,
                        });
                    }
                }

                Op::AttachDimensionLabelPlacement {
                    target,
                    label,
                    from,
                    to,
                    offset,
                    gap,
                    orientation,
                } => {
                    if let (Some(target_id), Some(label_id)) =
                        (id_map.get(target).copied(), id_map.get(label).copied())
                        && let (Some(target_state), Some(label_state)) =
                            (builder.states.get(target_id), builder.states.get(label_id))
                    {
                        builder.commands.entity(target_state.entity).insert(
                            DimensionLabelPlacement {
                                label: label_state.entity,
                                from: compile_tracking_endpoint(from, &id_map, &builder.states),
                                to: compile_tracking_endpoint(to, &id_map, &builder.states),
                                offset: *offset,
                                gap: *gap,
                                orientation: *orientation,
                            },
                        );
                    }
                }

                Op::AttachTrackingAngle {
                    arc,
                    arrows,
                    extensions,
                    vertex,
                    from,
                    to,
                    radius,
                    sweep,
                    arrowheads,
                } => {
                    let vertex = compile_tracking_endpoint(vertex, &id_map, &builder.states);
                    let from = compile_tracking_ray(from, &id_map, &builder.states);
                    let to = compile_tracking_ray(to, &id_map, &builder.states);
                    for (target, part) in [
                        (arc, TrackingAnglePart::Arc),
                        (arrows, TrackingAnglePart::Arrows),
                        (extensions, TrackingAnglePart::Extensions),
                    ] {
                        if let Some(runtime) = id_map.get(target).copied()
                            && let Some(state) = builder.states.get(runtime)
                        {
                            builder.commands.entity(state.entity).insert(TrackingAngle {
                                vertex: vertex.clone(),
                                from: from.clone(),
                                to: to.clone(),
                                radius: *radius,
                                sweep: *sweep,
                                arrowheads: *arrowheads,
                                part,
                            });
                        }
                    }
                }

                Op::AttachEndpointAngle {
                    target,
                    vertex,
                    from,
                    to,
                    sweep,
                    scale,
                } => {
                    if let Some(runtime) = id_map.get(target).copied()
                        && let Some(state) = builder.states.get(runtime)
                    {
                        builder.commands.entity(state.entity).insert(EndpointAngle {
                            vertex: compile_tracking_endpoint(vertex, &id_map, &builder.states),
                            from: compile_tracking_ray(from, &id_map, &builder.states),
                            to: compile_tracking_ray(to, &id_map, &builder.states),
                            sweep: *sweep,
                            scale: *scale,
                        });
                    }
                }

                Op::AttachAngleLabelPlacement {
                    target,
                    label,
                    vertex,
                    from,
                    to,
                    radius,
                    gap,
                    sweep,
                    orientation,
                } => {
                    if let (Some(target_runtime), Some(label_runtime)) =
                        (id_map.get(target).copied(), id_map.get(label).copied())
                        && let (Some(target_state), Some(label_state)) = (
                            builder.states.get(target_runtime),
                            builder.states.get(label_runtime),
                        )
                    {
                        builder
                            .commands
                            .entity(target_state.entity)
                            .insert(AngleLabelPlacement {
                                label: label_state.entity,
                                vertex: compile_tracking_endpoint(vertex, &id_map, &builder.states),
                                from: compile_tracking_ray(from, &id_map, &builder.states),
                                to: compile_tracking_ray(to, &id_map, &builder.states),
                                radius: *radius,
                                gap: *gap,
                                sweep: *sweep,
                                orientation: *orientation,
                            });
                    }
                }

                Op::AttachTrackingVectorHead {
                    target,
                    from,
                    to,
                    length,
                    width,
                } => {
                    if let Some(runtime) = id_map.get(target).copied()
                        && let Some(state) = builder.states.get(runtime)
                    {
                        builder
                            .commands
                            .entity(state.entity)
                            .insert(TrackingVectorHead {
                                from: compile_tracking_endpoint(from, &id_map, &builder.states),
                                to: compile_tracking_endpoint(to, &id_map, &builder.states),
                                length: *length,
                                width: *width,
                            });
                    }
                }

                Op::AttachRotationBinding {
                    target,
                    source,
                    ratio,
                    phase,
                } => {
                    if let (Some(target_runtime), Some(source_runtime)) =
                        (id_map.get(target).copied(), id_map.get(source).copied())
                        && let (Some(target_state), Some(source_state)) = (
                            builder.states.get(target_runtime),
                            builder.states.get(source_runtime),
                        )
                    {
                        builder
                            .commands
                            .entity(target_state.entity)
                            .insert(RotationBinding {
                                source: source_state.entity,
                                ratio: *ratio,
                                phase: *phase,
                            });
                    }
                }

                Op::AttachRotationTranslationBinding {
                    target,
                    source,
                    axis,
                    scale,
                } => {
                    if let (Some(target_runtime), Some(source_runtime)) =
                        (id_map.get(target).copied(), id_map.get(source).copied())
                        && let (Some(target_state), Some(source_state)) = (
                            builder.states.get(target_runtime),
                            builder.states.get(source_runtime),
                        )
                    {
                        builder.commands.entity(target_state.entity).insert(
                            RotationTranslationBinding {
                                source: source_state.entity,
                                axis: *axis,
                                scale: *scale,
                                base_position: None,
                                base_angle: None,
                            },
                        );
                    }
                }

                Op::AttachTrackerArc {
                    target,
                    tracker,
                    center,
                    radius,
                    start_angle,
                    sweep_scale,
                    sweep_offset,
                } => {
                    if let Some(target_id) = id_map.get(target).copied()
                        && let Some(tracker_id) = id_map.get(tracker).copied()
                        && let Some(target_st) = builder.states.get(target_id)
                        && let Some(tracker_st) = builder.states.get(tracker_id)
                    {
                        let tracker_entity = tracker_st.entity;
                        let center = Point::new(center.0, center.1);
                        let radius = *radius;
                        let start_angle = *start_angle;
                        let sweep_scale = *sweep_scale;
                        let sweep_offset = *sweep_offset;
                        let redraw = gaanim_animation::AlwaysRedrawRegen::new(move |world| {
                            let value = world
                                .get::<gaanim_animation::FloatSignal>(tracker_entity)
                                .map(|signal| signal.value)
                                .unwrap_or(0.0);
                            gaanim_objects::primitives::curved_arrow_arc(
                                gaanim_core::ObjectId::from_raw(0),
                                center,
                                radius,
                                start_angle,
                                value * sweep_scale + sweep_offset,
                            )
                            .path
                            .0
                            .as_ref()
                            .clone()
                        });
                        builder.commands.entity(target_st.entity).insert(redraw);
                    }
                }
                Op::AttachPointOnCurve {
                    target,
                    curve,
                    tracker,
                } => {
                    if let Some(target_id) = id_map.get(target).copied()
                        && let Some(curve_id) = id_map.get(curve).copied()
                        && let Some(tracker_id) = id_map.get(tracker).copied()
                        && let Some(target_st) = builder.states.get(target_id)
                        && let Some(curve_st) = builder.states.get(curve_id)
                        && let Some(tracker_st) = builder.states.get(tracker_id)
                    {
                        builder
                            .commands
                            .entity(target_st.entity)
                            .insert(PointOnCurve::new(curve_st.entity, tracker_st.entity));
                    }
                }
                Op::AttachTangentOnCurve {
                    target,
                    curve,
                    tracker,
                } => {
                    if let Some(target_id) = id_map.get(target).copied()
                        && let Some(curve_id) = id_map.get(curve).copied()
                        && let Some(tracker_id) = id_map.get(tracker).copied()
                        && let Some(target_st) = builder.states.get(target_id)
                        && let Some(curve_st) = builder.states.get(curve_id)
                        && let Some(tracker_st) = builder.states.get(tracker_id)
                    {
                        builder
                            .commands
                            .entity(target_st.entity)
                            .insert(TangentOnCurve::new(curve_st.entity, tracker_st.entity));
                    }
                }
                Op::AttachNormalOnCurve {
                    target,
                    curve,
                    tracker,
                } => {
                    if let Some(target_id) = id_map.get(target).copied()
                        && let Some(curve_id) = id_map.get(curve).copied()
                        && let Some(tracker_id) = id_map.get(tracker).copied()
                        && let Some(target_st) = builder.states.get(target_id)
                        && let Some(curve_st) = builder.states.get(curve_id)
                        && let Some(tracker_st) = builder.states.get(tracker_id)
                    {
                        builder
                            .commands
                            .entity(target_st.entity)
                            .insert(NormalOnCurve::new(curve_st.entity, tracker_st.entity));
                    }
                }
                Op::AttachCurvatureOnCurve {
                    target,
                    curve,
                    tracker,
                    window,
                } => {
                    if let Some(target_id) = id_map.get(target).copied()
                        && let Some(curve_id) = id_map.get(curve).copied()
                        && let Some(tracker_id) = id_map.get(tracker).copied()
                        && let Some(target_st) = builder.states.get(target_id)
                        && let Some(curve_st) = builder.states.get(curve_id)
                        && let Some(tracker_st) = builder.states.get(tracker_id)
                    {
                        builder
                            .commands
                            .entity(target_st.entity)
                            .insert(CurvatureOnCurve::new(
                                curve_st.entity,
                                tracker_st.entity,
                                *window,
                            ));
                    }
                }
                Op::AttachCustomUpdater { target, updater } => {
                    if let Some(target_id) = id_map.get(target).copied()
                        && let Some(state) = builder.states.get(target_id)
                    {
                        builder
                            .commands
                            .entity(state.entity)
                            .insert(updater.clone().starting_at(builder.current_time));
                    }
                }
                Op::AttachTracedPath3D {
                    target,
                    source,
                    min_distance,
                    max_points,
                    colormap,
                    dissipating_time,
                } => {
                    if let Some(target_id) = id_map.get(target).copied()
                        && let Some(source_id) = id_map.get(source).copied()
                        && let Some(target_st) = builder.states.get(target_id)
                        && let Some(source_st) = builder.states.get(source_id)
                    {
                        let comp = gaanim_animation::TracedPath3D::new(
                            source_st.entity,
                            *min_distance,
                            *max_points,
                            colormap.clone(),
                        )
                        .starting_at(builder.current_time)
                        .with_dissipating_time(*dissipating_time);
                        builder.commands.entity(target_st.entity).insert(comp);
                    }
                }
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn apply_scene_object_scope(
        builder: &mut SceneBuilder,
        logical_target: ObjectId,
        action: SceneObjectScopeAction,
        scene_id: SceneId,
        previous_scene: Option<SceneId>,
        transition: Option<&gaanim_timeline::transition::TransitionType>,
        scene_start: f64,
        id_map: &HashMap<ObjectId, ObjectId>,
        object_scopes: &mut HashMap<ObjectId, CompiledObjectScope>,
    ) {
        let Some(&target) = id_map.get(&logical_target) else {
            return;
        };
        builder.manage_hierarchy_membership(target);
        let current_time = builder.current_time;
        let at_scene_start = (current_time - scene_start).abs() <= 1e-9;
        let transition_duration = transition.map(|value| value.duration()).unwrap_or(0.0);
        let current_scope = if builder.is_persistent(target) {
            CompiledObjectScope::Persistent
        } else {
            object_scopes
                .get(&logical_target)
                .copied()
                .unwrap_or(CompiledObjectScope::Segment(scene_id))
        };
        let visible_before = current_scope == CompiledObjectScope::Persistent
            || previous_scene
                .is_some_and(|previous| current_scope == CompiledObjectScope::Segment(previous));

        let next_scope = match action {
            SceneObjectScopeAction::Reuse if current_scope == CompiledObjectScope::Persistent => {
                CompiledObjectScope::Persistent
            }
            SceneObjectScopeAction::Reuse => {
                if at_scene_start && visible_before && transition_duration > 0.0 {
                    builder.schedule_scene_membership(target, None, scene_start);
                    builder.schedule_scene_membership(
                        target,
                        Some(scene_id),
                        scene_start + transition_duration,
                    );
                } else {
                    builder.schedule_scene_membership(target, Some(scene_id), current_time);
                }
                builder.set_hierarchy_persistent(target, false);
                CompiledObjectScope::Segment(scene_id)
            }
            SceneObjectScopeAction::Persist => {
                if current_scope != CompiledObjectScope::Persistent {
                    if at_scene_start && !visible_before && transition_duration > 0.0 {
                        builder.schedule_scene_membership(target, Some(scene_id), scene_start);
                        builder.schedule_scene_membership(
                            target,
                            None,
                            scene_start + transition_duration,
                        );
                    } else {
                        builder.schedule_scene_membership(target, None, current_time);
                    }
                    builder.set_hierarchy_persistent(target, true);
                }
                CompiledObjectScope::Persistent
            }
            SceneObjectScopeAction::Release => {
                if at_scene_start
                    && current_scope == CompiledObjectScope::Persistent
                    && transition_duration > 0.0
                {
                    builder.schedule_scene_membership(
                        target,
                        Some(scene_id),
                        scene_start + transition_duration,
                    );
                } else if current_scope != CompiledObjectScope::Segment(scene_id) {
                    builder.schedule_scene_membership(target, Some(scene_id), current_time);
                }
                builder.set_hierarchy_persistent(target, false);
                CompiledObjectScope::Segment(scene_id)
            }
        };

        let hierarchy: HashSet<ObjectId> = builder.hierarchy_ids(target).into_iter().collect();
        for (logical, actual) in id_map {
            if hierarchy.contains(actual) {
                object_scopes.insert(*logical, next_scope);
            }
        }
    }

    fn transform_targets(ops: &[Op]) -> std::collections::HashSet<ObjectId> {
        let mut targets = std::collections::HashSet::new();
        for op in ops {
            match op {
                Op::Animate { anim, active } if *active => match &anim.anim_type {
                    AnimationType::Transform { target }
                    | AnimationType::ReplacementTransform { target }
                    | AnimationType::FadeTransform { target } => {
                        targets.insert(*target);
                    }
                    _ => {}
                },
                Op::Play(anims) => {
                    for anim in anims {
                        match &anim.anim_type {
                            AnimationType::Transform { target }
                            | AnimationType::ReplacementTransform { target }
                            | AnimationType::FadeTransform { target } => {
                                targets.insert(*target);
                            }
                            _ => {}
                        }
                    }
                }
                Op::TransformMatching { target, .. } => {
                    targets.insert(*target);
                }
                _ => {}
            }
        }
        targets
    }

    fn reveal_deferred_on_play(
        builder: &mut SceneBuilder,
        deferred_visibility: &HashSet<ObjectId>,
        revealed_deferred: &mut HashSet<ObjectId>,
        anim: &AnimationBuilder,
        id_map: &HashMap<ObjectId, ObjectId>,
    ) {
        if !Self::animation_reveals_deferred(&anim.anim_type) {
            return;
        }

        let Some(&actual) = id_map.get(&anim.target) else {
            return;
        };

        let hierarchy: HashSet<ObjectId> = builder.hierarchy_ids(actual).into_iter().collect();
        let pending: Vec<ObjectId> = deferred_visibility
            .iter()
            .filter(|logical| {
                !revealed_deferred.contains(logical)
                    && id_map
                        .get(logical)
                        .is_some_and(|runtime| hierarchy.contains(runtime))
            })
            .copied()
            .collect();
        if pending.is_empty() {
            return;
        }

        // Moving an aggregate is not an entry animation for its deferred
        // descendants. A deferred root itself may still need to become visible,
        // but children such as forces and traces retain their own entry point.
        if matches!(
            &anim.anim_type,
            AnimationType::Properties(properties) if properties.is_transform_only()
        ) {
            if deferred_visibility.contains(&anim.target) && revealed_deferred.insert(anim.target) {
                builder.schedule_show_root_at(actual, builder.current_time + anim.delay.max(0.0));
            }
            return;
        }

        for logical in pending {
            revealed_deferred.insert(logical);
        }

        // FadeIn/FadeInFrom already author their own root opacity lens, but
        // composite deferred objects still need their descendants restored.
        // Other animations need an instantaneous reveal so Create/Write/
        // movement animations can be used directly in scene.play as the entry
        // point.
        if matches!(
            &anim.anim_type,
            AnimationType::FadeIn | AnimationType::FadeInFrom { .. }
        ) {
            builder
                .schedule_show_descendants_at(actual, builder.current_time + anim.delay.max(0.0));
            return;
        }

        builder.schedule_show_at(actual, builder.current_time + anim.delay.max(0.0));
    }

    fn animation_reveals_deferred(anim_type: &AnimationType) -> bool {
        match anim_type {
            AnimationType::FadeOut
            | AnimationType::Unwrite { .. }
            | AnimationType::Uncreate { .. }
            | AnimationType::ShrinkToCenter => false,
            AnimationType::FadeTo { to } => *to > 0.0,
            _ => true,
        }
    }

    fn add_camera_lens(
        builder: &mut SceneBuilder,
        start: f64,
        anim: &AnimationBuilder,
        lens: gaanim_timeline::clip::PropertyLensSpec,
    ) {
        builder.timeline.add_clip(
            builder.default_track,
            start + anim.delay.max(0.0),
            anim.duration.max(0.0),
            gaanim_timeline::clip::ClipPayload::Animation(gaanim_timeline::clip::AnimationSpec {
                target: gaanim_core::ObjectId::from_parts(0, 1),
                lens,
                rate_func: anim.rate_func.clone(),
                delay: 0.0,
                label: Some("Camera".to_string()),
            }),
        );
    }

    #[allow(clippy::too_many_arguments)]
    fn schedule_camera_animation(
        builder: &mut SceneBuilder,
        frame_bounds: Bounds3D,
        id_map: &HashMap<ObjectId, ObjectId>,
        camera_position: &mut DVec3,
        camera_zoom: &mut f64,
        camera_rotation: &mut gaanim_core::glam::DQuat,
        camera_target: &mut DVec3,
        camera_up: &mut DVec3,
        camera_fov: &mut Option<(f64, f64, f64)>,
        anim: &AnimationBuilder,
        start: f64,
    ) {
        use gaanim_timeline::clip::PropertyLensSpec;

        match &anim.anim_type {
            AnimationType::CameraPosition { to } => {
                Self::add_camera_lens(
                    builder,
                    start,
                    anim,
                    PropertyLensSpec::CameraPosition {
                        from: *camera_position,
                        to: *to,
                    },
                );
                *camera_position = *to;
            }
            AnimationType::CameraPositionSource { target } => {
                Self::add_camera_lens(
                    builder,
                    start,
                    anim,
                    PropertyLensSpec::CameraPositionSource {
                        from: *camera_position,
                        to: compile_tracking_endpoint(target, id_map, &builder.states),
                    },
                );
            }
            AnimationType::CameraZoom { to } => {
                Self::add_camera_lens(
                    builder,
                    start,
                    anim,
                    PropertyLensSpec::CameraZoom {
                        from: *camera_zoom,
                        to: *to,
                    },
                );
                *camera_zoom = *to;
            }
            AnimationType::CameraZoomSource { to } => {
                Self::add_camera_lens(
                    builder,
                    start,
                    anim,
                    PropertyLensSpec::CameraZoomSource {
                        from: *camera_zoom,
                        to: compile_tracking_scalar(to, id_map, &builder.states),
                    },
                );
            }
            AnimationType::CameraRotation { to } => {
                Self::add_camera_lens(
                    builder,
                    start,
                    anim,
                    PropertyLensSpec::CameraRotation {
                        from: *camera_rotation,
                        to: *to,
                    },
                );
                *camera_rotation = *to;
            }
            AnimationType::CameraRotationSource { to } => {
                Self::add_camera_lens(
                    builder,
                    start,
                    anim,
                    PropertyLensSpec::CameraRotationSource {
                        from: 2.0 * camera_rotation.z.atan2(camera_rotation.w),
                        to: compile_tracking_scalar(to, id_map, &builder.states),
                    },
                );
            }
            AnimationType::CameraFrame { target, margin } => {
                let Some(state) = builder.states.get(*target) else {
                    return;
                };
                let bounds = state
                    .bounds
                    .transform_2d(&builder.get_world_transform(*target).to_affine_2d());
                let width = (bounds.width() + margin * 2.0).max(1.0);
                let height = (bounds.height() + margin * 2.0).max(1.0);
                let zoom = (frame_bounds.width() / width)
                    .min(frame_bounds.height() / height)
                    .max(0.01);
                let center = bounds.center();
                Self::add_camera_lens(
                    builder,
                    start,
                    anim,
                    PropertyLensSpec::CameraPosition {
                        from: *camera_position,
                        to: center,
                    },
                );
                Self::add_camera_lens(
                    builder,
                    start,
                    anim,
                    PropertyLensSpec::CameraZoom {
                        from: *camera_zoom,
                        to: zoom,
                    },
                );
                *camera_position = center;
                *camera_zoom = zoom;
            }
            AnimationType::CameraFrameMany {
                targets,
                margins,
                dynamic,
            } => {
                let target_states: Vec<_> = targets
                    .iter()
                    .filter_map(|target| builder.states.get(*target))
                    .collect();
                if target_states.is_empty() {
                    return;
                }
                if *dynamic {
                    Self::add_camera_lens(
                        builder,
                        start,
                        anim,
                        PropertyLensSpec::CameraFrameDynamic {
                            targets: target_states.iter().map(|state| state.entity).collect(),
                            from_position: *camera_position,
                            from_zoom: *camera_zoom,
                            margins: *margins,
                            frame_width: frame_bounds.width(),
                            frame_height: frame_bounds.height(),
                        },
                    );
                    // Keep subsequent authored camera animations continuous.
                    // Runtime evaluation still recomputes these bounds every
                    // frame; this authored estimate is only the hand-off pose.
                    if let Some(bounds) = targets
                        .iter()
                        .filter_map(|target| {
                            builder.states.get(*target).map(|state| {
                                state.bounds.transform_2d(
                                    &builder.get_world_transform(*target).to_affine_2d(),
                                )
                            })
                        })
                        .reduce(|left, right| left.union(&right))
                    {
                        let [top, right, bottom, left] = *margins;
                        let framed = Bounds3D::new_2d(
                            bounds.min.x - left,
                            bounds.min.y - bottom,
                            bounds.max.x + right,
                            bounds.max.y + top,
                        );
                        *camera_position = framed.center();
                        *camera_zoom = (frame_bounds.width() / framed.width().max(1.0))
                            .min(frame_bounds.height() / framed.height().max(1.0));
                    }
                } else {
                    let bounds = targets
                        .iter()
                        .filter_map(|target| {
                            builder.states.get(*target).map(|state| {
                                state.bounds.transform_2d(
                                    &builder.get_world_transform(*target).to_affine_2d(),
                                )
                            })
                        })
                        .reduce(|left, right| left.union(&right))
                        .expect("non-empty camera frame targets");
                    let [top, right, bottom, left] = *margins;
                    let framed = Bounds3D::new_2d(
                        bounds.min.x - left,
                        bounds.min.y - bottom,
                        bounds.max.x + right,
                        bounds.max.y + top,
                    );
                    let zoom = (frame_bounds.width() / framed.width().max(1.0))
                        .min(frame_bounds.height() / framed.height().max(1.0));
                    let center = framed.center();
                    Self::add_camera_lens(
                        builder,
                        start,
                        anim,
                        PropertyLensSpec::CameraPosition {
                            from: *camera_position,
                            to: center,
                        },
                    );
                    Self::add_camera_lens(
                        builder,
                        start,
                        anim,
                        PropertyLensSpec::CameraZoom {
                            from: *camera_zoom,
                            to: zoom,
                        },
                    );
                    *camera_position = center;
                    *camera_zoom = zoom;
                }
            }
            AnimationType::CameraFollow { target } => {
                Self::add_camera_lens(
                    builder,
                    start,
                    anim,
                    PropertyLensSpec::CameraFollow { target: *target },
                );
                if let Some(state) = builder.states.get(*target) {
                    camera_position.x = state.transform.translation.x;
                    camera_position.y = state.transform.translation.y;
                }
            }
            AnimationType::CameraFollowEndpoint {
                target,
                offset,
                offset_space,
                lag,
            } => {
                Self::add_camera_lens(
                    builder,
                    start,
                    anim,
                    PropertyLensSpec::CameraFollowEndpoint {
                        target: compile_tracking_endpoint(target, id_map, &builder.states),
                        from: *camera_position,
                        offset: *offset,
                        offset_space: *offset_space,
                        lag: *lag,
                    },
                );
            }
            AnimationType::CameraShake {
                amplitude,
                frequency,
            } => Self::add_camera_lens(
                builder,
                start,
                anim,
                PropertyLensSpec::CameraShake {
                    origin: *camera_position,
                    amplitude: *amplitude,
                    frequency: *frequency,
                },
            ),
            AnimationType::CameraLookAt { eye, target, up } => {
                let to_rotation = gaanim_core::glam::DMat4::look_at_rh(*eye, *target, *up)
                    .inverse()
                    .to_scale_rotation_translation()
                    .1;
                Self::add_camera_lens(
                    builder,
                    start,
                    anim,
                    PropertyLensSpec::CameraPosition {
                        from: *camera_position,
                        to: *eye,
                    },
                );
                Self::add_camera_lens(
                    builder,
                    start,
                    anim,
                    PropertyLensSpec::CameraTarget {
                        from: *camera_target,
                        to: *target,
                    },
                );
                Self::add_camera_lens(
                    builder,
                    start,
                    anim,
                    PropertyLensSpec::CameraRotation {
                        from: *camera_rotation,
                        to: to_rotation,
                    },
                );
                *camera_position = *eye;
                *camera_target = *target;
                *camera_up = *up;
                *camera_rotation = to_rotation;
            }
            AnimationType::CameraLookAtSource { eye, target, up } => {
                Self::add_camera_lens(
                    builder,
                    start,
                    anim,
                    PropertyLensSpec::CameraLookAtSource {
                        from_position: *camera_position,
                        from_target: *camera_target,
                        from_rotation: *camera_rotation,
                        eye: compile_tracking_endpoint(eye, id_map, &builder.states),
                        target: compile_tracking_endpoint(target, id_map, &builder.states),
                        up: *up,
                    },
                );
                *camera_up = *up;
            }
            AnimationType::CameraOrbit {
                delta_yaw,
                delta_pitch,
            } => {
                let mut camera = gaanim_math::Camera::ortho_2d(1, 1);
                camera.position = *camera_position;
                camera.target = *camera_target;
                camera.rotation = *camera_rotation;
                camera.up = DVec3::Y;
                camera.projection = if let Some((fov_y, near, far)) = *camera_fov {
                    gaanim_math::Projection::Perspective { fov_y, near, far }
                } else {
                    gaanim_math::Projection::Orthographic { zoom: *camera_zoom }
                };
                camera
                    .orbit_around_target(*delta_yaw, *delta_pitch)
                    .expect("camera orbit requires a finite, non-degenerate pose");
                Self::add_camera_lens(
                    builder,
                    start,
                    anim,
                    PropertyLensSpec::CameraPosition {
                        from: *camera_position,
                        to: camera.position,
                    },
                );
                Self::add_camera_lens(
                    builder,
                    start,
                    anim,
                    PropertyLensSpec::CameraRotation {
                        from: *camera_rotation,
                        to: camera.rotation,
                    },
                );
                *camera_position = camera.position;
                *camera_rotation = camera.rotation;
            }
            AnimationType::CameraPerspective { fov_y, near, far } => {
                let (from_fov, from_near, from_far) =
                    camera_fov.unwrap_or((std::f64::consts::FRAC_PI_4, 0.1, 1000.0));
                Self::add_camera_lens(
                    builder,
                    start,
                    anim,
                    PropertyLensSpec::CameraPerspective {
                        from_fov,
                        to_fov: *fov_y,
                        from_near,
                        to_near: *near,
                        from_far,
                        to_far: *far,
                    },
                );
                *camera_fov = Some((*fov_y, *near, *far));
            }
            AnimationType::CameraOrthographic { zoom } => {
                Self::add_camera_lens(
                    builder,
                    start,
                    anim,
                    PropertyLensSpec::CameraOrthographic {
                        from: *camera_zoom,
                        to: *zoom,
                    },
                );
                *camera_zoom = *zoom;
                *camera_fov = None;
            }
            AnimationType::CameraReset => {
                Self::add_camera_lens(
                    builder,
                    start,
                    anim,
                    PropertyLensSpec::CameraReset {
                        from_position: *camera_position,
                        from_rotation: *camera_rotation,
                        from_target: *camera_target,
                        from_up: *camera_up,
                        from_zoom: *camera_zoom,
                        to_zoom: 1.0,
                    },
                );
                *camera_position = DVec3::ZERO;
                *camera_rotation = gaanim_core::glam::DQuat::IDENTITY;
                *camera_target = DVec3::ZERO;
                *camera_up = DVec3::Y;
                *camera_zoom = 1.0;
                *camera_fov = None;
            }
            AnimationType::CameraDolly { factor } => {
                let direction = *camera_position - *camera_target;
                let destination = *camera_target + direction * factor;
                Self::add_camera_lens(
                    builder,
                    start,
                    anim,
                    PropertyLensSpec::CameraPosition {
                        from: *camera_position,
                        to: destination,
                    },
                );
                *camera_position = destination;
            }
            _ => {}
        }
    }

    fn remap_anim(
        anim: &AnimationBuilder,
        id_map: &HashMap<ObjectId, ObjectId>,
    ) -> Option<AnimationBuilder> {
        let target = if anim.anim_type.is_camera() {
            anim.target
        } else {
            *id_map.get(&anim.target)?
        };
        let anim_type = match &anim.anim_type {
            AnimationType::CameraFrame { target, margin } => AnimationType::CameraFrame {
                target: *id_map.get(target)?,
                margin: *margin,
            },
            AnimationType::CameraFrameMany {
                targets,
                margins,
                dynamic,
            } => AnimationType::CameraFrameMany {
                targets: targets
                    .iter()
                    .filter_map(|target| id_map.get(target).copied())
                    .collect(),
                margins: *margins,
                dynamic: *dynamic,
            },
            AnimationType::CameraFollow { target } => AnimationType::CameraFollow {
                target: *id_map.get(target)?,
            },
            AnimationType::FadeTransform { target } => AnimationType::FadeTransform {
                target: *id_map.get(target)?,
            },
            AnimationType::Transform { target } => AnimationType::Transform {
                target: *id_map.get(target)?,
            },
            AnimationType::ReplacementTransform { target } => AnimationType::ReplacementTransform {
                target: *id_map.get(target)?,
            },
            AnimationType::TextTransition {
                target,
                copy,
                semantic_pairs,
            } => AnimationType::TextTransition {
                target: *id_map.get(target)?,
                copy: *copy,
                semantic_pairs: semantic_pairs.clone(),
            },
            AnimationType::TextSelectionTransform {
                target,
                source_fragment,
                source_occurrence,
                target_fragment,
                target_occurrence,
                copy,
            } => AnimationType::TextSelectionTransform {
                target: *id_map.get(target)?,
                source_fragment: source_fragment.clone(),
                source_occurrence: *source_occurrence,
                target_fragment: target_fragment.clone(),
                target_occurrence: *target_occurrence,
                copy: *copy,
            },
            AnimationType::MoveAlongPath { path, path_target } => AnimationType::MoveAlongPath {
                path: path.clone(),
                path_target: path_target.map(|id| *id_map.get(&id).unwrap_or(&id)),
            },
            other => other.clone(),
        };
        Some(AnimationBuilder {
            target,
            anim_type,
            duration: anim.duration,
            delay: anim.delay,
            rate_func: anim.rate_func.clone(),
        })
    }

    fn fragment_child_ids(
        builder: &mut SceneBuilder,
        id_map: &HashMap<ObjectId, ObjectId>,
        target: ObjectId,
        fragment: &str,
        occurrence: Option<usize>,
    ) -> Vec<ObjectId> {
        let Some(&target) = id_map.get(&target) else {
            return Vec::new();
        };
        builder
            .select_occurrence(MobjectRef { id: target }, fragment, occurrence)
            .child_ids
    }

    fn fade_cancellation_marks(
        builder: &mut SceneBuilder,
        cancellation_marks: &mut HashMap<ObjectId, Vec<ObjectId>>,
        source: ObjectId,
        transition_duration: f64,
    ) {
        let Some(marks) = cancellation_marks.remove(&source) else {
            return;
        };
        let duration = (transition_duration * 0.25).clamp(0.12, 0.3);
        for target in marks {
            builder.play_at_current_time(AnimationBuilder {
                target,
                anim_type: AnimationType::FadeOut,
                duration,
                rate_func: RateFunc::Smooth,
                delay: 0.0,
            });
        }
    }

    fn fade_canceled_term_children(
        builder: &mut SceneBuilder,
        canceled_term_children: &mut HashMap<ObjectId, Vec<ObjectId>>,
        source: ObjectId,
        transition_duration: f64,
    ) {
        let Some(children) = canceled_term_children.remove(&source) else {
            return;
        };
        let duration = (transition_duration * 0.25).clamp(0.12, 0.3);
        for target in children {
            builder.play_at_current_time(AnimationBuilder {
                target,
                anim_type: AnimationType::FadeOut,
                duration,
                rate_func: RateFunc::Smooth,
                delay: 0.0,
            });
        }
    }

    fn spawn_one(
        builder: &mut SceneBuilder,
        spec: &ObjectSpec,
        id_map: &HashMap<ObjectId, ObjectId>,
        frame_bounds: Bounds3D,
        text_config: &gaanim_text::prelude::TextConfig,
        scene_background: gaanim_core::peniko::Color,
    ) -> MobjectRef {
        match &spec.kind {
            SpawnKind::Circle(r) => {
                let b = builder.circle(*r);
                let mr = Self::finish_spawn_builder(b, spec);
                Self::apply_layout(builder, mr.id, spec, id_map, frame_bounds);
                mr
            }
            SpawnKind::Rect(w, h) => {
                let b = builder.rectangle(*w, *h);
                let mr = Self::finish_spawn_builder(b, spec);
                Self::apply_layout(builder, mr.id, spec, id_map, frame_bounds);
                mr
            }
            SpawnKind::RoundedRect(w, h, r) => {
                let b = builder.rounded_rect(*w, *h, *r);
                let mr = Self::finish_spawn_builder(b, spec);
                Self::apply_layout(builder, mr.id, spec, id_map, frame_bounds);
                mr
            }
            SpawnKind::Square(sz) => {
                let b = builder.square(*sz);
                let mr = Self::finish_spawn_builder(b, spec);
                Self::apply_layout(builder, mr.id, spec, id_map, frame_bounds);
                mr
            }
            SpawnKind::Dot(r) => {
                let b = builder.dot(*r);
                let mr = Self::finish_spawn_builder(b, spec);
                Self::apply_layout(builder, mr.id, spec, id_map, frame_bounds);
                mr
            }
            SpawnKind::Ellipse(rx, ry) => {
                let b = builder.ellipse(*rx, *ry);
                let mr = Self::finish_spawn_builder(b, spec);
                Self::apply_layout(builder, mr.id, spec, id_map, frame_bounds);
                mr
            }
            SpawnKind::Line(x1, y1, x2, y2) => {
                let b = builder.line(Point::new(*x1, *y1), Point::new(*x2, *y2));
                let mr = Self::finish_spawn_builder(b, spec);
                Self::apply_layout(builder, mr.id, spec, id_map, frame_bounds);
                mr
            }
            SpawnKind::Arrow(x1, y1, x2, y2) => {
                let b = builder.arrow(Point::new(*x1, *y1), Point::new(*x2, *y2));
                let mr = Self::finish_spawn_builder(b, spec);
                Self::apply_layout(builder, mr.id, spec, id_map, frame_bounds);
                mr
            }
            SpawnKind::DashedLine {
                start,
                end,
                dash_length,
                gap_length,
            } => {
                let b = builder.dashed_line(
                    Point::new(start.0, start.1),
                    Point::new(end.0, end.1),
                    *dash_length,
                    *gap_length,
                );
                let mr = Self::finish_spawn_builder(b, spec);
                Self::apply_layout(builder, mr.id, spec, id_map, frame_bounds);
                mr
            }
            SpawnKind::DoubleArrow {
                start,
                end,
                head_length,
                head_width,
            } => {
                let b = builder.double_arrow(
                    Point::new(start.0, start.1),
                    Point::new(end.0, end.1),
                    *head_length,
                    *head_width,
                );
                let mr = Self::finish_spawn_builder(b, spec);
                Self::apply_layout(builder, mr.id, spec, id_map, frame_bounds);
                mr
            }
            SpawnKind::Polygon(points) => {
                let points: Vec<Point> = points.iter().map(|&(x, y)| Point::new(x, y)).collect();
                let b = builder.polygon(&points);
                let mr = Self::finish_spawn_builder(b, spec);
                Self::apply_layout(builder, mr.id, spec, id_map, frame_bounds);
                mr
            }
            SpawnKind::Star {
                points,
                outer_radius,
                inner_radius,
            } => {
                let b = builder.star(*points, *outer_radius, *inner_radius);
                let mr = Self::finish_spawn_builder(b, spec);
                Self::apply_layout(builder, mr.id, spec, id_map, frame_bounds);
                mr
            }
            SpawnKind::RegularPolygon { sides, radius } => {
                let b = builder.regular_polygon(*sides, *radius);
                let mr = Self::finish_spawn_builder(b, spec);
                Self::apply_layout(builder, mr.id, spec, id_map, frame_bounds);
                mr
            }
            SpawnKind::Sector {
                center,
                radius,
                start_angle,
                sweep_angle,
            } => {
                let b = builder.sector(
                    Point::new(center.0, center.1),
                    *radius,
                    *start_angle,
                    *sweep_angle,
                );
                let mr = Self::finish_spawn_builder(b, spec);
                Self::apply_layout(builder, mr.id, spec, id_map, frame_bounds);
                mr
            }
            SpawnKind::Annulus {
                outer_radius,
                inner_radius,
            } => {
                let b = builder.annulus(*outer_radius, *inner_radius);
                let mr = Self::finish_spawn_builder(b, spec);
                Self::apply_layout(builder, mr.id, spec, id_map, frame_bounds);
                mr
            }
            SpawnKind::Brace { start, end, height } => {
                let b = builder.brace(
                    Point::new(start.0, start.1),
                    Point::new(end.0, end.1),
                    *height,
                );
                let mr = Self::finish_spawn_builder(b, spec);
                Self::apply_layout(builder, mr.id, spec, id_map, frame_bounds);
                mr
            }
            SpawnKind::Checkmark(size) => {
                let b = builder.checkmark(*size);
                let mr = Self::finish_spawn_builder(b, spec);
                Self::apply_layout(builder, mr.id, spec, id_map, frame_bounds);
                mr
            }
            SpawnKind::Cross(size) => {
                let b = builder.cross(*size);
                let mr = Self::finish_spawn_builder(b, spec);
                Self::apply_layout(builder, mr.id, spec, id_map, frame_bounds);
                mr
            }
            SpawnKind::RightAngle(arm_length) => {
                let b = builder.right_angle(*arm_length);
                let mr = Self::finish_spawn_builder(b, spec);
                Self::apply_layout(builder, mr.id, spec, id_map, frame_bounds);
                mr
            }
            SpawnKind::Arc {
                center,
                radius,
                start_angle,
                sweep_angle,
            } => {
                let b = builder.arc(
                    Point::new(center.0, center.1),
                    Vec2::new(*radius, *radius),
                    *start_angle,
                    *sweep_angle,
                    0.0,
                );
                let mr = Self::finish_spawn_builder(b, spec);
                Self::apply_layout(builder, mr.id, spec, id_map, frame_bounds);
                mr
            }
            SpawnKind::CurvedArrow(x1, y1, x2, y2, angle) => {
                let b = builder.curved_arrow(Point::new(*x1, *y1), Point::new(*x2, *y2), *angle);
                let mr = Self::finish_spawn_builder(b, spec);
                Self::apply_layout(builder, mr.id, spec, id_map, frame_bounds);
                mr
            }
            SpawnKind::CurvedArrowArc {
                center,
                radius,
                start_angle,
                sweep_angle,
            } => {
                let b = builder.curved_arrow_arc(
                    Point::new(center.0, center.1),
                    *radius,
                    *start_angle,
                    *sweep_angle,
                );
                let mr = Self::finish_spawn_builder(b, spec);
                Self::apply_layout(builder, mr.id, spec, id_map, frame_bounds);
                mr
            }
            SpawnKind::Dimension { start, end, offset } => {
                let dx = end.0 - start.0;
                let dy = end.1 - start.1;
                let length = dx.hypot(dy);
                if length <= f64::EPSILON {
                    let b = builder.line(Point::new(start.0, start.1), Point::new(end.0, end.1));
                    let mr = Self::finish_spawn_builder(b, spec);
                    Self::apply_layout(builder, mr.id, spec, id_map, frame_bounds);
                    mr
                } else {
                    let normal = (-dy / length, dx / length);
                    let dimension_start =
                        Point::new(start.0 + normal.0 * *offset, start.1 + normal.1 * *offset);
                    let dimension_end =
                        Point::new(end.0 + normal.0 * *offset, end.1 + normal.1 * *offset);
                    let color = PenikoColor::from_rgb8(0x80, 0x80, 0x80);
                    let extension_a = builder
                        .line(Point::new(start.0, start.1), dimension_start)
                        .no_fill()
                        .stroke(color, 2.0)
                        .spawn();
                    let extension_b = builder
                        .line(Point::new(end.0, end.1), dimension_end)
                        .no_fill()
                        .stroke(color, 2.0)
                        .spawn();
                    let measurement = builder
                        .double_arrow(dimension_start, dimension_end, Some(12.0), Some(10.0))
                        .fill(color)
                        .no_stroke()
                        .spawn();
                    let group = builder.group(&[extension_a, extension_b, measurement]);
                    Self::post_apply(builder, group.id, spec, id_map, frame_bounds);
                    group
                }
            }
            SpawnKind::Polyline(points) => {
                let points: Vec<Point> = points.iter().map(|&(x, y)| Point::new(x, y)).collect();
                let b = builder.open_path(&points);
                let mr = Self::finish_spawn_builder(b, spec);
                Self::apply_layout(builder, mr.id, spec, id_map, frame_bounds);
                mr
            }
            SpawnKind::Bezier {
                start,
                controls,
                end,
            } => {
                let mut path = gaanim_core::kurbo::BezPath::new();
                path.move_to(Point::new(start.0, start.1));
                match controls.as_slice() {
                    [control] => {
                        path.quad_to(Point::new(control.0, control.1), Point::new(end.0, end.1))
                    }
                    [control1, control2] => path.curve_to(
                        Point::new(control1.0, control1.1),
                        Point::new(control2.0, control2.1),
                        Point::new(end.0, end.1),
                    ),
                    _ => path.line_to(Point::new(end.0, end.1)),
                }
                let rect = path.bounding_box();
                let svg_path = gaanim_objects::prelude::SvgPath {
                    id: "Bezier".to_string(),
                    path,
                    bounds: Bounds3D::new_2d(rect.x0, rect.y0, rect.x1, rect.y1),
                    fill: None,
                    stroke: StrokeBrush::transparent(),
                };
                let b = builder.svg_path(&svg_path);
                let mr = Self::finish_spawn_builder(b, spec);
                Self::apply_layout(builder, mr.id, spec, id_map, frame_bounds);
                mr
            }
            SpawnKind::Curve(elements) => {
                let mut path = gaanim_core::kurbo::BezPath::new();
                let mut cursor = Point::ORIGIN;
                let mut subpath_start = Point::ORIGIN;
                let mut last_quad = None;
                let mut last_cubic = None;
                let resolve = |point: (f64, f64), relative: bool, cursor: Point| {
                    if relative {
                        Point::new(cursor.x + point.0, cursor.y + point.1)
                    } else {
                        Point::new(point.0, point.1)
                    }
                };
                for element in elements {
                    match element {
                        crate::canvas::CurveElement::Move { to, relative } => {
                            cursor = resolve(*to, *relative, cursor);
                            subpath_start = cursor;
                            path.move_to(cursor);
                            last_quad = None;
                            last_cubic = None;
                        }
                        crate::canvas::CurveElement::Line { to, relative } => {
                            cursor = resolve(*to, *relative, cursor);
                            path.line_to(cursor);
                            last_quad = None;
                            last_cubic = None;
                        }
                        crate::canvas::CurveElement::Quad {
                            control,
                            to,
                            relative,
                        } => {
                            let end = resolve(*to, *relative, cursor);
                            let control = match control {
                                crate::canvas::CurveControl::None => end,
                                crate::canvas::CurveControl::Auto => last_quad
                                    .map(|p: Point| {
                                        Point::new(2.0 * cursor.x - p.x, 2.0 * cursor.y - p.y)
                                    })
                                    .unwrap_or(cursor),
                                crate::canvas::CurveControl::Point(point) => {
                                    resolve(*point, *relative, cursor)
                                }
                            };
                            path.quad_to(control, end);
                            cursor = end;
                            last_quad = Some(control);
                            last_cubic = None;
                        }
                        crate::canvas::CurveElement::Cubic {
                            control_start,
                            control_end,
                            to,
                            relative,
                        } => {
                            let end = resolve(*to, *relative, cursor);
                            let start = match control_start {
                                crate::canvas::CurveControl::None => cursor,
                                crate::canvas::CurveControl::Auto => last_cubic
                                    .map(|p: Point| {
                                        Point::new(2.0 * cursor.x - p.x, 2.0 * cursor.y - p.y)
                                    })
                                    .unwrap_or(cursor),
                                crate::canvas::CurveControl::Point(point) => {
                                    resolve(*point, *relative, cursor)
                                }
                            };
                            let finish = match control_end {
                                crate::canvas::CurveControl::None => end,
                                crate::canvas::CurveControl::Auto => end,
                                crate::canvas::CurveControl::Point(point) => {
                                    resolve(*point, *relative, cursor)
                                }
                            };
                            path.curve_to(start, finish, end);
                            cursor = end;
                            last_cubic = Some(finish);
                            last_quad = None;
                        }
                        crate::canvas::CurveElement::Close { smooth } => {
                            if *smooth {
                                let control = last_cubic
                                    .or(last_quad)
                                    .map(|p: Point| {
                                        Point::new(2.0 * cursor.x - p.x, 2.0 * cursor.y - p.y)
                                    })
                                    .unwrap_or(cursor);
                                path.curve_to(control, subpath_start, subpath_start);
                            } else {
                                path.close_path();
                            }
                            cursor = subpath_start;
                            last_quad = None;
                            last_cubic = None;
                        }
                    }
                }
                let rect = path.bounding_box();
                let svg_path = gaanim_objects::prelude::SvgPath {
                    id: "Curve".to_string(),
                    path,
                    bounds: Bounds3D::new_2d(rect.x0, rect.y0, rect.x1, rect.y1),
                    fill: None,
                    stroke: StrokeBrush::transparent(),
                };
                let b = builder.svg_path(&svg_path);
                let mr = Self::finish_spawn_builder(b, spec);
                Self::apply_layout(builder, mr.id, spec, id_map, frame_bounds);
                mr
            }
            SpawnKind::ExpressionPlot {
                map,
                expression,
                variable,
                domain,
                reveal,
                sampling,
            } => {
                let mut parameter_ids = expression.parameter_ids();
                if let Some(reveal) = reveal {
                    parameter_ids.extend(reveal.parameter_ids());
                    parameter_ids.sort_unstable();
                    parameter_ids.dedup();
                }
                let parameter_entities: Vec<(gaanim_core::ObjectId, bevy::prelude::Entity)> =
                    parameter_ids
                        .into_iter()
                        .filter_map(|logical| {
                            let actual = id_map.get(&logical).copied()?;
                            let entity = builder.states.get(actual)?.entity;
                            Some((logical, entity))
                        })
                        .collect();
                let mut context = gaanim_expr::EvalContext::new();
                for (logical, _) in &parameter_entities {
                    if let Some(actual) = id_map.get(logical).copied()
                        && let Some(value) = builder.float_signals.get(&actual).copied()
                    {
                        context.set_parameter(*logical, value);
                    }
                }
                let path = sampled_expression_path(
                    map,
                    expression,
                    variable,
                    *domain,
                    reveal.as_ref(),
                    *sampling,
                    &context,
                );
                let svg_path = gaanim_objects::prelude::SvgPath {
                    id: "ExpressionPlot".to_owned(),
                    path,
                    bounds: map.frame.bounds(),
                    fill: None,
                    stroke: StrokeBrush::transparent(),
                };
                let b = builder.svg_path(&svg_path);
                let mr = Self::finish_spawn_builder(b, spec);
                Self::apply_layout(builder, mr.id, spec, id_map, frame_bounds);
                if !parameter_entities.is_empty()
                    && let Some(state) = builder.states.get(mr.id)
                {
                    let map = map.clone();
                    let expression = expression.clone();
                    let variable = variable.clone();
                    let domain = *domain;
                    let reveal = reveal.clone();
                    let sampling = *sampling;
                    let redraw = gaanim_animation::AlwaysRedrawRegen::new(move |world| {
                        let mut context = gaanim_expr::EvalContext::new();
                        for (logical, entity) in &parameter_entities {
                            if let Some(signal) =
                                world.get::<gaanim_animation::FloatSignal>(*entity)
                            {
                                context.set_parameter(*logical, signal.value);
                            }
                        }
                        sampled_expression_path(
                            &map,
                            &expression,
                            &variable,
                            domain,
                            reveal.as_ref(),
                            sampling,
                            &context,
                        )
                    });
                    builder.commands.entity(state.entity).insert(redraw);
                }
                mr
            }
            SpawnKind::ExpressionReadout {
                expression,
                format,
                prefix,
                suffix,
                invalid,
                font_size,
            } => {
                let parameter_entities: Vec<(gaanim_core::ObjectId, bevy::prelude::Entity)> =
                    expression
                        .parameter_ids()
                        .into_iter()
                        .filter_map(|logical| {
                            let actual = id_map.get(&logical).copied()?;
                            Some((logical, builder.states.get(actual)?.entity))
                        })
                        .collect();
                let mut context = gaanim_expr::EvalContext::new();
                for (logical, _) in &parameter_entities {
                    if let Some(actual) = id_map.get(logical).copied()
                        && let Some(value) = builder.float_signals.get(&actual).copied()
                    {
                        context.set_parameter(*logical, value);
                    }
                }
                let body = &text_config.roles[&gaanim_text::prelude::TextRole::Body];
                let size = font_size.unwrap_or(body.size);
                let text = format!(
                    "{}{}{}",
                    prefix,
                    gaanim_animation::format_reactive_number(
                        expression.eval(&context).unwrap_or(f64::NAN),
                        format,
                        invalid
                    ),
                    suffix,
                );
                let (path, bounds) = gaanim_text::shaper::compile_text_to_path(
                    builder.font_registry,
                    &text,
                    &body.font_family,
                    size,
                )
                .unwrap_or_else(|_| (gaanim_core::kurbo::BezPath::new(), Bounds3D::default()));
                let baseline = gaanim_animation::right_aligned_readout_baseline(bounds);
                let (path, bounds) = gaanim_animation::right_align_readout_path(path, bounds);
                let svg_path = gaanim_objects::prelude::SvgPath {
                    id: "ReactiveReadout".to_owned(),
                    path,
                    bounds,
                    fill: None,
                    stroke: StrokeBrush::transparent(),
                };
                let source_path = std::sync::Arc::new(svg_path.path.clone());
                let b = builder.svg_path(&svg_path);
                let mr = Self::finish_spawn_builder(b, spec);
                Self::apply_layout(builder, mr.id, spec, id_map, frame_bounds);
                if let Some(state) = builder.states.get(mr.id) {
                    builder.commands.entity(state.entity).insert((
                        gaanim_scene::PathSource(source_path.clone()),
                        gaanim_scene::TextBaseline(baseline),
                        gaanim_animation::ReactiveReadout {
                            expression: expression.clone(),
                            parameters: parameter_entities,
                            format: format.clone(),
                            prefix: prefix.clone(),
                            suffix: suffix.clone(),
                            invalid: invalid.clone(),
                            font_family: body.font_family.clone(),
                            font_size: size,
                            last_text: text,
                            last_path: source_path,
                            last_bounds: bounds,
                        },
                    ));
                }
                mr
            }
            SpawnKind::DataMark { map, source, kind } => {
                let path = gaanim_visualization::data_mark_path(map, &source.snapshot(), kind)
                    .unwrap_or_default();
                let svg_path = gaanim_objects::prelude::SvgPath {
                    id: "DataMark".to_owned(),
                    path: path.clone(),
                    bounds: map.frame.bounds(),
                    fill: None,
                    stroke: StrokeBrush::transparent(),
                };
                let b = builder.svg_path(&svg_path);
                let mr = Self::finish_spawn_builder(b, spec);
                Self::apply_layout(builder, mr.id, spec, id_map, frame_bounds);
                if let Some(state) = builder.states.get(mr.id) {
                    let map = map.clone();
                    let source = source.clone();
                    let kind = kind.clone();
                    let initial_version = source.version();
                    let cache = Arc::new(Mutex::new((initial_version, path)));
                    let redraw = gaanim_animation::AlwaysRedrawRegen::new(move |_world| {
                        let version = source.version();
                        let mut cached = cache.lock().expect("data mark cache poisoned");
                        if cached.0 != version {
                            cached.1 = gaanim_visualization::data_mark_path(
                                &map,
                                &source.snapshot(),
                                &kind,
                            )
                            .unwrap_or_default();
                            cached.0 = version;
                        }
                        cached.1.clone()
                    });
                    builder.commands.entity(state.entity).insert(redraw);
                }
                mr
            }
            SpawnKind::Axes {
                x_range,
                y_range,
                config,
            } => {
                let axes = Self::styled_axes(builder, *x_range, *y_range, config, frame_bounds);
                Self::post_apply(builder, axes.id, spec, id_map, frame_bounds);
                axes
            }
            SpawnKind::Axes3D {
                x_range,
                y_range,
                z_range,
                config,
            } => {
                let axes = Self::styled_axes_3d(
                    builder,
                    *x_range,
                    *y_range,
                    *z_range,
                    config,
                    frame_bounds,
                );
                Self::post_apply(builder, axes.id, spec, id_map, frame_bounds);
                axes
            }
            SpawnKind::SurfaceMesh {
                vertices,
                indices,
                color,
                colors,
            } => {
                let colors = colors.as_ref().map(|colors| {
                    colors
                        .iter()
                        .map(|color| {
                            let rgba = color.to_rgba8();
                            [
                                rgba.r as f32 / 255.0,
                                rgba.g as f32 / 255.0,
                                rgba.b as f32 / 255.0,
                                rgba.a as f32 / 255.0,
                            ]
                        })
                        .collect()
                });
                let mref = builder.spawn_triangle_mesh_with_colors(
                    vertices.clone(),
                    indices.clone(),
                    *color,
                    colors,
                );
                Self::post_apply(builder, mref.id, spec, id_map, frame_bounds);
                mref
            }
            SpawnKind::Primitive3D(mesh) => {
                let mref = builder.spawn_triangle_mesh_data(mesh.clone());
                Self::post_apply(builder, mref.id, spec, id_map, frame_bounds);
                mref
            }
            SpawnKind::Polyline3D { points, colors } => {
                let base_color = spec
                    .stroke
                    .as_ref()
                    .and_then(|(brush, _)| match brush {
                        gaanim_core::peniko::Brush::Solid(color) => Some(*color),
                        _ => None,
                    })
                    .or_else(|| {
                        spec.fill.as_ref().and_then(|brush| match brush {
                            gaanim_core::peniko::Brush::Solid(color) => Some(*color),
                            _ => None,
                        })
                    })
                    .unwrap_or(gaanim_core::peniko::Color::from_rgb8(20, 20, 20));
                let mref = if let Some(cols) = colors {
                    // Convert peniko::Color Vec to linear RGBA f32 for vertex colors
                    let cols_f32: Vec<[f32; 4]> = cols
                        .iter()
                        .map(|c| {
                            let rgba = c.to_rgba8();
                            [
                                rgba.r as f32 / 255.0,
                                rgba.g as f32 / 255.0,
                                rgba.b as f32 / 255.0,
                                rgba.a as f32 / 255.0,
                            ]
                        })
                        .collect();
                    if cols_f32.len() == points.len() {
                        builder.spawn_line_strip_with_colors(
                            points.clone(),
                            base_color,
                            Some(cols_f32),
                        )
                    } else {
                        // Mismatched lengths: fallback to uniform color (ignore per-vertex)
                        builder.spawn_line_strip(points.clone(), base_color)
                    }
                } else {
                    builder.spawn_line_strip(points.clone(), base_color)
                };
                Self::post_apply(builder, mref.id, spec, id_map, frame_bounds);
                mref
            }
            SpawnKind::LineSegments3D { points, colors } => {
                let base_color = spec
                    .stroke
                    .as_ref()
                    .and_then(|(brush, _)| match brush {
                        gaanim_core::peniko::Brush::Solid(color) => Some(*color),
                        _ => None,
                    })
                    .or_else(|| {
                        spec.fill.as_ref().and_then(|brush| match brush {
                            gaanim_core::peniko::Brush::Solid(color) => Some(*color),
                            _ => None,
                        })
                    })
                    .unwrap_or(gaanim_core::peniko::Color::from_rgb8(20, 20, 20));
                let colors = colors.as_ref().map(|colors| {
                    colors
                        .iter()
                        .map(|color| {
                            let rgba = color.to_rgba8();
                            [
                                rgba.r as f32 / 255.0,
                                rgba.g as f32 / 255.0,
                                rgba.b as f32 / 255.0,
                                rgba.a as f32 / 255.0,
                            ]
                        })
                        .collect::<Vec<_>>()
                });
                let mref = builder.spawn_line_list_with_colors(
                    points.clone(),
                    base_color,
                    colors.filter(|colors| colors.len() == points.len()),
                );
                Self::post_apply(builder, mref.id, spec, id_map, frame_bounds);
                mref
            }
            SpawnKind::GltfNode {
                node_index,
                path,
                bounds,
            } => {
                let id = builder.next_id();
                let transform = SpatialTransform::identity();
                let entity = builder
                    .commands
                    .spawn((
                        GltfNodeWrapper {
                            node_index: *node_index,
                            path: path.clone(),
                        },
                        GroupMarker,
                        MobjectId(id),
                        transform,
                        GlobalSpatialTransform::from_local(&transform),
                        Transform::default(),
                        Visibility::default(),
                        Opacity::default(),
                        GlobalOpacity::default(),
                        LocalBounds(*bounds),
                        WorldBounds(*bounds),
                        RenderLayer::Wgpu3D,
                        RenderOrder::default(),
                        Visible,
                    ))
                    .id();
                builder.tag_entity(entity);
                builder.states.insert(
                    id,
                    MobjectState {
                        path: std::sync::Arc::new(gaanim_core::kurbo::BezPath::new()),
                        bounds: *bounds,
                        transform,
                        opacity: 1.0,
                        fill: None,
                        stroke: StrokeBrush::transparent(),
                        entity,
                        child_spans: Vec::new(),
                        children: Vec::new(),
                        parent: None,
                        exclude_from_parent_draw: false,
                    },
                );
                let mref = MobjectRef { id };
                Self::post_apply(builder, id, spec, id_map, frame_bounds);
                mref
            }
            SpawnKind::GltfModel {
                path,
                scene_index,
                bounds,
                nodes,
                animation_names,
            } => {
                let id = builder.next_id();
                let transform = SpatialTransform::identity();
                let entity = builder
                    .commands
                    .spawn((
                        GroupMarker,
                        MobjectId(id),
                        transform,
                        GlobalSpatialTransform::from_local(&transform),
                        Transform::default(),
                        Visibility::default(),
                        Opacity::default(),
                        GlobalOpacity::default(),
                        LocalBounds(*bounds),
                        WorldBounds(*bounds),
                        RenderLayer::Wgpu3D,
                        RenderOrder::default(),
                        Visible,
                    ))
                    .id();
                builder.tag_entity(entity);

                let mapped = nodes
                    .iter()
                    .filter_map(|(index, _, node_path, canvas_id)| {
                        let compiled_id = id_map.get(canvas_id).copied()?;
                        let wrapper = builder.states.get(compiled_id)?.entity;
                        Some((*index, node_path.clone(), compiled_id, wrapper))
                    })
                    .collect::<Vec<_>>();
                let node_entities = mapped
                    .iter()
                    .map(|(index, _, _, wrapper)| (*index, *wrapper))
                    .collect::<HashMap<_, _>>();
                let canvas_ids = mapped
                    .iter()
                    .map(|(index, _, canvas_id, _)| (*index, *canvas_id))
                    .collect::<HashMap<_, _>>();
                for (node_index, parent_index, _, canvas_id) in nodes {
                    let Some(compiled_id) = id_map.get(canvas_id).copied() else {
                        continue;
                    };
                    let Some(state) = builder.states.get_mut(compiled_id) else {
                        continue;
                    };
                    let parent_entity = parent_index
                        .and_then(|parent| node_entities.get(&parent).copied())
                        .unwrap_or(entity);
                    state.parent = parent_index
                        .and_then(|parent| canvas_ids.get(&parent).copied())
                        .or(Some(id));
                    builder
                        .commands
                        .entity(state.entity)
                        .insert(ChildOf(parent_entity));
                    let _ = node_index;
                }
                builder.commands.entity(entity).insert(GltfModelRoot {
                    path: path.clone(),
                    scene_index: *scene_index,
                    nodes: mapped
                        .iter()
                        .map(|(node_index, node_path, _, wrapper)| GltfNodeBinding {
                            node_index: *node_index,
                            path: node_path.clone(),
                            wrapper: *wrapper,
                        })
                        .collect(),
                    animation_names: animation_names.clone(),
                });
                let child_ids = mapped.iter().map(|(_, _, id, _)| *id).collect::<Vec<_>>();
                builder.states.insert(
                    id,
                    MobjectState {
                        path: std::sync::Arc::new(gaanim_core::kurbo::BezPath::new()),
                        bounds: *bounds,
                        transform,
                        opacity: 1.0,
                        fill: None,
                        stroke: StrokeBrush::transparent(),
                        entity,
                        child_spans: Vec::new(),
                        children: child_ids,
                        parent: None,
                        exclude_from_parent_draw: false,
                    },
                );
                let mref = MobjectRef { id };
                Self::post_apply(builder, id, spec, id_map, frame_bounds);
                mref
            }
            SpawnKind::Text(text) => {
                let role = &text_config.roles[&text.role];
                let mut styled_spec =
                    Self::with_default_text_fill(spec, text.style.color.unwrap_or(role.fill_color));
                if let Some(opacity) = text.style.opacity {
                    styled_spec.opacity *= opacity.clamp(0.0, 1.0);
                }
                if !styled_spec.stroke_overridden
                    && let (Some(color), Some(width)) =
                        (text.style.stroke_color, text.style.stroke_width)
                {
                    styled_spec.stroke =
                        Some((gaanim_core::peniko::Brush::Solid(color), width.max(0.0)));
                }
                let compiled = compiled_text_measure(&styled_spec, text_config)
                    .expect("unified text spawn must produce a text measure");
                let source = structured_text_typst_source(
                    text,
                    Some(frame_bounds.width().max(1.0)),
                    compiled.font_size,
                    &compiled.font_family,
                    compiled.color,
                );
                let mr = builder.typst(
                    &source,
                    false,
                    Some(&compiled.font_family),
                    Some(&compiled.math_font),
                    Some(compiled.font_size),
                    Some(compiled.font_size),
                );
                Self::post_apply(builder, mr.id, &styled_spec, id_map, frame_bounds);
                Self::apply_fragment_fills(builder, mr, &styled_spec);
                mr
            }
            SpawnKind::Typst { source, page_width } => {
                let body = &text_config.roles[&gaanim_text::prelude::TextRole::Body];
                let foreground = typst_foreground_for_background(scene_background);
                let page_directive = if let Some(w) = page_width {
                    // ponytail: raw interpolation into Typst source — reject control chars to avoid injection
                    if w.trim().is_empty() || w.contains(['\n', '\r', ';', '"', '\'']) {
                        panic!("invalid Typst page width: {w:?}");
                    }
                    format!("#set page(width: {w}, height: auto, margin: 0pt)\n")
                } else {
                    "#set page(height: auto, margin: 0pt)\n".to_string()
                };
                let source =
                    format!("{page_directive}#set text(fill: rgb(\"{foreground}\"))\n{source}");
                let mr = builder.typst(&source, false, Some(&body.font_family), None, None, None);
                Self::post_apply(builder, mr.id, spec, id_map, frame_bounds);
                Self::apply_fragment_fills(builder, mr, spec);
                mr
            }
            SpawnKind::Image { image, view } => {
                let b = builder.image(image.clone(), *view);
                let mr = Self::finish_spawn_builder(b, spec);
                Self::apply_layout(builder, mr.id, spec, id_map, frame_bounds);
                mr
            }
            SpawnKind::SvgPath(path) => {
                let b = builder.svg_path(path);
                let mr = Self::finish_spawn_builder(b, spec);
                Self::apply_layout(builder, mr.id, spec, id_map, frame_bounds);
                mr
            }
            SpawnKind::Group(ids) => {
                let refs: Vec<MobjectRef> = ids
                    .iter()
                    .filter_map(|id| id_map.get(id).copied().map(|id| MobjectRef { id }))
                    .collect();
                let mr = builder.group(&refs);
                Self::post_apply(builder, mr.id, spec, id_map, frame_bounds);
                if let Some(layout) = &spec.reactive_readout_layout
                    && let Some(state) = builder.states.get(mr.id)
                {
                    builder.commands.entity(state.entity).insert(
                        gaanim_animation::ReactiveReadoutLayout {
                            label: layout.label.and_then(|id| id_map.get(&id).copied()),
                            equals: layout.equals.and_then(|id| id_map.get(&id).copied()),
                            number: id_map.get(&layout.number).copied().unwrap_or(layout.number),
                            unit: layout.unit.and_then(|id| id_map.get(&id).copied()),
                            spacing: layout.spacing,
                        },
                    );
                }
                mr
            }
            SpawnKind::GroupNoCenter(ids) => {
                let refs: Vec<MobjectRef> = ids
                    .iter()
                    .filter_map(|id| id_map.get(id).copied().map(|id| MobjectRef { id }))
                    .collect();
                let mr = builder.group_identity(&refs);
                Self::post_apply(builder, mr.id, spec, id_map, frame_bounds);
                mr
            }
            SpawnKind::ValueTracker(initial) => {
                // Spawn a FloatSignal entity (no visual output).
                let new_id = builder.next_id();
                let entity = builder
                    .commands
                    .spawn((
                        gaanim_scene::MobjectId(new_id),
                        gaanim_animation::FloatSignal::new(*initial),
                    ))
                    .id();
                builder.tag_entity(entity);
                builder.states.insert(
                    new_id,
                    MobjectState {
                        path: std::sync::Arc::new(gaanim_core::kurbo::BezPath::new()),
                        bounds: Bounds3D::default(),
                        transform: SpatialTransform::default(),
                        opacity: 1.0,
                        fill: None,
                        stroke: StrokeBrush::default(),
                        entity,
                        child_spans: Vec::new(),
                        children: Vec::new(),
                        parent: None,
                        exclude_from_parent_draw: false,
                    },
                );
                builder.float_signals.insert(new_id, *initial);
                MobjectRef { id: new_id }
            }
            SpawnKind::TracedPathLine => {
                // Spawn a minimal line (0,0)→(0,0). TracedPath will overwrite its Path2D.
                let b = builder.line(Point::new(0.0, 0.0), Point::new(0.0, 0.0));
                let mr = Self::finish_spawn_builder(b, spec);
                mr
            }
            SpawnKind::TrackingLine => {
                // Spawn a minimal line (0,0)→(0,0). TrackingLine will overwrite its Path2D.
                let b = builder.line(Point::new(0.0, 0.0), Point::new(0.0, 0.0));
                let mr = Self::finish_spawn_builder(b, spec);
                mr
            }
            SpawnKind::TracedPath3DLine => {
                // Empty 3D line placeholder; TracedPath3D will fill its LineListData.
                let mref = builder.spawn_line_strip(vec![], gaanim_core::peniko::Color::WHITE);
                Self::post_apply(builder, mref.id, spec, id_map, frame_bounds);
                mref
            }
        }
    }

    fn finish_spawn_builder<'b, 'w, 's, 'a>(
        mut b: crate::builder::MobjectSpawnBuilder<'b, 'w, 's, 'a>,
        spec: &ObjectSpec,
    ) -> MobjectRef {
        if spec.stroke_overridden {
            if let Some((ref brush, w)) = spec.stroke {
                b = if let Some(style) = &spec.stroke_style {
                    b.stroke_with_style(brush.clone(), style.clone())
                } else {
                    b.stroke_brush(brush.clone(), w)
                };
            } else {
                b = b.no_stroke();
            }
        }
        if spec.fill_overridden {
            if let Some(ref f) = spec.fill {
                b = b.fill_brush(f.clone());
            } else {
                b = b.no_fill();
            }
        }
        b = b.opacity(spec.opacity).z_index(spec.z_index);
        b.spawn_with_effects(spec.glow.clone(), spec.blur, spec.shadow.clone())
    }

    /// Applies deferred glyph-level color overrides after the normal object
    /// style has been propagated to the compiled text hierarchy.
    fn apply_fragment_fills(builder: &mut SceneBuilder, target: MobjectRef, spec: &ObjectSpec) {
        for (fragment, color) in &spec.fragment_fills {
            builder.select(target, fragment).set_fill(*color);
        }
    }

    fn with_default_text_fill(spec: &ObjectSpec, color: PenikoColor) -> ObjectSpec {
        let mut styled = spec.clone();
        if !styled.fill_overridden {
            styled.fill = Some(gaanim_core::peniko::Brush::Solid(color));
            // The Typst source already applies this inherited/default paint.
            // Keep it non-overridden so `post_apply` does not flatten the
            // locally styled fills of semantic text parts back to one color.
        }
        styled
    }

    fn post_apply(
        builder: &mut SceneBuilder,
        id: ObjectId,
        spec: &ObjectSpec,
        id_map: &HashMap<ObjectId, ObjectId>,
        frame_bounds: Bounds3D,
    ) {
        let mut child_spans = Vec::new();
        if let Some(st) = builder.states.get_mut(id) {
            child_spans = st.child_spans.clone();
            let is_textual_hierarchy = !child_spans.is_empty();
            if spec.stroke_overridden {
                if let Some((ref brush, w)) = spec.stroke {
                    let sb = StrokeBrush {
                        brush: Some(brush.clone()),
                        style: spec
                            .stroke_style
                            .clone()
                            .unwrap_or_else(|| gaanim_core::kurbo::Stroke::new(w)),
                    };
                    st.stroke = sb.clone();
                    if !is_textual_hierarchy {
                        builder.commands.entity(st.entity).insert(sb);
                    }
                } else {
                    st.stroke = StrokeBrush::transparent();
                    if !is_textual_hierarchy {
                        builder
                            .commands
                            .entity(st.entity)
                            .insert(StrokeBrush::transparent());
                    }
                }
            }
            if spec.fill_overridden {
                if let Some(ref f) = spec.fill {
                    st.fill = Some(f.clone());
                    if !is_textual_hierarchy {
                        builder
                            .commands
                            .entity(st.entity)
                            .insert(FillBrush(Some(f.clone())));
                    }
                } else {
                    st.fill = None;
                    if !is_textual_hierarchy {
                        builder
                            .commands
                            .entity(st.entity)
                            .insert(FillBrush::transparent());
                    }
                }
            }
            if spec.opacity != 1.0 {
                st.opacity = spec.opacity;
                builder
                    .commands
                    .entity(st.entity)
                    .insert(Opacity(spec.opacity));
            }
            if spec.z_index != 0 {
                builder.commands.entity(st.entity).insert(RenderOrder {
                    z_index: spec.z_index,
                    ..Default::default()
                });
            }
        }
        if spec.fill_overridden {
            for child in &child_spans {
                if let Some(child_state) = builder.states.get_mut(child.id) {
                    child_state.fill = spec.fill.clone();
                    if let Some(ref f) = spec.fill
                        && child_state.stroke.brush.is_some()
                    {
                        child_state.stroke.brush = Some(f.clone());
                        builder
                            .commands
                            .entity(child.entity)
                            .insert(child_state.stroke.clone());
                    }
                }
                builder
                    .commands
                    .entity(child.entity)
                    .insert(if let Some(ref f) = spec.fill {
                        FillBrush(Some(f.clone()))
                    } else {
                        FillBrush::transparent()
                    });
            }
        }
        if spec.opacity != 1.0 {
            for child in &child_spans {
                if let Some(child_state) = builder.states.get_mut(child.id) {
                    child_state.opacity = spec.opacity;
                }
                builder
                    .commands
                    .entity(child.entity)
                    .insert(Opacity(spec.opacity));
            }
        }
        if spec.stroke_overridden {
            for child in &child_spans {
                let sb = if let Some((ref brush, w)) = spec.stroke {
                    StrokeBrush {
                        brush: Some(brush.clone()),
                        style: spec
                            .stroke_style
                            .clone()
                            .unwrap_or_else(|| gaanim_core::kurbo::Stroke::new(w)),
                    }
                } else {
                    StrokeBrush::transparent()
                };
                if let Some(child_state) = builder.states.get_mut(child.id) {
                    child_state.stroke = sb.clone();
                }
                builder.commands.entity(child.entity).insert(sb);
            }
        }
        let effect_targets = if child_spans.is_empty() {
            builder
                .states
                .get(id)
                .map(|state| vec![state.entity])
                .unwrap_or_default()
        } else {
            child_spans.iter().map(|child| child.entity).collect()
        };
        for entity in effect_targets {
            let mut commands = builder.commands.entity(entity);
            if let Some(glow) = &spec.glow {
                commands.insert(glow.clone());
            }
            if let Some(blur) = spec.blur {
                commands.insert(blur);
            }
            if let Some(shadow) = &spec.shadow {
                commands.insert(shadow.clone());
            }
        }
        // Billboard / HUD chaining (.billboard() / .hud())
        if spec.billboard {
            if let Some(state) = builder.states.get(id) {
                builder
                    .commands
                    .entity(state.entity)
                    .insert(gaanim_scene::Billboard);
                builder
                    .commands
                    .entity(state.entity)
                    .insert(bevy::prelude::Transform::default());
            }
        }
        if spec.hud {
            // HUD is screen-space fixed but still rendered via Vello2D.
            // Keeping RenderLayer::Vello2D ensures it participates in the
            // main Vello pass which is now drawn AFTER the 3D meshes (order
            // 1 vs 0) and after background suppression for perspective, so
            // HUD appears on top of 3D. HudOverlay is retained as a marker
            // for potential future dedicated overlay pass.
            let targets = if child_spans.is_empty() {
                builder
                    .states
                    .get(id)
                    .map(|s| vec![s.entity])
                    .unwrap_or_default()
            } else {
                child_spans.iter().map(|c| c.entity).collect::<Vec<_>>()
            };
            for entity in targets {
                builder
                    .commands
                    .entity(entity)
                    .insert(gaanim_scene::HudOverlay);
                // Keep Vello2D so the element is rendered; ensure high z for HUD.
                builder
                    .commands
                    .entity(entity)
                    .insert(gaanim_scene::RenderOrder {
                        z_index: 1000,
                        ..Default::default()
                    });
            }
            if let Some(state) = builder.states.get(id) {
                builder
                    .commands
                    .entity(state.entity)
                    .insert(gaanim_scene::HudOverlay);
                builder
                    .commands
                    .entity(state.entity)
                    .insert(gaanim_scene::RenderOrder {
                        z_index: 1000,
                        ..Default::default()
                    });
            }
        }
        Self::apply_layout(builder, id, spec, id_map, frame_bounds);
    }

    fn apply_layout(
        builder: &mut SceneBuilder,
        id: ObjectId,
        spec: &ObjectSpec,
        id_map: &HashMap<ObjectId, ObjectId>,
        frame_bounds: Bounds3D,
    ) {
        if spec.layout_ops.is_empty() {
            return;
        }

        let Some(state) = builder.states.get(id) else {
            return;
        };
        let bounds = state.bounds;
        let original_transform = state.transform;
        let entity = state.entity;
        let mut transform = original_transform;
        let mut pivot_in_scene = None;

        for op in &spec.layout_ops {
            match op {
                LayoutOp::SetTranslation(translation) => {
                    transform.translation = if matches!(spec.kind, SpawnKind::Group(_)) {
                        *translation - bounds.center()
                    } else {
                        *translation
                    };
                }
                LayoutOp::SetScale(factor) => {
                    transform.scale = original_transform.scale * *factor;
                }
                LayoutOp::SetScale3D(scale) => {
                    transform.scale = original_transform.scale * *scale;
                }
                LayoutOp::SetRotation(radians) => {
                    transform.rotation = gaanim_core::glam::DQuat::from_rotation_z(*radians);
                }
                LayoutOp::SetRotation3D(euler) => {
                    transform.rotation = gaanim_core::glam::DQuat::from_euler(
                        gaanim_core::glam::EulerRot::XYZ,
                        euler.x,
                        euler.y,
                        euler.z,
                    );
                }
                LayoutOp::SetPivot(pivot) => {
                    pivot_in_scene = Some(*pivot);
                }
                LayoutOp::MoveAnchorTo { target, anchor } => {
                    transform =
                        gaanim_layout::compute_move_to(bounds, &transform, *target, *anchor);
                }
                LayoutOp::NextTo {
                    reference,
                    direction,
                    spacing,
                    aligned_edge,
                } => {
                    let Some(reference_id) = id_map.get(reference).copied() else {
                        bevy::prelude::warn!(
                            "Canvas layout skipped: reference object {:?} was not spawned before {:?}",
                            reference,
                            spec.id
                        );
                        continue;
                    };
                    let Some(reference_state) = builder.states.get(reference_id) else {
                        bevy::prelude::warn!(
                            "Canvas layout skipped: missing state for reference object {:?}",
                            reference_id
                        );
                        continue;
                    };
                    let reference_transform = builder.get_world_transform(reference_id);
                    let shift = gaanim_layout::compute_next_to_new(
                        bounds,
                        &transform,
                        reference_state.bounds,
                        &reference_transform,
                        *direction,
                        *spacing,
                        *aligned_edge,
                    );
                    transform = transform.shift_3d(shift);
                }
                LayoutOp::AlignTo {
                    reference,
                    target_anchor,
                    reference_anchor,
                } => {
                    let Some(reference_id) = id_map.get(reference).copied() else {
                        bevy::prelude::warn!(
                            "Canvas layout skipped: reference object {:?} was not spawned before {:?}",
                            reference,
                            spec.id
                        );
                        continue;
                    };
                    let Some(reference_state) = builder.states.get(reference_id) else {
                        bevy::prelude::warn!(
                            "Canvas layout skipped: missing state for reference object {:?}",
                            reference_id
                        );
                        continue;
                    };
                    let reference_transform = builder.get_world_transform(reference_id);
                    let shift = gaanim_layout::compute_align_to_new(
                        bounds,
                        &transform,
                        reference_state.bounds,
                        &reference_transform,
                        *target_anchor,
                        *reference_anchor,
                    );
                    transform = transform.shift_3d(shift);
                }
                LayoutOp::ToEdge { direction, buff } => {
                    transform = gaanim_layout::compute_to_edge(
                        bounds,
                        &transform,
                        *direction,
                        *buff,
                        frame_bounds,
                    );
                }
                LayoutOp::ToCorner { corner, buff } => {
                    transform = gaanim_layout::compute_to_corner(
                        bounds,
                        &transform,
                        *corner,
                        *buff,
                        frame_bounds,
                    );
                }
            }
        }

        if let Some(pivot) = pivot_in_scene {
            // SpatialTransform stores anchors in local coordinates, while the
            // public API accepts the stable scene-space point users see.
            transform.anchor = pivot - transform.translation;
        }

        if transform != original_transform {
            if let Some(state) = builder.states.get_mut(id) {
                state.transform = transform;
            }
            builder.commands.entity(entity).insert(transform);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::canvas::DrawableHandle;
    use bevy::ecs::world::CommandQueue;
    use gaanim_core::peniko::Brush;
    use gaanim_scene::LocalBounds;
    use gaanim_timeline::snapshot::WorldSnapshot;

    #[test]
    fn expression_reveal_ends_at_exact_data_coordinate() {
        let map = gaanim_visualization::CoordinateMap2D::new(
            gaanim_visualization::Axis::linear(0.0, 3.0 * std::f64::consts::PI).unwrap(),
            gaanim_visualization::Axis::linear(-1.0, 1.0).unwrap(),
            gaanim_visualization::PlotFrame::new(600.0, 240.0).unwrap(),
        );
        let expression = gaanim_expr::Expr::variable("x").sin();
        let reveal = gaanim_expr::Expr::constant(std::f64::consts::FRAC_PI_2);
        let path = sampled_expression_path(
            &map,
            &expression,
            "x",
            (0.0, 3.0 * std::f64::consts::PI),
            Some(&reveal),
            gaanim_visualization::Sampling::Fixed { samples: 65 },
            &gaanim_expr::EvalContext::new(),
        );

        assert!((path.bounding_box().x1 + 200.0).abs() < 1e-9);
        assert!(
            sampled_expression_path(
                &map,
                &expression,
                "x",
                (0.0, 3.0 * std::f64::consts::PI),
                Some(&gaanim_expr::Expr::constant(0.0)),
                gaanim_visualization::Sampling::Fixed { samples: 65 },
                &gaanim_expr::EvalContext::new(),
            )
            .is_empty()
        );
    }

    trait UnifiedTextFixture {
        fn math_text(&mut self, source: &str) -> DrawableHandle;
        fn test_title(&mut self, source: &str) -> DrawableHandle;
        fn test_subtitle(&mut self, source: &str) -> DrawableHandle;
        fn configured_text(
            &mut self,
            source: &str,
            style: gaanim_text::prelude::TextStyle,
            flow: gaanim_text::prelude::TextFlow,
        ) -> DrawableHandle;
    }

    impl UnifiedTextFixture for Canvas {
        fn math_text(&mut self, source: &str) -> DrawableHandle {
            self.text(&format!("${source}$"))
        }

        fn test_title(&mut self, source: &str) -> DrawableHandle {
            let spec = StructuredTextSpec::new(
                vec![source.into()],
                Some(gaanim_text::prelude::TextRole::Title),
                gaanim_text::prelude::TextStyle::default(),
                gaanim_text::prelude::TextFlow::default(),
            )
            .expect("valid title fixture");
            self.text_spec(spec)
        }

        fn test_subtitle(&mut self, source: &str) -> DrawableHandle {
            let spec = StructuredTextSpec::new(
                vec![source.into()],
                Some(gaanim_text::prelude::TextRole::Subtitle),
                gaanim_text::prelude::TextStyle::default(),
                gaanim_text::prelude::TextFlow::default(),
            )
            .expect("valid subtitle fixture");
            self.text_spec(spec)
        }

        fn configured_text(
            &mut self,
            source: &str,
            style: gaanim_text::prelude::TextStyle,
            flow: gaanim_text::prelude::TextFlow,
        ) -> DrawableHandle {
            let spec = StructuredTextSpec::new(vec![source.into()], None, style, flow)
                .expect("valid configured text fixture");
            self.text_spec(spec)
        }
    }

    #[test]
    fn structured_math_parts_remain_inline_across_semantic_boundaries() {
        let spec = StructuredTextSpec::new(
            vec![
                "$".into(),
                gaanim_text::prelude::TextPart::new(
                    "variable",
                    vec!["x".into()],
                    StructuredTextStyle::default(),
                )
                .into(),
                " dot 5 = ".into(),
                gaanim_text::prelude::TextPart::new(
                    "result",
                    vec!["25".into()],
                    StructuredTextStyle::default(),
                )
                .into(),
                "$".into(),
            ],
            None,
            StructuredTextStyle::default(),
            gaanim_text::prelude::TextFlow::default(),
        )
        .expect("valid structured equation");

        let source = structured_typst_content(&spec, 32.0);
        assert_eq!(
            source,
            "$x$#h(8.96pt, weak: false)$dot 5 =$#h(8.96pt, weak: false)$25$"
        );
        assert!(!source.contains("$$"));
        assert!(!source.contains("$ "));
        assert!(!source.contains(" $"));
        assert!(!source.contains("#h(0pt)"));
    }

    #[test]
    fn structured_inline_markup_emits_typst_styles_and_skips_math() {
        let spec = StructuredTextSpec::new(
            vec!["Normal, _emphasis_, *strong*, *_both_* y $x_1 * 5$.".into()],
            None,
            StructuredTextStyle::default(),
            gaanim_text::prelude::TextFlow::default(),
        )
        .expect("valid inline markup");

        let source = structured_typst_content(&spec, 32.0);
        assert!(source.contains("style: \"italic\""));
        assert!(source.contains("weight: 700"));
        assert!(source.contains("$x_1 * 5$"));
        assert!(!source.contains("_emphasis_"));
        assert!(!source.contains("*strong*"));
    }

    #[test]
    fn structured_math_boundary_spaces_increase_the_compiled_width() {
        let equation = |with_boundary_spaces: bool| {
            StructuredTextSpec::new(
                vec![
                    "$".into(),
                    gaanim_text::prelude::TextPart::new(
                        "variable",
                        vec!["5".into()],
                        StructuredTextStyle::default(),
                    )
                    .into(),
                    if with_boundary_spaces {
                        " 5 = ".into()
                    } else {
                        "5 =".into()
                    },
                    gaanim_text::prelude::TextPart::new(
                        "result",
                        vec!["25".into()],
                        StructuredTextStyle::default(),
                    )
                    .into(),
                    "$".into(),
                ],
                None,
                StructuredTextStyle::default(),
                gaanim_text::prelude::TextFlow::default(),
            )
            .expect("valid structured equation")
        };
        let mut canvas = Canvas::new(640, 360);
        canvas.text_spec(equation(false));
        canvas.text_spec(equation(true));

        let world = World::new();
        let mut queue = CommandQueue::default();
        let mut commands = Commands::new(&mut queue, &world);
        let mut timeline = Timeline::new();
        let fonts = gaanim_text::font::FontRegistry::new();
        let text_config = gaanim_text::prelude::TextConfig::default();
        canvas.compile_into(&mut commands, &mut timeline, &fonts, &text_config);
        drop(commands);
        let mut world = world;
        queue.apply(&mut world);

        let mut widths = world
            .query::<(&LocalBounds, Option<&bevy::prelude::ChildOf>)>()
            .iter(&world)
            .filter_map(|(bounds, parent)| parent.is_none().then_some(bounds.0.width()))
            .collect::<Vec<_>>();
        widths.sort_by(f64::total_cmp);
        assert_eq!(
            widths.len(),
            2,
            "expected two compiled text roots: {widths:?}"
        );
        assert!(
            widths[1] > widths[0] + 6.0,
            "part-boundary spaces must add visible width: {widths:?}"
        );
    }

    #[test]
    fn camera_and_drawable_play_compile_at_the_same_start_time() {
        let mut canvas = Canvas::new(640, 360);
        let marker = canvas.circle(20.0);
        let marker_anim = marker.fade_in(2.0);
        let camera_anim = canvas.camera_orbit(0.4, 0.1, 2.0);
        canvas.play(vec![marker_anim, camera_anim]);

        let world = World::new();
        let mut queue = CommandQueue::default();
        let mut commands = Commands::new(&mut queue, &world);
        let mut timeline = Timeline::new();
        let fonts = gaanim_text::font::FontRegistry::new();
        let text_config = gaanim_text::prelude::TextConfig::default();
        canvas.compile_into(&mut commands, &mut timeline, &fonts, &text_config);

        let parallel_clips: Vec<_> = timeline
            .clips
            .values()
            .filter(|clip| (clip.duration - 2.0).abs() < 1e-9)
            .collect();
        assert_eq!(parallel_clips.len(), 3); // fade + orbit position + orbit rotation
        assert!(parallel_clips.iter().all(|clip| clip.start.abs() < 1e-9));
        assert!((timeline.cached_duration - 2.0).abs() < 1e-9);
    }

    #[test]
    fn object_declared_after_wait_stays_hidden_until_its_declaration_time() {
        let mut canvas = Canvas::new(640, 360);
        canvas.wait(1.0);
        canvas.circle(20.0);

        let world = World::new();
        let mut queue = CommandQueue::default();
        let mut commands = Commands::new(&mut queue, &world);
        let mut timeline = Timeline::new();
        let fonts = gaanim_text::font::FontRegistry::new();
        let text_config = gaanim_text::prelude::TextConfig::default();
        canvas.compile_into(&mut commands, &mut timeline, &fonts, &text_config);
        drop(commands);

        let mut world = world;
        queue.apply(&mut world);
        timeline.add_keyframe(0.0, WorldSnapshot::capture(&mut world));

        timeline.seek(&mut world, 0.5);
        assert!(
            world
                .query::<&Opacity>()
                .iter(&world)
                .all(|opacity| opacity.0 == 0.0),
            "late-declared object leaked before its declaration time"
        );

        timeline.seek(&mut world, 1.0);
        assert!(
            world
                .query::<&Opacity>()
                .iter(&world)
                .any(|opacity| opacity.0 == 1.0),
            "late-declared object did not become visible at its declaration time"
        );
    }

    #[test]
    fn visual_effects_are_attached_to_compiled_drawables() {
        let mut canvas = Canvas::new(640, 360);
        canvas
            .circle(60.0)
            .glow(PenikoColor::WHITE, 18.0, 1.2)
            .blur(5.0)
            .shadow(
                PenikoColor::BLACK,
                gaanim_core::glam::DVec2::new(8.0, -8.0),
                7.0,
            );

        let world = World::new();
        let mut queue = CommandQueue::default();
        let mut commands = Commands::new(&mut queue, &world);
        let mut timeline = Timeline::new();
        let fonts = gaanim_text::font::FontRegistry::new();
        let text_config = gaanim_text::prelude::TextConfig::default();
        canvas.compile_into(&mut commands, &mut timeline, &fonts, &text_config);
        drop(commands);

        let mut world = world;
        queue.apply(&mut world);
        assert_eq!(
            world
                .query::<&gaanim_renderer::effects::Glow>()
                .iter(&world)
                .count(),
            1
        );
        assert_eq!(
            world
                .query::<&gaanim_renderer::effects::GaussianBlur>()
                .iter(&world)
                .count(),
            1
        );
        assert_eq!(
            world
                .query::<&gaanim_renderer::effects::DropShadow>()
                .iter(&world)
                .count(),
            1
        );
    }

    #[test]
    fn clip_mask_uses_another_drawables_world_geometry() {
        let mut canvas = Canvas::new(640, 360);
        let target = canvas.rect(300.0, 160.0).at(80.0, 0.0);
        let mask = canvas.circle(55.0).at(80.0, 0.0);
        target.clip(&mask, gaanim_core::peniko::Fill::NonZero);

        let world = World::new();
        let mut queue = CommandQueue::default();
        let mut commands = Commands::new(&mut queue, &world);
        let mut timeline = Timeline::new();
        let fonts = gaanim_text::font::FontRegistry::new();
        let text_config = gaanim_text::prelude::TextConfig::default();
        canvas.compile_into(&mut commands, &mut timeline, &fonts, &text_config);
        drop(commands);

        let mut world = world;
        queue.apply(&mut world);
        let masks = world
            .query::<&gaanim_renderer::effects::ClipMask>()
            .iter(&world)
            .collect::<Vec<_>>();
        assert_eq!(masks.len(), 1);
        assert!(!masks[0].path.is_empty());
        let bounds = masks[0].path.bounding_box();
        assert!((bounds.center().x).abs() < 1e-6);
        assert!((bounds.width() - 110.0).abs() < 1e-3);
    }

    #[test]
    fn segments_show_only_the_active_segment_on_seek() {
        let red = PenikoColor::from_rgb8(255, 0, 0);
        let blue = PenikoColor::from_rgb8(0, 0, 255);
        let mut canvas = Canvas::new(640, 360);
        canvas.segment("first", None).unwrap();
        canvas.text("First segment").fill(red);
        canvas.wait(1.0);
        canvas.segment("second", None).unwrap();
        canvas.text("Second segment").fill(blue);
        canvas.wait(1.0);

        let world = World::new();
        let mut queue = CommandQueue::default();
        let mut commands = Commands::new(&mut queue, &world);
        let mut timeline = Timeline::new();
        let fonts = gaanim_text::font::FontRegistry::new();
        let text_config = gaanim_text::prelude::TextConfig::default();
        canvas.compile_into(&mut commands, &mut timeline, &fonts, &text_config);
        drop(commands);

        let mut world = world;
        queue.apply(&mut world);
        timeline.add_keyframe(0.0, WorldSnapshot::capture(&mut world));

        let visible_for = |world: &mut World, color| {
            world
                .query::<(&FillBrush, Option<&gaanim_scene::Visible>)>()
                .iter(world)
                .find_map(|(fill, visible)| {
                    matches!(&fill.0, Some(Brush::Solid(found)) if *found == color)
                        .then_some(visible.is_some())
                })
                .expect("colored segment object should exist")
        };

        timeline.seek(&mut world, 0.0);
        assert!(visible_for(&mut world, red));
        assert!(!visible_for(&mut world, blue));

        timeline.seek(&mut world, 1.0);
        assert!(!visible_for(&mut world, red));
        assert!(visible_for(&mut world, blue));
    }

    #[test]
    fn justified_paragraph_compiles_to_vector_glyphs() {
        let mut canvas = Canvas::new(640, 360);
        canvas.configured_text(
            "Este párrafo debe ocupar varias líneas y conservar glifos vectoriales.",
            gaanim_text::prelude::TextStyle {
                size: Some(28.0),
                ..Default::default()
            },
            gaanim_text::prelude::TextFlow {
                wrap: gaanim_text::prelude::TextWrap::Width(180.0),
                align: gaanim_text::prelude::TextAlign::Justify,
                line_spacing: 1.25,
                ..Default::default()
            },
        );

        let world = World::new();
        let mut queue = CommandQueue::default();
        let mut commands = Commands::new(&mut queue, &world);
        let mut timeline = Timeline::new();
        let fonts = gaanim_text::font::FontRegistry::new();
        let text_config = gaanim_text::prelude::TextConfig::default();
        canvas.compile_into(&mut commands, &mut timeline, &fonts, &text_config);
        drop(commands);

        let mut world = world;
        queue.apply(&mut world);
        let mut query = world.query::<&LocalBounds>();
        let visible_bounds = query
            .iter(&world)
            .filter(|bounds| bounds.0.width() > 0.0 && bounds.0.height() > 0.0)
            .count();
        assert!(visible_bounds > 5, "paragraph should produce vector glyphs");
    }

    #[test]
    fn paragraph_max_lines_emits_a_clipped_text_box() {
        let spec = StructuredTextSpec::new(
            vec!["A bounded paragraph".into()],
            None,
            gaanim_text::prelude::TextStyle {
                size: Some(30.0),
                ..Default::default()
            },
            gaanim_text::prelude::TextFlow {
                wrap: gaanim_text::prelude::TextWrap::Width(240.0),
                line_spacing: 1.2,
                max_lines: Some(2),
                overflow: gaanim_text::prelude::TextOverflow::Clip,
                ..Default::default()
            },
        )
        .unwrap();
        let source = structured_text_typst_source(
            &spec,
            Some(240.0),
            30.0,
            "New Computer Modern",
            gaanim_core::peniko::Color::WHITE,
        );

        assert!(source.contains("height: 72pt"));
        assert!(source.contains("clip: true"));
    }

    #[test]
    fn equation_fragment_fill_overrides_matching_vector_glyphs() {
        let highlight = gaanim_core::peniko::Color::from_rgb8(255, 180, 0);
        let mut canvas = Canvas::new(640, 360);
        canvas.math_text("E = m c^2").color_by("m", highlight);

        let world = World::new();
        let mut queue = CommandQueue::default();
        let mut commands = Commands::new(&mut queue, &world);
        let mut timeline = Timeline::new();
        let fonts = gaanim_text::font::FontRegistry::new();
        let text_config = gaanim_text::prelude::TextConfig::default();
        canvas.compile_into(&mut commands, &mut timeline, &fonts, &text_config);
        drop(commands);

        let mut world = world;
        queue.apply(&mut world);
        let mut query = world.query::<&gaanim_scene::FillBrush>();
        assert!(query.iter(&world).any(|fill| {
            matches!(
                &fill.0,
                Some(gaanim_core::peniko::Brush::Solid(color)) if *color == highlight
            )
        }));
    }

    #[test]
    fn text_fragment_fill_overrides_matching_vector_glyphs() {
        let highlight = gaanim_core::peniko::Color::from_rgb8(64, 180, 255);
        let mut canvas = Canvas::new(640, 360);
        canvas
            .text("Energy depends on mass")
            .color_by("mass", highlight);

        let world = World::new();
        let mut queue = CommandQueue::default();
        let mut commands = Commands::new(&mut queue, &world);
        let mut timeline = Timeline::new();
        let fonts = gaanim_text::font::FontRegistry::new();
        let text_config = gaanim_text::prelude::TextConfig::default();
        canvas.compile_into(&mut commands, &mut timeline, &fonts, &text_config);
        drop(commands);

        let mut world = world;
        queue.apply(&mut world);
        let mut query = world.query::<&gaanim_scene::FillBrush>();
        assert!(query.iter(&world).any(|fill| {
            matches!(
                &fill.0,
                Some(gaanim_core::peniko::Brush::Solid(color)) if *color == highlight
            )
        }));
    }

    #[test]
    fn compound_paint_animation_reaches_text_glyphs() {
        let fill_target = PenikoColor::from_rgb8(32, 96, 224);
        let stroke_target = PenikoColor::from_rgb8(255, 180, 0);
        let fragment_start = PenikoColor::from_rgb8(220, 32, 64);
        let mut canvas = Canvas::new(640, 360);
        let text = canvas
            .text("Color")
            .stroke(PenikoColor::WHITE, 2.0)
            .color_by("C", fragment_start);
        canvas.play(vec![
            text.animate()
                .color(fill_target)
                .stroke(stroke_target, 7.0)
                .duration(1.0),
        ]);

        let world = World::new();
        let mut queue = CommandQueue::default();
        let mut commands = Commands::new(&mut queue, &world);
        let mut timeline = Timeline::new();
        let fonts = gaanim_text::font::FontRegistry::new();
        let text_config = gaanim_text::prelude::TextConfig::default();
        canvas.compile_into(&mut commands, &mut timeline, &fonts, &text_config);
        drop(commands);

        let mut world = world;
        queue.apply(&mut world);
        timeline.add_keyframe(0.0, WorldSnapshot::capture(&mut world));
        timeline.seek(&mut world, 0.5);

        let mut halfway_fills = world
            .query::<(&FillBrush, &LocalBounds)>()
            .iter(&world)
            .filter_map(|(fill, bounds)| {
                (bounds.0.width() > 0.0 && bounds.0.height() > 0.0)
                    .then(|| match fill.0 {
                        Some(Brush::Solid(color)) => Some(color.to_rgba8()),
                        _ => None,
                    })
                    .flatten()
            })
            .collect::<Vec<_>>();
        halfway_fills.sort_by_key(|color| (color.r, color.g, color.b, color.a));
        halfway_fills.dedup();
        assert!(
            halfway_fills.len() > 1,
            "each glyph should interpolate from its own fill, including fragment overrides"
        );

        timeline.seek(&mut world, 1.0);

        let painted_glyphs = world
            .query::<(&FillBrush, &gaanim_scene::StrokeBrush, &LocalBounds)>()
            .iter(&world)
            .filter(|(fill, stroke, bounds)| {
                bounds.0.width() > 0.0
                    && bounds.0.height() > 0.0
                    && matches!(&fill.0, Some(Brush::Solid(color)) if *color == fill_target)
                    && matches!(&stroke.brush, Some(Brush::Solid(color)) if *color == stroke_target)
                    && (stroke.style.width - 7.0).abs() < 1e-9
            })
            .count();

        assert!(
            painted_glyphs > 1,
            "compound paint animation should update the visible text glyphs, got {painted_glyphs}"
        );
    }

    #[test]
    fn paper_theme_applies_role_fills_to_text_glyphs() {
        let mut canvas = Canvas::new(640, 360);
        canvas
            .set_theme("paper")
            .expect("paper is a built-in theme");
        canvas.test_title("Heading");
        canvas.test_subtitle("Subheading");
        canvas.text("Body copy");
        canvas.math_text("x = y");

        let world = World::new();
        let mut queue = CommandQueue::default();
        let mut commands = Commands::new(&mut queue, &world);
        let mut timeline = Timeline::new();
        let fonts = gaanim_text::font::FontRegistry::new();
        let text_config = canvas.themed_text_config();
        canvas.compile_into(&mut commands, &mut timeline, &fonts, &text_config);
        drop(commands);

        let mut world = world;
        queue.apply(&mut world);
        let fills: Vec<_> = world
            .query::<&gaanim_scene::FillBrush>()
            .iter(&world)
            .filter_map(|fill| match fill.0 {
                Some(gaanim_core::peniko::Brush::Solid(color)) => Some(color),
                _ => None,
            })
            .collect();

        assert!(fills.contains(&PenikoColor::BLACK));
        assert!(!fills.contains(&PenikoColor::WHITE));
    }

    #[test]
    fn theme_selected_after_authoring_is_materialized_during_compile() {
        let mut canvas = Canvas::new(640, 360);
        canvas.circle(40.0);
        canvas.circle(20.0).fill(PenikoColor::BLACK).at(100.0, 0.0);
        let mut theme = crate::canvas::CanvasTheme::builtin("paper").unwrap();
        let brand = PenikoColor::from_rgb8(0x25, 0x63, 0xEB);
        theme
            .set_colors(&HashMap::from([("accent".to_string(), brand)]))
            .unwrap();
        canvas.apply_theme(theme);

        let mut world = World::new();
        world.insert_resource(Timeline::new());
        world.insert_resource(gaanim_text::font::FontRegistry::new());
        world.insert_resource(gaanim_text::prelude::TextConfig::default());
        canvas.compile(&mut world);
        world.flush();

        let mut query = world.query::<&gaanim_scene::FillBrush>();
        let colors = query
            .iter(&world)
            .filter_map(|fill| match fill.0.as_ref() {
                Some(gaanim_core::peniko::Brush::Solid(color)) => Some(*color),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert!(colors.contains(&brand));
        assert!(colors.contains(&PenikoColor::BLACK));
    }

    #[test]
    fn dynamic_camera_frame_keeps_all_compiled_targets_and_bounds() {
        let mut canvas = Canvas::new(960, 540);
        let left = canvas.circle(85.0).at(-260.0, -10.0);
        let right = canvas.rect(180.0, 110.0).at(250.0, -10.0);
        let frame = canvas.camera_frame_many(
            &[left.clone(), right.clone()],
            [48.0, 72.0, 48.0, 72.0],
            true,
            1.0,
        );
        canvas.play(vec![frame]);

        let mut world = World::new();
        world.insert_resource(Timeline::new());
        world.insert_resource(gaanim_text::font::FontRegistry::new());
        world.insert_resource(gaanim_text::prelude::TextConfig::default());
        canvas.compile(&mut world);
        world.flush();

        let targets = world
            .resource::<Timeline>()
            .clips
            .values()
            .find_map(|clip| match &clip.payload {
                gaanim_timeline::clip::ClipPayload::Animation(animation) => match &animation.lens {
                    gaanim_timeline::clip::PropertyLensSpec::CameraFrameDynamic {
                        targets, ..
                    } => Some(targets.clone()),
                    _ => None,
                },
                _ => None,
            })
            .expect("dynamic camera frame clip");
        assert_eq!(targets.len(), 2);
        let bounds = targets
            .iter()
            .map(|target| gaanim_animation::resolve_entity_bounds(*target, &world))
            .collect::<Vec<_>>();
        assert!(bounds.iter().all(Option::is_some), "bounds: {bounds:?}");
        let union = bounds
            .into_iter()
            .flatten()
            .reduce(|left, right| left.union(&right))
            .unwrap();
        assert!(union.min.x < -300.0, "union: {union:?}");
        assert!(union.max.x > 300.0, "union: {union:?}");

        let mut timeline = world.remove_resource::<Timeline>().unwrap();
        timeline.add_keyframe(0.0, WorldSnapshot::capture(&mut world));
        timeline.seek(&mut world, 0.5);
        world.insert_resource(timeline);
        let restored_bounds = targets
            .iter()
            .map(|target| gaanim_animation::resolve_entity_bounds(*target, &world))
            .collect::<Vec<_>>();
        assert!(
            restored_bounds.iter().all(Option::is_some),
            "restored bounds: {restored_bounds:?}"
        );
        let restored_union = restored_bounds
            .into_iter()
            .flatten()
            .reduce(|left, right| left.union(&right))
            .unwrap();
        assert!(
            restored_union.min.x < -300.0,
            "restored: {restored_union:?}"
        );
        assert!(restored_union.max.x > 300.0, "restored: {restored_union:?}");
    }

    #[test]
    fn compiled_camera_binding_is_active_during_its_authored_window() {
        let mut canvas = Canvas::new(960, 540);
        let constraint = canvas
            .camera_bind_2d(
                Some(CanvasEndpoint::Static(DVec3::new(120.0, -35.0, 0.0))),
                Some(gaanim_expr::Expr::constant(1.4)),
                None,
                gaanim_expr::Expr::constant(1.0),
                true,
            )
            .unwrap();
        canvas.wait(2.0);
        constraint.disable();

        let mut world = World::new();
        world.insert_resource(Timeline::new());
        world.insert_resource(gaanim_text::font::FontRegistry::new());
        world.insert_resource(gaanim_text::prelude::TextConfig::default());
        canvas.compile(&mut world);
        world.flush();
        world.insert_resource(gaanim_math::Camera::ortho_2d(960, 540));

        assert_eq!(
            world
                .query::<&gaanim_animation::CameraBinding>()
                .iter(&world)
                .count(),
            1
        );
        gaanim_animation::apply_camera_bindings(&mut world, 1.0);
        let camera = world.resource::<gaanim_math::Camera>();
        assert_eq!(camera.position.x, 120.0);
        assert_eq!(camera.position.y, -35.0);
        assert!(matches!(
            camera.projection,
            gaanim_math::Projection::Orthographic { zoom: 1.4 }
        ));
    }

    #[test]
    fn compiled_camera_binding_resolves_point_ref_parameters() {
        let mut canvas = Canvas::new(960, 540);
        let parameter = canvas.parameter(0.0).unwrap();
        let x = parameter.expression() * 260.0 - 130.0;
        let point = canvas.point_ref(x, gaanim_expr::Expr::constant(25.0));
        let _constraint = canvas
            .camera_bind_2d(
                Some(point.0),
                None,
                None,
                gaanim_expr::Expr::constant(1.0),
                true,
            )
            .unwrap();

        let mut world = World::new();
        world.insert_resource(Timeline::new());
        world.insert_resource(gaanim_text::font::FontRegistry::new());
        world.insert_resource(gaanim_text::prelude::TextConfig::default());
        canvas.compile(&mut world);
        world.flush();
        world.insert_resource(gaanim_math::Camera::ortho_2d(960, 540));

        gaanim_animation::apply_camera_bindings(&mut world, 0.0);
        let camera = world.resource::<gaanim_math::Camera>();
        assert_eq!(camera.position.x, -130.0);
        assert_eq!(camera.position.y, 25.0);
    }

    #[test]
    fn fragment_transform_moves_selected_vector_glyphs() {
        let mut canvas = Canvas::new(640, 360);
        let source = canvas.math_text("E = m c^2");
        let target = canvas.math_text("p = m v");
        let morph = source
            .select("m")
            .morph_to(&target.select("m"), 0.8)
            .expect("selections share a Canvas");
        canvas.play(vec![morph]);

        let world = World::new();
        let mut queue = CommandQueue::default();
        let mut commands = Commands::new(&mut queue, &world);
        let mut timeline = Timeline::new();
        let fonts = gaanim_text::font::FontRegistry::new();
        let text_config = gaanim_text::prelude::TextConfig::default();
        canvas.compile_into(&mut commands, &mut timeline, &fonts, &text_config);

        assert!(timeline.clips.values().any(|clip| {
            matches!(
                &clip.payload,
                gaanim_timeline::clip::ClipPayload::Animation(
                    gaanim_timeline::clip::AnimationSpec {
                        lens: gaanim_timeline::clip::PropertyLensSpec::Translation { .. },
                        ..
                    }
                )
            )
        }));
    }

    #[test]
    fn named_fragment_tag_resolves_to_a_vector_selection() {
        let highlight = gaanim_core::peniko::Color::from_rgb8(64, 180, 255);
        let mut canvas = Canvas::new(640, 360);
        let formula = canvas.math_text("E = m c^2").define_tag("mass", "m", None);
        formula
            .tag("mass")
            .expect("registered tag should resolve")
            .fill(highlight);

        let world = World::new();
        let mut queue = CommandQueue::default();
        let mut commands = Commands::new(&mut queue, &world);
        let mut timeline = Timeline::new();
        let fonts = gaanim_text::font::FontRegistry::new();
        let text_config = gaanim_text::prelude::TextConfig::default();
        canvas.compile_into(&mut commands, &mut timeline, &fonts, &text_config);
        drop(commands);

        let mut world = world;
        queue.apply(&mut world);
        let mut query = world.query::<&gaanim_scene::FillBrush>();
        assert!(query.iter(&world).any(|fill| {
            matches!(
                &fill.0,
                Some(gaanim_core::peniko::Brush::Solid(color)) if *color == highlight
            )
        }));
    }

    #[test]
    fn cancel_term_places_a_diagonal_strike_over_the_selected_glyphs() {
        let strike_color = PenikoColor::WHITE;
        let mut canvas = Canvas::new(640, 360);
        let formula = canvas
            .math_text("x + 3 = 7")
            .define_tag("constant", "3", None);
        let cancel = formula
            .tag("constant")
            .expect("registered tag should resolve")
            .cancel(0.6);
        canvas.play(vec![cancel]);

        let world = World::new();
        let mut queue = CommandQueue::default();
        let mut commands = Commands::new(&mut queue, &world);
        let mut timeline = Timeline::new();
        let fonts = gaanim_text::font::FontRegistry::new();
        let text_config = gaanim_text::prelude::TextConfig::default();
        canvas.compile_into(&mut commands, &mut timeline, &fonts, &text_config);
        drop(commands);

        let mut world = world;
        queue.apply(&mut world);
        let selected_bounds = {
            let mut query = world.query::<(&gaanim_scene::components::TextSpan, &LocalBounds)>();
            query
                .iter(&world)
                .find_map(|(span, bounds)| (span.character == '3').then_some(bounds.0))
                .expect("the selected digit should retain its glyph bounds")
        };
        let strike_bounds = {
            let mut query = world.query::<(&gaanim_scene::StrokeBrush, &LocalBounds)>();
            query
                .iter(&world)
                .find_map(|(stroke, bounds)| {
                    matches!(&stroke.brush, Some(gaanim_core::peniko::Brush::Solid(color)) if *color == strike_color)
                        .then_some(bounds.0)
                })
                .expect("cancel should spawn a white strikethrough")
        };
        let center_delta = strike_bounds.center() - selected_bounds.center();
        assert!(
            center_delta.length() < 3.0,
            "cancel strike center {:?} should overlap selected glyph center {:?}",
            strike_bounds.center(),
            selected_bounds.center()
        );
        assert!(strike_bounds.width() > 0.0 && strike_bounds.height() > 0.0);
    }

    #[test]
    fn cancel_mark_fades_when_text_step_replaces_its_source() {
        let mut canvas = Canvas::new(640, 360);
        let source = canvas.text_spec(
            StructuredTextSpec::new(
                vec![
                    "$".into(),
                    gaanim_text::prelude::TextPart::new(
                        "variable",
                        vec!["x".into()],
                        StructuredTextStyle::default(),
                    )
                    .into(),
                    " + ".into(),
                    gaanim_text::prelude::TextPart::new(
                        "constant",
                        vec!["3".into()],
                        StructuredTextStyle::default(),
                    )
                    .into(),
                    " = ".into(),
                    gaanim_text::prelude::TextPart::new(
                        "result",
                        vec!["7".into()],
                        StructuredTextStyle::default(),
                    )
                    .into(),
                    "$".into(),
                ],
                None,
                StructuredTextStyle::default(),
                gaanim_text::prelude::TextFlow::default(),
            )
            .expect("valid source equation"),
        );
        let target = canvas.text_spec(
            StructuredTextSpec::new(
                vec![
                    "$".into(),
                    gaanim_text::prelude::TextPart::new(
                        "variable",
                        vec!["x".into()],
                        StructuredTextStyle::default(),
                    )
                    .into(),
                    " = ".into(),
                    gaanim_text::prelude::TextPart::new(
                        "result",
                        vec!["4".into()],
                        StructuredTextStyle::default(),
                    )
                    .into(),
                    "$".into(),
                ],
                None,
                StructuredTextStyle::default(),
                gaanim_text::prelude::TextFlow::default(),
            )
            .expect("valid target equation"),
        );
        canvas.play(vec![
            source.tag("constant").expect("constant tag").cancel(0.6),
        ]);
        canvas.play(vec![source.step_to(&target, None, 0.8).unwrap()]);

        let world = World::new();
        let mut queue = CommandQueue::default();
        let mut commands = Commands::new(&mut queue, &world);
        let mut timeline = Timeline::new();
        let fonts = gaanim_text::font::FontRegistry::new();
        let text_config = gaanim_text::prelude::TextConfig::default();
        canvas.compile_into(&mut commands, &mut timeline, &fonts, &text_config);
        drop(commands);

        let mut world = world;
        queue.apply(&mut world);
        let strike = world
            .query::<(
                bevy::prelude::Entity,
                &gaanim_scene::StrokeBrush,
                Option<&bevy::prelude::ChildOf>,
            )>()
            .iter(&world)
            .find_map(|(entity, stroke, parent)| {
                (parent.is_none() && stroke.brush.is_some()).then_some(entity)
            })
            .expect("cancel should spawn a root strike");

        timeline.add_keyframe(0.0, WorldSnapshot::capture(&mut world));
        timeline.seek(&mut world, 1.4);
        assert!(
            world
                .get::<Opacity>(strike)
                .is_some_and(|opacity| opacity.0 < 0.01),
            "the cancellation mark must leave with the replaced source text"
        );
    }

    #[test]
    fn tagged_equation_transform_moves_shared_tags() {
        let mut canvas = Canvas::new(640, 360);
        let source = canvas
            .math_text("E = m c^2")
            .define_tag("mass", "m", None)
            .at(0.0, 70.0);
        let target = canvas
            .math_text("p = m v")
            .define_tag("mass", "m", None)
            .at(0.0, -90.0);
        let copy = source
            .tag("mass")
            .unwrap()
            .copy_to(&target.tag("mass").unwrap(), 0.8)
            .expect("selections share a Canvas");
        canvas.play(vec![copy]);

        let world = World::new();
        let mut queue = CommandQueue::default();
        let mut commands = Commands::new(&mut queue, &world);
        let mut timeline = Timeline::new();
        let fonts = gaanim_text::font::FontRegistry::new();
        let text_config = gaanim_text::prelude::TextConfig::default();
        canvas.compile_into(&mut commands, &mut timeline, &fonts, &text_config);

        assert!(timeline.clips.values().any(|clip| {
            matches!(
                &clip.payload,
                gaanim_timeline::clip::ClipPayload::Animation(
                    gaanim_timeline::clip::AnimationSpec {
                        lens: gaanim_timeline::clip::PropertyLensSpec::Translation { from, to },
                        ..
                    }
                ) if from.y > to.y
            )
        }));
    }

    #[test]
    fn equation_expansion_morphs_semantic_terms_without_cross_fading_pairs() {
        let mut canvas = Canvas::new(640, 360);
        let source = canvas.math_text("E = m c^2").define_tag("mass", "m", None);
        let target =
            canvas
                .math_text("E = (m_1 + m_2) c^2")
                .define_tag("mass", "(m_1 + m_2)", None);
        let expansion = source.expand_to(&target, "mass", 0.8).unwrap();
        canvas.play(vec![expansion]);

        let world = World::new();
        let mut queue = CommandQueue::default();
        let mut commands = Commands::new(&mut queue, &world);
        let mut timeline = Timeline::new();
        let fonts = gaanim_text::font::FontRegistry::new();
        let text_config = gaanim_text::prelude::TextConfig::default();
        canvas.compile_into(&mut commands, &mut timeline, &fonts, &text_config);

        let semantic_morphs = timeline
            .clips
            .values()
            .filter(|clip| {
                matches!(
                    &clip.payload,
                    gaanim_timeline::clip::ClipPayload::Animation(
                        gaanim_timeline::clip::AnimationSpec {
                            lens: gaanim_timeline::clip::PropertyLensSpec::PathMorph { .. },
                            label: Some(label),
                            ..
                        }
                    ) if label == "EquationSemanticMorph"
                )
            })
            .count();
        assert!(semantic_morphs >= 3, "shared and tagged terms should morph");
        assert!(timeline.clips.values().all(|clip| {
            !matches!(
                &clip.payload,
                gaanim_timeline::clip::ClipPayload::Animation(
                    gaanim_timeline::clip::AnimationSpec {
                        lens: gaanim_timeline::clip::PropertyLensSpec::Opacity { .. },
                        label: Some(label),
                        ..
                    }
                ) if clip.duration > 0.0 && label == "EquationSemanticMorph"
            )
        }));
        assert!(timeline.clips.values().any(|clip| {
            matches!(
                &clip.payload,
                gaanim_timeline::clip::ClipPayload::Animation(
                    gaanim_timeline::clip::AnimationSpec {
                        lens: gaanim_timeline::clip::PropertyLensSpec::Scale { from, to },
                        label: Some(label),
                        ..
                    }
                ) if label == "EquationEmerge"
                    && (clip.start - 0.16).abs() < 1e-9
                    && *from == gaanim_core::glam::DVec3::ZERO
                    && *to != gaanim_core::glam::DVec3::ZERO
            )
        }));
        assert!(timeline.clips.values().all(|clip| {
            !matches!(
                &clip.payload,
                gaanim_timeline::clip::ClipPayload::Animation(
                    gaanim_timeline::clip::AnimationSpec {
                        lens: gaanim_timeline::clip::PropertyLensSpec::Opacity { .. },
                        label: Some(label),
                        ..
                    }
                ) if label == "EquationEmerge" && clip.duration > 0.0
            )
        }));
    }

    #[test]
    fn equation_step_prioritizes_semantic_tags_then_matches_common_glyphs() {
        let mut canvas = Canvas::new(640, 360);
        let source = canvas
            .math_text("x + 3 = 7")
            .define_tag("result", "7", None);
        let target = canvas.math_text("x = 4").define_tag("result", "4", None);
        let step = source
            .step_to(
                &target,
                Some(vec![("result".to_string(), "result".to_string())]),
                0.8,
            )
            .unwrap();
        canvas.play(vec![step]);

        let world = World::new();
        let mut queue = CommandQueue::default();
        let mut commands = Commands::new(&mut queue, &world);
        let mut timeline = Timeline::new();
        let fonts = gaanim_text::font::FontRegistry::new();
        let text_config = gaanim_text::prelude::TextConfig::default();
        canvas.compile_into(&mut commands, &mut timeline, &fonts, &text_config);

        let semantic_morphs = timeline
            .clips
            .values()
            .filter(|clip| {
                matches!(
                    &clip.payload,
                    gaanim_timeline::clip::ClipPayload::Animation(
                        gaanim_timeline::clip::AnimationSpec {
                            lens: gaanim_timeline::clip::PropertyLensSpec::PathMorph { .. },
                            label: Some(label),
                            ..
                        }
                    ) if label == "EquationSemanticMorph"
                )
            })
            .count();
        assert!(
            semantic_morphs >= 3,
            "x and = should auto-match while the result tag forces 7 -> 4"
        );
        let handoffs = timeline
            .clips
            .values()
            .filter(|clip| {
                matches!(
                    &clip.payload,
                    gaanim_timeline::clip::ClipPayload::Animation(
                        gaanim_timeline::clip::AnimationSpec {
                            lens: gaanim_timeline::clip::PropertyLensSpec::Opacity { .. },
                            label: Some(label),
                            ..
                        }
                    ) if label == "EquationHandoff" && clip.duration == 0.0
                )
            })
            .count();
        assert!(handoffs >= semantic_morphs * 2);
        assert!(timeline.clips.values().any(|clip| {
            matches!(
                &clip.payload,
                gaanim_timeline::clip::ClipPayload::Animation(
                    gaanim_timeline::clip::AnimationSpec {
                        lens: gaanim_timeline::clip::PropertyLensSpec::Scale { to, .. },
                        label: Some(label),
                        ..
                    }
                ) if label == "EquationCollapse"
                    && *to == gaanim_core::glam::DVec3::ZERO
            )
        }));
        assert!(timeline.clips.values().all(|clip| {
            !matches!(
                &clip.payload,
                gaanim_timeline::clip::ClipPayload::Animation(
                    gaanim_timeline::clip::AnimationSpec {
                        lens: gaanim_timeline::clip::PropertyLensSpec::Opacity { .. },
                        label: Some(label),
                        ..
                    }
                ) if label == "EquationCollapse" && clip.duration > 0.0
            )
        }));
    }

    #[test]
    fn equation_step_handoff_is_exact_and_target_remains_animatable() {
        let mut canvas = Canvas::new(640, 360);
        let equation = |middle: &str, result: &str| {
            StructuredTextSpec::new(
                vec![
                    "$".into(),
                    gaanim_text::prelude::TextPart::new(
                        "variable",
                        vec!["x".into()],
                        StructuredTextStyle::default(),
                    )
                    .into(),
                    middle.into(),
                    gaanim_text::prelude::TextPart::new(
                        "result",
                        vec![result.into()],
                        StructuredTextStyle::default(),
                    )
                    .into(),
                    "$".into(),
                ],
                None,
                StructuredTextStyle::default(),
                gaanim_text::prelude::TextFlow::default(),
            )
            .expect("valid structured equation")
        };
        let source = canvas.text_spec(equation(" dot 5 = ", "25")).scaled(2.0);
        let target = canvas.text_spec(equation(" = ", "5")).scaled(2.0);
        let step = source.step_to(&target, None, 0.8).unwrap();
        canvas.play(vec![step]);
        canvas.play(vec![
            target.tag("result").expect("result tag").indicate(0.4),
        ]);

        let world = World::new();
        let mut queue = CommandQueue::default();
        let mut commands = Commands::new(&mut queue, &world);
        let mut timeline = Timeline::new();
        let fonts = gaanim_text::font::FontRegistry::new();
        let text_config = gaanim_text::prelude::TextConfig::default();
        canvas.compile_into(&mut commands, &mut timeline, &fonts, &text_config);
        drop(commands);

        let mut world = world;
        queue.apply(&mut world);
        timeline.add_keyframe(0.0, WorldSnapshot::capture(&mut world));
        timeline.seek(&mut world, 0.8);

        let child_opacities: Vec<Vec<f32>> = world
            .query::<(
                Option<&bevy::prelude::ChildOf>,
                Option<&bevy::prelude::Children>,
            )>()
            .iter(&world)
            .filter_map(|(parent, children)| {
                (parent.is_none()).then_some(children?).map(|children| {
                    children
                        .iter()
                        .filter_map(|child| world.get::<Opacity>(child).map(|opacity| opacity.0))
                        .collect()
                })
            })
            .collect();
        assert_eq!(child_opacities.len(), 2);
        assert!(
            child_opacities
                .iter()
                .any(|values| values.iter().all(|value| *value == 0.0))
        );
        assert!(
            child_opacities
                .iter()
                .any(|values| values.iter().all(|value| *value > 0.99))
        );

        timeline.seek(&mut world, 1.0);
        assert!(
            world
                .query::<&Opacity>()
                .iter(&world)
                .filter(|opacity| opacity.0 > 0.99)
                .count()
                >= 3
        );

        timeline.seek(&mut world, 1.2);
        let child_opacities: Vec<Vec<f32>> = world
            .query::<(
                Option<&bevy::prelude::ChildOf>,
                Option<&bevy::prelude::Children>,
            )>()
            .iter(&world)
            .filter_map(|(parent, children)| {
                (parent.is_none()).then_some(children?).map(|children| {
                    children
                        .iter()
                        .filter_map(|child| world.get::<Opacity>(child).map(|opacity| opacity.0))
                        .collect()
                })
            })
            .collect();
        assert!(
            child_opacities
                .iter()
                .any(|values| !values.is_empty() && values.iter().all(|value| *value > 0.99)),
            "all target glyphs must remain visible after animating one selection: {child_opacities:?}"
        );
    }

    #[test]
    fn equation_part_colors_survive_morph_handoff_and_seek() {
        let red = PenikoColor::from_rgb8(0xef, 0x44, 0x44);
        let equation = |term: &str| {
            StructuredTextSpec::new(
                vec![
                    "$".into(),
                    gaanim_text::prelude::TextPart::new(
                        "term",
                        vec![term.into()],
                        StructuredTextStyle::default(),
                    )
                    .into(),
                    " + 1 = 2$".into(),
                ],
                None,
                StructuredTextStyle::default(),
                gaanim_text::prelude::TextFlow::default(),
            )
            .expect("valid structured equation")
        };
        let mut canvas = Canvas::new(640, 360);
        let source = canvas.text_spec(equation("x"));
        let target = canvas.text_spec(equation("theta''"));
        assert!(target.fill_text_part(&["term".to_owned()], red));
        let target_spec = target.text_spec().expect("target text spec");
        let target_source = structured_text_typst_source(
            &target_spec,
            Some(640.0),
            48.0,
            "New Computer Modern",
            PenikoColor::BLACK,
        );
        assert!(target_source.contains("ef4444"), "{target_source}");
        canvas.play(vec![source.morph_to(&target, 0.8).unwrap()]);

        let world = World::new();
        let mut queue = CommandQueue::default();
        let mut commands = Commands::new(&mut queue, &world);
        let mut timeline = Timeline::new();
        let fonts = gaanim_text::font::FontRegistry::new();
        let text_config = gaanim_text::prelude::TextConfig::default();
        canvas.compile_into(&mut commands, &mut timeline, &fonts, &text_config);
        drop(commands);

        let mut world = world;
        queue.apply(&mut world);
        assert!(
            world
                .query::<&gaanim_scene::FillBrush>()
                .iter(&world)
                .any(|fill| matches!(fill.0.as_ref(), Some(gaanim_core::peniko::Brush::Solid(color)) if *color == red)),
            "the semantic target must compile with its authored part fill"
        );
        timeline.add_keyframe(0.0, WorldSnapshot::capture(&mut world));
        timeline.seek(&mut world, 0.8);

        let red_states = world
            .query::<(
                bevy::prelude::Entity,
                &Opacity,
                &gaanim_scene::FillBrush,
                Option<&bevy::prelude::ChildOf>,
                Option<&gaanim_scene::ObjectTag>,
            )>()
            .iter(&world)
            .filter_map(|(entity, opacity, fill, parent, tag)| {
                matches!(fill.0.as_ref(), Some(gaanim_core::peniko::Brush::Solid(color)) if *color == red)
                    .then_some((entity, opacity.0, parent.map(|parent| parent.parent()), tag.map(|tag| tag.0.clone())))
            })
            .collect::<Vec<_>>();

        assert!(
            world
                .query::<(&Opacity, &gaanim_scene::FillBrush)>()
                .iter(&world)
                .any(|(opacity, fill)| {
                    opacity.0 > 0.99
                        && matches!(
                            fill.0.as_ref(),
                            Some(gaanim_core::peniko::Brush::Solid(color)) if *color == red
                        )
                }),
            "the visible target term must retain its selected fill after the handoff: {red_states:?}"
        );
    }

    #[test]
    fn text_step_does_not_mutate_unrelated_written_text() {
        let mut canvas = Canvas::new(640, 360);
        let title = canvas
            .text_spec(
                StructuredTextSpec::new(
                    vec!["Resolver paso a paso".into()],
                    Some(gaanim_text::prelude::TextRole::Title),
                    StructuredTextStyle::default(),
                    gaanim_text::prelude::TextFlow::default(),
                )
                .expect("valid title"),
            )
            .at(0.0, 120.0);
        let equation = |middle: &str, result: &str| {
            StructuredTextSpec::new(
                vec![
                    "$".into(),
                    gaanim_text::prelude::TextPart::new(
                        "variable",
                        vec!["x".into()],
                        StructuredTextStyle::default(),
                    )
                    .into(),
                    middle.into(),
                    gaanim_text::prelude::TextPart::new(
                        "result",
                        vec![result.into()],
                        StructuredTextStyle::default(),
                    )
                    .into(),
                    "$".into(),
                ],
                None,
                StructuredTextStyle::default(),
                gaanim_text::prelude::TextFlow::default(),
            )
            .expect("valid structured equation")
        };
        let source = canvas.text_spec(equation(" dot 5 = ", "25")).scaled(2.0);
        let target = canvas.text_spec(equation(" = ", "5")).scaled(2.0);
        canvas.play(vec![title.write(1.0), source.write(1.0)]);
        canvas.wait(0.4);
        canvas.play(vec![source.step_to(&target, None, 0.8).unwrap()]);
        canvas.play(vec![
            target.tag("result").expect("result tag").indicate(0.45),
        ]);

        let world = World::new();
        let mut queue = CommandQueue::default();
        let mut commands = Commands::new(&mut queue, &world);
        let mut timeline = Timeline::new();
        let fonts = gaanim_text::font::FontRegistry::new();
        let text_config = gaanim_text::prelude::TextConfig::default();
        canvas.compile_into(&mut commands, &mut timeline, &fonts, &text_config);
        drop(commands);

        let mut world = world;
        queue.apply(&mut world);
        let title_children = world
            .query::<(
                &MobjectId,
                &ObjectTag,
                Option<&bevy::prelude::ChildOf>,
                Option<&bevy::prelude::Children>,
            )>()
            .iter(&world)
            .find_map(|(_, tag, parent, children)| {
                (parent.is_none() && tag.0.contains("Resolver paso a paso"))
                    .then_some(children?.iter().collect::<Vec<_>>())
            })
            .expect("compiled title root");
        timeline.add_keyframe(0.0, WorldSnapshot::capture(&mut world));
        timeline.seek(&mut world, 1.4);
        let title_paths = title_children
            .iter()
            .map(|child| {
                world
                    .get::<gaanim_scene::Path2D>(*child)
                    .expect("title glyph path")
                    .clone()
            })
            .collect::<Vec<_>>();
        for time in [1.8, 2.2] {
            timeline.seek(&mut world, time);
            assert!(
                title_children.iter().enumerate().all(|(index, child)| {
                    world
                        .get::<Opacity>(*child)
                        .is_some_and(|opacity| opacity.0 > 0.99)
                        && world
                            .get::<gaanim_animation::writing::FillDrawProgress>(*child)
                            .is_none_or(|progress| progress.0 > 0.99)
                        && world.get::<gaanim_scene::Path2D>(*child) == title_paths.get(index)
                }),
                "title glyphs must remain fully drawn during the equation transition at {time}s"
            );
        }
    }

    #[test]
    fn tagged_equation_copy_keeps_opacity_changes_instantaneous() {
        let mut canvas = Canvas::new(640, 360);
        let source = canvas
            .math_text("E = m c^2")
            .define_tag("mass", "m", None)
            .at(0.0, 70.0);
        let target = canvas
            .math_text("p = m v")
            .define_tag("mass", "m", None)
            .at(0.0, -90.0);
        let copy = source
            .tag("mass")
            .unwrap()
            .copy_to(&target.tag("mass").unwrap(), 0.8)
            .expect("selections share a Canvas");
        canvas.play(vec![copy]);

        let world = World::new();
        let mut queue = CommandQueue::default();
        let mut commands = Commands::new(&mut queue, &world);
        let mut timeline = Timeline::new();
        let fonts = gaanim_text::font::FontRegistry::new();
        let text_config = gaanim_text::prelude::TextConfig::default();
        canvas.compile_into(&mut commands, &mut timeline, &fonts, &text_config);

        assert!(timeline.clips.values().any(|clip| {
            matches!(
                &clip.payload,
                gaanim_timeline::clip::ClipPayload::Animation(
                    gaanim_timeline::clip::AnimationSpec {
                        lens: gaanim_timeline::clip::PropertyLensSpec::PathMorph { .. },
                        label: Some(label),
                        ..
                    }
                ) if label == "EquationSemanticMorph"
            )
        }));
        assert!(timeline.clips.values().all(|clip| {
            !matches!(
                &clip.payload,
                gaanim_timeline::clip::ClipPayload::Animation(
                    gaanim_timeline::clip::AnimationSpec {
                        lens: gaanim_timeline::clip::PropertyLensSpec::Opacity { .. },
                        ..
                    }
                ) if clip.duration > 0.0
            )
        }));
    }

    #[test]
    fn tagged_equation_copy_preserves_both_visible_equations_after_seek() {
        let mut canvas = Canvas::new(640, 360);
        let title = canvas.text("One variable");
        let source = canvas
            .math_text("E = m c^2")
            .define_tag("mass", "m", None)
            .at(0.0, 70.0);
        let target = canvas
            .math_text("p = m v")
            .define_tag("mass", "m", None)
            .at(0.0, -90.0);
        canvas.play(vec![
            title.write(1.0),
            source.write(1.0),
            target.fade_in(1.0),
        ]);
        canvas.wait(0.5);
        let copy = source
            .tag("mass")
            .unwrap()
            .copy_to(&target.tag("mass").unwrap(), 0.9)
            .expect("selections share a Canvas");
        canvas.play(vec![copy]);
        canvas.wait(0.25);

        let world = World::new();
        let mut queue = CommandQueue::default();
        let mut commands = Commands::new(&mut queue, &world);
        let mut timeline = Timeline::new();
        let fonts = gaanim_text::font::FontRegistry::new();
        let text_config = gaanim_text::prelude::TextConfig::default();
        canvas.compile_into(&mut commands, &mut timeline, &fonts, &text_config);
        drop(commands);

        let mut world = world;
        queue.apply(&mut world);
        timeline.add_keyframe(0.0, WorldSnapshot::capture(&mut world));
        timeline.seek(&mut world, 2.55);

        let textual_roots: Vec<_> = world
            .query::<(
                bevy::prelude::Entity,
                Option<&bevy::prelude::ChildOf>,
                Option<&bevy::prelude::Children>,
            )>()
            .iter(&world)
            .filter_map(|(entity, parent, children)| {
                (parent.is_none() && children.is_some()).then_some(entity)
            })
            .collect();
        assert_eq!(textual_roots.len(), 3);
        for root in textual_roots {
            let children = world.get::<bevy::prelude::Children>(root).unwrap();
            assert!(children.iter().all(|child| {
                world
                    .get::<Opacity>(child)
                    .is_some_and(|opacity| opacity.0 > 0.99)
            }));
        }
    }

    #[test]
    fn fade_in_from_down_schedules_opacity_and_upward_translation() {
        let mut canvas = Canvas::new(640, 360);
        let label = canvas.text("Aparece desde abajo");
        label.fade_in_from(crate::canvas::Direction::Down, 72.0, 0.8);

        let world = World::new();
        let mut queue = CommandQueue::default();
        let mut commands = Commands::new(&mut queue, &world);
        let mut timeline = Timeline::new();
        let fonts = gaanim_text::font::FontRegistry::new();
        let text_config = gaanim_text::prelude::TextConfig::default();
        canvas.compile_into(&mut commands, &mut timeline, &fonts, &text_config);

        assert!(timeline.clips.values().any(|clip| {
            matches!(
                &clip.payload,
                gaanim_timeline::clip::ClipPayload::Animation(
                    gaanim_timeline::clip::AnimationSpec {
                        lens: gaanim_timeline::clip::PropertyLensSpec::Opacity { from, to },
                        ..
                    }
                ) if *from == 0.0 && *to == 1.0
            )
        }));
        assert!(timeline.clips.values().any(|clip| {
            matches!(
                &clip.payload,
                gaanim_timeline::clip::ClipPayload::Animation(
                    gaanim_timeline::clip::AnimationSpec {
                        lens: gaanim_timeline::clip::PropertyLensSpec::Translation { from, to },
                        ..
                    }
                ) if from.y < to.y
            )
        }));
    }

    #[test]
    fn text_with_inline_math_compiles_to_vector_glyphs() {
        let mut canvas = Canvas::new(640, 360);
        canvas.text("Energia $E = m c^2$ es famosa");
        canvas.text("Valor $1/2$ y $sqrt(2)$");
        canvas.text("Angulo $alpha + beta$");
        let world = World::new();
        let mut queue = CommandQueue::default();
        let mut commands = Commands::new(&mut queue, &world);
        let mut timeline = Timeline::new();
        let fonts = gaanim_text::font::FontRegistry::new();
        let text_config = gaanim_text::prelude::TextConfig::default();
        canvas.compile_into(&mut commands, &mut timeline, &fonts, &text_config);
        drop(commands);
        let mut world = world;
        queue.apply(&mut world);
        let fills: Vec<_> = world
            .query::<&gaanim_scene::FillBrush>()
            .iter(&world)
            .filter_map(|f| f.0.clone())
            .collect();
        assert!(
            fills.len() > 10,
            "text with inline math should produce vector glyphs, got {}",
            fills.len()
        );
        assert!(
            fills.iter().all(|b| match b {
                gaanim_core::peniko::Brush::Solid(c) => *c != gaanim_core::peniko::Color::BLACK,
                _ => true,
            }),
            "inline math glyphs should not be black on black background"
        );
        assert!(
            world
                .query::<&LocalBounds>()
                .iter(&world)
                .any(|b| b.0.width() > 50.0)
        );
    }

    #[test]
    fn paragraph_with_inline_math_compiles_to_vector_glyphs() {
        let mut canvas = Canvas::new(640, 360);
        canvas.configured_text(
            "La energia $E = m c^2$ relaciona masa y energia en $x^2 + y^2 = z^2$.",
            gaanim_text::prelude::TextStyle {
                size: Some(28.0),
                ..Default::default()
            },
            gaanim_text::prelude::TextFlow {
                wrap: gaanim_text::prelude::TextWrap::Width(500.0),
                ..Default::default()
            },
        );
        let world = World::new();
        let mut queue = CommandQueue::default();
        let mut commands = Commands::new(&mut queue, &world);
        let mut timeline = Timeline::new();
        let fonts = gaanim_text::font::FontRegistry::new();
        let text_config = gaanim_text::prelude::TextConfig::default();
        canvas.compile_into(&mut commands, &mut timeline, &fonts, &text_config);
        drop(commands);
        let mut world = world;
        queue.apply(&mut world);
        let fills: Vec<_> = world
            .query::<&gaanim_scene::FillBrush>()
            .iter(&world)
            .filter_map(|f| f.0.clone())
            .collect();
        assert!(
            fills.len() > 15,
            "paragraph with inline math should produce vector glyphs, got {}",
            fills.len()
        );
        assert!(
            fills.iter().all(|b| match b {
                gaanim_core::peniko::Brush::Solid(c) => *c != gaanim_core::peniko::Color::BLACK,
                _ => true,
            }),
            "inline math paragraph glyphs should not be black on black background"
        );
    }

    #[test]
    fn compiled_text_measure_rewraps_at_the_offered_width() {
        let id = gaanim_layout::LayoutId(7);
        let fonts = gaanim_text::font::FontRegistry::new();
        let measurer = CompiledLayoutMeasure {
            fixed: BTreeMap::new(),
            texts: BTreeMap::from([(
                id,
                CompiledTextMeasure {
                    spec: StructuredTextSpec::new(
                        vec!["Responsive text measures its true wrapped height before any ECS entity is materialized.".into()],
                        None,
                        gaanim_text::prelude::TextStyle::default(),
                        gaanim_text::prelude::TextFlow::default(),
                    ).unwrap(),
                    font_size: 28.0,
                    font_family: "New Computer Modern".into(),
                    math_font: "New Computer Modern Math".into(),
                    color: gaanim_core::peniko::Color::WHITE,
                },
            )]),
            text_composition_widths: RefCell::default(),
            font_registry: &fonts,
        };
        let narrow = gaanim_layout::IntrinsicMeasure::measure(
            &measurer,
            id,
            gaanim_layout::BoxConstraints {
                min: DVec2::ZERO,
                max: DVec2::new(180.0, 1000.0),
            },
        )
        .unwrap();
        let wide = gaanim_layout::IntrinsicMeasure::measure(
            &measurer,
            id,
            gaanim_layout::BoxConstraints {
                min: DVec2::ZERO,
                max: DVec2::new(520.0, 1000.0),
            },
        )
        .unwrap();

        assert!(narrow.y > wide.y, "narrow={narrow:?}, wide={wide:?}");
        assert!(gaanim_layout::IntrinsicMeasure::is_width_sensitive(
            &measurer, id
        ));
    }

    #[test]
    fn auto_text_measure_preserves_the_width_used_for_composition() {
        let fonts = gaanim_text::font::FontRegistry::new();
        let cases = [
            ("Titulo de presentacion", 64.0),
            ("Hola mundo", 48.0),
            ("Texto Normal", 32.0),
            ("$integral alpha d t + 2 = 0$", 32.0),
        ];

        for (index, (content, font_size)) in cases.into_iter().enumerate() {
            let id = gaanim_layout::LayoutId(index as u64 + 1);
            let measurer = CompiledLayoutMeasure {
                fixed: BTreeMap::new(),
                texts: BTreeMap::from([(
                    id,
                    CompiledTextMeasure {
                        spec: StructuredTextSpec::new(
                            vec![content.into()],
                            None,
                            gaanim_text::prelude::TextStyle::default(),
                            gaanim_text::prelude::TextFlow::default(),
                        )
                        .unwrap(),
                        font_size,
                        font_family: "New Computer Modern".into(),
                        math_font: "New Computer Modern Math".into(),
                        color: gaanim_core::peniko::Color::WHITE,
                    },
                )]),
                text_composition_widths: RefCell::default(),
                font_registry: &fonts,
            };
            let wide = gaanim_layout::IntrinsicMeasure::measure(
                &measurer,
                id,
                gaanim_layout::BoxConstraints {
                    min: DVec2::ZERO,
                    max: DVec2::new(1760.0, 1000.0),
                },
            )
            .unwrap();
            assert!(wide.x < 1760.0, "fixture should have tight visual bounds");
            assert_eq!(
                measurer.text_composition_widths.borrow().get(&id),
                Some(&1760.0),
                "{content:?} must be materialized at the width used to measure it"
            );
        }
    }

    #[test]
    fn nested_layout_materializes_paragraph_at_the_outer_assigned_width() {
        let mut canvas = Canvas::new(1280, 720);
        let paragraph = canvas.text(
            "Nested responsive paragraphs must use the card width instead of the safe frame width.",
        );
        let inner = canvas.group(&[&paragraph]);
        paragraph.claim_layout(&inner).unwrap();
        canvas.reflow_layout(
            &inner,
            vec![crate::canvas::LayoutMemberSpec {
                id: paragraph.id,
                style: gaanim_layout::LayoutItemStyle::default(),
            }],
            crate::canvas::LayoutSpec {
                kind: gaanim_layout::LayoutNodeKind::Column { wrap: false },
                style: gaanim_layout::LayoutStyle {
                    width: gaanim_layout::SizeRule::Fill(1.0),
                    height: gaanim_layout::SizeRule::Fill(1.0),
                    padding: gaanim_layout::Insets::all(28.0),
                    align: gaanim_layout::Align::Stretch,
                    ..Default::default()
                },
                within: LayoutWithin::Intrinsic,
            },
            1,
            None,
            None,
            None,
        );

        let outer = canvas.group(&[&inner]);
        inner.claim_layout(&outer).unwrap();
        canvas.reflow_layout(
            &outer,
            vec![crate::canvas::LayoutMemberSpec {
                id: inner.id,
                style: gaanim_layout::LayoutItemStyle::default(),
            }],
            crate::canvas::LayoutSpec {
                kind: gaanim_layout::LayoutNodeKind::Stack,
                style: gaanim_layout::LayoutStyle {
                    width: gaanim_layout::SizeRule::Fixed(340.0),
                    height: gaanim_layout::SizeRule::Fixed(220.0),
                    align: gaanim_layout::Align::Stretch,
                    ..Default::default()
                },
                within: LayoutWithin::Safe,
            },
            1,
            None,
            None,
            None,
        );

        let world = World::new();
        let mut queue = CommandQueue::default();
        let mut commands = Commands::new(&mut queue, &world);
        let mut timeline = Timeline::new();
        let fonts = gaanim_text::font::FontRegistry::new();
        let text_config = gaanim_text::prelude::TextConfig::default();
        canvas.compile_into(&mut commands, &mut timeline, &fonts, &text_config);
        drop(commands);

        let mut world = world;
        queue.apply(&mut world);
        timeline.add_keyframe(0.0, WorldSnapshot::capture(&mut world));
        timeline.seek(&mut world, 0.0);
        let visible_bounds: Vec<_> = world
            .query::<(&LocalBounds, &Opacity)>()
            .iter(&world)
            .filter(|(_, opacity)| opacity.0 > 0.99)
            .map(|(bounds, _)| bounds.0)
            .collect();
        assert!(
            visible_bounds.iter().any(|bounds| bounds.width() > 240.0
                && bounds.width() < 300.0
                && bounds.height() > 60.0),
            "expected a wrapped paragraph inside the 284-unit content box, got {visible_bounds:?}"
        );
        assert!(
            visible_bounds
                .iter()
                .all(|bounds| bounds.width() <= 340.0 + 1.0e-6),
            "safe-frame paragraph leaked into the nested layout: {visible_bounds:?}"
        );
    }

    #[test]
    fn inline_math_helpers_handle_escapes_and_doubles() {
        assert_eq!(
            typst_inline_content("a $x$ b"),
            "#text(\"a \")$x$#text(\" b\")"
        );
        assert_eq!(typst_inline_content("sin math"), "#text(\"sin math\")");
        assert_eq!(typst_inline_content("$E=mc^2$"), "$E=mc^2$");
        assert_eq!(
            typst_inline_content("Escapado \\$ literal $x$"),
            "#text(\"Escapado $ literal \")$x$"
        );
        assert_eq!(split_text_math("a $x$ b $y$ c").len(), 5);
        let spec = StructuredTextSpec::new(
            vec!["Hola $x^2$ mundo".into()],
            None,
            gaanim_text::prelude::TextStyle::default(),
            gaanim_text::prelude::TextFlow {
                wrap: gaanim_text::prelude::TextWrap::Width(400.0),
                ..Default::default()
            },
        )
        .unwrap();
        let para = structured_text_typst_source(
            &spec,
            Some(400.0),
            32.0,
            "New Computer Modern",
            gaanim_core::peniko::Color::WHITE,
        );
        assert!(
            para.contains("$x^2$"),
            "paragraph source should embed math, got {para}"
        );
        assert!(para.contains("#text(\"Hola \")"));
        let txt = text_inline_typst_source("prueba $x^2$ fin", gaanim_core::peniko::Color::WHITE);
        assert!(txt.contains("$x^2$"));
        let txt2 =
            text_inline_typst_source("$alpha + beta = 1$", gaanim_core::peniko::Color::WHITE);
        assert!(txt2.contains("$alpha + beta = 1$"));
    }

    #[test]
    fn layout_reflow_animates_displaced_members_and_fades_the_insertion() {
        let mut canvas = Canvas::new(640, 360);
        let first = canvas.rect(80.0, 30.0);
        let second = canvas.rect(80.0, 30.0);
        let container = canvas.group(&[&first]);
        let spec = crate::canvas::LayoutSpec {
            kind: gaanim_layout::LayoutNodeKind::Column { wrap: false },
            style: gaanim_layout::LayoutStyle {
                gap: DVec2::splat(20.0),
                align: gaanim_layout::Align::Center,
                ..Default::default()
            },
            within: LayoutWithin::Intrinsic,
        };
        canvas.reflow_layout(
            &container,
            vec![crate::canvas::LayoutMemberSpec {
                id: first.id,
                style: gaanim_layout::LayoutItemStyle::default(),
            }],
            spec.clone(),
            1,
            None,
            None,
            None,
        );
        canvas.segment("layout-update", None).unwrap();
        canvas.set_group_members(&container, &[&first, &second]);
        canvas.reflow_layout(
            &container,
            vec![
                crate::canvas::LayoutMemberSpec {
                    id: first.id,
                    style: gaanim_layout::LayoutItemStyle::default(),
                },
                crate::canvas::LayoutMemberSpec {
                    id: second.id,
                    style: gaanim_layout::LayoutItemStyle::default(),
                },
            ],
            spec,
            2,
            Some(0.5),
            Some(&second),
            None,
        );

        let world = World::new();
        let mut queue = CommandQueue::default();
        let mut commands = Commands::new(&mut queue, &world);
        let mut timeline = Timeline::new();
        let fonts = gaanim_text::font::FontRegistry::new();
        let text_config = gaanim_text::prelude::TextConfig::default();
        canvas.compile_into(&mut commands, &mut timeline, &fonts, &text_config);

        assert!(timeline.clips.values().any(|clip| matches!(
            &clip.payload,
            gaanim_timeline::clip::ClipPayload::Animation(gaanim_timeline::clip::AnimationSpec {
                lens: gaanim_timeline::clip::PropertyLensSpec::Translation { .. },
                ..
            })
        )));
        assert!(timeline.clips.values().any(|clip| matches!(
            &clip.payload,
            gaanim_timeline::clip::ClipPayload::Animation(
                gaanim_timeline::clip::AnimationSpec {
                    lens: gaanim_timeline::clip::PropertyLensSpec::Opacity { from, to },
                    ..
                }
            ) if *from == 0.0 && *to == 1.0
        )));
    }
}
