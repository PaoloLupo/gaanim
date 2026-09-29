//! Compile — replay SceneModel ops into SceneBuilder/Bevy.

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
    FillBrush, GlobalOpacity, GroupMarker, LineListData, LocalBounds, MobjectId, ObjectTag,
    Opacity, RenderOrder, StrokeBrush, TriangleMeshData, Visible, WorldBounds,
};
use gaanim_timeline::clip::SceneId;
use gaanim_timeline::timeline::{SegmentMetadata, SegmentStop, Timeline};

use crate::anim::{AnimationBuilder, AnimationType};
use crate::builder::{
    EquationTransitionMode, MobjectRef, MobjectState, MobjectStateMap, SceneBuilder,
};
use crate::canvas::canvas_impl::SceneModel;
use crate::canvas::ops::{
    CanvasCameraBindingKind, CanvasEndpoint, CanvasRay, FragmentRevealStyle, Op, Segment,
};
use crate::canvas::types::{
    AxesConfig, LayoutOp, LayoutTreeSnapshot, LayoutWithin, ObjectSpec, SpawnKind,
};
use gaanim_text::prelude::{
    TextContent as StructuredTextContent, TextDirection as StructuredTextDirection,
    TextOverflow as StructuredTextOverflow, TextRevealUnit, TextSpec as StructuredTextSpec,
    TextStyle as StructuredTextStyle, TextWrap as StructuredTextWrap,
};

use gaanim_animation::{
    AngleLabelPlacement, CurvatureOnCurve, DimensionLabelPlacement, EndpointAngle,
    EndpointDistance, EndpointFollow, NormalOnCurve, PointOnCurve, PositionBinding,
    ReactiveFunction, ReactiveLineRegen, ReactiveMeshRegen, RotationBinding,
    RotationTranslationBinding, ScalarSource, TangentOnCurve, TracedPath, TrackingAngle,
    TrackingAnglePart, TrackingEndpoint, TrackingLine, TrackingRay, TrackingScalar,
    TrackingVectorHead, Updater,
};
use gaanim_math::{RateFunc, SpatialTransform};

/// Typst's default text size, in points.
const TYPST_DEFAULT_TEXT_PT: f64 = 11.0;

fn sampled_reactive_path(
    map: &gaanim_visualization::CoordinateMap2D,
    function: &ReactiveFunction,
    domain: (f64, f64),
    reveal: Option<&ScalarSource>,
    sampling: gaanim_visualization::Sampling,
    time: f64,
    values: &[(ObjectId, f64)],
) -> BezPath {
    let sampled_domain = if let Some(reveal) = reveal {
        let Ok(end) = reveal.evaluate(time, |logical| {
            values
                .iter()
                .find_map(|(id, value)| (*id == logical).then_some(*value))
        }) else {
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
    gaanim_visualization::sample_function(map, sampled_domain, sampling, |coordinate| {
        function
            .evaluate(&[coordinate], time, |logical| {
                values
                    .iter()
                    .find_map(|(id, value)| (*id == logical).then_some(*value))
            })
            .ok()
            .map(|value| value[0])
    })
    .map(|sampled| sampled.to_bez_path())
    .unwrap_or_default()
}

fn reactive_values(
    world: &World,
    parameters: &[(ObjectId, Entity)],
) -> (f64, Vec<(ObjectId, f64)>) {
    let time = world
        .get_resource::<gaanim_animation::PlaybackState>()
        .map_or(0.0, |state| state.current_time);
    let values = parameters
        .iter()
        .filter_map(|(logical, entity)| {
            world
                .get::<gaanim_animation::FloatSignal>(*entity)
                .map(|signal| (*logical, signal.value))
        })
        .collect();
    (time, values)
}

fn sampled_reactive_parametric_2d(
    map: &gaanim_visualization::CoordinateMap2D,
    function: &ReactiveFunction,
    domain: (f64, f64),
    sampling: gaanim_visualization::Sampling,
    time: f64,
    values: &[(ObjectId, f64)],
) -> BezPath {
    gaanim_visualization::sample_parametric(map, domain, sampling, |coordinate| {
        function
            .evaluate(&[coordinate], time, |logical| {
                values
                    .iter()
                    .find_map(|(id, value)| (*id == logical).then_some(*value))
            })
            .ok()
            .map(|output| (output[0], output[1]))
    })
    .map(|sampled| sampled.to_bez_path())
    .unwrap_or_default()
}

fn sampled_reactive_parametric_3d(
    map: &gaanim_visualization::CoordinateMap3D,
    function: &ReactiveFunction,
    domain: (f64, f64),
    samples: usize,
    time: f64,
    values: &[(ObjectId, f64)],
    color: PenikoColor,
) -> LineListData {
    let mut points = Vec::new();
    let mut previous = None;
    for index in 0..samples {
        let progress = index as f64 / (samples - 1) as f64;
        let coordinate = domain.0 + (domain.1 - domain.0) * progress;
        let current = function
            .evaluate(&[coordinate], time, |logical| {
                values
                    .iter()
                    .find_map(|(id, value)| (*id == logical).then_some(*value))
            })
            .ok()
            .and_then(|output| map.data_to_local([output[0], output[1], output[2]]).ok())
            .map(|point| [point[0] as f32, point[1] as f32, point[2] as f32]);
        if let (Some(from), Some(to)) = (previous, current) {
            points.extend_from_slice(&[from, to]);
        }
        previous = current;
    }
    LineListData {
        points,
        indices: None,
        strip: false,
        color,
        colors: None,
    }
}

fn sampled_reactive_surface_3d(
    map: &gaanim_visualization::CoordinateMap3D,
    function: &ReactiveFunction,
    resolution: [usize; 2],
    time: f64,
    values: &[(ObjectId, f64)],
) -> TriangleMeshData {
    let mesh = gaanim_visualization::sample_surface(map, resolution, |x, y| {
        function
            .evaluate(&[x, y], time, |logical| {
                values
                    .iter()
                    .find_map(|(id, value)| (*id == logical).then_some(*value))
            })
            .ok()
            .map(|output| output[0])
    })
    .unwrap_or(gaanim_visualization::SurfaceMesh {
        vertices: Vec::new(),
        indices: Vec::new(),
        values: Vec::new(),
    });
    let finite = mesh
        .values
        .iter()
        .copied()
        .filter(|value| value.is_finite())
        .collect::<Vec<_>>();
    let minimum = finite.iter().copied().reduce(f64::min).unwrap_or(0.0);
    let maximum = finite.iter().copied().reduce(f64::max).unwrap_or(1.0);
    let span = (maximum - minimum).max(f64::EPSILON);
    let colors = mesh
        .values
        .iter()
        .map(|value| {
            let t = if value.is_finite() {
                ((value - minimum) / span).clamp(0.0, 1.0)
            } else {
                0.0
            };
            [
                (0x20 as f64 + t * 0xD0 as f64) as f32 / 255.0,
                (0x60 as f64 + (1.0 - (2.0 * t - 1.0).abs()) * 0x90 as f64) as f32 / 255.0,
                (0xD0 as f64 - t * 0x90 as f64) as f32 / 255.0,
                if value.is_finite() { 1.0 } else { 0.0 },
            ]
        })
        .collect();
    TriangleMeshData {
        vertices: mesh.vertices,
        indices: mesh.indices,
        normals: None,
        uvs: None,
        color: None,
        colors: Some(colors),
        material: None,
    }
}

fn compile_tracking_scalar(
    source: &ScalarSource,
    id_map: &HashMap<ObjectId, ObjectId>,
    states: &MobjectStateMap,
) -> TrackingScalar {
    let parameters = source
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
        source: source.clone(),
        parameters,
    }
}

fn compile_function_parameters(
    function: &ReactiveFunction,
    id_map: &HashMap<ObjectId, ObjectId>,
    states: &MobjectStateMap,
) -> Vec<(ObjectId, Entity)> {
    let mut ids = function.parameter_ids();
    ids.sort_unstable();
    ids.dedup();
    ids.into_iter()
        .filter_map(|logical| {
            let actual = id_map.get(&logical).copied()?;
            states.get(actual).map(|state| (logical, state.entity))
        })
        .collect()
}

fn runtime_field_2d(
    function: &ReactiveFunction,
    parameters: &[(ObjectId, Entity)],
    world: &World,
    point: [f64; 2],
) -> Option<[f64; 2]> {
    let time = world
        .get_resource::<gaanim_animation::PlaybackState>()
        .map_or(0.0, |state| state.current_time);
    let value = function
        .evaluate(&point, time, |logical| {
            parameters
                .iter()
                .find_map(|(id, entity)| (*id == logical).then_some(*entity))
                .and_then(|entity| world.get::<gaanim_animation::FloatSignal>(entity))
                .map(|signal| signal.value)
        })
        .ok()?;
    Some([value[0], value[1]])
}

fn reactive_field_color(
    value: f64,
    range: (f64, f64),
    color: Option<PenikoColor>,
    colormap: Option<&gaanim_core::ColorMap>,
    opacity: f64,
) -> PenikoColor {
    let base = color.unwrap_or_else(|| {
        let position = ((value - range.0) / (range.1 - range.0)).clamp(0.0, 1.0);
        colormap
            .and_then(|map| map.sample(position).ok())
            .unwrap_or(PenikoColor::WHITE)
    });
    let rgba = base.to_rgba8();
    PenikoColor::from_rgba8(
        rgba.r,
        rgba.g,
        rgba.b,
        (f64::from(rgba.a) * opacity.clamp(0.0, 1.0)).round() as u8,
    )
}

fn reactive_arrow_path_2d(
    map: &gaanim_visualization::CoordinateMap2D,
    position: [f64; 2],
    vector: [f64; 2],
    options: &crate::canvas::ArrowFieldOptions,
) -> Option<BezPath> {
    let start = map.data_to_local(position[0], position[1]).ok()?;
    let displaced = map
        .data_to_local(position[0] + vector[0], position[1] + vector[1])
        .ok()?;
    let delta = displaced - start;
    let raw_length = delta.hypot() * options.length_scale;
    if raw_length <= f64::EPSILON {
        return None;
    }
    let length = raw_length.clamp(options.min_length, options.max_length);
    let direction = delta / delta.hypot();
    let end = start + direction * length;
    let tip_length = options
        .tip_length
        .unwrap_or((length * 0.3).clamp(0.05, 0.12));
    let tip_width = options.tip_width.unwrap_or(tip_length * 0.8);
    let perpendicular = Vec2::new(-direction.y, direction.x);
    let shoulder = end - direction * tip_length;
    let mut path = BezPath::new();
    path.move_to(start);
    path.line_to(end);
    path.move_to(shoulder + perpendicular * (tip_width * 0.5));
    path.line_to(end);
    path.line_to(shoulder - perpendicular * (tip_width * 0.5));
    Some(path)
}

fn runtime_model_2d(
    function: ReactiveFunction,
    parameters: &[(ObjectId, Entity)],
    world: &World,
) -> Option<gaanim_visualization::VectorField<2>> {
    let values = parameters
        .iter()
        .map(|(logical, entity)| {
            world
                .get::<gaanim_animation::FloatSignal>(*entity)
                .map(|signal| (*logical, signal.value))
        })
        .collect::<Option<Vec<_>>>()?;
    let time = world
        .get_resource::<gaanim_animation::PlaybackState>()
        .map_or(0.0, |state| state.current_time);
    Some(gaanim_visualization::VectorField::new(move |point| {
        let result = function
            .evaluate(&point, time, |logical| {
                values
                    .iter()
                    .find_map(|(id, value)| (*id == logical).then_some(*value))
            })
            .ok()?;
        Some([result[0], result[1]])
    }))
}

fn runtime_model_3d(
    function: ReactiveFunction,
    parameters: &[(ObjectId, Entity)],
    world: &World,
) -> Option<gaanim_visualization::VectorField<3>> {
    let values = parameters
        .iter()
        .map(|(logical, entity)| {
            world
                .get::<gaanim_animation::FloatSignal>(*entity)
                .map(|signal| (*logical, signal.value))
        })
        .collect::<Option<Vec<_>>>()?;
    let time = world
        .get_resource::<gaanim_animation::PlaybackState>()
        .map_or(0.0, |state| state.current_time);
    Some(gaanim_visualization::VectorField::new(move |point| {
        let result = function
            .evaluate(&point, time, |logical| {
                values
                    .iter()
                    .find_map(|(id, value)| (*id == logical).then_some(*value))
            })
            .ok()?;
        Some([result[0], result[1], result[2]])
    }))
}

fn reactive_arrow_lines_3d(
    field: &gaanim_visualization::VectorField<3>,
    map: &gaanim_visualization::CoordinateMap3D,
    resolution: [usize; 3],
    options: &crate::canvas::ArrowFieldOptions,
    color_range: (f64, f64),
) -> Option<gaanim_scene::LineListData> {
    let domains = [map.x.domain(), map.y.domain(), map.z.domain()];
    let samples = field.sample_grid(domains, resolution).ok()?;
    let mut points = Vec::new();
    let mut colors = Vec::new();
    for sample in samples {
        let start = DVec3::from_array(map.data_to_local(sample.position).ok()?);
        let displaced = DVec3::from_array(
            map.data_to_local([
                sample.position[0] + sample.vector[0],
                sample.position[1] + sample.vector[1],
                sample.position[2] + sample.vector[2],
            ])
            .ok()?,
        );
        let delta = displaced - start;
        let raw_length = delta.length() * options.length_scale;
        if raw_length <= f64::EPSILON {
            continue;
        }
        let length = raw_length.clamp(options.min_length, options.max_length);
        let direction = delta.normalize();
        let end = start + direction * length;
        let tip_length = options
            .tip_length
            .unwrap_or((length * 0.3).clamp(0.04, 0.10));
        let tip_width = options.tip_width.unwrap_or(tip_length * 0.75);
        let reference = if direction.cross(DVec3::Z).length_squared() > 1e-8 {
            DVec3::Z
        } else {
            DVec3::Y
        };
        let side = direction.cross(reference).normalize() * tip_width * 0.5;
        let shoulder = end - direction * tip_length;
        for (from, to) in [(start, end), (end, shoulder + side), (end, shoulder - side)] {
            points.extend([
                from.to_array().map(|value| value as f32),
                to.to_array().map(|value| value as f32),
            ]);
        }
        let rgba = reactive_field_color(
            sample.magnitude,
            color_range,
            options.color,
            options.colormap.as_ref(),
            1.0,
        )
        .to_rgba8();
        colors.extend(std::iter::repeat_n(
            [
                f32::from(rgba.r) / 255.0,
                f32::from(rgba.g) / 255.0,
                f32::from(rgba.b) / 255.0,
                f32::from(rgba.a) / 255.0,
            ],
            6,
        ));
    }
    Some(gaanim_scene::LineListData {
        points,
        indices: None,
        strip: false,
        color: PenikoColor::WHITE,
        colors: Some(colors),
    })
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
        CanvasEndpoint::LocalNumberLine {
            space,
            axis,
            length,
            value,
            normal_offset,
        } => id_map
            .get(space)
            .and_then(|runtime| states.get(*runtime))
            .map(|state| {
                let axis = axis.clone();
                TrackingEndpoint::LocalNumberLine {
                    space: state.entity,
                    map: gaanim_animation::ScalarMap::new(move |value| axis.normalize(value).ok()),
                    length: *length,
                    value: compile_tracking_scalar(value, id_map, states),
                    normal_offset: compile_tracking_scalar(normal_offset, id_map, states),
                }
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

/// Line composition chosen for a responsive text leaf while measuring it.
#[derive(Debug, Clone, Copy, PartialEq)]
enum TextComposition {
    /// The text fits the offered width unwrapped; compose it without a limit.
    Natural,
    /// Wrap at this width. `ink_width` is the resulting visible width.
    Wrapped { width: f64, ink_width: f64 },
}

impl TextComposition {
    /// Width to materialize the text at; `None` composes it unwrapped.
    fn width(self) -> Option<f64> {
        match self {
            Self::Natural => None,
            Self::Wrapped { width, .. } => Some(width),
        }
    }
}

struct CompiledLayoutMeasure<'a> {
    fixed: BTreeMap<gaanim_layout::LayoutId, DVec2>,
    texts: BTreeMap<gaanim_layout::LayoutId, CompiledTextMeasure>,
    text_compositions: RefCell<BTreeMap<gaanim_layout::LayoutId, TextComposition>>,
    /// Every composition measured per text leaf, with its size: the solver
    /// probes several widths (min-content among them) in no fixed order.
    text_candidates: RefCell<BTreeMap<gaanim_layout::LayoutId, Vec<(TextComposition, DVec2)>>>,
    natural_text_sizes: RefCell<BTreeMap<gaanim_layout::LayoutId, DVec2>>,
    /// Ascent and descent of a full line of each text leaf's font and size.
    line_extents: RefCell<BTreeMap<gaanim_layout::LayoutId, (f64, f64)>>,
    /// Cap height of each text leaf's font and size, for `TextBox::Cap`.
    cap_heights: RefCell<BTreeMap<gaanim_layout::LayoutId, f64>>,
    font_registry: &'a gaanim_text::font::FontRegistry,
}

/// The box a text occupies in a layout: its ink widened vertically to full
/// lines (from the first baseline up by a line's ascent, and for one line down
/// by its descent), so texts of one style share heights and baselines
/// whatever glyphs they contain.
fn text_line_box(
    ink: Bounds3D,
    metrics: gaanim_text::prelude::TextMetrics,
    (ascent, descent): (f64, f64),
) -> Bounds3D {
    let top = ink.max.y.max(metrics.first_baseline + ascent);
    let bottom = if metrics.line_count <= 1 {
        ink.min.y.min(metrics.first_baseline - descent)
    } else {
        ink.min.y
    };
    Bounds3D::new_2d(ink.min.x, bottom, ink.max.x, top)
}

/// The box a text occupies in a layout for its `text_box` mode. Formulas
/// keep their ink box, the convention equations and matrices are laid out
/// with.
fn text_layout_box(
    ink: Bounds3D,
    metrics: gaanim_text::prelude::TextMetrics,
    mode: gaanim_text::prelude::TextBox,
    line_extent: (f64, f64),
    cap_height: Option<f64>,
) -> Bounds3D {
    use gaanim_text::prelude::TextBox;
    match (mode, cap_height) {
        (TextBox::Ink, _) => ink,
        (TextBox::Cap, Some(cap_height)) => {
            let line_box = text_line_box(ink, metrics, line_extent);
            let top = metrics.first_baseline + cap_height;
            // One line ends on its baseline; more lines keep the last one's
            // ink, as the line box does.
            let bottom = if metrics.line_count <= 1 {
                metrics.first_baseline
            } else {
                line_box.min.y
            };
            Bounds3D::new_2d(line_box.min.x, bottom, line_box.max.x, top.max(bottom))
        }
        _ => text_line_box(ink, metrics, line_extent),
    }
}

/// Whether text content is a single `$…$` formula.
fn is_formula(content: &[StructuredTextContent]) -> bool {
    fn collect(content: &[StructuredTextContent], out: &mut String) {
        for item in content {
            match item {
                StructuredTextContent::Literal(text) => out.push_str(text),
                StructuredTextContent::Part(part) => collect(&part.content, out),
            }
        }
    }
    let mut text = String::new();
    collect(content, &mut text);
    let text = text.trim();
    text.len() >= 2 && text.starts_with('$') && text.ends_with('$') && !text.ends_with("\\$")
}

impl CompiledLayoutMeasure<'_> {
    /// Visible size of `text` composed at `width` (`None`: unwrapped).
    fn measure_text(
        &self,
        id: gaanim_layout::LayoutId,
        text: &CompiledTextMeasure,
        width: Option<f64>,
    ) -> Result<DVec2, gaanim_layout::LayoutError> {
        let source = structured_text_typst_source(
            &text.spec,
            width,
            text.font_size,
            &text.font_family,
            text.color,
        );
        let (bounds, metrics) = self.typst_lines(id, text, &source)?;
        let line_box = if is_formula(&text.spec.content) {
            bounds
        } else {
            let mode = text.spec.flow.text_box;
            let cap_height = match mode {
                gaanim_text::prelude::TextBox::Cap => Some(self.cap_height(id, text)?),
                _ => None,
            };
            text_layout_box(
                bounds,
                metrics,
                mode,
                self.line_extent(id, text)?,
                cap_height,
            )
        };
        Ok(DVec2::new(
            line_box.width().max(0.0),
            line_box.height().max(0.0),
        ))
    }

    fn typst_lines(
        &self,
        id: gaanim_layout::LayoutId,
        text: &CompiledTextMeasure,
        source: &str,
    ) -> Result<(Bounds3D, gaanim_text::prelude::TextMetrics), gaanim_layout::LayoutError> {
        gaanim_text::prelude::measure_typst_lines(
            self.font_registry,
            source,
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
        })
    }

    /// Ascent and descent of a full line in this text's style, from glyphs
    /// that reach the ascender and the descender.
    fn line_extent(
        &self,
        id: gaanim_layout::LayoutId,
        text: &CompiledTextMeasure,
    ) -> Result<(f64, f64), gaanim_layout::LayoutError> {
        if let Some(extent) = self.line_extents.borrow().get(&id) {
            return Ok(*extent);
        }
        let mut reference = text.spec.clone();
        reference.content = vec![StructuredTextContent::Literal("ÁÉgjpqy".to_owned())];
        reference.flow.wrap = StructuredTextWrap::NoWrap;
        reference.flow.max_lines = None;
        let source = structured_text_typst_source(
            &reference,
            None,
            text.font_size,
            &text.font_family,
            text.color,
        );
        let (bounds, metrics) = self.typst_lines(id, text, &source)?;
        let extent = (
            (bounds.max.y - metrics.first_baseline).max(0.0),
            (metrics.first_baseline - bounds.min.y).max(0.0),
        );
        self.line_extents.borrow_mut().insert(id, extent);
        Ok(extent)
    }

    /// Height of a capital above the baseline in this text's style.
    fn cap_height(
        &self,
        id: gaanim_layout::LayoutId,
        text: &CompiledTextMeasure,
    ) -> Result<f64, gaanim_layout::LayoutError> {
        if let Some(height) = self.cap_heights.borrow().get(&id) {
            return Ok(*height);
        }
        let mut reference = text.spec.clone();
        reference.content = vec![StructuredTextContent::Literal("H".to_owned())];
        reference.flow.wrap = StructuredTextWrap::NoWrap;
        reference.flow.max_lines = None;
        let source = structured_text_typst_source(
            &reference,
            None,
            text.font_size,
            &text.font_family,
            text.color,
        );
        let (bounds, metrics) = self.typst_lines(id, text, &source)?;
        let height = (bounds.max.y - metrics.first_baseline).max(0.0);
        self.cap_heights.borrow_mut().insert(id, height);
        Ok(height)
    }
}

impl gaanim_layout::IntrinsicMeasure for CompiledLayoutMeasure<'_> {
    fn measure(
        &self,
        id: gaanim_layout::LayoutId,
        constraints: gaanim_layout::BoxConstraints,
    ) -> Result<DVec2, gaanim_layout::LayoutError> {
        self.measure_inner(id, constraints)
    }

    fn is_width_sensitive(&self, id: gaanim_layout::LayoutId) -> bool {
        self.texts
            .get(&id)
            .is_some_and(|text| !matches!(text.spec.flow.wrap, StructuredTextWrap::NoWrap))
    }
}

impl CompiledLayoutMeasure<'_> {
    fn measure_inner(
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
        let limit = match text.spec.flow.wrap {
            StructuredTextWrap::NoWrap => None,
            StructuredTextWrap::Auto => Some(offered_width),
            StructuredTextWrap::Width(limit) => Some(offered_width.min(limit).max(1.0)),
        };
        let cached_natural = self.natural_text_sizes.borrow().get(&id).copied();
        let natural = match cached_natural {
            Some(size) => size,
            None => {
                let size = self.measure_text(id, text, None)?;
                self.natural_text_sizes.borrow_mut().insert(id, size);
                size
            }
        };
        let Some(limit) = limit else {
            return Ok(constraints.constrain(natural));
        };
        // Measurement reports visible (ink) bounds, so a hugging parent
        // offers the text exactly its ink width next. Composing at that
        // narrower width can break lines again, because line advances exceed
        // the ink. Keep a composition whose ink already fits the offer:
        // unwrapped text stays unwrapped, and a wrapped text keeps its lines
        // until it is offered less than its ink or more than it wrapped at.
        const FIT_EPSILON: f64 = 1.0e-6;
        let previous = self.text_compositions.borrow().get(&id).copied();
        let width = if natural.x <= limit + FIT_EPSILON {
            None
        } else if let Some(TextComposition::Wrapped { width, ink_width }) = previous
            && ink_width <= limit + FIT_EPSILON
            && limit <= width + FIT_EPSILON
        {
            Some(width)
        } else {
            Some(limit)
        };
        let (composition, size) = match width {
            None => (TextComposition::Natural, natural),
            Some(width) => {
                let size = self.measure_text(id, text, Some(width))?;
                (
                    TextComposition::Wrapped {
                        width,
                        ink_width: size.x,
                    },
                    size,
                )
            }
        };
        self.text_compositions.borrow_mut().insert(id, composition);
        self.text_candidates
            .borrow_mut()
            .entry(id)
            .or_default()
            .push((composition, size));
        Ok(constraints.constrain(size))
    }

    /// The composition that produced a text leaf's final box: the widest one
    /// measured that fits `width`, not whichever the solver probed last.
    fn final_composition(
        &self,
        id: gaanim_layout::LayoutId,
        width: f64,
    ) -> Option<TextComposition> {
        const FIT_EPSILON: f64 = 1.0e-6;
        let candidates = self.text_candidates.borrow();
        candidates
            .get(&id)
            .and_then(|candidates| {
                candidates
                    .iter()
                    .filter(|(_, size)| size.x <= width + FIT_EPSILON)
                    .max_by(|(a, a_size), (b, b_size)| {
                        let wrap = |composition: &TextComposition| {
                            composition.width().unwrap_or(f64::INFINITY)
                        };
                        a_size
                            .x
                            .total_cmp(&b_size.x)
                            .then(wrap(a).total_cmp(&wrap(b)))
                    })
                    .map(|(composition, _)| *composition)
            })
            .or_else(|| self.text_compositions.borrow().get(&id).copied())
    }
}

/// A world callback queued between segments, applied in command order.
pub(crate) type SegmentMarker = Box<dyn FnOnce(&mut World) + Send + 'static>;

/// Compilation state carried from one segment to the next.
#[derive(Clone)]
pub(crate) struct CompileCursor {
    /// Index of the next segment to compile.
    pub(crate) next_segment: usize,
    builder: Option<crate::builder::SceneBuilderState>,
    scene_ids: Vec<SceneId>,
    id_map: HashMap<ObjectId, ObjectId>,
    object_specs: HashMap<ObjectId, ObjectSpec>,
    responsive_text_widths: HashMap<ObjectId, f64>,
    layout_versions: HashMap<ObjectId, u64>,
    layout_snapshots: HashMap<ObjectId, LayoutTreeSnapshot>,
    object_scopes: HashMap<ObjectId, CompiledObjectScope>,
    camera_position: DVec3,
    camera_zoom: f64,
    camera_rotation: gaanim_core::glam::DQuat,
    camera_target: DVec3,
    camera_up: DVec3,
    /// `(fov_y, near, far)` when the camera is perspective.
    camera_fov: Option<(f64, f64, f64)>,
    cancellation_marks: HashMap<ObjectId, Vec<ObjectId>>,
    canceled_term_children: HashMap<ObjectId, Vec<ObjectId>>,
    deferred_visibility: HashSet<ObjectId>,
    revealed_deferred: HashSet<ObjectId>,
    /// Metadata of the compiled segments, aligned to the builder clock.
    compiled_metadata: Vec<SegmentMetadata>,
    layout_diagnostics: Vec<(Option<ObjectId>, String)>,
}

impl CompileCursor {
    fn fresh() -> Self {
        Self {
            next_segment: 0,
            builder: None,
            scene_ids: Vec::new(),
            id_map: HashMap::new(),
            object_specs: HashMap::new(),
            responsive_text_widths: HashMap::new(),
            layout_versions: HashMap::new(),
            layout_snapshots: HashMap::new(),
            object_scopes: HashMap::new(),
            camera_position: DVec3::ZERO,
            camera_zoom: 1.0,
            camera_rotation: gaanim_core::glam::DQuat::IDENTITY,
            camera_target: DVec3::ZERO,
            camera_up: DVec3::Y,
            camera_fov: None,
            cancellation_marks: HashMap::new(),
            canceled_term_children: HashMap::new(),
            deferred_visibility: HashSet::new(),
            revealed_deferred: HashSet::new(),
            compiled_metadata: Vec::new(),
            layout_diagnostics: Vec::new(),
        }
    }
}

/// A resumable compilation point: the carried state and the timeline exactly
/// as they were before segment `cursor.next_segment` compiled.
#[derive(Clone)]
pub(crate) struct CompileCheckpoint {
    pub(crate) cursor: CompileCursor,
    pub(crate) timeline: Timeline,
}

impl CompileCursor {
    /// The compiled object that stands for authored object `id`.
    pub(crate) fn runtime_id(&self, id: ObjectId) -> Option<ObjectId> {
        self.id_map.get(&id).copied()
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

/// Opacity layers of a fading plexus: links split into this many length ranges.
const CONNECT_FADE_LAYERS: usize = 6;

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

#[derive(Clone)]
struct StyledTextLeaf {
    text: String,
    style: StructuredTextStyle,
    content_boundary_before: bool,
}

fn collect_styled_text_leaves(
    content: &[StructuredTextContent],
    inherited: &StructuredTextStyle,
    leaves: &mut Vec<StyledTextLeaf>,
) {
    let mut has_previous_node = false;
    for node in content {
        let first_leaf = leaves.len();
        match node {
            StructuredTextContent::Literal(text) => {
                leaves.push(StyledTextLeaf {
                    text: text.clone(),
                    style: inherited.clone(),
                    content_boundary_before: false,
                });
            }
            StructuredTextContent::Part(part) => {
                let style = merge_text_style(inherited, &part.style);
                collect_styled_text_leaves(&part.content, &style, leaves);
            }
        }
        if let Some(leaf) = leaves.get_mut(first_leaf) {
            leaf.content_boundary_before = has_previous_node;
        }
        has_previous_node = true;
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

fn typst_math_style_arguments(style: &StructuredTextStyle) -> Vec<String> {
    let mut args = Vec::new();
    if let Some(font) = style.math_font.as_ref().or(style.font.as_ref()) {
        args.push(format!("font: \"{}\"", escape_typst_string(font)));
    }
    if let Some(size) = style.size {
        args.push(format!("size: #{size}pt"));
    }
    if let Some(weight) = style.weight {
        args.push(format!("weight: #{weight}"));
    }
    if let Some(italic) = style.italic {
        args.push(format!(
            "style: \"{}\"",
            if italic { "italic" } else { "normal" }
        ));
    }
    if let Some(color) = style.color {
        args.push(format!("fill: #rgb(\"{}\")", color_to_hex(color)));
    }
    if let Some(spacing) = style.letter_spacing {
        args.push(format!("tracking: #{spacing}pt"));
    }
    if let Some(spacing) = style.word_spacing {
        args.push(format!("spacing: #{spacing}pt"));
    }
    args
}

fn styled_typst_chunk(text: &str, math: bool, style: &StructuredTextStyle) -> String {
    if text.is_empty() {
        return String::new();
    }
    if math {
        let args = typst_math_style_arguments(style);
        let mut content = if args.is_empty() {
            text.to_owned()
        } else {
            format!("text({}, {text})", args.join(", "))
        };
        for decoration in &style.decorations {
            content = match decoration.as_str() {
                "underline" => format!("underline({content})"),
                "strike" | "strikethrough" => format!("strike({content})"),
                _ => content,
            };
        }
        if let Some(baseline) = style.baseline.filter(|value| *value != 0.0) {
            content = format!("#move(dy: {}pt)[${content}$]", -baseline);
        }
        return content;
    }

    let args = typst_style_arguments(style);
    let content = format!("#text(\"{}\")", escape_typst_string(text));
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
fn structured_typst_content(spec: &StructuredTextSpec, _font_size: f64) -> String {
    let mut raw_leaves = Vec::new();
    collect_styled_text_leaves(
        &spec.content,
        &StructuredTextStyle::default(),
        &mut raw_leaves,
    );
    let mut markup = gaanim_text::structured::InlineMarkupParser::with_markup(spec.markup);
    let mut marked_leaves = Vec::new();
    for leaf in raw_leaves {
        let mut first_segment = true;
        for segment in markup
            .push(&leaf.text)
            .expect("TextSpec validates inline markup before compilation")
        {
            if segment.text.is_empty() {
                continue;
            }
            let mut style = leaf.style.clone();
            if segment.strong {
                style.weight = Some(style.weight.unwrap_or(400).max(700));
            }
            if segment.emphasis {
                style.italic = Some(true);
            }
            marked_leaves.push(StyledTextLeaf {
                text: segment.text,
                style,
                content_boundary_before: leaf.content_boundary_before && first_segment,
            });
            first_segment = false;
        }
    }
    markup
        .finish()
        .expect("TextSpec validates inline markup before compilation");
    let mut raw_leaves = marked_leaves;
    // Preserve every authored content boundary in math as ordinary Typst
    // whitespace. Typst remains responsible for deciding whether that
    // whitespace separates identifiers, participates in math syntax, or has
    // no visual effect. Boundary whitespace is normalized to one token.
    let mut token_boundary_before = vec![false; raw_leaves.len()];
    let mut in_math = false;
    let mut escaped = false;
    for index in 0..raw_leaves.len().saturating_sub(1) {
        for character in raw_leaves[index].text.chars() {
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
        let left_has_space = raw_leaves[index].text.ends_with([' ', '\t']);
        let right_has_space = raw_leaves[index + 1].text.starts_with([' ', '\t']);
        let display_open = left_has_space
            && raw_leaves[index]
                .text
                .trim_end_matches([' ', '\t'])
                .ends_with('$');
        let display_close = right_has_space
            && raw_leaves[index + 1]
                .text
                .trim_start_matches([' ', '\t'])
                .starts_with('$');
        let delimiter_open = raw_leaves[index]
            .text
            .trim_end_matches([' ', '\t'])
            .ends_with('$');
        let delimiter_close = raw_leaves[index + 1]
            .text
            .trim_start_matches([' ', '\t'])
            .starts_with('$');
        if (left_has_space || right_has_space) && !display_open && !display_close {
            let trimmed_left_len = raw_leaves[index].text.trim_end_matches([' ', '\t']).len();
            raw_leaves[index].text.truncate(trimmed_left_len);
            raw_leaves[index + 1].text = raw_leaves[index + 1]
                .text
                .trim_start_matches([' ', '\t'])
                .to_string();
        }
        if raw_leaves[index + 1].content_boundary_before && !delimiter_open && !delimiter_close {
            token_boundary_before[index + 1] = true;
        }
    }
    // Coalesce adjacent leaves with the same resolved style into one Typst
    // expression. This makes the structured form shape exactly like the
    // equivalent source string containing ordinary spaces between contents.
    let mut leaves: Vec<(String, StructuredTextStyle, bool)> = Vec::new();
    for (leaf, token_boundary_before) in raw_leaves.into_iter().zip(token_boundary_before) {
        if let Some((previous, previous_style, _)) = leaves.last_mut()
            && *previous_style == leaf.style
        {
            if token_boundary_before {
                // This is parser whitespace, not an authored fixed-width gap.
                previous.push(' ');
            }
            previous.push_str(&leaf.text);
        } else {
            leaves.push((leaf.text, leaf.style, token_boundary_before));
        }
    }
    let mut output = String::new();
    let mut in_math = false;
    for (text, style, token_boundary_before) in leaves {
        if token_boundary_before && in_math {
            output.push(' ');
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
                output.push_str(&styled_typst_chunk(&chunk, in_math, &style));
                chunk.clear();
                output.push('$');
                in_math = !in_math;
            } else {
                chunk.push(character);
            }
        }
        if escaped {
            chunk.push('\\');
        }
        output.push_str(&styled_typst_chunk(&chunk, in_math, &style));
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
pub(crate) fn structured_text_typst_source(
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
    // Validated as a lowercase ISO 639 code, so it needs no escaping.
    let lang = spec
        .flow
        .lang
        .as_deref()
        .map(|lang| format!(", lang: \"{lang}\""))
        .unwrap_or_default();
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
         #set text({font}fill: rgb(\"{hex}\"), dir: {direction}, hyphenate: {}{lang}{weight}{italic}{tracking})\n\
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
        font_size: spec.style.size.unwrap_or(role.size).max(1.0e-6),
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

#[allow(clippy::too_many_arguments)]
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
    rests: &HashMap<ObjectId, crate::builder::LayoutRest>,
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
                rests,
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
        if let Some(rest) = rests.get(&source) {
            transform.scale = rest.scale;
            transform.rotation = rest.rotation;
        }
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
    rests: &HashMap<ObjectId, crate::builder::LayoutRest>,
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
        rests,
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

/// Where a box sits in its layout tree, for diagnostics: the kind and index
/// of each ancestor below the root, such as `column[1] > row[0]`.
fn layout_box_path(
    tree: &CompiledLayoutTree,
    snapshots: &HashMap<ObjectId, LayoutTreeSnapshot>,
    id: gaanim_layout::LayoutId,
) -> String {
    let kind_name = |id: &gaanim_layout::LayoutId| {
        tree.source_by_id
            .get(id)
            .and_then(|source| snapshots.get(source))
            .map_or("box", |snapshot| match snapshot.spec.kind {
                gaanim_layout::LayoutNodeKind::Row { .. } => "row",
                gaanim_layout::LayoutNodeKind::Column { .. } => "column",
                gaanim_layout::LayoutNodeKind::Grid { .. } => "grid",
                gaanim_layout::LayoutNodeKind::Stack => "stack",
                gaanim_layout::LayoutNodeKind::Leaf => "box",
            })
    };
    let mut segments = Vec::new();
    let mut current = id;
    while let Some(parent) = tree.parent_by_id.get(&current) {
        let index = tree
            .children_by_id
            .get(parent)
            .and_then(|children| children.iter().position(|child| *child == current))
            .unwrap_or(0);
        segments.push(format!("{}[{index}]", kind_name(parent)));
        current = *parent;
    }
    if segments.is_empty() {
        return format!("root {}", kind_name(&id));
    }
    segments.reverse();
    segments.join(" > ")
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

impl SceneModel {
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
        let base_size = builder.text_config.roles[&role].size.max(1.0e-6);
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
            let tip_len: f64 = 0.10;
            let tip_half_w: f64 = 0.05;
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
                    let estimated_width = text.chars().count() as f64 * 0.09;
                    children.push(Self::axis_text(
                        builder,
                        &text,
                        sx(0.0) - tick_half - 0.08 - estimated_width * 0.5,
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
                    sx(x_max) + 0.18,
                    sy(0.0) - tick_half - 2.0,
                    config.label_color,
                    config.label_size,
                ));
            }
            if let Some(label) = config.y_label.as_deref().filter(|_| y_axis_in_range) {
                children.push(Self::axis_text(
                    builder,
                    label,
                    sx(0.0) + tick_half + 0.12,
                    sy(y_max) + 0.12,
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
                    // Glyphs inherit the parent's z_index in the renderer.
                    builder
                        .commands
                        .entity(state.entity)
                        .insert(gaanim_scene::RenderOrder {
                            z_index: 1000,
                            creation_order: label.id.index() as u64,
                        });
                    for child in &child_entities {
                        builder
                            .commands
                            .entity(*child)
                            .insert(gaanim_scene::HudOverlay);
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
            fill_level: 0.0,
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
        self.compile_resumable(
            commands,
            timeline,
            font_registry,
            text_config,
            None,
            None,
            Vec::new(),
        );
    }

    /// Compile the segments from `resume` (or from the start), optionally
    /// capturing a checkpoint before segment `checkpoint_at` and queueing
    /// world markers before chosen segments. An index equal to the segment
    /// count denotes the point after the last segment.
    ///
    /// Resuming is valid only when every segment before the cursor, and every
    /// scene-wide input, is identical to the compilation that produced it,
    /// and `timeline` is that checkpoint's timeline.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn compile_resumable<'w, 's>(
        &self,
        commands: &mut Commands<'w, 's>,
        timeline: &mut Timeline,
        font_registry: &gaanim_text::font::FontRegistry,
        text_config: &gaanim_text::prelude::TextConfig,
        resume: Option<CompileCursor>,
        checkpoint_at: Option<usize>,
        mut markers: Vec<(usize, SegmentMarker)>,
    ) -> Option<CompileCheckpoint> {
        let CompileCursor {
            next_segment: start,
            builder: builder_state,
            mut scene_ids,
            mut id_map,
            mut object_specs,
            mut responsive_text_widths,
            mut layout_versions,
            mut layout_snapshots,
            mut object_scopes,
            mut camera_position,
            mut camera_zoom,
            mut camera_rotation,
            mut camera_target,
            mut camera_up,
            mut camera_fov,
            mut cancellation_marks,
            mut canceled_term_children,
            mut deferred_visibility,
            mut revealed_deferred,
            compiled_metadata,
            layout_diagnostics,
        } = resume.unwrap_or_else(CompileCursor::fresh);
        self.state
            .lock()
            .expect("canvas state poisoned")
            .layout_diagnostics = layout_diagnostics;
        let manifest = self.segment_manifest();
        let mut segment_metadata = manifest
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
                        ambient: stop.ambient,
                    })
                    .collect(),
            })
            .collect::<Vec<_>>();
        // Segments before a resume point keep the clock they were compiled with.
        for (metadata, compiled) in segment_metadata.iter_mut().zip(compiled_metadata) {
            *metadata = compiled;
        }
        timeline.set_segments(segment_metadata.clone());
        timeline.beat_grid = self.tempo;
        timeline.set_markers(
            self.markers()
                .into_iter()
                .map(|marker| gaanim_timeline::timeline::TimelineMarker {
                    name: marker.name,
                    time: marker.time,
                })
                .collect(),
        );
        let segments = self
            .state
            .lock()
            .expect("canvas state poisoned")
            .segments
            .clone();
        let mut builder = match builder_state {
            Some(state) => {
                SceneBuilder::resume(commands, timeline, font_registry, text_config, state)
            }
            None => SceneBuilder::new(commands, timeline, font_registry, text_config),
        };
        let mut checkpoint = None;
        // Raw bounds for the canvas background (visual, no margin).
        let raw_bounds = self.frame.bounds();
        // Inset bounds for layout operations (to_edge, to_corner respect margin).
        let m = &self.margin;
        let frame_bounds = Bounds3D::new_2d(
            raw_bounds.min.x + m.left,
            raw_bounds.min.y + m.bottom,
            raw_bounds.max.x - m.right,
            raw_bounds.max.y - m.top,
        );
        let bg_color = self.background.unwrap_or(gaanim_core::peniko::Color::WHITE);
        let bg_paint = self
            .background_paint
            .clone()
            .unwrap_or_else(|| gaanim_renderer::background::BackgroundPaint::solid(bg_color));
        for index in start..=segments.len() {
            if checkpoint_at == Some(index) {
                checkpoint = Some(CompileCheckpoint {
                    cursor: CompileCursor {
                        next_segment: index,
                        builder: Some(builder.state()),
                        scene_ids: scene_ids.clone(),
                        id_map: id_map.clone(),
                        object_specs: object_specs.clone(),
                        responsive_text_widths: responsive_text_widths.clone(),
                        layout_versions: layout_versions.clone(),
                        layout_snapshots: layout_snapshots.clone(),
                        object_scopes: object_scopes.clone(),
                        camera_position,
                        camera_zoom,
                        camera_rotation,
                        camera_target,
                        camera_up,
                        camera_fov,
                        cancellation_marks: cancellation_marks.clone(),
                        canceled_term_children: canceled_term_children.clone(),
                        deferred_visibility: deferred_visibility.clone(),
                        revealed_deferred: revealed_deferred.clone(),
                        compiled_metadata: segment_metadata[..index].to_vec(),
                        layout_diagnostics: self
                            .state
                            .lock()
                            .expect("canvas state poisoned")
                            .layout_diagnostics
                            .clone(),
                    },
                    timeline: builder.timeline.clone(),
                });
            }
            for (_, marker) in markers.extract_if(.., |(at, _)| *at == index) {
                builder.commands.queue(marker);
            }
            let Some(seg) = segments.get(index) else {
                break;
            };
            let previous_scene = seg
                .prev_segment
                .and_then(|index| scene_ids.get(index).copied());
            // Zones belong to the segment that created them; a resumed
            // compile replays this segment's zones again.
            builder.commands.queue(move |world: &mut World| {
                world
                    .get_resource_or_insert_with(gaanim_scene::LayoutZones::default)
                    .0
                    .retain(|zone| zone.segment < index);
            });
            let scene_id = builder.begin_scene(&seg.name);
            scene_ids.push(scene_id);
            let start_time = builder.current_time;
            let first_stop = builder.stop_times.len();
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
            // The manifest sums each segment's local cursor, while clips,
            // stops, and scene starts use the builder's running clock. The two
            // float sums can differ by a few ULPs, which made a terminal stop
            // resolve to the next scene or land past the scene duration.
            // Segment metadata adopts the builder clock so they agree exactly.
            let stop_times = &builder.stop_times[first_stop..];
            if let Some(metadata) = segment_metadata.get_mut(index)
                && metadata.stops.len() == stop_times.len()
            {
                metadata.start_time = start_time;
                metadata.end_time = builder.current_time;
                for (stop, &time) in metadata.stops.iter_mut().zip(stop_times) {
                    stop.time = time;
                }
            }
            let end_time = builder.current_time;
            builder.commands.queue(move |world: &mut World| {
                if let Some(mut zones) = world.get_resource_mut::<gaanim_scene::LayoutZones>() {
                    for zone in zones.0.iter_mut().filter(|zone| zone.segment == usize::MAX) {
                        zone.segment = index;
                        zone.end = end_time;
                    }
                }
            });
            builder.end_scene();
        }
        let segment_paints = segments
            .iter()
            .zip(&segment_metadata)
            .map(
                |(segment, metadata)| gaanim_renderer::pipeline::SegmentBackgroundPaint {
                    start_time: metadata.start_time,
                    end_time: metadata.end_time,
                    paint: segment.background.clone(),
                    hold_at_end: metadata
                        .stops
                        .iter()
                        .any(|stop| (stop.time - metadata.end_time).abs() <= 1e-5),
                },
            )
            .collect();
        let segment_post_processes = segments
            .iter()
            .zip(&segment_metadata)
            .map(
                |(segment, metadata)| gaanim_renderer::post_process::SegmentPostProcess {
                    start_time: metadata.start_time,
                    end_time: metadata.end_time,
                    hold_at_end: metadata
                        .stops
                        .iter()
                        .any(|stop| (stop.time - metadata.end_time).abs() <= 1e-5),
                    post: segment.post_process.clone(),
                },
            )
            .collect();
        builder.timeline.set_segments(segment_metadata);

        // Every tween is scheduled now: continuous rolling displays settle
        // outside the windows in which their parameters are animated.
        for (entity, sources) in std::mem::take(&mut builder.rolling_tween_sources) {
            let mut windows: Vec<(f64, f64)> = builder
                .timeline
                .clips
                .values()
                .filter_map(|clip| match &clip.payload {
                    gaanim_timeline::clip::ClipPayload::Animation(animation)
                        if clip.duration > 0.0
                            && sources.contains(&animation.target)
                            && matches!(
                                animation.lens,
                                gaanim_timeline::clip::PropertyLensSpec::SignalFloat { .. }
                            ) =>
                    {
                        Some((clip.start, clip.duration))
                    }
                    _ => None,
                })
                .collect();
            // Untweened sources (e.g. sample drivers) keep free continuous wheels.
            if windows.is_empty() {
                continue;
            }
            windows.sort_by(|a, b| a.0.total_cmp(&b.0));
            builder
                .commands
                .entity(entity)
                .insert(gaanim_animation::RollingTweens(windows));
        }

        for (i, seg) in segments.iter().enumerate() {
            if let Some(prev) = seg.prev_segment
                && prev < i
                && i < scene_ids.len()
                && prev < scene_ids.len()
                && let Some(tr) = &seg.transition
            {
                builder.timeline.connect(
                    scene_ids[prev],
                    scene_ids[i],
                    Self::runtime_transition(tr, &id_map),
                );
            }
        }

        // Echo copies clone their sources once every segment has been
        // compiled, so they carry every component the sources ended up with.
        let mut echoes: Vec<_> = object_specs
            .iter()
            .filter_map(|(logical, spec)| Some((*id_map.get(logical)?, spec.echo?)))
            .collect();
        echoes.sort_by_key(|(id, _)| *id);
        for (id, echo) in echoes {
            Self::attach_echo(&mut builder, id, echo);
        }

        // Insert canvas background resource so the renderer draws a visible
        // canvas boundary, distinguishing the canvas area from the window.
        // Uses raw_bounds (no margin) — the visual background covers the full canvas.
        builder
            .commands
            .insert_resource(gaanim_renderer::pipeline::CanvasBackground {
                paint: bg_paint,
                segment_paints,
                pixel_size: self.frame.preview_pixel_size(),
                bounds: raw_bounds,
            });
        let post_process = Self::compiled_post_process(
            &builder,
            &id_map,
            self.post_process.clone(),
            segment_post_processes,
        );
        builder.commands.insert_resource(post_process);
        match self.motion_blur {
            Some(blur) => builder.commands.insert_resource(blur),
            None => builder
                .commands
                .remove_resource::<gaanim_renderer::effects::MotionBlur>(),
        }

        // Clear with the canvas color as well. The drawable background is
        // world-space geometry and can be rotated by the camera; using the
        // same clear color prevents Bevy's window clear color from showing
        // through at the viewport edges during that rotation.
        let rgba = bg_color.to_rgba8();
        builder
            .commands
            .insert_resource(ClearColor(Color::srgba_u8(rgba.r, rgba.g, rgba.b, rgba.a)));
        builder
            .commands
            .insert_resource(gaanim_media::PreviewAudioTracks(self.audio_tracks.clone()));
        builder.commands.insert_resource(self.lighting_3d);
        checkpoint
    }

    /// Compile the scene into a scratch world and return every layout
    /// diagnostic with the box it belongs to, including those only a
    /// resolved layout reveals (a decorated box of zero size, a failed
    /// resolution). `gaanim check` and `check_layout()` use it; the scene's
    /// own diagnostics are left as they were.
    pub fn compiled_layout_diagnostics(&self) -> Vec<(Option<ObjectId>, String)> {
        let saved = self
            .state
            .lock()
            .expect("canvas state poisoned")
            .layout_diagnostics
            .clone();
        let mut world = World::new();
        let mut timeline = Timeline::new();
        let mut font_registry = gaanim_text::font::FontRegistry::new();
        let text_config = self.scene_text_config(&gaanim_text::prelude::TextConfig::default());
        self.register_theme_fonts(&mut font_registry);
        {
            let mut commands = world.commands();
            self.compile_into(&mut commands, &mut timeline, &font_registry, &text_config);
        }
        world.flush();
        let compiled = std::mem::replace(
            &mut self
                .state
                .lock()
                .expect("canvas state poisoned")
                .layout_diagnostics,
            saved,
        );
        let mut unique = Vec::new();
        for entry in compiled {
            if !unique.contains(&entry) {
                unique.push(entry);
            }
        }
        unique
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
        // Same resolution as runtime replay: the theme's roles, or the host
        // configuration unchanged without a theme.
        text_config = self.scene_text_config(&text_config);
        self.register_theme_fonts(&mut font_registry);
        let mut commands = world.commands();
        self.compile_into(&mut commands, &mut timeline, &font_registry, &text_config);
        world.insert_resource(timeline);
        world.insert_resource(font_registry);
        world.insert_resource(text_config);
    }

    #[allow(clippy::too_many_arguments)]
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
        let mut fade_in_targets: HashSet<ObjectId> = seg
            .ops
            .iter()
            .flat_map(|op| {
                let anims: &[AnimationBuilder] = match op {
                    Op::Animate { anim, active: true } => std::slice::from_ref(anim),
                    Op::Play(anims) | Op::Launch(anims) => anims,
                    _ => &[],
                };
                anims.iter().filter_map(|anim| {
                    matches!(
                        anim.anim_type,
                        AnimationType::FadeIn | AnimationType::FadeInFrom { .. }
                    )
                    .then_some(anim.target)
                })
            })
            .collect();
        fade_in_targets.extend(Self::camera_view_entries(&seg.ops));
        for (op_index, op) in seg.ops.iter().enumerate() {
            match op {
                Op::Spawn(spec) => {
                    let live = spec.lock().expect("object spec poisoned").clone();
                    let mut authored = diagnostic_state
                        .lock()
                        .expect("canvas state poisoned")
                        .frozen_spawn_specs
                        .get(&live.id)
                        .cloned()
                        .unwrap_or_else(|| live.clone());
                    // Playback is scheduled after declaration geometry has been
                    // frozen. Keep the final media schedule while retaining the
                    // original geometry and its reversible timeline cuts.
                    match (&mut authored.kind, &live.kind) {
                        (
                            SpawnKind::Video {
                                playback: frozen, ..
                            },
                            SpawnKind::Video {
                                playback: current, ..
                            },
                        ) => *frozen = current.clone(),
                        (
                            SpawnKind::Lottie { playback: frozen },
                            SpawnKind::Lottie { playback: current },
                        ) => *frozen = current.clone(),
                        _ => {}
                    }
                    // Layering and box structure have no timeline cut: a
                    // z-index, a box background or the box that adopts the
                    // drawable, set after the declaration froze, still apply.
                    // A drawable later detached keeps the box it was spawned
                    // in, whose layout snapshots still place it until then.
                    authored.z_index = live.z_index;
                    authored.layout_background =
                        authored.layout_background.or(live.layout_background);
                    authored.layout_owner = authored.layout_owner.or(live.layout_owner);
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
                    if spec.hud {
                        Self::apply_hud(builder, actual.id);
                    }
                    if spec.exclude_from_parent_draw
                        && let Some(state) = builder.states.get_mut(actual.id)
                    {
                        state.exclude_from_parent_draw = true;
                    }
                    id_map.insert(spec.id, actual.id);
                    if spec.svg_root {
                        Self::scene_unit_svg_strokes(builder, spec.id, object_specs, id_map);
                        Self::apply_svg_part_layouts(
                            builder,
                            spec.id,
                            object_specs,
                            id_map,
                            frame_bounds,
                        );
                    }
                    object_scopes.insert(spec.id, CompiledObjectScope::Segment(scene_id));
                    // Compilation creates every entity up front so arbitrary timeline seeks
                    // remain possible. An object declared after earlier animations must still
                    // stay hidden until the playhead reaches its declaration point. A group
                    // adds no geometry of its own: its members already follow their own
                    // declarations, and hiding the group would hide them since time zero.
                    if spec.defer_visibility_until_play {
                        if let Some(state) = builder.states.get(actual.id).cloned() {
                            builder.hide_visuals_now(&state);
                        }
                    } else if !transform_targets.contains(&spec.id)
                        && !matches!(spec.kind, SpawnKind::Group(_) | SpawnKind::GroupNoCenter(_))
                        && builder.current_time > scene_start + 1e-9
                        && let Some(state) = builder.states.get(actual.id).cloned()
                    {
                        builder.hide_visuals_now(&state);
                        // A declaration makes new geometry available, but must not
                        // override a later fade-in, including on a group member.
                        let fade_roots: HashSet<ObjectId> = fade_in_targets
                            .iter()
                            .filter_map(|id| id_map.get(id).copied())
                            .collect();
                        if fade_roots.is_empty() {
                            builder.schedule_show_now(actual.id);
                        } else {
                            for target in builder.hierarchy_ids(actual.id) {
                                if !fade_roots.contains(&target) {
                                    builder.schedule_show_root_at(target, builder.current_time);
                                }
                            }
                        }
                    }
                    if transform_targets.contains(&spec.id)
                        && let Some(state) = builder.states.get(actual.id).cloned()
                    {
                        builder.hide_visuals_now(&state);
                        builder.schedule_hide_hierarchy(actual.id);
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
                Op::CaptureCameraState { id } => {
                    builder.timeline.add_clip(
                        builder.default_track,
                        builder.current_time,
                        0.0,
                        gaanim_timeline::clip::ClipPayload::CameraCapture { id: *id },
                    );
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
                        if let Some(mut remapped) = Self::remap_anim(anim, id_map, object_specs) {
                            Self::resolve_reveal_groups(
                                builder,
                                object_specs,
                                anim.target,
                                &mut remapped,
                            );
                            super::text_motion::attach_text_motion_context(
                                object_specs,
                                frame_bounds,
                                text_config,
                                anim.target,
                                &mut remapped,
                            );
                            let anim = remapped;
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
                        Self::continue_text_transition_identity(anim, id_map);
                    }
                }
                Op::Immediate(anim) => {
                    if let Some(mut anim) = Self::remap_anim(anim, id_map, object_specs) {
                        anim.duration = 0.0;
                        anim.delay = 0.0;
                        builder.play(anim);
                    }
                }
                Op::Play(anims) | Op::Launch(anims) => {
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
                        .filter_map(|anim| {
                            let mut remapped = Self::remap_anim(anim, id_map, object_specs)?;
                            Self::resolve_reveal_groups(
                                builder,
                                object_specs,
                                anim.target,
                                &mut remapped,
                            );
                            super::text_motion::attach_text_motion_context(
                                object_specs,
                                frame_bounds,
                                text_config,
                                anim.target,
                                &mut remapped,
                            );
                            Some(remapped)
                        })
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
                    // A launch schedules like a play but leaves the cursor
                    // where it was, so what follows runs over it.
                    if matches!(op, Op::Play(_)) {
                        builder.current_time = start + max_duration;
                    }
                    for anim in anims {
                        Self::continue_text_transition_identity(anim, id_map);
                    }
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
                                    offset: DVec3::new(0.0, -0.24, 0.0),
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
                        let pad = (bounds.width() * 0.08).max(0.03);
                        let strike = builder
                            .line(
                                Point::new(bounds.min.x - pad, bounds.min.y - pad * 0.25),
                                Point::new(bounds.max.x + pad, bounds.max.y + pad * 0.25),
                            )
                            .no_fill()
                            .stroke(strike_color, 0.03)
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
                            bounds.max.y + 0.12
                        } else {
                            bounds.min.y - 0.12
                        };
                        let brace = builder
                            .brace(
                                Point::new(bounds.min.x, y),
                                Point::new(bounds.max.x, y),
                                -side * 0.10,
                            )
                            .no_fill()
                            .stroke(color, 0.02)
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
                            .stroke(PenikoColor::WHITE, 0.02)
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
                    // The authoring cursor always advances by `duration`, so the
                    // compiled playhead must too, even when nothing matched.
                    let end = builder.current_time + duration.max(0.0);
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
                    builder.current_time = end;
                }
                Op::LayoutTransition {
                    from_version,
                    to,
                    duration,
                    entering,
                    leaving,
                    resolve,
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
                    if !*resolve {
                        continue;
                    }
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
                        &builder.layout_rests,
                    ) else {
                        continue;
                    };
                    let root_id = tree.root.id;
                    let Some(container) = id_map.get(&root_source).copied() else {
                        continue;
                    };
                    // A child entering now appears in its place: only the
                    // others move, from where they were.
                    let entering_subtree = |id: gaanim_layout::LayoutId| {
                        let mut current = Some(id);
                        while let Some(id) = current {
                            if tree.source_by_id.get(&id) == entering.as_ref() {
                                return true;
                            }
                            current = tree.parent_by_id.get(&id).copied();
                        }
                        false
                    };
                    let before: HashMap<ObjectId, SpatialTransform> = tree
                        .source_by_id
                        .iter()
                        .filter(|(id, _)| **id != root_id && !entering_subtree(**id))
                        .filter_map(|(_, source)| {
                            let actual = id_map.get(source).copied()?;
                            builder
                                .states
                                .get(actual)
                                .map(|state| (actual, state.transform))
                        })
                        .collect();
                    // Where each child stands now, by source object: its
                    // offset from the rest its layout last gave it survives
                    // this reflow, as a CSS transform survives a relayout.
                    let current_translations: HashMap<ObjectId, DVec3> = tree
                        .source_by_id
                        .iter()
                        .filter(|(id, _)| **id != root_id && !entering_subtree(**id))
                        .filter_map(|(_, source)| {
                            let actual = id_map.get(source).copied()?;
                            builder
                                .states
                                .get(actual)
                                .map(|state| (*source, state.transform.translation))
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
                        text_compositions: RefCell::default(),
                        text_candidates: RefCell::default(),
                        line_extents: RefCell::default(),
                        cap_heights: RefCell::default(),
                        natural_text_sizes: RefCell::default(),
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
                    // A decorated box that resolves to nothing draws nothing:
                    // usually an empty bar or rule missing `width="fill"`.
                    for (layout_id, source) in &tree.source_by_id {
                        if !object_specs
                            .get(source)
                            .is_some_and(|spec| spec.layout_background.is_some())
                        {
                            continue;
                        }
                        let Some(size) = resolved
                            .boxes
                            .get(layout_id)
                            .map(|resolved| resolved.bounds.size())
                        else {
                            continue;
                        };
                        // Keyed by the box itself, so resolving it again (on
                        // its own first, then inside the box that adopts it)
                        // replaces the earlier report.
                        const EMPTY_BOX: &str = "a box with a background or border has zero";
                        let mut state = diagnostic_state.lock().expect("canvas state poisoned");
                        state.layout_diagnostics.retain(|(owner, message)| {
                            *owner != Some(*source) || !message.contains(EMPTY_BOX)
                        });
                        let empty = match (size.x <= 1.0e-6, size.y <= 1.0e-6) {
                            (true, true) => "width and height",
                            (true, false) => "width",
                            (false, true) => "height",
                            (false, false) => continue,
                        };
                        let path = layout_box_path(&tree, layout_snapshots, *layout_id);
                        state.layout_diagnostics.push((
                            Some(*source),
                            format!(
                                "{path}: {EMPTY_BOX} {empty}, so it draws nothing; give it \
                                 content or a size such as width=\"fill\" or height=\"8px\""
                            ),
                        ));
                    }
                    let text_compositions: BTreeMap<_, _> = tree
                        .texts
                        .keys()
                        .filter_map(|id| {
                            let width = resolved.boxes.get(id)?.bounds.width();
                            Some((*id, measurer.final_composition(*id, width)?))
                        })
                        .collect();
                    let line_extents = measurer.line_extents.into_inner();
                    let cap_heights = measurer.cap_heights.into_inner();
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
                        // Unwrapped (natural) text is keyed by an infinite width.
                        let width = match text_compositions.get(layout_id) {
                            Some(composition) => composition.width().unwrap_or(f64::INFINITY),
                            None => target_box.bounds.width(),
                        }
                        .max(1.0);
                        let current_width = responsive_text_widths
                            .get(source)
                            .copied()
                            .unwrap_or_else(|| frame_bounds.width().max(1.0));
                        if width == current_width || (width - current_width).abs() <= 1.0e-6 {
                            continue;
                        }
                        let mut materialized = text_spec;
                        let SpawnKind::Text(text) = &mut materialized.kind else {
                            continue;
                        };
                        text.flow.wrap = if width.is_finite() {
                            StructuredTextWrap::Width(match text.flow.wrap {
                                StructuredTextWrap::Width(limit) => limit.min(width),
                                StructuredTextWrap::Auto => width,
                                StructuredTextWrap::NoWrap => continue,
                            })
                        } else {
                            StructuredTextWrap::NoWrap
                        };
                        let replacement = Self::spawn_one(
                            builder,
                            &materialized,
                            id_map,
                            frame_bounds,
                            text_config,
                            scene_background,
                        );
                        let entering_ancestor = entering.is_some_and(|entering| {
                            let mut current = Some(*layout_id);
                            while let Some(id) = current {
                                if tree.source_by_id.get(&id) == Some(&entering) {
                                    return true;
                                }
                                current = tree.parent_by_id.get(&id).copied();
                            }
                            false
                        });
                        let entry_pending = Self::fade_in_pending(&seg.ops, op_index, *source);
                        // A text placed for the first time, or entering with
                        // its box, has shown no composition worth fading from.
                        let instant =
                            entering_ancestor || !responsive_text_widths.contains_key(source);
                        let own_entry = entering.as_ref() == Some(source);
                        text_crossfades.push((
                            member,
                            replacement.id,
                            entry_pending,
                            instant,
                            own_entry,
                        ));
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
                        let mut children: Vec<_> = child_ids
                            .iter()
                            .filter_map(|id| materialized_by_id.get(id).copied())
                            .collect();
                        let background_source = tree
                            .source_by_id
                            .get(parent_id)
                            .and_then(|source| object_specs.get(source))
                            .and_then(|spec| spec.layout_background);
                        let background = background_source
                            .and_then(|source| id_map.get(&source))
                            .copied();
                        if let Some(background) = background {
                            children.insert(0, background);
                        }
                        // A leaving child, or a text replaced by its rewrapped
                        // copy, fades out where it stands. It keeps its parent
                        // in the scene hierarchy: reparenting is not on the
                        // timeline, so it would also change the child's earlier,
                        // parent-relative placement and inherited opacity.
                        let leaving_child = leaving
                            .as_ref()
                            .and_then(|member| id_map.get(member))
                            .copied();
                        let keeps_parent = |child: &ObjectId| {
                            Some(*child) == leaving_child
                                || text_crossfades.iter().any(|(old, ..)| old == child)
                        };
                        let removed_children: Vec<_> = builder
                            .states
                            .get(parent)
                            .map(|state| {
                                state
                                    .children
                                    .iter()
                                    .copied()
                                    .filter(|child| !children.contains(child))
                                    .filter(|child| !keeps_parent(child))
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
                        if let Some(background) = background
                            && let Some(state) = builder.states.get_mut(background)
                        {
                            // Group attachment preserves world placement by default;
                            // a decoration instead uses its container's local box.
                            state.transform = SpatialTransform::identity();
                            builder
                                .commands
                                .entity(state.entity)
                                .insert(state.transform);
                        }
                        // The background shares the order of the box's first
                        // content and draws just before it: under that content,
                        // over whatever was created before the box.
                        if let Some(background) = background {
                            let first = children
                                .iter()
                                .filter(|child| **child != background)
                                .flat_map(|child| builder.hierarchy_ids(*child))
                                .map(|id| id.index())
                                .min();
                            let z_index = background_source
                                .and_then(|source| object_specs.get(&source))
                                .map_or(0, |spec| spec.z_index);
                            if let (Some(first), Some(state)) =
                                (first, builder.states.get(background))
                            {
                                builder.commands.entity(state.entity).insert((
                                    gaanim_scene::LayoutBackdrop,
                                    RenderOrder {
                                        z_index,
                                        creation_order: first as u64,
                                    },
                                ));
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
                        state.transform = SpatialTransform::identity();
                        state.transform.translation = root_center;
                        builder
                            .commands
                            .entity(state.entity)
                            .insert((LocalBounds(local_root_bounds), state.transform));
                        let entity = state.entity;
                        let time = builder.current_time;
                        let span = duration.unwrap_or(0.0);
                        builder.commands.queue(move |world: &mut World| {
                            gaanim_animation::updaters::record_layout_bounds(
                                world,
                                entity,
                                time,
                                span,
                                local_root_bounds,
                            );
                        });
                    }
                    if let Some(root_spec) = object_specs.get(&root_source) {
                        // The responsive solve changes the root's bounds, so
                        // replay its authored Drawable transform against the
                        // final box. Otherwise the solve overwrites `at()` and
                        // anchor/edge positioning, or retains a stale pivot.
                        Self::apply_layout(builder, container, root_spec, id_map, frame_bounds);
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
                            let rest = target_box.bounds.center() - parent_center;
                            let Some(current) =
                                builder.states.get(*member).map(|state| state.transform)
                            else {
                                continue;
                            };
                            let translation = Self::keep_layout_offset(
                                builder,
                                tree.source_by_id.get(layout_id).copied(),
                                &current_translations,
                                &current,
                                rest,
                            );
                            let Some(state) = builder.states.get_mut(*member) else {
                                continue;
                            };
                            let mut target = state.transform;
                            target.translation = translation;
                            state.bounds = local_bounds;
                            state.transform = target;
                            let entity = state.entity;
                            builder
                                .commands
                                .entity(entity)
                                .insert(LocalBounds(local_bounds));
                            Self::place_layout_member(builder, entity, target);
                            let time = builder.current_time;
                            let span = duration.unwrap_or(0.0);
                            builder.commands.queue(move |world: &mut World| {
                                gaanim_animation::updaters::record_layout_bounds(
                                    world,
                                    entity,
                                    time,
                                    span,
                                    local_bounds,
                                );
                            });
                            targets.push((*member, target));
                            continue;
                        }
                        let metrics = builder.text_metrics.get(member).copied();
                        let Some((transform, bounds)) = builder
                            .states
                            .get(*member)
                            .map(|state| (state.transform, state.bounds))
                        else {
                            continue;
                        };
                        let mut zero_translation = transform;
                        zero_translation.translation = DVec3::ZERO;
                        // Text sits by its line box, as it was measured, unless
                        // its item sets the height: then its ink is centered.
                        let sized = tree
                            .item_style_by_id
                            .get(layout_id)
                            .is_some_and(|style| style.height.is_some());
                        let mode = tree
                            .texts
                            .get(layout_id)
                            .map(|text| text.spec.flow.text_box)
                            .unwrap_or_default();
                        let local = match (line_extents.get(layout_id), metrics) {
                            (Some(extent), Some(metrics)) if !sized => text_layout_box(
                                bounds,
                                metrics,
                                mode,
                                *extent,
                                cap_heights.get(layout_id).copied(),
                            ),
                            _ => bounds,
                        };
                        let intrinsic = gaanim_layout::transform_bounds(local, &zero_translation);
                        let target_center = target_box.bounds.center() - parent_center;
                        let intrinsic_center = intrinsic.center();
                        let mut target = transform;
                        target.translation = Self::keep_layout_offset(
                            builder,
                            tree.source_by_id.get(layout_id).copied(),
                            &current_translations,
                            &transform,
                            target_center - intrinsic_center,
                        );
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
                        let Some(state) = builder.states.get_mut(*member) else {
                            continue;
                        };
                        state.transform = target;
                        let entity = state.entity;
                        Self::place_layout_member(builder, entity, target);
                    }
                    // What the editor's layout inspector draws for each box:
                    // padding, gap and its children's cells from now on.
                    for (container_id, child_ids) in &tree.children_by_id {
                        let (Some(container_box), Some(entity)) = (
                            resolved.boxes.get(container_id),
                            materialized_by_id
                                .get(container_id)
                                .and_then(|member| builder.states.get(*member))
                                .map(|state| state.entity),
                        ) else {
                            continue;
                        };
                        let Some(snapshot) = tree
                            .source_by_id
                            .get(container_id)
                            .and_then(|source| layout_snapshots.get(source))
                        else {
                            continue;
                        };
                        let center = container_box.bounds.center();
                        let insets = |insets: gaanim_layout::Insets| {
                            [insets.top, insets.right, insets.bottom, insets.left]
                        };
                        let frame = gaanim_scene::LayoutInspectionFrame {
                            kind: match snapshot.spec.kind {
                                gaanim_layout::LayoutNodeKind::Row { .. } => {
                                    gaanim_scene::LayoutInspectionKind::Row
                                }
                                gaanim_layout::LayoutNodeKind::Column { .. } => {
                                    gaanim_scene::LayoutInspectionKind::Column
                                }
                                gaanim_layout::LayoutNodeKind::Grid { .. } => {
                                    gaanim_scene::LayoutInspectionKind::Grid
                                }
                                _ => gaanim_scene::LayoutInspectionKind::Stack,
                            },
                            padding: insets(snapshot.spec.style.padding),
                            gap: snapshot.spec.style.gap,
                            cells: child_ids
                                .iter()
                                .filter_map(|child| {
                                    let bounds = resolved.boxes.get(child)?.bounds;
                                    Some(gaanim_scene::LayoutInspectionCell {
                                        bounds: Bounds3D::new_2d(
                                            bounds.min.x - center.x,
                                            bounds.min.y - center.y,
                                            bounds.max.x - center.x,
                                            bounds.max.y - center.y,
                                        ),
                                        margin: insets(
                                            tree.item_style_by_id
                                                .get(child)
                                                .map(|style| style.margin)
                                                .unwrap_or_default(),
                                        ),
                                    })
                                })
                                .collect(),
                        };
                        let time = builder.current_time;
                        builder.commands.queue(move |world: &mut World| {
                            let Ok(mut entity) = world.get_entity_mut(entity) else {
                                return;
                            };
                            if let Some(mut inspection) =
                                entity.get_mut::<gaanim_scene::LayoutInspection>()
                            {
                                inspection.record(time, frame);
                            } else {
                                let mut inspection = gaanim_scene::LayoutInspection::default();
                                inspection.record(time, frame);
                                entity.insert(inspection);
                            }
                        });
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
                                    sources: Vec::new(),
                                    invert: false,
                                },
                            );
                        }
                    }
                    // Arrangement writes the final transforms. Restore the
                    // layout visible at the current timeline cursor, then let
                    // the regular animation machinery interpolate to the new
                    // arrangement and advance its cursor.
                    for (member, transform) in before {
                        if let Some(state) = builder.states.get_mut(member) {
                            state.transform = transform;
                            let entity = state.entity;
                            Self::place_layout_member(builder, entity, transform);
                        }
                    }
                    let transition_duration = (*duration).unwrap_or(0.0);
                    // Zero-duration clips retain reversible cuts for immediate
                    // reflow; otherwise earlier seeks inherit final positions.
                    let mut animations: Vec<AnimationBuilder> = targets
                        .into_iter()
                        .flat_map(|(target, to)| {
                            [
                                AnimationBuilder {
                                    target,
                                    anim_type: AnimationType::TranslateTo { to: to.translation },
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
                        .collect();
                    for (old, new, entry_pending, instant, own_entry) in text_crossfades {
                        if entry_pending {
                            // The text has not entered yet and its fade-in now
                            // targets the replacement: a crossfade here would
                            // show it before that fade-in starts.
                            if let Some(state) = builder.states.get(old).cloned() {
                                builder.hide_visuals_now(&state);
                            }
                            continue;
                        }
                        if instant {
                            animations.push(AnimationBuilder {
                                target: old,
                                anim_type: AnimationType::FadeOut,
                                duration: 0.0,
                                rate_func: RateFunc::Linear,
                                delay: 0.0,
                            });
                            // The copy exists only from now on; a text that is
                            // itself entering gets its fade-in below.
                            if !own_entry {
                                animations.push(AnimationBuilder {
                                    target: new,
                                    anim_type: AnimationType::FadeIn,
                                    duration: 0.0,
                                    rate_func: RateFunc::Linear,
                                    delay: 0.0,
                                });
                            }
                            continue;
                        }
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
                        // The scene advances its own cursor (or not, with
                        // `advance=False`): the transition only starts here.
                        let start = builder.current_time;
                        builder.play_parallel(animations);
                        builder.current_time = start;
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
                    let start = builder.current_time;
                    builder.play_parallel(animations);
                    builder.current_time = start;
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
                                    interpolation: gaanim_math::ZoomInterpolation::Linear,
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
                            interpolation: gaanim_math::ZoomInterpolation::Linear,
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
                                    trauma: None,
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
                    let from_eye =
                        if (*camera_position - *camera_target).length_squared() <= f64::EPSILON {
                            *eye
                        } else {
                            *camera_position
                        };
                    let to_eye = *eye;
                    let from_target = *camera_target;
                    let to_target = *target;
                    let view_to =
                        gaanim_core::glam::dcamera::rh::view::look_at_mat4(to_eye, to_target, *up);
                    let to_rot = view_to.inverse().to_scale_rotation_translation().1;
                    builder.timeline.add_clip(
                        builder.default_track,
                        builder.current_time,
                        *duration,
                        gaanim_timeline::clip::ClipPayload::Animation(
                            gaanim_timeline::clip::AnimationSpec {
                                target: gaanim_core::ObjectId::from_parts(0, 1),
                                lens: gaanim_timeline::clip::PropertyLensSpec::CameraLookAt {
                                    from_position: from_eye,
                                    from_target,
                                    eye: to_eye,
                                    target: to_target,
                                    up: *up,
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
                                lens: gaanim_timeline::clip::PropertyLensSpec::CameraOrbit {
                                    from_position: *camera_position,
                                    target: *camera_target,
                                    up: gaanim_core::glam::DVec3::Y,
                                    delta_yaw: *delta_yaw,
                                    delta_pitch: *delta_pitch,
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
                Op::SetClip {
                    target,
                    mask,
                    rule,
                    invert,
                } => {
                    let Some(target) = id_map.get(target).copied() else {
                        continue;
                    };
                    let target_leaves = Self::visual_leaf_ids(builder, target);
                    if let Some(mask) = mask {
                        let Some(mask) = id_map.get(mask).copied() else {
                            continue;
                        };
                        let mask_world = Self::mask_path_in_world(builder, mask);
                        let sources: Vec<Entity> = Self::visual_leaf_ids(builder, mask)
                            .into_iter()
                            .filter_map(|id| builder.states.get(id).map(|state| state.entity))
                            .collect();
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
                                    sources: sources.clone(),
                                    invert: *invert,
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
                Op::SetCameraView { target, view } => {
                    let entity_of = |id: &ObjectId| {
                        let id = id_map.get(id).copied()?;
                        builder.states.get(id).map(|state| state.entity)
                    };
                    let Some(screen) = entity_of(target) else {
                        continue;
                    };
                    let Some(view) = view else {
                        builder
                            .commands
                            .entity(screen)
                            .remove::<gaanim_renderer::effects::CameraView>();
                        continue;
                    };
                    let Some(source) = entity_of(&view.source) else {
                        continue;
                    };
                    let exclude = view.exclude.iter().filter_map(entity_of).collect();
                    let zoom = view
                        .zoom
                        .as_ref()
                        .map(|zoom| compile_tracking_scalar(zoom, id_map, &builder.states));
                    builder
                        .commands
                        .entity(screen)
                        .insert(gaanim_renderer::effects::CameraView {
                            source,
                            fit: view.fit,
                            background: view.background.clone(),
                            exclude,
                            zoom,
                            layers: view.layers.clone(),
                        });
                }
                Op::SetViewLayer { target, layer } => {
                    let Some(target) = id_map.get(target).copied() else {
                        continue;
                    };
                    for leaf in Self::visual_leaf_ids(builder, target) {
                        let Some(state) = builder.states.get(leaf) else {
                            continue;
                        };
                        let mut entity = builder.commands.entity(state.entity);
                        match layer {
                            Some(layer) => {
                                entity
                                    .insert(gaanim_renderer::effects::ViewLayer(Arc::clone(layer)));
                            }
                            None => {
                                entity.remove::<gaanim_renderer::effects::ViewLayer>();
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
                Op::AttachToGroupLocal { group, child } => {
                    if let (Some(group), Some(child)) =
                        (id_map.get(group).copied(), id_map.get(child).copied())
                        && builder.states.get(group).is_some()
                        && builder.states.get(child).is_some()
                    {
                        builder
                            .add_to_group_local(MobjectRef { id: group }, MobjectRef { id: child });
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

                Op::SetPropertyBinding { target, sources } => {
                    if let Some(target) = id_map.get(target).copied() {
                        builder.compile_property_binding(target, sources, id_map);
                    }
                }
                Op::ClearPropertyBinding { target, channel } => {
                    if let Some(target) = id_map.get(target).copied() {
                        builder.clear_compiled_property_binding(target, *channel);
                    }
                }
                // -- Reactive ops --
                Op::AttachUpdater {
                    target,
                    preset: crate::canvas::UpdaterPreset::Procedural(layer),
                } => {
                    if let Some(target_id) = id_map.get(target).copied()
                        && let Some(st) = builder.states.get(target_id)
                    {
                        let layer = layer.clone();
                        let start = builder.current_time;
                        builder.commands.entity(st.entity).queue(
                            move |mut entity: bevy::prelude::EntityWorldMut| {
                                if let Some(mut motion) =
                                    entity.get_mut::<gaanim_animation::ProceduralMotion>()
                                {
                                    motion.push(layer, start);
                                } else {
                                    let mut motion = gaanim_animation::ProceduralMotion::default();
                                    motion.push(layer, start);
                                    entity.insert(motion);
                                }
                            },
                        );
                    }
                }
                Op::AttachUpdater {
                    target,
                    preset: crate::canvas::UpdaterPreset::DashFlow { speed },
                } => {
                    if let Some(target_id) = id_map.get(target).copied() {
                        let (speed, start) = (*speed, builder.current_time);
                        for (entity, _) in Self::hierarchy_entities(builder, target_id) {
                            builder.commands.entity(entity).queue(
                                move |mut entity: bevy::prelude::EntityWorldMut| {
                                    if let Some(mut flow) =
                                        entity.get_mut::<gaanim_animation::DashFlow>()
                                    {
                                        flow.push(speed, start);
                                    } else {
                                        let mut flow = gaanim_animation::DashFlow::default();
                                        flow.push(speed, start);
                                        entity.insert(flow);
                                    }
                                },
                            );
                        }
                    }
                }
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
                        let end = builder.current_time;
                        for (entity, _) in Self::hierarchy_entities(builder, target_id) {
                            builder.commands.entity(entity).queue(
                                move |mut entity: bevy::prelude::EntityWorldMut| {
                                    if let Some(mut flow) =
                                        entity.get_mut::<gaanim_animation::DashFlow>()
                                    {
                                        flow.stop_at(end);
                                    }
                                },
                            );
                        }
                        if let Some(st) = builder.states.get(target_id) {
                            let end = builder.current_time;
                            builder.commands.entity(st.entity).queue(
                                move |mut entity: bevy::prelude::EntityWorldMut| {
                                    if let Some(mut motion) =
                                        entity.get_mut::<gaanim_animation::ProceduralMotion>()
                                    {
                                        motion.stop_at(end);
                                    }
                                },
                            );
                        }
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
                                    id_map,
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
                            compile_tracking_endpoint(from, id_map, &builder.states),
                            compile_tracking_endpoint(to, id_map, &builder.states),
                        );
                        builder.commands.entity(st.entity).insert(line);
                    }
                }

                Op::AttachTrackingConnector {
                    target,
                    points,
                    head_length,
                    head_width,
                    body_width,
                    max_head_ratio,
                } => {
                    if let Some(target_id) = id_map.get(target).copied()
                        && let Some(st) = builder.states.get(target_id)
                    {
                        builder.commands.entity(st.entity).insert(
                            gaanim_animation::updaters::TrackingConnector {
                                points: points
                                    .iter()
                                    .map(|p| compile_tracking_endpoint(p, id_map, &builder.states))
                                    .collect(),
                                head_length: *head_length,
                                head_width: *head_width,
                                body_width: *body_width,
                                max_head_ratio: *max_head_ratio,
                                progress: 1.0,
                            },
                        );
                        builder.connectors.insert(target_id);
                    }
                }
                Op::AttachLayoutBackground {
                    target,
                    container,
                    radius,
                } => {
                    if let (Some(target), Some(container)) = (
                        id_map.get(target).and_then(|id| builder.states.get(*id)),
                        id_map.get(container).and_then(|id| builder.states.get(*id)),
                    ) {
                        builder.commands.entity(target.entity).insert(
                            gaanim_animation::updaters::LayoutBackground {
                                container: container.entity,
                                radius: *radius,
                            },
                        );
                    }
                }
                Op::RecordLayoutZones { zones } => {
                    let start = builder.current_time;
                    let records: Vec<_> = zones
                        .iter()
                        .map(|(name, bounds)| gaanim_scene::LayoutZoneRecord {
                            name: name.clone(),
                            bounds: *bounds,
                            // Set when the segment ends.
                            segment: usize::MAX,
                            start,
                            end: f64::INFINITY,
                        })
                        .collect();
                    builder.commands.queue(move |world: &mut World| {
                        world
                            .get_resource_or_insert_with(gaanim_scene::LayoutZones::default)
                            .0
                            .extend(records);
                    });
                }
                Op::AttachSurroundingRect {
                    target,
                    sources,
                    padding,
                    corner_radius,
                } => {
                    let compiled = sources
                        .iter()
                        .flat_map(|source| match source {
                            crate::anim::BoundsTarget::Drawable(source) => {
                                id_map.get(source).copied().into_iter().collect::<Vec<_>>()
                            }
                            crate::anim::BoundsTarget::TextSelection {
                                target,
                                fragment,
                                occurrence,
                            } => Self::fragment_child_ids(
                                builder,
                                id_map,
                                *target,
                                fragment,
                                *occurrence,
                            ),
                        })
                        .collect::<Vec<_>>();
                    if let Some(target_id) = id_map.get(target).copied()
                        && let Some(state) = builder.states.get(target_id)
                    {
                        builder.commands.entity(state.entity).insert(
                            gaanim_animation::SurroundingRect::new(
                                compiled,
                                *padding,
                                *corner_radius,
                            ),
                        );
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
                        let from = compile_tracking_endpoint(from, id_map, &builder.states);
                        let to = compile_tracking_endpoint(to, id_map, &builder.states);
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
                    side,
                    line_width,
                    extension_dash,
                } => {
                    let from = compile_tracking_endpoint(from, id_map, &builder.states);
                    let to = compile_tracking_endpoint(to, id_map, &builder.states);
                    let authored_offset = *offset;
                    let side = *side;
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
                                let (from, to) = (endpoint_position(&from), endpoint_position(&to));
                                // The side is a scene direction, so resolve it before
                                // moving the endpoints into the target's local space.
                                let offset = gaanim_animation::DimensionSide::resolve(
                                    side,
                                    authored_offset,
                                    from.truncate(),
                                    to.truncate(),
                                );
                                let from = gaanim_animation::tracking_world_to_local(
                                    target_entity,
                                    from,
                                    world,
                                );
                                let to = gaanim_animation::tracking_world_to_local(
                                    target_entity,
                                    to,
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
                            from: compile_tracking_endpoint(from, id_map, &builder.states),
                            to: compile_tracking_endpoint(to, id_map, &builder.states),
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
                    side,
                    gap,
                    orientation,
                    clear_label_width,
                } => {
                    if let (Some(target_id), Some(label_id)) =
                        (id_map.get(target).copied(), id_map.get(label).copied())
                        && let (Some(target_state), Some(label_state)) =
                            (builder.states.get(target_id), builder.states.get(label_id))
                    {
                        builder.commands.entity(target_state.entity).insert(
                            DimensionLabelPlacement {
                                label: label_state.entity,
                                from: compile_tracking_endpoint(from, id_map, &builder.states),
                                to: compile_tracking_endpoint(to, id_map, &builder.states),
                                offset: *offset,
                                side: *side,
                                gap: *gap,
                                orientation: *orientation,
                                clear_label_width: *clear_label_width,
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
                    let vertex = compile_tracking_endpoint(vertex, id_map, &builder.states);
                    let from = compile_tracking_ray(from, id_map, &builder.states);
                    let to = compile_tracking_ray(to, id_map, &builder.states);
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
                            vertex: compile_tracking_endpoint(vertex, id_map, &builder.states),
                            from: compile_tracking_ray(from, id_map, &builder.states),
                            to: compile_tracking_ray(to, id_map, &builder.states),
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
                                vertex: compile_tracking_endpoint(vertex, id_map, &builder.states),
                                from: compile_tracking_ray(from, id_map, &builder.states),
                                to: compile_tracking_ray(to, id_map, &builder.states),
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
                                from: compile_tracking_endpoint(from, id_map, &builder.states),
                                to: compile_tracking_endpoint(to, id_map, &builder.states),
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

                Op::AttachSampledSeries { target, driver } => {
                    if let Some(target_id) = id_map.get(target).copied()
                        && let Some(target_st) = builder.states.get(target_id)
                    {
                        let driver = driver.clone().starting_at(builder.current_time);
                        let entity = target_st.entity;
                        builder.commands.queue(move |world: &mut World| {
                            let Ok(mut target) = world.get_entity_mut(entity) else {
                                return;
                            };
                            match target.get_mut::<gaanim_animation::SampledSeriesDrivers>() {
                                Some(mut drivers) => drivers.attach(driver),
                                None => {
                                    target.insert(gaanim_animation::SampledSeriesDrivers::from(
                                        driver,
                                    ));
                                }
                            }
                        });
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
                Op::AttachReactiveArrowField2D {
                    target,
                    function,
                    position,
                    map,
                    options,
                    color_range,
                } => {
                    let parameters = compile_function_parameters(function, id_map, &builder.states);
                    if let Some(target_id) = id_map.get(target).copied()
                        && let Some(state) = builder.states.get(target_id)
                    {
                        let function = function.clone();
                        let position = *position;
                        let map = map.clone();
                        let options = options.clone();
                        let color_range = *color_range;
                        let updater = Updater::new(move |_dt, _elapsed, entity, world| {
                            let path = runtime_field_2d(&function, &parameters, world, position)
                                .and_then(|vector| {
                                    reactive_arrow_path_2d(&map, position, vector, &options)
                                        .map(|path| (vector, path))
                                });
                            let Some((vector, path)) = path else {
                                let empty = Arc::new(BezPath::new());
                                if let Some(mut visible) =
                                    world.get_mut::<gaanim_scene::Path2D>(entity)
                                {
                                    visible.0 = empty.clone();
                                }
                                if let Some(mut source) =
                                    world.get_mut::<gaanim_scene::PathSource>(entity)
                                {
                                    source.0 = empty;
                                }
                                return true;
                            };
                            let magnitude = (vector[0] * vector[0] + vector[1] * vector[1]).sqrt();
                            let color = reactive_field_color(
                                magnitude,
                                color_range,
                                options.color,
                                options.colormap.as_ref(),
                                1.0,
                            );
                            let path = Arc::new(path);
                            if let Some(mut visible) = world.get_mut::<gaanim_scene::Path2D>(entity)
                            {
                                visible.0 = path.clone();
                            }
                            if let Some(mut source) =
                                world.get_mut::<gaanim_scene::PathSource>(entity)
                            {
                                source.0 = path;
                            }
                            if let Some(mut stroke) = world.get_mut::<StrokeBrush>(entity) {
                                stroke.brush = Some(gaanim_core::peniko::Brush::Solid(color));
                            }
                            true
                        })
                        .starting_at(builder.current_time);
                        builder.commands.entity(state.entity).insert(updater);
                    }
                }
                Op::AttachReactiveArrowField3D {
                    target,
                    function,
                    resolution,
                    map,
                    options,
                    color_range,
                } => {
                    let parameters = compile_function_parameters(function, id_map, &builder.states);
                    if let Some(target_id) = id_map.get(target).copied()
                        && let Some(state) = builder.states.get(target_id)
                    {
                        let function = function.clone();
                        let resolution = *resolution;
                        let map = map.clone();
                        let options = options.clone();
                        let color_range = *color_range;
                        let updater = Updater::new(move |_dt, _elapsed, entity, world| {
                            let lines = runtime_model_3d(function.clone(), &parameters, world)
                                .and_then(|field| {
                                    reactive_arrow_lines_3d(
                                        &field,
                                        &map,
                                        resolution,
                                        &options,
                                        color_range,
                                    )
                                })
                                .unwrap_or(gaanim_scene::LineListData {
                                    points: Vec::new(),
                                    indices: None,
                                    strip: false,
                                    color: PenikoColor::WHITE,
                                    colors: None,
                                });
                            if let Some(mut visible) =
                                world.get_mut::<gaanim_scene::LineListData>(entity)
                            {
                                *visible = lines.clone();
                            }
                            if let Some(mut source) =
                                world.get_mut::<gaanim_scene::LineListSource>(entity)
                            {
                                source.0 = lines;
                            }
                            true
                        })
                        .starting_at(builder.current_time);
                        builder.commands.entity(state.entity).insert(updater);
                    }
                }
                Op::AttachReactiveStreamLine2D {
                    target,
                    function,
                    seed,
                    map,
                    style,
                    color_range,
                } => {
                    let parameters = compile_function_parameters(function, id_map, &builder.states);
                    if let Some(target_id) = id_map.get(target).copied()
                        && let Some(state) = builder.states.get(target_id)
                    {
                        let function = function.clone();
                        let seed = *seed;
                        let map = map.clone();
                        let style = style.clone();
                        let color_range = *color_range;
                        let updater = Updater::new(move |_dt, _elapsed, entity, world| {
                            let Some(field) =
                                runtime_model_2d(function.clone(), &parameters, world)
                            else {
                                let empty = Arc::new(BezPath::new());
                                if let Some(mut visible) =
                                    world.get_mut::<gaanim_scene::Path2D>(entity)
                                {
                                    visible.0 = empty.clone();
                                }
                                if let Some(mut source) =
                                    world.get_mut::<gaanim_scene::PathSource>(entity)
                                {
                                    source.0 = empty;
                                }
                                return true;
                            };
                            let domains = [map.x.domain(), map.y.domain()];
                            let Some(line) = field.integrate(seed, domains, style.integration)
                            else {
                                let empty = Arc::new(BezPath::new());
                                if let Some(mut visible) =
                                    world.get_mut::<gaanim_scene::Path2D>(entity)
                                {
                                    visible.0 = empty.clone();
                                }
                                if let Some(mut source) =
                                    world.get_mut::<gaanim_scene::PathSource>(entity)
                                {
                                    source.0 = empty;
                                }
                                return true;
                            };
                            let mut path = BezPath::new();
                            for (index, point) in line.points.iter().enumerate() {
                                let Ok(local) = map.data_to_local(point[0], point[1]) else {
                                    return true;
                                };
                                if index == 0 {
                                    path.move_to(local);
                                } else {
                                    path.line_to(local);
                                }
                            }
                            let speed = line.speeds.iter().sum::<f64>() / line.speeds.len() as f64;
                            let color = reactive_field_color(
                                speed,
                                color_range,
                                style.color,
                                style.colormap.as_ref(),
                                style.opacity,
                            );
                            let path = Arc::new(path);
                            if let Some(mut visible) = world.get_mut::<gaanim_scene::Path2D>(entity)
                            {
                                visible.0 = path.clone();
                            }
                            if let Some(mut source) =
                                world.get_mut::<gaanim_scene::PathSource>(entity)
                            {
                                source.0 = path;
                            }
                            if let Some(mut stroke) = world.get_mut::<StrokeBrush>(entity) {
                                stroke.brush = Some(gaanim_core::peniko::Brush::Solid(color));
                            }
                            true
                        })
                        .starting_at(builder.current_time);
                        builder.commands.entity(state.entity).insert(updater);
                    }
                }
                Op::AttachReactiveStreamLine3D {
                    target,
                    function,
                    seed,
                    map,
                    style,
                    color_range,
                } => {
                    let parameters = compile_function_parameters(function, id_map, &builder.states);
                    if let Some(target_id) = id_map.get(target).copied()
                        && let Some(state) = builder.states.get(target_id)
                    {
                        let function = function.clone();
                        let seed = *seed;
                        let map = map.clone();
                        let style = style.clone();
                        let color_range = *color_range;
                        let updater = Updater::new(move |_dt, _elapsed, entity, world| {
                            let Some(field) =
                                runtime_model_3d(function.clone(), &parameters, world)
                            else {
                                let lines = gaanim_scene::LineListData {
                                    points: Vec::new(),
                                    indices: None,
                                    strip: true,
                                    color: PenikoColor::WHITE,
                                    colors: None,
                                };
                                if let Some(mut visible) =
                                    world.get_mut::<gaanim_scene::LineListData>(entity)
                                {
                                    *visible = lines.clone();
                                }
                                if let Some(mut source) =
                                    world.get_mut::<gaanim_scene::LineListSource>(entity)
                                {
                                    source.0 = lines;
                                }
                                return true;
                            };
                            let domains = [map.x.domain(), map.y.domain(), map.z.domain()];
                            let Some(line) = field.integrate(seed, domains, style.integration)
                            else {
                                let lines = gaanim_scene::LineListData {
                                    points: Vec::new(),
                                    indices: None,
                                    strip: true,
                                    color: PenikoColor::WHITE,
                                    colors: None,
                                };
                                if let Some(mut visible) =
                                    world.get_mut::<gaanim_scene::LineListData>(entity)
                                {
                                    *visible = lines.clone();
                                }
                                if let Some(mut source) =
                                    world.get_mut::<gaanim_scene::LineListSource>(entity)
                                {
                                    source.0 = lines;
                                }
                                return true;
                            };
                            let points = line
                                .points
                                .iter()
                                .filter_map(|point| map.data_to_local(*point).ok())
                                .map(|point| point.map(|value| value as f32))
                                .collect::<Vec<_>>();
                            if points.len() < 2 {
                                return true;
                            }
                            let colors = line
                                .speeds
                                .iter()
                                .map(|speed| {
                                    let rgba = reactive_field_color(
                                        *speed,
                                        color_range,
                                        style.color,
                                        style.colormap.as_ref(),
                                        style.opacity,
                                    )
                                    .to_rgba8();
                                    [
                                        f32::from(rgba.r) / 255.0,
                                        f32::from(rgba.g) / 255.0,
                                        f32::from(rgba.b) / 255.0,
                                        f32::from(rgba.a) / 255.0,
                                    ]
                                })
                                .collect();
                            let lines = gaanim_scene::LineListData {
                                points,
                                indices: None,
                                strip: true,
                                color: PenikoColor::WHITE,
                                colors: Some(colors),
                            };
                            if let Some(mut visible) =
                                world.get_mut::<gaanim_scene::LineListData>(entity)
                            {
                                *visible = lines.clone();
                            }
                            if let Some(mut source) =
                                world.get_mut::<gaanim_scene::LineListSource>(entity)
                            {
                                source.0 = lines;
                            }
                            true
                        })
                        .starting_at(builder.current_time);
                        builder.commands.entity(state.entity).insert(updater);
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

    /// Rewrites authored morph pairs to runtime object ids, dropping pairs
    /// whose drawables were never compiled.
    fn runtime_transition(
        transition: &gaanim_timeline::transition::TransitionType,
        id_map: &HashMap<ObjectId, ObjectId>,
    ) -> gaanim_timeline::transition::TransitionType {
        use gaanim_timeline::transition::{MorphMapping, TransitionType};
        // Easing/overlay wrappers and drawable iris outlines carry authored ids too.
        match transition {
            TransitionType::Styled {
                base,
                easing,
                overlay,
            } => {
                return TransitionType::Styled {
                    base: Box::new(Self::runtime_transition(base, id_map)),
                    easing: easing.clone(),
                    overlay: overlay.clone(),
                };
            }
            TransitionType::Iris {
                duration,
                center,
                shape: gaanim_timeline::transition::IrisShape::Drawable(id),
            } => {
                return TransitionType::Iris {
                    duration: *duration,
                    center: *center,
                    shape: id_map
                        .get(id)
                        .map_or(gaanim_timeline::transition::IrisShape::Circle, |id| {
                            gaanim_timeline::transition::IrisShape::Drawable(*id)
                        }),
                };
            }
            _ => {}
        }
        let TransitionType::Morph { duration, mappings } = transition else {
            return transition.clone();
        };
        TransitionType::Morph {
            duration: *duration,
            mappings: mappings
                .iter()
                .filter_map(|mapping| {
                    Some(MorphMapping {
                        source: *id_map.get(&mapping.source)?,
                        target: *id_map.get(&mapping.target)?,
                        property: mapping.property.clone(),
                    })
                })
                .collect(),
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
                    | AnimationType::FadeTransform { target }
                    | AnimationType::TextTransition {
                        target,
                        copy: false,
                        ..
                    } => {
                        targets.insert(*target);
                    }
                    _ => {}
                },
                Op::Play(anims) | Op::Launch(anims) => {
                    for anim in anims {
                        match &anim.anim_type {
                            AnimationType::Transform { target }
                            | AnimationType::ReplacementTransform { target }
                            | AnimationType::FadeTransform { target }
                            | AnimationType::TextTransition {
                                target,
                                copy: false,
                                ..
                            } => {
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

    /// A replacing text transition hands the scene over to its target text.
    /// Later operations on the source handle continue on the target, as a
    /// transformed object keeps its identity (chained `transform_to`, moves).
    fn continue_text_transition_identity(
        anim: &AnimationBuilder,
        id_map: &mut HashMap<ObjectId, ObjectId>,
    ) {
        if let AnimationType::TextTransition {
            target,
            copy: false,
            ..
        } = &anim.anim_type
            && let Some(&actual) = id_map.get(target)
        {
            id_map.insert(anim.target, actual);
        }
    }

    /// Screens whose first pop in `ops` is a `pop_out`, with their companions:
    /// that pop is their entry, so their declaration does not show them.
    fn camera_view_entries(ops: &[Op]) -> Vec<ObjectId> {
        let mut seen = HashSet::new();
        let mut entries = Vec::new();
        for op in ops {
            let anims: &[AnimationBuilder] = match op {
                Op::Animate { anim, active: true } => std::slice::from_ref(anim),
                Op::Play(anims) | Op::Launch(anims) => anims,
                _ => &[],
            };
            for anim in anims {
                if let AnimationType::CameraViewPop {
                    out, companions, ..
                } = &anim.anim_type
                    && seen.insert(anim.target)
                    && *out
                {
                    entries.push(anim.target);
                    entries.extend(companions.iter().copied());
                }
            }
        }
        entries
    }

    /// Whether `target`'s first fade-in in this segment comes after the op at
    /// `index`, i.e. the object is still waiting for that entry animation.
    fn fade_in_pending(ops: &[Op], index: usize, target: ObjectId) -> bool {
        let fades_in = |op: &Op| {
            let anims: &[AnimationBuilder] = match op {
                Op::Animate { anim, active: true } => std::slice::from_ref(anim),
                Op::Play(anims) | Op::Launch(anims) => anims,
                _ => &[],
            };
            anims.iter().any(|anim| {
                anim.target == target
                    && matches!(
                        anim.anim_type,
                        AnimationType::FadeIn | AnimationType::FadeInFrom { .. }
                    )
            })
        };
        !ops[..index].iter().any(fades_in) && ops[index + 1..].iter().any(fades_in)
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

    /// Schedule a static pan+zoom. Linear mode keeps the historical pair of
    /// independent position/zoom clips; exponential mode couples them so the
    /// view scales about a fixed point.
    fn add_camera_pan_zoom(
        builder: &mut SceneBuilder,
        start: f64,
        anim: &AnimationBuilder,
        (from_position, to_position): (DVec3, DVec3),
        (from_zoom, to_zoom): (f64, f64),
        interpolation: gaanim_math::ZoomInterpolation,
    ) {
        use gaanim_timeline::clip::PropertyLensSpec;
        match interpolation {
            gaanim_math::ZoomInterpolation::Linear => {
                Self::add_camera_lens(
                    builder,
                    start,
                    anim,
                    PropertyLensSpec::CameraPosition {
                        from: from_position,
                        to: to_position,
                    },
                );
                Self::add_camera_lens(
                    builder,
                    start,
                    anim,
                    PropertyLensSpec::CameraZoom {
                        from: from_zoom,
                        to: to_zoom,
                        interpolation,
                    },
                );
            }
            gaanim_math::ZoomInterpolation::Exponential => Self::add_camera_lens(
                builder,
                start,
                anim,
                PropertyLensSpec::CameraPanZoom {
                    from_position,
                    to_position,
                    from_zoom,
                    to_zoom,
                    interpolation,
                },
            ),
        }
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
            AnimationType::CameraState { from, to } => {
                if let gaanim_animation::CameraStateSource::Captured(id) = from {
                    builder.timeline.add_clip(
                        builder.default_track,
                        start + anim.delay.max(0.0),
                        0.0,
                        gaanim_timeline::clip::ClipPayload::CameraCapture { id: *id },
                    );
                }
                Self::add_camera_lens(
                    builder,
                    start,
                    anim,
                    PropertyLensSpec::CameraState {
                        from: *from,
                        to: *to,
                    },
                );
                if let gaanim_animation::CameraStateSource::Concrete(pose) = to {
                    *camera_position = pose.position;
                    *camera_rotation = pose.rotation;
                    *camera_target = pose.target;
                    *camera_up = pose.up;
                    match pose.projection {
                        gaanim_math::Projection::Orthographic { zoom } => {
                            *camera_zoom = zoom;
                            *camera_fov = None;
                        }
                        gaanim_math::Projection::Perspective { fov_y, near, far } => {
                            *camera_fov = Some((fov_y, near, far));
                        }
                    }
                }
            }
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
            AnimationType::CameraZoom { to, interpolation } => {
                Self::add_camera_lens(
                    builder,
                    start,
                    anim,
                    PropertyLensSpec::CameraZoom {
                        from: *camera_zoom,
                        to: *to,
                        interpolation: *interpolation,
                    },
                );
                *camera_zoom = *to;
            }
            AnimationType::CameraZoomSource { to, interpolation } => {
                Self::add_camera_lens(
                    builder,
                    start,
                    anim,
                    PropertyLensSpec::CameraZoomSource {
                        from: *camera_zoom,
                        to: compile_tracking_scalar(to, id_map, &builder.states),
                        interpolation: *interpolation,
                    },
                );
                // A constant target is the hand-off zoom for later camera clips.
                if let Some(zoom) = to.constant_value() {
                    *camera_zoom = zoom;
                }
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
            AnimationType::CameraFrame {
                target,
                margin,
                interpolation,
            } => {
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
                Self::add_camera_pan_zoom(
                    builder,
                    start,
                    anim,
                    (*camera_position, center),
                    (*camera_zoom, zoom),
                    *interpolation,
                );
                *camera_position = center;
                *camera_zoom = zoom;
            }
            AnimationType::CameraFrameMany {
                targets,
                margins,
                dynamic,
                interpolation,
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
                            interpolation: *interpolation,
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
                    Self::add_camera_pan_zoom(
                        builder,
                        start,
                        anim,
                        (*camera_position, center),
                        (*camera_zoom, zoom),
                        *interpolation,
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
                trauma,
            } => Self::add_camera_lens(
                builder,
                start,
                anim,
                PropertyLensSpec::CameraShake {
                    origin: *camera_position,
                    amplitude: *amplitude,
                    frequency: *frequency,
                    trauma: *trauma,
                },
            ),
            AnimationType::CameraLookAt { eye, target, up } => {
                let from_position =
                    if (*camera_position - *camera_target).length_squared() <= f64::EPSILON {
                        *eye
                    } else {
                        *camera_position
                    };
                Self::add_camera_lens(
                    builder,
                    start,
                    anim,
                    PropertyLensSpec::CameraLookAt {
                        from_position,
                        from_target: *camera_target,
                        eye: *eye,
                        target: *target,
                        up: *up,
                    },
                );
                let to_rotation =
                    gaanim_core::glam::dcamera::rh::view::look_at_mat4(*eye, *target, *up)
                        .inverse()
                        .to_scale_rotation_translation()
                        .1;
                *camera_position = *eye;
                *camera_target = *target;
                *camera_up = *up;
                *camera_rotation = to_rotation;
            }
            AnimationType::CameraLookAtSource { eye, target, up } => {
                let compiled_eye = compile_tracking_endpoint(eye, id_map, &builder.states);
                let compiled_target = compile_tracking_endpoint(target, id_map, &builder.states);
                let from_position =
                    if (*camera_position - *camera_target).length_squared() <= f64::EPSILON {
                        match &compiled_eye {
                            gaanim_animation::TrackingEndpoint::Static(eye) => *eye,
                            _ => *camera_target + DVec3::Z * 10.0,
                        }
                    } else {
                        *camera_position
                    };
                Self::add_camera_lens(
                    builder,
                    start,
                    anim,
                    PropertyLensSpec::CameraLookAtSource {
                        from_position,
                        from_target: *camera_target,
                        from_rotation: *camera_rotation,
                        eye: compiled_eye.clone(),
                        target: compiled_target.clone(),
                        up: *up,
                    },
                );
                *camera_up = *up;
                if let (
                    gaanim_animation::TrackingEndpoint::Static(eye),
                    gaanim_animation::TrackingEndpoint::Static(target),
                ) = (compiled_eye, compiled_target)
                {
                    let view = gaanim_core::glam::dcamera::rh::view::look_at_mat4(eye, target, *up);
                    *camera_position = eye;
                    *camera_target = target;
                    *camera_rotation = view.inverse().to_scale_rotation_translation().1;
                }
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
                    PropertyLensSpec::CameraOrbit {
                        from_position: *camera_position,
                        target: *camera_target,
                        up: *camera_up,
                        delta_yaw: *delta_yaw,
                        delta_pitch: *delta_pitch,
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

    /// Uniform scale an imported SVG root declares through its layout.
    fn svg_declared_scale(spec: &ObjectSpec) -> f64 {
        let mut scale = DVec3::ONE;
        for op in &spec.layout_ops {
            match op {
                LayoutOp::SetScale(factor) => scale = DVec3::splat(*factor),
                LayoutOp::SetScale3D(value) => scale = *value,
                LayoutOp::ScaleBy(value) => scale *= *value,
                _ => {}
            }
        }
        (scale.x.abs() * scale.y.abs()).sqrt()
    }

    /// Strokes are drawn in scene units whatever the drawable's scale, so
    /// the widths an imported SVG declares itself are multiplied by its
    /// root's declared scale: they keep scaling with the drawing. Widths set
    /// with `stroke` on the SVG or one of its parts are already in scene units.
    fn scene_unit_svg_strokes(
        builder: &mut SceneBuilder,
        root: ObjectId,
        object_specs: &HashMap<ObjectId, ObjectSpec>,
        id_map: &HashMap<ObjectId, ObjectId>,
    ) {
        let Some(root_spec) = object_specs.get(&root) else {
            return;
        };
        let scale = Self::svg_declared_scale(root_spec);
        if !scale.is_finite() || scale <= 1.0e-12 || (scale - 1.0).abs() < 1.0e-12 {
            return;
        }
        let paths: Vec<ObjectId> = object_specs
            .values()
            .filter(|spec| {
                spec.svg_owner == Some(root)
                    && !spec.stroke_overridden
                    && matches!(spec.kind, SpawnKind::SvgPath(_))
            })
            .filter_map(|spec| id_map.get(&spec.id).copied())
            .collect();
        for path in paths {
            if let Some(state) = builder.states.get_mut(path) {
                state.stroke.style.width *= scale;
                builder
                    .commands
                    .entity(state.entity)
                    .insert(state.stroke.clone());
            }
        }
    }

    fn remap_anim(
        anim: &AnimationBuilder,
        id_map: &HashMap<ObjectId, ObjectId>,
        object_specs: &HashMap<ObjectId, ObjectSpec>,
    ) -> Option<AnimationBuilder> {
        let target = if anim.anim_type.is_camera() {
            anim.target
        } else {
            *id_map.get(&anim.target)?
        };
        let anim_type = match &anim.anim_type {
            AnimationType::Properties(properties) => {
                let mut properties = properties.clone();
                if let Some(crate::anim::PropertyTranslation::ToAnchorPoint(point)) =
                    &mut properties.translation
                {
                    point.object = *id_map.get(&point.object)?;
                }
                for source in &mut properties.source_targets {
                    for (_, native) in &mut source.parameters {
                        *native = *id_map.get(native).unwrap_or(native);
                    }
                }
                AnimationType::Properties(properties)
            }
            AnimationType::PropertySource(source) => {
                let mut source = source.clone();
                for (_, native) in &mut source.parameters {
                    *native = *id_map.get(native).unwrap_or(native);
                }
                AnimationType::PropertySource(source)
            }
            AnimationType::CameraFrame {
                target,
                margin,
                interpolation,
            } => AnimationType::CameraFrame {
                target: *id_map.get(target)?,
                margin: *margin,
                interpolation: *interpolation,
            },
            AnimationType::CameraFrameMany {
                targets,
                margins,
                dynamic,
                interpolation,
            } => AnimationType::CameraFrameMany {
                targets: targets
                    .iter()
                    .filter_map(|target| id_map.get(target).copied())
                    .collect(),
                margins: *margins,
                dynamic: *dynamic,
                interpolation: *interpolation,
            },
            AnimationType::CameraFollow { target } => AnimationType::CameraFollow {
                target: *id_map.get(target)?,
            },
            AnimationType::CameraViewZoomTo { screen, zoom, fit } => {
                AnimationType::CameraViewZoomTo {
                    screen: *id_map.get(screen)?,
                    zoom: *zoom,
                    fit: *fit,
                }
            }
            AnimationType::CameraViewPop {
                frame,
                focus,
                zoom,
                out,
                companions,
            } => AnimationType::CameraViewPop {
                frame: *id_map.get(frame)?,
                focus: match focus {
                    Some((object, normalized, offset)) => {
                        Some((*id_map.get(object)?, *normalized, *offset))
                    }
                    None => None,
                },
                zoom: match zoom {
                    Some((signal, logarithm)) => Some((*id_map.get(signal)?, *logarithm)),
                    None => None,
                },
                out: *out,
                companions: companions
                    .iter()
                    .filter_map(|id| id_map.get(id).copied())
                    .collect(),
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
            // A marker draws behind the glyphs at the text's stack level.
            AnimationType::TextSelection {
                fragment,
                occurrence,
                effect: crate::anim::TextSelectionEffect::Marker(style),
            } => AnimationType::TextSelection {
                fragment: fragment.clone(),
                occurrence: *occurrence,
                effect: crate::anim::TextSelectionEffect::Marker(style.clone().with_text_z_index(
                    crate::builder::text_marker::stacked_spec_z_index(object_specs, anim.target),
                )),
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
            AnimationType::SurroundingRectRetarget { from, to } => {
                let remap_target = |target: &crate::anim::BoundsTarget| match target {
                    crate::anim::BoundsTarget::Drawable(id) => id_map
                        .get(id)
                        .copied()
                        .map(crate::anim::BoundsTarget::Drawable),
                    crate::anim::BoundsTarget::TextSelection {
                        target,
                        fragment,
                        occurrence,
                    } => id_map.get(target).copied().map(|target| {
                        crate::anim::BoundsTarget::TextSelection {
                            target,
                            fragment: fragment.clone(),
                            occurrence: *occurrence,
                        }
                    }),
                };
                AnimationType::SurroundingRectRetarget {
                    from: from.iter().filter_map(remap_target).collect(),
                    to: to.iter().filter_map(remap_target).collect(),
                }
            }
            AnimationType::MoveAlongPath {
                path,
                path_target,
                follow,
            } => AnimationType::MoveAlongPath {
                path: path.clone(),
                path_target: path_target.map(|id| *id_map.get(&id).unwrap_or(&id)),
                follow: *follow,
            },
            AnimationType::TranslateToAnchorPoint { point } => {
                let mut point = *point;
                point.object = *id_map.get(&point.object)?;
                AnimationType::TranslateToAnchorPoint { point }
            }
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

    /// Resolve `write(by=...)` into glyph groups that follow the segmentation
    /// of `text.words`, `text.lines`, and `text.parts`.
    fn resolve_reveal_groups(
        builder: &SceneBuilder,
        object_specs: &HashMap<ObjectId, ObjectSpec>,
        authored_target: ObjectId,
        anim: &mut AnimationBuilder,
    ) {
        let AnimationType::Write { config } = &mut anim.anim_type else {
            return;
        };
        if config.reveal_unit == TextRevealUnit::Grapheme {
            return;
        }
        let Some(SpawnKind::Text(text)) = object_specs.get(&authored_target).map(|spec| &spec.kind)
        else {
            return;
        };
        let Some(state) = builder.states.get(anim.target) else {
            return;
        };
        let glyphs: Vec<(ObjectId, char)> = state
            .child_spans
            .iter()
            .map(|child| (child.id, child.span.character))
            .collect();
        config.groups = reveal_groups(&glyphs, &text.visible_char_units(config.reveal_unit));
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
        let mref = match &spec.kind {
            SpawnKind::FillLevelOutline { mask } => {
                let b = builder.svg_path(&gaanim_objects::prelude::SvgPath {
                    id: "FillLevelOutline".into(),
                    path: BezPath::new(),
                    bounds: Bounds3D::default(),
                    fill: None,
                    stroke: StrokeBrush::transparent(),
                });
                let mr = Self::finish_spawn_builder(b, spec);
                let sources = id_map
                    .get(mask)
                    .into_iter()
                    .flat_map(|id| Self::visual_leaf_ids(builder, *id))
                    .filter_map(|id| builder.states.get(id).map(|state| state.entity))
                    .collect();
                if let Some(state) = builder.states.get(mr.id) {
                    builder
                        .commands
                        .entity(state.entity)
                        .insert(gaanim_renderer::effects::VectorOutlineBinding { sources });
                }
                mr
            }
            SpawnKind::FillLevel {
                mask,
                level,
                direction,
            } => {
                let b = builder.svg_path(&gaanim_objects::prelude::SvgPath {
                    id: "FillLevel".into(),
                    path: BezPath::new(),
                    bounds: Bounds3D::default(),
                    fill: spec.fill.clone(),
                    stroke: StrokeBrush::transparent(),
                });
                let mr = Self::finish_spawn_builder(b, spec);
                let sources = id_map
                    .get(mask)
                    .into_iter()
                    .flat_map(|id| Self::visual_leaf_ids(builder, *id))
                    .filter_map(|id| builder.states.get(id).map(|state| state.entity))
                    .collect();
                if let Some(state) = builder.states.get_mut(mr.id) {
                    state.fill_level = *level;
                    builder.commands.entity(state.entity).insert((
                        gaanim_scene::FillLevel(*level),
                        gaanim_renderer::effects::FillLevelBinding {
                            sources,
                            direction: direction.native(),
                        },
                    ));
                }
                mr
            }
            SpawnKind::Connect {
                sources,
                max_distance,
                mode,
                neighbors,
                fade,
            } => {
                let source_entities: Vec<Entity> = sources
                    .iter()
                    .filter_map(|source| id_map.get(source))
                    .filter_map(|id| builder.states.get(*id).map(|state| state.entity))
                    .collect();
                let layers = if *fade { CONNECT_FADE_LAYERS } else { 1 };
                let mut layer_spec = spec.clone();
                layer_spec.fill = None;
                layer_spec.fill_overridden = true;
                layer_spec.stroke_overridden = spec.stroke.is_some();
                let mut refs = Vec::with_capacity(layers);
                for layer in 0..layers {
                    // Longer links fall in later, fainter layers.
                    layer_spec.opacity = 1.0 - layer as f32 / layers as f32;
                    let path = gaanim_objects::prelude::SvgPath {
                        id: "Connect".into(),
                        path: BezPath::new(),
                        bounds: Bounds3D::default(),
                        fill: None,
                        stroke: StrokeBrush::transparent(),
                    };
                    let layer_ref =
                        Self::finish_spawn_builder(builder.svg_path(&path), &layer_spec);
                    if let Some(state) = builder.states.get(layer_ref.id) {
                        builder.commands.entity(state.entity).insert(
                            gaanim_renderer::effects::ConnectBinding {
                                sources: source_entities.clone(),
                                max_distance: *max_distance,
                                mode: *mode,
                                neighbors: *neighbors,
                                layer,
                                layers,
                            },
                        );
                    }
                    refs.push(layer_ref);
                }
                let mr = builder.group(&refs);
                Self::post_apply(builder, mr.id, spec, id_map, frame_bounds);
                mr
            }
            SpawnKind::Boolean {
                sources,
                op,
                live,
                tolerance,
                rule,
            } => {
                let mut paths = sources.iter().filter_map(|source| {
                    id_map
                        .get(source)
                        .copied()
                        .map(|id| Self::mask_path_in_world(builder, id))
                });
                let mut result = paths.next().unwrap_or_default();
                for path in paths {
                    let resolved = gaanim_objects::boolean::apply_with_options(
                        &result,
                        &path,
                        op.native(),
                        *tolerance,
                        rule.native(),
                    );
                    result = resolved
                        .paths
                        .into_iter()
                        .fold(BezPath::new(), |mut joined, path| {
                            joined.extend(path);
                            joined
                        });
                }
                let rect = result.bounding_box();
                let path = gaanim_objects::prelude::SvgPath {
                    id: "BooleanResult".into(),
                    path: result,
                    bounds: Bounds3D::new_2d(rect.x0, rect.y0, rect.x1, rect.y1),
                    fill: spec.fill.clone(),
                    stroke: StrokeBrush::transparent(),
                };
                let mr = Self::finish_spawn_builder(builder.svg_path(&path), spec);
                if *live {
                    let source_entities = sources
                        .iter()
                        .flat_map(|source| {
                            id_map
                                .get(source)
                                .into_iter()
                                .flat_map(|id| Self::visual_leaf_ids(builder, *id))
                        })
                        .filter_map(|id| builder.states.get(id).map(|state| state.entity))
                        .collect();
                    if let Some(state) = builder.states.get(mr.id) {
                        builder.commands.entity(state.entity).insert(
                            gaanim_renderer::effects::BooleanBinding {
                                sources: source_entities,
                                op: op.native(),
                                tolerance: *tolerance,
                                rule: rule.native(),
                            },
                        );
                    }
                }
                Self::apply_layout(builder, mr.id, spec, id_map, frame_bounds);
                mr
            }
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
            SpawnKind::SurroundingRect => {
                let b = builder.surrounding_rectangle(0.0, 0.0, 0.0);
                Self::finish_spawn_builder(b, spec)
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
            SpawnKind::SizedArrow {
                start,
                end,
                head_length,
                head_width,
                body_width,
            } => {
                let b = builder.arrow_with_dimensions(
                    Point::new(start.0, start.1),
                    Point::new(end.0, end.1),
                    *head_length,
                    *head_width,
                    *body_width,
                );
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
            SpawnKind::Points { positions, radius } => {
                let positions: Vec<Point> =
                    positions.iter().map(|&(x, y)| Point::new(x, y)).collect();
                let b = builder.points(&positions, *radius);
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
            SpawnKind::CurvedArrow {
                start,
                end,
                angle,
                head_length,
                head_width,
                body_width,
            } => {
                let b = builder.curved_arrow_with_dimensions(
                    Point::new(start.0, start.1),
                    Point::new(end.0, end.1),
                    *angle,
                    *head_length,
                    *head_width,
                    *body_width,
                );
                let mr = Self::finish_spawn_builder(b, spec);
                Self::apply_layout(builder, mr.id, spec, id_map, frame_bounds);
                mr
            }
            SpawnKind::CurvedArrowArc {
                center,
                radius,
                start_angle,
                sweep_angle,
                head_length,
                head_width,
                body_width,
            } => {
                let b = builder.curved_arrow_arc_with_dimensions(
                    Point::new(center.0, center.1),
                    *radius,
                    *start_angle,
                    *sweep_angle,
                    *head_length,
                    *head_width,
                    *body_width,
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
                    // Scene-unit extension lines; the measurement uses the
                    // default arrow metrics.
                    let extension_a = builder
                        .line(Point::new(start.0, start.1), dimension_start)
                        .no_fill()
                        .stroke(color, 0.02)
                        .spawn();
                    let extension_b = builder
                        .line(Point::new(end.0, end.1), dimension_end)
                        .no_fill()
                        .stroke(color, 0.02)
                        .spawn();
                    let measurement = builder
                        .double_arrow(dimension_start, dimension_end, None, None)
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
            SpawnKind::ReactivePlot {
                map,
                function,
                domain,
                reveal,
                sampling,
            } => {
                let mut parameter_ids = function.parameter_ids();
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
                let values = parameter_entities
                    .iter()
                    .filter_map(|(logical, _)| {
                        let actual = id_map.get(logical).copied()?;
                        builder
                            .float_signals
                            .get(&actual)
                            .copied()
                            .map(|value| (*logical, value))
                    })
                    .collect::<Vec<_>>();
                let path = sampled_reactive_path(
                    map,
                    function,
                    *domain,
                    reveal.as_ref(),
                    *sampling,
                    builder.current_time,
                    &values,
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
                let changes_over_time = !parameter_entities.is_empty()
                    || function.depends_on_time()
                    || reveal.as_ref().is_some_and(ScalarSource::depends_on_time);
                if changes_over_time && let Some(state) = builder.states.get(mr.id) {
                    let map = map.clone();
                    let function = function.clone();
                    let domain = *domain;
                    let reveal = reveal.clone();
                    let sampling = *sampling;
                    let redraw = gaanim_animation::AlwaysRedrawRegen::new(move |world| {
                        let values = parameter_entities
                            .iter()
                            .filter_map(|(logical, entity)| {
                                world
                                    .get::<gaanim_animation::FloatSignal>(*entity)
                                    .map(|signal| (*logical, signal.value))
                            })
                            .collect::<Vec<_>>();
                        let time = world
                            .get_resource::<gaanim_animation::PlaybackState>()
                            .map_or(0.0, |state| state.current_time);
                        sampled_reactive_path(
                            &map,
                            &function,
                            domain,
                            reveal.as_ref(),
                            sampling,
                            time,
                            &values,
                        )
                    });
                    builder.commands.entity(state.entity).insert(redraw);
                }
                mr
            }
            SpawnKind::ReactiveParametric2D {
                map,
                function,
                domain,
                sampling,
            } => {
                let parameter_entities =
                    compile_function_parameters(function, id_map, &builder.states);
                let values = parameter_entities
                    .iter()
                    .filter_map(|(logical, _)| {
                        let actual = id_map.get(logical).copied()?;
                        builder
                            .float_signals
                            .get(&actual)
                            .copied()
                            .map(|value| (*logical, value))
                    })
                    .collect::<Vec<_>>();
                let path = sampled_reactive_parametric_2d(
                    map,
                    function,
                    *domain,
                    *sampling,
                    builder.current_time,
                    &values,
                );
                let svg_path = gaanim_objects::prelude::SvgPath {
                    id: "ReactiveParametric2D".to_owned(),
                    path,
                    bounds: map.frame.bounds(),
                    fill: None,
                    stroke: StrokeBrush::transparent(),
                };
                let b = builder.svg_path(&svg_path);
                let mr = Self::finish_spawn_builder(b, spec);
                Self::apply_layout(builder, mr.id, spec, id_map, frame_bounds);
                if (!parameter_entities.is_empty() || function.depends_on_time())
                    && let Some(state) = builder.states.get(mr.id)
                {
                    let map = map.clone();
                    let function = function.clone();
                    let domain = *domain;
                    let sampling = *sampling;
                    let redraw = gaanim_animation::AlwaysRedrawRegen::new(move |world| {
                        let (time, values) = reactive_values(world, &parameter_entities);
                        sampled_reactive_parametric_2d(
                            &map, &function, domain, sampling, time, &values,
                        )
                    });
                    builder.commands.entity(state.entity).insert(redraw);
                }
                mr
            }
            SpawnKind::ReactiveParametric3D {
                map,
                function,
                domain,
                samples,
            } => {
                let color = spec
                    .stroke
                    .as_ref()
                    .and_then(|(brush, _)| match brush {
                        gaanim_core::peniko::Brush::Solid(color) => Some(*color),
                        _ => None,
                    })
                    .unwrap_or(PenikoColor::from_rgb8(20, 20, 20));
                let parameter_entities =
                    compile_function_parameters(function, id_map, &builder.states);
                let values = parameter_entities
                    .iter()
                    .filter_map(|(logical, _)| {
                        let actual = id_map.get(logical).copied()?;
                        builder
                            .float_signals
                            .get(&actual)
                            .copied()
                            .map(|value| (*logical, value))
                    })
                    .collect::<Vec<_>>();
                let data = sampled_reactive_parametric_3d(
                    map,
                    function,
                    *domain,
                    *samples,
                    builder.current_time,
                    &values,
                    color,
                );
                let mr = builder.spawn_line_list(data.points, color);
                Self::post_apply(builder, mr.id, spec, id_map, frame_bounds);
                if (!parameter_entities.is_empty() || function.depends_on_time())
                    && let Some(state) = builder.states.get(mr.id)
                {
                    let map = map.clone();
                    let function = function.clone();
                    let domain = *domain;
                    let samples = *samples;
                    let regen = ReactiveLineRegen::new(move |world| {
                        let (time, values) = reactive_values(world, &parameter_entities);
                        sampled_reactive_parametric_3d(
                            &map, &function, domain, samples, time, &values, color,
                        )
                    });
                    builder.commands.entity(state.entity).insert(regen);
                }
                mr
            }
            SpawnKind::ReactiveSurface3D {
                map,
                function,
                resolution,
            } => {
                let parameter_entities =
                    compile_function_parameters(function, id_map, &builder.states);
                let values = parameter_entities
                    .iter()
                    .filter_map(|(logical, _)| {
                        let actual = id_map.get(logical).copied()?;
                        builder
                            .float_signals
                            .get(&actual)
                            .copied()
                            .map(|value| (*logical, value))
                    })
                    .collect::<Vec<_>>();
                let data = sampled_reactive_surface_3d(
                    map,
                    function,
                    *resolution,
                    builder.current_time,
                    &values,
                );
                let mr = builder.spawn_triangle_mesh_data(data);
                Self::post_apply(builder, mr.id, spec, id_map, frame_bounds);
                if (!parameter_entities.is_empty() || function.depends_on_time())
                    && let Some(state) = builder.states.get(mr.id)
                {
                    let map = map.clone();
                    let function = function.clone();
                    let resolution = *resolution;
                    let regen = ReactiveMeshRegen::new(move |world| {
                        let (time, values) = reactive_values(world, &parameter_entities);
                        sampled_reactive_surface_3d(&map, &function, resolution, time, &values)
                    });
                    builder.commands.entity(state.entity).insert(regen);
                }
                mr
            }
            SpawnKind::ReactiveReadout {
                source,
                format,
                prefix,
                suffix,
                invalid,
                decimal_separator,
                font_size,
                font_family,
                font_weight,
                rolling,
            } => {
                let parameter_entities: Vec<(gaanim_core::ObjectId, bevy::prelude::Entity)> =
                    source
                        .parameter_ids()
                        .into_iter()
                        .filter_map(|logical| {
                            let actual = id_map.get(&logical).copied()?;
                            Some((logical, builder.states.get(actual)?.entity))
                        })
                        .collect();
                let values = parameter_entities
                    .iter()
                    .filter_map(|(logical, _)| {
                        let actual = id_map.get(logical).copied()?;
                        builder
                            .float_signals
                            .get(&actual)
                            .copied()
                            .map(|value| (*logical, value))
                    })
                    .collect::<Vec<_>>();
                let body = &text_config.roles[&gaanim_text::prelude::TextRole::Body];
                let size = font_size.unwrap_or(body.size);
                let digit_family = font_family.as_ref().unwrap_or(&body.font_family);
                let number = gaanim_animation::localize_decimal_separator(
                    &gaanim_animation::format_reactive_number(
                        source
                            .evaluate(builder.current_time, |logical| {
                                values
                                    .iter()
                                    .find_map(|(id, value)| (*id == logical).then_some(*value))
                            })
                            .unwrap_or(f64::NAN),
                        format,
                        invalid,
                    ),
                    *decimal_separator,
                );
                let text = format!("{prefix}{number}{suffix}");
                let (path, bounds) = gaanim_animation::shape_readout_text_with_weight(
                    builder.font_registry,
                    prefix,
                    &number,
                    suffix,
                    digit_family,
                    *font_weight,
                    size,
                )
                .unwrap_or_else(|_| (gaanim_core::kurbo::BezPath::new(), Bounds3D::default()));
                let baseline = gaanim_animation::right_aligned_readout_baseline(bounds);
                let (path, bounds) = gaanim_animation::right_align_readout_path(path, bounds);
                let rolling_component = rolling.as_ref().map(|options| {
                    let mut options = options.clone();
                    options
                        .font_family
                        .get_or_insert_with(|| body.font_family.clone());
                    gaanim_animation::RollingNumber::new(builder.font_registry, options)
                        .expect("validated rolling number font must compile")
                });
                let baseline = rolling_component
                    .as_ref()
                    .map_or(baseline, |rolling| rolling.baseline());
                let (path, bounds) = if let Some(rolling) = &rolling_component {
                    let value = source
                        .evaluate(builder.current_time, |logical| {
                            values
                                .iter()
                                .find_map(|(id, value)| (*id == logical).then_some(*value))
                        })
                        .unwrap_or(f64::NAN);
                    rolling.geometry(value)
                } else {
                    (path, bounds)
                };
                // Numbers are text: without an explicit or themed fill they use
                // the body color, which contrasts with the scene background.
                let svg_path = gaanim_objects::prelude::SvgPath {
                    id: "ReactiveReadout".to_owned(),
                    path,
                    bounds,
                    fill: Some(gaanim_core::peniko::Brush::Solid(body.fill_color)),
                    stroke: StrokeBrush::transparent(),
                };
                let source_path = std::sync::Arc::new(svg_path.path.clone());
                let b = builder.svg_path(&svg_path);
                let mr = Self::finish_spawn_builder(b, spec);
                if rolling_component.is_some() {
                    builder.text_metrics.insert(
                        mr.id,
                        gaanim_text::prelude::TextMetrics {
                            first_baseline: baseline,
                            line_count: 1,
                        },
                    );
                }
                Self::apply_layout(builder, mr.id, spec, id_map, frame_bounds);
                if let Some(state) = builder.states.get(mr.id) {
                    let entity = state.entity;
                    if let Some(rolling) = rolling_component {
                        // Tweens are scheduled later; windows attach after compilation.
                        let sources: Vec<_> = source
                            .parameter_ids()
                            .into_iter()
                            .filter_map(|logical| id_map.get(&logical).copied())
                            .collect();
                        if rolling.options.mode == gaanim_animation::RollingMode::Continuous
                            && !sources.is_empty()
                        {
                            builder.rolling_tween_sources.push((entity, sources));
                        }
                        builder.commands.entity(entity).insert(rolling);
                    }
                    builder.commands.entity(entity).insert((
                        gaanim_scene::PathSource(source_path.clone()),
                        gaanim_scene::TextBaseline(baseline),
                        gaanim_animation::ReactiveReadout {
                            source: source.clone(),
                            parameters: parameter_entities,
                            format: format.clone(),
                            prefix: prefix.clone(),
                            suffix: suffix.clone(),
                            invalid: invalid.clone(),
                            decimal_separator: *decimal_separator,
                            font_family: digit_family.clone(),
                            font_weight: *font_weight,
                            font_size: size,
                            last_text: text,
                            last_path: source_path,
                            last_bounds: bounds,
                        },
                    ));
                }
                mr
            }
            SpawnKind::ProgressArc {
                source,
                radius,
                maximum,
            } => {
                let parameters: Vec<(gaanim_core::ObjectId, bevy::prelude::Entity)> = source
                    .parameter_ids()
                    .into_iter()
                    .filter_map(|logical| {
                        let actual = id_map.get(&logical).copied()?;
                        Some((logical, builder.states.get(actual)?.entity))
                    })
                    .collect();
                let value = source
                    .evaluate(builder.current_time, |logical| {
                        let actual = id_map.get(&logical).copied()?;
                        builder.float_signals.get(&actual).copied()
                    })
                    .unwrap_or(f64::NAN);
                let arc = gaanim_animation::ProgressArc::new(
                    source.clone(),
                    parameters,
                    *radius,
                    *maximum,
                );
                // The box is the whole ring, so layout does not follow the sweep.
                let svg_path = gaanim_objects::prelude::SvgPath {
                    id: "ProgressArc".to_owned(),
                    path: gaanim_animation::progress_arc_path(*radius, arc.fraction(value)),
                    bounds: Bounds3D::new_2d(-radius, -radius, *radius, *radius),
                    fill: None,
                    stroke: StrokeBrush::transparent(),
                };
                let b = builder.svg_path(&svg_path);
                let mr = Self::finish_spawn_builder(b, spec);
                Self::apply_layout(builder, mr.id, spec, id_map, frame_bounds);
                if let Some(state) = builder.states.get(mr.id) {
                    builder.commands.entity(state.entity).insert(arc);
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
            SpawnKind::Typst {
                source,
                page_width,
                scene_units,
            } => {
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
                // A document keeps Typst's own proportions (11pt text, table
                // insets, rule widths) and is scaled so its default text is
                // as large as the body role.
                let scale = if *scene_units {
                    1.0
                } else {
                    body.size / TYPST_DEFAULT_TEXT_PT
                };
                let mr = builder.scaled_typst(
                    &source,
                    false,
                    Some(&body.font_family),
                    None,
                    None,
                    None,
                    scale,
                );
                Self::post_apply(builder, mr.id, spec, id_map, frame_bounds);
                Self::apply_fragment_fills(builder, mr, spec);
                mr
            }
            SpawnKind::Image { image, view } => {
                let b = builder.image(
                    image.clone(),
                    spec.media_frame
                        .map(super::types::media_view)
                        .unwrap_or(*view),
                );
                let mr = Self::finish_spawn_builder(b, spec);
                if let (Some(frame), Some(state)) =
                    (spec.media_frame, builder.states.get_mut(mr.id))
                {
                    state.bounds = Bounds3D::new_2d(
                        -frame.width / 2.0,
                        -frame.height / 2.0,
                        frame.width / 2.0,
                        frame.height / 2.0,
                    );
                    builder.media_frames.insert(mr.id, frame);
                    builder.commands.entity(state.entity).insert(frame);
                }
                Self::apply_layout(builder, mr.id, spec, id_map, frame_bounds);
                mr
            }
            SpawnKind::Video {
                poster,
                view,
                playback,
            } => {
                let b = builder.image(
                    poster.clone(),
                    spec.media_frame
                        .map(super::types::media_view)
                        .unwrap_or(*view),
                );
                let mr = Self::finish_spawn_builder(b, spec);
                if let (Some(frame), Some(state)) =
                    (spec.media_frame, builder.states.get_mut(mr.id))
                {
                    state.bounds = Bounds3D::new_2d(
                        -frame.width / 2.0,
                        -frame.height / 2.0,
                        frame.width / 2.0,
                        frame.height / 2.0,
                    );
                    builder.media_frames.insert(mr.id, frame);
                    builder.commands.entity(state.entity).insert(frame);
                }
                if let Some(state) = builder.states.get(mr.id) {
                    builder
                        .commands
                        .entity(state.entity)
                        .insert(playback.clone());
                }
                Self::apply_layout(builder, mr.id, spec, id_map, frame_bounds);
                mr
            }
            SpawnKind::Lottie { playback } => {
                let w2 = playback.view.display_width * 0.5;
                let h2 = playback.view.display_height * 0.5;
                let placeholder = gaanim_objects::prelude::SvgPath {
                    id: "Lottie".to_owned(),
                    path: gaanim_core::kurbo::BezPath::new(),
                    bounds: Bounds3D::new_2d(-w2, -h2, w2, h2),
                    // The empty marker advertises a fill phase to the drawing
                    // scheduler. LottiePlayer renders the actual authored paints.
                    fill: Some(gaanim_core::peniko::Brush::Solid(PenikoColor::WHITE)),
                    stroke: StrokeBrush::transparent(),
                };
                let b = builder.svg_path(&placeholder);
                let mr = Self::finish_spawn_builder(b, spec);
                if let Some(state) = builder.states.get(mr.id) {
                    builder
                        .commands
                        .entity(state.entity)
                        .insert(gaanim_renderer::lottie::LottiePlayer::new(playback.clone()));
                }
                Self::apply_layout(builder, mr.id, spec, id_map, frame_bounds);
                mr
            }
            SpawnKind::SvgPath(path) => {
                let b = builder.svg_path(path);
                let mr = Self::finish_spawn_builder(b, spec);
                // A part of an imported SVG is laid out after its root.
                if spec.svg_owner.is_none() {
                    Self::apply_layout(builder, mr.id, spec, id_map, frame_bounds);
                }
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
                        fill_level: 0.0,
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

                Self::finish_spawn_builder(b, spec)
            }
            SpawnKind::TrackingLine => {
                // Spawn a minimal line (0,0)→(0,0). TrackingLine will overwrite its Path2D.
                let b = builder.line(Point::new(0.0, 0.0), Point::new(0.0, 0.0));

                Self::finish_spawn_builder(b, spec)
            }
            SpawnKind::TracedPath3DLine => {
                // Empty 3D line placeholder; TracedPath3D will fill its LineListData.
                let mref = builder.spawn_line_strip(vec![], gaanim_core::peniko::Color::WHITE);
                Self::post_apply(builder, mref.id, spec, id_map, frame_bounds);
                mref
            }
        };
        // Applies to primitives as well as groups/text, which skip `post_apply`
        // when they finish through `finish_spawn_builder`.
        if let Some(align) = spec.stroke_align
            && let Some(state) = builder.states.get(mref.id)
        {
            let entities: Vec<_> = std::iter::once(state.entity)
                .chain(state.child_spans.iter().map(|child| child.entity))
                .collect();
            for entity in entities {
                builder.commands.entity(entity).insert(align);
            }
        }
        if let Some(blend) = spec.blend {
            Self::apply_blend(builder, mref.id, blend);
        }
        if let Some(profile) = &spec.stroke_profile {
            let profile = gaanim_renderer::effects::StrokeProfile(profile.clone());
            for (entity, _) in Self::hierarchy_entities(builder, mref.id) {
                builder.commands.entity(entity).insert(profile.clone());
            }
        }
        if let Some((amount, max_ratio)) = spec.squash_stretch
            && let Some(state) = builder.states.get(mref.id)
        {
            builder
                .commands
                .entity(state.entity)
                .insert(gaanim_animation::SquashStretch::new(amount, max_ratio));
        }
        if let Some((_, count)) = spec.repeat_count
            && let Some(state) = builder.states.get(mref.id)
        {
            let members: Vec<_> = state
                .children
                .iter()
                .filter_map(|member| builder.states.get(*member).map(|state| state.entity))
                .collect();
            for (index, entity) in members.into_iter().enumerate() {
                let presence = crate::count_lens::presence(count, index);
                if presence < 1.0 {
                    builder
                        .commands
                        .entity(entity)
                        .insert(gaanim_scene::Presence(presence));
                }
            }
        }
        if spec.motion_blur_exempt {
            for (entity, _) in Self::hierarchy_entities(builder, mref.id) {
                builder
                    .commands
                    .entity(entity)
                    .insert(gaanim_renderer::effects::MotionBlurExempt);
            }
        }
        if let Some(tips) = &spec.tips {
            Self::attach_tips(builder, mref.id, tips.clone());
        }
        if let Some(role) = spec.coordinate_view_role
            && let Some(state) = builder.states.get(mref.id)
        {
            builder.commands.entity(state.entity).insert(role);
        }
        if let Some(state) = builder.states.get(mref.id) {
            let entity = state.entity;
            if let Some(offset) = spec.coordinate_label_offset {
                builder
                    .commands
                    .entity(entity)
                    .insert(gaanim_scene::CoordinateLabelOffset(offset));
            }
            if let Some((axis, generation, anchors)) = &spec.coordinate_tick_level {
                let anchors = anchors.lock().expect("tick anchors poisoned").clone();
                builder
                    .commands
                    .entity(entity)
                    .insert(gaanim_scene::CoordinateTickLevel {
                        axis: *axis,
                        generation: *generation,
                        anchors,
                    });
            }
        }
        mref
    }

    /// Entities of `id`, its glyph spans and its descendants, each flagged
    /// with whether it belongs to `id` itself.
    fn hierarchy_entities(
        builder: &SceneBuilder,
        id: ObjectId,
    ) -> Vec<(bevy::prelude::Entity, bool)> {
        let mut entities = Vec::new();
        let mut pending = vec![(id, true)];
        while let Some((id, own)) = pending.pop() {
            let Some(state) = builder.states.get(id) else {
                continue;
            };
            entities.push((state.entity, own));
            entities.extend(state.child_spans.iter().map(|child| (child.entity, own)));
            pending.extend(state.children.iter().map(|child| (*child, false)));
        }
        entities
    }

    /// The scene and segment post-process chains, with the entities that
    /// hold the signals of the parameters their uniforms read.
    fn compiled_post_process(
        builder: &SceneBuilder,
        id_map: &HashMap<ObjectId, ObjectId>,
        passes: Vec<gaanim_renderer::post_process::PostProcessPass>,
        segments: Vec<gaanim_renderer::post_process::SegmentPostProcess>,
    ) -> gaanim_renderer::post_process::CanvasPostProcess {
        let segment_passes = segments.iter().flat_map(|segment| match &segment.post {
            gaanim_renderer::post_process::PostProcessOverride::Passes(passes) => passes.as_slice(),
            _ => &[],
        });
        let mut parameters: Vec<(ObjectId, bevy::prelude::Entity)> = Vec::new();
        for pass in passes.iter().chain(segment_passes) {
            for logical in pass.values.iter().flat_map(|source| source.parameter_ids()) {
                if parameters.iter().any(|(id, _)| *id == logical) {
                    continue;
                }
                if let Some(state) = id_map
                    .get(&logical)
                    .and_then(|actual| builder.states.get(*actual))
                {
                    parameters.push((logical, state.entity));
                }
            }
        }
        gaanim_renderer::post_process::CanvasPostProcess {
            passes,
            segments,
            parameters,
        }
    }

    /// Spawns the tip entities of `id`'s path as its children.
    fn attach_tips(
        builder: &mut SceneBuilder,
        id: ObjectId,
        mut tips: gaanim_animation::StrokeTips,
    ) {
        let Some(route) = builder.states.get(id).map(|state| state.entity) else {
            return;
        };
        let spawn_tip = |builder: &mut SceneBuilder| {
            let tip_id = builder.next_id();
            let mut bundle = gaanim_objects::primitives::MobjectBundle::new(
                tip_id,
                BezPath::new(),
                Bounds3D::default(),
            );
            bundle.fill = FillBrush(None);
            bundle.stroke = StrokeBrush::transparent();
            builder
                .commands
                .spawn((bundle, gaanim_animation::StrokeTip, ChildOf(route)))
                .id()
        };
        tips.start_entity = tips.start.map(|_| spawn_tip(builder));
        tips.end_entity = tips.end.map(|_| spawn_tip(builder));
        builder.commands.entity(route).insert(tips);
    }

    /// Spawn `echo`'s copies of `id` and its descendants: render-only
    /// entities that the timeline re-evaluates at delayed times.
    fn attach_echo(builder: &mut SceneBuilder, id: ObjectId, echo: super::types::EchoSpec) {
        // Each node with its parent inside the echoed subtree, parents first.
        let mut nodes = Vec::new();
        let mut pending = vec![(id, None)];
        while let Some((node, parent)) = pending.pop() {
            let Some(state) = builder.states.get(node) else {
                continue;
            };
            nodes.push((node, state.entity, parent));
            nodes.extend(
                state
                    .child_spans
                    .iter()
                    .map(|child| (child.id, child.entity, Some(node))),
            );
            pending.extend(
                state
                    .children
                    .iter()
                    .rev()
                    .map(|child| (*child, Some(node))),
            );
        }
        let motion_sources: Vec<ObjectId> = nodes.iter().map(|(node, _, _)| *node).collect();
        for copy in 1..=echo.count() {
            let mut copies: HashMap<ObjectId, bevy::prelude::Entity> = HashMap::new();
            for &(node, entity, parent) in &nodes {
                let ghost_id = builder.next_id();
                let parent = parent.and_then(|parent| copies.get(&parent).copied());
                let root = parent.is_none();
                let ghost = builder
                    .commands
                    .entity(entity)
                    .clone_and_spawn_with_opt_in(|cloner| {
                        cloner.allow::<(
                            gaanim_scene::RasterImage,
                            gaanim_scene::HudOverlay,
                            gaanim_renderer::effects::DropShadow,
                            gaanim_renderer::effects::Glow,
                            gaanim_renderer::effects::GaussianBlur,
                            gaanim_renderer::effects::ClipMask,
                            gaanim_renderer::effects::StrokeAlign,
                            gaanim_renderer::effects::ElementBlend,
                            gaanim_renderer::effects::ViewLayer,
                            gaanim_renderer::effects::MotionBlurExempt,
                            gaanim_renderer::effects::StrokeProfile,
                        )>();
                    })
                    .insert((
                        gaanim_scene::MobjectId(ghost_id),
                        gaanim_animation::EchoGhost {
                            source: node,
                            lag: echo.delay() * f64::from(copy),
                            opacity: if root {
                                echo.decay().powi(copy as i32)
                            } else {
                                1.0
                            },
                            parent,
                            rank: copy,
                            hold: echo.hold(),
                            motion_sources: motion_sources.clone(),
                        },
                    ))
                    .id();
                copies.insert(node, ghost);
            }
        }
    }

    /// Composites `id` and its drawn descendants with `blend`.
    ///
    /// Members of a group are compiled before it, so a member restyled after
    /// grouping already carries its own blend and keeps it.
    fn apply_blend(
        builder: &mut SceneBuilder,
        id: ObjectId,
        blend: gaanim_core::peniko::BlendMode,
    ) {
        let blend = gaanim_renderer::effects::ElementBlend(blend);
        for (entity, own) in Self::hierarchy_entities(builder, id) {
            let mut commands = builder.commands.entity(entity);
            if own {
                commands.insert(blend);
            } else {
                commands.insert_if_new(blend);
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

    /// Mark a spawned drawable (and its glyph spans) as a HUD overlay.
    ///
    /// HUD overlays stay in the Vello2D pass, drawn after the 3D meshes, and
    /// `pin_hud_overlays_system` keeps them fixed on the output frame. The
    /// high z of the root is inherited by its glyphs.
    fn apply_hud(builder: &mut SceneBuilder, id: ObjectId) {
        let Some(state) = builder.states.get(id) else {
            return;
        };
        let root = state.entity;
        let spans: Vec<Entity> = state.child_spans.iter().map(|span| span.entity).collect();
        for entity in std::iter::once(root).chain(spans) {
            builder
                .commands
                .entity(entity)
                .insert(gaanim_scene::HudOverlay);
        }
        builder
            .commands
            .entity(root)
            .insert(gaanim_scene::RenderOrder {
                z_index: 1000,
                creation_order: id.index() as u64,
            });
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
                // Keep the creation tie-breaker used by every other object.
                builder.commands.entity(st.entity).insert(RenderOrder {
                    z_index: spec.z_index,
                    creation_order: id.index() as u64,
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
        // Opacity multiplies down the hierarchy: the spans keep their own, so
        // animating the root's opacity alone can bring them back.
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
        let effect_targets: Vec<(ObjectId, bevy::prelude::Entity)> = if child_spans.is_empty() {
            builder
                .states
                .get(id)
                .map(|state| vec![(id, state.entity)])
                .unwrap_or_default()
        } else {
            child_spans
                .iter()
                .map(|child| (child.id, child.entity))
                .collect()
        };
        let effects = crate::effect_lens::EffectState {
            glow: spec.glow.clone(),
            blur: spec.blur,
            shadow: spec.shadow.clone(),
        };
        for (target, entity) in effect_targets {
            if effects != crate::effect_lens::EffectState::default() {
                builder.effects.insert(target, effects.clone());
            }
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
        if spec.billboard
            && let Some(state) = builder.states.get(id)
        {
            builder
                .commands
                .entity(state.entity)
                .insert(gaanim_scene::Billboard);
            builder
                .commands
                .entity(state.entity)
                .insert(bevy::prelude::Transform::default());
        }
        // A part of an imported SVG is laid out after its root.
        if spec.svg_owner.is_none() {
            Self::apply_layout(builder, id, spec, id_map, frame_bounds);
        }
    }

    /// Lay out the parts of an imported SVG after its root. The root is
    /// placed, scaled and pivoted by the drawing as the file declares it,
    /// so moving a part (`svg.part("g").shift_by(...)`) never moves the
    /// whole SVG, and a part placed in scene coordinates lands there.
    fn apply_svg_part_layouts(
        builder: &mut SceneBuilder,
        root: ObjectId,
        object_specs: &HashMap<ObjectId, ObjectSpec>,
        id_map: &HashMap<ObjectId, ObjectId>,
        frame_bounds: Bounds3D,
    ) {
        let mut parts: Vec<&ObjectSpec> = object_specs
            .values()
            .filter(|spec| spec.svg_owner == Some(root) && !spec.layout_ops.is_empty())
            .collect();
        parts.sort_by_key(|spec| spec.id.index());
        for spec in parts {
            if let Some(&id) = id_map.get(&spec.id) {
                Self::apply_layout(builder, id, spec, id_map, frame_bounds);
            }
        }
    }

    /// Shapes whose geometry is authored at absolute scene positions, such
    /// as lines, polygons and arcs, rather than around their own origin.
    fn declared_in_scene_coordinates(kind: &SpawnKind) -> bool {
        matches!(
            kind,
            SpawnKind::Line(..)
                | SpawnKind::Arrow(..)
                | SpawnKind::SizedArrow { .. }
                | SpawnKind::DashedLine { .. }
                | SpawnKind::DoubleArrow { .. }
                | SpawnKind::Polygon(_)
                | SpawnKind::Points { .. }
                | SpawnKind::Sector { .. }
                | SpawnKind::Brace { .. }
                | SpawnKind::Arc { .. }
                | SpawnKind::CurvedArrow { .. }
                | SpawnKind::CurvedArrowArc { .. }
                | SpawnKind::Dimension { .. }
                | SpawnKind::Polyline(_)
                | SpawnKind::Bezier { .. }
                | SpawnKind::Curve(_)
        )
    }

    /// Write the translation and scale a layout gives a member to its
    /// entity. Its rotation and skew are its own: a child rotated later in
    /// the timeline must not show that rotation before its animation starts.
    fn place_layout_member(
        builder: &mut SceneBuilder,
        entity: bevy::prelude::Entity,
        transform: SpatialTransform,
    ) {
        builder.commands.queue(move |world: &mut World| {
            if let Some(mut current) = world.get_mut::<SpatialTransform>(entity) {
                current.translation = transform.translation;
                current.scale = transform.scale;
            } else if let Ok(mut entity) = world.get_entity_mut(entity) {
                entity.insert(transform);
            }
        });
    }

    /// The translation a box child takes at its new `rest`: the rest plus the
    /// offset it has been moved from the rest its layout last gave it, so
    /// the child's own animations survive a reflow. Records `rest`, and on
    /// the child's first placement its `current` scale and rotation.
    fn keep_layout_offset(
        builder: &mut SceneBuilder,
        source: Option<ObjectId>,
        current_translations: &HashMap<ObjectId, DVec3>,
        current: &SpatialTransform,
        rest: DVec3,
    ) -> DVec3 {
        let Some(source) = source else {
            return rest;
        };
        let previous = builder.layout_rests.get(&source).copied();
        builder.layout_rests.insert(
            source,
            crate::builder::LayoutRest {
                translation: rest,
                scale: previous.map_or(current.scale, |previous| previous.scale),
                rotation: previous.map_or(current.rotation, |previous| previous.rotation),
            },
        );
        let offset = previous
            .zip(current_translations.get(&source))
            .map_or(DVec3::ZERO, |(previous, current)| {
                *current - previous.translation
            });
        rest + offset
    }

    fn apply_layout(
        builder: &mut SceneBuilder,
        id: ObjectId,
        spec: &ObjectSpec,
        id_map: &HashMap<ObjectId, ObjectId>,
        frame_bounds: Bounds3D,
    ) {
        let uses_default_text_anchor = matches!(spec.kind, SpawnKind::Text(_));
        let pivots_on_box_center = Self::declared_in_scene_coordinates(&spec.kind)
            && !spec
                .layout_ops
                .iter()
                .any(|op| matches!(op, LayoutOp::SetPivot(_)));
        if spec.layout_ops.is_empty() && !uses_default_text_anchor && !pivots_on_box_center {
            return;
        }

        let Some(state) = builder.states.get(id) else {
            return;
        };
        let bounds = state.bounds;
        let original_transform = state.transform;
        let entity = state.entity;
        let mut transform = original_transform;
        // Geometry declared in scene coordinates keeps its local origin at the
        // scene origin. Its default pivot is its box center instead, so
        // rotations, scales and skews turn it in place. The anchor does not
        // move an untransformed shape.
        if pivots_on_box_center && transform.anchor == DVec3::ZERO {
            transform.anchor = bounds.center();
        }
        let mut pivot_in_scene = None;
        let mut pending_text_anchor = uses_default_text_anchor.then_some((
            DVec3::ZERO,
            gaanim_text::prelude::TextAnchor::BaselineCenter,
            true,
        ));

        for op in &spec.layout_ops {
            match op {
                LayoutOp::SetTranslation(translation) => {
                    pending_text_anchor = None;
                    transform.translation = if matches!(spec.kind, SpawnKind::Group(_)) {
                        *translation - bounds.center()
                    } else {
                        *translation
                    };
                }
                LayoutOp::ShiftBy(delta) => {
                    if let Some((target, _, _)) = &mut pending_text_anchor {
                        *target += *delta;
                    } else {
                        transform.translation += *delta;
                    }
                }
                LayoutOp::SetScale(factor) => {
                    transform.scale = original_transform.scale * *factor;
                }
                LayoutOp::SetScale3D(scale) => {
                    transform.scale = original_transform.scale * *scale;
                }
                LayoutOp::ScaleBy(factor) => {
                    transform.scale *= *factor;
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
                LayoutOp::RotateBy(delta) => {
                    transform.rotation = *delta * transform.rotation;
                }
                LayoutOp::SetSkew(skew) => {
                    transform.skew = *skew;
                }
                LayoutOp::SetPivot(pivot) => {
                    pivot_in_scene = Some(*pivot);
                }
                LayoutOp::MoveAnchorTo { target, anchor } => {
                    pending_text_anchor = None;
                    transform =
                        gaanim_layout::compute_move_to(bounds, &transform, *target, *anchor);
                }
                LayoutOp::MoveToAnchorPoint { point } => {
                    pending_text_anchor = None;
                    let Some(reference_id) = id_map.get(&point.object).copied() else {
                        bevy::prelude::warn!(
                            "SceneModel layout skipped: anchor point object {:?} was not spawned before {:?}",
                            point.object,
                            spec.id
                        );
                        continue;
                    };
                    let Some(reference_state) = builder.states.get(reference_id) else {
                        bevy::prelude::warn!(
                            "SceneModel layout skipped: missing state for anchor point object {:?}",
                            reference_id
                        );
                        continue;
                    };
                    let reference_transform = builder.get_world_transform(reference_id);
                    let local = reference_state.bounds.center()
                        + reference_state.bounds.size() * 0.5 * point.normalized
                        + point.offset;
                    let target = reference_transform.to_mat4().transform_point3(local);
                    transform = gaanim_layout::compute_move_to(
                        bounds,
                        &transform,
                        target,
                        gaanim_layout::Anchor::Center,
                    );
                }
                LayoutOp::MoveTextAnchorTo {
                    target,
                    anchor,
                    center_multiline,
                } => {
                    pending_text_anchor = Some((*target, *anchor, *center_multiline));
                }
                LayoutOp::NextTo {
                    reference,
                    direction,
                    spacing,
                    aligned_edge,
                } => {
                    pending_text_anchor = None;
                    let Some(reference_id) = id_map.get(reference).copied() else {
                        bevy::prelude::warn!(
                            "SceneModel layout skipped: reference object {:?} was not spawned before {:?}",
                            reference,
                            spec.id
                        );
                        continue;
                    };
                    let Some(reference_state) = builder.states.get(reference_id) else {
                        bevy::prelude::warn!(
                            "SceneModel layout skipped: missing state for reference object {:?}",
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
                    pending_text_anchor = None;
                    let Some(reference_id) = id_map.get(reference).copied() else {
                        bevy::prelude::warn!(
                            "SceneModel layout skipped: reference object {:?} was not spawned before {:?}",
                            reference,
                            spec.id
                        );
                        continue;
                    };
                    let Some(reference_state) = builder.states.get(reference_id) else {
                        bevy::prelude::warn!(
                            "SceneModel layout skipped: missing state for reference object {:?}",
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
                    pending_text_anchor = None;
                    transform = gaanim_layout::compute_to_edge(
                        bounds,
                        &transform,
                        *direction,
                        *buff,
                        frame_bounds,
                    );
                }
                LayoutOp::ToCorner { corner, buff } => {
                    pending_text_anchor = None;
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

        if let Some((target, anchor, center_multiline)) = pending_text_anchor {
            let point = builder
                .text_metrics
                .get(&id)
                .filter(|metrics| metrics.line_count > 0)
                .map(|metrics| {
                    if center_multiline && metrics.line_count > 1 {
                        bounds.center()
                    } else {
                        let x = match anchor {
                            gaanim_text::prelude::TextAnchor::BaselineLeft => bounds.min.x,
                            gaanim_text::prelude::TextAnchor::BaselineCenter => bounds.center().x,
                            gaanim_text::prelude::TextAnchor::BaselineRight => bounds.max.x,
                        };
                        DVec3::new(x, metrics.first_baseline, bounds.center().z)
                    }
                })
                .unwrap_or_else(|| bounds.center());

            if let Some(pivot) = pivot_in_scene {
                // Satisfy both constraints exactly: the public pivot remains
                // fixed in scene space while the requested text point lands on
                // its target after the final rotation, skew and scale.
                let rotated_delta =
                    transform.unskew(transform.rotation.inverse() * (target - pivot));
                let local_delta = DVec3::new(
                    if transform.scale.x.abs() > f64::EPSILON {
                        rotated_delta.x / transform.scale.x
                    } else {
                        0.0
                    },
                    if transform.scale.y.abs() > f64::EPSILON {
                        rotated_delta.y / transform.scale.y
                    } else {
                        0.0
                    },
                    if transform.scale.z.abs() > f64::EPSILON {
                        rotated_delta.z / transform.scale.z
                    } else {
                        0.0
                    },
                );
                transform.translation = pivot - point + local_delta;
                transform.anchor = pivot - transform.translation;
            } else {
                transform = gaanim_layout::compute_move_point_to(&transform, target, point);
            }
        } else if let Some(pivot) = pivot_in_scene {
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

/// Groups glyph children by the text unit of the visible character each one
/// draws, in reading order.
///
/// Glyphs are aligned with `visible` greedily with a short lookahead, so a
/// ligature that consumes several characters or a glyph with no source
/// character (math, smart quotes, hyphenation) does not derail the alignment.
/// Such glyphs, and punctuation outside every unit, join the preceding unit,
/// or the following one at the start. Returns `None` when no glyph matches a
/// unit.
pub(crate) fn reveal_groups(
    glyphs: &[(ObjectId, char)],
    visible: &[(char, Option<usize>)],
) -> Option<Vec<Vec<ObjectId>>> {
    const LOOKAHEAD: usize = 4;
    let mut cursor = 0;
    let mut units: Vec<Option<usize>> = glyphs
        .iter()
        .map(|(_, character)| {
            let offset = visible
                .get(cursor..)?
                .iter()
                .take(LOOKAHEAD)
                .position(|(visible, _)| visible == character)?;
            cursor += offset + 1;
            visible[cursor - 1].1
        })
        .collect();
    let mut previous = None;
    for unit in &mut units {
        *unit = unit.or(previous);
        previous = *unit;
    }
    let mut next = None;
    for unit in units.iter_mut().rev() {
        *unit = unit.or(next);
        next = *unit;
    }

    let mut groups: Vec<Vec<ObjectId>> = Vec::new();
    let mut group_of_unit = HashMap::new();
    for ((id, _), unit) in glyphs.iter().zip(units) {
        let group = *group_of_unit.entry(unit?).or_insert_with(|| {
            groups.push(Vec::new());
            groups.len() - 1
        });
        groups[group].push(*id);
    }
    (!groups.is_empty()).then_some(groups)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::canvas::{Anchor, DrawableHandle, TextAnchor};
    use bevy::ecs::world::CommandQueue;
    use gaanim_core::peniko::Brush;
    use gaanim_math::SpatialTransform;
    use gaanim_scene::{LocalBounds, TextBaseline};
    use gaanim_timeline::snapshot::WorldSnapshot;

    fn compile_canvas_for_layout(canvas: SceneModel) -> World {
        let world = World::new();
        let mut queue = CommandQueue::default();
        let mut commands = Commands::new(&mut queue, &world);
        let mut timeline = Timeline::new();
        let fonts = gaanim_text::font::FontRegistry::new();
        let text_config = gaanim_text::prelude::TextConfig::default();
        canvas.compile_into(&mut commands, &mut timeline, &fonts, &text_config);

        let mut world = world;
        queue.apply(&mut world);
        world
    }

    fn lottie_test_asset(name: &str) -> std::path::PathBuf {
        let path = std::env::temp_dir().join(format!(
            "gaanim-compiled-lottie-{name}-{}.json",
            std::process::id()
        ));
        std::fs::write(
            &path,
            r#"{
            "v":"5.7.5","fr":30,"ip":0,"op":30,"w":100,"h":100,
            "layers":[{"ty":4,"ind":1,"st":0,"ip":0,"op":30,
                "ks":{"p":{"a":1,"k":[
                    {"t":0,"s":[0,0],"o":{"x":0.33,"y":0.33},"i":{"x":0.67,"y":0.67}},
                    {"t":30,"s":[60,0]}
                ]},"s":{"a":0,"k":[100,100]},"r":{"a":0,"k":0}},
                "shapes":[
                    {"ty":"rc","p":{"a":0,"k":[10,50]},"s":{"a":0,"k":[10,10]},"r":{"a":0,"k":0}},
                    {"ty":"fl","c":{"a":0,"k":[1,0,0,1]},"o":{"a":0,"k":100},"r":1}
                ]
            }]
        }"#,
        )
        .unwrap();

        path
    }

    #[test]
    fn compiled_lottie_playback_animates_after_frozen_declaration_and_seeks_back() {
        use gaanim_renderer::lottie::{LottiePlayer, sample_lottie_system};

        let path = lottie_test_asset("playback");

        for (activate, looping) in [(false, false), (true, false), (true, true)] {
            let mut canvas = SceneModel::new(100, 100);
            let clip = canvas
                .lottie_with_options(
                    &path,
                    crate::canvas::LottieOptions {
                        offset: 0.25,
                        duration: Some(0.5),
                        speed: 2.0,
                        looping,
                        ..Default::default()
                    },
                )
                .unwrap();
            canvas.wait(1.0); // Freeze the declaration before scheduling playback.
            canvas.segment("lottie", None).unwrap();
            canvas.wait(2.0);
            if activate {
                canvas.play_items(vec![clip.into()]).unwrap();
            }
            canvas.wait(1.0);
            let mut world = compile_canvas_for_layout(canvas);
            world.insert_resource(gaanim_animation::PlaybackState::default());
            let mut schedule = Schedule::default();
            schedule.add_systems(sample_lottie_system);
            let mut sample = |time| {
                world
                    .resource_mut::<gaanim_animation::PlaybackState>()
                    .current_time = time;
                schedule.run(&mut world);
                world
                    .query::<&LottiePlayer>()
                    .single(&world)
                    .unwrap()
                    .scene()
                    .encoding()
                    .transforms
                    .clone()
            };
            let first = sample(0.0);
            assert_eq!(sample(2.9), first, "hold the source offset before play");
            assert_eq!(
                sample(3.0),
                first,
                "play begins at the absolute scene cursor"
            );
            let middle = sample(3.125);
            if activate {
                assert_ne!(middle, first, "Scene.play must produce visible motion");
                if looping {
                    assert_eq!(sample(3.375), middle, "repeat the selected source interval");
                } else {
                    assert_ne!(sample(3.5), middle, "advance to the final frame");
                    assert_eq!(sample(3.5), sample(4.0), "hold the final frame");
                }
            } else {
                assert_eq!(middle, first, "an unplayed declaration must remain still");
            }
            assert_eq!(sample(0.0), first, "backward seek restores the first frame");
            assert_eq!(sample(3.125), middle, "forward seek is deterministic");
        }
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn dotlottie_commands_survive_compilation_and_exact_seeks() {
        use gaanim_renderer::lottie::{
            LottieInput, LottiePackageOptions, LottiePlayer, sample_lottie_system,
        };
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../examples/assets/dotlottie_demo.lottie");
        let mut canvas = SceneModel::new(200, 160);
        canvas.preload(std::slice::from_ref(&path)).unwrap();
        let clip = canvas
            .lottie_with_package_options(
                &path,
                Default::default(),
                LottiePackageOptions {
                    state_machine_id: Some("main".into()),
                    ..Default::default()
                },
            )
            .unwrap();
        assert_eq!(clip.animation_ids(), ["idle", "active"]);
        assert!(clip.clone().fire_event("reset").is_err());
        canvas.play(vec![clip.drawable.write(1.0)]);
        let start = canvas.current_time();
        canvas.play_items(vec![clip.clone().into()]).unwrap();
        assert_eq!(
            canvas.current_time(),
            start,
            "machine activation is non-blocking"
        );
        canvas.wait(1.0);
        clip.clone()
            .set_input("active", LottieInput::Boolean(true))
            .unwrap();
        clip.clone().set_theme(Some("gold")).unwrap();
        canvas.wait(1.0);
        clip.clone().fire_event("reset").unwrap();
        canvas.wait(1.0);
        clip.set_theme(None).unwrap();
        canvas.wait(1.0);
        let (mut world, mut timeline) = compile_camera_timeline(canvas);
        world.insert_resource(gaanim_animation::PlaybackState::default());
        let mut schedule = Schedule::default();
        schedule.add_systems(sample_lottie_system);
        let mut sample = |time| {
            timeline.seek(&mut world, time);
            world
                .resource_mut::<gaanim_animation::PlaybackState>()
                .current_time = time;
            schedule.run(&mut world);
            let player = world.query::<&LottiePlayer>().single(&world).unwrap();
            let encoding = player.scene().encoding();
            (
                encoding.path_data.clone(),
                encoding.draw_data.clone(),
                encoding.transforms.clone(),
            )
        };
        let times = [0.0, 0.35, 1.0, 1.5, 2.0, 2.5, 3.0, 4.0];
        let expected: Vec<_> = times.iter().map(|t| sample(*t)).collect();
        assert_ne!(expected[0], expected[1]);
        assert_ne!(expected[3], expected[4]);
        assert_ne!(
            expected[6], expected[7],
            "theme resets even at an unchanged source frame"
        );
        for index in (0..times.len()).rev() {
            assert_eq!(sample(times[index]), expected[index]);
        }
    }

    #[test]
    fn lottie_write_and_create_reveal_vectors_before_playback() {
        use gaanim_renderer::lottie::{LottiePlayer, sample_lottie_system};
        let path = lottie_test_asset("reveal");
        for write in [false, true] {
            let mut canvas = SceneModel::new(100, 100);
            let clip = canvas.lottie(&path).unwrap();
            let animation = if write {
                clip.drawable.write(1.0)
            } else {
                clip.drawable.create(1.0)
            };
            canvas.play(vec![animation]);
            canvas.play_items(vec![clip.into()]).unwrap();
            let (mut world, mut timeline) = compile_camera_timeline(canvas);
            world.insert_resource(gaanim_animation::PlaybackState::default());
            let mut schedule = Schedule::default();
            schedule.add_systems(sample_lottie_system);
            let mut sample = |time| {
                timeline.seek(&mut world, time);
                world
                    .resource_mut::<gaanim_animation::PlaybackState>()
                    .current_time = time;
                schedule.run(&mut world);
                let player = world.query::<&LottiePlayer>().single(&world).unwrap();
                let encoded = player.scene().encoding();
                (
                    encoded.path_data.clone(),
                    encoded.draw_data.clone(),
                    encoded.transforms.clone(),
                )
            };
            let empty = sample(0.0);
            let outline = sample(0.35);
            let fill = sample(0.85);
            let first = sample(1.0);
            assert!(
                outline.0.len() > empty.0.len(),
                "draw visible contours during reveal"
            );
            assert_ne!(
                outline, fill,
                "cross-fade the authored fill after the outline"
            );
            assert_ne!(fill, first, "finish the fill phase");
            assert_ne!(sample(1.5).2, first.2, "start source playback after reveal");
            assert_eq!(sample(0.0), empty, "rewind hides the composition");
            assert_eq!(sample(0.35), outline, "rewind restores partial outlines");
            assert_eq!(
                sample(1.0),
                first,
                "reveal returns to the first source frame"
            );
        }
        let _ = std::fs::remove_file(path);
    }

    fn only_text_root(
        world: &mut World,
    ) -> (gaanim_math::Bounds3D, TextBaseline, SpatialTransform) {
        let roots = world
            .query::<(
                &LocalBounds,
                &TextBaseline,
                &SpatialTransform,
                Option<&bevy::prelude::ChildOf>,
            )>()
            .iter(world)
            .filter_map(|(bounds, baseline, transform, parent)| {
                parent
                    .is_none()
                    .then_some((bounds.0, *baseline, *transform))
            })
            .collect::<Vec<_>>();
        assert_eq!(roots.len(), 1, "expected exactly one text root");
        roots[0]
    }

    fn assert_point_close(actual: DVec3, expected: DVec3) {
        assert!(
            actual.distance(expected) < 1e-6,
            "expected {expected:?}, got {actual:?}"
        );
    }

    #[test]
    fn at_anchor_point_places_center_on_transformed_reference_anchor() {
        let mut canvas = SceneModel::new(640, 360);
        let reference = canvas
            .rect(100.0, 40.0)
            .move_to(30.0, -10.0)
            .scale_to(1.5)
            .rotate_to(std::f64::consts::FRAC_PI_2);
        let point = reference.anchor_point(Anchor::TopRight, DVec3::new(5.0, -3.0, 0.0));
        canvas.rect(10.0, 6.0).at_anchor_point(point);

        let mut world = compile_canvas_for_layout(canvas);
        let mut roots = world
            .query::<(
                &LocalBounds,
                &SpatialTransform,
                Option<&bevy::prelude::ChildOf>,
            )>()
            .iter(&world)
            .filter_map(|(bounds, transform, parent)| {
                parent.is_none().then_some((bounds.0, *transform))
            })
            .collect::<Vec<_>>();
        roots.sort_by(|(left, _), (right, _)| left.width().total_cmp(&right.width()));
        assert_eq!(roots.len(), 2);

        let (target_bounds, target_transform) = roots[0];
        let (reference_bounds, reference_transform) = roots[1];
        let reference_local =
            Anchor::TopRight.get_point(&reference_bounds) + DVec3::new(5.0, -3.0, 0.0);
        assert_point_close(
            target_transform
                .to_mat4()
                .transform_point3(target_bounds.center()),
            reference_transform
                .to_mat4()
                .transform_point3(reference_local),
        );
    }

    #[test]
    fn default_single_line_text_keeps_its_baseline_center_after_transform_layout() {
        for source in ["HAPPY", "gyp", "$frac(x_1^2, y_2) = 1$"] {
            let mut canvas = SceneModel::new(640, 360);
            canvas
                .text(source)
                .at_text_default(73.0, -41.0)
                .scale_to(1.65)
                .rotate_to(0.23)
                .with_pivot(-120.0, 95.0);

            let mut world = compile_canvas_for_layout(canvas);
            let (bounds, baseline, transform) = only_text_root(&mut world);
            let local_anchor = DVec3::new(bounds.center().x, baseline.0, bounds.center().z);
            assert_point_close(
                transform.to_mat4().transform_point3(local_anchor),
                DVec3::new(73.0, -41.0, 0.0),
            );
        }
    }

    #[test]
    fn unpositioned_single_line_text_places_its_baseline_center_at_the_origin() {
        let mut canvas = SceneModel::new(16.0, 9.0);
        canvas.text("Hola mundo");

        let mut world = compile_canvas_for_layout(canvas);
        let (bounds, baseline, transform) = only_text_root(&mut world);
        let local_anchor = DVec3::new(bounds.center().x, baseline.0, bounds.center().z);
        assert_point_close(
            transform.to_mat4().transform_point3(local_anchor),
            DVec3::ZERO,
        );
    }

    #[test]
    fn equation_transform_hands_off_the_target_baseline_at_the_endpoint() {
        use gaanim_text::prelude::{TextContent, TextFlow, TextRole, TextSpec, TextStyle};

        let equation = |source: &str| {
            TextSpec::new(
                vec![TextContent::Literal(format!("$ {source} $"))],
                Some(TextRole::Title),
                TextStyle::default(),
                TextFlow::default(),
            )
            .expect("valid equation")
        };
        let mut canvas = SceneModel::new(16.0, 9.0);
        let source = canvas
            .text_spec(equation("integral_(-infinity)^infinity = x^2 d x"))
            .scale_by(3.0)
            .at_text_default(0.0, 0.0);
        let target = canvas
            .text_spec(equation("x^2 d x"))
            .scale_by(3.0)
            .at_text_default(0.0, 0.0);
        canvas.play(vec![source.transform(&target)]);
        canvas.wait(0.1);

        let world = World::new();
        let mut queue = CommandQueue::default();
        let mut commands = Commands::new(&mut queue, &world);
        let mut timeline = Timeline::new();
        let fonts = gaanim_text::font::FontRegistry::new();
        let text_config = gaanim_text::prelude::TextConfig::default();
        canvas.compile_into(&mut commands, &mut timeline, &fonts, &text_config);

        let mut world = world;
        queue.apply(&mut world);
        let target_baseline = world
            .query::<(&LocalBounds, &TextBaseline, Option<&bevy::prelude::ChildOf>)>()
            .iter(&world)
            .filter(|(_, _, parent)| parent.is_none())
            .min_by(|(left, ..), (right, ..)| left.0.width().total_cmp(&right.0.width()))
            .map(|(_, baseline, _)| baseline.0)
            .expect("target text root");

        timeline.add_keyframe(0.0, WorldSnapshot::capture(&mut world));
        timeline.seek(&mut world, 1.0);
        let transformed_baseline = world
            .query::<(
                &gaanim_scene::Path2D,
                &TextBaseline,
                Option<&bevy::prelude::ChildOf>,
            )>()
            .iter(&world)
            .find_map(|(path, baseline, parent)| {
                (parent.is_none() && !path.0.elements().is_empty()).then_some(baseline.0)
            })
            .expect("flattened transformed text root");

        assert!((transformed_baseline - target_baseline).abs() < 1.0e-9);
    }

    #[test]
    fn shifting_unpositioned_text_preserves_its_implicit_baseline_anchor() {
        let mut canvas = SceneModel::new(16.0, 9.0);
        canvas.text("Hola mundo").shift_by(2.0, -1.0);

        let mut world = compile_canvas_for_layout(canvas);
        let (bounds, baseline, transform) = only_text_root(&mut world);
        let local_anchor = DVec3::new(bounds.center().x, baseline.0, bounds.center().z);
        assert_point_close(
            transform.to_mat4().transform_point3(local_anchor),
            DVec3::new(2.0, -1.0, 0.0),
        );
    }

    #[test]
    fn unpositioned_multiline_text_keeps_its_visual_center_at_the_origin() {
        let mut canvas = SceneModel::new(16.0, 9.0);
        canvas.text("Primera línea\nSegunda línea");

        let mut world = compile_canvas_for_layout(canvas);
        let (bounds, _, transform) = only_text_root(&mut world);
        assert_point_close(
            transform.to_mat4().transform_point3(bounds.center()),
            DVec3::ZERO,
        );
    }

    #[test]
    fn explicit_text_baseline_anchors_place_left_center_and_right_edges() {
        for (anchor, x_selector) in [
            (
                TextAnchor::BaselineLeft,
                fn_x_min as fn(&gaanim_math::Bounds3D) -> f64,
            ),
            (TextAnchor::BaselineCenter, fn_x_center),
            (TextAnchor::BaselineRight, fn_x_max),
        ] {
            let mut canvas = SceneModel::new(640, 360);
            canvas.text("gyp").at_text_anchor(17.0, 29.0, anchor);

            let mut world = compile_canvas_for_layout(canvas);
            let (bounds, baseline, transform) = only_text_root(&mut world);
            let local_anchor = DVec3::new(x_selector(&bounds), baseline.0, bounds.center().z);
            assert_point_close(
                transform.to_mat4().transform_point3(local_anchor),
                DVec3::new(17.0, 29.0, 0.0),
            );
        }

        fn fn_x_min(bounds: &gaanim_math::Bounds3D) -> f64 {
            bounds.min.x
        }
        fn fn_x_center(bounds: &gaanim_math::Bounds3D) -> f64 {
            bounds.center().x
        }
        fn fn_x_max(bounds: &gaanim_math::Bounds3D) -> f64 {
            bounds.max.x
        }
    }

    #[test]
    fn multiline_default_is_visual_center_but_explicit_anchor_uses_first_baseline() {
        let mut default_canvas = SceneModel::new(640, 360);
        default_canvas
            .text("First line\nsecond line")
            .at_text_default(-35.0, 61.0);
        let mut default_world = compile_canvas_for_layout(default_canvas);
        let (bounds, _, transform) = only_text_root(&mut default_world);
        assert_point_close(
            transform.to_mat4().transform_point3(bounds.center()),
            DVec3::new(-35.0, 61.0, 0.0),
        );

        let mut explicit_canvas = SceneModel::new(640, 360);
        explicit_canvas
            .text("First line\nsecond line")
            .at_text_anchor(-35.0, 61.0, TextAnchor::BaselineLeft);
        let mut explicit_world = compile_canvas_for_layout(explicit_canvas);
        let (bounds, baseline, transform) = only_text_root(&mut explicit_world);
        assert_point_close(
            transform.to_mat4().transform_point3(DVec3::new(
                bounds.min.x,
                baseline.0,
                bounds.center().z,
            )),
            DVec3::new(-35.0, 61.0, 0.0),
        );
        assert!(
            (baseline.0 - bounds.center().y).abs() > 0.01,
            "first baseline must remain distinct from the multiline visual center"
        );
    }

    #[test]
    fn geometric_top_left_places_the_corner_without_promising_a_baseline() {
        let mut baselines = Vec::new();
        for source in ["HAPPY", "gyp"] {
            let mut canvas = SceneModel::new(640, 360);
            canvas.text(source).at_anchor(12.0, 34.0, Anchor::TopLeft);
            let mut world = compile_canvas_for_layout(canvas);
            let (bounds, baseline, transform) = only_text_root(&mut world);
            assert_point_close(
                transform
                    .to_mat4()
                    .transform_point3(Anchor::TopLeft.get_point(&bounds)),
                DVec3::new(12.0, 34.0, 0.0),
            );
            baselines.push(
                transform
                    .to_mat4()
                    .transform_point3(DVec3::new(bounds.min.x, baseline.0, 0.0))
                    .y,
            );
        }
        assert!(
            (baselines[0] - baselines[1]).abs() > 0.1,
            "geometric top-left alignment must not imply equal baselines: {baselines:?}"
        );
    }

    #[test]
    fn reactive_reveal_ends_at_exact_data_coordinate() {
        let map = gaanim_visualization::CoordinateMap2D::new(
            gaanim_visualization::Axis::linear(0.0, 3.0 * std::f64::consts::PI).unwrap(),
            gaanim_visualization::Axis::linear(-1.0, 1.0).unwrap(),
            gaanim_visualization::PlotFrame::new(600.0, 240.0).unwrap(),
        );
        let function = ReactiveFunction::new(1, 1, vec![], |values| Ok(vec![values[0].sin()]));
        let reveal = ScalarSource::constant(std::f64::consts::FRAC_PI_2);
        let path = sampled_reactive_path(
            &map,
            &function,
            (0.0, 3.0 * std::f64::consts::PI),
            Some(&reveal),
            gaanim_visualization::Sampling::Fixed { samples: 65 },
            0.0,
            &[],
        );

        assert!((path.bounding_box().x1 + 200.0).abs() < 1e-9);
        assert!(
            sampled_reactive_path(
                &map,
                &function,
                (0.0, 3.0 * std::f64::consts::PI),
                Some(&ScalarSource::constant(0.0)),
                gaanim_visualization::Sampling::Fixed { samples: 65 },
                0.0,
                &[],
            )
            .is_empty()
        );
    }

    #[test]
    fn reactive_geometry_matches_across_direct_rewind_and_repeated_seeks() {
        let mut canvas = SceneModel::new(640, 360);
        let amplitude = canvas.parameter(1.0).unwrap();
        let amplitude_id = amplitude.drawable().id;
        let space = canvas
            .coordinate_axes(
                gaanim_visualization::Axis::linear(-2.0, 2.0).unwrap(),
                gaanim_visualization::Axis::linear(-3.0, 3.0).unwrap(),
                Some(400.0),
                Some(240.0),
                false,
            )
            .unwrap();
        canvas
            .reactive_plot(
                &space,
                ReactiveFunction::new(
                    1,
                    1,
                    vec![
                        gaanim_animation::ReactiveInput::Signal(amplitude_id),
                        gaanim_animation::ReactiveInput::Time,
                    ],
                    |values| Ok(vec![values[1] * (values[0] + values[2]).sin()]),
                ),
                (-2.0, 2.0),
                gaanim_visualization::Sampling::Fixed { samples: 33 },
            )
            .unwrap();
        canvas.play(vec![amplitude.animate().set(2.0).duration(2.0)]);

        let mut world = World::new();
        world.insert_resource(gaanim_animation::PlaybackState::default());
        let mut queue = CommandQueue::default();
        let mut commands = Commands::new(&mut queue, &world);
        let mut timeline = Timeline::new();
        let fonts = gaanim_text::font::FontRegistry::new();
        let text_config = gaanim_text::prelude::TextConfig::default();
        canvas.compile_into(&mut commands, &mut timeline, &fonts, &text_config);
        queue.apply(&mut world);
        timeline.add_keyframe(0.0, WorldSnapshot::capture(&mut world));
        let plot = world
            .query_filtered::<Entity, With<gaanim_animation::AlwaysRedrawRegen>>()
            .single(&world)
            .expect("one reactive plot");

        let mut capture = |time| {
            timeline.seek(&mut world, time);
            gaanim_animation::always_redraw_regen_system(&mut world);
            world
                .get::<gaanim_scene::Path2D>(plot)
                .expect("reactive path")
                .0
                .elements()
                .to_vec()
        };
        let direct = capture(1.5);
        let _earlier = capture(0.25);
        let rewind = capture(1.5);
        let _later = capture(1.9);
        let repeated = capture(1.5);

        assert_eq!(direct, rewind);
        assert_eq!(direct, repeated);
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

    impl UnifiedTextFixture for SceneModel {
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
    fn structured_math_parts_preserve_native_typst_whitespace() {
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
        assert_eq!(source, "$x dot 5 = 25$");
        assert!(!source.contains("$$"));
        assert!(!source.contains("#h(0pt)"));
    }

    #[test]
    fn adjacent_sibling_parts_use_typst_spacing_only_inside_shared_math() {
        let part = |name: &str, text: &str, style: StructuredTextStyle| {
            gaanim_text::prelude::TextPart::new(name, vec![text.into()], style).into()
        };
        let spec = |content| {
            StructuredTextSpec::new(
                content,
                None,
                StructuredTextStyle::default(),
                gaanim_text::prelude::TextFlow::default(),
            )
            .expect("valid structured text")
        };

        let implicit = spec(vec![
            "$".into(),
            part("left", "a", StructuredTextStyle::default()),
            part("right", "b", StructuredTextStyle::default()),
            "$".into(),
        ]);
        assert_eq!(structured_typst_content(&implicit, 32.0), "$a b$");

        let explicit = spec(vec![
            "$".into(),
            part("left", "a ", StructuredTextStyle::default()),
            part("right", " b", StructuredTextStyle::default()),
            "$".into(),
        ]);
        let explicit_source = structured_typst_content(&explicit, 32.0);
        assert_eq!(explicit_source, "$a b$");

        let prose = spec(vec![
            part("left", "a", StructuredTextStyle::default()),
            part("right", "b", StructuredTextStyle::default()),
        ]);
        assert_eq!(structured_typst_content(&prose, 32.0), "#text(\"ab\")");

        let tight_literal = spec(vec![
            "$".into(),
            part("variable", "x", StructuredTextStyle::default()),
            "_1".into(),
            "$".into(),
        ]);
        assert_eq!(structured_typst_content(&tight_literal, 32.0), "$x _1$");

        let colored = StructuredTextStyle {
            color: Some(PenikoColor::from_rgb8(255, 0, 0)),
            ..StructuredTextStyle::default()
        };
        let distributed_delimiters = spec(vec![
            part("left", "$a", StructuredTextStyle::default()),
            part("right", "b$", colored),
        ]);
        let distributed_source = structured_typst_content(&distributed_delimiters, 32.0);
        assert!(!distributed_source.contains("#h("));
        assert!(distributed_source.starts_with("$a text("));
        assert!(distributed_source.contains("ff0000"));
    }

    #[test]
    fn all_math_content_boundaries_receive_typst_whitespace() {
        let part = |name: &str, text: &str, style: StructuredTextStyle| {
            gaanim_text::prelude::TextPart::new(name, vec![text.into()], style).into()
        };
        let spec = |content| {
            StructuredTextSpec::new(
                content,
                None,
                StructuredTextStyle::default(),
                gaanim_text::prelude::TextFlow::default(),
            )
            .expect("valid structured equation")
        };

        let equation = spec(vec![
            "$".into(),
            part("sum_force", "sum F_t", StructuredTextStyle::default()),
            "=".into(),
            part("mass", "m", StructuredTextStyle::default()),
            part("acceleration", "a_t", StructuredTextStyle::default()),
            "$".into(),
        ]);
        assert_eq!(
            structured_typst_content(&equation, 32.0),
            "$sum F_t = m a_t$"
        );

        let tight_syntax = spec(vec![
            "$-".into(),
            part("variable", "x", StructuredTextStyle::default()),
            "_1".into(),
            "$".into(),
        ]);
        assert_eq!(structured_typst_content(&tight_syntax, 32.0), "$- x _1$");

        let colored = StructuredTextStyle {
            color: Some(PenikoColor::from_rgb8(255, 0, 0)),
            ..StructuredTextStyle::default()
        };
        let styled_right = spec(vec![
            "$".into(),
            part("left", "x", StructuredTextStyle::default()),
            "=".into(),
            part("right", "y", colored),
            "$".into(),
        ]);
        let styled_source = structured_typst_content(&styled_right, 32.0);
        assert!(styled_source.starts_with("$x = text("));
        assert!(!styled_source.contains("#h("));
        assert!(styled_source.contains("ff0000"));

        let display_wrapped = spec(vec![
            "$ ".into(),
            part("variable", "x", StructuredTextStyle::default()),
            "= 1".into(),
            " $".into(),
        ]);
        assert_eq!(
            structured_typst_content(&display_wrapped, 32.0),
            "$ x = 1 $"
        );

        let inline = spec(vec!["$x".into(), "+".into(), "1$".into()]);
        assert_eq!(structured_typst_content(&inline, 32.0), "$x + 1$");
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
    fn disabled_inline_markup_emits_literal_delimiters() {
        let spec = StructuredTextSpec::new_with_markup(
            vec!["tb:agriet_xy, *nota y $x_1$".into()],
            None,
            StructuredTextStyle::default(),
            gaanim_text::prelude::TextFlow::default(),
            false,
        )
        .expect("markup-free text accepts lone delimiters");

        let source = structured_typst_content(&spec, 32.0);
        assert!(source.contains("tb:agriet_xy, *nota y "));
        assert!(source.contains("$x_1$"));
        assert!(!source.contains("weight: 700"));
        assert!(!source.contains("italic"));
    }

    #[test]
    fn explicit_math_boundary_spaces_match_implicit_typst_whitespace() {
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
        let mut canvas = SceneModel::new(640, 360);
        canvas.text_spec(equation(false));
        canvas.text_spec(equation(true));

        let world = World::new();
        let mut queue = CommandQueue::default();
        let mut commands = Commands::new(&mut queue, &world);
        let mut timeline = Timeline::new();
        let fonts = gaanim_text::font::FontRegistry::new();
        let text_config = gaanim_text::prelude::TextConfig::default();
        canvas.compile_into(&mut commands, &mut timeline, &fonts, &text_config);
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
            (widths[1] - widths[0]).abs() < 0.01,
            "explicit and implicit boundary spaces must compile identically: {widths:?}"
        );
    }

    #[test]
    fn local_math_fill_preserves_unstyled_typst_spacing_and_width() {
        let equation = |colored: bool| {
            let mut gravity_style = StructuredTextStyle::default();
            let mut acceleration_style = StructuredTextStyle::default();
            if colored {
                gravity_style.color = Some(PenikoColor::from_rgb8(255, 215, 0));
                acceleration_style.color = Some(PenikoColor::from_rgb8(255, 215, 0));
            }
            StructuredTextSpec::new(
                vec![
                    "$ ".into(),
                    "-".into(),
                    gaanim_text::prelude::TextPart::new(
                        "mass_left",
                        vec!["m".into()],
                        StructuredTextStyle::default(),
                    )
                    .into(),
                    gaanim_text::prelude::TextPart::new(
                        "gravity",
                        vec!["g sin(theta)".into()],
                        gravity_style,
                    )
                    .into(),
                    "=".into(),
                    gaanim_text::prelude::TextPart::new(
                        "mass_right",
                        vec!["m".into()],
                        StructuredTextStyle::default(),
                    )
                    .into(),
                    gaanim_text::prelude::TextPart::new(
                        "length",
                        vec!["L".into()],
                        StructuredTextStyle::default(),
                    )
                    .into(),
                    gaanim_text::prelude::TextPart::new(
                        "acceleration",
                        vec!["theta''".into()],
                        acceleration_style,
                    )
                    .into(),
                    " $".into(),
                ],
                None,
                StructuredTextStyle::default(),
                gaanim_text::prelude::TextFlow::default(),
            )
            .expect("valid display equation")
        };

        let plain_source = structured_typst_content(&equation(false), 32.0);
        let colored_source = structured_typst_content(&equation(true), 32.0);
        assert_eq!(plain_source, "$ - m g sin(theta) = m L theta'' $");
        assert!(
            !colored_source.contains("#h("),
            "local color must not replace Typst parser spacing: {colored_source}"
        );

        let mut canvas = SceneModel::new(640, 360);
        canvas.text_spec(equation(false));
        canvas.text_spec(equation(true));
        let world = World::new();
        let mut queue = CommandQueue::default();
        let mut commands = Commands::new(&mut queue, &world);
        let mut timeline = Timeline::new();
        let fonts = gaanim_text::font::FontRegistry::new();
        let text_config = gaanim_text::prelude::TextConfig::default();
        canvas.compile_into(&mut commands, &mut timeline, &fonts, &text_config);
        let mut world = world;
        queue.apply(&mut world);

        let mut widths = world
            .query::<(&LocalBounds, Option<&bevy::prelude::ChildOf>)>()
            .iter(&world)
            .filter_map(|(bounds, parent)| parent.is_none().then_some(bounds.0.width()))
            .collect::<Vec<_>>();
        widths.sort_by(f64::total_cmp);
        assert_eq!(widths.len(), 2, "expected two text roots: {widths:?}");
        assert!(
            (widths[1] - widths[0]).abs() < 0.01,
            "local fill changed equation width: {widths:?}"
        );
    }

    #[test]
    fn every_local_math_style_stays_inside_one_typst_equation() {
        let style = StructuredTextStyle {
            font: Some("New Computer Modern".to_owned()),
            math_font: Some("New Computer Modern Math".to_owned()),
            fallbacks: vec!["Consolas".to_owned()],
            size: Some(36.0),
            weight: Some(700),
            italic: Some(false),
            color: Some(PenikoColor::from_rgb8(255, 215, 0)),
            stroke_color: Some(PenikoColor::from_rgb8(20, 20, 20)),
            stroke_width: Some(0.25),
            opacity: Some(0.8),
            letter_spacing: Some(0.25),
            word_spacing: Some(0.5),
            decorations: vec!["underline".to_owned()],
            baseline: Some(1.0),
        };
        let equation = StructuredTextSpec::new(
            vec![
                "$ ".into(),
                gaanim_text::prelude::TextPart::new(
                    "left",
                    vec!["x".into()],
                    StructuredTextStyle::default(),
                )
                .into(),
                "=".into(),
                gaanim_text::prelude::TextPart::new("right", vec!["y".into()], style).into(),
                " $".into(),
            ],
            None,
            StructuredTextStyle::default(),
            gaanim_text::prelude::TextFlow::default(),
        )
        .expect("valid styled display equation");

        let source = structured_typst_content(&equation, 32.0);
        assert!(
            source.starts_with("$ ") && source.ends_with(" $"),
            "{source}"
        );
        assert!(!source.contains("#h("), "{source}");

        let mut canvas = SceneModel::new(640, 360);
        canvas.text_spec(equation);
        let world = World::new();
        let mut queue = CommandQueue::default();
        let mut commands = Commands::new(&mut queue, &world);
        let mut timeline = Timeline::new();
        let fonts = gaanim_text::font::FontRegistry::new();
        let text_config = gaanim_text::prelude::TextConfig::default();
        canvas.compile_into(&mut commands, &mut timeline, &fonts, &text_config);
        let mut world = world;
        queue.apply(&mut world);
        assert_eq!(
            world
                .query::<(&LocalBounds, Option<&bevy::prelude::ChildOf>)>()
                .iter(&world)
                .filter(|(_, parent)| parent.is_none())
                .count(),
            1
        );
        assert!(
            world
                .query::<&bevy::prelude::ChildOf>()
                .iter(&world)
                .count()
                > 0,
            "the styled equation must compile vector children"
        );
    }

    #[test]
    fn implicit_sibling_part_boundary_compiles_as_valid_math_tokens() {
        let equation = StructuredTextSpec::new(
            vec![
                "$".into(),
                gaanim_text::prelude::TextPart::new(
                    "left",
                    vec!["m".into()],
                    StructuredTextStyle::default(),
                )
                .into(),
                gaanim_text::prelude::TextPart::new(
                    "right",
                    vec!["g".into()],
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
        assert_eq!(structured_typst_content(&equation, 32.0), "$m g$");
        let mut canvas = SceneModel::new(640, 360);
        canvas.text_spec(equation);

        let world = World::new();
        let mut queue = CommandQueue::default();
        let mut commands = Commands::new(&mut queue, &world);
        let mut timeline = Timeline::new();
        let fonts = gaanim_text::font::FontRegistry::new();
        let text_config = gaanim_text::prelude::TextConfig::default();
        canvas.compile_into(&mut commands, &mut timeline, &fonts, &text_config);
        let mut world = world;
        queue.apply(&mut world);

        let widths = world
            .query::<(&LocalBounds, Option<&bevy::prelude::ChildOf>)>()
            .iter(&world)
            .filter_map(|(bounds, parent)| parent.is_none().then_some(bounds.0.width()))
            .collect::<Vec<_>>();
        assert_eq!(widths.len(), 1, "expected one text root: {widths:?}");
        assert!(widths[0] > 0.0, "compiled math must have positive width");
    }

    #[test]
    fn camera_and_drawable_play_compile_at_the_same_start_time() {
        let mut canvas = SceneModel::new(640, 360);
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
        assert_eq!(parallel_clips.len(), 2); // fade + one atomic orbit pose
        assert!(parallel_clips.iter().any(|clip| matches!(
            &clip.payload,
            gaanim_timeline::clip::ClipPayload::Animation(animation)
                if matches!(
                    animation.lens,
                    gaanim_timeline::clip::PropertyLensSpec::CameraOrbit { .. }
                )
        )));
        assert!(parallel_clips.iter().all(|clip| clip.start.abs() < 1e-9));
        assert!((timeline.cached_duration - 2.0).abs() < 1e-9);
    }

    fn compile_camera_timeline(canvas: SceneModel) -> (World, Timeline) {
        let mut world = World::new();
        world.insert_resource(gaanim_math::Camera::ortho_2d(1280, 720));
        let mut queue = CommandQueue::default();
        let mut commands = Commands::new(&mut queue, &world);
        let mut timeline = Timeline::new();
        let fonts = gaanim_text::font::FontRegistry::new();
        let text_config = gaanim_text::prelude::TextConfig::default();
        canvas.compile_into(&mut commands, &mut timeline, &fonts, &text_config);
        queue.apply(&mut world);
        timeline.add_keyframe(0.0, WorldSnapshot::capture(&mut world));
        (world, timeline)
    }

    #[test]
    fn named_camera_state_restores_complete_authored_pose() {
        let mut canvas = SceneModel::new(960, 540);
        let _marker = canvas.circle(1.0);
        let pan_to_detail = canvas.camera_pan_to(120.0, -30.0, 1.0);
        canvas.play(vec![pan_to_detail]);
        let saved = canvas.camera_save("detail").unwrap();
        let pan_home = canvas.camera_pan_to(0.0, 0.0, 1.0);
        canvas.play(vec![pan_home]);
        let restored = canvas.camera_restore("detail", 1.0).unwrap();
        canvas.play(vec![restored.clone()]);
        let _ = saved;
        assert!(matches!(
            restored.inner.anim_type,
            AnimationType::CameraState { .. }
        ));

        let (mut world, mut timeline) = compile_camera_timeline(canvas);
        timeline.seek(&mut world, 3.0);
        let camera = world.resource::<gaanim_math::Camera>();
        assert!((camera.position - DVec3::new(120.0, -30.0, 0.0)).length() < 1e-9);

        timeline.seek(&mut world, 0.25);
        timeline.seek(&mut world, 3.0);
        let camera = world.resource::<gaanim_math::Camera>();
        assert!((camera.position - DVec3::new(120.0, -30.0, 0.0)).length() < 1e-9);
    }

    #[test]
    fn camera_state_handles_validate_names_ownership_and_projection() {
        let first = SceneModel::new(960, 540);
        let second = SceneModel::new(960, 540);
        let state = first
            .camera_state_3d(
                DVec3::new(7.0, 5.0, 6.0),
                DVec3::ZERO,
                DVec3::Y,
                0.8,
                0.1,
                1000.0,
            )
            .unwrap();
        assert_eq!(
            second.camera_to(&state, 1.0).unwrap_err(),
            crate::canvas::CameraStateError::ForeignScene
        );
        assert_eq!(
            first.camera_save("").unwrap_err(),
            crate::canvas::CameraStateError::EmptyName
        );
        assert_eq!(
            first.camera_restore("missing", 1.0).unwrap_err(),
            crate::canvas::CameraStateError::UnknownName("missing".into())
        );
        assert!(first.camera_state_2d(DVec2::ZERO, 0.0, 0.0).is_err());
    }

    #[test]
    fn camera_capture_freezes_reactive_value_at_its_cursor() {
        let mut canvas = SceneModel::new(960, 540);
        let parameter = canvas.parameter(0.0).unwrap();
        let parameter_id = parameter.drawable().id;
        let point = canvas.point_ref(
            ScalarSource::function(ReactiveFunction::new(
                0,
                1,
                vec![gaanim_animation::ReactiveInput::Signal(parameter_id)],
                |values| Ok(vec![values[0] * 260.0 - 130.0]),
            ))
            .unwrap(),
            ScalarSource::constant(25.0),
        );
        let parameter_anim = parameter.animate().set(1.0).duration(1.0);
        let camera_anim = canvas.camera_pan_to_endpoint(point.0, 1.0);
        canvas.play(vec![parameter_anim, camera_anim]);
        let captured = canvas.camera_capture();
        let pan_home = canvas.camera_pan_to(0.0, 0.0, 1.0);
        canvas.play(vec![pan_home]);
        let restore_capture = canvas.camera_to(&captured, 1.0).unwrap();
        canvas.play(vec![restore_capture]);

        let (mut world, mut timeline) = compile_camera_timeline(canvas);
        timeline.seek(&mut world, 3.0);
        let camera = world.resource::<gaanim_math::Camera>();
        assert!((camera.position - DVec3::new(130.0, 25.0, 0.0)).length() < 1e-9);
    }

    #[test]
    fn camera_state_transitions_between_2d_and_3d() {
        let mut canvas = SceneModel::new(960, 540);
        let _marker = canvas.circle(1.0);
        let perspective = canvas
            .camera_state_3d(
                DVec3::new(7.0, 5.0, 6.0),
                DVec3::ZERO,
                DVec3::Y,
                0.8,
                0.1,
                1000.0,
            )
            .unwrap();
        let to_perspective = canvas.camera_to(&perspective, 1.0).unwrap();
        canvas.play(vec![to_perspective]);
        let orthographic = canvas
            .camera_state_2d(DVec2::new(40.0, -20.0), 1.5, 0.2)
            .unwrap();
        let to_orthographic = canvas.camera_to(&orthographic, 0.0).unwrap();
        canvas.play(vec![to_orthographic]);

        let (mut world, mut timeline) = compile_camera_timeline(canvas);
        timeline.seek(&mut world, 0.0);
        timeline.seek(&mut world, 0.5);
        assert!(matches!(
            world.resource::<gaanim_math::Camera>().projection,
            gaanim_math::Projection::Perspective { .. }
        ));
        timeline.seek(&mut world, 1.0);
        let camera = world.resource::<gaanim_math::Camera>();
        assert_eq!(camera.position, DVec3::new(40.0, -20.0, 0.0));
        assert!(matches!(
            camera.projection,
            gaanim_math::Projection::Orthographic { zoom } if (zoom - 1.5).abs() < 1e-9
        ));
    }

    #[test]
    fn animated_look_at_starts_from_a_valid_perspective_pose() {
        let mut canvas = SceneModel::new(1280, 720);
        let _marker = canvas.circle(1.0);
        let perspective = canvas.camera_perspective(0.8, 0.1, 1000.0, 0.0);
        canvas.play(vec![perspective]);
        let look_at = canvas.camera_look_at((8.0, 6.0, 8.0), (0.0, 0.0, 0.0), None, 1.0);
        canvas.play(vec![look_at]);
        let (mut world, mut timeline) = compile_camera_timeline(canvas);

        timeline.seek(&mut world, 0.0);
        let start = *world.resource::<gaanim_math::Camera>();
        assert!(start.validate().is_ok());
        assert!(matches!(
            start.projection,
            gaanim_math::Projection::Perspective { .. }
        ));
        assert_eq!(start.position, DVec3::new(8.0, 6.0, 8.0));
        assert!((start.position - start.target).length() > 1.0);
        timeline.seek(&mut world, 0.5);
        let middle = *world.resource::<gaanim_math::Camera>();
        assert!(middle.validate().is_ok());
        assert!((middle.position - middle.target).length() > 1.0);
    }

    #[test]
    fn camera_orbit_seek_preserves_authored_radius() {
        let eye = DVec3::new(10.0, 7.0, 13.0);
        let target = DVec3::new(0.0, -0.5, 0.0);
        let radius = (eye - target).length();
        let mut canvas = SceneModel::new(1280, 720);
        let _marker = canvas.circle(1.0);
        let perspective = canvas.camera_perspective(0.8, 0.1, 1000.0, 0.0);
        canvas.play(vec![perspective]);
        let look_at = canvas.camera_look_at_endpoints(
            CanvasEndpoint::Static(eye),
            CanvasEndpoint::Static(target),
            DVec3::Y,
            0.5,
        );
        canvas.play(vec![look_at]);
        let orbit = canvas.camera_orbit(0.8, 0.2, 1.4);
        canvas.play(vec![orbit]);
        let (mut world, mut timeline) = compile_camera_timeline(canvas);

        for time in [0.5, 1.2, 1.9] {
            timeline.seek(&mut world, time);
            let camera = *world.resource::<gaanim_math::Camera>();
            assert!(camera.validate().is_ok(), "invalid camera at {time}");
            assert_eq!(camera.target, target);
            assert!(
                ((camera.position - target).length() - radius).abs() < 1e-9,
                "orbit radius changed at {time}: {:?}",
                camera.position
            );
            let forward = camera.rotation * -DVec3::Z;
            let expected = (target - camera.position).normalize();
            assert!(forward.dot(expected) > 1.0 - 1e-9);
        }
    }

    #[test]
    fn reactive_readout_resolves_body_family_like_scene_text() {
        use gaanim_core::kurbo::Shape as _;

        let mut canvas = SceneModel::new(640, 360);
        canvas.reactive_readout(
            gaanim_animation::ScalarSource::Constant(54.0),
            ".0f",
            "",
            "%",
            "invalid",
            Some(0.5),
        );
        let mut world = World::new();
        let mut queue = CommandQueue::default();
        let mut commands = Commands::new(&mut queue, &world);
        let mut timeline = Timeline::new();
        let fonts = gaanim_text::font::FontRegistry::new();
        let mut config = gaanim_text::prelude::TextConfig::default();
        // Typst embeds Libertinus; the legacy registry does not know it by name.
        config
            .roles
            .get_mut(&gaanim_text::prelude::TextRole::Body)
            .unwrap()
            .font_family = "Libertinus Serif".to_owned();
        canvas.compile_into(&mut commands, &mut timeline, &fonts, &config);
        queue.apply(&mut world);

        let (path, baseline) = world
            .query::<(
                &gaanim_animation::ReactiveReadout,
                &gaanim_scene::PathSource,
                &gaanim_scene::TextBaseline,
            )>()
            .iter(&world)
            .map(|(_, path, baseline)| (path.0.clone(), baseline.0))
            .next()
            .expect("readout spawned");
        let expected = gaanim_text::typst_compiler::shape_typst_text_run(
            &fonts,
            "54%",
            "Libertinus Serif",
            None,
            0.5,
        )
        .unwrap()
        .path;
        let ink = expected.bounding_box();
        let (expected, _) =
            gaanim_animation::right_align_readout_path(expected, Bounds3D::default());
        let (actual, expected) = (path.bounding_box(), expected.bounding_box());
        for delta in [
            actual.x0 - expected.x0,
            actual.y0 - expected.y0,
            actual.x1 - expected.x1,
            actual.y1 - expected.y1,
            baseline + (ink.y0 + ink.y1) * 0.5,
        ] {
            assert!(
                delta.abs() < 1e-9,
                "{actual:?} vs {expected:?}, baseline {baseline}"
            );
        }
    }

    #[test]
    fn late_declared_group_members_stay_hidden_until_fade_in() {
        let mut canvas = SceneModel::new(640, 360);
        canvas.wait(0.8);
        let outline = canvas.circle(20.0);
        let number = canvas.reactive_readout(
            gaanim_animation::ScalarSource::Constant(54.0),
            ".0f",
            "",
            "%",
            "invalid",
            Some(24.0),
        );
        let readout = canvas.reactive_readout_group(None, None, &number, None, 10.0);
        let _group = canvas.group(&[&outline, &readout]);
        let caption = canvas.circle(10.0);
        canvas.play(vec![outline.fade_in(1.0)]);
        canvas.play(vec![readout.fade_in(1.0), caption.fade_in(1.0)]);

        let mut world = World::new();
        let mut queue = CommandQueue::default();
        let mut commands = Commands::new(&mut queue, &world);
        let mut timeline = Timeline::new();
        let fonts = gaanim_text::font::FontRegistry::new();
        let config = gaanim_text::prelude::TextConfig::default();
        canvas.compile_into(&mut commands, &mut timeline, &fonts, &config);
        queue.apply(&mut world);
        timeline.add_keyframe(0.0, WorldSnapshot::capture(&mut world));

        for time in [0.0, 1.3, 2.3, 3.0, 1.3, 0.0] {
            timeline.seek(&mut world, time);
            for handle in [&readout, &caption] {
                let id = ObjectId::from_raw(handle.id.as_raw() - 1);
                let opacity = world
                    .query::<(&MobjectId, &Opacity)>()
                    .iter(&world)
                    .find(|(object, _)| object.0 == id)
                    .unwrap()
                    .1
                    .0;
                let expected = if time < 1.8 {
                    0.0
                } else if time > 2.8 {
                    1.0
                } else {
                    0.5
                };
                assert!(
                    (opacity - expected).abs() < 1e-5,
                    "opacity {opacity} at {time}"
                );
            }
        }
    }

    #[test]
    fn object_declared_after_wait_stays_hidden_until_its_declaration_time() {
        let mut canvas = SceneModel::new(640, 360);
        canvas.wait(1.0);
        canvas.circle(20.0);

        let world = World::new();
        let mut queue = CommandQueue::default();
        let mut commands = Commands::new(&mut queue, &world);
        let mut timeline = Timeline::new();
        let fonts = gaanim_text::font::FontRegistry::new();
        let text_config = gaanim_text::prelude::TextConfig::default();
        canvas.compile_into(&mut commands, &mut timeline, &fonts, &text_config);

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
    fn late_group_keeps_already_visible_members_visible() {
        let mut canvas = SceneModel::new(640, 360);
        let first = canvas.circle(10.0);
        let second = canvas.circle(10.0);
        canvas.wait(1.0);
        let group = canvas.group(&[&first, &second]);
        canvas.play(vec![group.animate().opacity(0.3).duration(0.5)]);

        let mut world = World::new();
        let mut queue = CommandQueue::default();
        let mut commands = Commands::new(&mut queue, &world);
        let mut timeline = Timeline::new();
        let fonts = gaanim_text::font::FontRegistry::new();
        let config = gaanim_text::prelude::TextConfig::default();
        canvas.compile_into(&mut commands, &mut timeline, &fonts, &config);
        queue.apply(&mut world);
        timeline.add_keyframe(0.0, WorldSnapshot::capture(&mut world));

        let opacity_of = |world: &mut World, handle: &DrawableHandle| {
            let id = ObjectId::from_raw(handle.id.as_raw() - 1);
            world
                .query::<(&MobjectId, &Opacity)>()
                .iter(world)
                .find(|(object, _)| object.0 == id)
                .unwrap()
                .1
                .0
        };
        for time in [0.5, 2.0, 0.0] {
            timeline.seek(&mut world, time);
            for member in [&first, &second] {
                assert_eq!(opacity_of(&mut world, member), 1.0, "member at {time}");
            }
            let expected = if time < 1.0 { 1.0 } else { 0.3 };
            let group_opacity = opacity_of(&mut world, &group);
            assert!(
                (group_opacity - expected).abs() < 1e-5,
                "group opacity {group_opacity} at {time}"
            );
        }
    }

    #[test]
    fn visual_effects_are_attached_to_compiled_drawables() {
        let mut canvas = SceneModel::new(640, 360);
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
    fn tips_stay_hidden_with_their_path_until_its_entry() {
        use bevy::prelude::App;
        use gaanim_animation::{StrokeTips, TipKind};
        // A trim before a later grow keeps the path hidden until the grow.
        let mut canvas = SceneModel::new(640, 360);
        let route = canvas
            .polyline(&[(0.0, 0.0), (1.0, 1.0), (2.0, 0.0)])
            .tip(Some(TipKind::Dot), Some(TipKind::Arrow), None, None)
            .unwrap();
        canvas.play(vec![route.animate().trim(Some(0.2), None, None)]);
        canvas.wait(0.5);
        canvas.play(vec![route.animate().grow_arrow()]);
        let mut app = App::new();
        app.add_plugins(bevy::prelude::MinimalPlugins)
            .add_plugins(gaanim_scene::GaanimScenePlugin)
            .add_plugins(gaanim_animation::GaanimAnimationPlugin)
            .add_plugins(gaanim_timeline::GaanimTimelinePlugin)
            .add_plugins(gaanim_text::GaanimTextPlugin)
            .add_plugins(gaanim_renderer::GaanimDerivedGeometryPlugin);
        app.finish();
        app.cleanup();
        app.update();
        crate::runtime::replay_canvas_into(app.world_mut(), canvas);
        app.update();
        let entity = entity_of(app.world_mut(), &route);
        for (time, shown) in [(0.5, false), (1.2, false), (2.0, true), (0.3, false)] {
            app.world_mut().resource_mut::<Timeline>().seek_request = Some(time);
            app.update();
            let world = app.world();
            let tips = world.get::<StrokeTips>(entity).unwrap();
            for tip in [tips.start_entity, tips.end_entity] {
                let shape = world.get::<gaanim_scene::Path2D>(tip.unwrap()).unwrap();
                assert_eq!(!shape.0.elements().is_empty(), shown, "tip at {time}");
            }
        }
    }

    #[test]
    fn tipped_paths_spawn_their_tips_as_children() {
        use gaanim_animation::{StrokeTip, StrokeTips, TipKind};
        let mut canvas = SceneModel::new(640, 360);
        let route = canvas
            .polyline(&[(0.0, 0.0), (1.0, 1.0), (2.0, 0.0)])
            .tip(None, Some(TipKind::Arrow), Some(0.3), None)
            .unwrap();
        let plain = canvas.circle(1.0);
        assert!(
            plain
                .clone()
                .tip(None, Some(TipKind::Dot), Some(-1.0), None)
                .is_err()
        );

        let world = World::new();
        let mut queue = CommandQueue::default();
        let mut commands = Commands::new(&mut queue, &world);
        let mut timeline = Timeline::new();
        let fonts = gaanim_text::font::FontRegistry::new();
        let text_config = gaanim_text::prelude::TextConfig::default();
        canvas.compile_into(&mut commands, &mut timeline, &fonts, &text_config);
        let mut world = world;
        queue.apply(&mut world);

        let entity = entity_of(&mut world, &route);
        let tips = world.get::<StrokeTips>(entity).unwrap();
        assert_eq!((tips.start, tips.end), (None, Some(TipKind::Arrow)));
        assert!(tips.start_entity.is_none());
        let tip = tips.end_entity.unwrap();
        assert!(world.get::<StrokeTip>(tip).is_some());
        assert_eq!(world.get::<ChildOf>(tip).unwrap().parent(), entity);
        let plain = entity_of(&mut world, &plain);
        assert!(world.get::<StrokeTips>(plain).is_none());
    }

    #[test]
    fn group_blend_modes_reach_members_that_can_override_them() {
        use gaanim_core::peniko::{BlendMode, Mix};
        let mut canvas = SceneModel::new(640, 360);
        let plain = canvas.circle(1.0);
        let own = canvas.square(1.0);
        let opted = canvas.circle(0.3);
        let group = canvas
            .group(&[&plain, &own, &opted])
            .blend(Some(Mix::Screen.into()));
        // Like fill, a member restyled after grouping keeps its own mode,
        // including an explicit normal one.
        let own = own.blend(Some(Mix::Multiply.into()));
        let opted = opted.blend(Some(BlendMode::default()));
        let lone = canvas.circle(0.5);

        let world = World::new();
        let mut queue = CommandQueue::default();
        let mut commands = Commands::new(&mut queue, &world);
        let mut timeline = Timeline::new();
        let fonts = gaanim_text::font::FontRegistry::new();
        let text_config = gaanim_text::prelude::TextConfig::default();
        canvas.compile_into(&mut commands, &mut timeline, &fonts, &text_config);
        let mut world = world;
        queue.apply(&mut world);

        let mut blend_of = |handle: &DrawableHandle| {
            let entity = entity_of(&mut world, handle);
            world
                .get::<gaanim_renderer::effects::ElementBlend>(entity)
                .map(|blend| blend.0)
        };
        assert_eq!(blend_of(&plain), Some(BlendMode::from(Mix::Screen)));
        assert_eq!(blend_of(&own), Some(BlendMode::from(Mix::Multiply)));
        assert_eq!(blend_of(&opted), Some(BlendMode::default()));
        assert_eq!(blend_of(&group), Some(BlendMode::from(Mix::Screen)));
        assert_eq!(blend_of(&lone), None);
    }

    fn entity_of(world: &mut World, handle: &DrawableHandle) -> Entity {
        let id = ObjectId::from_raw(handle.id.as_raw() - 1);
        world
            .query::<(Entity, &MobjectId)>()
            .iter(world)
            .find(|(_, object)| object.0 == id)
            .unwrap()
            .0
    }

    #[test]
    fn camera_views_bind_their_screen_to_the_framing_drawable() {
        use crate::canvas::{CameraViewError, CameraViewFit, CameraViewOptions};
        use gaanim_renderer::effects::{CameraView, ViewLayer};

        let mut canvas = SceneModel::new(640, 360);
        let frame = canvas.rect(2.0, 1.0).move_to(3.0, 2.0);
        let marker = canvas.dot(0.1).move_to(2.5, 2.0);
        let bone = canvas
            .circle(0.2)
            .move_to(3.2, 1.8)
            .view_layer(Some("xray"))
            .unwrap();
        let view = canvas
            .rounded_rect(4.0, 2.0, 0.2)
            .move_to(-4.0, -1.0)
            .camera_view_with(
                Some(&frame),
                CameraViewOptions {
                    fit: CameraViewFit::Cover,
                    exclude: vec![marker.clone()],
                    layers: vec![" xray ".into()],
                    ..Default::default()
                },
            )
            .unwrap();
        let screen = view.screen().clone();
        assert_eq!(view.frame().id, frame.id);
        assert!(
            view.zoom_source().is_none(),
            "the frame's size sets the zoom"
        );

        let (mut world, _) = compiled_world(&canvas);
        let screen_entity = entity_of(&mut world, &screen);
        let camera = world.get::<CameraView>(screen_entity).unwrap().clone();
        assert_eq!(camera.source, entity_of(&mut world, &frame));
        assert_eq!(camera.exclude, vec![entity_of(&mut world, &marker)]);
        assert_eq!(camera.fit, CameraViewFit::Cover);
        assert_eq!(camera.layers, vec![Arc::<str>::from("xray")]);
        assert!(camera.zoom.is_none());
        let bone_entity = entity_of(&mut world, &bone);
        assert_eq!(
            world.get::<ViewLayer>(bone_entity),
            Some(&ViewLayer(Arc::from("xray")))
        );

        let label = canvas.text("zoom");
        assert_eq!(
            label.camera_view(&frame).unwrap_err(),
            CameraViewError::UnsupportedScreen
        );
        assert_eq!(
            screen.clone().camera_view(&screen).unwrap_err(),
            CameraViewError::OwnSource
        );
        let foreign = SceneModel::new(640, 360).rect(1.0, 1.0);
        assert_eq!(
            screen.clone().camera_view(&foreign).unwrap_err(),
            CameraViewError::ForeignScene
        );
        assert_eq!(
            bone.clone().view_layer(Some("  ")).unwrap_err(),
            CameraViewError::InvalidLayer
        );

        screen.no_camera_view();
        bone.view_layer(None).unwrap();
        let (mut world, _) = compiled_world(&canvas);
        assert_eq!(world.query::<&CameraView>().iter(&world).count(), 0);
        assert_eq!(world.query::<&ViewLayer>().iter(&world).count(), 0);
    }

    #[test]
    fn explicit_camera_view_zoom_animates_at_constant_perceived_speed() {
        use crate::canvas::{CameraViewError, CameraViewOptions, CameraViewZoom};
        use gaanim_renderer::effects::CameraView;

        let mut canvas = SceneModel::new(640, 360);
        let view = canvas
            .rect(4.0, 2.0)
            .camera_view_with(
                None,
                CameraViewOptions {
                    zoom: Some(CameraViewZoom::Value(3.0)),
                    ..Default::default()
                },
            )
            .unwrap();
        canvas.play(vec![view.animate_zoom_to(12.0).unwrap().duration(1.0)]);
        assert_eq!(view.zoom_to(0.0).unwrap_err(), CameraViewError::InvalidZoom);

        let (mut world, mut timeline) = compiled_world(&canvas);
        let screen = entity_of(&mut world, view.screen());
        let zoom_at = |world: &mut World, timeline: &mut Timeline, time: f64| {
            timeline.seek(world, time);
            let camera = world.get::<CameraView>(screen).unwrap().clone();
            camera.zoom.unwrap().evaluate(world).unwrap()
        };
        assert!((zoom_at(&mut world, &mut timeline, 0.0) - 3.0).abs() < 1e-9);
        // Halfway through the eased time the zoom is the geometric mean.
        assert!((zoom_at(&mut world, &mut timeline, 0.5) - 6.0).abs() < 1e-9);
        assert!((zoom_at(&mut world, &mut timeline, 1.0) - 12.0).abs() < 1e-9);
        let camera = world.get::<CameraView>(screen).unwrap().clone();
        assert_eq!(camera.source, entity_of(&mut world, view.frame()));

        let computed = canvas
            .rect(4.0, 2.0)
            .camera_view_with(
                None,
                CameraViewOptions {
                    zoom: Some(CameraViewZoom::Source(2.0.into())),
                    ..Default::default()
                },
            )
            .unwrap();
        assert_eq!(
            computed.zoom_to(3.0).unwrap_err(),
            CameraViewError::ComputedZoom
        );
    }

    #[test]
    fn frame_sized_camera_views_zoom_by_scaling_their_frame() {
        let mut canvas = SceneModel::new(640, 360);
        let frame = canvas.rect(2.0, 1.0).move_to(3.0, 2.0);
        let view = canvas
            .rect(4.0, 2.0)
            .move_to(-4.0, -1.0)
            .camera_view(&frame)
            .unwrap();
        view.zoom_to(4.0).unwrap();
        canvas.play(vec![view.animate_zoom_to(8.0).unwrap().duration(1.0)]);
        let (mut world, mut timeline) = compiled_world(&canvas);
        for (time, scale) in [(0.0, 0.5), (1.0, 0.25)] {
            timeline.seek(&mut world, time);
            let transform = transform_of(&mut world, &frame);
            assert!(
                (transform.scale.x - scale).abs() < 1e-9,
                "{time}: {transform:?}"
            );
            assert!(
                (transform.scale.y - scale).abs() < 1e-9,
                "{time}: {transform:?}"
            );
        }
    }

    #[test]
    fn pop_out_and_pop_in_move_the_screen_through_the_region_its_camera_sees() {
        use crate::canvas::{CameraViewOptions, CameraViewZoom};

        let mut canvas = SceneModel::new(640, 360);
        let frame = canvas.rect(2.0, 1.0).move_to(3.0, 2.0);
        let view = canvas
            .rect(4.0, 2.0)
            .move_to(-4.0, -1.0)
            .camera_view(&frame)
            .unwrap();
        canvas.play(vec![view.pop_out().duration(1.0)]);
        canvas.play(vec![view.pop_in().duration(1.0)]);
        canvas.play(vec![view.pop_out().duration(1.0)]);
        // An explicit zoom of 4 shrinks the screen to a quarter over the frame.
        let lens = canvas
            .circle(1.0)
            .move_to(4.0, -3.0)
            .camera_view_with(
                Some(&frame),
                CameraViewOptions {
                    zoom: Some(CameraViewZoom::Value(4.0)),
                    ..Default::default()
                },
            )
            .unwrap();
        canvas.play(vec![lens.pop_out().duration(1.0)]);
        // A following camera pops back to where its target is now.
        let dot = canvas.dot(0.1).move_to(1.0, 1.0);
        let chase = canvas
            .rect(4.0, 2.0)
            .move_to(0.0, -3.0)
            .camera_view_with(
                None,
                CameraViewOptions {
                    zoom: Some(CameraViewZoom::Value(2.0)),
                    ..Default::default()
                },
            )
            .unwrap();
        chase.follow(crate::canvas::CanvasEndpoint::Entity(dot.id), DVec3::ZERO);
        canvas.play(vec![dot.animate().move_to(5.0, 2.0).duration(1.0)]);
        canvas.play(vec![chase.pop_in().duration(1.0)]);

        let (mut world, mut timeline) = compiled_world(&canvas);
        let placed = |world: &mut World, handle: &DrawableHandle| {
            let transform = transform_of(world, handle);
            (transform.translation.truncate(), transform.scale.x)
        };
        let region = DVec2::new(3.0, 2.0);
        let rest = DVec2::new(-4.0, -1.0);
        for (time, center, scale) in [
            (0.0, region, 0.5),
            (1.0, rest, 1.0),
            (2.0, region, 0.5),
            (3.0, rest, 1.0),
        ] {
            timeline.seek(&mut world, time);
            let (at, size) = placed(&mut world, view.screen());
            assert!(at.distance(center) < 1e-9, "{time}: {at:?}");
            assert!((size - scale).abs() < 1e-9, "{time}: {size}");
        }
        timeline.seek(&mut world, 3.0);
        let (at, size) = placed(&mut world, lens.screen());
        assert!(at.distance(region) < 1e-9 && (size - 0.25).abs() < 1e-9);
        timeline.seek(&mut world, 4.0);
        let (at, size) = placed(&mut world, lens.screen());
        assert!(at.distance(DVec2::new(4.0, -3.0)) < 1e-9 && (size - 1.0).abs() < 1e-9);
        timeline.seek(&mut world, 6.0);
        let (at, size) = placed(&mut world, chase.screen());
        assert!(at.distance(DVec2::new(5.0, 2.0)) < 1e-9, "{at:?}");
        assert!((size - 0.5).abs() < 1e-9, "{size}");
    }

    #[test]
    fn camera_insets_join_their_frame_and_screen_and_hide_the_joins_from_the_view() {
        use crate::canvas::{
            CameraInsetOptions, CameraInsetShape, CameraViewError, CanvasEndpoint,
        };
        use gaanim_renderer::effects::CameraView;

        let mut canvas = SceneModel::new(640, 360);
        let target = canvas.dot(0.1).move_to(-3.0, -1.0);
        let inset = canvas
            .camera_inset(
                CanvasEndpoint::Entity(target.id),
                CameraInsetOptions::default(),
            )
            .unwrap();
        assert_eq!(inset.connectors().len(), 2);
        let lens = canvas
            .camera_inset(
                CanvasEndpoint::Static(DVec3::new(2.0, 1.0, 0.0)),
                CameraInsetOptions {
                    shape: CameraInsetShape::Circle,
                    connectors: false,
                    fixed: true,
                    ..Default::default()
                },
            )
            .unwrap();
        assert!(lens.connectors().is_empty());
        let reactive = CanvasEndpoint::Expression {
            x: 1.0.into(),
            y: 2.0.into(),
        };
        assert_eq!(
            canvas
                .camera_inset(reactive.clone(), CameraInsetOptions::default())
                .unwrap_err(),
            CameraViewError::UnsupportedTarget
        );
        canvas
            .camera_inset(
                reactive,
                CameraInsetOptions {
                    follow: true,
                    ..Default::default()
                },
            )
            .unwrap();

        let (mut world, mut timeline) = compiled_world(&canvas);
        timeline.seek(&mut world, 0.0);
        let screen = entity_of(&mut world, inset.screen());
        let camera = world.get::<CameraView>(screen).unwrap().clone();
        for connector in inset.connectors() {
            let connector = entity_of(&mut world, connector);
            assert!(camera.exclude.contains(&connector));
        }
        assert!((camera.zoom.unwrap().evaluate(&world).unwrap() - 2.0).abs() < 1e-9);
        // Later ids shift past the helpers the first inset compiles, so find
        // the fixed screen by its components.
        let fixed_screens = world
            .query_filtered::<(), (With<CameraView>, With<gaanim_scene::HudOverlay>)>()
            .iter(&world)
            .count();
        let huds = world
            .query_filtered::<Entity, With<gaanim_scene::HudOverlay>>()
            .iter(&world)
            .count();
        let views = world.query::<&CameraView>().iter(&world).count();
        assert_eq!(
            fixed_screens, 1,
            "{huds} HUD entities, {views} camera views"
        );
        // The frame sits on the target, sized like the screen before its zoom.
        let frame = transform_of(&mut world, inset.frame());
        assert!(
            frame
                .translation
                .truncate()
                .distance(DVec2::new(-3.0, -1.0))
                < 1e-9
        );
    }

    #[test]
    fn clip_mask_uses_another_drawables_world_geometry() {
        let mut canvas = SceneModel::new(640, 360);
        let target = canvas.rect(300.0, 160.0).move_to(80.0, 0.0);
        let mask = canvas.circle(55.0).move_to(80.0, 0.0);
        target.clip(&mask, gaanim_core::peniko::Fill::NonZero);

        let world = World::new();
        let mut queue = CommandQueue::default();
        let mut commands = Commands::new(&mut queue, &world);
        let mut timeline = Timeline::new();
        let fonts = gaanim_text::font::FontRegistry::new();
        let text_config = gaanim_text::prelude::TextConfig::default();
        canvas.compile_into(&mut commands, &mut timeline, &fonts, &text_config);

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
        let mut canvas = SceneModel::new(640, 360);
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
    fn terminal_stop_holds_the_completed_segment_at_a_shared_boundary() {
        let red = PenikoColor::from_rgb8(255, 0, 0);
        let blue = PenikoColor::from_rgb8(0, 0, 255);
        let mut canvas = SceneModel::new(640, 360);
        canvas.segment("first", None).unwrap();
        canvas.text("First segment").fill(red);
        canvas.wait(1.0);
        canvas.stop(None).unwrap();
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

        timeline.seek(&mut world, 1.0);
        assert!(visible_for(&mut world, red));
        assert!(!visible_for(&mut world, blue));
        assert_eq!(
            timeline.segment_label().as_deref(),
            Some("1 / 2 · first · stop 1")
        );

        timeline.seek(&mut world, 1.000_001);
        assert!(!visible_for(&mut world, red));
        assert!(visible_for(&mut world, blue));
    }

    #[test]
    fn segment_metadata_uses_the_compiled_clock_exactly() {
        // Per-segment cursors and the builder's running clock sum these waits
        // in different orders: the manifest ends at 9.4, the clips at
        // 9.399999999999999.
        let slides: [&[f64]; 8] = [
            &[0.15],
            &[0.55, 1.05, 0.35],
            &[0.45, 0.55, 0.35],
            &[0.35, 0.25],
            &[1.05, 1.05],
            &[0.95, 0.55],
            &[0.45, 0.35],
            &[0.95],
        ];
        let mut canvas = SceneModel::new(640, 360);
        for (index, waits) in slides.iter().enumerate() {
            canvas.segment(format!("slide {index}"), None).unwrap();
            for &wait in *waits {
                canvas.wait(wait);
                canvas.stop(None).unwrap();
            }
        }
        let manifest_end = canvas
            .segment_manifest()
            .segments
            .last()
            .map(|segment| segment.end_time)
            .unwrap();

        let world = World::new();
        let mut queue = CommandQueue::default();
        let mut commands = Commands::new(&mut queue, &world);
        let mut timeline = Timeline::new();
        let fonts = gaanim_text::font::FontRegistry::new();
        let text_config = gaanim_text::prelude::TextConfig::default();
        canvas.compile_into(&mut commands, &mut timeline, &fonts, &text_config);

        assert!(
            manifest_end > timeline.cached_duration,
            "fixture must reproduce the float drift"
        );
        let stop_clips = timeline
            .clips
            .values()
            .filter(|clip| matches!(clip.payload, gaanim_timeline::clip::ClipPayload::Stop))
            .map(|clip| clip.start.to_bits())
            .collect::<HashSet<_>>();
        for segment in &timeline.segments {
            assert!(segment.end_time <= timeline.cached_duration);
            for stop in &segment.stops {
                assert!(
                    stop_clips.contains(&stop.time.to_bits()),
                    "{} has a stop at {} without a matching clip",
                    segment.name,
                    stop.time
                );
            }
            // A terminal stop keeps its own scene on screen, not the next one.
            let terminal = segment.stops.last().unwrap();
            let scene = timeline.scene_at(terminal.time).unwrap();
            assert_eq!(timeline.scenes[scene].name, segment.name);
        }
    }

    #[test]
    fn justified_paragraph_compiles_to_vector_glyphs() {
        let mut canvas = SceneModel::new(640, 360);
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

        let mut world = world;
        queue.apply(&mut world);
        let mut query = world.query::<&LocalBounds>();
        let visible_bounds = query
            .iter(&world)
            .filter(|bounds| bounds.0.width() > 0.0 && bounds.0.height() > 0.0)
            .count();
        assert!(visible_bounds > 5, "paragraph should produce vector glyphs");
    }

    /// Height of the tallest compiled drawable in `canvas`.
    fn tallest_compiled_height(canvas: &SceneModel) -> f64 {
        let world = World::new();
        let mut queue = CommandQueue::default();
        let mut commands = Commands::new(&mut queue, &world);
        let mut timeline = Timeline::new();
        let fonts = gaanim_text::font::FontRegistry::new();
        let text_config = gaanim_text::prelude::TextConfig::default();
        canvas.compile_into(&mut commands, &mut timeline, &fonts, &text_config);
        let mut world = world;
        queue.apply(&mut world);
        let mut query = world.query::<&LocalBounds>();
        query
            .iter(&world)
            .map(|bounds| bounds.0.height())
            .fold(0.0, f64::max)
    }

    #[test]
    fn typst_documents_default_to_body_text_size() {
        let mut text = SceneModel::new(16.0, 9.0);
        text.text("Result table");
        let mut document = SceneModel::new(16.0, 9.0);
        document.typst("Result table");
        let text_height = tallest_compiled_height(&text);
        let document_height = tallest_compiled_height(&document);
        assert!(text_height > 0.0 && text_height < 1.0, "{text_height}");
        let ratio = document_height / text_height;
        assert!(
            (0.7..1.4).contains(&ratio),
            "a Typst document line ({document_height}) should match body text ({text_height})"
        );

        // Typst's own lengths keep their proportion to the text.
        let mut table = SceneModel::new(16.0, 9.0);
        table.typst("#table(columns: 2, [A], [B], [C], [D])");
        let table_height = tallest_compiled_height(&table);
        assert!(
            table_height > 2.0 * document_height && table_height < 8.0 * document_height,
            "two table rows with 5pt insets: {table_height} vs line {document_height}"
        );

        // Built-in components size generated markup in scene units.
        let mut units = SceneModel::new(16.0, 9.0);
        units.typst_in_scene_units("#set text(size: 0.5pt)\nResult table");
        let units_height = tallest_compiled_height(&units);
        assert!((0.3..0.8).contains(&units_height), "{units_height}");
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
        let mut canvas = SceneModel::new(640, 360);
        canvas.math_text("E = m c^2").color_by("m", highlight);

        let world = World::new();
        let mut queue = CommandQueue::default();
        let mut commands = Commands::new(&mut queue, &world);
        let mut timeline = Timeline::new();
        let fonts = gaanim_text::font::FontRegistry::new();
        let text_config = gaanim_text::prelude::TextConfig::default();
        canvas.compile_into(&mut commands, &mut timeline, &fonts, &text_config);

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
        let mut canvas = SceneModel::new(640, 360);
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
        let mut canvas = SceneModel::new(640, 360);
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
        let mut canvas = SceneModel::new(640, 360);
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
    fn group_z_index_keeps_its_creation_order() {
        // Regression for #25: post_apply used to reset creation_order to 0,
        // dropping the tie-breaker for layered groups and texts.
        let mut canvas = SceneModel::new(16.0, 9.0);
        let first = canvas.rect(1.0, 1.0);
        let second = canvas.circle(0.5);
        canvas.group(&[&first, &second]).z_index(5);

        let mut world = World::new();
        world.insert_resource(Timeline::new());
        world.insert_resource(gaanim_text::font::FontRegistry::new());
        world.insert_resource(gaanim_text::prelude::TextConfig::default());
        canvas.compile(&mut world);
        world.flush();

        let mut query = world.query_filtered::<&RenderOrder, With<gaanim_scene::GroupMarker>>();
        let layered = query
            .iter(&world)
            .find(|order| order.z_index == 5)
            .expect("layered group");
        assert_ne!(layered.creation_order, 0);
    }

    fn compiled_solid_fills(canvas: &SceneModel) -> (World, Vec<PenikoColor>) {
        let mut world = World::new();
        world.insert_resource(Timeline::new());
        world.insert_resource(gaanim_text::font::FontRegistry::new());
        world.insert_resource(gaanim_text::prelude::TextConfig::default());
        canvas.compile(&mut world);
        world.flush();
        let colors = world
            .query::<&gaanim_scene::FillBrush>()
            .iter(&world)
            .filter_map(|fill| match fill.0.as_ref() {
                Some(gaanim_core::peniko::Brush::Solid(color)) => Some(*color),
                _ => None,
            })
            .collect();
        (world, colors)
    }

    #[test]
    fn new_scenes_compile_with_the_default_technical_theme() {
        let technical = crate::canvas::CanvasTheme::builtin("technical").unwrap();
        let mut canvas = SceneModel::new(640, 360);
        assert_eq!(canvas.theme.as_deref(), Some("technical"));
        assert_eq!(canvas.background, Some(technical.palette.background));
        canvas.circle(40.0);
        let explicit = PenikoColor::from_rgb8(0xFF, 0x00, 0x00);
        canvas.circle(20.0).fill(explicit).move_to(100.0, 0.0);
        canvas.text("Body copy");

        let (world, fills) = compiled_solid_fills(&canvas);
        assert!(
            fills.contains(&technical.palette.accent),
            "themed shape fill"
        );
        assert!(fills.contains(&explicit), "explicit fills still win");
        assert!(fills.contains(&technical.palette.foreground), "themed text");
        let rgba = technical.palette.background.to_rgba8();
        assert_eq!(
            world.resource::<ClearColor>().0,
            Color::srgba_u8(rgba.r, rgba.g, rgba.b, rgba.a)
        );
    }

    #[test]
    fn explicit_backgrounds_survive_theme_changes_and_removal() {
        let navy = PenikoColor::from_rgb8(0x0F, 0x17, 0x2A);
        let mut canvas = SceneModel::new(640, 360);
        canvas.set_background(Some(navy));
        canvas.set_theme("paper").unwrap();
        assert_eq!(canvas.background, Some(navy));
        canvas.clear_theme();
        assert_eq!(
            (canvas.theme.as_deref(), canvas.background),
            (None, Some(navy))
        );

        let mut plain = SceneModel::new(640, 360);
        plain.clear_theme();
        assert_eq!(
            (plain.background, plain.background_paint.is_none()),
            (None, true)
        );
        assert!(plain.theme_color("foreground").is_err());
        plain.set_theme("paper").unwrap();
        assert_eq!(plain.background, Some(PenikoColor::WHITE));
    }

    #[test]
    fn unthemed_compile_uses_the_host_text_config_like_runtime_replay() {
        let mut canvas = SceneModel::new(640, 360);
        canvas.clear_theme();
        canvas.text("Body copy");
        let host = gaanim_text::prelude::TextConfig::default();
        let resolved = canvas.scene_text_config(&host);
        for (role, style) in &host.roles {
            assert_eq!(
                resolved.roles[role].fill_color, style.fill_color,
                "{role:?}"
            );
        }
        // `compile` no longer picks black text for the white default
        // background: both paths keep the host (white) foreground, which
        // `unthemed_contrast_warning` reports.
        let (_, fills) = compiled_solid_fills(&canvas);
        assert!(fills.contains(&PenikoColor::WHITE));
        assert!(!fills.contains(&PenikoColor::BLACK));
        assert!(canvas.unthemed_contrast_warning().is_some());
    }

    #[test]
    fn theme_selected_after_authoring_is_materialized_during_compile() {
        let mut canvas = SceneModel::new(640, 360);
        canvas.circle(40.0);
        canvas
            .circle(20.0)
            .fill(PenikoColor::BLACK)
            .move_to(100.0, 0.0);
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
    fn camera_zoom_and_frame_default_to_exponential_with_zoom_hand_off() {
        let mut canvas = SceneModel::new(960, 540);
        let card = canvas.rect(120.0, 80.0).move_to(200.0, -60.0);
        let zoom_in = canvas.camera_zoom_to_source(ScalarSource::constant(8.0), 1.0);
        canvas.play(vec![zoom_in]);
        let frame = canvas.camera_frame_many(std::slice::from_ref(&card), [20.0; 4], false, 1.0);
        canvas.play(vec![frame]);
        let linear = canvas.camera_frame_many_with_interpolation(
            &[card],
            [20.0; 4],
            false,
            gaanim_math::ZoomInterpolation::Linear,
            1.0,
        );
        canvas.play(vec![linear]);

        let mut world = World::new();
        world.insert_resource(Timeline::new());
        world.insert_resource(gaanim_text::font::FontRegistry::new());
        world.insert_resource(gaanim_text::prelude::TextConfig::default());
        canvas.compile(&mut world);
        world.flush();

        use gaanim_timeline::clip::PropertyLensSpec;
        let mut lenses: Vec<_> = world
            .resource::<Timeline>()
            .clips
            .values()
            .filter_map(|clip| match &clip.payload {
                gaanim_timeline::clip::ClipPayload::Animation(animation) => {
                    Some((clip.start, animation.lens.clone()))
                }
                _ => None,
            })
            .collect();
        lenses.sort_by(|left, right| left.0.total_cmp(&right.0));
        assert!(lenses.iter().any(|(_, lens)| matches!(
            lens,
            PropertyLensSpec::CameraZoomSource {
                interpolation: gaanim_math::ZoomInterpolation::Exponential,
                ..
            }
        )));
        let pan_zoom = lenses
            .iter()
            .find_map(|(_, lens)| match lens {
                PropertyLensSpec::CameraPanZoom {
                    from_zoom,
                    interpolation,
                    ..
                } => Some((*from_zoom, *interpolation)),
                _ => None,
            })
            .expect("exponential frame_to emits one coupled pan+zoom clip");
        // The constant zoom_to target is the hand-off pose for the next frame.
        assert_eq!(pan_zoom, (8.0, gaanim_math::ZoomInterpolation::Exponential));
        assert!(lenses.iter().any(|(_, lens)| matches!(
            lens,
            PropertyLensSpec::CameraZoom {
                interpolation: gaanim_math::ZoomInterpolation::Linear,
                ..
            }
        )));
    }

    #[test]
    fn dynamic_camera_frame_keeps_all_compiled_targets_and_bounds() {
        let mut canvas = SceneModel::new(960, 540);
        let left = canvas.circle(85.0).move_to(-260.0, -10.0);
        let right = canvas.rect(180.0, 110.0).move_to(250.0, -10.0);
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
        let mut canvas = SceneModel::new(960, 540);
        let constraint = canvas
            .camera_bind_2d(
                Some(CanvasEndpoint::Static(DVec3::new(120.0, -35.0, 0.0))),
                Some(ScalarSource::constant(1.4)),
                None,
                ScalarSource::constant(1.0),
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
        let mut canvas = SceneModel::new(960, 540);
        let parameter = canvas.parameter(0.0).unwrap();
        let parameter_id = parameter.drawable().id;
        let x = ScalarSource::function(ReactiveFunction::new(
            0,
            1,
            vec![gaanim_animation::ReactiveInput::Signal(parameter_id)],
            |values| Ok(vec![values[0] * 260.0 - 130.0]),
        ))
        .unwrap();
        let point = canvas.point_ref(x, ScalarSource::constant(25.0));
        let _constraint = canvas
            .camera_bind_2d(Some(point.0), None, None, ScalarSource::constant(1.0), true)
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
        let mut canvas = SceneModel::new(640, 360);
        let source = canvas.math_text("E = m c^2");
        let target = canvas.math_text("p = m v");
        let morph = source
            .select("m")
            .morph_to(&target.select("m"), 0.8)
            .expect("selections share a SceneModel");
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
        let mut canvas = SceneModel::new(640, 360);
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
        let mut canvas = SceneModel::new(640, 360);
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
        let mut canvas = SceneModel::new(640, 360);
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
        let mut canvas = SceneModel::new(640, 360);
        let source = canvas
            .math_text("E = m c^2")
            .define_tag("mass", "m", None)
            .move_to(0.0, 70.0);
        let target = canvas
            .math_text("p = m v")
            .define_tag("mass", "m", None)
            .move_to(0.0, -90.0);
        let copy = source
            .tag("mass")
            .unwrap()
            .copy_to(&target.tag("mass").unwrap(), 0.8)
            .expect("selections share a SceneModel");
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
    fn reveal_groups_align_ligatures_and_attach_punctuation() {
        let ids: Vec<ObjectId> = (1..=6).map(ObjectId::from_raw).collect();
        // "¡fire, ok": the "fi" ligature draws one glyph for two characters.
        let glyphs: Vec<(ObjectId, char)> = ids.iter().copied().zip("¡fre,o".chars()).collect();
        let visible = [
            ('¡', None),
            ('f', Some(0)),
            ('i', Some(0)),
            ('r', Some(0)),
            ('e', Some(0)),
            (',', None),
            ('o', Some(1)),
        ];
        assert_eq!(
            reveal_groups(&glyphs, &visible),
            Some(vec![ids[..5].to_vec(), vec![ids[5]]])
        );
        assert_eq!(reveal_groups(&glyphs, &[('x', None)]), None);
    }

    fn compiled_timeline(canvas: &SceneModel) -> Timeline {
        let world = World::new();
        let mut queue = CommandQueue::default();
        let mut commands = Commands::new(&mut queue, &world);
        let mut timeline = Timeline::new();
        let fonts = gaanim_text::font::FontRegistry::new();
        let text_config = gaanim_text::prelude::TextConfig::default();
        canvas.compile_into(&mut commands, &mut timeline, &fonts, &text_config);
        timeline
    }

    fn translation_clips(timeline: &Timeline) -> Vec<(f64, f64, DVec3, DVec3, RateFunc)> {
        let mut clips: Vec<_> = timeline
            .clips
            .values()
            .filter_map(|clip| match &clip.payload {
                gaanim_timeline::clip::ClipPayload::Animation(
                    gaanim_timeline::clip::AnimationSpec {
                        lens: gaanim_timeline::clip::PropertyLensSpec::Translation { from, to },
                        rate_func,
                        ..
                    },
                ) => Some((clip.start, clip.duration, *from, *to, rate_func.clone())),
                _ => None,
            })
            .collect();
        clips.sort_by(|a, b| a.0.total_cmp(&b.0));
        clips
    }

    #[test]
    fn even_yoyo_returns_to_start_and_offset_loops_accumulate() {
        use gaanim_math::RepeatMode;
        let mut canvas = SceneModel::new(640, 360);
        let dot = canvas.circle(0.2);
        canvas.play(vec![dot.animate().shift_by(1.0, 0.0).duration(0.5).repeat(
            2,
            RepeatMode::PingPong,
            0.25,
        )]);
        canvas.play(vec![dot.animate().shift_by(0.0, 1.0).duration(1.0)]);
        let clips = translation_clips(&compiled_timeline(&canvas));
        assert_eq!(clips.len(), 2);
        let (start, duration, from, to, rate) = &clips[0];
        assert_eq!((*start, *duration), (0.0, 1.25));
        assert_eq!(*to, DVec3::new(1.0, 0.0, 0.0));
        assert!(rate.evaluate(1.0).abs() < 1e-12);
        // The next animation starts from where the yoyo came back to.
        assert_eq!(clips[1].0, 1.25);
        assert_eq!(clips[1].2, *from);
        assert_eq!(clips[1].3, *from + DVec3::Y);

        let mut canvas = SceneModel::new(640, 360);
        let dot = canvas.circle(0.2);
        canvas.play(vec![
            dot.animate()
                .shift_by(1.0, 0.0)
                .duration(0.5)
                .loop_for(1.6, RepeatMode::Offset, 0.0),
        ]);
        let clips = translation_clips(&compiled_timeline(&canvas));
        let starts: Vec<f64> = clips.iter().map(|clip| clip.0).collect();
        assert_eq!(starts, [0.0, 0.5, 1.0]);
        assert_eq!(clips[2].3, clips[0].2 + DVec3::new(3.0, 0.0, 0.0));
    }

    #[test]
    fn oriented_paths_and_arcs_compile_to_path_follow() {
        let mut canvas = SceneModel::new(640, 360);
        let route = canvas.polyline(&[(0.0, 0.0), (4.0, 0.0), (4.0, 3.0)]);
        let plane = canvas.circle(0.2);
        canvas.play(vec![
            plane
                .animate()
                .move_along_with(
                    &route,
                    crate::anim::PathFollowOptions {
                        orient: Some(0.5),
                        start: 0.0,
                        end: 0.5,
                    },
                )
                .unwrap(),
        ]);
        let ball = canvas.circle(0.2);
        canvas.play(vec![ball.animate().shift_by(2.0, 0.0).path_arc(1.0)]);
        let timeline = compiled_timeline(&canvas);
        let follows: Vec<_> = timeline
            .clips
            .values()
            .filter_map(|clip| match &clip.payload {
                gaanim_timeline::clip::ClipPayload::Animation(
                    gaanim_timeline::clip::AnimationSpec {
                        lens:
                            gaanim_timeline::clip::PropertyLensSpec::PathFollow {
                                path,
                                orient,
                                reset_anchor,
                            },
                        ..
                    },
                ) => Some((clip.start, path.clone(), *orient, *reset_anchor)),
                _ => None,
            })
            .collect();
        assert_eq!(follows.len(), 2);
        let (_, route_part, orient, reset_anchor) =
            follows.iter().find(|entry| entry.0 == 0.0).unwrap();
        assert_eq!(*orient, Some(0.5));
        assert!(
            *reset_anchor,
            "move_along places the local origin on the route"
        );
        // Half of the 7-unit route ends 3.5 units along it.
        let end = gaanim_math::get_point_at_alpha(route_part, 1.0);
        assert!((end.x - 3.5).abs() < 1e-3 && end.y.abs() < 1e-3, "{end:?}");
        let (_, arc, orient, reset_anchor) = follows.iter().find(|entry| entry.0 > 0.0).unwrap();
        assert_eq!(*orient, None);
        assert!(!*reset_anchor, "path_arc keeps the authored pivot");
        let middle = gaanim_math::get_point_at_alpha(arc, 0.5);
        assert!(
            middle.y < -0.2,
            "arc should bow below the chord: {middle:?}"
        );
    }

    #[test]
    fn move_along_travels_backwards_when_start_exceeds_end() {
        let mut canvas = SceneModel::new(640, 360);
        let route = canvas.polyline(&[(0.0, 0.0), (4.0, 0.0), (4.0, 3.0)]);
        let dot = canvas.circle(0.2);
        canvas.play(vec![
            dot.animate()
                .move_along_with(
                    &route,
                    crate::anim::PathFollowOptions {
                        orient: None,
                        start: 5.5 / 7.0,
                        end: 2.0 / 7.0,
                    },
                )
                .unwrap(),
        ]);
        let timeline = compiled_timeline(&canvas);
        let path = timeline
            .clips
            .values()
            .find_map(|clip| match &clip.payload {
                gaanim_timeline::clip::ClipPayload::Animation(
                    gaanim_timeline::clip::AnimationSpec {
                        lens: gaanim_timeline::clip::PropertyLensSpec::PathFollow { path, .. },
                        ..
                    },
                ) => Some(path.clone()),
                _ => None,
            })
            .unwrap();
        // 5.5 units along the route is (4, 1.5); 2 units is (2, 0).
        let first = gaanim_math::get_point_at_alpha(&path, 0.0);
        let last = gaanim_math::get_point_at_alpha(&path, 1.0);
        assert!(
            (first.x - 4.0).abs() < 1e-3 && (first.y - 1.5).abs() < 1e-3,
            "{first:?}"
        );
        assert!(
            (last.x - 2.0).abs() < 1e-3 && last.y.abs() < 1e-3,
            "{last:?}"
        );
    }

    #[test]
    fn move_along_keeps_the_declared_state_until_it_starts() {
        let mut canvas = SceneModel::new(640, 360);
        let route = canvas.polyline(&[(-6.0, -2.0), (0.0, 2.0), (6.0, -2.0)]);
        let ball = canvas.circle(0.2).move_to(-6.0, -2.0);
        canvas.wait(1.0);
        canvas.play(vec![ball.animate().grow_from_center().duration(0.3)]);
        canvas.play(vec![
            ball.animate().move_along(&route).unwrap().duration(1.0),
        ]);

        let mut world = World::new();
        let mut queue = CommandQueue::default();
        let mut commands = Commands::new(&mut queue, &world);
        let mut timeline = Timeline::new();
        let fonts = gaanim_text::font::FontRegistry::new();
        let config = gaanim_text::prelude::TextConfig::default();
        canvas.compile_into(&mut commands, &mut timeline, &fonts, &config);
        queue.apply(&mut world);
        timeline.add_keyframe(0.0, WorldSnapshot::capture(&mut world));

        let id = ObjectId::from_raw(ball.id.as_raw() - 1);
        for (time, translation, scale) in [
            (0.5, DVec3::new(-6.0, -2.0, 0.0), 0.0),
            (3.0, DVec3::new(6.0, -2.0, 0.0), 1.0),
            (0.5, DVec3::new(-6.0, -2.0, 0.0), 0.0),
        ] {
            timeline.seek(&mut world, time);
            let transform = *world
                .query::<(&MobjectId, &SpatialTransform)>()
                .iter(&world)
                .find(|(object, _)| object.0 == id)
                .unwrap()
                .1;
            assert!(
                transform.translation.distance(translation) < 1e-6,
                "{:?} at {time}",
                transform.translation
            );
            assert!(
                (transform.scale.x - scale).abs() < 1e-9,
                "{:?} at {time}",
                transform.scale
            );
        }
    }

    /// Compile `canvas` and return its timeline with a t = 0 keyframe.
    fn compiled_world(canvas: &SceneModel) -> (World, Timeline) {
        let mut world = World::new();
        let mut queue = CommandQueue::default();
        let mut commands = Commands::new(&mut queue, &world);
        let mut timeline = Timeline::new();
        let fonts = gaanim_text::font::FontRegistry::new();
        let config = gaanim_text::prelude::TextConfig::default();
        canvas.compile_into(&mut commands, &mut timeline, &fonts, &config);
        queue.apply(&mut world);
        timeline.add_keyframe(0.0, WorldSnapshot::capture(&mut world));
        (world, timeline)
    }

    fn transform_of(world: &mut World, handle: &DrawableHandle) -> SpatialTransform {
        let id = ObjectId::from_raw(handle.id.as_raw() - 1);
        *world
            .query::<(&MobjectId, &SpatialTransform)>()
            .iter(world)
            .find(|(object, _)| object.0 == id)
            .unwrap()
            .1
    }

    #[test]
    fn squash_stretch_deforms_along_velocity_only_while_moving() {
        let mut canvas = SceneModel::new(640, 360);
        let ball = canvas
            .circle(0.5)
            .move_to(-3.0, 0.0)
            .squash_stretch(0.1, 1.5)
            .unwrap();
        assert!(canvas.circle(0.5).squash_stretch(-1.0, 1.5).is_err());
        assert!(canvas.circle(0.5).squash_stretch(0.1, 0.5).is_err());
        canvas.play(vec![
            ball.animate()
                .move_to(3.0, 0.0)
                .duration(1.0)
                .rate_func(gaanim_math::RateFunc::Linear),
        ]);
        canvas.wait(1.0);
        let (mut world, mut timeline) = compiled_world(&canvas);
        let id = ObjectId::from_raw(ball.id.as_raw() - 1);
        let mut deform_at = |time: f64| {
            timeline.seek(&mut world, time);
            world
                .query::<(&MobjectId, Option<&gaanim_scene::ShapeDeform>)>()
                .iter(&world)
                .find(|(object, _)| object.0 == id)
                .unwrap()
                .1
                .map_or(kurbo::Affine::IDENTITY, |deform| deform.0)
        };
        // 6 units per second, so the stretch is capped at 1.5 along x.
        let moving = deform_at(0.5).as_coeffs();
        assert!((moving[0] - 1.5).abs() < 1e-9, "{moving:?}");
        assert!((moving[3] - 1.0 / 1.5).abs() < 1e-9, "{moving:?}");
        assert_eq!(deform_at(1.8), kurbo::Affine::IDENTITY);
        assert!((deform_at(0.5).as_coeffs()[0] - 1.5).abs() < 1e-9);
    }

    #[test]
    fn repeater_count_shows_copies_in_order_and_seeks_back() {
        let mut canvas = SceneModel::new(640, 360);
        let dot = canvas.circle(0.1);
        let step = crate::canvas::RepeatStep {
            offset: DVec2::new(0.5, 0.0),
            ..crate::canvas::RepeatStep::default()
        };
        let row = canvas.repeat(&dot, 4, step).unwrap().count(1.0).unwrap();
        assert!(row.clone().count(4.5).is_err());
        assert!(canvas.circle(0.1).count(1.0).is_err());
        assert!(canvas.circle(0.1).animate().count(1.0).is_err());
        canvas.play(vec![
            row.animate()
                .count(4.0)
                .unwrap()
                .duration(1.0)
                .rate_func(gaanim_math::RateFunc::Linear),
        ]);
        canvas.wait(0.5);
        row.clone().count(2.0).unwrap();
        canvas.wait(0.5);
        let (mut world, mut timeline) = compiled_world(&canvas);
        let mut presences = |time: f64| {
            timeline.seek(&mut world, time);
            // Copies are declared in order, so their ids are increasing.
            let mut copies: Vec<(u64, f32)> = world
                .query::<(&gaanim_scene::Presence, &MobjectId)>()
                .iter(&world)
                .map(|(presence, object)| (object.0.as_raw(), presence.0))
                .collect();
            copies.sort_by_key(|copy| copy.0);
            copies
                .into_iter()
                .map(|(_, presence)| presence)
                .collect::<Vec<_>>()
        };
        assert_eq!(presences(0.5), [1.0, 1.0, 0.5, 0.0]);
        assert_eq!(presences(1.2), [1.0; 4]);
        assert_eq!(presences(1.8), [1.0, 1.0, 0.0, 0.0]);
        assert_eq!(presences(0.0), [1.0, 0.0, 0.0, 0.0]);
    }

    #[test]
    fn connect_validates_its_points() {
        let mut canvas = SceneModel::new(640, 360);
        let a = canvas.circle(0.1);
        let b = canvas.circle(0.1).move_to(1.0, 0.0);
        let mode = gaanim_renderer::effects::ConnectMode::Range;
        assert!(canvas.connect(&[&a], 1.0, mode, 1, true).is_err());
        assert!(canvas.connect(&[&a, &b], 0.0, mode, 1, true).is_err());
        assert!(canvas.connect(&[&a, &b], 1.0, mode, 0, true).is_err());
        let group = canvas.group(&[&a, &b]);
        let links = canvas.connect(&[&group], 2.0, mode, 1, true).unwrap();
        assert!(matches!(
            &links.spec.lock().unwrap().kind,
            SpawnKind::Connect { sources, fade: true, .. } if sources.len() == 2
        ));
        let (world, _) = compiled_world(&canvas);
        let mut world = world;
        let layers = world
            .query::<&gaanim_renderer::effects::ConnectBinding>()
            .iter(&world)
            .count();
        assert_eq!(layers, CONNECT_FADE_LAYERS);
    }

    #[test]
    fn points_move_each_vertex_straight_and_seek_exactly() {
        let mut canvas = SceneModel::new(640, 360);
        let square = canvas.polygon(vec![(0.0, 0.0), (2.0, 0.0), (2.0, 2.0), (0.0, 2.0)]);
        let target = vec![(1.0, -1.0), (3.0, 1.0), (1.0, 3.0), (-1.0, 1.0)];
        assert!(square.animate().points(vec![(0.0, 0.0)]).is_err());
        assert!(square.animate().points(vec![(f64::NAN, 0.0); 4]).is_err());
        assert!(canvas.circle(1.0).animate().points(target.clone()).is_err());
        canvas.play(vec![
            square
                .animate()
                .points(target)
                .unwrap()
                .duration(1.0)
                .rate_func(gaanim_math::RateFunc::Linear),
        ]);
        canvas.wait(0.5);
        let (mut world, mut timeline) = compiled_world(&canvas);
        let id = ObjectId::from_raw(square.id.as_raw() - 1);
        let mut vertices_at = |time: f64| {
            timeline.seek(&mut world, time);
            let path = world
                .query::<(&MobjectId, &gaanim_scene::Path2D)>()
                .iter(&world)
                .find(|(object, _)| object.0 == id)
                .unwrap()
                .1
                .0
                .clone();
            path.elements()
                .iter()
                .filter_map(|element| match element {
                    gaanim_core::kurbo::PathEl::MoveTo(point)
                    | gaanim_core::kurbo::PathEl::LineTo(point) => Some((point.x, point.y)),
                    _ => None,
                })
                .collect::<Vec<_>>()
        };
        assert_eq!(
            vertices_at(1.2),
            [(1.0, -1.0), (3.0, 1.0), (1.0, 3.0), (-1.0, 1.0)]
        );
        assert_eq!(
            vertices_at(0.5),
            [(0.5, -0.5), (2.5, 0.5), (1.5, 2.5), (-0.5, 1.5)]
        );
        assert_eq!(
            vertices_at(0.0),
            [(0.0, 0.0), (2.0, 0.0), (2.0, 2.0), (0.0, 2.0)]
        );
    }

    #[test]
    fn launched_animations_run_under_later_plays() {
        let mut canvas = SceneModel::new(640, 360);
        let wheel = canvas.circle(1.0);
        let label = canvas.circle(0.5).move_to(3.0, 0.0);
        canvas
            .launch_composition_configured(
                super::super::canvas_impl::Composition::leaf(
                    wheel
                        .animate()
                        .rotate_by(4.0)
                        .duration(4.0)
                        .rate_func(gaanim_math::RateFunc::Linear),
                ),
                None,
                None,
            )
            .unwrap();
        canvas.play(vec![label.animate().move_to(3.0, 2.0).duration(1.0)]);
        canvas.wait(3.0);
        assert!((canvas.current_time() - 4.0).abs() < 1e-9);
        let (mut world, mut timeline) = compiled_world(&canvas);
        let angle = |world: &mut World, timeline: &mut Timeline, time: f64| {
            timeline.seek(world, time);
            transform_of(world, &wheel)
                .rotation
                .to_euler(gaanim_core::glam::EulerRot::XYZ)
                .2
        };
        assert!((angle(&mut world, &mut timeline, 2.0) - 2.0).abs() < 1e-6);
        assert!((angle(&mut world, &mut timeline, 0.5) - 0.5).abs() < 1e-6);
        let label_y = |world: &mut World, timeline: &mut Timeline, time: f64| {
            timeline.seek(world, time);
            transform_of(world, &label).translation.y
        };
        // The play after the launch starts at the same instant.
        assert!((label_y(&mut world, &mut timeline, 1.0) - 2.0).abs() < 1e-6);
    }

    #[test]
    fn points_setter_declares_the_shape_then_cuts_at_the_cursor() {
        let mut canvas = SceneModel::new(640, 360);
        let shape = canvas.polygon(vec![(0.0, 0.0), (2.0, 0.0), (2.0, 2.0), (0.0, 2.0)]);
        assert!(shape.clone().points(vec![(0.0, 0.0)]).is_err());
        assert!(canvas.circle(1.0).points(vec![(0.0, 0.0)]).is_err());
        let declared = vec![(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0)];
        let shape = shape.points(declared.clone()).unwrap();
        canvas.play(vec![shape.animate().shift_by(0.0, 0.0).duration(1.0)]);
        let cut = vec![(0.0, 0.0), (3.0, 0.0), (3.0, 3.0), (0.0, 3.0)];
        let _ = shape.clone().points(cut.clone()).unwrap();
        canvas.wait(1.0);
        let (mut world, mut timeline) = compiled_world(&canvas);
        let id = ObjectId::from_raw(shape.id.as_raw() - 1);
        let mut vertices_at = |time: f64| {
            timeline.seek(&mut world, time);
            let path = world
                .query::<(&MobjectId, &gaanim_scene::Path2D)>()
                .iter(&world)
                .find(|(object, _)| object.0 == id)
                .unwrap()
                .1
                .0
                .clone();
            path.elements()
                .iter()
                .filter_map(|element| match element {
                    gaanim_core::kurbo::PathEl::MoveTo(point)
                    | gaanim_core::kurbo::PathEl::LineTo(point) => Some((point.x, point.y)),
                    _ => None,
                })
                .collect::<Vec<_>>()
        };
        assert_eq!(vertices_at(0.5), declared);
        assert_eq!(vertices_at(1.5), cut);
        assert_eq!(vertices_at(0.2), declared);
    }

    #[test]
    fn held_echo_copies_freeze_when_the_drawable_stops() {
        let mut canvas = SceneModel::new(640, 360);
        let ball = canvas.circle(0.5).move_to(-3.0, 0.0).echo(Some(
            super::super::types::EchoSpec::new(2, 0.25, 0.5)
                .unwrap()
                .with_hold(true),
        ));
        canvas.play(vec![
            ball.animate()
                .move_to(3.0, 0.0)
                .duration(1.0)
                .rate_func(gaanim_math::RateFunc::Linear),
        ]);
        canvas.wait(1.0);
        let (mut world, mut timeline) = compiled_world(&canvas);
        let mut copies: Vec<(Entity, u32)> = world
            .query::<(Entity, &gaanim_animation::EchoGhost)>()
            .iter(&world)
            .map(|(entity, echo)| (entity, echo.rank))
            .collect();
        copies.sort_by_key(|(_, rank)| *rank);
        let copy_x = |world: &mut World, timeline: &mut Timeline, time: f64| {
            timeline.seek(world, time);
            copies
                .iter()
                .map(|(entity, _)| {
                    world
                        .get::<SpatialTransform>(*entity)
                        .unwrap()
                        .translation
                        .x
                })
                .collect::<Vec<_>>()
        };
        // Moving: like a plain echo, 0.25 s and 0.5 s behind (6 units per s).
        let moving = copy_x(&mut world, &mut timeline, 0.75);
        assert!(
            (moving[0] - 0.0).abs() < 1e-9 && (moving[1] + 1.5).abs() < 1e-9,
            "{moving:?}"
        );
        // Stopped at 1.0 s: the copies stay where they were instead of
        // catching up with the ball at x = 3.
        for time in [1.5, 1.99] {
            let held = copy_x(&mut world, &mut timeline, time);
            assert!(
                (held[0] - 1.5).abs() < 1e-9 && (held[1] - 0.0).abs() < 1e-9,
                "{held:?}"
            );
        }
    }

    #[test]
    fn echo_copies_trail_their_drawable_through_seeks() {
        let mut canvas = SceneModel::new(640, 360);
        let ball = canvas.circle(0.5).move_to(-3.0, 0.0).echo(Some(
            super::super::types::EchoSpec::new(3, 0.1, 0.5).unwrap(),
        ));
        canvas.play(vec![ball.animate().move_to(3.0, 0.0).duration(1.0)]);
        canvas.wait(0.5);
        let (mut world, mut timeline) = compiled_world(&canvas);

        let mut copies: Vec<(Entity, gaanim_animation::EchoGhost)> = world
            .query::<(Entity, &gaanim_animation::EchoGhost)>()
            .iter(&world)
            .map(|(entity, echo)| (entity, echo.clone()))
            .collect();
        copies.sort_by_key(|(_, echo)| echo.rank);
        let ranks: Vec<_> = copies.iter().map(|(_, echo)| echo.rank).collect();
        assert_eq!(ranks, [1, 2, 3]);
        for ((_, echo), (lag, opacity)) in
            copies.iter().zip([(0.1, 0.5), (0.2, 0.25), (0.3, 0.125)])
        {
            assert!((echo.lag - lag).abs() < 1e-12);
            assert!((echo.opacity - opacity).abs() < 1e-6);
        }

        for time in [0.6, 0.25, 1.4, 0.05] {
            let expected: Vec<Option<f64>> = copies
                .iter()
                .map(|(_, echo)| {
                    let earlier = time - echo.lag;
                    (earlier >= 0.0).then(|| {
                        timeline.seek(&mut world, earlier);
                        transform_of(&mut world, &ball).translation.x
                    })
                })
                .collect();
            timeline.seek(&mut world, time);
            for ((copy, echo), expected) in copies.iter().zip(expected) {
                let visible = world.get::<gaanim_scene::Visible>(*copy).is_some();
                match expected {
                    None => assert!(!visible, "copy {} at {time}", echo.rank),
                    Some(x) => {
                        assert!(visible, "copy {} at {time}", echo.rank);
                        let actual = world.get::<SpatialTransform>(*copy).unwrap().translation.x;
                        assert!(
                            (actual - x).abs() < 1e-9,
                            "copy {} at {time}: {actual} vs {x}",
                            echo.rank
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn skew_shears_about_the_pivot_and_seeks_exactly() {
        let mut canvas = SceneModel::new(640, 360);
        let wall = canvas
            .rect(2.0, 2.0)
            .move_to(0.0, 0.0)
            .with_pivot(0.0, -1.0);
        canvas.play(vec![wall.animate().skew_to(0.5, 0.0).duration(1.0)]);
        canvas.wait(1.0);
        let wall = wall.skew_to(0.0, 0.0);
        canvas.wait(1.0);
        let (mut world, mut timeline) = compiled_world(&canvas);

        // The base stays on the pivot; the top leans by skew × height (2).
        for (time, top_x) in [(0.5, 0.5), (1.5, 1.0), (2.5, 0.0), (0.5, 0.5)] {
            timeline.seek(&mut world, time);
            let affine = transform_of(&mut world, &wall).to_affine_2d();
            let base = affine * Point::new(0.0, -1.0);
            let top = affine * Point::new(0.0, 1.0);
            assert!(
                base.distance(Point::new(0.0, -1.0)) < 1e-6,
                "{base:?} at {time}"
            );
            assert!(
                top.distance(Point::new(top_x, 1.0)) < 1e-6,
                "{top:?} at {time}"
            );
        }
    }

    #[test]
    fn anchored_placement_lands_on_the_skewed_shape() {
        let mut canvas = SceneModel::new(640, 360);
        canvas
            .rect(3.0, 2.0)
            .skew_to(0.5, 0.0)
            .at_anchor(-4.0, 2.0, Anchor::TopLeft);
        canvas
            .text("gyp")
            .with_pivot(0.0, 0.0)
            .skew_to(0.3, 0.1)
            .at_text_anchor(5.0, -1.0, TextAnchor::BaselineLeft);

        let mut world = compile_canvas_for_layout(canvas);
        let (text_bounds, baseline, text_transform) = only_text_root(&mut world);
        let rect = world
            .query_filtered::<(&LocalBounds, &SpatialTransform), Without<TextBaseline>>()
            .iter(&world)
            .find(|(bounds, _)| (bounds.0.width() - 3.0).abs() < 1e-9)
            .map(|(bounds, transform)| (bounds.0, *transform))
            .unwrap();
        assert_point_close(
            rect.1
                .to_mat4()
                .transform_point3(Anchor::TopLeft.get_point(&rect.0)),
            DVec3::new(-4.0, 2.0, 0.0),
        );
        let text_anchor = DVec3::new(text_bounds.min.x, baseline.0, text_bounds.center().z);
        assert_point_close(
            text_transform.to_mat4().transform_point3(text_anchor),
            DVec3::new(5.0, -1.0, 0.0),
        );
    }

    #[test]
    fn pivot_rotation_keeps_the_declared_pose_until_it_starts() {
        let mut canvas = SceneModel::new(640, 360);
        let bar = canvas.rect(3.0, 0.3).move_to(1.5, 0.0).with_pivot(0.0, 0.0);
        canvas.wait(1.0);
        canvas.play(vec![
            bar.animate()
                .rotate_by(std::f64::consts::FRAC_PI_2)
                .duration(1.0),
        ]);
        let (mut world, mut timeline) = compiled_world(&canvas);

        for (time, origin, tip) in [
            (0.5, Point::new(1.5, 0.0), Point::new(2.5, 0.0)),
            (3.0, Point::new(0.0, 1.5), Point::new(0.0, 2.5)),
            (0.5, Point::new(1.5, 0.0), Point::new(2.5, 0.0)),
        ] {
            timeline.seek(&mut world, time);
            let affine = transform_of(&mut world, &bar).to_affine_2d();
            let center = affine * Point::ORIGIN;
            let along = affine * Point::new(1.0, 0.0);
            assert!(center.distance(origin) < 1e-6, "{center:?} at {time}");
            assert!(along.distance(tip) < 1e-6, "{along:?} at {time}");
        }
    }

    #[test]
    fn about_point_turns_an_animate_rotation_around_that_point() {
        let mut canvas = SceneModel::new(640, 360);
        let bar = canvas.rect(3.0, 0.3).move_to(1.5, 0.0);
        let other = canvas.rect(3.0, 0.3).move_to(1.5, 0.0);
        canvas.play(vec![
            bar.animate()
                .rotate_by(std::f64::consts::FRAC_PI_2)
                .about_point(0.0, 0.0)
                .duration(1.0),
            other
                .animate()
                .rotate_by(std::f64::consts::FRAC_PI_2)
                .pivot(0.0, 0.0)
                .duration(1.0),
        ]);
        let (mut world, mut timeline) = compiled_world(&canvas);

        timeline.seek(&mut world, 1.0);
        for handle in [&bar, &other] {
            let affine = transform_of(&mut world, handle).to_affine_2d();
            let center = affine * Point::ORIGIN;
            let along = affine * Point::new(1.0, 0.0);
            assert!(center.distance(Point::new(0.0, 1.5)) < 1e-6, "{center:?}");
            assert!(along.distance(Point::new(0.0, 2.5)) < 1e-6, "{along:?}");
        }
    }

    #[test]
    fn move_along_an_arrow_travels_its_axis_from_tail_to_tip() {
        let mut canvas = SceneModel::new(640, 360);
        let straight = canvas.arrow(-2.0, 1.0, 2.0, 1.0);
        let curved = canvas.curved_arrow_arc(0.0, -1.0, 1.5, 0.0, std::f64::consts::PI);
        let dot = canvas.dot(0.05).move_to(-2.0, 1.0);
        let other = canvas.dot(0.05).move_to(1.5, -1.0);
        canvas.play(vec![
            dot.animate()
                .move_along(&straight)
                .unwrap()
                .duration(1.0)
                .rate_func(RateFunc::Linear),
            other
                .animate()
                .move_along(&curved)
                .unwrap()
                .duration(1.0)
                .rate_func(RateFunc::Linear),
        ]);
        let (mut world, mut timeline) = compiled_world(&canvas);

        for (time, on_line, on_arc) in [
            (0.5, Point::new(0.0, 1.0), Point::new(0.0, 0.5)),
            (1.0, Point::new(2.0, 1.0), Point::new(-1.5, -1.0)),
        ] {
            timeline.seek(&mut world, time);
            let at = |world: &mut World, handle: &DrawableHandle| {
                transform_of(world, handle).to_affine_2d() * Point::ORIGIN
            };
            let line = at(&mut world, &dot);
            let arc = at(&mut world, &other);
            assert!(line.distance(on_line) < 1e-3, "{line:?} at {time}");
            assert!(arc.distance(on_arc) < 1e-3, "{arc:?} at {time}");
        }
    }

    #[test]
    fn full_turns_about_an_off_center_pivot_keep_the_pivot_fixed() {
        // The pivot is off the drawable's origin, so the turn also swings its
        // translation; both must follow the same eased angle.
        let (cx, cy) = (4.3, -2.2);
        let mut canvas = SceneModel::new(640, 360);
        let arm = canvas
            .rect(0.6, 0.1)
            .move_to(cx + 0.5, cy)
            .with_pivot(cx, cy);
        canvas.play(vec![
            arm.animate()
                .rotate_by(std::f64::consts::TAU)
                .duration(1.0)
                .rate_func(RateFunc::Smooth),
        ]);
        let (mut world, mut timeline) = compiled_world(&canvas);

        timeline.seek(&mut world, 0.0);
        let local_pivot =
            transform_of(&mut world, &arm).to_affine_2d().inverse() * Point::new(cx, cy);
        for time in [0.1, 0.25, 0.4, 0.5, 0.75, 0.9, 1.0] {
            timeline.seek(&mut world, time);
            let affine = transform_of(&mut world, &arm).to_affine_2d();
            let pivot = affine * local_pivot;
            assert!(
                pivot.distance(Point::new(cx, cy)) < 1e-3,
                "pivot drifted to {pivot:?} at {time}"
            );
            let direction = affine * Point::new(1.0, 0.0) - affine * Point::ORIGIN;
            let expected = std::f64::consts::TAU * RateFunc::Smooth.evaluate(time);
            let turned = direction
                .y
                .atan2(direction.x)
                .rem_euclid(std::f64::consts::TAU);
            let error = (turned - expected.rem_euclid(std::f64::consts::TAU)).abs();
            assert!(
                error.min(std::f64::consts::TAU - error) < 1e-6,
                "turned {turned} instead of {expected} at {time}"
            );
        }
    }

    #[test]
    fn scale_entries_keep_the_declared_position_until_they_start() {
        let mut canvas = SceneModel::new(640, 360);
        let rows = [2.5, 0.8, -0.8, -2.5];
        let squares: Vec<_> = rows
            .iter()
            .map(|y| canvas.square(0.6).move_to(-5.0, *y))
            .collect();
        canvas.play(
            squares
                .iter()
                .map(|square| square.animate().scale_by(1.5).duration(1.0))
                .collect(),
        );
        canvas.play(
            squares
                .iter()
                .zip(rows)
                .map(|(square, y)| square.animate().move_to(4.0, y).duration(1.0))
                .collect(),
        );
        canvas.play(vec![
            squares[0].animate().grow_from_center().duration(1.0),
            squares[1].animate().spin_in_from_nothing().duration(1.0),
            squares[2].animate().grow_from_point(0.0, 0.0).duration(1.0),
            squares[3]
                .animate()
                .grow_from_edge(gaanim_layout::Direction::Left)
                .duration(1.0),
        ]);
        let (mut world, mut timeline) = compiled_world(&canvas);

        // Mid-way through the first scale, before any move or entry.
        for (time, x, scale) in [(0.5, -5.0, None), (3.5, 4.0, Some(1.5)), (0.5, -5.0, None)] {
            timeline.seek(&mut world, time);
            for (square, y) in squares.iter().zip(rows) {
                let transform = transform_of(&mut world, square);
                assert!(
                    transform.translation.distance(DVec3::new(x, y, 0.0)) < 1e-6,
                    "{:?} at {time}",
                    transform.translation
                );
                if let Some(scale) = scale {
                    assert!(
                        (transform.scale.x - scale).abs() < 1e-6,
                        "{:?}",
                        transform.scale
                    );
                } else {
                    assert!(transform.scale.x > 1.0, "visible while scaling at {time}");
                }
            }
        }
    }

    #[test]
    fn text_declared_transparent_fades_back_in_with_its_root_opacity() {
        let mut canvas = SceneModel::new(16.0, 9.0);
        let text = canvas.text("(4.6) texto").opacity(0.0);
        canvas.play(vec![text.animate().opacity(1.0).duration(1.0)]);
        let (mut world, mut timeline) = compiled_world(&canvas);
        let root = ObjectId::from_raw(text.id.as_raw() - 1);
        let root_entity = world
            .query::<(Entity, &MobjectId)>()
            .iter(&world)
            .find(|(_, object)| object.0 == root)
            .unwrap()
            .0;
        for (time, expected) in [(0.0, 0.0), (1.0, 1.0)] {
            timeline.seek(&mut world, time);
            assert!((opacity_of(&mut world, &text) - expected).abs() < 1e-5);
            let spans: Vec<f32> = world
                .query::<(&Opacity, &ChildOf)>()
                .iter(&world)
                .filter(|(_, parent)| parent.parent() == root_entity)
                .map(|(opacity, _)| opacity.0)
                .collect();
            assert!(!spans.is_empty());
            assert!(
                spans.iter().all(|&opacity| opacity == 1.0),
                "{spans:?} at {time}"
            );
        }
    }

    #[test]
    fn z_index_set_after_a_reactive_binding_orders_the_drawable() {
        let mut canvas = SceneModel::new(16.0, 9.0);
        let level = canvas.parameter(0.2).expect("parameter");
        let mask = canvas.rect(2.0, 2.0).opacity(0.0);
        let water = canvas
            .fill_level(
                &mask,
                Brush::Solid(PenikoColor::WHITE),
                0.2,
                crate::canvas::FillLevelDirection::Up,
                false,
            )
            .expect("fill level")
            .set_fill_level(&level)
            .expect("reactive level")
            .z_index(4);
        canvas.wait(0.5);
        let (mut world, _) = compiled_world(&canvas);
        let id = ObjectId::from_raw(water.id.as_raw() - 1);
        let order = world
            .query::<(&MobjectId, &RenderOrder)>()
            .iter(&world)
            .find(|(object, _)| object.0 == id)
            .map(|(_, order)| order.z_index);
        assert_eq!(order, Some(4));
    }

    fn opacity_of(world: &mut World, handle: &DrawableHandle) -> f32 {
        let id = ObjectId::from_raw(handle.id.as_raw() - 1);
        world
            .query::<(&MobjectId, &Opacity)>()
            .iter(world)
            .find(|(object, _)| object.0 == id)
            .unwrap()
            .1
            .0
    }

    #[test]
    fn absolute_geometry_turns_scales_and_skews_about_its_box_center() {
        use std::f64::consts::PI;

        let mut canvas = SceneModel::new(640, 360);
        // Box center (4, 2); the local origin stays at the scene origin.
        let triangle = canvas.polygon(vec![(3.0, 1.0), (5.0, 1.0), (4.0, 3.0)]);
        let arc = canvas.curved_arrow_arc(0.0, -2.5, 0.38, 1.9, 5.2);
        let line = canvas.line(1.0, 1.0, 3.0, 1.0).rotate_to(PI / 2.0);
        canvas.play(vec![
            triangle.animate().rotate_by(2.0 * PI).duration(1.0),
            arc.animate().rotate_by(PI).duration(1.0),
        ]);
        canvas.play(vec![
            triangle.animate().scale_to(2.0).duration(1.0),
            arc.animate().skew_to(0.4, 0.0).duration(1.0),
        ]);
        let (mut world, mut timeline) = compiled_world(&canvas);

        let world_point = |world: &mut World, handle: &DrawableHandle, local: DVec3| {
            transform_of(world, handle)
                .to_mat4()
                .transform_point3(local)
        };
        timeline.seek(&mut world, 0.0);
        let arc_center =
            transform_of(&mut world, &arc).translation + transform_of(&mut world, &arc).anchor;
        for time in [0.0, 0.3, 0.5, 1.0, 1.5, 2.0] {
            timeline.seek(&mut world, time);
            let center = world_point(&mut world, &triangle, DVec3::new(4.0, 2.0, 0.0));
            assert!(
                center.distance(DVec3::new(4.0, 2.0, 0.0)) < 1e-6,
                "{time}: {center:?}"
            );
            let pivot =
                transform_of(&mut world, &arc).translation + transform_of(&mut world, &arc).anchor;
            assert!(pivot.distance(arc_center) < 1e-6, "{time}: {pivot:?}");
        }
        assert!(arc_center.distance(DVec3::ZERO) > 1.0, "{arc_center:?}");
        // The declared rotation also turns the line about its midpoint.
        let start = world_point(&mut world, &line, DVec3::new(1.0, 1.0, 0.0));
        let end = world_point(&mut world, &line, DVec3::new(3.0, 1.0, 0.0));
        assert!(
            start.distance(DVec3::new(2.0, 0.0, 0.0)) < 1e-9,
            "{start:?}"
        );
        assert!(end.distance(DVec3::new(2.0, 2.0, 0.0)) < 1e-9, "{end:?}");
    }

    #[test]
    fn pivot_turns_of_a_rotated_object_start_from_its_pose() {
        let mut canvas = SceneModel::new(640, 360);
        let bar = canvas
            .rect(2.0, 0.4)
            .move_to(1.0, 0.0)
            .with_pivot(0.0, 0.0)
            .rotate_to(0.5);
        canvas.wait(1.0);
        canvas.play(vec![bar.animate().rotate_by(1.0).duration(1.0)]);
        let (mut world, mut timeline) = compiled_world(&canvas);
        let tip = |world: &mut World| {
            transform_of(world, &bar)
                .to_mat4()
                .transform_point3(DVec3::new(2.0, 0.0, 0.0))
        };
        let radius = |point: DVec3| point.truncate().length();
        timeline.seek(&mut world, 0.5);
        let declared = tip(&mut world);
        timeline.seek(&mut world, 1.0);
        let started = tip(&mut world);
        assert!(
            started.distance(declared) < 1e-6,
            "{started:?} vs {declared:?}"
        );
        // The swing follows a polyline with one-degree steps.
        for time in [1.3, 1.7, 2.0] {
            timeline.seek(&mut world, time);
            let point = tip(&mut world);
            assert!(
                (radius(point) - radius(declared)).abs() < 1e-4,
                "{time}: {point:?}"
            );
        }
        let angle = |point: DVec3| point.y.atan2(point.x);
        assert!((angle(tip(&mut world)) - angle(declared) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn pop_in_hides_the_screen_until_the_next_pop_out() {
        let mut canvas = SceneModel::new(640, 360);
        let frame = canvas.rect(2.0, 1.0).move_to(3.0, 2.0);
        let view = canvas
            .rect(4.0, 2.0)
            .move_to(-4.0, -1.0)
            .camera_view(&frame)
            .unwrap();
        canvas.play(vec![view.pop_out().duration(1.0)]);
        canvas.play(vec![view.pop_in().duration(1.0)]);
        canvas.wait(1.0);
        canvas.play(vec![view.pop_out().duration(1.0)]);
        canvas.play(vec![view.screen().animate().fade_out().duration(1.0)]);
        let (mut world, mut timeline) = compiled_world(&canvas);
        for (time, expected) in [
            (0.5, 1.0),
            (1.5, 1.0),
            (2.0, 0.0),
            (2.5, 0.0),
            (3.0, 1.0),
            (3.5, 1.0),
            (4.0, 1.0),
            (5.0, 0.0),
        ] {
            timeline.seek(&mut world, time);
            let opacity = opacity_of(&mut world, view.screen());
            assert!((opacity - expected).abs() < 1e-6, "{time}: {opacity}");
        }
        timeline.seek(&mut world, 1.5);
        assert!((opacity_of(&mut world, view.screen()) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn stops_record_their_ambient_loop_length() {
        let mut canvas = SceneModel::new(640, 360);
        let dot = canvas.dot(0.1);
        canvas.wait(1.0);
        canvas
            .stop_with_loop(
                Some("placas".into()),
                crate::canvas::Composition::leaf(dot.animate().shift_by(1.0, 0.0).duration(1.5)),
            )
            .unwrap();
        canvas.wait(0.5);
        canvas.stop(None).unwrap();
        let stops = &canvas.segment_manifest().segments[0].stops;
        assert_eq!(stops.len(), 2);
        assert_eq!((stops[0].time, stops[0].ambient), (1.0, Some(1.5)));
        assert_eq!((stops[1].time, stops[1].ambient), (3.0, None));
        canvas.wait(1.0);
        assert!(matches!(
            canvas.stop_with_loop(
                None,
                crate::canvas::Composition::leaf(dot.animate().shift_by(1.0, 0.0).duration(0.0)),
            ),
            Err(crate::canvas::StopLoopError::Empty)
        ));
    }

    #[test]
    fn bounds_measure_any_drawable_at_the_cursor() {
        let mut canvas = SceneModel::new(640, 360);
        let rect = canvas.rect(2.0, 1.0).move_to(3.0, -1.0);
        let group = {
            let a = canvas.square(1.0).move_to(-4.0, 0.0);
            let b = canvas.circle(0.5).move_to(-1.0, 2.0);
            canvas.group(&[&a, &b])
        };
        canvas.play(vec![rect.animate().move_to(0.0, 0.0).duration(1.0)]);
        let canvas = canvas.into_shared();
        let (rect_box, group_box) = {
            let _guard = canvas.lock().unwrap();
            (rect.clone(), group.clone())
        };
        let rect_box = rect_box.bounds().unwrap();
        assert!((rect_box.width() - 2.0).abs() < 1e-9 && (rect_box.height() - 1.0).abs() < 1e-9);
        // Measured at the cursor, after the move ended.
        assert!(rect_box.center().truncate().length() < 1e-9, "{rect_box:?}");
        let group_box = group_box.bounds().unwrap();
        assert!((group_box.min.x + 4.5).abs() < 1e-9 && (group_box.max.y - 2.5).abs() < 1e-9);
    }

    #[test]
    fn pixels_map_onto_the_displayed_image() {
        use gaanim_objects::primitives::ImageView;
        let mut canvas = SceneModel::new(640, 360);
        let image = gaanim_core::peniko::ImageData {
            data: gaanim_core::peniko::Blob::new(std::sync::Arc::new(vec![255u8; 4 * 200 * 100])),
            format: gaanim_core::peniko::ImageFormat::Rgba8,
            alpha_type: gaanim_core::peniko::ImageAlphaType::Alpha,
            width: 200,
            height: 100,
        };
        // 200x100 px shown 4 units wide: 50 px per unit.
        let view = ImageView {
            source_x: 0.0,
            source_y: 0.0,
            source_width: 200.0,
            source_height: 100.0,
            display_width: 4.0,
            display_height: 2.0,
            scale_x: 0.02,
            scale_y: 0.02,
            quality: gaanim_core::peniko::ImageQuality::Medium,
        };
        let picture = canvas
            .spawn(SpawnKind::Image { image, view })
            .move_to(1.0, 1.0);
        let corner = picture.pixel(0.0, 0.0).unwrap();
        let inner = picture.pixel(150.0, 25.0).unwrap();
        assert!(canvas.rect(1.0, 1.0).pixel(0.0, 0.0).is_err());
        let marker = canvas.dot(0.05).at_anchor_point(inner);
        let (mut world, mut timeline) = compiled_world(&canvas);
        timeline.seek(&mut world, 0.0);
        let at = transform_of(&mut world, &marker).translation;
        // Pixel (150, 25): x = -2 + 3 = 1, y = 1 - 0.5 = 0.5, then moved by (1, 1).
        assert!(at.distance(DVec3::new(2.0, 1.5, 0.0)) < 1e-9, "{at:?}");
        assert_eq!(corner.offset, DVec3::new(-2.0, 1.0, 0.0));
    }

    #[test]
    fn matrix_to_applies_a_linear_map_about_the_pivot() {
        use crate::canvas::{LinearMap2D, LinearMapError};
        assert_eq!(
            LinearMap2D::new([[1.0, 2.0], [2.0, 4.0]]),
            Err(LinearMapError::Singular)
        );
        let rows = [[0.8, 0.3], [-0.5, 1.2]];
        let map = LinearMap2D::new(rows).unwrap();
        let mut canvas = SceneModel::new(640, 360);
        let square = canvas.square(2.0).move_to(1.0, 1.0).matrix_to(map);
        let mirrored = canvas
            .square(2.0)
            .matrix_to(LinearMap2D::new([[-1.0, 0.0], [0.0, 1.0]]).unwrap());
        let animated = canvas.square(2.0);
        canvas.play(vec![animated.animate().matrix_to(map).duration(1.0)]);
        let (mut world, mut timeline) = compiled_world(&canvas);
        timeline.seek(&mut world, 1.0);
        let apply = |local: DVec3| {
            DVec3::new(
                rows[0][0] * local.x + rows[0][1] * local.y,
                rows[1][0] * local.x + rows[1][1] * local.y,
                0.0,
            )
        };
        for local in [DVec3::new(1.0, 1.0, 0.0), DVec3::new(-1.0, 0.5, 0.0)] {
            let point = transform_of(&mut world, &square)
                .to_mat4()
                .transform_point3(local);
            assert!(
                point.distance(DVec3::new(1.0, 1.0, 0.0) + apply(local)) < 1e-9,
                "{point:?}"
            );
            let point = transform_of(&mut world, &animated)
                .to_mat4()
                .transform_point3(local);
            assert!(point.distance(apply(local)) < 1e-6, "{point:?}");
        }
        let point = transform_of(&mut world, &mirrored)
            .to_mat4()
            .transform_point3(DVec3::new(1.0, 0.5, 0.0));
        assert!(
            point.distance(DVec3::new(-1.0, 0.5, 0.0)) < 1e-9,
            "{point:?}"
        );
    }

    #[test]
    fn rectangular_insets_take_their_own_aspect() {
        use crate::canvas::{
            CameraInsetOptions, CameraInsetShape, CameraViewError, CanvasEndpoint,
        };
        let mut canvas = SceneModel::new(640, 360);
        let target = CanvasEndpoint::Static(DVec3::new(-3.0, 0.0, 0.0));
        let tall = canvas
            .camera_inset(
                target.clone(),
                CameraInsetOptions {
                    size: Some(2.0),
                    aspect: Some(0.5),
                    shape: CameraInsetShape::Rect,
                    ..Default::default()
                },
            )
            .unwrap();
        assert!(matches!(
            canvas.camera_inset(
                target,
                CameraInsetOptions {
                    aspect: Some(2.0),
                    shape: CameraInsetShape::Circle,
                    ..Default::default()
                },
            ),
            Err(CameraViewError::InvalidAspect)
        ));
        let (mut world, mut timeline) = compiled_world(&canvas);
        timeline.seek(&mut world, 0.0);
        for part in [tall.screen(), tall.frame()] {
            let id = ObjectId::from_raw(part.id.as_raw() - 1);
            let bounds = world
                .query::<(&MobjectId, &gaanim_scene::LocalBounds)>()
                .iter(&world)
                .find(|(object, _)| object.0 == id)
                .unwrap()
                .1
                .0;
            assert!(
                (bounds.width() / bounds.height() - 0.5).abs() < 1e-9,
                "{bounds:?}"
            );
        }
    }

    #[test]
    fn inset_frames_and_connectors_enter_and_leave_with_their_screen() {
        use crate::canvas::{CameraInsetOptions, CanvasEndpoint};

        let mut canvas = SceneModel::new(640, 360);
        let target = CanvasEndpoint::Static(DVec3::new(-3.0, -1.0, 0.0));
        let inset = canvas
            .camera_inset(target.clone(), CameraInsetOptions::default())
            .unwrap();
        canvas.wait(0.5);
        // Declared mid-scene, it still waits for its entry.
        let late = canvas
            .camera_inset(target, CameraInsetOptions::default())
            .unwrap();
        canvas.wait(0.5);
        canvas.play(vec![
            inset.pop_out().duration(1.0),
            late.pop_out().duration(1.0),
        ]);
        canvas.play(vec![inset.pop_in().duration(1.0)]);
        canvas.wait(1.0);
        canvas.play(vec![inset.pop_out().duration(1.0)]);
        let (mut world, mut timeline) = compiled_world(&canvas);

        let parts = |view: &crate::canvas::CameraViewHandle| {
            let mut parts = vec![view.screen().clone(), view.frame().clone()];
            parts.extend(view.connectors().iter().cloned());
            parts
        };
        for (time, shown) in [
            (0.25, false),
            (1.5, true),
            (2.5, true),
            (3.5, false),
            (4.5, true),
        ] {
            timeline.seek(&mut world, time);
            for part in parts(&inset) {
                let opacity = opacity_of(&mut world, &part);
                assert_eq!(opacity > 0.5, shown, "{time}: {opacity}");
            }
        }
        for (time, shown) in [(0.75, false), (1.5, true)] {
            timeline.seek(&mut world, time);
            for part in parts(&late) {
                let opacity = opacity_of(&mut world, &part);
                assert_eq!(opacity > 0.5, shown, "{time}: {opacity}");
            }
        }
    }

    #[test]
    fn dimensions_are_visible_from_their_declaration_without_an_entry() {
        use crate::canvas::{CanvasEndpoint, DimensionOptions};

        let mut canvas = SceneModel::new(640, 360);
        let endpoint = |x: f64, y: f64| CanvasEndpoint::Static(DVec3::new(x, y, 0.0));
        // Declared before the labelled one: `opacity_of` maps handle ids to
        // runtime ids, which the label's glyphs shift.
        let entering = canvas
            .dimension_between_with_options(
                endpoint(1.0, 0.0),
                endpoint(4.0, 0.0),
                0.3,
                DimensionOptions::default(),
            )
            .unwrap();
        let still = canvas
            .dimension_between_with_options(
                endpoint(-5.5, 0.1),
                endpoint(-2.5, 0.1),
                0.3,
                DimensionOptions {
                    label: Some("control".into()),
                    ..Default::default()
                },
            )
            .unwrap();
        canvas.wait(1.0);
        canvas.play(vec![entering.drawable.animate().fade_in().duration(1.0)]);
        let (mut world, mut timeline) = compiled_world(&canvas);
        // Opacity is inherited, so the entry fades the group itself.
        timeline.seek(&mut world, 0.5);
        for part in [&still.line, &still.extensions] {
            assert!(opacity_of(&mut world, part) > 0.99);
        }
        assert!(opacity_of(&mut world, &entering.drawable) < 1e-6);
        timeline.seek(&mut world, 2.0);
        for part in [&entering.drawable, &entering.extensions] {
            assert!(opacity_of(&mut world, part) > 0.99);
        }
    }

    #[test]
    fn scale_entries_pin_their_box_point_with_absolute_geometry_and_pivots() {
        let mut canvas = SceneModel::new(640, 360);
        // Declared in absolute coordinates: the pivot stays at the origin.
        let triangle = canvas.polygon(vec![(3.0, 1.0), (5.0, 1.0), (4.0, 3.0)]);
        let square = canvas.square(1.0).with_pivot(0.3, 0.2).move_to(-2.0, -2.0);
        canvas.play(vec![
            triangle.animate().grow_from_center().duration(1.0),
            square
                .animate()
                .grow_from_edge(gaanim_layout::Direction::Down)
                .duration(1.0),
        ]);
        canvas.play(vec![triangle.animate().scale_by(2.0).duration(1.0)]);
        let (mut world, mut timeline) = compiled_world(&canvas);

        let world_point = |world: &mut World, handle: &DrawableHandle, local: DVec3| {
            transform_of(world, handle)
                .to_mat4()
                .transform_point3(local)
        };
        for (handle, local) in [
            (&triangle, DVec3::new(4.0, 2.0, 0.0)),
            (&square, DVec3::new(0.0, -0.5, 0.0)),
        ] {
            timeline.seek(&mut world, 1.0);
            let pinned = world_point(&mut world, handle, local);
            for time in [0.1, 0.5, 0.9] {
                timeline.seek(&mut world, time);
                let point = world_point(&mut world, handle, local);
                assert!(
                    point.distance(pinned) < 1e-6,
                    "{point:?} drifted from {pinned:?} at {time}"
                );
            }
        }
        timeline.seek(&mut world, 2.0);
        assert!((transform_of(&mut world, &triangle).scale.x - 2.0).abs() < 1e-6);
    }

    #[test]
    fn trims_chain_from_the_current_window() {
        let mut canvas = SceneModel::new(640, 360);
        let ring = canvas.circle(1.0).trim(Some(0.5), Some(0.5), None, None);
        canvas.play(vec![ring.animate().trim(Some(0.0), Some(1.0), None)]);
        canvas.play(vec![ring.animate().trim(None, None, Some(0.25))]);
        let timeline = compiled_timeline(&canvas);
        let mut trims: Vec<_> = timeline
            .clips
            .values()
            .filter_map(|clip| match &clip.payload {
                gaanim_timeline::clip::ClipPayload::Animation(
                    gaanim_timeline::clip::AnimationSpec {
                        lens: gaanim_timeline::clip::PropertyLensSpec::PathTrim { from, to, .. },
                        ..
                    },
                ) => Some((clip.start, clip.duration, *from, *to)),
                _ => None,
            })
            .collect();
        trims.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.total_cmp(&b.1)));
        assert_eq!(
            trims
                .iter()
                .map(|trim| (trim.2, trim.3))
                .collect::<Vec<_>>(),
            [
                ([0.0, 1.0, 0.0], [0.5, 0.5, 0.0]),
                ([0.5, 0.5, 0.0], [0.0, 1.0, 0.0]),
                ([0.0, 1.0, 0.0], [0.0, 1.0, 0.25]),
            ]
        );
        assert_eq!(trims[0].1, 0.0);
    }

    #[test]
    fn dash_offset_animations_continue_from_the_current_offset() {
        let mut canvas = SceneModel::new(640, 360);
        let style = gaanim_core::kurbo::Stroke::new(0.05).with_dashes(0.5, [0.2, 0.1]);
        let ring = canvas
            .circle(1.0)
            .stroke_with_style(Brush::Solid(PenikoColor::WHITE), style);
        canvas.play(vec![ring.animate().dash_offset(2.0)]);
        canvas.play(vec![ring.animate().dash_offset(3.0)]);
        let timeline = compiled_timeline(&canvas);
        let mut lenses: Vec<_> = timeline
            .clips
            .values()
            .filter_map(|clip| match &clip.payload {
                gaanim_timeline::clip::ClipPayload::Animation(
                    gaanim_timeline::clip::AnimationSpec {
                        lens: gaanim_timeline::clip::PropertyLensSpec::Dynamic(lens),
                        ..
                    },
                ) => Some((clip.start, format!("{lens:?}"))),
                _ => None,
            })
            .collect();
        lenses.sort_by(|a, b| a.0.total_cmp(&b.0));
        let lenses: Vec<_> = lenses.into_iter().map(|(_, lens)| lens).collect();
        assert_eq!(
            lenses,
            [
                "DashOffsetLens { from: 0.5, to: 2.0 }",
                "DashOffsetLens { from: 2.0, to: 3.0 }",
            ]
        );
    }

    #[test]
    fn dash_flow_updaters_reach_every_drawn_member() {
        let mut canvas = SceneModel::new(640, 360);
        let first = canvas.circle(1.0);
        let second = canvas.square(1.0);
        let pipes = canvas.group(&[&first, &second]);
        canvas.wait(1.0);
        pipes.add_updater(crate::canvas::UpdaterPreset::DashFlow { speed: 0.5 });
        canvas.wait(2.0);
        pipes.remove_updater();

        let world = World::new();
        let mut queue = CommandQueue::default();
        let mut commands = Commands::new(&mut queue, &world);
        let mut timeline = Timeline::new();
        let fonts = gaanim_text::font::FontRegistry::new();
        let text_config = gaanim_text::prelude::TextConfig::default();
        canvas.compile_into(&mut commands, &mut timeline, &fonts, &text_config);
        let mut world = world;
        queue.apply(&mut world);

        for member in [&first, &second] {
            let entity = entity_of(&mut world, member);
            let flow = world.get::<gaanim_animation::DashFlow>(entity).unwrap();
            assert_eq!(flow.runs, [(0.5, 1.0, Some(3.0))]);
        }
    }

    #[test]
    fn effect_animations_continue_from_the_current_effects() {
        use crate::canvas::{DropShadow, Glow};
        let mut canvas = SceneModel::new(640, 360);
        let card = canvas.rect(2.0, 1.0).shadow(
            PenikoColor::BLACK,
            gaanim_core::glam::DVec2::new(0.0, -0.05),
            0.05,
        );
        let glow = Glow {
            radius: 0.5,
            intensity: 2.0,
            color: PenikoColor::WHITE,
        };
        let lifted = DropShadow {
            color: PenikoColor::BLACK,
            offset: gaanim_core::glam::DVec2::new(0.0, -0.25),
            blur_radius: 0.4,
        };
        canvas.play(vec![
            card.animate()
                .shadow(Some(lifted.clone()))
                .glow(Some(glow.clone()))
                .scale_to(1.04),
        ]);
        canvas.play(vec![card.animate().glow(None)]);
        let timeline = compiled_timeline(&canvas);
        let mut lenses: Vec<_> = timeline
            .clips
            .values()
            .filter_map(|clip| match &clip.payload {
                gaanim_timeline::clip::ClipPayload::Animation(
                    gaanim_timeline::clip::AnimationSpec {
                        lens: gaanim_timeline::clip::PropertyLensSpec::Dynamic(lens),
                        ..
                    },
                ) => Some((clip.start, format!("{lens:?}"))),
                _ => None,
            })
            .collect();
        lenses.sort_by(|a, b| a.0.total_cmp(&b.0));
        assert_eq!(lenses.len(), 2);
        assert!(lenses[0].1.contains("blur_radius: 0.05"), "{}", lenses[0].1);
        assert!(lenses[0].1.contains("blur_radius: 0.4"));
        // The second clip starts from the glow and keeps the lifted shadow.
        assert!(lenses[1].1.contains("intensity: 2.0"));
        assert!(lenses[1].1.contains("to: EffectState { glow: None"));
        assert!(lenses[1].1.matches("blur_radius: 0.4").count() == 2);
    }

    #[test]
    fn spatial_stagger_delays_follow_distance_from_the_origin() {
        use crate::canvas::{Composition, StaggerLayout, StaggerOrigin};
        let mut canvas = SceneModel::new(640, 360);
        let dots: Vec<_> = (0..5)
            .map(|index| canvas.circle(0.1).move_to(index as f64 - 2.0, 0.0))
            .collect();
        let starts = |origin, total| {
            let children = dots
                .iter()
                .map(|dot| Composition::leaf(dot.animate().opacity(0.5).duration(1.0)))
                .collect();
            let layout = StaggerLayout {
                origin,
                grid: None,
                total,
                easing: None,
            };
            let schedule = Composition::stagger_layout(children, 0.1, layout)
                .unwrap()
                .schedule(None)
                .unwrap();
            schedule
                .entries
                .iter()
                .map(|entry| (entry.start * 1000.0).round() / 1000.0)
                .collect::<Vec<_>>()
        };
        assert_eq!(
            starts(StaggerOrigin::Start, None),
            [0.0, 0.1, 0.2, 0.3, 0.4]
        );
        assert_eq!(
            starts(StaggerOrigin::Center, None),
            [0.2, 0.1, 0.0, 0.1, 0.2]
        );
        assert_eq!(
            starts(StaggerOrigin::Edges, None),
            [0.0, 0.1, 0.2, 0.1, 0.0]
        );
        assert_eq!(
            starts(StaggerOrigin::Point(-2.0, 0.0), Some(2.0)),
            [0.0, 0.5, 1.0, 1.5, 2.0]
        );
        let mut random = starts(StaggerOrigin::Random(7), None);
        assert_eq!(random, starts(StaggerOrigin::Random(7), None));
        random.sort_by(f64::total_cmp);
        assert_eq!(random, [0.0, 0.1, 0.2, 0.3, 0.4]);
    }

    #[test]
    fn repeated_compositions_report_their_expanded_schedule() {
        use crate::canvas::Composition;
        let mut canvas = SceneModel::new(640, 360);
        let dot = canvas.circle(0.2);
        let spin = Composition::leaf(dot.animate().rotate_by(1.0).duration(1.0).repeat(
            3,
            gaanim_math::RepeatMode::Cycle,
            0.5,
        ));
        assert_eq!(spin.schedule(None).unwrap().span, 4.0);
        let pair = Composition::parallel(vec![
            Composition::leaf(dot.animate().shift_by(1.0, 0.0).duration(1.0)),
            Composition::leaf(canvas.circle(0.1).animate().rotate_by(1.0).duration(0.5)),
        ])
        .unwrap()
        .repeat(2, 0.5)
        .unwrap();
        let schedule = pair.schedule(None).unwrap();
        assert_eq!(schedule.span, 2.5);
        assert_eq!(schedule.entries.len(), 4);
        canvas
            .play_composition_configured(pair, None, None)
            .unwrap();
        let clips = translation_clips(&compiled_timeline(&canvas));
        assert_eq!(clips.len(), 2);
        assert_eq!(clips[1].0, 1.5);
        assert_eq!(clips[1].2, clips[0].3);
    }

    fn write_start_times(anim: impl FnOnce(&DrawableHandle) -> crate::canvas::Anim) -> Vec<f64> {
        let mut canvas = SceneModel::new(640, 360);
        let text = canvas.text("uno bb dos");
        canvas.play(vec![anim(&text).duration(1.0)]);

        let world = World::new();
        let mut queue = CommandQueue::default();
        let mut commands = Commands::new(&mut queue, &world);
        let mut timeline = Timeline::new();
        let fonts = gaanim_text::font::FontRegistry::new();
        let text_config = gaanim_text::prelude::TextConfig::default();
        canvas.compile_into(&mut commands, &mut timeline, &fonts, &text_config);

        let mut starts: Vec<f64> = timeline
            .clips
            .values()
            .filter(|clip| {
                matches!(
                    &clip.payload,
                    gaanim_timeline::clip::ClipPayload::Animation(
                        gaanim_timeline::clip::AnimationSpec {
                            lens: gaanim_timeline::clip::PropertyLensSpec::PathCompletion {
                                from,
                                to,
                            },
                            ..
                        }
                    ) if *from == 0.0 && *to == 1.0
                )
            })
            .map(|clip| clip.start)
            .collect();
        starts.sort_by(f64::total_cmp);
        starts
    }

    fn distinct_counts(starts: &[f64]) -> Vec<usize> {
        let mut counts: Vec<usize> = Vec::new();
        for (index, start) in starts.iter().enumerate() {
            if index > 0 && (start - starts[index - 1]).abs() < 1e-9 {
                *counts.last_mut().unwrap() += 1;
            } else {
                counts.push(1);
            }
        }
        counts
    }

    #[test]
    fn write_by_word_starts_each_word_together() {
        let by_glyph = write_start_times(|text| text.animate().write());
        assert_eq!(distinct_counts(&by_glyph), [1; 8]);

        let by_word =
            write_start_times(|text| text.animate().write().reveal_unit(TextRevealUnit::Word));
        assert_eq!(distinct_counts(&by_word), [3, 2, 3]);

        // Center order starts the middle word "bb" first, then both sides.
        let centered = write_start_times(|text| {
            text.animate()
                .write()
                .reveal_unit(TextRevealUnit::Word)
                .draw_order(crate::anim::DrawOrder::Center)
        });
        assert_eq!(distinct_counts(&centered), [2, 6]);
    }

    #[test]
    fn equation_expansion_morphs_semantic_terms_without_cross_fading_pairs() {
        let mut canvas = SceneModel::new(640, 360);
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
        let mut canvas = SceneModel::new(640, 360);
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
        let mut canvas = SceneModel::new(640, 360);
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
        let source = canvas.text_spec(equation(" dot 5 = ", "25")).scale_to(2.0);
        let target = canvas.text_spec(equation(" = ", "5")).scale_to(2.0);
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
        let mut canvas = SceneModel::new(640, 360);
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
        let mut canvas = SceneModel::new(640, 360);
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
            .move_to(0.0, 120.0);
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
        let source = canvas.text_spec(equation(" dot 5 = ", "25")).scale_to(2.0);
        let target = canvas.text_spec(equation(" = ", "5")).scale_to(2.0);
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
        let mut canvas = SceneModel::new(640, 360);
        let source = canvas
            .math_text("E = m c^2")
            .define_tag("mass", "m", None)
            .move_to(0.0, 70.0);
        let target = canvas
            .math_text("p = m v")
            .define_tag("mass", "m", None)
            .move_to(0.0, -90.0);
        let copy = source
            .tag("mass")
            .unwrap()
            .copy_to(&target.tag("mass").unwrap(), 0.8)
            .expect("selections share a SceneModel");
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
        let mut canvas = SceneModel::new(640, 360);
        let title = canvas.text("One variable");
        let source = canvas
            .math_text("E = m c^2")
            .define_tag("mass", "m", None)
            .move_to(0.0, 70.0);
        let target = canvas
            .math_text("p = m v")
            .define_tag("mass", "m", None)
            .move_to(0.0, -90.0);
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
            .expect("selections share a SceneModel");
        canvas.play(vec![copy]);
        canvas.wait(0.25);

        let world = World::new();
        let mut queue = CommandQueue::default();
        let mut commands = Commands::new(&mut queue, &world);
        let mut timeline = Timeline::new();
        let fonts = gaanim_text::font::FontRegistry::new();
        let text_config = gaanim_text::prelude::TextConfig::default();
        canvas.compile_into(&mut commands, &mut timeline, &fonts, &text_config);

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
        let mut canvas = SceneModel::new(640, 360);
        let label = canvas.text("Aparece desde abajo");
        let entrance = label
            .animate()
            .fade_in_from(crate::canvas::Direction::Down, 72.0)
            .duration(0.8);
        canvas.play(vec![entrance]);

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
        let mut canvas = SceneModel::new(640, 360);
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
                .any(|b| b.0.width() > 0.5)
        );
    }

    #[test]
    fn paragraph_with_inline_math_compiles_to_vector_glyphs() {
        let mut canvas = SceneModel::new(640, 360);
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
            text_compositions: RefCell::default(),
            text_candidates: RefCell::default(),
            line_extents: RefCell::default(),
            cap_heights: RefCell::default(),
            natural_text_sizes: RefCell::default(),
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
                text_compositions: RefCell::default(),
                text_candidates: RefCell::default(),
                line_extents: RefCell::default(),
                cap_heights: RefCell::default(),
                natural_text_sizes: RefCell::default(),
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
                measurer.text_compositions.borrow().get(&id),
                Some(&TextComposition::Natural),
                "{content:?} fits unwrapped and must be materialized unwrapped"
            );
            // A hugging parent offers exactly the measured ink width next; the
            // text must keep its single line instead of breaking again.
            let hugged = gaanim_layout::IntrinsicMeasure::measure(
                &measurer,
                id,
                gaanim_layout::BoxConstraints {
                    min: DVec2::ZERO,
                    max: DVec2::new(wide.x, 1000.0),
                },
            )
            .unwrap();
            assert_eq!(hugged, wide, "{content:?} rewrapped at its own ink width");
            assert_eq!(
                measurer.text_compositions.borrow().get(&id),
                Some(&TextComposition::Natural)
            );
        }
    }

    #[test]
    fn wrapped_text_keeps_its_lines_when_offered_its_ink_width() {
        let id = gaanim_layout::LayoutId(3);
        let fonts = gaanim_text::font::FontRegistry::new();
        let measurer = CompiledLayoutMeasure {
            fixed: BTreeMap::new(),
            texts: BTreeMap::from([(
                id,
                CompiledTextMeasure {
                    spec: StructuredTextSpec::new(
                        vec!["A wrapped paragraph inside a hugging card keeps the lines it was measured with.".into()],
                        None,
                        gaanim_text::prelude::TextStyle::default(),
                        gaanim_text::prelude::TextFlow::default(),
                    )
                    .unwrap(),
                    font_size: 28.0,
                    font_family: "New Computer Modern".into(),
                    math_font: "New Computer Modern Math".into(),
                    color: gaanim_core::peniko::Color::WHITE,
                },
            )]),
            text_compositions: RefCell::default(),
            text_candidates: RefCell::default(),
            line_extents: RefCell::default(),
            cap_heights: RefCell::default(),
            natural_text_sizes: RefCell::default(),
            font_registry: &fonts,
        };
        let measure = |width: f64| {
            gaanim_layout::IntrinsicMeasure::measure(
                &measurer,
                id,
                gaanim_layout::BoxConstraints {
                    min: DVec2::ZERO,
                    max: DVec2::new(width, 1000.0),
                },
            )
            .unwrap()
        };
        let first = measure(400.0);
        assert!(
            first.x < 400.0,
            "fixture must wrap with ragged ink: {first:?}"
        );
        let hugged = measure(first.x);
        assert_eq!(hugged, first, "offering the ink width must not add lines");
        assert_eq!(
            measurer.text_compositions.borrow().get(&id).copied(),
            Some(TextComposition::Wrapped {
                width: 400.0,
                ink_width: first.x,
            })
        );
        // Less room than the ink really rewraps.
        let narrow = measure(first.x * 0.5);
        assert!(narrow.y > first.y, "narrow={narrow:?}, first={first:?}");
    }

    #[test]
    fn nested_layout_materializes_paragraph_at_the_outer_assigned_width() {
        let mut canvas = SceneModel::new(1280, 720);
        let paragraph = canvas.configured_text(
            "Nested responsive paragraphs must use the card width instead of the safe frame width.",
            gaanim_text::prelude::TextStyle {
                size: Some(40.0),
                ..Default::default()
            },
            gaanim_text::prelude::TextFlow::default(),
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
            visible_bounds.iter().any(|bounds| bounds.width() > 200.0
                && bounds.width() < 300.0
                && bounds.height() > 100.0),
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
    fn root_layout_preserves_authored_at_translation() {
        let mut canvas = SceneModel::new(1280, 720);
        let first = canvas.rect(160.0, 60.0);
        let second = canvas.rect(120.0, 40.0);
        let root = canvas.group(&[&first, &second]);
        first.claim_layout(&root).unwrap();
        second.claim_layout(&root).unwrap();
        canvas.reflow_layout(
            &root,
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
            crate::canvas::LayoutSpec {
                kind: gaanim_layout::LayoutNodeKind::Column { wrap: false },
                style: gaanim_layout::LayoutStyle {
                    gap: DVec2::splat(24.0),
                    ..Default::default()
                },
                within: LayoutWithin::Intrinsic,
            },
            1,
            None,
            None,
            None,
        );
        root.move_to(400.0, 200.0);

        let mut world = compile_canvas_for_layout(canvas);
        let transform = world
            .query_filtered::<&SpatialTransform, (
                bevy::prelude::With<gaanim_scene::GroupMarker>,
                bevy::prelude::Without<bevy::prelude::ChildOf>,
            )>()
            .single(&world)
            .expect("one root layout group");
        assert_point_close(transform.translation, DVec3::new(400.0, 200.0, 0.0));
    }

    #[test]
    fn root_layout_replays_anchor_scale_and_rotation_against_resolved_bounds() {
        let mut canvas = SceneModel::new(1280, 720);
        let first = canvas.rect(160.0, 60.0);
        let second = canvas.rect(120.0, 40.0);
        let root = canvas.group(&[&first, &second]);
        first.claim_layout(&root).unwrap();
        second.claim_layout(&root).unwrap();
        canvas.reflow_layout(
            &root,
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
            crate::canvas::LayoutSpec {
                kind: gaanim_layout::LayoutNodeKind::Column { wrap: false },
                style: gaanim_layout::LayoutStyle {
                    gap: DVec2::splat(24.0),
                    ..Default::default()
                },
                within: LayoutWithin::Intrinsic,
            },
            1,
            None,
            None,
            None,
        );
        root.scale_to(1.5)
            .rotate_to(0.25)
            .at_anchor(300.0, 140.0, Anchor::TopRight);

        let mut world = compile_canvas_for_layout(canvas);
        let (bounds, transform) = world
            .query_filtered::<(&LocalBounds, &SpatialTransform), (
                bevy::prelude::With<gaanim_scene::GroupMarker>,
                bevy::prelude::Without<bevy::prelude::ChildOf>,
            )>()
            .single(&world)
            .map(|(bounds, transform)| (bounds.0, *transform))
            .expect("one root layout group");
        assert_point_close(transform.scale, DVec3::splat(1.5));
        assert_point_close(
            transform
                .to_mat4()
                .transform_point3(Anchor::TopRight.get_point(&bounds)),
            DVec3::new(300.0, 140.0, 0.0),
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
        assert!(!para.contains("lang:"), "no lang unless requested: {para}");
        let spanish = StructuredTextSpec::new(
            vec!["Hola".into()],
            None,
            gaanim_text::prelude::TextStyle::default(),
            gaanim_text::prelude::TextFlow {
                hyphenate: true,
                lang: Some("es".into()),
                ..Default::default()
            },
        )
        .unwrap();
        let spanish = structured_text_typst_source(
            &spanish,
            Some(400.0),
            32.0,
            "New Computer Modern",
            gaanim_core::peniko::Color::WHITE,
        );
        assert!(
            spanish.contains("hyphenate: true, lang: \"es\""),
            "{spanish}"
        );
        for invalid in ["", "e", "espa", "ES", "e\"s"] {
            assert!(
                StructuredTextSpec::new(
                    vec!["Hola".into()],
                    None,
                    gaanim_text::prelude::TextStyle::default(),
                    gaanim_text::prelude::TextFlow {
                        lang: Some(invalid.into()),
                        ..Default::default()
                    },
                )
                .is_err(),
                "{invalid:?} must be rejected"
            );
        }
        let txt = text_inline_typst_source("prueba $x^2$ fin", gaanim_core::peniko::Color::WHITE);
        assert!(txt.contains("$x^2$"));
        let txt2 =
            text_inline_typst_source("$alpha + beta = 1$", gaanim_core::peniko::Color::WHITE);
        assert!(txt2.contains("$alpha + beta = 1$"));
    }

    #[test]
    fn layout_reflow_animates_displaced_members_and_fades_the_insertion() {
        let mut canvas = SceneModel::new(640, 360);
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

    #[test]
    fn layout_records_what_the_editor_inspector_draws() {
        let mut canvas = SceneModel::new(640, 360);
        let first = canvas.rect(80.0, 30.0);
        let second = canvas.rect(80.0, 30.0);
        let container = canvas.group(&[&first, &second]);
        let member = |handle: &crate::canvas::DrawableHandle| crate::canvas::LayoutMemberSpec {
            id: handle.id,
            style: gaanim_layout::LayoutItemStyle::default(),
        };
        canvas.reflow_layout(
            &container,
            vec![member(&first), member(&second)],
            crate::canvas::LayoutSpec {
                kind: gaanim_layout::LayoutNodeKind::Row { wrap: false },
                style: gaanim_layout::LayoutStyle {
                    gap: DVec2::splat(20.0),
                    padding: gaanim_layout::Insets::all(10.0),
                    ..Default::default()
                },
                within: LayoutWithin::Intrinsic,
            },
            1,
            None,
            None,
            None,
        );
        canvas.record_layout_zones(vec![(
            "top".to_owned(),
            Bounds3D::new_2d(-1.0, 0.0, 1.0, 1.0),
        )]);
        canvas.wait(1.0);

        let (mut world, _) = compiled_world(&canvas);
        let mut inspections = world.query::<&gaanim_scene::LayoutInspection>();
        let inspection = inspections.iter(&world).next().expect("box inspection");
        let frame = inspection.at(0.0).expect("arrangement at the start");
        assert_eq!(frame.kind, gaanim_scene::LayoutInspectionKind::Row);
        assert_eq!(frame.padding, [10.0; 4]);
        assert_eq!(frame.cells.len(), 2);
        // The cells sit side by side, 20 apart, around the box's center.
        let gap = frame.cells[1].bounds.min.x - frame.cells[0].bounds.max.x;
        assert!((gap - 20.0).abs() < 1.0e-6, "{gap}");
        let zones = world.resource::<gaanim_scene::LayoutZones>();
        assert_eq!(zones.0.len(), 1);
        assert_eq!(zones.0[0].name, "top");
        assert_eq!((zones.0[0].segment, zones.0[0].start), (0, 0.0));
        assert!((zones.0[0].end - 1.0).abs() < 1.0e-9);
    }

    #[test]
    fn layout_reflow_keeps_a_member_offset_from_its_rest() {
        use gaanim_timeline::clip::{AnimationSpec, ClipPayload, PropertyLensSpec};
        let mut canvas = SceneModel::new(640, 360);
        let first = canvas.rect(80.0, 30.0);
        let second = canvas.rect(80.0, 30.0);
        let third = canvas.rect(80.0, 30.0);
        let container = canvas.group(&[&first, &second]);
        let spec = crate::canvas::LayoutSpec {
            kind: gaanim_layout::LayoutNodeKind::Column { wrap: false },
            style: gaanim_layout::LayoutStyle {
                gap: DVec2::splat(20.0),
                align: gaanim_layout::Align::Center,
                ..Default::default()
            },
            within: LayoutWithin::Intrinsic,
        };
        let member = |handle: &crate::canvas::DrawableHandle| crate::canvas::LayoutMemberSpec {
            id: handle.id,
            style: gaanim_layout::LayoutItemStyle::default(),
        };
        canvas.reflow_layout(
            &container,
            vec![member(&first), member(&second)],
            spec.clone(),
            1,
            None,
            None,
            None,
        );
        // The first member moves on its own, then the box gains a member.
        canvas.play(vec![first.animate().shift_by(30.0, 0.0).duration(0.5)]);
        canvas.set_group_members(&container, &[&first, &second, &third]);
        canvas.reflow_layout(
            &container,
            vec![member(&first), member(&second), member(&third)],
            spec,
            2,
            Some(0.5),
            Some(&third),
            None,
        );

        let (_, timeline) = compiled_world(&canvas);
        // Every member rests at x = 0; only the moved one stays 30 to the right
        // while the reflow slides it to its new row.
        let reflow_moves: Vec<_> = timeline
            .clips
            .values()
            .filter(|clip| clip.start >= 0.5 - 1.0e-9)
            .filter_map(|clip| match &clip.payload {
                ClipPayload::Animation(AnimationSpec {
                    lens: PropertyLensSpec::Translation { from, to },
                    ..
                }) => Some((*from, *to)),
                _ => None,
            })
            .collect();
        assert!(
            reflow_moves
                .iter()
                .any(|(from, to)| (from.x - 30.0).abs() < 1.0e-6 && (to.x - 30.0).abs() < 1.0e-6),
            "{reflow_moves:?}"
        );
        assert!(
            reflow_moves
                .iter()
                .all(|(_, to)| to.x.abs() < 1.0e-6 || (to.x - 30.0).abs() < 1.0e-6),
            "{reflow_moves:?}"
        );
    }

    #[test]
    fn text_transform_to_continues_the_source_handle_on_its_target() {
        use gaanim_timeline::clip::{AnimationSpec, ClipPayload, PropertyLensSpec};
        let mut canvas = SceneModel::new(640, 360);
        let source = canvas.math_text("x + 3 = 7");
        let target = canvas.math_text("x = 4");
        canvas.play(vec![
            source
                .animate()
                .transform_to(&target)
                .unwrap()
                .duration(1.0),
        ]);
        canvas.play(vec![source.animate().fade_out().duration(0.5)]);

        let (_, timeline) = compiled_world(&canvas);
        let opacity_clips: Vec<_> = timeline
            .clips
            .values()
            .filter_map(|clip| match &clip.payload {
                ClipPayload::Animation(AnimationSpec {
                    target,
                    lens: PropertyLensSpec::Opacity { to, .. },
                    label,
                    ..
                }) => Some((clip.start, clip.duration, *target, *to, label.clone())),
                _ => None,
            })
            .collect();
        // The target root is shown when the transition starts...
        let target_root = opacity_clips
            .iter()
            .find(|(start, _, _, to, label)| {
                *start == 0.0 && *to > 0.0 && label.as_deref() == Some("EquationHandoff")
            })
            .map(|(_, _, id, _, _)| *id)
            .expect("the transition reveals its target root");
        // ...and a later animation of the source handle drives that target.
        assert!(
            opacity_clips.iter().any(|(start, duration, id, to, _)| {
                (*start - 1.0).abs() < 1.0e-9
                    && *duration == 0.5
                    && *id == target_root
                    && *to == 0.0
            }),
            "fade_out on the source must continue on the target: {opacity_clips:?}"
        );
    }

    #[test]
    fn create_after_fade_out_shows_the_object_again() {
        use gaanim_timeline::clip::{AnimationSpec, ClipPayload, PropertyLensSpec};
        let mut canvas = SceneModel::new(640, 360);
        let line = canvas.line(-1.0, 0.0, 1.0, 0.0);
        canvas.play(vec![line.animate().create().duration(1.0)]);
        canvas.play(vec![line.animate().fade_out().duration(0.5)]);
        canvas.play(vec![line.animate().create().duration(1.0)]);

        let (_, timeline) = compiled_world(&canvas);
        let restored = timeline.clips.values().any(|clip| {
            matches!(
                &clip.payload,
                ClipPayload::Animation(AnimationSpec {
                    lens: PropertyLensSpec::Opacity { from, to },
                    ..
                }) if *from == 0.0 && *to == 1.0
            ) && (clip.start - 1.5).abs() < 1.0e-9
        });
        assert!(restored, "the second create() must restore the opacity");
    }

    #[test]
    fn responsive_layout_text_stays_hidden_until_its_fade_in() {
        let mut canvas = SceneModel::new(640, 360);
        let text = canvas.configured_text(
            "Hidden until it fades in",
            gaanim_text::prelude::TextStyle::default(),
            gaanim_text::prelude::TextFlow::default(),
        );
        let container = canvas.group(&[&text]);
        text.claim_layout(&container).unwrap();
        canvas.reflow_layout(
            &container,
            vec![crate::canvas::LayoutMemberSpec {
                id: text.id,
                style: gaanim_layout::LayoutItemStyle::default(),
            }],
            crate::canvas::LayoutSpec {
                kind: gaanim_layout::LayoutNodeKind::Column { wrap: false },
                style: gaanim_layout::LayoutStyle::default(),
                within: LayoutWithin::Safe,
            },
            1,
            None,
            None,
            None,
        );
        canvas.wait(0.5);
        canvas.play(vec![text.animate().fade_in().duration(0.5)]);

        let (_, timeline) = compiled_world(&canvas);
        // The layout recomposes the text at its offered width. That swap must
        // not reveal the replacement before the authored fade-in at 0.5 s.
        let reveal_starts: Vec<_> = timeline
            .clips
            .values()
            .filter(|clip| {
                matches!(
                    &clip.payload,
                    gaanim_timeline::clip::ClipPayload::Animation(
                        gaanim_timeline::clip::AnimationSpec {
                            lens: gaanim_timeline::clip::PropertyLensSpec::Opacity { to, .. },
                            ..
                        }
                    ) if *to > 0.0
                )
            })
            .map(|clip| clip.start)
            .collect();
        assert!(
            reveal_starts
                .iter()
                .any(|start| (start - 0.5).abs() < 1.0e-9),
            "the authored fade-in must remain: {reveal_starts:?}"
        );
        assert!(
            reveal_starts.iter().all(|start| *start >= 0.5 - 1.0e-9),
            "revealed before the fade-in: {reveal_starts:?}"
        );
    }

    #[test]
    fn points_compile_to_one_path_with_a_circle_per_position() {
        let mut canvas = SceneModel::new(640, 360);
        assert!(canvas.points(Vec::new(), 0.1).is_err());
        assert!(canvas.points(vec![(0.0, f64::NAN)], 0.1).is_err());
        assert!(canvas.points(vec![(0.0, 0.0)], 0.0).is_err());
        canvas
            .points(vec![(-1.0, 0.0), (2.0, 1.0), (0.5, -3.0)], 0.25)
            .expect("valid point cloud");

        let mut world = compile_canvas_for_layout(canvas);
        let mut query = world.query::<(
            &gaanim_scene::ObjectTag,
            &gaanim_scene::Path2D,
            &gaanim_scene::LocalBounds,
        )>();
        let (_, path, bounds) = query
            .iter(&world)
            .find(|(tag, _, _)| tag.0 == "Points")
            .expect("one Points object");
        let subpaths = path
            .0
            .elements()
            .iter()
            .filter(|element| matches!(element, gaanim_core::kurbo::PathEl::MoveTo(_)))
            .count();
        assert_eq!(subpaths, 3);
        assert_eq!(
            bounds.0.min.truncate(),
            gaanim_core::glam::DVec2::new(-1.25, -3.25)
        );
        assert_eq!(
            bounds.0.max.truncate(),
            gaanim_core::glam::DVec2::new(2.25, 1.25)
        );
    }

    #[test]
    fn readout_decimal_separator_reaches_the_compiled_text() {
        let mut canvas = SceneModel::new(640, 360);
        let number = canvas.reactive_readout(
            gaanim_animation::ScalarSource::constant(1.23456),
            ".2f",
            "",
            "",
            "—",
            None,
        );
        assert!(number.readout_decimal_separator('7').is_err());
        number.readout_decimal_separator(',').unwrap();
        let mut world = compile_canvas_for_layout(canvas);
        let mut query = world.query::<&gaanim_animation::ReactiveReadout>();
        let readout = query.single(&world).expect("exactly one compiled readout");
        assert_eq!(readout.last_text, "1,23");
    }

    #[test]
    fn sampled_series_op_compiles_to_driver_component() {
        use gaanim_animation::{SampledInterpolation, SampledProperty};

        let mut canvas = SceneModel::new(640, 360);
        let dot = canvas.dot(8.0);
        canvas.wait(1.5);
        dot.drive_from_samples(
            vec![0.0, 1.0],
            vec![1.0, 2.0],
            SampledProperty::TranslateX,
            SampledInterpolation::Linear,
            10.0,
            0.0,
        )
        .expect("valid sampled series");

        dot.drive_from_samples(
            vec![0.0, 1.0],
            vec![3.0, 4.0],
            SampledProperty::TranslateY,
            SampledInterpolation::Step,
            1.0,
            0.0,
        )
        .expect("valid sampled series");

        let mut world = compile_canvas_for_layout(canvas);
        let mut query = world.query::<&gaanim_animation::SampledSeriesDrivers>();
        let drivers = query.single(&world).expect("exactly one driven entity");
        assert_eq!(drivers.0.len(), 2, "x and y channels are independent");
        let y = drivers.get(SampledProperty::TranslateY).expect("y channel");
        assert_eq!(y.values.to_vec(), vec![3.0, 4.0]);
        let driver = drivers.get(SampledProperty::TranslateX).expect("x channel");
        assert_eq!(driver.property, SampledProperty::TranslateX);
        assert_eq!(driver.interpolation, SampledInterpolation::Linear);
        assert_eq!(driver.times.to_vec(), vec![0.0, 1.0]);
        assert_eq!(driver.values.to_vec(), vec![1.0, 2.0]);
        assert_eq!(driver.scale, 10.0);
        assert_eq!(driver.start_at, 1.5, "driver starts at the authored cursor");
        assert!(driver.base.is_none());
    }
}
