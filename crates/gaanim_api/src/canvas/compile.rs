//! Compile — replay SceneModel ops into SceneBuilder/Bevy.

use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::Duration;

thread_local! {
    /// How long each segment took to compile, while a replay profiles them.
    static SEGMENT_TIMES: RefCell<Option<Vec<(String, Duration)>>> = const { RefCell::new(None) };
}

/// Record how long each segment that this thread compiles takes, until
/// [`take_segment_times`].
pub(crate) fn time_segments() {
    SEGMENT_TIMES.with_borrow_mut(|times| *times = Some(Vec::new()));
}

/// The segments compiled since [`time_segments`], with how long each took.
pub(crate) fn take_segment_times() -> Vec<(String, Duration)> {
    SEGMENT_TIMES
        .with_borrow_mut(Option::take)
        .unwrap_or_default()
}

use bevy::prelude::*;
use gaanim_core::ObjectId;
use gaanim_core::glam::{DVec2, DVec3};
use gaanim_core::kurbo::{BezPath, Point, Rect, Shape, Vec2};
use gaanim_core::peniko::Color as PenikoColor;
use gaanim_math::Bounds3D;
use gaanim_scene::{
    ClearColor, FillBrush, LineListData, LocalBounds, Opacity, RenderOrder, StrokeBrush,
    TriangleMeshData, Visible,
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

/// Room, in scene units, an outline's antialiased edge takes beyond its
/// width in a shader effect's texture.
const OUTLINE_ANTIALIAS_MARGIN: f64 = 0.02;

/// The polyline through `points` evaluated at `time`; a point whose source
/// cannot be evaluated is left out.
fn reactive_polyline_path(
    points: &[(
        gaanim_animation::ScalarSource,
        gaanim_animation::ScalarSource,
    )],
    closed: bool,
    time: f64,
    signal: impl Fn(gaanim_core::ObjectId) -> Option<f64>,
) -> gaanim_core::kurbo::BezPath {
    let mut path = gaanim_core::kurbo::BezPath::new();
    for (x, y) in points {
        let (Ok(x), Ok(y)) = (x.evaluate(time, &signal), y.evaluate(time, &signal)) else {
            continue;
        };
        if path.elements().is_empty() {
            path.move_to((x, y));
        } else {
            path.line_to((x, y));
        }
    }
    if closed && path.elements().len() > 2 {
        path.close_path();
    }
    path
}

/// The 3D line strip through `points` evaluated at `time`, in `color` or
/// with `colors` (one per point): a point whose value cannot be read is
/// left out, with its color.
fn reactive_polyline_3d_line(
    points: &[[gaanim_animation::ScalarSource; 3]],
    colors: Option<&[[f32; 4]]>,
    color: PenikoColor,
    time: f64,
    signal: impl Fn(gaanim_core::ObjectId) -> Option<f64>,
) -> LineListData {
    let mut line = LineListData {
        points: Vec::with_capacity(points.len()),
        indices: None,
        strip: true,
        color,
        colors: colors.map(|_| Vec::with_capacity(points.len())),
        revealed: None,
    };
    for (index, [x, y, z]) in points.iter().enumerate() {
        let (Ok(x), Ok(y), Ok(z)) = (
            x.evaluate(time, &signal),
            y.evaluate(time, &signal),
            z.evaluate(time, &signal),
        ) else {
            continue;
        };
        if !(x.is_finite() && y.is_finite() && z.is_finite()) {
            continue;
        }
        line.points.push([x as f32, y as f32, z as f32]);
        if let (Some(kept), Some(colors)) = (line.colors.as_mut(), colors) {
            kept.push(colors[index]);
        }
    }
    line
}

/// Where source pixel `(x, y)` of the image or video `media` shows in the
/// scene as authored so far: through its media frame and world transform.
fn authored_media_point(builder: &SceneBuilder, media: ObjectId, x: f64, y: f64) -> Option<DVec3> {
    let frame = builder.media_frames.get(&media)?;
    let local = frame.geometry().1 * gaanim_core::kurbo::Point::new(x, y);
    let matrix = builder.get_world_transform(media).to_mat4();
    Some(matrix.transform_point3(DVec3::new(local.x, local.y, 0.0)))
}

/// The stretches of stepped time: each segment with a step rate, cut at its
/// stops so that the steps start again at each one.
fn step_windows(
    segments: &[crate::canvas::ops::Segment],
    metadata: &[SegmentMetadata],
) -> Vec<gaanim_timeline::timeline::StepWindow> {
    let mut windows = Vec::new();
    for (segment, metadata) in segments.iter().zip(metadata) {
        let Some(rate) = segment.step_rate else {
            continue;
        };
        let (start, end) = (metadata.start_time, metadata.end_time);
        let mut bounds = vec![start];
        bounds.extend(
            metadata
                .stops
                .iter()
                .map(|stop| stop.time)
                .filter(|&time| time > start && time < end),
        );
        bounds.push(end);
        windows.extend(
            bounds
                .windows(2)
                .filter(|pair| pair[1] > pair[0])
                .map(|pair| gaanim_timeline::timeline::StepWindow {
                    start: pair[0],
                    end: pair[1],
                    rate,
                }),
        );
    }
    windows
}

/// `entity` as a metaball: the center of its local bounds and half their
/// larger side, in scene coordinates.
fn metaball_of(
    world: &bevy::prelude::World,
    entity: bevy::prelude::Entity,
) -> Option<gaanim_objects::metaballs::Ball> {
    let matrix = gaanim_animation::updaters::entity_world_matrix(entity, world)?;
    let bounds = world.get::<gaanim_scene::LocalBounds>(entity)?.0;
    let center = matrix.transform_point3(bounds.center());
    let size = bounds.size();
    let scale = 0.5 * (matrix.x_axis.truncate().length() + matrix.y_axis.truncate().length());
    Some(gaanim_objects::metaballs::Ball {
        center: Point::new(center.x, center.y),
        radius: 0.5 * size.x.max(size.y) * scale,
    })
}

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

/// The color a 3D line of `spec` is drawn in: its stroke's, else its fill's
/// when that is solid, else a near black.
fn line_3d_color(spec: &ObjectSpec) -> PenikoColor {
    let solid = |brush: &gaanim_core::peniko::Brush| match brush {
        gaanim_core::peniko::Brush::Solid(color) => Some(*color),
        _ => None,
    };
    spec.stroke
        .as_ref()
        .and_then(|(brush, _)| solid(brush))
        .or_else(|| spec.fill.as_ref().and_then(solid))
        .unwrap_or(PenikoColor::from_rgb8(20, 20, 20))
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
        revealed: None,
    }
}

fn sampled_reactive_surface_3d(
    map: &gaanim_visualization::CoordinateMap3D,
    function: &ReactiveFunction,
    resolution: [usize; 2],
    color_map: Option<&gaanim_core::ColorMap>,
    material: Option<gaanim_scene::Material3D>,
    time: f64,
    values: &[(ObjectId, f64)],
) -> TriangleMeshData {
    let lookup = |logical| {
        values
            .iter()
            .find_map(|(id, value)| (*id == logical).then_some(*value))
    };
    // A function with no coordinates gives every height at once, in the
    // grid's order; f64::MAX marks a missing one.
    let mut heights = (function.coordinate_arity() == 0)
        .then(|| function.evaluate(&[], time, lookup).unwrap_or_default())
        .map(Vec::into_iter);
    let mesh = gaanim_visualization::sample_surface(map, resolution, |x, y| match &mut heights {
        Some(heights) => heights.next().filter(|z| *z != f64::MAX),
        None => function
            .evaluate(&[x, y], time, lookup)
            .ok()
            .map(|output| output[0]),
    })
    .unwrap_or(gaanim_visualization::SurfaceMesh {
        vertices: Vec::new(),
        indices: Vec::new(),
        values: Vec::new(),
    });
    let colors = surface_vertex_colors(&mesh.values, color_map, None);
    surface_sheet(mesh.vertices, mesh.indices, colors, material)
}

/// A sampled surface's mesh, drawn on both sides: lit meshes hide their
/// back faces, so a lit sheet gets a second copy wound the other way, whose
/// normals face the other side.
fn surface_sheet(
    mut vertices: Vec<[f32; 3]>,
    mut indices: Vec<u32>,
    mut colors: Vec<[f32; 4]>,
    material: Option<gaanim_scene::Material3D>,
) -> TriangleMeshData {
    if material.is_some() {
        let count = vertices.len() as u32;
        vertices.extend_from_within(..);
        colors.extend_from_within(..);
        let back = indices
            .as_chunks::<3>()
            .0
            .iter()
            .flat_map(|[a, b, c]| [a + count, c + count, b + count])
            .collect::<Vec<_>>();
        indices.extend(back);
    }
    TriangleMeshData {
        vertices,
        indices,
        normals: None,
        uvs: None,
        color: None,
        colors: Some(colors),
        material,
        surface: None,
        blend: None,
    }
}

/// One color per surface vertex: `values` mapped through `color_map` over
/// `range`, or over the span of the finite values; a non-finite value is
/// transparent. Without a map, a blue, green and red ramp.
fn surface_vertex_colors(
    values: &[f64],
    color_map: Option<&gaanim_core::ColorMap>,
    range: Option<(f64, f64)>,
) -> Vec<[f32; 4]> {
    let (minimum, maximum) = range.unwrap_or_else(|| {
        let finite = values
            .iter()
            .copied()
            .filter(|value| value.is_finite())
            .collect::<Vec<_>>();
        (
            finite.iter().copied().reduce(f64::min).unwrap_or(0.0),
            finite.iter().copied().reduce(f64::max).unwrap_or(1.0),
        )
    });
    let span = maximum - minimum;
    let span = if span.abs() < f64::EPSILON {
        f64::EPSILON
    } else {
        span
    };
    values
        .iter()
        .map(|value| {
            let t = if value.is_finite() {
                ((value - minimum) / span).clamp(0.0, 1.0)
            } else {
                0.0
            };
            if let Some(color) = color_map.and_then(|color_map| color_map.sample(t).ok()) {
                let [r, g, b, a] = color.components;
                return [r, g, b, if value.is_finite() { a } else { 0.0 }];
            }
            [
                (0x20 as f64 + t * 0xD0 as f64) as f32 / 255.0,
                (0x60 as f64 + (1.0 - (2.0 * t - 1.0).abs()) * 0x90 as f64) as f32 / 255.0,
                (0xD0 as f64 - t * 0x90 as f64) as f32 / 255.0,
                if value.is_finite() { 1.0 } else { 0.0 },
            ]
        })
        .collect()
}

/// Remembers the inputs a reactive mesh was last built from, so it is built
/// again only when they change.
#[derive(Default)]
struct LastInputs(std::sync::Mutex<Option<InputBits>>);

/// The time's and every input's bits, as [`LastInputs`] compares them.
type InputBits = (u64, Vec<(ObjectId, u64)>);

impl LastInputs {
    /// Whether `values` (and `time`, for a mesh that follows it) differ from
    /// the last call's.
    fn changed(&self, time: Option<f64>, values: &[(ObjectId, f64)]) -> bool {
        let snapshot = (
            time.map_or(0, f64::to_bits),
            values
                .iter()
                .map(|(id, value)| (*id, value.to_bits()))
                .collect::<Vec<_>>(),
        );
        let mut last = self.0.lock().expect("reactive mesh inputs poisoned");
        if last.as_ref() == Some(&snapshot) {
            return false;
        }
        *last = Some(snapshot);
        true
    }
}

/// A batched function's values when it is evaluated with no coordinates;
/// `None` when it fails.
fn evaluate_batch(
    function: &ReactiveFunction,
    time: f64,
    values: &[(ObjectId, f64)],
) -> Option<Vec<f64>> {
    function
        .evaluate(&[], time, |logical| {
            values
                .iter()
                .find_map(|(id, value)| (*id == logical).then_some(*value))
        })
        .ok()
}

#[allow(clippy::too_many_arguments)]
fn sampled_reactive_parametric_surface_3d(
    map: &gaanim_visualization::CoordinateMap3D,
    function: &ReactiveFunction,
    u_domain: (f64, f64),
    v_domain: (f64, f64),
    resolution: [usize; 2],
    color: &super::SurfaceColor,
    color_map: Option<&gaanim_core::ColorMap>,
    color_range: Option<(f64, f64)>,
    material: Option<gaanim_scene::Material3D>,
    time: f64,
    values: &[(ObjectId, f64)],
) -> TriangleMeshData {
    let signal = |logical| {
        values
            .iter()
            .find_map(|(id, value)| (*id == logical).then_some(*value))
    };
    // Batched functions return the whole grid at once.
    let points = (function.coordinate_arity() == 0)
        .then(|| evaluate_batch(function, time, values).unwrap_or_default());
    let colors = match color {
        super::SurfaceColor::Function(color) if color.coordinate_arity() == 0 => {
            Some(evaluate_batch(color, time, values).unwrap_or_default())
        }
        _ => None,
    };
    let mut index = 0;
    let mesh = gaanim_visualization::sample_parametric_surface(
        map,
        u_domain,
        v_domain,
        resolution,
        |u, v| {
            let vertex = index;
            index += 1;
            let point = match &points {
                Some(points) => {
                    let point = points.get(3 * vertex..3 * vertex + 3)?;
                    // A batch marks a missing point with f64::MAX.
                    if point.contains(&f64::MAX) {
                        return None;
                    }
                    [point[0], point[1], point[2]]
                }
                None => {
                    let point = function.evaluate(&[u, v], time, signal).ok()?;
                    [point[0], point[1], point[2]]
                }
            };
            let value = match color {
                super::SurfaceColor::Height => point[2],
                super::SurfaceColor::Values(values) => values[vertex],
                super::SurfaceColor::Function(color) => match &colors {
                    Some(colors) => colors.get(vertex).copied().unwrap_or(f64::NAN),
                    None => color
                        .evaluate(&[u, v], time, signal)
                        .map_or(f64::NAN, |value| value[0]),
                },
            };
            Some((point, value))
        },
    )
    .unwrap_or_default();
    let colors = surface_vertex_colors(&mesh.values, color_map, color_range);
    surface_sheet(mesh.vertices, mesh.indices, colors, material)
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
        revealed: None,
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
        CanvasEndpoint::Expression { x, y, z } => TrackingEndpoint::Expression {
            x: compile_tracking_scalar(x, id_map, states),
            y: compile_tracking_scalar(y, id_map, states),
            z: compile_tracking_scalar(z, id_map, states),
        },
        CanvasEndpoint::MediaPixel { object, x, y } => id_map
            .get(object)
            .and_then(|runtime| states.get(*runtime))
            .map(|state| TrackingEndpoint::MediaPixel {
                entity: state.entity,
                x: *x,
                y: *y,
            })
            .unwrap_or(TrackingEndpoint::Static(DVec3::ZERO)),
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
            alpha: compile_tracking_scalar(alpha, id_map, states),
            offset: *offset,
        },
        CanvasEndpoint::Rim {
            object,
            toward,
            radius,
        } => id_map
            .get(object)
            .and_then(|runtime| states.get(*runtime))
            .map(|state| TrackingEndpoint::Rim {
                entity: state.entity,
                toward: Box::new(compile_tracking_endpoint(toward, id_map, states)),
                radius: *radius,
            })
            .unwrap_or(TrackingEndpoint::Static(DVec3::ZERO)),
        CanvasEndpoint::Polar {
            origin,
            radius,
            angle,
        } => TrackingEndpoint::Polar {
            origin: Box::new(compile_tracking_endpoint(origin, id_map, states)),
            radius: compile_tracking_scalar(radius, id_map, states),
            angle: compile_tracking_scalar(angle, id_map, states),
        },
        CanvasEndpoint::Screen(point) => {
            TrackingEndpoint::Screen(Box::new(compile_tracking_endpoint(point, id_map, states)))
        }
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
        let source = fitted_text_typst_source(
            self.font_registry,
            &text.spec,
            width,
            text.font_size,
            &text.font_family,
            &text.math_font,
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

/// What a compilation leaves for later.
pub(crate) struct Compiled {
    /// The checkpoint asked for, if the compilation reached it.
    pub(crate) checkpoint: Option<CompileCheckpoint>,
    /// The compiled object that stands for each authored object.
    pub(crate) ids: HashMap<ObjectId, ObjectId>,
    /// The compiled clock after the last segment.
    pub(crate) end_time: f64,
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
    if let Some(stroke) = typst_text_stroke(style) {
        args.push(format!("stroke: {stroke}"));
    }
    if let Some(spacing) = style.letter_spacing {
        args.push(format!("tracking: {spacing}pt"));
    }
    if let Some(spacing) = style.word_spacing {
        args.push(format!("spacing: 100% + {spacing}pt"));
    }
    // Typst's baseline shift points down; a positive style baseline raises.
    if let Some(baseline) = style.baseline.filter(|value| *value != 0.0) {
        args.push(format!("baseline: {}pt", -baseline));
    }
    args
}

/// Outline width of text whose style names a stroke color but no width.
const DEFAULT_TEXT_STROKE_WIDTH: f64 = 0.02;

/// The Typst stroke of glyphs styled with a stroke color, or `None`.
fn typst_text_stroke(style: &StructuredTextStyle) -> Option<String> {
    let color = style.stroke_color?;
    let width = style
        .stroke_width
        .unwrap_or(DEFAULT_TEXT_STROKE_WIDTH)
        .max(0.0);
    (width > 0.0).then(|| format!("{width}pt + rgb(\"{}\")", color_to_hex(color)))
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
    if let Some(stroke) = typst_text_stroke(style) {
        args.push(format!("stroke: #({stroke})"));
    }
    if let Some(spacing) = style.letter_spacing {
        args.push(format!("tracking: #{spacing}pt"));
    }
    if let Some(spacing) = style.word_spacing {
        args.push(format!("spacing: #(100% + {spacing}pt)"));
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

    // The baseline shift is a text argument, so a raised or lowered part
    // stays in its line.
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
    content
}

/// Compile structured leaves without discarding the semantic style stack.
/// Math delimiters are tracked across part boundaries, so a styled nested
/// part may safely live inside a `$...$` expression.
fn structured_typst_content(spec: &StructuredTextSpec, _font_size: f64) -> String {
    leaves_typst_content(structured_text_leaves(spec))
}

/// The root style's arguments that `#set text` does not carry: they wrap
/// every leaf, as a part's do.
fn root_leaf_style(style: &StructuredTextStyle) -> StructuredTextStyle {
    StructuredTextStyle {
        stroke_color: style.stroke_color,
        stroke_width: style.stroke_width,
        decorations: style.decorations.clone(),
        baseline: style.baseline,
        ..StructuredTextStyle::default()
    }
}

/// The styled leaves of `spec` with its inline markup resolved: plain text
/// with `$` math delimiters and `\$` escapes.
fn structured_text_leaves(spec: &StructuredTextSpec) -> Vec<StyledTextLeaf> {
    let mut raw_leaves = Vec::new();
    collect_styled_text_leaves(
        &spec.content,
        &root_leaf_style(&spec.style),
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
    marked_leaves
}

/// The end of one visible unit of text: a character outside math or a
/// whole formula.
#[derive(Debug, Clone, Copy, PartialEq)]
struct CutPoint {
    leaf: usize,
    /// Byte offset in the leaf just after the unit.
    end: usize,
    /// Whether the unit is whitespace.
    space: bool,
}

/// Where a cut may end text: after each visible unit. A backslash escapes
/// only a `$`, as in `parse_inline_math`.
fn leaf_cut_points(leaves: &[StyledTextLeaf]) -> Vec<CutPoint> {
    let mut points = Vec::new();
    let mut in_math = false;
    let mut escaped = false;
    for (leaf, text) in leaves.iter().enumerate() {
        for (offset, character) in text.text.char_indices() {
            let end = offset + character.len_utf8();
            let point = CutPoint {
                leaf,
                end,
                space: character.is_whitespace(),
            };
            if std::mem::take(&mut escaped) && character == '$' {
                if !in_math {
                    points.push(point);
                }
                continue;
            }
            match character {
                '\\' => escaped = true,
                '$' => {
                    in_math = !in_math;
                    if !in_math {
                        points.push(point);
                    }
                }
                _ if !in_math => points.push(point),
                _ => {}
            }
        }
    }
    points
}

/// Unit counts at which a cut ends a word: before a space or at the end.
fn word_cuts(points: &[CutPoint]) -> Vec<usize> {
    (1..=points.len())
        .filter(|&keep| !points[keep - 1].space && points.get(keep).is_none_or(|next| next.space))
        .collect()
}

/// The first `keep` units of `leaves` (see [`leaf_cut_points`]) without
/// trailing spaces, followed by an ellipsis in the last leaf's style when
/// `ellipsis` is set.
fn cut_leaves(
    leaves: &[StyledTextLeaf],
    points: &[CutPoint],
    keep: usize,
    ellipsis: bool,
) -> Vec<StyledTextLeaf> {
    let mut kept = match keep.checked_sub(1).and_then(|index| points.get(index)) {
        Some(point) => {
            let mut kept = leaves[..=point.leaf].to_vec();
            kept[point.leaf].text.truncate(point.end);
            kept
        }
        None => Vec::new(),
    };
    while let Some(last) = kept.last_mut() {
        let trimmed = last.text.trim_end().len();
        last.text.truncate(trimmed);
        if !last.text.is_empty() {
            break;
        }
        kept.pop();
    }
    if ellipsis {
        let style = kept
            .last()
            .or(leaves.first())
            .map(|leaf| leaf.style.clone())
            .unwrap_or_default();
        kept.push(StyledTextLeaf {
            text: "…".to_owned(),
            style,
            content_boundary_before: false,
        });
    }
    kept
}

/// Typst markup of styled leaves.
fn leaves_typst_content(mut raw_leaves: Vec<StyledTextLeaf>) -> String {
    // Preserve every authored content boundary in math as ordinary Typst
    // whitespace. Typst remains responsible for deciding whether that
    // whitespace separates identifiers, participates in math syntax, or has
    // no visual effect. Boundary whitespace is normalized to one token.
    let mut token_boundary_before = vec![false; raw_leaves.len()];
    let mut in_math = false;
    let mut escaped = false;
    for index in 0..raw_leaves.len().saturating_sub(1) {
        for character in raw_leaves[index].text.chars() {
            // A backslash escapes only a `$`; before anything else it is a
            // literal backslash and the next character keeps its meaning.
            if std::mem::take(&mut escaped) && character == '$' {
                continue;
            }
            if character == '\\' {
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
            if std::mem::take(&mut escaped) {
                if character == '$' {
                    chunk.push('$');
                    continue;
                }
                chunk.push('\\');
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
    text_typst_source(
        spec,
        offered_width,
        font_size,
        font_family,
        color,
        &structured_typst_content(spec, font_size),
    )
}

/// [`structured_text_typst_source`] cut to the text's `max_lines`: unless
/// its overflow is `visible`, a text that composes into more lines keeps as
/// many characters as fit, followed by an ellipsis for `ellipsis`.
#[allow(clippy::too_many_arguments)]
pub(crate) fn fitted_text_typst_source(
    font_registry: &gaanim_text::font::FontRegistry,
    spec: &StructuredTextSpec,
    offered_width: Option<f64>,
    font_size: f64,
    font_family: &str,
    math_font: &str,
    color: gaanim_core::peniko::Color,
) -> String {
    let source = structured_text_typst_source(spec, offered_width, font_size, font_family, color);
    let Some(max_lines) = spec.flow.max_lines else {
        return source;
    };
    if matches!(spec.flow.overflow, StructuredTextOverflow::Visible) {
        return source;
    }
    let lines = |source: &str| {
        gaanim_text::prelude::measure_typst_lines(
            font_registry,
            source,
            false,
            Some(font_family),
            Some(math_font),
            Some(font_size),
            None,
            Some(gaanim_core::peniko::Brush::Solid(color)),
            StrokeBrush::transparent(),
        )
        .map_or(0, |(_, metrics)| metrics.line_count)
    };
    if lines(&source) <= max_lines {
        return source;
    }
    let leaves = structured_text_leaves(spec);
    let points = leaf_cut_points(&leaves);
    let ellipsis = matches!(spec.flow.overflow, StructuredTextOverflow::Ellipsis);
    let cut = |keep: usize| {
        text_typst_source(
            spec,
            offered_width,
            font_size,
            font_family,
            color,
            &leaves_typst_content(cut_leaves(&leaves, &points, keep, ellipsis)),
        )
    };
    // The longest cut that still fits, among `cuts` in increasing order;
    // the whole text does not fit.
    let longest = |cuts: &[usize]| {
        let (mut fits, mut overflows) = (None, cuts.len());
        let mut low = 0;
        while low < overflows {
            let middle = (low + overflows) / 2;
            if lines(&cut(cuts[middle])) <= max_lines {
                fits = Some(cuts[middle]);
                low = middle + 1;
            } else {
                overflows = middle;
            }
        }
        fits
    };
    // Whole words, as the lines broke; a first word too long for the lines
    // is cut between characters.
    let words = word_cuts(&points);
    let characters: Vec<usize> = (1..points.len()).collect();
    let keep = longest(&words)
        .or_else(|| longest(&characters))
        .unwrap_or(0);
    cut(keep)
}

/// Typst's default gap between the rows of an equation, in ems.
const MATH_ROW_GAP_EM: f64 = 0.65;

fn text_typst_source(
    spec: &StructuredTextSpec,
    offered_width: Option<f64>,
    font_size: f64,
    font_family: &str,
    color: gaanim_core::peniko::Color,
    content: &str,
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
    // Each line's box is one em, so baselines are `line_spacing` em apart,
    // as CSS `line-height` sets them. Typst's own edges (the cap height and
    // the baseline) would put them a cap height closer, overlapping lines in
    // fonts with tall ascenders.
    let leading = font_size * (spec.flow.line_spacing.max(0.1) - 1.0);
    // The rows of an equation broken with `\` stack by their ink instead,
    // so they keep Typst's own gap between rows at `line_spacing` 1, and
    // each extra em of spacing widens it as it does between text lines.
    let math_leading = font_size * (spec.flow.line_spacing.max(0.1) - 1.0 + MATH_ROW_GAP_EM);
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
    let spacing = spec
        .style
        .word_spacing
        .map(|spacing| format!(", spacing: 100% + {spacing}pt"))
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
    format!(
        "#set page(width: {page_width}, height: auto, margin: 0pt)\n\
         #set text({font}fill: rgb(\"{hex}\"), dir: {direction}, hyphenate: {}{lang}{weight}{italic}{tracking}{spacing}, top-edge: 0.8em, bottom-edge: -0.2em)\n\
         #set par(justify: {justify}, leading: {leading}pt)\n\
         #show math.equation.where(block: true): set par(leading: {math_leading}pt)\n\
         #align({alignment})[{content}]",
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

/// The animated layout changes that start at one instant (`advance=False`),
/// which run together: a later one animates from where the earlier ones
/// started to the layout they all leave, as if it were the only one.
#[derive(Default)]
struct LayoutInstant {
    time: f64,
    /// Each box child, root box and leaving child as it stood before the
    /// first change of the instant moved it.
    starts: HashMap<ObjectId, SpatialTransform>,
    /// Each root box's rest before the first change of the instant.
    root_rests: HashMap<ObjectId, SpatialTransform>,
    /// Children that entered, by source object, and whether they unfold.
    entering: HashMap<ObjectId, bool>,
    /// Children that left and whether they fold out.
    leaving: HashMap<ObjectId, bool>,
}

impl LayoutInstant {
    /// Whether a change at `time` starts at this instant. At a stop, what
    /// changes the frame at once comes [`crate::builder::STOP_REST`] later
    /// (a box made between two changes, for one), and the changes after it
    /// with it: they still start at the stop's instant.
    fn holds(&self, time: f64) -> bool {
        (self.time..=self.time + crate::builder::STOP_REST).contains(&time)
    }
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

impl CompiledLayoutTree {
    /// Whether node `id` is one of `sources` or lies inside one.
    fn within(&self, id: gaanim_layout::LayoutId, sources: &HashSet<ObjectId>) -> bool {
        let mut current = Some(id);
        while let Some(id) = current {
            if self
                .source_by_id
                .get(&id)
                .is_some_and(|source| sources.contains(source))
            {
                return true;
            }
            current = self.parent_by_id.get(&id).copied();
        }
        false
    }
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

/// Widen the margins of the layout child `id`, at any depth below `node`, by
/// `extra` on each side.
fn widen_layout_margins(
    node: &mut gaanim_layout::LayoutNode,
    id: gaanim_layout::LayoutId,
    extra: DVec2,
) -> bool {
    for child in &mut node.children {
        if child.node.id == id {
            let margin = &mut child.style.margin;
            margin.left += extra.x;
            margin.right += extra.x;
            margin.top += extra.y;
            margin.bottom += extra.y;
            return true;
        }
        if widen_layout_margins(&mut child.node, id, extra) {
            return true;
        }
    }
    false
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

/// How far, in scene units, a box may leave its parent or touch a sibling
/// before `check` reports it: rounding and antialiasing, not a layout mistake.
const LAYOUT_SLACK: f64 = 0.01;

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

/// The first words of the text `id` holds, itself or in its boxes, to
/// recognize it in a diagnostic.
fn layout_text_excerpt(tree: &CompiledLayoutTree, id: gaanim_layout::LayoutId) -> Option<String> {
    const CHARS: usize = 32;
    let mut pending = vec![id];
    while let Some(id) = pending.pop() {
        if let Some(text) = tree.texts.get(&id) {
            let text = text.spec.plain_text();
            let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
            if text.is_empty() {
                continue;
            }
            return Some(if text.chars().count() > CHARS {
                let head: String = text.chars().take(CHARS).collect();
                format!("{}…", head.trim_end())
            } else {
                text
            });
        }
        if let Some(children) = tree.children_by_id.get(&id) {
            pending.extend(children.iter().rev());
        }
    }
    None
}

fn outermost_layout_source(
    source: ObjectId,
    snapshots: &HashMap<ObjectId, LayoutTreeSnapshot>,
    object_specs: &HashMap<ObjectId, ObjectSpec>,
) -> ObjectId {
    let mut current = source;
    let mut visited = HashSet::from([source]);
    // A spec names the box that owns the object by the end of the script:
    // until that box's snapshot holds it, the object is a box of its own.
    while let Some(owner) = object_specs
        .get(&current)
        .and_then(|spec| spec.layout_owner)
        .filter(|owner| {
            snapshots
                .get(owner)
                .is_some_and(|snapshot| snapshot.members.iter().any(|member| member.id == current))
        })
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

    /// Rig the imported glTF model whose root compiled as `root`: the root
    /// names its nodes' entities, which its animations move, and each
    /// skinned primitive names its joints' entities.
    fn rig_gltf_model(
        builder: &mut SceneBuilder<'_, '_, '_>,
        root: ObjectId,
        rig: &crate::gltf::GltfRigSpec,
        id_map: &HashMap<ObjectId, ObjectId>,
    ) {
        let entity_of = |builder: &SceneBuilder<'_, '_, '_>, id: &ObjectId| {
            let compiled = id_map.get(id)?;
            Some(builder.states.get(*compiled)?.entity)
        };
        let Some(nodes) = rig
            .nodes
            .iter()
            .map(|id| entity_of(builder, id))
            .collect::<Option<Vec<Entity>>>()
        else {
            tracing::warn!("a glTF model lost some of its nodes; its animations are left out");
            return;
        };
        let compiled: Vec<ObjectId> = rig
            .nodes
            .iter()
            .filter_map(|id| id_map.get(id).copied())
            .collect();
        builder.gltf_poses.rigged(root, compiled);
        // Its primitives compiled before it, with the weights they were
        // declared with.
        let morphed = rig
            .morphs
            .iter()
            .filter_map(|(primitive, node)| {
                let primitive = *id_map.get(primitive)?;
                let declared = builder.morph_weights.get(&primitive)?;
                Some((primitive, *node, Arc::clone(declared)))
            })
            .collect();
        builder.gltf_poses.morphed(root, morphed);
        for (mesh, joints, inverse_bind) in &rig.skins {
            let Some(entity) = entity_of(builder, mesh) else {
                continue;
            };
            let joints: Option<Vec<Entity>> = joints
                .iter()
                .map(|node| nodes.get(*node).copied())
                .collect();
            let Some(joints) = joints else {
                continue;
            };
            builder.commands.entity(entity).insert((
                gaanim_scene::MeshSkin {
                    joints: joints.into(),
                    inverse_bind: Arc::clone(inverse_bind),
                },
                gaanim_scene::SkinPalette::default(),
            ));
        }
        let morphs: Vec<(Entity, usize)> = rig
            .morphs
            .iter()
            .filter_map(|(primitive, node)| Some((entity_of(builder, primitive)?, *node)))
            .collect();
        if let Some(state) = builder.states.get(root) {
            builder
                .commands
                .entity(state.entity)
                .insert(crate::gltf::GltfRig {
                    nodes: nodes.into(),
                    morphs: morphs.into(),
                });
        }
    }

    /// The mesh extruded from the outline `source` fills in the scene (a
    /// flat sheet at depth 0), and the center it belongs at; `None`, with a
    /// console warning, when the outline encloses no area.
    fn extruded_mesh(
        builder: &SceneBuilder<'_, '_, '_>,
        source: ObjectId,
        extrusion: &super::ExtrusionSpec,
    ) -> Option<(gaanim_scene::TriangleMeshData, gaanim_core::kurbo::Point)> {
        let outline = Self::mask_path_in_world(builder, source);
        let material = extrusion.material.unwrap_or_else(|| {
            // The source's own color, as its first filled leaf shows it.
            let color = Self::visual_leaf_ids(builder, source)
                .into_iter()
                .find_map(|id| match builder.states.get(id)?.fill.as_ref()? {
                    gaanim_core::peniko::Brush::Solid(color) => Some(*color),
                    _ => None,
                });
            color.map_or_else(
                gaanim_scene::Material3D::default,
                gaanim_scene::Material3D::matte,
            )
        });
        let built = if extrusion.depth > 0.0 {
            gaanim_objects::extrude::extrude(
                &outline,
                extrusion.depth,
                extrusion.bevel,
                extrusion.tolerance,
                material,
            )
        } else {
            gaanim_objects::extrude::flat(&outline, extrusion.tolerance, material)
        };
        match built {
            Ok(extruded) => Some((extruded.mesh, extruded.center)),
            Err(error) => {
                let factory = if extrusion.depth > 0.0 {
                    "extrude"
                } else {
                    "flat_3d"
                };
                gaanim_core::console::warn("3d", format!("{factory}: {error}"));
                None
            }
        }
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

    /// Values whose text each reactive number inside a layout keeps room
    /// for, by number: its explicit reserve and its source at the corners of
    /// the ranges its parameters take. A number outside a layout keeps none,
    /// so its row stays centered on what it shows.
    fn layout_readout_reserves(&self) -> HashMap<ObjectId, Vec<f64>> {
        // Corners grow as 2^n; beyond a few inputs the explicit reserve rules.
        const MAX_INPUTS: usize = 4;
        let state = self.state.lock().expect("canvas state poisoned");
        let mut numbers = HashSet::new();
        for spec in state.object_specs.values() {
            let spec = spec.lock().expect("object spec poisoned");
            if spec.layout_owner.is_none() {
                continue;
            }
            if matches!(spec.kind, SpawnKind::ReactiveReadout { .. }) {
                numbers.insert(spec.id);
            }
            if let Some(layout) = &spec.reactive_readout_layout {
                numbers.insert(layout.number);
            }
        }
        numbers
            .into_iter()
            .filter_map(|id| {
                let spec = state
                    .object_specs
                    .get(&id)?
                    .lock()
                    .expect("object spec poisoned");
                let SpawnKind::ReactiveReadout {
                    source, reserve, ..
                } = &spec.kind
                else {
                    return None;
                };
                let mut values: Vec<f64> = reserve.iter().copied().collect();
                let inputs = source.parameter_ids();
                let ranges = inputs
                    .iter()
                    .map(|input| state.parameter_ranges.get(input).copied())
                    .collect::<Option<Vec<_>>>()
                    .filter(|ranges| ranges.len() <= MAX_INPUTS);
                if let Some(ranges) = ranges {
                    for corner in 0..1usize << ranges.len() {
                        let value = source.evaluate(0.0, |logical| {
                            let index = inputs.iter().position(|input| *input == logical)?;
                            let (low, high) = ranges[index];
                            Some(if corner >> index & 1 == 1 { high } else { low })
                        });
                        values.extend(value.ok());
                    }
                }
                (!values.is_empty()).then_some((id, values))
            })
            .collect()
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
    ) -> Compiled {
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
                target: segment.target,
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
        timeline.talk_target = self.talk_target;
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
        builder.readout_reserves = self.layout_readout_reserves();
        builder.run_time_holds.probed = Arc::clone(&self.camera_hold_probes);
        builder.morph_mesh_ids = Self::morphed_ids(segments.iter().flat_map(|seg| &seg.ops));
        if let Ok(accent) = self.theme_color("accent") {
            builder.emphasis_color = accent;
        }
        if let Ok(foreground) = self.theme_color("foreground") {
            builder.emphasis_contrast = foreground;
        }
        let mut checkpoint = None;
        // Raw bounds for the canvas background (visual, no margin).
        let raw_bounds = self.frame.bounds();
        builder.draw_outline_width =
            raw_bounds.height() * crate::builder::WRITE_OUTLINE_PER_FRAME_HEIGHT;
        builder.typst_theme = self.typst_theme_prelude(
            text_config.roles[&gaanim_text::prelude::TextRole::Body].fill_color,
        );
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
            let timed = SEGMENT_TIMES.with_borrow(Option::is_some);
            let compile_started = timed.then(std::time::Instant::now);
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
            builder.timeline.origin = None;
            if let Some(started) = compile_started {
                let elapsed = started.elapsed();
                SEGMENT_TIMES.with_borrow_mut(|times| {
                    if let Some(times) = times {
                        times.push((seg.name.clone(), elapsed));
                    }
                });
            }
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
        // Holds on endpoints only the run time moves, which a probe of the
        // scene can find where they end.
        let run_time_holds = std::mem::take(&mut builder.run_time_holds.found);
        builder.commands.queue(move |world: &mut World| {
            if run_time_holds.is_empty() {
                world.remove_resource::<crate::builder::RunTimeCameraHolds>();
            } else {
                world.insert_resource(crate::builder::RunTimeCameraHolds(run_time_holds));
            }
        });
        let segment_paints: Vec<gaanim_renderer::pipeline::SegmentBackgroundPaint> = segments
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
        // A segment with its own motion blur turns the scene's into one
        // window per segment. A frame on a cut belongs to the segment it
        // enters, so a window stops short of the next segment. Whip pans
        // follow: they blur the frames that have no blur of their own.
        let own_windows: Vec<(f64, f64, gaanim_renderer::effects::MotionBlur)> =
            if segments.iter().any(|segment| segment.motion_blur.is_some()) {
                let last = segment_metadata.len().saturating_sub(1);
                segments
                    .iter()
                    .zip(&segment_metadata)
                    .enumerate()
                    .filter_map(|(index, (segment, metadata))| {
                        let blur = segment.motion_blur.unwrap_or(self.motion_blur)?;
                        let end = if index < last {
                            metadata.end_time - 1.0e-8
                        } else {
                            metadata.end_time
                        };
                        Some((metadata.start_time, end, blur))
                    })
                    .collect()
            } else {
                Vec::new()
            };
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
        let (
            polls,
            poll_session,
            poll_lobby,
            lobby_at,
            stop_gates,
            live_zones,
            rehearsal,
            teams,
            ask,
        ) = {
            let state = self.state.lock().expect("canvas state poisoned");
            (
                state.polls.clone(),
                state.poll_session.clone(),
                state.poll_lobby,
                state.poll_lobby_at,
                state.stop_gates.clone(),
                state.live_zones.clone(),
                state.rehearsal.clone(),
                state.poll_teams.clone(),
                state.poll_ask.clone(),
            )
        };
        let lobby_segment = lobby_at.map(|(segment, _)| segment);
        let at = |(segment, local): (usize, f64)| {
            segment_metadata
                .get(segment)
                .map(|metadata: &SegmentMetadata| metadata.start_time + local)
        };
        // The game begins at the segment that first shows the audience or
        // opens a poll.
        let game_segment = polls
            .iter()
            .map(|poll| poll.open.0)
            .chain(lobby_segment)
            .min()
            .and_then(|segment| segment_metadata.get(segment))
            .map(|metadata| metadata.id);
        // The first stop after `time`, before `until`: where a presentation
        // waits.
        let stop_after = |time: f64, until: f64| {
            segment_metadata
                .iter()
                .flat_map(|metadata| &metadata.stops)
                .map(|stop| stop.time)
                .filter(|stop| *stop > time + 1e-6 && *stop <= until + 1e-6)
                .fold(None, |first: Option<f64>, stop| {
                    Some(first.map_or(stop, |first| first.min(stop)))
                })
        };
        // The made-up audience previews and exports show: players join
        // until the room's first stop, and answer each poll until the stop
        // where a presentation would wait for them.
        let rehearsal = poll_session.is_some().then(|| {
            let room = lobby_at.and_then(|lobby| {
                let open = at(lobby)?;
                let end = segment_metadata.get(lobby.0)?.end_time;
                Some((open, stop_after(open, end).unwrap_or(end)))
            });
            let planned: Vec<gaanim_animation::rehearsal::PlannedPoll> = polls
                .iter()
                .filter_map(|poll| {
                    let open = at(poll.open)?;
                    let close = match poll.close {
                        Some(close) => at(close)?,
                        None => segment_metadata.get(poll.open.0)?.end_time,
                    }
                    .max(open);
                    let due = stop_after(open, close).unwrap_or(match &poll.quiz {
                        Some(quiz) => close.min(open + f64::from(quiz.time)),
                        None => close,
                    });
                    Some(gaanim_animation::rehearsal::PlannedPoll {
                        id: poll.id.clone(),
                        answers: poll.options.len(),
                        open,
                        due,
                        quiz: poll.quiz.as_ref().map(|quiz| {
                            let mask = quiz
                                .correct
                                .iter()
                                .fold(0u32, |mask, answer| mask | 1 << answer);
                            (mask, quiz.time, quiz.points)
                        }),
                        multiple: poll.multiple,
                        lean: poll.lean.clone(),
                    })
                })
                .collect();
            let teams = teams
                .as_ref()
                .map(|teams| gaanim_animation::rehearsal::RehearsalTeams {
                    count: teams.names.len(),
                    choose: teams.choose,
                });
            gaanim_animation::rehearsal::Rehearsal::plan(&rehearsal, room, &planned, teams)
        });
        builder.timeline.set_polls(
            polls
                .into_iter()
                .filter_map(|poll| {
                    let open = at(poll.open)?;
                    let close = match poll.close {
                        Some(close) => at(close)?,
                        None => segment_metadata.get(poll.open.0)?.end_time,
                    };
                    // The counts the rehearsal ends with stand for the poll
                    // where nothing plays it, such as the presenter's notes.
                    let preview = rehearsal
                        .as_ref()
                        .map(|rehearsal| {
                            let results = rehearsal.results_at(close.max(open));
                            results.live_counts(&poll.id, poll.options.len())
                        })
                        .unwrap_or_else(|| vec![0; poll.options.len()]);
                    Some(gaanim_timeline::timeline::TimelinePoll {
                        id: poll.id,
                        question: poll.question,
                        options: poll.options,
                        preview,
                        segment: segment_metadata.get(poll.open.0)?.id,
                        open,
                        close: close.max(open),
                        quiz: poll
                            .quiz
                            .map(|quiz| gaanim_timeline::timeline::TimelineQuiz {
                                correct: quiz.correct,
                                time: quiz.time,
                                points: quiz.points,
                                reveal: quiz.reveal.and_then(at),
                            }),
                        multiple: poll.multiple,
                        image: poll.image,
                    })
                })
                .collect(),
            poll_session.map(|session| gaanim_timeline::timeline::PollSessionInfo {
                relay: session.relay,
                code: session.code,
                lobby: poll_lobby,
                game_segment,
                teams,
                ask,
            }),
        );
        // Always set, so a reload without zones clears the previous ones.
        let stop_at = |time: f64| {
            segment_metadata
                .iter()
                .flat_map(|metadata| &metadata.stops)
                .any(|stop| (stop.time - time).abs() <= 1e-5)
        };
        let live_zones: Vec<gaanim_animation::live::LiveZone> = live_zones
            .into_iter()
            .filter_map(|record| {
                let mut zone = record.zone;
                zone.open = at(record.open)?;
                zone.close = match record.close {
                    Some(close) => at(close)?,
                    None => segment_metadata.get(record.open.0)?.end_time,
                }
                .max(zone.open);
                zone.stop_at_open = stop_at(zone.open);
                zone.stop_at_close = stop_at(zone.close);
                if let Some(names) = zone.names.as_mut() {
                    // Shape the glyphs now, so bundles draw names without fonts.
                    let body = &text_config.roles[&gaanim_text::prelude::TextRole::Body];
                    let family = names
                        .font
                        .clone()
                        .unwrap_or_else(|| body.font_family.clone());
                    names.glyphs = gaanim_animation::polls::atlas_characters()
                        .filter_map(|ch| {
                            let (path, advance) = gaanim_animation::polls::shape_glyph(
                                font_registry,
                                ch,
                                &family,
                                names.weight,
                                names.size,
                            )?;
                            Some(gaanim_animation::live::NameGlyph {
                                ch,
                                advance,
                                path: path.to_svg(),
                            })
                        })
                        .collect();
                }
                Some(zone)
            })
            .collect();
        builder
            .commands
            .insert_resource(gaanim_animation::live::LiveZones(live_zones));
        match rehearsal {
            Some(rehearsal) => builder.commands.insert_resource(rehearsal),
            None => builder
                .commands
                .remove_resource::<gaanim_animation::rehearsal::Rehearsal>(),
        }
        builder.timeline.set_stop_gates(
            stop_gates
                .into_iter()
                .filter_map(|(segment, local, until)| {
                    Some(gaanim_timeline::timeline::StopGate {
                        time: at((segment, local))?,
                        until,
                    })
                })
                .collect(),
        );
        // Camera constraints spawn once their switches, in whatever segment,
        // have set their windows on the compiled clock.
        for binding in std::mem::take(&mut builder.camera_bindings) {
            builder.commands.spawn(binding);
        }
        builder
            .timeline
            .set_step_windows(step_windows(&segments, &segment_metadata));
        builder.timeline.set_segments(segment_metadata);
        builder.timeline.talk_target = self.talk_target;

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
            // Untweened sources (e.g. sample drivers) keep free continuous
            // wheels. A resumed compile may find the windows a previous
            // revision inserted on a kept entity.
            if windows.is_empty() {
                builder
                    .commands
                    .entity(entity)
                    .remove::<gaanim_animation::RollingTweens>();
                continue;
            }
            windows.sort_by(|a, b| a.0.total_cmp(&b.0));
            builder
                .commands
                .entity(entity)
                .insert(gaanim_animation::RollingTweens(windows));
        }

        // Parameter-driven procedural layers integrate their signal from the
        // layer's start, so they need its whole schedule: each signal's tweens
        // in timeline order, gathered in one pass over the clips.
        let procedural_tracks = std::mem::take(&mut builder.procedural_signal_tracks);
        let mut signal_tweens: HashMap<ObjectId, Vec<(f64, f64, gaanim_animation::SignalTween)>> =
            procedural_tracks
                .iter()
                .map(|(_, signal, _, _)| (*signal, Vec::new()))
                .collect();
        if !signal_tweens.is_empty() {
            for clip in builder.timeline.clips.values() {
                let gaanim_timeline::clip::ClipPayload::Animation(animation) = &clip.payload else {
                    continue;
                };
                let gaanim_timeline::clip::PropertyLensSpec::SignalFloat { from, to } =
                    animation.lens
                else {
                    continue;
                };
                if let Some(tweens) = signal_tweens.get_mut(&animation.target) {
                    tweens.push((
                        clip.start,
                        clip.end(),
                        gaanim_animation::SignalTween {
                            start: clip.start,
                            duration: clip.duration,
                            from,
                            to,
                            rate: animation.rate_func.clone(),
                        },
                    ));
                }
            }
        }
        for (target, signal, start, base) in procedural_tracks {
            let tweens: Vec<gaanim_animation::SignalTween> = signal_tweens
                .get(&signal)
                .into_iter()
                .flatten()
                .filter(|(clip_start, clip_end, _)| *clip_start >= start || *clip_end > start)
                .map(|(_, _, tween)| tween.clone())
                .collect();
            let track = std::sync::Arc::new(gaanim_animation::SignalTrack::new(base, tweens));
            builder.commands.entity(target).queue(
                move |mut entity: bevy::prelude::EntityWorldMut| {
                    let Some(mut motion) = entity.get_mut::<gaanim_animation::ProceduralMotion>()
                    else {
                        return;
                    };
                    for scheduled in &mut motion.layers {
                        if scheduled.start.to_bits() != start.to_bits() {
                            continue;
                        }
                        for param in scheduled.layer.params_mut() {
                            if let gaanim_animation::LayerParam::Signal {
                                id, track: slot, ..
                            } = param
                                && *id == signal
                            {
                                *slot = Some(track.clone());
                            }
                        }
                    }
                },
            );
        }

        let mut connections = Vec::new();
        for (i, seg) in segments.iter().enumerate() {
            if let Some(prev) = seg.prev_segment
                && prev < i
                && i < scene_ids.len()
                && prev < scene_ids.len()
                && let Some(tr) = &seg.transition
            {
                // Drawables hidden when the outgoing segment ends (such as the
                // source of a finished magic move) cannot stand in for a key.
                let shown_at_end = |id: ObjectId| {
                    id_map
                        .get(&id)
                        .and_then(|actual| builder.states.get(*actual))
                        .is_some_and(|state| state.opacity > 1.0e-6)
                };
                let tr = Self::resolve_magic_move(
                    tr,
                    &segments[prev],
                    seg,
                    &object_specs,
                    &shown_at_end,
                );
                let runtime = Self::runtime_transition(&tr, &id_map, &builder);
                connections.push((scene_ids[prev], scene_ids[i], runtime));
            }
        }
        builder.timeline.connect_all(connections);

        // Shader effects read parameters any segment may declare, so they
        // resolve once every object exists; echo copies then carry them.
        let mut effects: Vec<_> = object_specs
            .iter()
            .filter(|(_, spec)| spec.shader_effect.is_some() || spec.outline.is_some())
            .filter_map(|(logical, spec)| {
                Some((
                    *id_map.get(logical)?,
                    spec.shader_effect.clone(),
                    spec.outline,
                ))
            })
            .collect();
        effects.sort_by_key(|(id, _, _)| *id);
        for (id, effect, outline) in effects {
            let Some(entity) = builder.states.get(id).map(|state| state.entity) else {
                continue;
            };
            let (mut passes, mut margin) =
                effect.map_or((Vec::new(), 0.0), |effect| (effect.passes, effect.margin));
            // An outline traces what the effect's passes leave, beyond
            // their reach.
            if let Some(outline) = outline
                && let Some(outline_passes) =
                    gaanim_renderer::object_effects::outline_passes(outline.width, outline.color)
            {
                passes.extend(outline_passes);
                margin += outline.width + OUTLINE_ANTIALIAS_MARGIN;
            }
            let post = Self::compiled_post_process(&builder, &id_map, passes, Vec::new());
            builder
                .commands
                .entity(entity)
                .insert(gaanim_renderer::object_effects::ShaderEffect { post, margin });
        }

        // A matte names another drawable, which then only draws as the
        // matte; both exist once every segment has been compiled.
        let mut mattes: Vec<_> = object_specs
            .iter()
            .filter_map(|(logical, spec)| {
                let (source, mode) = spec.matte?;
                Some((*id_map.get(logical)?, *id_map.get(&source)?, mode))
            })
            .collect();
        mattes.sort_by_key(|(id, _, _)| *id);
        for (id, source, mode) in mattes {
            let (Some(entity), Some(source)) = (
                builder.states.get(id).map(|state| state.entity),
                builder.states.get(source).map(|state| state.entity),
            ) else {
                continue;
            };
            builder
                .commands
                .entity(entity)
                .insert(gaanim_renderer::object_effects::Matte { source, mode });
            builder
                .commands
                .entity(source)
                .insert(gaanim_renderer::object_effects::MatteSource);
        }

        let mut glasses: Vec<_> = object_specs
            .iter()
            .filter_map(|(logical, spec)| Some((*id_map.get(logical)?, spec.glass?)))
            .collect();
        glasses.sort_by_key(|(id, _)| *id);
        for (id, glass) in glasses {
            if let Some(entity) = builder.states.get(id).map(|state| state.entity) {
                builder.commands.entity(entity).insert(glass);
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
        builder.commands.insert_resource({
            // The entities holding the signals that shader uniforms read.
            let mut parameters: Vec<(ObjectId, bevy::prelude::Entity)> = Vec::new();
            let paints = std::iter::once(&bg_paint).chain(
                segment_paints
                    .iter()
                    .filter_map(|segment| segment.paint.as_ref()),
            );
            for paint in paints {
                let gaanim_renderer::background::BackgroundPaint::Shader(shader) = paint else {
                    continue;
                };
                for logical in shader
                    .uniform_sources()
                    .iter()
                    .flat_map(|source| source.parameter_ids())
                {
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
            gaanim_renderer::pipeline::CanvasBackground {
                paint: bg_paint,
                segment_paints,
                pixel_size: self.frame.preview_pixel_size(),
                bounds: raw_bounds,
                parameters,
            }
        });
        let post_process = Self::compiled_post_process(
            &builder,
            &id_map,
            self.post_process.clone(),
            segment_post_processes,
        );
        builder.commands.insert_resource(post_process);
        match self.motion_blur {
            Some(blur) if own_windows.is_empty() => builder.commands.insert_resource(blur),
            _ => builder
                .commands
                .remove_resource::<gaanim_renderer::effects::MotionBlur>(),
        }
        // Clips change it from here; seeks restore it from snapshots.
        builder
            .commands
            .insert_resource(gaanim_scene::DepthOfField::default());
        let windows: Vec<_> = own_windows
            .into_iter()
            .chain(builder.motion_blur_windows.iter().copied())
            .collect();
        if windows.is_empty() {
            builder
                .commands
                .remove_resource::<gaanim_renderer::effects::MotionBlurWindows>();
        } else {
            builder
                .commands
                .insert_resource(gaanim_renderer::effects::MotionBlurWindows(windows));
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
        builder.commands.insert_resource(gaanim_scene::Lighting3D {
            z_up: self.world_up == DVec3::Z,
            ..self.lighting_3d
        });
        builder.commands.insert_resource(self.transparency_3d);
        Compiled {
            checkpoint,
            ids: id_map,
            end_time: builder.current_time,
        }
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
        let mut layout_instant: Option<LayoutInstant> = None;
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
        let camera_entries: HashSet<ObjectId> =
            Self::camera_view_entries(&seg.ops).into_iter().collect();
        fade_in_targets.extend(camera_entries.iter().copied());
        let track_origins = crate::canvas::tracks_script_lines();
        for (op_index, op) in seg.ops.iter().enumerate() {
            // The editor's animation timeline traces each clip to its `play`.
            builder.timeline.origin = (track_origins && matches!(op, Op::Play(_) | Op::Launch(_)))
                .then(|| crate::canvas::ops::op_origin(seg.id, op_index));
            // A stop shows what was made before it: a change made after it
            // at its instant, such as a cut, comes a moment later.
            if builder.rest_at == Some(builder.current_time)
                && Self::changes_the_frame_at_once(op, builder.current_time > scene_start + 1e-9)
            {
                builder.current_time += crate::builder::STOP_REST;
            }
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
                    let actual = Self::spawn_one(builder, &spec, id_map, frame_bounds, text_config);
                    if spec.hud {
                        Self::apply_hud(builder, actual.id);
                    }
                    // Placed in the 3D world: its parts draw in the 3D pass.
                    if let Some(in_3d) = spec.in_3d
                        && let Some(state) = builder.states.get(actual.id)
                    {
                        builder.commands.entity(state.entity).insert(in_3d);
                    }
                    // Facing the camera, whatever it is: an image, a video or
                    // a Lottie scene as well as a shape or a text.
                    if spec.billboard
                        && let Some(state) = builder.states.get(actual.id)
                    {
                        builder
                            .commands
                            .entity(state.entity)
                            .insert((gaanim_scene::Billboard, bevy::prelude::Transform::default()));
                        // A group turns about its pivot, which `move_to_3d`
                        // puts on its point, and anchors to its members' box.
                        let point = builder
                            .group_pivots
                            .get(&actual.id)
                            .copied()
                            .unwrap_or_default();
                        if spec.billboard_shift.is_some() || point != DVec3::ZERO {
                            let shift = spec.billboard_shift.unwrap_or_default();
                            builder
                                .commands
                                .entity(state.entity)
                                .insert(gaanim_scene::BillboardShift { point, ..shift });
                        }
                    }
                    if let Some(depth) = spec.parallax_depth
                        && let Some(state) = builder.states.get(actual.id)
                    {
                        builder
                            .commands
                            .entity(state.entity)
                            .insert(gaanim_scene::ParallaxLayer { depth });
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
                        // override a later fade-in that is the entry, including on
                        // a group member: one a write or reveal shows first shows
                        // from then on.
                        let fade_roots: HashSet<ObjectId> = fade_in_targets
                            .iter()
                            .filter(|id| {
                                camera_entries.contains(id)
                                    || Self::fade_in_is_entry(&seg.ops, op_index, **id)
                            })
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
                            lens_shift,
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
                            up: compile_tracking_endpoint(up, id_map, &builder.states),
                            lens_shift: lens_shift.as_ref().map(|shift| {
                                shift.each_ref().map(|value| {
                                    compile_tracking_scalar(value, id_map, &builder.states)
                                })
                            }),
                        },
                    };
                    let start = builder.current_time;
                    builder
                        .camera_bindings
                        .push(gaanim_animation::CameraBinding {
                            order: spec.order,
                            kind,
                            influence: compile_tracking_scalar(
                                &spec.influence,
                                id_map,
                                &builder.states,
                            ),
                            windows: spec
                                .enabled
                                .then_some(gaanim_animation::CameraBindingWindow {
                                    start,
                                    end: None,
                                })
                                .into_iter()
                                .collect(),
                        });
                }
                Op::SwitchCameraBinding { order, enabled } => {
                    let time = builder.current_time;
                    if let Some(binding) = builder
                        .camera_bindings
                        .iter_mut()
                        .find(|binding| binding.order == *order)
                    {
                        binding.switch(time, *enabled);
                    }
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
                            object_specs,
                        );
                        if let Some(mut remapped) = Self::remap_anim(anim, id_map, object_specs) {
                            Self::resolve_emphasis_color(builder, &mut remapped);
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
                                Self::resolve_camera_holds(
                                    builder,
                                    id_map,
                                    camera_position,
                                    camera_zoom,
                                    camera_target,
                                    camera_rotation,
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
                Op::TimeMaps(maps) => {
                    builder.pending_time_maps = maps.clone();
                }
                Op::PathModifier {
                    target,
                    kind,
                    params,
                } => {
                    use gaanim_animation::path_modifiers::{
                        ModifierParam, PathModifier, PathModifiers,
                    };
                    if let Some(&target) = id_map.get(target) {
                        let params = params
                            .iter()
                            .map(|id| {
                                id_map
                                    .get(id)
                                    .and_then(|actual| builder.states.get(*actual))
                                    .map_or(ModifierParam::Fixed(0.0), |state| {
                                        ModifierParam::Signal(state.entity)
                                    })
                            })
                            .collect();
                        let modifier = PathModifier {
                            kind: kind.clone(),
                            params,
                            from: builder.current_time,
                        };
                        // Every member with a path of its own is modified.
                        for id in builder.hierarchy_ids(target) {
                            let Some(state) = builder.states.get(id) else {
                                continue;
                            };
                            if !state.children.is_empty() || state.path.elements().is_empty() {
                                continue;
                            }
                            let entity = state.entity;
                            let added = modifier.clone();
                            let first = modifier.clone();
                            builder
                                .commands
                                .entity(entity)
                                .entry::<PathModifiers>()
                                .and_modify(move |mut modifiers| modifiers.stack.push(added))
                                .or_insert(PathModifiers::new(vec![first]));
                        }
                    }
                }
                Op::Play(anims) | Op::Launch(anims) => {
                    let time_maps = std::mem::take(&mut builder.pending_time_maps);
                    // A fade-in reveals its target first: it owns the root's
                    // opacity, which a move played with it would show at once.
                    let fades_in = |anim: &&AnimationBuilder| {
                        matches!(
                            anim.anim_type,
                            AnimationType::FadeIn | AnimationType::FadeInFrom { .. }
                        )
                    };
                    for anim in anims
                        .iter()
                        .filter(fades_in)
                        .chain(anims.iter().filter(|anim| !fades_in(anim)))
                    {
                        Self::reveal_deferred_on_play(
                            builder,
                            deferred_visibility,
                            revealed_deferred,
                            anim,
                            id_map,
                            object_specs,
                        );
                    }
                    let remapped: Vec<(AnimationBuilder, Option<Arc<gaanim_math::TimeMap>>)> =
                        anims
                            .iter()
                            .enumerate()
                            .filter_map(|(index, anim)| {
                                let mut remapped = Self::remap_anim(anim, id_map, object_specs)?;
                                Self::resolve_emphasis_color(builder, &mut remapped);
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
                                let map = time_maps.get(index).cloned().flatten();
                                Some(
                                    Self::split_group_paints(
                                        object_specs,
                                        id_map,
                                        anim.target,
                                        remapped,
                                    )
                                    .into_iter()
                                    .map(move |piece| (piece, map.clone())),
                                )
                            })
                            .flatten()
                            .collect();
                    let start = builder.current_time;
                    let max_duration = remapped
                        .iter()
                        .map(|(anim, map)| match map {
                            Some(map) => map.played(anim.duration.max(0.0)).max(0.0),
                            None => anim.delay.max(0.0) + anim.duration.max(0.0),
                        })
                        .fold(0.0, f64::max);
                    for (anim, map) in remapped {
                        // A warped composition's clips are authored from the
                        // play's start and placed by the timeline.
                        builder.timeline.set_time_map(map.map(|map| (start, map)));
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
                        builder.timeline.set_time_map(None);
                    }
                    Self::resolve_camera_holds(
                        builder,
                        id_map,
                        camera_position,
                        camera_zoom,
                        camera_target,
                        camera_rotation,
                    );
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
                                "wiggle" | "wave" => AnimationType::Wiggle {
                                    amplitude: crate::anim::DEFAULT_WIGGLE_AMPLITUDE,
                                    count: crate::anim::DEFAULT_WIGGLE_COUNT,
                                },
                                "highlight" => AnimationType::Indicate {
                                    color: Some(builder.emphasis_tint(target)),
                                    scale_factor: 1.0,
                                },
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
                            .fill(color)
                            .spawn();
                        let style = &text_config.roles[&gaanim_text::prelude::TextRole::Body];
                        let label_ref = builder.text(label, &style.font_family, style.size);
                        if let Some(state) = builder.states.get_mut(label_ref.id) {
                            state.transform.translation =
                                DVec3::new(bounds.center().x, y + side * 0.25, 0.0);
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
                        tracing::warn!(
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
                            tracing::warn!(
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
                            tracing::warn!(
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
                                        tracing::warn!(
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
                        let tex = mode == "tex";
                        let semantic_groups = if tex {
                            semantic_pairs
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
                                .collect()
                        } else {
                            Vec::new()
                        };
                        builder.play_transform_matching_grouped(
                            src,
                            dst,
                            if tex {
                                gaanim_math::matching::MatchingMode::Tex
                            } else {
                                gaanim_math::matching::MatchingMode::Shapes
                            },
                            semantic_groups,
                            *duration,
                            RateFunc::Smooth,
                        );
                    }
                    builder.current_time = end;
                }
                Op::LayoutTransition {
                    from_version,
                    to,
                    duration,
                    entering,
                    leaving,
                    fold,
                    resolve,
                    remeasure,
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
                    let Some(mut tree) = compile_layout_tree(
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
                    // `box.reflow()` measures each child (a nested box too)
                    // with the scale and rotation it has now, so a child scaled
                    // since it was placed gets room for its new size.
                    if let Some(within) = remeasure {
                        let inside = |id: gaanim_layout::LayoutId| {
                            let mut current = tree.parent_by_id.get(&id).copied();
                            while let Some(parent) = current {
                                if tree.source_by_id.get(&parent) == Some(within) {
                                    return true;
                                }
                                current = tree.parent_by_id.get(&parent).copied();
                            }
                            false
                        };
                        for (layout_id, source) in &tree.source_by_id {
                            if !inside(*layout_id) {
                                continue;
                            }
                            let current = id_map
                                .get(source)
                                .and_then(|actual| builder.states.get(*actual))
                                .map(|state| state.transform);
                            if let (Some(rest), Some(current)) =
                                (builder.layout_rests.get_mut(source), current)
                            {
                                rest.scale = current.scale;
                                rest.rotation = current.rotation;
                            }
                        }
                        let Some(remeasured) = compile_layout_tree(
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
                        tree = remeasured;
                    }
                    let root_id = tree.root.id;
                    let Some(container) = id_map.get(&root_source).copied() else {
                        continue;
                    };
                    // A child that comes from another box, still showing,
                    // slides from where it stands instead of fading in at its
                    // new place.
                    let entering = entering.filter(|source| {
                        !(builder.layout_rests.contains_key(source)
                            && id_map
                                .get(source)
                                .and_then(|id| builder.states.get(*id))
                                .is_some_and(|state| state.opacity > 0.0))
                    });
                    // Animated changes that start at the same instant run
                    // together (see `LayoutInstant`). A cut at that instant,
                    // such as a new box laying itself out, does not join
                    // them: it places its children at once, and later changes
                    // move them from there.
                    if layout_instant
                        .as_ref()
                        .is_some_and(|instant| !instant.holds(builder.current_time))
                    {
                        layout_instant = None;
                    }
                    let joins_instant = duration.is_some();
                    if let Some(instant) = layout_instant.as_mut()
                        && !joins_instant
                    {
                        for actual in tree
                            .source_by_id
                            .values()
                            .filter_map(|source| id_map.get(source))
                        {
                            instant.starts.remove(actual);
                        }
                        instant.root_rests.remove(&container);
                    }
                    if joins_instant && layout_instant.is_none() {
                        layout_instant = Some(LayoutInstant {
                            time: builder.current_time,
                            ..Default::default()
                        });
                    }
                    // The children entering at this instant, by source: this
                    // change's and those of the changes before it.
                    let entered: HashSet<ObjectId> = layout_instant
                        .iter()
                        .filter(|_| joins_instant)
                        .flat_map(|instant| instant.entering.keys().copied())
                        .chain(entering)
                        .collect();
                    // A child entering now appears in its place: only the
                    // others move, from where they were.
                    let entering_subtree = |id| tree.within(id, &entered);
                    let mut before: HashMap<ObjectId, SpatialTransform> = tree
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
                        LayoutWithin::Region(region) => region,
                    };
                    let font_registry = builder.font_registry;
                    let solve = |tree: &CompiledLayoutTree| {
                        let measurer = CompiledLayoutMeasure {
                            fixed: tree.fixed.clone(),
                            texts: tree.texts.clone(),
                            text_compositions: RefCell::default(),
                            text_candidates: RefCell::default(),
                            line_extents: RefCell::default(),
                            cap_heights: RefCell::default(),
                            natural_text_sizes: RefCell::default(),
                            font_registry,
                        };
                        gaanim_layout::resolve_layout(&tree.root, viewport, &measurer, &[])
                            .map(|resolved| (measurer, resolved))
                    };
                    let mut solved = solve(&tree);
                    // A nested box takes the room of its scaled size, as a
                    // scaled leaf does: Taffy sizes a box by its content, so
                    // its margins make up the difference.
                    let scaled_boxes: Vec<(gaanim_layout::LayoutId, DVec2)> = tree
                        .children_by_id
                        .keys()
                        .filter(|id| **id != tree.root.id)
                        .filter_map(|id| {
                            // Its scale when first placed, as for a leaf: the
                            // current one until then.
                            let source = tree.source_by_id.get(id)?;
                            let scale = match builder.layout_rests.get(source) {
                                Some(rest) => rest.scale,
                                None => builder.states.get(*id_map.get(source)?)?.transform.scale,
                            };
                            let scale = DVec2::new(scale.x, scale.y);
                            (scale.min_element() > 1.0e-6
                                && (scale - DVec2::ONE).abs().max_element() > 1.0e-9)
                                .then_some((*id, scale))
                        })
                        .collect();
                    if let Ok((_, first)) = &solved
                        && !scaled_boxes.is_empty()
                    {
                        let sizes: Vec<_> = scaled_boxes
                            .iter()
                            .filter_map(|(id, scale)| {
                                let bounds = first.boxes.get(id)?.bounds;
                                Some((
                                    *id,
                                    (*scale - DVec2::ONE)
                                        * DVec2::new(bounds.width(), bounds.height())
                                        * 0.5,
                                ))
                            })
                            .collect();
                        for (id, extra) in sizes {
                            widen_layout_margins(&mut tree.root, id, extra);
                        }
                        solved = solve(&tree);
                    }
                    let (measurer, resolved) = match solved {
                        Ok(solved) => solved,
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
                    // Content that leaves its box, or siblings that lie on top of
                    // each other: usually a box too small for what it holds.
                    for (container_id, child_ids) in &tree.children_by_id {
                        let (Some(container), Some(source)) = (
                            resolved.boxes.get(container_id).copied(),
                            tree.source_by_id.get(container_id),
                        ) else {
                            continue;
                        };
                        const OVERFLOW: &str = "content leaves its box";
                        const OVERLAP: &str = "overlaps the box before it";
                        const WRAPS: &str = "the row leaves less than a line";
                        // Where to look: the box, the text it holds, and the
                        // segment and time it is laid out at.
                        let at = format!(
                            "in segment \"{}\" at {:.1} s",
                            seg.name,
                            (builder.current_time - scene_start).max(0.0)
                        );
                        let name = |child: gaanim_layout::LayoutId| {
                            let path = layout_box_path(&tree, layout_snapshots, child);
                            match layout_text_excerpt(&tree, child) {
                                Some(text) => format!("{path} \"{text}\""),
                                None => path,
                            }
                        };
                        let kind = layout_snapshots
                            .get(source)
                            .map(|snapshot| snapshot.spec.kind.clone());
                        let mut reports = Vec::new();
                        let placed: Vec<_> = child_ids
                            .iter()
                            .filter(|child| {
                                !tree
                                    .item_style_by_id
                                    .get(child)
                                    .is_some_and(|style| style.absolute)
                            })
                            .filter_map(|child| {
                                let resolved = resolved.boxes.get(child)?;
                                Some((*child, resolved.bounds, resolved.clip.is_some()))
                            })
                            .collect();
                        // Clipped content is meant to leave.
                        let clips = id_map
                            .get(source)
                            .is_some_and(|actual| builder.clipped.contains(actual));
                        if container.clip.is_none() && !clips {
                            for (child, bounds, clipped) in &placed {
                                if *clipped {
                                    continue;
                                }
                                let x = (container.bounds.min.x - bounds.min.x)
                                    .max(bounds.max.x - container.bounds.max.x)
                                    .max(0.0);
                                let y = (container.bounds.min.y - bounds.min.y)
                                    .max(bounds.max.y - container.bounds.max.y)
                                    .max(0.0);
                                if x > LAYOUT_SLACK || y > LAYOUT_SLACK {
                                    let by = match (x > LAYOUT_SLACK, y > LAYOUT_SLACK) {
                                        (true, true) => format!("{x:.2} wide and {y:.2} tall"),
                                        (true, false) => format!("{x:.2} wide"),
                                        _ => format!("{y:.2} tall"),
                                    };
                                    reports.push(format!(
                                        "{} {at}: {OVERFLOW} by {by} scene units; give the box \
                                         more room, let it grow or shorten its content",
                                        name(*child)
                                    ));
                                }
                            }
                        }
                        if matches!(
                            kind,
                            Some(
                                gaanim_layout::LayoutNodeKind::Row { .. }
                                    | gaanim_layout::LayoutNodeKind::Column { .. }
                            )
                        ) {
                            for pair in placed.windows(2) {
                                let ((_, before, _), (child, after, _)) = (pair[0], pair[1]);
                                let x =
                                    before.max.x.min(after.max.x) - before.min.x.max(after.min.x);
                                let y =
                                    before.max.y.min(after.max.y) - before.min.y.max(after.min.y);
                                if x > LAYOUT_SLACK && y > LAYOUT_SLACK {
                                    reports.push(format!(
                                        "{} {at}: it {OVERLAP} by {x:.2} by {y:.2} scene units; \
                                         check the gap and the size of the boxes around it",
                                        name(child)
                                    ));
                                }
                            }
                        }
                        // A row that leaves a text less than one line breaks
                        // it, which a text that did not choose a width to wrap
                        // at rarely means: usually the row holds too much.
                        if matches!(kind, Some(gaanim_layout::LayoutNodeKind::Row { .. })) {
                            let wrapped: Vec<String> = placed
                                .iter()
                                .filter(|(child, bounds, _)| {
                                    tree.texts.get(child).is_some_and(|text| {
                                        matches!(text.spec.flow.wrap, StructuredTextWrap::Auto)
                                    }) && matches!(
                                        measurer.final_composition(*child, bounds.width()),
                                        Some(TextComposition::Wrapped { .. })
                                    )
                                })
                                .filter_map(|(child, ..)| layout_text_excerpt(&tree, *child))
                                .map(|text| format!("\"{text}\""))
                                .collect();
                            if let Some(first) = wrapped.first() {
                                let (which, breaks) = match wrapped.len() {
                                    1 => (format!("text {first}"), "it breaks"),
                                    count => (format!("{count} texts, from {first}"), "they break"),
                                };
                                reports.push(format!(
                                    "{} {at}: {WRAPS} to {which}, so {breaks} onto more lines; \
                                     give the row room (fewer items, a smaller size), or choose \
                                     the width a text wraps at with wrap=<width>",
                                    layout_box_path(&tree, layout_snapshots, *container_id),
                                ));
                            }
                        }
                        // Keyed by the box that holds them, so resolving it
                        // again replaces the earlier report.
                        let mut state = diagnostic_state.lock().expect("canvas state poisoned");
                        state.layout_diagnostics.retain(|(owner, message)| {
                            *owner != Some(*source)
                                || !(message.contains(OVERFLOW)
                                    || message.contains(OVERLAP)
                                    || message.contains(WRAPS))
                        });
                        state
                            .layout_diagnostics
                            .extend(reports.into_iter().map(|message| (Some(*source), message)));
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
                        // The natural composition of a text with a wrap width
                        // is already wrapped at it.
                        text.flow.wrap = match (text.flow.wrap, width.is_finite()) {
                            (StructuredTextWrap::Width(limit), true) => {
                                StructuredTextWrap::Width(limit.min(width))
                            }
                            (StructuredTextWrap::Width(limit), false) => {
                                StructuredTextWrap::Width(limit)
                            }
                            (StructuredTextWrap::Auto, true) => StructuredTextWrap::Width(width),
                            (StructuredTextWrap::Auto, false) => StructuredTextWrap::NoWrap,
                            (StructuredTextWrap::NoWrap, _) => continue,
                        };
                        let replacement = Self::spawn_one(
                            builder,
                            &materialized,
                            id_map,
                            frame_bounds,
                            text_config,
                        );
                        Self::carry_text_fill(builder, member, replacement.id);
                        // What follows the text, given before now, follows the
                        // copy from now on.
                        if let (Some(old), Some(copy)) = (
                            builder.states.get(member),
                            builder.states.get(replacement.id),
                        ) {
                            let (old, copy) = (old.entity, copy.entity);
                            let since = builder.current_time;
                            builder
                                .commands
                                .entity(old)
                                .insert(gaanim_animation::ReplacedBy { copy, since });
                        }
                        let entering_ancestor = tree.within(*layout_id, &entered);
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
                        // A child that leaves the box fading out (removed, or a
                        // text replaced by its rewrapped copy) keeps its parent.
                        // One detached, which stays in sight, leaves every group
                        // from now on, as one moving to another box changes
                        // parent from now on: [`SceneBuilder::reparent_at_cursor`]
                        // puts the change on the timeline, so the child still
                        // follows its former box before it.
                        let leaving_child = leaving
                            .as_ref()
                            .and_then(|member| id_map.get(member))
                            .copied();
                        let detached: Vec<_> = builder
                            .states
                            .get(parent)
                            .map(|state| {
                                state
                                    .children
                                    .iter()
                                    .copied()
                                    .filter(|child| {
                                        Some(*child) != leaving_child
                                            && !materialized_by_id.values().any(|id| id == child)
                                            && !text_crossfades.iter().any(|(old, ..)| old == child)
                                            && id_map
                                                .iter()
                                                .find(|(_, actual)| *actual == child)
                                                .is_some_and(|(source, _)| {
                                                    builder.layout_rests.contains_key(source)
                                                })
                                    })
                                    .collect()
                            })
                            .unwrap_or_default();
                        for child in detached {
                            builder.reparent_at_cursor(child, None, false);
                        }
                        for child in &children {
                            let current_parent =
                                builder.states.get(*child).and_then(|state| state.parent);
                            let source = tree
                                .source_by_id
                                .get(&gaanim_layout::LayoutId(child.as_raw()))
                                .copied();
                            let moved_in = current_parent != Some(parent)
                                && source.is_some_and(|source| {
                                    builder.layout_rests.contains_key(&source)
                                });
                            if moved_in {
                                // It slides from where it stands, measured anew
                                // in this box.
                                if let Some(source) = source {
                                    builder.layout_rests.remove(&source);
                                }
                                // A child fading in appears at its new place.
                                let fading_in = entering.is_some() && entering == source;
                                if let Some(transform) =
                                    builder.reparent_at_cursor(*child, Some(parent), true)
                                    && !fading_in
                                {
                                    before.insert(*child, transform);
                                }
                            } else if current_parent != Some(parent) {
                                // A child keeps its own scale and rotation in
                                // the box, as an element does in a CSS
                                // container: a scaled box scales it too.
                                let own = builder
                                    .states
                                    .get(*child)
                                    .map(|state| (state.transform.scale, state.transform.rotation));
                                builder.add_to_group(
                                    MobjectRef { id: parent },
                                    MobjectRef { id: *child },
                                );
                                if let Some((scale, rotation)) = own
                                    && let Some(state) = builder.states.get_mut(*child)
                                {
                                    state.transform.scale = scale;
                                    state.transform.rotation = rotation;
                                    let (entity, transform) = (state.entity, state.transform);
                                    Self::insert_transform_keeping_grow(builder, entity, transform);
                                }
                            }
                        }
                        if let Some(background) = background
                            && let Some(state) = builder.states.get_mut(background)
                        {
                            // Group attachment preserves world placement by default;
                            // a decoration instead uses its container's local box.
                            state.transform = SpatialTransform::identity();
                            let (entity, transform) = (state.entity, state.transform);
                            Self::insert_transform_keeping_grow(builder, entity, transform);
                        }
                        // The background shares the order of the box's first
                        // content and draws just before it: under that content,
                        // over whatever was created before the box. A child
                        // created before the others and removed or replaced
                        // later still counts (see `backdrop_orders`).
                        if let Some(background) = background {
                            let first = children
                                .iter()
                                .filter(|child| **child != background)
                                .flat_map(|child| builder.hierarchy_ids(*child))
                                .map(|id| id.index() as u64)
                                .chain(builder.backdrop_orders.get(&background).copied())
                                .min();
                            if let Some(first) = first {
                                builder.backdrop_orders.insert(background, first);
                            }
                            let z_index = background_source
                                .and_then(|source| object_specs.get(&source))
                                .map_or(0, |spec| spec.z_index);
                            let mut depth = 0;
                            let mut ancestor = tree.parent_by_id.get(parent_id);
                            while let Some(id) = ancestor {
                                depth += 1;
                                ancestor = tree.parent_by_id.get(id);
                            }
                            if let (Some(first), Some(state)) =
                                (first, builder.states.get(background))
                            {
                                builder.commands.entity(state.entity).insert((
                                    gaanim_scene::LayoutBackdrop { depth },
                                    RenderOrder {
                                        z_index,
                                        creation_order: first,
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
                    // Where the box stands now, moved by its own animations,
                    // or where it stood before the changes of this instant.
                    let root_before = layout_instant
                        .as_ref()
                        .filter(|_| joins_instant)
                        .and_then(|instant| instant.starts.get(&container))
                        .copied()
                        .or_else(|| builder.states.get(container).map(|state| state.transform));
                    if let Some(state) = builder.states.get_mut(container) {
                        state.bounds = local_root_bounds;
                        state.transform = SpatialTransform::identity();
                        state.transform.translation = root_center;
                        let entity = state.entity;
                        let root_transform = state.transform;
                        builder
                            .commands
                            .entity(entity)
                            .insert(LocalBounds(local_root_bounds));
                        Self::insert_transform_keeping_grow(builder, entity, root_transform);
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
                    // That replay wrote the box's base transform, which holds
                    // from the start of the timeline. A later reflow keeps the
                    // base it had and moves the box by the change of its
                    // placement from the cursor on, over the transition, so
                    // the box stands where it was until then and keeps the
                    // moves its own animations gave it.
                    let mut root_shift = None;
                    if let Some(placed) = builder.states.get(container).map(|state| state.transform)
                    {
                        let previous = builder.layout_root_rests.insert(container, placed);
                        let previous = layout_instant
                            .as_ref()
                            .filter(|_| joins_instant)
                            .and_then(|instant| instant.root_rests.get(&container))
                            .copied()
                            .or(previous);
                        if let (Some(previous), Some(before)) = (previous, root_before) {
                            if let Some(instant) = layout_instant.as_mut().filter(|_| joins_instant)
                            {
                                instant.root_rests.entry(container).or_insert(previous);
                                instant.starts.entry(container).or_insert(before);
                            }
                            let entity = builder.states.get(container).map(|state| state.entity);
                            if let Some(entity) = entity {
                                Self::insert_transform_keeping_grow(builder, entity, previous);
                            }
                            if let Some(state) = builder.states.get_mut(container) {
                                state.transform = before;
                            }
                            let delta = placed.translation - previous.translation;
                            if delta.length_squared() > 1e-18 {
                                root_shift = Some(delta);
                            }
                        }
                    }

                    // A box entering now takes its size at once, as it takes
                    // its place: only the others resize, from what they were.
                    let entering_now = |id| tree.within(id, &entered);
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
                            let span = if entering_now(*layout_id) {
                                0.0
                            } else {
                                duration.unwrap_or(0.0)
                            };
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
                        // The cell, in its parent box's own space: the solve
                        // puts the root where it hugs the viewport, not where
                        // the root was moved.
                        if matches!(item_style.fit, gaanim_layout::FitMode::Cover)
                            && let Some(parent_member) = materialized_by_id.get(parent_id)
                        {
                            let cell = target_box.bounds;
                            cover_clips.push((
                                *member,
                                *parent_member,
                                Rect::new(
                                    cell.min.x - parent_center.x,
                                    cell.min.y - parent_center.y,
                                    cell.max.x - parent_center.x,
                                    cell.max.y - parent_center.y,
                                ),
                            ));
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
                    // A text replaced by its rewrapped copy draws clipped as it
                    // did: the copy takes the clip its leaves had, given to it
                    // or to a box before this one adopted it.
                    for (old, new, ..) in &text_crossfades {
                        let placed = |id: ObjectId| {
                            let state = builder.states.get(id)?;
                            Some((state.entity, builder.get_world_transform(id).to_affine_2d()))
                        };
                        let old: Vec<_> = Self::visual_leaf_ids(builder, *old)
                            .into_iter()
                            .filter_map(placed)
                            .collect();
                        let new: Vec<_> = Self::visual_leaf_ids(builder, *new)
                            .into_iter()
                            .filter_map(placed)
                            .collect();
                        builder.commands.queue(move |world: &mut World| {
                            let Some((clip, old_world)) =
                                old.iter().find_map(|(entity, world_of)| {
                                    world
                                        .get::<gaanim_renderer::effects::ClipMask>(*entity)
                                        .map(|clip| (clip.clone(), *world_of))
                                })
                            else {
                                return;
                            };
                            for (entity, new_world) in new {
                                // A clip with sources finds its path again
                                // every frame; one without stays in place.
                                let mut clip = clip.clone();
                                clip.path.apply_affine(new_world.inverse() * old_world);
                                if let Ok(mut entity) = world.get_entity_mut(entity) {
                                    entity.insert(clip);
                                }
                            }
                        });
                    }
                    for (member, parent_member, cell) in cover_clips {
                        let parent_world =
                            builder.get_world_transform(parent_member).to_affine_2d();
                        let cell_path = cell.to_path(0.1);
                        for leaf in Self::visual_leaf_ids(builder, member) {
                            let Some(state) = builder.states.get(leaf) else {
                                continue;
                            };
                            let mut local_path = cell_path.clone();
                            local_path.apply_affine(
                                builder.get_world_transform(leaf).to_affine_2d().inverse()
                                    * parent_world,
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
                    // A child that unfolds starts with the sibling before it
                    // and one that folds goes with it, while their wipes open
                    // or close the slot.
                    let folding = *fold && duration.is_some();
                    let entering_actual = entering
                        .as_ref()
                        .and_then(|member| id_map.get(member))
                        .copied();
                    let leaving_actual = leaving
                        .as_ref()
                        .and_then(|member| id_map.get(member))
                        .copied();
                    // The changes before this one at this instant moved the
                    // children from where they stood then: this one moves
                    // them from there to where all of them leave them, and
                    // redoes the entries and exits they started against that
                    // layout, whatever order they came in.
                    let mut entries: Vec<(ObjectId, bool)> = entering_actual
                        .map(|child| (child, folding))
                        .into_iter()
                        .collect();
                    let mut exits: Vec<(ObjectId, bool)> = leaving_actual
                        .map(|child| (child, folding))
                        .into_iter()
                        .collect();
                    // Exits an earlier change of the instant moved, put back
                    // where they started.
                    let mut restarted = HashSet::new();
                    if let Some(instant) = layout_instant.as_mut().filter(|_| joins_instant) {
                        for (member, transform) in before.iter_mut() {
                            *transform = *instant.starts.entry(*member).or_insert(*transform);
                        }
                        if let Some(source) = entering {
                            instant.entering.insert(source, folding);
                        }
                        if let Some(child) = leaving_actual
                            && let Some(state) = builder.states.get(child)
                        {
                            instant.starts.entry(child).or_insert(state.transform);
                            instant.leaving.insert(child, folding);
                        }
                        let in_tree =
                            |member: ObjectId| materialized_by_id.values().any(|id| *id == member);
                        entries = instant
                            .entering
                            .iter()
                            .filter_map(|(source, unfolds)| {
                                let child = id_map.get(source).copied()?;
                                in_tree(child).then_some((child, *unfolds))
                            })
                            .collect();
                        exits = instant
                            .leaving
                            .iter()
                            .filter(|(child, _)| {
                                builder
                                    .states
                                    .get(**child)
                                    .and_then(|state| state.parent)
                                    .is_some_and(in_tree)
                            })
                            .map(|(child, folds)| (*child, *folds))
                            .collect();
                        entries.sort_unstable_by_key(|(child, _)| *child);
                        exits.sort_unstable_by_key(|(child, _)| *child);
                        for (child, _) in &exits {
                            if let (Some(start), Some(state)) =
                                (instant.starts.get(child), builder.states.get_mut(*child))
                                && state.transform.translation != start.translation
                            {
                                state.transform.translation = start.translation;
                                restarted.insert(*child);
                            }
                        }
                    }
                    let unfold_starts: Vec<(ObjectId, DVec3)> = entries
                        .iter()
                        .filter(|(_, unfolds)| *unfolds)
                        .map(|(child, _)| {
                            let shift = Self::fold_shift(
                                builder,
                                &tree,
                                &resolved,
                                &materialized_by_id,
                                &before,
                                &targets,
                                *child,
                                true,
                            );
                            (*child, shift)
                        })
                        .collect();
                    // A child that folds out travels with its neighbor; one
                    // that fades out stays where it started.
                    let exit_travels: Vec<(ObjectId, DVec3)> = exits
                        .iter()
                        .map(|(child, folds)| {
                            let shift = if *folds {
                                Self::fold_shift(
                                    builder,
                                    &tree,
                                    &resolved,
                                    &materialized_by_id,
                                    &before,
                                    &targets,
                                    *child,
                                    false,
                                )
                            } else {
                                DVec3::ZERO
                            };
                            (*child, shift)
                        })
                        .collect();
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
                    for (child, shift) in unfold_starts {
                        if let Some(state) = builder.states.get_mut(child) {
                            state.transform.translation += shift;
                            let (entity, transform) = (state.entity, state.transform);
                            Self::place_layout_member(builder, entity, transform);
                        }
                    }
                    let transition_duration = (*duration).unwrap_or(0.0);
                    // A member whose entry is a grow from scale zero, still to
                    // come, keeps its scale at zero until then: a scale cut
                    // here would show it in full before its turn.
                    let held_scale: HashMap<ObjectId, DVec3> = tree
                        .source_by_id
                        .iter()
                        .filter(|(_, source)| Self::grow_pending(&seg.ops, op_index, **source))
                        .filter_map(|(layout_id, _)| materialized_by_id.get(layout_id))
                        .filter_map(|member| {
                            targets
                                .iter()
                                .find(|(target, _)| target == member)
                                .map(|(target, to)| (*target, to.scale))
                        })
                        .collect();
                    // Zero-duration clips retain reversible cuts for immediate
                    // reflow; otherwise earlier seeks inherit final positions.
                    let mut animations: Vec<AnimationBuilder> = targets
                        .into_iter()
                        .flat_map(|(target, to)| {
                            let translate = AnimationBuilder {
                                target,
                                anim_type: AnimationType::TranslateTo { to: to.translation },
                                duration: transition_duration,
                                rate_func: RateFunc::Smooth,
                                delay: 0.0,
                            };
                            let scale =
                                (!held_scale.contains_key(&target)).then(|| AnimationBuilder {
                                    target,
                                    anim_type: AnimationType::ScaleTo { to: to.scale },
                                    duration: transition_duration,
                                    rate_func: RateFunc::Smooth,
                                    delay: 0.0,
                                });
                            std::iter::once(translate).chain(scale)
                        })
                        .collect();
                    if let Some(delta) = root_shift {
                        animations.push(AnimationBuilder {
                            target: container,
                            anim_type: AnimationType::TranslateBy { delta },
                            duration: transition_duration,
                            rate_func: RateFunc::Smooth,
                            delay: 0.0,
                        });
                    }
                    // The grow starts from the scale the layout gave.
                    for (member, scale) in &held_scale {
                        if let Some(state) = builder.states.get_mut(*member) {
                            state.transform.scale = *scale;
                        }
                    }
                    for (old, new, entry_pending, instant, own_entry) in text_crossfades {
                        // The copy shows the text as it is now: one hidden
                        // with `opacity(0)` or faded out stays hidden, and a
                        // later fade-in brings it back as the text would.
                        let shown = builder.states.get(old).map_or(1.0, |state| state.opacity);
                        if let Some(faded) = builder.faded_opacities.get(&old).copied() {
                            builder.faded_opacities.insert(new, faded);
                        }
                        if let Some(state) = builder.states.get_mut(new) {
                            state.opacity = shown;
                        }
                        if shown <= 0.0 && !entry_pending {
                            if let Some(state) = builder.states.get(new) {
                                builder.commands.entity(state.entity).insert(Opacity(0.0));
                            }
                            continue;
                        }
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
                    // A folding child shows at once, its wipe hiding it, and
                    // hides once its wipe has closed.
                    if let Some(entering) = entering_actual {
                        animations.push(AnimationBuilder {
                            target: entering,
                            anim_type: AnimationType::FadeIn,
                            duration: if folding { 0.0 } else { transition_duration },
                            rate_func: RateFunc::Smooth,
                            delay: 0.0,
                        });
                    }
                    if let Some(leaving) = leaving_actual {
                        animations.push(AnimationBuilder {
                            target: leaving,
                            anim_type: AnimationType::FadeOut,
                            duration: if folding { 0.0 } else { transition_duration },
                            rate_func: RateFunc::Smooth,
                            delay: if folding { transition_duration } else { 0.0 },
                        });
                    }
                    for (child, delta) in exit_travels {
                        if delta.length_squared() > 1e-18 || restarted.contains(&child) {
                            animations.push(AnimationBuilder {
                                target: child,
                                anim_type: AnimationType::TranslateBy { delta },
                                duration: transition_duration,
                                rate_func: RateFunc::Smooth,
                                delay: 0.0,
                            });
                        }
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
                                    from_up: Some(*camera_up),
                                },
                                rate_func: gaanim_math::RateFunc::Smooth,
                                delay: 0.0,
                                label: None,
                            },
                        ),
                    );
                    *camera_position = to_eye;
                    *camera_target = to_target;
                    *camera_up = *up;
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
                    // It orbits about the camera's up, the world's for a 3D
                    // state.
                    temp_cam.up = *camera_up;
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
                                    up: *camera_up,
                                    delta_yaw: *delta_yaw,
                                    delta_pitch: *delta_pitch,
                                    dolly: 1.0,
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
                    if mask.is_some() {
                        builder.clipped.insert(target);
                    } else {
                        builder.clipped.remove(&target);
                    }
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
                    // A view shows from the cursor where it was set until
                    // the one where it was removed.
                    let Some(view) = view else {
                        match builder.camera_views.remove(&screen) {
                            Some(mut shown) if shown.since < builder.current_time => {
                                shown.until = Some(builder.current_time);
                                builder.commands.entity(screen).insert(shown);
                            }
                            _ => {
                                builder
                                    .commands
                                    .entity(screen)
                                    .remove::<gaanim_renderer::effects::CameraView>();
                            }
                        }
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
                    let camera_3d = view.camera_3d.as_ref().map(|camera| {
                        gaanim_renderer::effects::ViewCamera3D {
                            eye: compile_tracking_endpoint(&camera.eye, id_map, &builder.states),
                            target: compile_tracking_endpoint(
                                &camera.target,
                                id_map,
                                &builder.states,
                            ),
                            up: camera.up,
                            fov_y: camera.fov_y,
                        }
                    });
                    let shown = gaanim_renderer::effects::CameraView {
                        source,
                        fit: view.fit,
                        background: view.background.clone(),
                        exclude,
                        zoom,
                        layers: view.layers.clone(),
                        since: builder.current_time,
                        until: None,
                        camera_3d,
                    };
                    builder.camera_views.insert(screen, shown.clone());
                    builder.commands.entity(screen).insert(shown);
                }
                Op::SetCameraSnapshot { target, screen } => {
                    let entity_of = |id: &ObjectId| {
                        let id = id_map.get(id).copied()?;
                        builder.states.get(id).map(|state| state.entity)
                    };
                    let (Some(snapshot), Some(screen)) = (entity_of(target), entity_of(screen))
                    else {
                        continue;
                    };
                    // Keyed by the authored drawable, which every world
                    // built from the scene shares.
                    builder.commands.entity(snapshot).insert(
                        gaanim_renderer::effects::CameraSnapshot {
                            screen,
                            at: builder.current_time,
                            key: *target,
                        },
                    );
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
                    anchor,
                    offset,
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
                        let mut local_point = inv * desired_point;
                        // An offset in scene units, in the space's own units.
                        if *offset != DVec2::ZERO {
                            let to_space = builder.get_world_transform(space).to_affine_2d();
                            if to_space.determinant().abs() > 1e-12 {
                                let [a, b, c, d, _, _] = to_space.inverse().as_coeffs();
                                local_point += Vec2::new(
                                    a * offset.x + c * offset.y,
                                    b * offset.x + d * offset.y,
                                );
                            }
                        }
                        if let Some(state) = builder.states.get_mut(target) {
                            state.transform.translation =
                                DVec3::new(local_point.x, local_point.y, local.z);
                            // The anchor's point of the box, not the origin,
                            // goes to the coordinate.
                            if let Some(anchor) = anchor {
                                let point = anchor.get_point(&state.bounds);
                                let placed =
                                    state.transform.to_affine_2d() * Point::new(point.x, point.y);
                                state.transform.translation.x += local_point.x - placed.x;
                                state.transform.translation.y += local_point.y - placed.y;
                            }
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
                        let target_entity = st.entity;
                        let mut layer = layer.clone();
                        let start = builder.current_time;
                        // Point parameter-driven numbers at their compiled
                        // signals; their tracks are filled in once every
                        // tween is scheduled.
                        for param in layer.params_mut() {
                            if let gaanim_animation::LayerParam::Signal { id, entity, .. } = param
                                && let Some(compiled) = id_map.get(id).copied()
                            {
                                *id = compiled;
                                *entity = builder.states.get(compiled).map(|state| state.entity);
                                let base =
                                    builder.float_signals.get(&compiled).copied().unwrap_or(0.0);
                                builder.procedural_signal_tracks.push((
                                    target_entity,
                                    compiled,
                                    start,
                                    base,
                                ));
                            }
                        }
                        builder.commands.entity(target_entity).queue(
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
                        builder.run_time_holds.updated.insert(target_id);
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

                Op::ParticleBurst { target, count } => {
                    if let Some(target) = id_map.get(target).copied() {
                        builder.schedule_particle_burst(target, builder.current_time, *count);
                    }
                }
                Op::AttachFalloff { target, effect } => {
                    if let Some(target_id) = id_map.get(target).copied() {
                        if let gaanim_animation::FalloffEffect::LookAt {
                            target: goal,
                            offset,
                        } = effect
                        {
                            let goal = match goal {
                                gaanim_animation::FalloffTarget::Object(object) => id_map
                                    .get(object)
                                    .map(|id| gaanim_animation::FalloffTarget::Object(*id)),
                                other => Some(*other),
                            };
                            if let Some(goal) = goal {
                                for member in Self::falloff_member_ids(builder, target_id) {
                                    builder
                                        .look_ats
                                        .entry(member)
                                        .or_default()
                                        .push((goal, *offset));
                                }
                            }
                        }
                        let mut effect = effect.clone();
                        effect.resolve_targets(&mut |object| {
                            id_map
                                .get(&object)
                                .and_then(|id| builder.states.get(*id))
                                .map(|state| state.entity)
                        });
                        let start = builder.current_time;
                        let members = Self::falloff_members(builder, target_id);
                        let count = members.len();
                        for (index, entity) in members.into_iter().enumerate() {
                            let effect = effect.clone();
                            builder.commands.entity(entity).queue(
                                move |mut entity: bevy::prelude::EntityWorldMut| {
                                    if let Some(mut drive) =
                                        entity.get_mut::<gaanim_animation::FalloffDrive>()
                                    {
                                        drive.index = index;
                                        drive.count = count;
                                        drive.push(effect, start);
                                    } else {
                                        let mut drive =
                                            gaanim_animation::FalloffDrive::new(index, count);
                                        drive.push(effect, start);
                                        entity.insert(drive);
                                    }
                                },
                            );
                        }
                    }
                }
                Op::ClearFalloff(target) => {
                    if let Some(target_id) = id_map.get(target).copied() {
                        let end = builder.current_time;
                        for member in Self::falloff_member_ids(builder, target_id) {
                            Self::keep_look_at_turn(builder, member);
                        }
                        for entity in Self::falloff_members(builder, target_id) {
                            builder.commands.entity(entity).queue(
                                move |mut entity: bevy::prelude::EntityWorldMut| {
                                    if let Some(mut drive) =
                                        entity.get_mut::<gaanim_animation::FalloffDrive>()
                                    {
                                        drive.stop_at(end);
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
                        builder.authored_follows.insert(
                            target_id,
                            crate::builder::AuthoredFollow {
                                endpoint: CanvasEndpoint::Entity(*source),
                                offset: *offset,
                                offset_space: gaanim_animation::FollowOffsetSpace::World,
                            },
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
                                since: builder.current_time,
                            });
                        builder.authored_follows.insert(
                            target_id,
                            crate::builder::AuthoredFollow {
                                endpoint: endpoint.clone(),
                                offset: *offset,
                                offset_space: *offset_space,
                            },
                        );
                    }
                }

                Op::Deform {
                    target,
                    lattice,
                    gate,
                    scale,
                } => {
                    if let Some(target_id) = id_map.get(target).copied()
                        && let Some(entity) =
                            builder.states.get(target_id).map(|state| state.entity)
                    {
                        let term = gaanim_animation::DeformTerm {
                            lattice: lattice.clone(),
                            gate: compile_tracking_scalar(gate, id_map, &builder.states),
                            scale: compile_tracking_scalar(scale, id_map, &builder.states),
                        };
                        let deformation = builder.deformations.entry(target_id).or_default();
                        deformation.terms.push(term);
                        let deformation = deformation.clone();
                        builder.commands.entity(entity).insert(deformation);
                    }
                }

                Op::AttachDelayedFollow {
                    target,
                    source,
                    delay,
                    offset,
                    offset_space,
                } => {
                    if let Some(target_id) = id_map.get(target).copied()
                        && let Some(source_id) = id_map.get(source).copied()
                        && let Some(target_state) = builder.states.get(target_id)
                        && builder.states.get(source_id).is_some()
                    {
                        builder.commands.entity(target_state.entity).insert(
                            gaanim_animation::DelayedFollow::new(
                                source_id,
                                *delay,
                                *offset,
                                *offset_space,
                            ),
                        );
                    }
                }

                Op::AttachTrackingLine {
                    target,
                    from,
                    to,
                    dashes,
                } => {
                    if let Some(target_id) = id_map.get(target).copied()
                        && let Some(st) = builder.states.get(target_id)
                    {
                        let mut line = TrackingLine::new(
                            compile_tracking_endpoint(from, id_map, &builder.states),
                            compile_tracking_endpoint(to, id_map, &builder.states),
                        );
                        if let Some((dash, gap)) = *dashes {
                            line = line.dashed(dash, gap);
                            // The dashes draw along the line, as those of a
                            // fixed dashed line do.
                            builder
                                .commands
                                .entity(st.entity)
                                .insert(gaanim_scene::PathRevealOrder::Sequential);
                        }
                        builder.commands.entity(st.entity).insert(line);
                    }
                }

                Op::AttachTrackingLine3D { target, from, to } => {
                    if let Some(target_id) = id_map.get(target).copied()
                        && let Some(st) = builder.states.get(target_id)
                    {
                        let entity = st.entity;
                        builder.commands.entity(entity).insert(
                            gaanim_animation::TrackingLine3D::new(
                                compile_tracking_endpoint(from, id_map, &builder.states),
                                compile_tracking_endpoint(to, id_map, &builder.states),
                            ),
                        );
                        builder.lines_3d.insert(target_id);
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
                        // A connector a box places, with fixed points, is a
                        // shape of its own: its points are local, and the
                        // box measures it from the outline they draw.
                        let fixed: Option<Vec<DVec3>> = points
                            .iter()
                            .map(|point| match point {
                                crate::canvas::ops::CanvasEndpoint::Static(position) => {
                                    Some(*position)
                                }
                                _ => None,
                            })
                            .collect();
                        let local_points = fixed.filter(|_| {
                            object_specs
                                .get(target)
                                .is_some_and(|spec| spec.layout_owner.is_some())
                        });
                        let entity = st.entity;
                        builder.commands.entity(entity).insert(
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
                                local: local_points.is_some(),
                            },
                        );
                        if let Some(local_points) = local_points {
                            let outline = gaanim_animation::updaters::connector_path(
                                &local_points,
                                *head_length,
                                *head_width,
                                *body_width,
                                *max_head_ratio,
                                1.0,
                            );
                            let rect = gaanim_core::kurbo::Shape::bounding_box(&outline);
                            if let Some(state) = builder.states.get_mut(target_id) {
                                state.bounds = Bounds3D::new_2d(rect.x0, rect.y0, rect.x1, rect.y1);
                            }
                        }
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
                    let compiled = Self::bounds_source_ids(builder, id_map, sources);
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
                Op::AttachMediaRegion {
                    target,
                    media,
                    rect,
                } => {
                    let media = id_map.get(media).copied();
                    let target = id_map.get(target).copied();
                    if let (Some(media), Some(target)) = (media, target)
                        && let Some(media_entity) = builder.states.get(media).map(|s| s.entity)
                    {
                        let [x0, y0, x1, y1] = *rect;
                        // Placed now as the system will place it, for the
                        // measurements made while the scene is authored.
                        let corners = [(x0, y0), (x1, y0), (x1, y1), (x0, y1)]
                            .map(|(x, y)| authored_media_point(builder, media, x, y));
                        if let Some(state) = builder.states.get_mut(target) {
                            let entity = state.entity;
                            let mut commands = builder.commands.entity(entity);
                            commands.insert(gaanim_animation::MediaRegion {
                                media: media_entity,
                                rect: gaanim_core::kurbo::Rect::new(x0, y0, x1, y1),
                            });
                            if let [Some(a), Some(b), Some(c), Some(d)] = corners {
                                let (path, bounds) =
                                    gaanim_animation::media_region_shape([a, b, c, d]);
                                let path = std::sync::Arc::new(path);
                                state.path = path.clone();
                                state.bounds = bounds;
                                commands.insert((
                                    gaanim_scene::Path2D(path),
                                    gaanim_scene::LocalBounds(bounds),
                                ));
                            }
                        }
                    }
                }
                Op::AttachLineFrames {
                    target,
                    sources,
                    padding,
                    corner_radius,
                } => {
                    let compiled = Self::bounds_source_ids(builder, id_map, sources);
                    if let Some(target_id) = id_map.get(target).copied()
                        && let Some(state) = builder.states.get(target_id)
                    {
                        builder.commands.entity(state.entity).insert(
                            gaanim_animation::SurroundingRect::lines(
                                compiled,
                                *padding,
                                *corner_radius,
                            ),
                        );
                    }
                }
                Op::AttachUnderline {
                    target,
                    sources,
                    gap,
                    overhang,
                } => {
                    let compiled = Self::bounds_source_ids(builder, id_map, sources);
                    if let Some(target_id) = id_map.get(target).copied()
                        && let Some(state) = builder.states.get(target_id)
                    {
                        builder.commands.entity(state.entity).insert(
                            gaanim_animation::SurroundingRect::underline(compiled, *gap, *overhang),
                        );
                    }
                }
                Op::AttachRays {
                    target,
                    sources,
                    gap,
                    rays,
                } => {
                    let compiled = Self::bounds_source_ids(builder, id_map, sources);
                    if let Some(target_id) = id_map.get(target).copied()
                        && let Some(state) = builder.states.get(target_id)
                    {
                        builder.commands.entity(state.entity).insert(
                            gaanim_animation::SurroundingRect::rays(compiled, *gap, *rays),
                        );
                    }
                }
                Op::AttachRoughNotation {
                    target,
                    sources,
                    padding,
                    notation,
                } => {
                    let compiled = Self::bounds_source_ids(builder, id_map, sources);
                    if let Some(target_id) = id_map.get(target).copied()
                        && let Some(state) = builder.states.get(target_id)
                    {
                        builder.commands.entity(state.entity).insert((
                            gaanim_animation::SurroundingRect::rough(compiled, *padding, *notation),
                            gaanim_scene::PathRevealOrder::Sequential,
                        ));
                    }
                }
                Op::AttachStrokeCycle {
                    target,
                    colors,
                    rate,
                } => {
                    if let Some(target_id) = id_map.get(target).copied()
                        && let Some(state) = builder.states.get(target_id)
                    {
                        builder.commands.entity(state.entity).insert(
                            gaanim_animation::StrokeCycle::new(
                                colors.clone(),
                                *rate,
                                builder.current_time,
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

                Op::AttachPollValue { target, value } => {
                    if let Some(target_id) = id_map.get(target).copied()
                        && let Some(target_st) = builder.states.get(target_id)
                    {
                        let entity = target_st.entity;
                        let value = value.clone();
                        builder.commands.queue(move |world: &mut World| {
                            if let Ok(mut target) = world.get_entity_mut(entity) {
                                target.insert(value);
                            }
                        });
                    }
                }

                Op::AttachCharacter {
                    target,
                    layers,
                    rig,
                } => {
                    let entity_of = |id: &ObjectId| {
                        id_map
                            .get(id)
                            .copied()
                            .and_then(|index| builder.states.get(index))
                            .map(|state| state.entity)
                    };
                    if let Some(entity) = entity_of(target) {
                        let mut rig = rig.clone();
                        rig.layers = layers.iter().filter_map(entity_of).collect();
                        builder.commands.queue(move |world: &mut World| {
                            if let Ok(mut target) = world.get_entity_mut(entity) {
                                target.insert(rig);
                            }
                        });
                    }
                }

                Op::CharacterExpress { target, expression } => {
                    if let Some(target_id) = id_map.get(target).copied()
                        && let Some(target_st) = builder.states.get(target_id)
                    {
                        let entity = target_st.entity;
                        let time = builder.current_time;
                        let expression = expression.clone();
                        builder.commands.queue(move |world: &mut World| {
                            if let Some(mut rig) =
                                world.get_mut::<gaanim_animation::characters::CharacterRig>(entity)
                            {
                                rig.schedule(time, expression);
                            }
                        });
                    }
                }

                Op::AttachPollBar { target, bar } => {
                    if let Some(target_id) = id_map.get(target).copied()
                        && let Some(target_st) = builder.states.get(target_id)
                    {
                        let entity = target_st.entity;
                        let bar = bar.clone();
                        builder.commands.queue(move |world: &mut World| {
                            if let Ok(mut target) = world.get_entity_mut(entity) {
                                target.insert(bar);
                            }
                        });
                    }
                }

                Op::AttachLiveText {
                    target,
                    source,
                    preview,
                    options,
                } => {
                    if let Some(target_id) = id_map.get(target).copied()
                        && let Some(target_st) = builder.states.get(target_id)
                    {
                        let entity = target_st.entity;
                        let body = &text_config.roles[&gaanim_text::prelude::TextRole::Body];
                        let live = gaanim_animation::polls::LiveText {
                            source: source.clone(),
                            preview: preview.clone(),
                            font_family: options
                                .font
                                .clone()
                                .unwrap_or_else(|| body.font_family.clone()),
                            font_weight: options.weight,
                            font_size: options.size.unwrap_or(body.size),
                            align: options.align,
                            atlas: Default::default(),
                            last: None,
                        };
                        builder.commands.queue(move |world: &mut World| {
                            if let Ok(mut target) = world.get_entity_mut(entity) {
                                target.insert(live);
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
                        && let Some(target_st) = builder.states.get(target_id)
                    {
                        let tracker = compile_tracking_scalar(tracker, id_map, &builder.states);
                        let center = Point::new(center.0, center.1);
                        let radius = *radius;
                        let start_angle = *start_angle;
                        let sweep_scale = *sweep_scale;
                        let sweep_offset = *sweep_offset;
                        let redraw = gaanim_animation::AlwaysRedrawRegen::new(move |world| {
                            let value = tracker.evaluate(world).unwrap_or(0.0);
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
                        && let Some(target_st) = builder.states.get(target_id)
                        && let Some(curve_st) = builder.states.get(curve_id)
                    {
                        let tracker = compile_tracking_scalar(tracker, id_map, &builder.states);
                        builder
                            .commands
                            .entity(target_st.entity)
                            .insert(PointOnCurve::new(curve_st.entity, tracker));
                    }
                }
                Op::AttachTangentOnCurve {
                    target,
                    curve,
                    tracker,
                } => {
                    if let Some(target_id) = id_map.get(target).copied()
                        && let Some(curve_id) = id_map.get(curve).copied()
                        && let Some(target_st) = builder.states.get(target_id)
                        && let Some(curve_st) = builder.states.get(curve_id)
                    {
                        let tracker = compile_tracking_scalar(tracker, id_map, &builder.states);
                        builder
                            .commands
                            .entity(target_st.entity)
                            .insert(TangentOnCurve::new(curve_st.entity, tracker));
                    }
                }
                Op::AttachNormalOnCurve {
                    target,
                    curve,
                    tracker,
                } => {
                    if let Some(target_id) = id_map.get(target).copied()
                        && let Some(curve_id) = id_map.get(curve).copied()
                        && let Some(target_st) = builder.states.get(target_id)
                        && let Some(curve_st) = builder.states.get(curve_id)
                    {
                        let tracker = compile_tracking_scalar(tracker, id_map, &builder.states);
                        builder
                            .commands
                            .entity(target_st.entity)
                            .insert(NormalOnCurve::new(curve_st.entity, tracker));
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
                        && let Some(target_st) = builder.states.get(target_id)
                        && let Some(curve_st) = builder.states.get(curve_id)
                    {
                        let tracker = compile_tracking_scalar(tracker, id_map, &builder.states);
                        builder
                            .commands
                            .entity(target_st.entity)
                            .insert(CurvatureOnCurve::new(curve_st.entity, tracker, *window));
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
                        builder.run_time_holds.updated.insert(target_id);
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
                                    revealed: None,
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
                                    revealed: None,
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
                                    revealed: None,
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
                                revealed: None,
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

    /// Resolves a keyed magic-move transition into a morph between the
    /// outermost keyed drawables of the outgoing and incoming segments.
    /// Drawables shown in both segments are the same object and do not pair.
    fn resolve_magic_move(
        transition: &gaanim_timeline::transition::TransitionType,
        from: &super::ops::Segment,
        to: &super::ops::Segment,
        object_specs: &HashMap<ObjectId, ObjectSpec>,
        shown_at_end: &dyn Fn(ObjectId) -> bool,
    ) -> gaanim_timeline::transition::TransitionType {
        use gaanim_timeline::transition::{MorphMapping, MorphProperty, TransitionType};
        match transition {
            TransitionType::Styled {
                base,
                easing,
                overlay,
                sound,
            } => TransitionType::Styled {
                base: Box::new(Self::resolve_magic_move(
                    base,
                    from,
                    to,
                    object_specs,
                    shown_at_end,
                )),
                easing: easing.clone(),
                overlay: overlay.clone(),
                sound: sound.clone(),
            },
            TransitionType::MagicMove { duration, key } => {
                let keyed = |segment: &super::ops::Segment, outgoing: bool| {
                    super::magic_move::keyed_outermost(
                        &segment.mobject_ids,
                        true,
                        |id| {
                            object_specs
                                .get(&id)
                                .map(super::magic_move::spec_children)
                                .unwrap_or_default()
                        },
                        |id| {
                            if outgoing && !shown_at_end(id) {
                                return Ok::<_, std::convert::Infallible>(None);
                            }
                            Ok(object_specs
                                .get(&id)
                                .and_then(|spec| super::magic_move::spec_key(spec, *key)))
                        },
                    )
                    .unwrap_or_else(|never| match never {})
                };
                let pairs = super::magic_move::pair_by_key(&keyed(from, true), &keyed(to, false));
                TransitionType::Morph {
                    duration: *duration,
                    mappings: pairs
                        .pairs
                        .into_iter()
                        .filter(|(source, target)| source != target)
                        .map(|(source, target)| MorphMapping {
                            source,
                            target,
                            property: MorphProperty::All,
                        })
                        .collect(),
                }
            }
            other => other.clone(),
        }
    }

    /// Rewrites authored morph pairs to runtime object ids, dropping pairs
    /// whose drawables were never compiled.
    fn runtime_transition(
        transition: &gaanim_timeline::transition::TransitionType,
        id_map: &HashMap<ObjectId, ObjectId>,
        builder: &SceneBuilder,
    ) -> gaanim_timeline::transition::TransitionType {
        use gaanim_timeline::transition::{MorphMapping, TransitionType};
        // Easing/overlay wrappers and drawable iris outlines carry authored ids too.
        match transition {
            TransitionType::Styled {
                base,
                easing,
                overlay,
                sound,
            } => {
                return TransitionType::Styled {
                    base: Box::new(Self::runtime_transition(base, id_map, builder)),
                    easing: easing.clone(),
                    overlay: overlay.clone(),
                    sound: sound.clone(),
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
            // Reactive uniforms read the signals of the entities compiled
            // for their parameters.
            TransitionType::Shader {
                duration,
                shader,
                uniforms,
            } if !uniforms.is_empty() => {
                return TransitionType::Shader {
                    duration: *duration,
                    shader: shader.clone(),
                    uniforms: uniforms
                        .iter()
                        .map(|uniform| gaanim_animation::ResolvedScalarSource {
                            source: uniform.source.clone(),
                            parameters: uniform
                                .source
                                .parameter_ids()
                                .into_iter()
                                .filter_map(|logical| {
                                    let actual = id_map.get(&logical)?;
                                    Some((logical, builder.states.get(*actual)?.entity))
                                })
                                .collect(),
                        })
                        .collect(),
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

    /// Authored ids of the drawables some transform morphs from or into:
    /// the 3D meshes among them keep their meshes for the morph.
    fn morphed_ids<'a>(ops: impl Iterator<Item = &'a Op>) -> HashSet<ObjectId> {
        let mut ids = HashSet::new();
        for op in ops {
            let anims: &[AnimationBuilder] = match op {
                Op::Animate { anim, active: true } => std::slice::from_ref(anim),
                Op::Play(anims) | Op::Launch(anims) => anims,
                _ => &[],
            };
            for anim in anims {
                if let AnimationType::Transform { target, .. }
                | AnimationType::ReplacementTransform { target } = &anim.anim_type
                {
                    ids.extend([anim.target, *target]);
                }
            }
        }
        ids
    }

    fn transform_targets(ops: &[Op]) -> std::collections::HashSet<ObjectId> {
        let mut targets = std::collections::HashSet::new();
        for op in ops {
            match op {
                Op::Animate { anim, active } if *active => match &anim.anim_type {
                    AnimationType::Transform { target, .. }
                    | AnimationType::ReplacementTransform { target }
                    | AnimationType::MagicMove { target, .. }
                    | AnimationType::CodeEdit { target, .. }
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
                            AnimationType::Transform { target, .. }
                            | AnimationType::ReplacementTransform { target }
                            | AnimationType::MagicMove { target, .. }
                            | AnimationType::CodeEdit { target, .. }
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
        object_specs: &HashMap<ObjectId, ObjectSpec>,
    ) {
        if !Self::animation_reveals_deferred(&anim.anim_type) {
            return;
        }

        let Some(&actual) = id_map.get(&anim.target) else {
            return;
        };

        let hierarchy: HashSet<ObjectId> = builder.hierarchy_ids(actual).into_iter().collect();
        let (kept_logical, pending): (Vec<ObjectId>, Vec<ObjectId>) = deferred_visibility
            .iter()
            .filter(|logical| {
                !revealed_deferred.contains(logical)
                    && id_map
                        .get(logical)
                        .is_some_and(|runtime| hierarchy.contains(runtime))
            })
            .copied()
            .partition(|logical| {
                // A descendant the author hid waits for its own entry.
                *logical != anim.target
                    && object_specs
                        .get(logical)
                        .is_some_and(|spec| spec.hidden_by_author)
            });
        let kept: HashSet<ObjectId> = kept_logical
            .iter()
            .filter_map(|logical| id_map.get(logical).copied())
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
                builder
                    .schedule_show_visuals_at(actual, builder.current_time + anim.delay.max(0.0));
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
            builder.schedule_show_descendants_at_except(
                actual,
                builder.current_time + anim.delay.max(0.0),
                &kept,
            );
            return;
        }

        builder.schedule_show_at_except(actual, builder.current_time + anim.delay.max(0.0), &kept);
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

    /// Whether `op` changes the frame at the instant it is made: a cut, an
    /// instant animation (a camera move without duration), a drawable
    /// declared mid-segment, which shows at once, or a binding; not a wait,
    /// animations that start where things are, nor what only says which
    /// segment holds a drawable (`reuse`, `persist`).
    fn changes_the_frame_at_once(op: &Op, mid_segment: bool) -> bool {
        let instant = |anim: &AnimationBuilder| anim.duration <= 0.0 && anim.delay <= 0.0;
        match op {
            Op::Immediate(_)
            | Op::Show(_)
            | Op::Hide(_)
            | Op::Remove(_)
            | Op::SetPropertyBinding { .. }
            | Op::ClearPropertyBinding { .. }
            | Op::SpawnCameraBinding(_)
            | Op::SwitchCameraBinding { .. } => true,
            Op::Spawn(_) => mid_segment,
            Op::Play(anims) | Op::Launch(anims) => anims.iter().any(instant),
            Op::Animate { anim, active } => *active && instant(anim),
            _ => false,
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
    /// `index` and is its entry, i.e. the object is still waiting for it.
    fn fade_in_pending(ops: &[Op], index: usize, target: ObjectId) -> bool {
        let fades_in = |op: &Op| {
            Self::played(op).iter().any(|anim| {
                anim.target == target
                    && matches!(
                        anim.anim_type,
                        AnimationType::FadeIn | AnimationType::FadeInFrom { .. }
                    )
            })
        };
        !ops[..index].iter().any(fades_in) && Self::fade_in_is_entry(ops, index, target)
    }

    /// Whether the first animation after `ops[index]` that shows `target`,
    /// or changes how opaque it is, is a fade-in: then that fade-in is its
    /// entry. A later fade-in of an object that a write, reveal, create or
    /// grow shows first is not.
    fn fade_in_is_entry(ops: &[Op], index: usize, target: ObjectId) -> bool {
        ops[index + 1..]
            .iter()
            .flat_map(Self::played)
            .find(|anim| anim.target == target && anim.anim_type.shows_target())
            .is_some_and(|anim| {
                matches!(
                    anim.anim_type,
                    AnimationType::FadeIn | AnimationType::FadeInFrom { .. }
                )
            })
    }

    /// The animations `op` plays.
    fn played(op: &Op) -> &[AnimationBuilder] {
        match op {
            Op::Animate { anim, active: true } => std::slice::from_ref(anim),
            Op::Play(anims) | Op::Launch(anims) => anims,
            _ => &[],
        }
    }

    /// Whether `target` has a grow from scale zero (or a spin in) still to
    /// come at `ops[index]` and none before it: its entry is pending.
    fn grow_pending(ops: &[Op], index: usize, target: ObjectId) -> bool {
        let grows = |op: &Op| {
            let anims: &[AnimationBuilder] = match op {
                Op::Animate { anim, active: true } => std::slice::from_ref(anim),
                Op::Play(anims) | Op::Launch(anims) => anims,
                _ => &[],
            };
            anims.iter().any(|anim| {
                anim.target == target
                    && matches!(
                        anim.anim_type,
                        AnimationType::GrowFromCenter
                            | AnimationType::GrowFromPoint { .. }
                            | AnimationType::GrowFromEdge { .. }
                            | AnimationType::SpinInFromNothing
                    )
            })
        };
        !ops[..index].iter().any(grows) && ops[index + 1..].iter().any(grows)
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

    /// The motion blur of a whip pan's frames: a full-frame shutter with
    /// enough samples that a fast move smears instead of stepping.
    fn whip_pan_blur() -> gaanim_renderer::effects::MotionBlur {
        gaanim_renderer::effects::MotionBlur::new(360.0, 24, None)
            .expect("the whip pan shutter is valid")
    }

    fn add_camera_lens(
        builder: &mut SceneBuilder,
        start: f64,
        anim: &AnimationBuilder,
        lens: gaanim_timeline::clip::PropertyLensSpec,
    ) {
        Self::add_camera_clip(builder, start, anim, lens);
    }

    fn add_camera_clip(
        builder: &mut SceneBuilder,
        start: f64,
        anim: &AnimationBuilder,
        lens: gaanim_timeline::clip::PropertyLensSpec,
    ) -> gaanim_timeline::clip::ClipId {
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
        )
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

    /// Give the dynamic camera clips just scheduled the pose the camera
    /// keeps once they end, from where their targets are as they end (as
    /// authored after the whole play, glTF parts where their animations put
    /// them), and continue the authored camera from it. Without it a
    /// `follow` or a dynamic `frame_to` cut back to the pose before the
    /// shot. A target an updater moves is where a probe of the scene found
    /// it (see [`Self::held_endpoint`]).
    fn resolve_camera_holds(
        builder: &mut SceneBuilder,
        id_map: &HashMap<ObjectId, ObjectId>,
        camera_position: &mut DVec3,
        camera_zoom: &mut f64,
        camera_target: &mut DVec3,
        camera_rotation: &mut gaanim_core::glam::DQuat,
    ) {
        use crate::builder::PendingCameraHoldKind;
        use gaanim_timeline::clip::{ClipPayload, PropertyLensSpec};
        for pending in std::mem::take(&mut builder.pending_camera_holds) {
            let mut target_held = None;
            let index = builder.run_time_holds.resolved;
            builder.run_time_holds.resolved += 1;
            // Where the endpoints are as the clip ends.
            let at = builder
                .timeline
                .clips
                .get(pending.clip)
                .map_or(builder.current_time, |clip| clip.end());
            let (position, zoom) = match &pending.kind {
                PendingCameraHoldKind::LookAt { eye, target, up } => {
                    let (Some((eye, _)), Some((target, _))) = (
                        Self::held_endpoint(builder, id_map, eye, true, at, (index, 0)),
                        Self::held_endpoint(builder, id_map, target, true, at, (index, 1)),
                    ) else {
                        continue;
                    };
                    if gaanim_math::Camera::validate_look_at(eye, target, *up).is_err() {
                        continue;
                    }
                    if let Some(clip) = builder.timeline.clips.get_mut(pending.clip)
                        && let ClipPayload::Animation(animation) = &mut clip.payload
                        && let PropertyLensSpec::CameraLookAtSource { hold, .. } =
                            &mut animation.lens
                    {
                        *hold = Some((eye, target));
                    }
                    let view = gaanim_core::glam::dcamera::rh::view::look_at_mat4(eye, target, *up);
                    *camera_position = eye;
                    *camera_target = target;
                    *camera_rotation = view.inverse().to_scale_rotation_translation().1;
                    continue;
                }
                PendingCameraHoldKind::Follow {
                    target,
                    from,
                    offset,
                    offset_space,
                    lag,
                    turn,
                    duration,
                    posed,
                } => {
                    let found = Self::held_endpoint(
                        builder,
                        id_map,
                        target,
                        posed.is_some(),
                        at,
                        (index, 0),
                    );
                    let Some((point, basis)) = found else {
                        continue;
                    };
                    let offset = match offset_space {
                        gaanim_animation::FollowOffsetSpace::World => *offset,
                        gaanim_animation::FollowOffsetSpace::Local => {
                            basis.transform_vector3(*offset)
                        }
                    };
                    // The lag the camera phase applies, at the clip's end.
                    let reached = if *lag <= f64::EPSILON {
                        1.0
                    } else {
                        1.0 - (-duration / lag).exp()
                    };
                    match posed {
                        // Turned where it stands: its eye stays, its target
                        // reaches the point.
                        Some(posed) if *turn => {
                            let target = posed.target.lerp(point + offset, reached);
                            if gaanim_math::Camera::validate_look_at(
                                posed.position,
                                target,
                                posed.up,
                            )
                            .is_ok()
                            {
                                let view = gaanim_core::glam::dcamera::rh::view::look_at_mat4(
                                    posed.position,
                                    target,
                                    posed.up,
                                );
                                *camera_rotation = view.inverse().to_scale_rotation_translation().1;
                            }
                            target_held = Some(target);
                            (posed.position, None)
                        }
                        Some(posed) => {
                            let moved = posed.moved_toward(point + offset, reached);
                            target_held = Some(moved.target);
                            (moved.position, None)
                        }
                        None => (from.lerp(point + offset, reached), None),
                    }
                }
                PendingCameraHoldKind::Pan { target } => {
                    let Some((point, _)) =
                        Self::held_endpoint(builder, id_map, target, false, at, (index, 0))
                    else {
                        continue;
                    };
                    (point, None)
                }
                PendingCameraHoldKind::Frame {
                    targets,
                    margins,
                    frame,
                } => {
                    let Some(bounds) = targets
                        .iter()
                        .filter_map(|target| {
                            builder.states.get(*target).map(|state| {
                                state.bounds.transform_2d(
                                    &builder.get_world_transform(*target).to_affine_2d(),
                                )
                            })
                        })
                        .reduce(|left, right| left.union(&right))
                    else {
                        continue;
                    };
                    let [top, right, bottom, left] = *margins;
                    let framed = Bounds3D::new_2d(
                        bounds.min.x - left,
                        bounds.min.y - bottom,
                        bounds.max.x + right,
                        bounds.max.y + top,
                    );
                    let zoom =
                        (frame.0 / framed.width().max(1.0)).min(frame.1 / framed.height().max(1.0));
                    (framed.center(), Some(zoom))
                }
            };
            if let Some(clip) = builder.timeline.clips.get_mut(pending.clip)
                && let ClipPayload::Animation(animation) = &mut clip.payload
            {
                match &mut animation.lens {
                    PropertyLensSpec::CameraFollowEndpoint {
                        hold, hold_target, ..
                    } => {
                        *hold = Some(position);
                        *hold_target = target_held;
                    }
                    PropertyLensSpec::CameraPositionSource { hold, .. } => *hold = Some(position),
                    PropertyLensSpec::CameraFrameDynamic { hold, .. } => {
                        *hold = zoom.map(|zoom| (position, zoom));
                    }
                    _ => {}
                }
            }
            match target_held {
                Some(target) => {
                    *camera_position = position;
                    *camera_target = target;
                }
                None => {
                    camera_position.x = position.x;
                    camera_position.y = position.y;
                }
            }
            if let Some(zoom) = zoom {
                *camera_zoom = zoom;
            }
        }
    }

    /// The value of `source` as authored at the cursor: Parameters at the
    /// value their animations last reached and the time at the cursor.
    fn authored_scalar(
        builder: &SceneBuilder,
        id_map: &HashMap<ObjectId, ObjectId>,
        source: &ScalarSource,
    ) -> Option<f64> {
        match source {
            ScalarSource::Constant(value) => Some(*value),
            ScalarSource::Signal(id) => builder.float_signals.get(id_map.get(id)?).copied(),
            ScalarSource::Time => Some(builder.current_time),
            ScalarSource::Function(function) => function
                .evaluate(&[], builder.current_time, |logical| {
                    builder.float_signals.get(id_map.get(&logical)?).copied()
                })
                .ok()?
                .first()
                .copied(),
        }
    }

    /// Where `endpoint` is as authored, with the world matrix its local
    /// offsets turn with: a drawable that follows an endpoint is where that
    /// endpoint is, and reactive points take the authored values of their
    /// sources. `None` for endpoints only the run time resolves (number
    /// lines, sources driven by updaters).
    /// Where camera hold `key` finds `endpoint` as its clip ends at `at`:
    /// as authored (see [`Self::authored_endpoint_at_depth`]), or
    /// where a probe of the scene found it, when an updater moves it. Such
    /// an endpoint is noted for the probe.
    fn held_endpoint(
        builder: &mut SceneBuilder,
        id_map: &HashMap<ObjectId, ObjectId>,
        endpoint: &CanvasEndpoint,
        in_3d: bool,
        at: f64,
        key: (usize, u8),
    ) -> Option<(DVec3, gaanim_core::glam::DMat4)> {
        let authored = Self::authored_endpoint_at_depth(builder, id_map, endpoint, 0, in_3d, at);
        if builder.run_time_holds.updated.is_empty()
            || !Self::moved_at_run_time(builder, id_map, endpoint, 0)
        {
            return authored;
        }
        builder
            .run_time_holds
            .found
            .push(crate::builder::RunTimeHold {
                index: key.0,
                slot: key.1,
                time: at,
                endpoint: compile_tracking_endpoint(endpoint, id_map, &builder.states),
            });
        match builder.run_time_holds.probed.get(&key) {
            Some(point) => Some((
                *point,
                authored.map_or(gaanim_core::glam::DMat4::IDENTITY, |(_, basis)| basis),
            )),
            None => authored,
        }
    }

    /// Whether an updater moves `endpoint`: a drawable it moves or one inside
    /// it, or an endpoint built on one.
    fn moved_at_run_time(
        builder: &SceneBuilder,
        id_map: &HashMap<ObjectId, ObjectId>,
        endpoint: &CanvasEndpoint,
        depth: usize,
    ) -> bool {
        if depth > 16 {
            return false;
        }
        let moved = |id: &ObjectId| {
            let Some(&runtime) = id_map.get(id) else {
                return false;
            };
            if let Some(follow) = builder.authored_follows.get(&runtime) {
                return Self::moved_at_run_time(builder, id_map, &follow.endpoint, depth + 1);
            }
            let mut current = Some(runtime);
            while let Some(object) = current {
                if builder.run_time_holds.updated.contains(&object) {
                    return true;
                }
                current = builder.states.get(object).and_then(|state| state.parent);
            }
            false
        };
        let built_on = |endpoint: &CanvasEndpoint| {
            Self::moved_at_run_time(builder, id_map, endpoint, depth + 1)
        };
        match endpoint {
            CanvasEndpoint::Entity(id)
            | CanvasEndpoint::LocalExpression { space: id, .. }
            | CanvasEndpoint::MediaPixel { object: id, .. } => moved(id),
            CanvasEndpoint::Anchor(anchor) => moved(&anchor.object),
            CanvasEndpoint::Offset { origin, .. } | CanvasEndpoint::Polar { origin, .. } => {
                built_on(origin)
            }
            CanvasEndpoint::Between { from, to, .. } => built_on(from) || built_on(to),
            CanvasEndpoint::Rim { object, toward, .. } => moved(object) || built_on(toward),
            _ => false,
        }
    }

    /// Where `endpoint` is as authored, with the world matrix its local
    /// offsets turn with: a drawable that follows an endpoint is where that
    /// endpoint is, and reactive points take the authored values of their
    /// sources. With `in_3d` drawables keep the depth they were moved to,
    /// which a camera at an angle sees, and parts of glTF models are where
    /// their animations put them at timeline second `at`. `None` for
    /// endpoints only the run time resolves (number lines, sources driven
    /// by updaters).
    fn authored_endpoint_at_depth(
        builder: &SceneBuilder,
        id_map: &HashMap<ObjectId, ObjectId>,
        endpoint: &CanvasEndpoint,
        depth: usize,
        in_3d: bool,
        at: f64,
    ) -> Option<(DVec3, gaanim_core::glam::DMat4)> {
        use gaanim_core::glam::DMat4;
        // Follows can chain; a cycle has no authored position.
        if depth > 16 {
            return None;
        }
        let scalar = |source: &ScalarSource| Self::authored_scalar(builder, id_map, source);
        let point = |endpoint: &CanvasEndpoint| {
            Self::authored_endpoint_at_depth(builder, id_map, endpoint, depth + 1, in_3d, at)
        };
        // A part of a glTF model is where its animation puts it then.
        let world = |runtime: ObjectId| {
            if builder.moved_by_gltf(runtime) {
                builder.world_matrix_3d_at(runtime, at)
            } else if in_3d {
                builder.world_matrix_3d(runtime)
            } else {
                builder.get_world_transform(runtime).to_mat4()
            }
        };
        match endpoint {
            CanvasEndpoint::Static(position) => Some((*position, DMat4::IDENTITY)),
            CanvasEndpoint::Entity(id) => {
                let runtime = *id_map.get(id)?;
                builder.states.get(runtime)?;
                if let Some(follow) = builder.authored_follows.get(&runtime) {
                    let (origin, basis) = point(&follow.endpoint)?;
                    let offset = match follow.offset_space {
                        gaanim_animation::FollowOffsetSpace::World => follow.offset,
                        gaanim_animation::FollowOffsetSpace::Local => {
                            basis.transform_vector3(follow.offset)
                        }
                    };
                    return Some((origin + offset, basis));
                }
                let matrix = world(runtime);
                Some((matrix.transform_point3(DVec3::ZERO), matrix))
            }
            CanvasEndpoint::Expression { x, y, z } => Some((
                DVec3::new(scalar(x)?, scalar(y)?, scalar(z)?),
                DMat4::IDENTITY,
            )),
            CanvasEndpoint::LocalExpression { space, x, y, z } => {
                let runtime = *id_map.get(space)?;
                builder.states.get(runtime)?;
                let matrix = world(runtime);
                let local = DVec3::new(scalar(x)?, scalar(y)?, scalar(z)?);
                Some((matrix.transform_point3(local), matrix))
            }
            CanvasEndpoint::Offset { origin, dx, dy } => {
                let (origin, _) = point(origin)?;
                Some((
                    origin + DVec3::new(scalar(dx)?, scalar(dy)?, 0.0),
                    DMat4::IDENTITY,
                ))
            }
            CanvasEndpoint::Between {
                from,
                to,
                alpha,
                offset,
            } => {
                let (from, _) = point(from)?;
                let (to, _) = point(to)?;
                Some((from.lerp(to, scalar(alpha)?) + *offset, DMat4::IDENTITY))
            }
            CanvasEndpoint::Polar {
                origin,
                radius,
                angle,
            } => {
                let (origin, _) = point(origin)?;
                let (radius, angle) = (scalar(radius)?, scalar(angle)?);
                let offset = DVec3::new(radius * angle.cos(), radius * angle.sin(), 0.0);
                Some((origin + offset, DMat4::IDENTITY))
            }
            CanvasEndpoint::Rim {
                object,
                toward,
                radius,
            } => {
                let runtime = *id_map.get(object)?;
                builder.states.get(runtime)?;
                let (toward, _) = point(toward)?;
                Some((
                    gaanim_animation::updaters::rim(world(runtime), toward, *radius),
                    DMat4::IDENTITY,
                ))
            }
            CanvasEndpoint::Anchor(anchor) => {
                let runtime = *id_map.get(&anchor.object)?;
                let bounds = builder.states.get(runtime)?.bounds;
                let local =
                    bounds.center() + bounds.size() * 0.5 * anchor.normalized + anchor.offset;
                let matrix = world(runtime);
                Some((matrix.transform_point3(local), matrix))
            }
            CanvasEndpoint::MediaPixel { object, x, y } => {
                let runtime = *id_map.get(object)?;
                let matrix = builder.get_world_transform(runtime).to_mat4();
                Some((authored_media_point(builder, runtime, *x, *y)?, matrix))
            }
            _ => None,
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
            AnimationType::CameraState { from, to, orbit } => {
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
                        orbit: *orbit,
                    },
                );
                if let gaanim_animation::CameraStateSource::Concrete(pose) = to {
                    builder.lens_shift = pose.lens_shift;
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
                let clip = Self::add_camera_clip(
                    builder,
                    start,
                    anim,
                    PropertyLensSpec::CameraPositionSource {
                        from: *camera_position,
                        to: compile_tracking_endpoint(target, id_map, &builder.states),
                        hold: None,
                    },
                );
                builder
                    .pending_camera_holds
                    .push(crate::builder::PendingCameraHold {
                        clip,
                        kind: crate::builder::PendingCameraHoldKind::Pan {
                            target: Box::new(target.clone()),
                        },
                    });
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
                    let clip = Self::add_camera_clip(
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
                            hold: None,
                        },
                    );
                    builder
                        .pending_camera_holds
                        .push(crate::builder::PendingCameraHold {
                            clip,
                            kind: crate::builder::PendingCameraHoldKind::Frame {
                                targets: targets.clone(),
                                margins: *margins,
                                frame: (frame_bounds.width(), frame_bounds.height()),
                            },
                        });
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
                turn,
            } => {
                let clip = Self::add_camera_clip(
                    builder,
                    start,
                    anim,
                    PropertyLensSpec::CameraFollowEndpoint {
                        target: compile_tracking_endpoint(target, id_map, &builder.states),
                        from: *camera_position,
                        offset: *offset,
                        offset_space: *offset_space,
                        lag: *lag,
                        turn: *turn,
                        hold: None,
                        hold_target: None,
                    },
                );
                // The camera as authored here, which a camera at an angle
                // keeps the framing of.
                let mut posed = gaanim_math::Camera::ortho_2d(1, 1);
                posed.position = *camera_position;
                posed.rotation = *camera_rotation;
                posed.target = *camera_target;
                posed.up = *camera_up;
                if let Some((fov_y, near, far)) = *camera_fov {
                    posed.projection = gaanim_math::Projection::Perspective { fov_y, near, far };
                }
                builder
                    .pending_camera_holds
                    .push(crate::builder::PendingCameraHold {
                        clip,
                        kind: crate::builder::PendingCameraHoldKind::Follow {
                            target: Box::new(target.clone()),
                            from: *camera_position,
                            offset: *offset,
                            offset_space: *offset_space,
                            lag: *lag,
                            turn: *turn,
                            duration: anim.duration.max(0.0),
                            posed: (!posed.is_flat()).then(|| Box::new(posed)),
                        },
                    });
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
                        from_up: Some(*camera_up),
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
                // Without an eye the camera turns where it stands.
                let eye = eye
                    .clone()
                    .unwrap_or(CanvasEndpoint::Static(*camera_position));
                let compiled_eye = compile_tracking_endpoint(&eye, id_map, &builder.states);
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
                let clip = Self::add_camera_clip(
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
                        from_up: Some(*camera_up),
                        hold: None,
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
                } else {
                    // Moving endpoints: the camera ends aimed where they
                    // end as authored, and later moves start from there.
                    builder
                        .pending_camera_holds
                        .push(crate::builder::PendingCameraHold {
                            clip,
                            kind: crate::builder::PendingCameraHoldKind::LookAt {
                                eye: Box::new(eye),
                                target: Box::new(target.clone()),
                                up: *up,
                            },
                        });
                }
            }
            AnimationType::CameraOrbit {
                delta_yaw,
                delta_pitch,
                dolly,
            } => {
                let mut camera = gaanim_math::Camera::ortho_2d(1, 1);
                camera.position = *camera_position;
                camera.target = *camera_target;
                camera.rotation = *camera_rotation;
                // The pose the next camera move starts from, orbited about the
                // same up axis as the lens orbits during playback.
                camera.up = *camera_up;
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
                        dolly: *dolly,
                    },
                );
                *camera_position = *camera_target + (camera.position - *camera_target) * *dolly;
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
                let from_pose = camera_fov.map(|(fov_y, near, far)| gaanim_math::CameraPose {
                    position: *camera_position,
                    rotation: *camera_rotation,
                    target: *camera_target,
                    up: *camera_up,
                    projection: gaanim_math::Projection::Perspective { fov_y, near, far },
                    lens_shift: builder.lens_shift,
                });
                Self::add_camera_lens(
                    builder,
                    start,
                    anim,
                    PropertyLensSpec::CameraOrthographic {
                        from: *camera_zoom,
                        to: *zoom,
                        from_pose,
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
                        from_lens: *camera_fov,
                        from_lens_shift: builder.lens_shift,
                    },
                );
                builder.lens_shift = DVec2::ZERO;
                *camera_position = DVec3::ZERO;
                *camera_rotation = gaanim_core::glam::DQuat::IDENTITY;
                *camera_target = DVec3::ZERO;
                *camera_up = DVec3::Y;
                *camera_zoom = 1.0;
                *camera_fov = None;
            }
            AnimationType::CameraFollowPath { route, follow } => {
                let Some(path) = builder.world_route(*route) else {
                    return;
                };
                let path = crate::builder::route_portion(path, *follow);
                if gaanim_math::get_path_length(&path) <= 0.0 {
                    gaanim_core::console::warn("camera", "follow_path: the route has no length");
                    return;
                }
                Self::add_camera_lens(
                    builder,
                    start,
                    anim,
                    PropertyLensSpec::CameraPathFollow {
                        path: path.clone(),
                        z: camera_position.z,
                        orient: follow.orient,
                        route: Default::default(),
                    },
                );
                let end = gaanim_math::get_point_at_alpha(&path, 1.0);
                *camera_position = DVec3::new(end.x, end.y, camera_position.z);
                if let Some(offset) = follow.orient {
                    *camera_rotation = gaanim_core::glam::DQuat::from_rotation_z(
                        gaanim_math::path_tangent_angle(&path, 1.0) + offset,
                    );
                }
            }
            AnimationType::CameraWhipPan { target, blur } => {
                let to = compile_tracking_endpoint(target, id_map, &builder.states);
                match &to {
                    gaanim_animation::TrackingEndpoint::Static(point) => {
                        let to = DVec3::new(point.x, point.y, camera_position.z);
                        Self::add_camera_lens(
                            builder,
                            start,
                            anim,
                            PropertyLensSpec::CameraPosition {
                                from: *camera_position,
                                to,
                            },
                        );
                        *camera_position = to;
                    }
                    _ => Self::add_camera_lens(
                        builder,
                        start,
                        anim,
                        PropertyLensSpec::CameraPositionSource {
                            from: *camera_position,
                            to,
                            hold: None,
                        },
                    ),
                }
                if *blur {
                    let begin = builder.timeline.map_time(start + anim.delay.max(0.0));
                    let end = builder
                        .timeline
                        .map_time(start + anim.delay.max(0.0) + anim.duration.max(0.0));
                    builder
                        .motion_blur_windows
                        .push((begin, end, Self::whip_pan_blur()));
                }
            }
            AnimationType::CameraDepthOfField {
                focus,
                aperture,
                max_blur,
            } => {
                let from = builder.depth_of_field;
                let to = gaanim_scene::DepthOfField {
                    focus: focus.unwrap_or(from.focus),
                    aperture: aperture.unwrap_or(from.aperture),
                    max_blur: max_blur.unwrap_or(from.max_blur),
                };
                Self::add_camera_lens(
                    builder,
                    start,
                    anim,
                    PropertyLensSpec::CameraDepthOfField {
                        from: [from.focus, from.aperture, from.max_blur],
                        to: [to.focus, to.aperture, to.max_blur],
                    },
                );
                builder.depth_of_field = to;
            }
            AnimationType::CameraLensShift { to } => {
                let from = builder.lens_shift;
                Self::add_camera_lens(
                    builder,
                    start,
                    anim,
                    PropertyLensSpec::CameraLensShift { from, to: *to },
                );
                builder.lens_shift = *to;
            }
            AnimationType::CameraDollyZoom { factor } => {
                let Some((fov, near, far)) = *camera_fov else {
                    gaanim_core::console::warn(
                        "camera",
                        "dolly_zoom needs a perspective camera; call camera.perspective(...) first",
                    );
                    return;
                };
                if (*camera_position - *camera_target).length() <= f64::EPSILON {
                    gaanim_core::console::warn(
                        "camera",
                        "dolly_zoom needs the camera away from its look_at target",
                    );
                    return;
                }
                Self::add_camera_lens(
                    builder,
                    start,
                    anim,
                    PropertyLensSpec::CameraDollyZoom {
                        from_position: *camera_position,
                        target: *camera_target,
                        from_fov: fov,
                        near,
                        far,
                        factor: *factor,
                    },
                );
                let (position, fov) =
                    gaanim_math::dolly_zoom(*camera_position, *camera_target, fov, *factor, 1.0);
                *camera_position = position;
                *camera_fov = Some((fov, near, far));
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
                for (primitive, _, _) in &mut properties.morph {
                    *primitive = *id_map.get(primitive)?;
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
            AnimationType::Broadcast {
                source,
                ghosts,
                max_scale,
                lag,
            } => AnimationType::Broadcast {
                source: *id_map.get(source)?,
                ghosts: ghosts
                    .iter()
                    .filter_map(|ghost| id_map.get(ghost).copied())
                    .collect(),
                max_scale: *max_scale,
                lag: *lag,
            },
            AnimationType::CameraFollow { target } => AnimationType::CameraFollow {
                target: *id_map.get(target)?,
            },
            AnimationType::CameraFollowPath { route, follow } => AnimationType::CameraFollowPath {
                route: *id_map.get(route)?,
                follow: *follow,
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
            AnimationType::Transform { target, .. } => AnimationType::Transform {
                target: *id_map.get(target)?,
                pairs: super::magic_move::transform_pairs(anim.target, *target, object_specs)
                    .into_iter()
                    .filter_map(|(source, target)| {
                        Some((*id_map.get(&source)?, *id_map.get(&target)?))
                    })
                    .collect(),
            },
            AnimationType::ReplacementTransform { target } => AnimationType::ReplacementTransform {
                target: *id_map.get(target)?,
            },
            AnimationType::MagicMove {
                target,
                pairs,
                unmatched,
            } => AnimationType::MagicMove {
                target: *id_map.get(target)?,
                pairs: pairs
                    .iter()
                    .filter_map(|(source, target)| {
                        Some((*id_map.get(source)?, *id_map.get(target)?))
                    })
                    .collect(),
                unmatched: *unmatched,
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
            AnimationType::CodeEdit { target, from, to } => AnimationType::CodeEdit {
                target: *id_map.get(target)?,
                from: from.clone(),
                to: to.clone(),
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

    /// Split the fill and stroke of a property animation on a group into one
    /// animation per member, the members the group's immediate `fill` and
    /// `stroke` restyle: a group draws nothing of its own, so on the group
    /// these channels changed nothing. The group keeps its other channels.
    fn split_group_paints(
        object_specs: &HashMap<ObjectId, ObjectSpec>,
        id_map: &HashMap<ObjectId, ObjectId>,
        authored_target: ObjectId,
        anim: AnimationBuilder,
    ) -> Vec<AnimationBuilder> {
        let AnimationType::Properties(properties) = &anim.anim_type else {
            return vec![anim];
        };
        let paints = properties.fill.is_some()
            || properties.stroke_color.is_some()
            || properties.stroke_width.is_some()
            || properties.visible_color.is_some();
        let is_group = matches!(
            object_specs.get(&authored_target).map(|spec| &spec.kind),
            Some(SpawnKind::Group(_) | SpawnKind::GroupNoCenter(_))
        );
        if !paints || !is_group {
            return vec![anim];
        }
        let members: Vec<ObjectId> = Self::group_paint_members(object_specs, authored_target)
            .into_iter()
            .filter_map(|member| id_map.get(&member).copied())
            .collect();
        if members.is_empty() {
            return vec![anim];
        }
        let mut rest = properties.clone();
        let paint = crate::anim::PropertyAnimation {
            fill: rest.fill.take(),
            stroke_color: rest.stroke_color.take(),
            stroke_width: rest.stroke_width.take(),
            visible_color: rest.visible_color.take(),
            ..Default::default()
        };
        let mut split = Vec::with_capacity(members.len() + 1);
        if !rest.is_empty() {
            split.push(AnimationBuilder {
                anim_type: AnimationType::Properties(rest),
                ..anim.clone()
            });
        }
        split.extend(members.into_iter().map(|target| AnimationBuilder {
            target,
            anim_type: AnimationType::Properties(paint.clone()),
            ..anim.clone()
        }));
        split
    }

    /// The drawn members of a group at any depth, in order: what its fill
    /// and stroke reach. A box's background and a clip mask stay out, as
    /// they do from the group's immediate `fill`.
    fn group_paint_members(
        object_specs: &HashMap<ObjectId, ObjectSpec>,
        group: ObjectId,
    ) -> Vec<ObjectId> {
        let mut members = Vec::new();
        let mut pending = vec![group];
        while let Some(id) = pending.pop() {
            let Some(spec) = object_specs.get(&id) else {
                continue;
            };
            let (SpawnKind::Group(children) | SpawnKind::GroupNoCenter(children)) = &spec.kind
            else {
                members.push(id);
                continue;
            };
            pending.extend(children.iter().rev().copied().filter(|child| {
                Some(*child) != spec.layout_background
                    && object_specs
                        .get(child)
                        .is_none_or(|child| !child.exclude_from_parent_draw)
            }));
        }
        members
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
            SpawnKind::Particles(particles) => {
                let colors = if particles.colors.is_empty() {
                    vec![PenikoColor::WHITE]
                } else {
                    particles.colors.clone()
                };
                let levels = if particles.system.fade > 0.0 {
                    gaanim_animation::PARTICLE_FADE_LEVELS
                } else {
                    1
                };
                let mut layer_spec = spec.clone();
                layer_spec.stroke = None;
                layer_spec.stroke_overridden = true;
                layer_spec.fill_overridden = true;
                let mut refs = Vec::with_capacity(colors.len() * levels);
                let mut layers = Vec::with_capacity(colors.len() * levels);
                for color in &colors {
                    for level in 0..levels {
                        // Fainter levels hold particles further into their fade.
                        layer_spec.fill = Some(gaanim_core::peniko::Brush::Solid(*color));
                        layer_spec.opacity = (level + 1) as f32 / levels as f32;
                        let path = gaanim_objects::prelude::SvgPath {
                            id: "Particles".into(),
                            path: BezPath::new(),
                            bounds: Bounds3D::default(),
                            fill: None,
                            stroke: StrokeBrush::transparent(),
                        };
                        let layer_ref =
                            Self::finish_spawn_builder(builder.svg_path(&path), &layer_spec);
                        let entity = builder
                            .states
                            .get(layer_ref.id)
                            .map_or(Entity::PLACEHOLDER, |state| state.entity);
                        if entity != Entity::PLACEHOLDER {
                            builder
                                .commands
                                .entity(entity)
                                .insert(gaanim_animation::ParticleLayer);
                        }
                        layers.push(entity);
                        refs.push(layer_ref);
                    }
                }
                let mr = builder.group(&refs);
                Self::post_apply(builder, mr.id, spec, id_map, frame_bounds);
                // Continuous emission and the first burst start at the declaration.
                let start = builder.current_time;
                let mut system = particles.system.clone();
                system.colors = colors.len();
                system.start = start;
                system.stop = particles.duration.map(|duration| start + duration);
                if particles.initial_burst > 0 {
                    system.bursts.insert(0, (start, particles.initial_burst));
                }
                let anchor = particles
                    .anchor
                    .and_then(|anchor| id_map.get(&anchor).copied());
                if let Some(state) = builder.states.get(mr.id) {
                    builder.commands.entity(state.entity).insert(
                        gaanim_animation::ParticleEmitter {
                            system,
                            position: particles.position,
                            layers,
                            levels,
                            anchor,
                            trail: gaanim_animation::AnchorTrail::default(),
                            probe: None,
                        },
                    );
                }
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
                body_width,
            } => {
                let b = builder.double_arrow(
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
                radii,
                start_angle,
                sweep_angle,
            } => {
                let b = builder.arc(
                    Point::new(center.0, center.1),
                    Vec2::new(radii.0, radii.1),
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
                        .double_arrow(dimension_start, dimension_end, None, None, None)
                        .fill(color)
                        .no_stroke()
                        .spawn();
                    let group = builder.group(&[extension_a, extension_b, measurement]);
                    Self::post_apply(builder, group.id, spec, id_map, frame_bounds);
                    group
                }
            }
            SpawnKind::Polyline { points, closed } => {
                let points: Vec<Point> = points.iter().map(|&(x, y)| Point::new(x, y)).collect();
                let b = builder.polyline(&points, *closed);
                let mr = Self::finish_spawn_builder(b, spec);
                Self::apply_layout(builder, mr.id, spec, id_map, frame_bounds);
                mr
            }
            SpawnKind::ReactivePolyline { points, closed } => {
                let mut parameter_ids: Vec<gaanim_core::ObjectId> = points
                    .iter()
                    .flat_map(|(x, y)| x.parameter_ids().into_iter().chain(y.parameter_ids()))
                    .collect();
                parameter_ids.sort_unstable();
                parameter_ids.dedup();
                let parameters: Vec<(gaanim_core::ObjectId, bevy::prelude::Entity)> = parameter_ids
                    .into_iter()
                    .filter_map(|logical| {
                        let actual = id_map.get(&logical).copied()?;
                        Some((logical, builder.states.get(actual)?.entity))
                    })
                    .collect();
                let initial: Vec<(gaanim_core::ObjectId, f64)> = parameters
                    .iter()
                    .filter_map(|(logical, _)| {
                        let actual = id_map.get(logical).copied()?;
                        Some((*logical, builder.float_signals.get(&actual).copied()?))
                    })
                    .collect();
                let path = reactive_polyline_path(points, *closed, builder.current_time, |id| {
                    initial
                        .iter()
                        .find(|(logical, _)| *logical == id)
                        .map(|(_, value)| *value)
                });
                let svg_path = gaanim_objects::prelude::SvgPath {
                    id: "ReactivePolyline".to_owned(),
                    bounds: {
                        let rect = path.bounding_box();
                        gaanim_math::Bounds3D::new_2d(rect.x0, rect.y0, rect.x1, rect.y1)
                    },
                    path,
                    fill: None,
                    stroke: StrokeBrush::transparent(),
                };
                let b = builder.svg_path(&svg_path);
                let mr = Self::finish_spawn_builder(b, spec);
                Self::apply_layout(builder, mr.id, spec, id_map, frame_bounds);
                let changes = !parameters.is_empty()
                    || points
                        .iter()
                        .any(|(x, y)| x.depends_on_time() || y.depends_on_time());
                if changes && let Some(state) = builder.states.get(mr.id) {
                    let points = points.clone();
                    let closed = *closed;
                    let redraw = gaanim_animation::AlwaysRedrawRegen::new(move |world| {
                        let time = world
                            .get_resource::<gaanim_animation::PlaybackState>()
                            .map_or(0.0, |state| state.current_time);
                        reactive_polyline_path(&points, closed, time, |id| {
                            let entity = parameters.iter().find(|(logical, _)| *logical == id)?.1;
                            world
                                .get::<gaanim_animation::FloatSignal>(entity)
                                .map(|signal| signal.value)
                        })
                    });
                    builder.commands.entity(state.entity).insert(redraw);
                }
                mr
            }
            SpawnKind::Metaballs {
                sources,
                threshold,
                smoothness,
            } => {
                let entities: Vec<bevy::prelude::Entity> = sources
                    .iter()
                    .filter_map(|logical| {
                        let actual = id_map.get(logical).copied()?;
                        Some(builder.states.get(actual)?.entity)
                    })
                    .collect();
                let svg_path = gaanim_objects::prelude::SvgPath {
                    id: "Metaballs".to_owned(),
                    bounds: gaanim_math::Bounds3D::default(),
                    path: gaanim_core::kurbo::BezPath::new(),
                    fill: None,
                    stroke: StrokeBrush::transparent(),
                };
                let b = builder.svg_path(&svg_path);
                let mr = Self::finish_spawn_builder(b, spec);
                Self::apply_layout(builder, mr.id, spec, id_map, frame_bounds);
                if let Some(state) = builder.states.get(mr.id) {
                    let (threshold, smoothness) = (*threshold, *smoothness);
                    let redraw = gaanim_animation::AlwaysRedrawRegen::new(move |world| {
                        let balls: Vec<gaanim_objects::metaballs::Ball> = entities
                            .iter()
                            .filter_map(|&entity| metaball_of(world, entity))
                            .collect();
                        gaanim_objects::metaballs::metaballs_path(&balls, threshold, smoothness)
                    });
                    builder.commands.entity(state.entity).insert(redraw);
                }
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
                color_map,
                material,
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
                    color_map.as_ref(),
                    material.as_deref().copied(),
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
                    let color_map = color_map.clone();
                    let material = material.as_deref().copied();
                    let follows_time = function.depends_on_time();
                    let last = LastInputs::default();
                    let regen = ReactiveMeshRegen::new(move |world| {
                        let (time, values) = reactive_values(world, &parameter_entities);
                        if !last.changed(follows_time.then_some(time), &values) {
                            return None;
                        }
                        Some(sampled_reactive_surface_3d(
                            &map,
                            &function,
                            resolution,
                            color_map.as_ref(),
                            material,
                            time,
                            &values,
                        ))
                    });
                    builder.commands.entity(state.entity).insert(regen);
                }
                mr
            }
            SpawnKind::ReactiveParametricSurface3D {
                map,
                function,
                u_domain,
                v_domain,
                resolution,
                color,
                color_map,
                color_range,
                material,
            } => {
                let mut parameter_entities =
                    compile_function_parameters(function, id_map, &builder.states);
                let mut follows_time = function.depends_on_time();
                if let super::SurfaceColor::Function(color) = color {
                    for entity in compile_function_parameters(color, id_map, &builder.states) {
                        if !parameter_entities.contains(&entity) {
                            parameter_entities.push(entity);
                        }
                    }
                    follows_time |= color.depends_on_time();
                }
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
                let data = sampled_reactive_parametric_surface_3d(
                    map,
                    function,
                    *u_domain,
                    *v_domain,
                    *resolution,
                    color,
                    color_map.as_ref(),
                    *color_range,
                    material.as_deref().copied(),
                    builder.current_time,
                    &values,
                );
                let mr = builder.spawn_triangle_mesh_data(data);
                Self::post_apply(builder, mr.id, spec, id_map, frame_bounds);
                if (!parameter_entities.is_empty() || follows_time)
                    && let Some(state) = builder.states.get(mr.id)
                {
                    let map = map.clone();
                    let function = function.clone();
                    let (u_domain, v_domain, resolution) = (*u_domain, *v_domain, *resolution);
                    let color = color.clone();
                    let color_map = color_map.clone();
                    let (color_range, material) = (*color_range, material.as_deref().copied());
                    let last = LastInputs::default();
                    let regen = ReactiveMeshRegen::new(move |world| {
                        let (time, values) = reactive_values(world, &parameter_entities);
                        if !last.changed(follows_time.then_some(time), &values) {
                            return None;
                        }
                        Some(sampled_reactive_parametric_surface_3d(
                            &map,
                            &function,
                            u_domain,
                            v_domain,
                            resolution,
                            &color,
                            color_map.as_ref(),
                            color_range,
                            material,
                            time,
                            &values,
                        ))
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
                reserve: _,
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
                let (path, bounds, pen) = gaanim_animation::shape_readout_text_advance(
                    builder.font_registry,
                    prefix,
                    &number,
                    suffix,
                    digit_family,
                    *font_weight,
                    size,
                )
                .unwrap_or_else(|_| (gaanim_core::kurbo::BezPath::new(), Bounds3D::default(), 0.0));
                let baseline = gaanim_animation::right_aligned_readout_baseline(bounds);
                let ink = bounds;
                let (path, bounds) = gaanim_animation::right_align_readout_path(path, bounds);
                // Inside a layout, the widest text it will show: of its ink
                // and of its advance.
                let (reserve, reserve_advance) =
                    builder
                        .readout_reserves
                        .get(&spec.id)
                        .map_or((0.0, 0.0), |values| {
                            values
                                .iter()
                                .filter_map(|value| {
                                    let number = gaanim_animation::localize_decimal_separator(
                                        &gaanim_animation::format_reactive_number(
                                            *value, format, invalid,
                                        ),
                                        *decimal_separator,
                                    );
                                    gaanim_animation::shape_readout_text_advance(
                                        builder.font_registry,
                                        prefix,
                                        &number,
                                        suffix,
                                        digit_family,
                                        *font_weight,
                                        size,
                                    )
                                    .ok()
                                })
                                .map(|(_, bounds, pen)| {
                                    let advance =
                                        gaanim_animation::readout_advance(bounds, pen, 0.0);
                                    (bounds.width(), advance.right - advance.left)
                                })
                                .fold((0.0, 0.0), |(ink, advance), (width, length)| {
                                    (f64::max(ink, width), f64::max(advance, length))
                                })
                        });
                // A rolling number keeps its cells; its ink sets it in a row.
                let advance = rolling
                    .is_none()
                    .then(|| gaanim_animation::readout_advance(ink, pen, reserve_advance));
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
                let bounds = gaanim_animation::reserve_readout_bounds(bounds, reserve);
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
                builder.readout_baselines.insert(mr.id, (baseline, advance));
                if rolling_component.is_some() {
                    builder.text_metrics.insert(
                        mr.id,
                        gaanim_text::prelude::TextMetrics {
                            first_baseline: baseline,
                            line_count: 1,
                            ..Default::default()
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
                    if let Some(advance) = advance {
                        builder.commands.entity(entity).insert(advance);
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
                            reserve,
                            reserve_advance,
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
                    // Redrawn only when its data changes, so a morph of the
                    // mark (transform_to) keeps the shape it animates.
                    let versioned = source.clone();
                    let redraw = gaanim_animation::AlwaysRedrawRegen::versioned(
                        move || versioned.version(),
                        move |_world| {
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
                        },
                    );
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
                let mesh = TriangleMeshData {
                    vertices: vertices.clone(),
                    indices: indices.clone(),
                    normals: None,
                    uvs: None,
                    color: *color,
                    colors,
                    material: None,
                    surface: None,
                    blend: None,
                };
                if builder.morph_mesh_ids.contains(&spec.id) {
                    let mref = builder.spawn_triangle_mesh_data(mesh.clone());
                    builder.morph_meshes.insert(mref.id, Arc::new(mesh));
                    Self::post_apply(builder, mref.id, spec, id_map, frame_bounds);
                    return mref;
                }
                let mref = builder.spawn_triangle_mesh_data(mesh);
                Self::post_apply(builder, mref.id, spec, id_map, frame_bounds);
                mref
            }
            SpawnKind::Volume(volume) => {
                let mref = builder.spawn_volume(volume.clone());
                Self::post_apply(builder, mref.id, spec, id_map, frame_bounds);
                mref
            }
            SpawnKind::Primitive3D(mesh) => {
                let extruded = spec.extrusion.as_ref().and_then(|extrusion| {
                    let source = id_map.get(&extrusion.source).copied()?;
                    Self::extruded_mesh(builder, source, extrusion)
                });
                let (mesh, center) = match extruded {
                    Some((mesh, center)) => (mesh, Some(center)),
                    None => (mesh.clone(), None),
                };
                let morphed = builder
                    .morph_mesh_ids
                    .contains(&spec.id)
                    .then(|| Arc::new(mesh.clone()));
                // Copies of the mesh bound what the drawable draws.
                let copies = spec.copies.as_ref().map(|copies| {
                    let bounds = crate::mesh_copies::copies_bounds(&mesh, copies);
                    (Arc::clone(copies), bounds)
                });
                let mref = builder.spawn_triangle_mesh_data(mesh);
                if let Some(mesh) = morphed {
                    builder.morph_meshes.insert(mref.id, mesh);
                }
                // An imported primitive with morph targets starts with the
                // weights it was declared with.
                if let Some(weights) = &spec.morph_weights
                    && let Some(state) = builder.states.get(mref.id)
                {
                    builder
                        .commands
                        .entity(state.entity)
                        .insert(gaanim_scene::MorphWeights(Arc::clone(weights)));
                    builder.morph_weights.insert(mref.id, Arc::clone(weights));
                }
                if let Some((copies, bounds)) = copies
                    && let Some(state) = builder.states.get_mut(mref.id)
                {
                    state.bounds = bounds;
                    builder.commands.entity(state.entity).insert((
                        gaanim_scene::MeshInstances::new_shared(Arc::clone(&copies)),
                        gaanim_scene::LocalBounds(bounds),
                    ));
                    builder.instanced.insert(mref.id, copies);
                }
                // The mesh is centered on its own origin, so moves, pivots
                // and `create()` behave as for other primitives.
                if let Some(center) = center
                    && let Some(state) = builder.states.get_mut(mref.id)
                {
                    state.transform = state
                        .transform
                        .shift_3d(gaanim_core::glam::DVec3::new(center.x, center.y, 0.0));
                    builder
                        .commands
                        .entity(state.entity)
                        .insert(state.transform);
                }
                Self::post_apply(builder, mref.id, spec, id_map, frame_bounds);
                mref
            }
            SpawnKind::Light3D(light) => {
                // An empty path: the light is a position, shaded with.
                let svg_path = gaanim_objects::prelude::SvgPath {
                    id: "Light3D".to_owned(),
                    path: gaanim_core::kurbo::BezPath::new(),
                    bounds: Bounds3D::default(),
                    fill: None,
                    stroke: StrokeBrush::transparent(),
                };
                let b = builder.svg_path(&svg_path);
                let mr = Self::finish_spawn_builder(b, spec);
                if let Some(state) = builder.states.get(mr.id) {
                    builder.commands.entity(state.entity).insert(*light);
                }
                Self::post_apply(builder, mr.id, spec, id_map, frame_bounds);
                mr
            }
            SpawnKind::Polyline3D { points, colors } => {
                let base_color = line_3d_color(spec);
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
            SpawnKind::ReactivePolyline3D { points, colors } => {
                let base_color = line_3d_color(spec);
                let mut parameter_ids: Vec<gaanim_core::ObjectId> = points
                    .iter()
                    .flatten()
                    .flat_map(|source| source.parameter_ids())
                    .collect();
                parameter_ids.sort_unstable();
                parameter_ids.dedup();
                let parameters: Vec<(gaanim_core::ObjectId, bevy::prelude::Entity)> = parameter_ids
                    .into_iter()
                    .filter_map(|logical| {
                        let actual = id_map.get(&logical).copied()?;
                        Some((logical, builder.states.get(actual)?.entity))
                    })
                    .collect();
                let initial: Vec<(gaanim_core::ObjectId, f64)> = parameters
                    .iter()
                    .filter_map(|(logical, _)| {
                        let actual = id_map.get(logical).copied()?;
                        Some((*logical, builder.float_signals.get(&actual).copied()?))
                    })
                    .collect();
                let colors: Option<Vec<[f32; 4]>> = colors
                    .as_ref()
                    .filter(|colors| colors.len() == points.len())
                    .map(|colors| {
                        colors
                            .iter()
                            .map(|color| {
                                let rgba = color.to_rgba8();
                                [rgba.r, rgba.g, rgba.b, rgba.a].map(|c| f32::from(c) / 255.0)
                            })
                            .collect()
                    });
                let line = reactive_polyline_3d_line(
                    points,
                    colors.as_deref(),
                    base_color,
                    builder.current_time,
                    |id| {
                        initial
                            .iter()
                            .find(|(logical, _)| *logical == id)
                            .map(|(_, value)| *value)
                    },
                );
                let mref =
                    builder.spawn_line_strip_with_colors(line.points, base_color, line.colors);
                Self::post_apply(builder, mref.id, spec, id_map, frame_bounds);
                let changes = !parameters.is_empty()
                    || points
                        .iter()
                        .flatten()
                        .any(|source| source.depends_on_time());
                if changes && let Some(state) = builder.states.get(mref.id) {
                    let points = points.clone();
                    let regen = ReactiveLineRegen::new(move |world| {
                        let time = world
                            .get_resource::<gaanim_animation::PlaybackState>()
                            .map_or(0.0, |state| state.current_time);
                        reactive_polyline_3d_line(
                            &points,
                            colors.as_deref(),
                            base_color,
                            time,
                            |id| {
                                let entity =
                                    parameters.iter().find(|(logical, _)| *logical == id)?.1;
                                world
                                    .get::<gaanim_animation::FloatSignal>(entity)
                                    .map(|signal| signal.value)
                            },
                        )
                    });
                    builder.commands.entity(state.entity).insert(regen);
                }
                mref
            }
            SpawnKind::LineSegments3D { points, colors } => {
                let base_color = line_3d_color(spec);
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
                // A style's stroke outlines the glyphs it covers in the
                // Typst source, the root's like a part's.
                let compiled = compiled_text_measure(&styled_spec, text_config)
                    .expect("unified text spawn must produce a text measure");
                let source = fitted_text_typst_source(
                    builder.font_registry,
                    text,
                    Some(frame_bounds.width().max(1.0)),
                    compiled.font_size,
                    &compiled.font_family,
                    &compiled.math_font,
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
                files,
            } => {
                // A document's text takes the body role's color, as the
                // theme's text does.
                let body = &text_config.roles[&gaanim_text::prelude::TextRole::Body];
                let foreground = color_to_hex(body.fill_color);
                let page_directive = if let Some(w) = page_width {
                    // ponytail: raw interpolation into Typst source — reject control chars to avoid injection
                    if w.trim().is_empty() || w.contains(['\n', '\r', ';', '"', '\'']) {
                        panic!("invalid Typst page width: {w:?}");
                    }
                    format!("#set page(width: {w}, height: auto, margin: 0pt)\n")
                } else {
                    // Fit the page to the content, so `#align(center)` centers
                    // within it instead of widening the drawable to a page.
                    "#set page(width: auto, height: auto, margin: 0pt)\n".to_string()
                };
                let source = format!(
                    "{page_directive}{}#set text(fill: rgb(\"{foreground}\"))\n{source}",
                    builder.typst_theme
                );
                // A document keeps Typst's own proportions (11pt text, table
                // insets, rule widths) and is scaled so its default text is
                // as large as the body role.
                let scale = if *scene_units {
                    1.0
                } else {
                    body.size / TYPST_DEFAULT_TEXT_PT
                };
                let mr = builder.scaled_typst_with_files(
                    &source,
                    false,
                    Some(&body.font_family),
                    None,
                    None,
                    None,
                    scale,
                    files,
                );
                // Typst strokes tables, rules and shapes black unless told
                // otherwise: those strokes take the body color, as the
                // document's text does. Packages that pick their own colors
                // take the theme's from `theme`.
                Self::recolor_black_strokes(builder, mr.id, body.fill_color);
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
                let baseline = spec
                    .reactive_readout_layout
                    .as_ref()
                    .and_then(|layout| Self::lay_out_readout_row(builder, id_map, layout));
                let mr = builder.group_about(&refs, spec.group_origin);
                // A readout's row is placed by its translation (a bar race
                // binds its value's), so only its declaration uses its pivot.
                if spec.reactive_readout_layout.is_none()
                    && let Some(state) = builder.states.get(mr.id)
                {
                    let pivot = spec.group_origin.unwrap_or_else(|| state.bounds.center());
                    builder.group_pivots.insert(mr.id, pivot);
                }
                // A readout is one line of text: its text anchors use the
                // row's baseline.
                if let Some(baseline) = baseline {
                    builder.text_metrics.insert(
                        mr.id,
                        gaanim_text::prelude::TextMetrics {
                            first_baseline: baseline,
                            line_count: 1,
                            ..Default::default()
                        },
                    );
                }
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
                            align: layout.align,
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
                // A glTF node starts in its rest pose, in its parent's space.
                if let Some(pose) = spec.gltf_pose
                    && let Some(state) = builder.states.get_mut(mr.id)
                {
                    state.transform.translation = pose.translation;
                    state.transform.rotation = pose.rotation;
                    state.transform.scale = pose.scale;
                    builder
                        .commands
                        .entity(state.entity)
                        .insert(state.transform);
                }
                Self::post_apply(builder, mr.id, spec, id_map, frame_bounds);
                if let Some(rig) = &spec.gltf_rig {
                    Self::rig_gltf_model(builder, mr.id, rig, id_map);
                }
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
                // Empty 3D line placeholder; TracedPath3D will fill its
                // LineListData, keeping the color of its stroke.
                let mref = builder.spawn_line_strip(vec![], line_3d_color(spec));
                Self::post_apply(builder, mref.id, spec, id_map, frame_bounds);
                mref
            }
        };
        if let Some((_, rank)) = &spec.z_layer
            && let Some(state) = builder.states.get(mref.id)
        {
            builder
                .commands
                .entity(state.entity)
                .insert(gaanim_scene::ZLayer(*rank));
        }
        if spec.covers_on_purpose
            && let Some(state) = builder.states.get(mref.id)
        {
            builder
                .commands
                .entity(state.entity)
                .insert(gaanim_scene::CoversOnPurpose);
        }
        if spec.outside_safe_on_purpose
            && let Some(state) = builder.states.get(mref.id)
        {
            builder
                .commands
                .entity(state.entity)
                .insert(gaanim_scene::OutsideSafeOnPurpose);
        }
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
        if let Some(scales) = spec.stroke_scales_with_object
            && let Some(state) = builder.states.get(mref.id)
        {
            let entities: Vec<_> = std::iter::once(state.entity)
                .chain(state.child_spans.iter().map(|child| child.entity))
                .collect();
            for entity in entities {
                let mut entity = builder.commands.entity(entity);
                if scales {
                    entity.insert(gaanim_renderer::effects::StrokeScalesWithObject);
                } else {
                    entity.remove::<gaanim_renderer::effects::StrokeScalesWithObject>();
                }
            }
        }
        if let Some(chalk) = spec.chalk {
            for (entity, _) in Self::hierarchy_entities(builder, mref.id) {
                builder.commands.entity(entity).insert(chalk);
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
        if spec.post_exempt {
            for (entity, _) in Self::hierarchy_entities(builder, mref.id) {
                builder
                    .commands
                    .entity(entity)
                    .insert(gaanim_renderer::effects::PostProcessExempt);
            }
        }
        if let Some(rate) = spec.step_rate {
            builder.timeline.stepped_objects = true;
            // A member's own rate wins over its group's.
            for (entity, own) in Self::hierarchy_entities(builder, mref.id) {
                let mut entity = builder.commands.entity(entity);
                if own {
                    entity.insert(gaanim_scene::StepRate(rate));
                } else {
                    entity.insert_if_new(gaanim_scene::StepRate(rate));
                }
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
            if let Some(gap) = spec.axis_label_gap {
                builder.commands.entity(entity).insert(gap);
            }
            if let Some(part) = spec.axis_box_part {
                builder.commands.entity(entity).insert(part);
                if matches!(
                    part.role,
                    gaanim_scene::AxisBoxRole::PlaneEdge { .. }
                        | gaanim_scene::AxisBoxRole::CornerNumber { .. }
                ) {
                    builder
                        .commands
                        .entity(entity)
                        .insert(gaanim_scene::AxisBoxShown(true));
                }
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
    /// The instances a falloff drives: the members of a group, or the
    /// drawable itself when it has none.
    fn falloff_members(builder: &SceneBuilder, id: ObjectId) -> Vec<bevy::prelude::Entity> {
        Self::falloff_member_ids(builder, id)
            .into_iter()
            .filter_map(|member| builder.states.get(member))
            .map(|member| member.entity)
            .collect()
    }

    /// An indication without a color tints with the theme's accent, or its
    /// foreground for what already shows the accent.
    fn resolve_emphasis_color(builder: &SceneBuilder, anim: &mut AnimationBuilder) {
        if let AnimationType::Indicate {
            color: color @ None,
            ..
        } = &mut anim.anim_type
        {
            *color = Some(builder.emphasis_tint(anim.target));
        }
    }

    /// The compiled ids of [`Self::falloff_members`].
    fn falloff_member_ids(builder: &SceneBuilder, id: ObjectId) -> Vec<ObjectId> {
        let Some(state) = builder.states.get(id) else {
            return Vec::new();
        };
        let members: Vec<_> = state
            .children
            .iter()
            .copied()
            .filter(|child| builder.states.contains_key(*child))
            .collect();
        if members.is_empty() {
            vec![id]
        } else {
            members
        }
    }

    /// Turn `member` for good by the angle its `look_at`s give it at the
    /// cursor, measured on the authored state as the falloff measures it,
    /// so it keeps pointing where they left it when they end.
    fn keep_look_at_turn(builder: &mut SceneBuilder, member: ObjectId) {
        let Some(look_ats) = builder.look_ats.remove(&member) else {
            return;
        };
        let center = |builder: &SceneBuilder, id: ObjectId| {
            let state = builder.states.get(id)?;
            let world = builder.get_world_transform(id);
            let center = gaanim_layout::transform_bounds(state.bounds, &world).center();
            Some(DVec2::new(center.x, center.y))
        };
        let Some(position) = center(builder, member) else {
            return;
        };
        let turn: f64 = look_ats
            .iter()
            .filter_map(|(goal, offset)| {
                let point = match goal {
                    gaanim_animation::FalloffTarget::Point(point) => Some(*point),
                    gaanim_animation::FalloffTarget::Object(id) => center(builder, *id),
                    gaanim_animation::FalloffTarget::Entity(_) => None,
                }?;
                let toward = point - position;
                (toward.length_squared() > f64::EPSILON).then(|| toward.y.atan2(toward.x) + offset)
            })
            .sum();
        let Some(state) = builder.states.get_mut(member) else {
            return;
        };
        let from = state.transform.rotation;
        let to = from * gaanim_core::glam::DQuat::from_rotation_z(turn);
        state.transform.rotation = to;
        let track = builder.ensure_track(member);
        builder.timeline.add_clip(
            track,
            builder.current_time,
            0.0,
            gaanim_timeline::clip::ClipPayload::Animation(gaanim_timeline::clip::AnimationSpec {
                target: member,
                lens: gaanim_timeline::clip::PropertyLensSpec::Rotation { from, to },
                rate_func: RateFunc::Linear,
                delay: 0.0,
                label: Some("LookAtHold".into()),
            }),
        );
    }

    /// The compiled object ids a bounds frame follows: the drawables, and the
    /// glyphs of the text selections.
    fn bounds_source_ids(
        builder: &mut SceneBuilder,
        id_map: &HashMap<ObjectId, ObjectId>,
        sources: &[crate::anim::BoundsTarget],
    ) -> Vec<ObjectId> {
        sources
            .iter()
            .flat_map(|source| match source {
                crate::anim::BoundsTarget::Drawable(source) => {
                    id_map.get(source).copied().into_iter().collect::<Vec<_>>()
                }
                crate::anim::BoundsTarget::TextSelection {
                    target,
                    fragment,
                    occurrence,
                } => Self::fragment_child_ids(builder, id_map, *target, fragment, *occurrence),
            })
            .collect()
    }

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
        let motion_sources: std::sync::Arc<[ObjectId]> =
            nodes.iter().map(|(node, _, _)| *node).collect();
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
                            gaanim_renderer::effects::PostProcessExempt,
                            gaanim_renderer::effects::StrokeProfile,
                            gaanim_renderer::effects::ChalkBrush,
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
                            window: echo.window(),
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
        if let Some(mark) = &spec.arc_slice {
            b.builder.arc_slices.insert(b.id, mark.clone());
        }
        b.spawn_with_effects(spec.glow.clone(), spec.blur, spec.shadow.clone())
    }

    /// Stroke in `color` the pieces of a compiled Typst hierarchy stroked in
    /// opaque black.
    fn recolor_black_strokes(builder: &mut SceneBuilder, id: ObjectId, color: PenikoColor) {
        let Some(children) = builder
            .states
            .get(id)
            .map(|state| state.child_spans.clone())
        else {
            return;
        };
        for child in children {
            let Some(state) = builder.states.get_mut(child.id) else {
                continue;
            };
            let Some(gaanim_core::peniko::Brush::Solid(stroke)) = &state.stroke.brush else {
                continue;
            };
            if stroke.to_rgba8() != PenikoColor::BLACK.to_rgba8() {
                continue;
            }
            state.stroke.brush = Some(gaanim_core::peniko::Brush::Solid(color));
            builder
                .commands
                .entity(child.entity)
                .insert(state.stroke.clone());
        }
    }

    /// Applies deferred glyph-level color overrides after the normal object
    /// style has been propagated to the compiled text hierarchy.
    fn apply_fragment_fills(builder: &mut SceneBuilder, target: MobjectRef, spec: &ObjectSpec) {
        for (fragment, occurrence, color) in &spec.fragment_fills {
            builder
                .select_occurrence(target, fragment, *occurrence)
                .set_fill(*color);
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

    /// Give `copy`, a text a layout composed anew from the declaration of
    /// `text`, the fill `text` shows now. Both start from the same
    /// declaration, so they differ only when a cut or an animation (such as
    /// a box's `set(color=)`) recolored the text since; then every glyph
    /// takes the new fill, as `post_apply` paints a declared one. A text
    /// never recolored keeps its own glyph colors (markup, highlighting).
    fn carry_text_fill(builder: &mut SceneBuilder, text: ObjectId, copy: ObjectId) {
        let Some(fill) = builder.states.get(text).map(|state| state.fill.clone()) else {
            return;
        };
        let spans = match builder.states.get_mut(copy) {
            Some(state) if state.fill != fill => {
                state.fill = fill.clone();
                state.child_spans.clone()
            }
            _ => return,
        };
        for span in &spans {
            if let Some(span_state) = builder.states.get_mut(span.id) {
                let previous = std::mem::replace(&mut span_state.fill, fill.clone());
                if let Some(ref paint) = fill
                    && span_state.stroke.brush.is_some()
                    && span_state.stroke.brush == previous
                {
                    span_state.stroke.brush = Some(paint.clone());
                    builder
                        .commands
                        .entity(span.entity)
                        .insert(span_state.stroke.clone());
                }
            }
            builder.commands.entity(span.entity).insert(match fill {
                Some(ref paint) => FillBrush(Some(paint.clone())),
                None => FillBrush::transparent(),
            });
        }
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
                    // Strokes painted like the glyphs (rules, underlines)
                    // follow the new fill; a styled outline keeps its color.
                    let previous = std::mem::replace(&mut child_state.fill, spec.fill.clone());
                    if let Some(ref f) = spec.fill
                        && child_state.stroke.brush.is_some()
                        && child_state.stroke.brush == previous
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
        // Glyphs go on their path before a layout measures the Text.
        if let Some(text_path) = &spec.text_path
            && let Some(path) = id_map.get(&text_path.path).copied()
        {
            builder.attach_text_path(id, path, text_path);
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
                | SpawnKind::Metaballs { .. }
                | SpawnKind::Points { .. }
                | SpawnKind::Sector { .. }
                | SpawnKind::Brace { .. }
                | SpawnKind::Arc { .. }
                | SpawnKind::CurvedArrow { .. }
                | SpawnKind::CurvedArrowArc { .. }
                | SpawnKind::Dimension { .. }
                | SpawnKind::Polyline { .. }
                | SpawnKind::ReactivePolyline { .. }
                | SpawnKind::Bezier { .. }
                | SpawnKind::Curve(_)
        )
    }

    /// Set an entity's whole transform, except that a scale of zero stays: it
    /// is the start of a grow entry, and a reflow must not show the object
    /// before its turn.
    fn insert_transform_keeping_grow(
        builder: &mut SceneBuilder,
        entity: bevy::prelude::Entity,
        transform: SpatialTransform,
    ) {
        builder.commands.queue(move |world: &mut World| {
            let Ok(mut entity) = world.get_entity_mut(entity) else {
                return;
            };
            let mut transform = transform;
            if entity
                .get::<SpatialTransform>()
                .is_some_and(|current| current.scale == DVec3::ZERO)
            {
                transform.scale = DVec3::ZERO;
            }
            entity.insert(transform);
        });
    }

    /// Write the translation and scale a layout gives a member to its
    /// entity. Its rotation and skew are its own: a child rotated later in
    /// the timeline must not show that rotation before its animation starts.
    /// A scale of zero stays: it is the start of a grow entry, and a later
    /// reflow must not show the member before its turn.
    /// Place a readout's parts in their row now, as
    /// [`gaanim_animation::signals::reactive_readout_layout_system`] does every
    /// frame, so the group's box is the row's: a layout then centers the row
    /// in its cell.
    /// Returns the row's baseline, which the readout's text anchors use.
    fn lay_out_readout_row(
        builder: &mut SceneBuilder,
        id_map: &HashMap<ObjectId, ObjectId>,
        layout: &super::types::ReactiveReadoutLayoutSpec,
    ) -> Option<f64> {
        let parts: Vec<(ObjectId, Bounds3D, f64, Option<gaanim_scene::TextAdvance>)> = [
            layout.label,
            layout.equals,
            Some(layout.number),
            layout.unit,
        ]
        .into_iter()
        .flatten()
        .filter_map(|id| {
            let id = *id_map.get(&id)?;
            let state = builder.states.get(id)?;
            let (bounds, entity) = (state.bounds, state.entity);
            let (baseline, advance) = builder
                .readout_baselines
                .get(&id)
                .copied()
                .or_else(|| {
                    let metrics = builder.text_metrics.get(&id)?;
                    let advance = metrics
                        .advance
                        .map(|(left, right)| gaanim_scene::TextAdvance { left, right });
                    Some((metrics.first_baseline, advance))
                })
                .unwrap_or(((bounds.min.y + bounds.max.y) * 0.5, None));
            // The frame sets the texts in the row by their advance too.
            if let Some(advance) = advance {
                builder.commands.entity(entity).insert(advance);
            }
            Some((id, bounds, baseline, advance))
        })
        .collect();
        let rows: Vec<_> = parts
            .iter()
            .map(|(_, bounds, baseline, advance)| (*bounds, *baseline, *advance))
            .collect();
        let translations =
            gaanim_animation::signals::readout_row(&rows, layout.spacing, layout.align);
        // Every part's baseline lands on the same height.
        let baseline = parts
            .first()
            .zip(translations.first())
            .map(|((_, _, baseline, _), translation)| baseline + translation.y);
        for ((id, ..), translation) in parts.iter().zip(translations) {
            let Some(state) = builder.states.get_mut(*id) else {
                continue;
            };
            state.transform.translation.x = translation.x;
            state.transform.translation.y = translation.y;
            let (entity, transform) = (state.entity, state.transform);
            Self::place_layout_member(builder, entity, transform);
        }
        baseline
    }

    /// Where a child that unfolds into its box starts, from its place
    /// (`entering`), or how far one that folds out of it travels: with the
    /// sibling before it, so its content keeps to the edge it enters from.
    /// The first child keeps to the start of the box, where the next sibling
    /// began or ends. In the parent's space; zero in a grid or a stack.
    #[allow(clippy::too_many_arguments)]
    fn fold_shift(
        builder: &SceneBuilder,
        tree: &CompiledLayoutTree,
        resolved: &gaanim_layout::ResolvedLayout,
        materialized_by_id: &BTreeMap<gaanim_layout::LayoutId, ObjectId>,
        before: &HashMap<ObjectId, SpatialTransform>,
        targets: &[(ObjectId, SpatialTransform)],
        child: ObjectId,
        entering: bool,
    ) -> DVec3 {
        fn main_axis(
            node: &gaanim_layout::LayoutNode,
            id: gaanim_layout::LayoutId,
        ) -> Option<usize> {
            if node.id == id {
                return match node.kind {
                    gaanim_layout::LayoutNodeKind::Row { .. } => Some(0),
                    gaanim_layout::LayoutNodeKind::Column { .. } => Some(1),
                    _ => None,
                };
            }
            node.children
                .iter()
                .find_map(|child| main_axis(&child.node, id))
        }
        // The leading edge of a cell along the main axis: its left in a row,
        // its top in a column (y up).
        let lead = |cell: Bounds3D, axis: usize| if axis == 0 { cell.min.x } else { cell.max.y };
        let travel = |member: ObjectId| -> Option<DVec3> {
            let from = before.get(&member)?.translation;
            let to = targets
                .iter()
                .find(|(target, _)| *target == member)?
                .1
                .translation;
            Some(to - from)
        };
        let layout_id = |member: ObjectId| {
            materialized_by_id
                .iter()
                .find(|(_, actual)| **actual == member)
                .map(|(id, _)| *id)
        };
        if entering {
            let Some(id) = layout_id(child) else {
                return DVec3::ZERO;
            };
            let Some(parent) = tree.parent_by_id.get(&id).copied() else {
                return DVec3::ZERO;
            };
            let Some(axis) = main_axis(&tree.root, parent) else {
                return DVec3::ZERO;
            };
            let siblings = tree
                .children_by_id
                .get(&parent)
                .cloned()
                .unwrap_or_default();
            let Some(index) = siblings.iter().position(|sibling| *sibling == id) else {
                return DVec3::ZERO;
            };
            if index > 0 {
                return materialized_by_id
                    .get(&siblings[index - 1])
                    .and_then(|previous| travel(*previous))
                    .map_or(DVec3::ZERO, |moved| -moved);
            }
            let Some(next_id) = siblings.get(index + 1) else {
                return DVec3::ZERO;
            };
            let (Some(moved), Some(cell), Some(next_cell)) = (
                materialized_by_id
                    .get(next_id)
                    .and_then(|next| travel(*next)),
                resolved.boxes.get(&id),
                resolved.boxes.get(next_id),
            ) else {
                return DVec3::ZERO;
            };
            // It starts where the next child, the first one until now, began.
            let mut shift = -moved;
            shift[axis] = lead(next_cell.bounds, axis) - moved[axis] - lead(cell.bounds, axis);
            return shift;
        }
        let Some(state) = builder.states.get(child) else {
            return DVec3::ZERO;
        };
        let Some(parent) = state.parent.and_then(layout_id) else {
            return DVec3::ZERO;
        };
        let Some(axis) = main_axis(&tree.root, parent) else {
            return DVec3::ZERO;
        };
        let at = state.transform.translation[axis];
        // Before its place along the main axis: to the left in a row, above
        // in a column.
        let ahead = |position: f64| {
            if axis == 0 {
                position < at
            } else {
                position > at
            }
        };
        let siblings: Vec<(ObjectId, f64)> = tree
            .children_by_id
            .get(&parent)
            .into_iter()
            .flatten()
            .filter_map(|id| materialized_by_id.get(id))
            .filter_map(|member| Some((*member, before.get(member)?.translation[axis])))
            .collect();
        let nearest = |candidates: Vec<(ObjectId, f64)>| {
            candidates
                .into_iter()
                .min_by(|left, right| (left.1 - at).abs().total_cmp(&(right.1 - at).abs()))
        };
        let (previous, next): (Vec<_>, Vec<_>) = siblings
            .into_iter()
            .partition(|(_, position)| ahead(*position));
        if let Some((previous, _)) = nearest(previous) {
            return travel(previous).unwrap_or(DVec3::ZERO);
        }
        let Some((next, _)) = nearest(next) else {
            return DVec3::ZERO;
        };
        let (Some(next_id), Some(parent_box)) = (layout_id(next), resolved.boxes.get(&parent))
        else {
            return DVec3::ZERO;
        };
        let Some(next_cell) = resolved.boxes.get(&next_id) else {
            return DVec3::ZERO;
        };
        // It ends where the next child, the first one from now on, begins.
        let own_lead = at
            + if axis == 0 {
                state.bounds.min.x * state.transform.scale.x
            } else {
                state.bounds.max.y * state.transform.scale.y
            };
        let mut shift = travel(next).unwrap_or(DVec3::ZERO);
        shift[axis] = lead(next_cell.bounds, axis) - parent_box.bounds.center()[axis] - own_lead;
        shift
    }

    fn place_layout_member(
        builder: &mut SceneBuilder,
        entity: bevy::prelude::Entity,
        transform: SpatialTransform,
    ) {
        builder.commands.queue(move |world: &mut World| {
            if let Some(mut current) = world.get_mut::<SpatialTransform>(entity) {
                current.translation = transform.translation;
                if current.scale != DVec3::ZERO {
                    current.scale = transform.scale;
                }
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
        // A Text on a path already sits where its path is.
        let uses_default_text_anchor =
            matches!(spec.kind, SpawnKind::Text(_)) && spec.text_path.is_none();
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
        let mut pivot_in_parent = None;
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
                        *translation - spec.group_origin.unwrap_or_else(|| bounds.center())
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
                    // The anchor lives in the parent's space, such as the
                    // document of an SVG part, laid out after its SVG.
                    pivot_in_parent = Some(builder.declare_pivot(id, *pivot));
                }
                LayoutOp::MoveAnchorTo { target, anchor } => {
                    pending_text_anchor = None;
                    transform =
                        gaanim_layout::compute_move_to(bounds, &transform, *target, *anchor);
                }
                LayoutOp::MoveToAnchorPoint { point } => {
                    pending_text_anchor = None;
                    let Some(reference_id) = id_map.get(&point.object).copied() else {
                        tracing::warn!(
                            "SceneModel layout skipped: anchor point object {:?} was not spawned before {:?}",
                            point.object,
                            spec.id
                        );
                        continue;
                    };
                    let Some(reference_state) = builder.states.get(reference_id) else {
                        tracing::warn!(
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
                        tracing::warn!(
                            "SceneModel layout skipped: reference object {:?} was not spawned before {:?}",
                            reference,
                            spec.id
                        );
                        continue;
                    };
                    let Some(reference_state) = builder.states.get(reference_id) else {
                        tracing::warn!(
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
                        tracing::warn!(
                            "SceneModel layout skipped: reference object {:?} was not spawned before {:?}",
                            reference,
                            spec.id
                        );
                        continue;
                    };
                    let Some(reference_state) = builder.states.get(reference_id) else {
                        tracing::warn!(
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

            if let Some(pivot) = pivot_in_parent {
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
        } else if let Some(pivot) = pivot_in_parent {
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
mod tests;
