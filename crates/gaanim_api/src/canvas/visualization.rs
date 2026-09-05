use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, Mutex};

use gaanim_animation::{ReactiveFunction, ScalarSource};
use gaanim_core::glam::DVec3;
use gaanim_core::kurbo::{BezPath, Circle, Point, Rect, Shape};
use gaanim_core::peniko::Color;
use gaanim_core::{ColorMap, ObjectId};
use gaanim_objects::prelude::SvgPath;
use gaanim_visualization::{
    Axis, AxisLabelPosition, Cartesian3DVisibility, CartesianSpace, CartesianVisibility, Channel,
    ChartSpec, ConstantValue, CoordinateMap2D, CoordinateMap3D, DataMarkKind, Encoding, MarkKind,
    MatchPolicy, NonFinitePolicy, NumberLine, NumberLineVisibility, PlotFrame, PolarSpace,
    PolarVisibility, Sampling, Scale, SpaceGeometry2D, SpaceLayer, StreamlineOptions,
    TransitionFallback, VectorField as FieldModel, area_path, bars, box_stats, error_bar_path,
    histogram, implicit_contours, line_path, sample_function, sample_parametric, sample_surface,
    scatter_points, step_path, violin_path,
};

use super::ops::Op;
use super::{Anchor, CanvasEndpoint, DrawableHandle, PointRef, SceneModel, SpawnKind};

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum VisualizationError {
    #[error(transparent)]
    Axis(#[from] gaanim_visualization::AxisError),
    #[error(transparent)]
    Sampling(#[from] gaanim_visualization::SamplingError),
    #[error(transparent)]
    Chart(#[from] gaanim_visualization::ChartError),
    #[error(transparent)]
    VectorField(#[from] gaanim_visualization::VectorFieldError),
    #[error("data columns must have the same non-zero length")]
    LengthMismatch,
    #[error("mark dimensions must be finite and positive")]
    InvalidSize,
    #[error("statistics require at least one finite value")]
    EmptyData,
    #[error("3D axes currently require linear or temporal scales")]
    Unsupported3DScale,
    #[error("animated domain views currently require linear or temporal scales")]
    UnsupportedAnimatedView,
    #[error("parameter values must be finite")]
    InvalidParameter,
    #[error("mark {0:?} does not have a 3D batch materializer")]
    UnsupportedChartMark3D(MarkKind),
}

fn axis_title_coordinate(position: AxisLabelPosition, extent: f64, gap: f64) -> f64 {
    match position {
        AxisLabelPosition::Start => -extent * 0.5 - gap,
        AxisLabelPosition::Center => 0.0,
        AxisLabelPosition::End => extent * 0.5 + gap,
    }
}

/// A point expressed in one coordinate space's local data mapping.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CoordinateRef {
    pub space: ObjectId,
    pub local: DVec3,
}

/// Animatable scalar exposed to Python reactive callables as an explicit input.
#[derive(Debug, Clone)]
pub struct Parameter {
    handle: DrawableHandle,
    value: Arc<Mutex<f64>>,
}

impl Parameter {
    pub fn drawable(&self) -> &DrawableHandle {
        &self.handle
    }

    /// Internal scalar source used by the Python binding.
    #[doc(hidden)]
    pub fn source(&self) -> ScalarSource {
        let owner = self
            .handle
            .state
            .lock()
            .expect("canvas state poisoned")
            .scene_id;
        ScalarSource::Function(
            ReactiveFunction::new(
                0,
                1,
                vec![gaanim_animation::ReactiveInput::Signal(self.handle.id)],
                |values| Ok(vec![values[0]]),
            )
            .with_scene_owner(owner),
        )
    }

    pub fn current(&self) -> f64 {
        *self.value.lock().expect("parameter poisoned")
    }

    /// Update the authoring-side mirror without queuing a deferred mutation.
    #[doc(hidden)]
    pub fn set_runtime_current(&self, value: f64) {
        *self.value.lock().expect("parameter poisoned") = value;
    }

    pub fn set(&self, value: f64) -> Result<(), VisualizationError> {
        if !value.is_finite() {
            return Err(VisualizationError::InvalidParameter);
        }
        *self.value.lock().expect("parameter poisoned") = value;
        self.handle.clone().set_value(value);
        Ok(())
    }

    pub fn animate(&self) -> super::types::Anim {
        self.handle.animate()
    }
}

/// Typed coordinate-space handle. Its layers and plots are real child
/// drawables, so root layout transforms remain coherent.
#[derive(Debug, Clone)]
pub struct CoordinateSpaceHandle {
    pub(crate) root: DrawableHandle,
    pub(crate) view: DrawableHandle,
    pub(crate) map: CoordinateMap2D,
    pub(crate) layers: HashMap<SpaceLayer, DrawableHandle>,
}

/// Typed 3D coordinate space with immediate data/local conversions.
#[derive(Debug, Clone)]
pub struct CoordinateSpace3DHandle {
    pub(crate) root: DrawableHandle,
    pub(crate) map: CoordinateMap3D,
    pub(crate) layers: HashMap<SpaceLayer, DrawableHandle>,
}

#[derive(Clone)]
pub struct VectorField2DHandle {
    pub(crate) space: CoordinateSpaceHandle,
    pub(crate) field: FieldModel<2>,
    pub(crate) function: Option<ReactiveFunction>,
}

#[derive(Clone)]
pub struct VectorField3DHandle {
    pub(crate) space: CoordinateSpace3DHandle,
    pub(crate) field: FieldModel<3>,
    pub(crate) function: Option<ReactiveFunction>,
}

#[derive(Debug, Clone)]
pub struct ArrowVectorFieldHandle {
    pub(crate) drawable: DrawableHandle,
}

impl ArrowVectorFieldHandle {
    pub fn drawable(&self) -> &DrawableHandle {
        &self.drawable
    }
}

#[derive(Debug, Clone)]
pub struct StreamLinesHandle {
    pub(crate) drawable: DrawableHandle,
    pub(crate) lines: Vec<DrawableHandle>,
    pub(crate) flow_lines: Vec<DrawableHandle>,
}

#[derive(Debug, Clone)]
pub struct FlowParticlesHandle {
    pub(crate) drawable: DrawableHandle,
    pub(crate) animations: Vec<super::types::Anim>,
}

impl FlowParticlesHandle {
    pub fn drawable(&self) -> &DrawableHandle {
        &self.drawable
    }

    pub fn flow(&self) -> Vec<super::types::Anim> {
        self.animations.clone()
    }
}

impl StreamLinesHandle {
    pub fn drawable(&self) -> &DrawableHandle {
        &self.drawable
    }

    /// Build one finite, seekable passing-flash animation per streamline.
    pub fn flow(&self, duration: f64, time_width: f64) -> Vec<super::types::Anim> {
        let fade_duration = (duration * 0.1).min(0.2).max(f64::EPSILON);
        self.lines
            .iter()
            .zip(&self.flow_lines)
            .flat_map(|(base, line)| {
                debug_assert_ne!(base.id, line.id);
                [
                    line.show_passing_flash(duration, time_width),
                    line.fade_in(Some(fade_duration)),
                ]
            })
            .collect()
    }
}

#[derive(Debug, Clone)]
pub struct ArrowFieldOptions {
    pub min_length: f64,
    pub max_length: f64,
    pub length_scale: f64,
    pub width: f64,
    pub tip_length: Option<f64>,
    pub tip_width: Option<f64>,
    pub color: Option<Color>,
    pub colormap: Option<ColorMap>,
    pub color_range: Option<(f64, f64)>,
}

impl Default for ArrowFieldOptions {
    fn default() -> Self {
        Self {
            min_length: 0.0,
            max_length: 28.0,
            length_scale: 1.0,
            width: 2.0,
            tip_length: None,
            tip_width: None,
            color: None,
            colormap: ColorMap::named("viridis").ok(),
            color_range: None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct StreamLinesStyle {
    pub integration: StreamlineOptions,
    pub width: f64,
    pub opacity: f64,
    pub color: Option<Color>,
    pub colormap: Option<ColorMap>,
    pub color_range: Option<(f64, f64)>,
}

#[derive(Debug, Clone)]
pub struct FlowParticleOptions {
    pub integration: StreamlineOptions,
    pub duration: f64,
    pub radius: f64,
    pub color: Option<Color>,
    pub colormap: Option<ColorMap>,
    pub color_range: Option<(f64, f64)>,
    pub opacity: f64,
}

impl Default for FlowParticleOptions {
    fn default() -> Self {
        let mut integration = StreamlineOptions::default();
        integration.direction = gaanim_visualization::StreamDirection::Forward;
        Self {
            integration,
            duration: 3.0,
            radius: 5.0,
            color: None,
            colormap: ColorMap::named("viridis").ok(),
            color_range: None,
            opacity: 1.0,
        }
    }
}

impl Default for StreamLinesStyle {
    fn default() -> Self {
        Self {
            integration: StreamlineOptions::default(),
            width: 2.0,
            opacity: 1.0,
            color: None,
            colormap: ColorMap::named("viridis").ok(),
            color_range: None,
        }
    }
}

/// Materialized declarative chart with stable semantic layers.
#[derive(Debug, Clone)]
pub struct ChartHandle {
    pub(crate) root: DrawableHandle,
    pub(crate) marks: DrawableHandle,
    pub(crate) axes: DrawableHandle,
    pub(crate) grid: Option<DrawableHandle>,
    pub(crate) guides: Option<DrawableHandle>,
    pub(crate) labels: Option<DrawableHandle>,
    pub(crate) spec: ChartSpec,
}

impl ChartHandle {
    pub fn drawable(&self) -> &DrawableHandle {
        &self.root
    }

    pub fn spec(&self) -> &ChartSpec {
        &self.spec
    }

    pub fn layer(&self, name: &str) -> Option<&DrawableHandle> {
        match name {
            "marks" => Some(&self.marks),
            "axes" => Some(&self.axes),
            "grid" => self.grid.as_ref(),
            "guides" => self.guides.as_ref(),
            "labels" => self.labels.as_ref(),
            _ => None,
        }
    }

    pub fn move_to(self, x: f64, y: f64) -> Self {
        self.root.clone().move_to(x, y);
        self
    }

    pub fn move_to_3d(self, x: f64, y: f64, z: f64) -> Self {
        self.root.clone().move_to_3d(x, y, z);
        self
    }

    pub fn scale_to(self, factor: f64) -> Self {
        self.root.clone().scale_to(factor);
        self
    }

    pub fn transition_to(
        &self,
        target: &ChartHandle,
        matching: MatchPolicy,
        fallback: TransitionFallback,
    ) -> Result<super::types::Anim, VisualizationError> {
        let transition = self.spec.transition_to(&target.spec, matching, fallback)?;
        Ok(match transition.kind {
            // Charts are composite retained hierarchies and can cross the
            // vector/mesh renderer boundary.  Replacement preserves the
            // morph proxy during the clip, then atomically hands ownership to
            // the complete target hierarchy at the exact endpoint.
            gaanim_visualization::TransitionKind::Morph
                if transition.source.dimensions == transition.target.dimensions =>
            {
                self.root.replacement_transform(&target.root)
            }
            // Crossing the vector/mesh renderer boundary cannot use a path
            // proxy: the vector root has no triangle geometry to interpolate.
            // A hierarchy-aware fade keeps both retained batches alive and
            // hands visibility to the native 3D target deterministically.
            gaanim_visualization::TransitionKind::Morph => self.root.fade_transform(&target.root),
            gaanim_visualization::TransitionKind::Crossfade => {
                self.root.fade_transform(&target.root)
            }
        })
    }
}

fn chart_field(spec: &ChartSpec, channel: Channel) -> Result<String, VisualizationError> {
    spec.encodings()
        .get(&channel)
        .and_then(Encoding::column)
        .map(str::to_owned)
        .ok_or_else(|| {
            gaanim_visualization::ChartError::MissingRequiredEncoding(spec.mark_spec().kind).into()
        })
}

fn chart_option_number(spec: &ChartSpec, name: &str, default: f64) -> f64 {
    match spec.mark_spec().options.get(name) {
        Some(ConstantValue::Number(value)) if value.is_finite() => *value,
        _ => default,
    }
}

fn chart_option_usize(spec: &ChartSpec, name: &str, default: usize) -> usize {
    chart_option_number(spec, name, default as f64)
        .round()
        .max(1.0) as usize
}

fn chart_option_field(spec: &ChartSpec, name: &str) -> Result<String, VisualizationError> {
    match spec.mark_spec().options.get(name) {
        Some(ConstantValue::Text(value)) => Ok(value.clone()),
        _ => Err(
            gaanim_visualization::ChartError::MissingRequiredEncoding(spec.mark_spec().kind).into(),
        ),
    }
}

fn chart_series_color(canvas: &SceneModel) -> Color {
    canvas
        .theme_style
        .as_ref()
        .and_then(|theme| theme.series.first().copied())
        .unwrap_or(Color::from_rgb8(0x2E, 0x86, 0xAB))
}

fn chart_encoding_number(spec: &ChartSpec, channel: Channel, default: f64) -> f64 {
    match spec.encodings().get(&channel) {
        Some(Encoding::Value(ConstantValue::Number(value))) if value.is_finite() => *value,
        _ => default,
    }
}

fn chart_encoding_color(spec: &ChartSpec) -> Option<Color> {
    match spec.encodings().get(&Channel::Color) {
        Some(Encoding::Value(ConstantValue::Color(color))) => Some(*color),
        _ => None,
    }
}

fn color_with_opacity(color: Color, opacity: f64) -> Color {
    let rgba = color.to_rgba8();
    Color::from_rgba8(
        rgba.r,
        rgba.g,
        rgba.b,
        (f64::from(rgba.a) * opacity.clamp(0.0, 1.0)).round() as u8,
    )
}

fn value_range(
    values: impl IntoIterator<Item = f64>,
    explicit: Option<(f64, f64)>,
) -> Option<(f64, f64)> {
    if let Some((min, max)) = explicit {
        return (min.is_finite() && max.is_finite() && min < max).then_some((min, max));
    }
    let mut min = f64::INFINITY;
    let mut max = f64::NEG_INFINITY;
    for value in values.into_iter().filter(|value| value.is_finite()) {
        min = min.min(value);
        max = max.max(value);
    }
    if !min.is_finite() {
        None
    } else if (max - min).abs() <= f64::EPSILON {
        Some((min - 0.5, max + 0.5))
    } else {
        Some((min, max))
    }
}

fn mapped_color(
    value: f64,
    range: (f64, f64),
    color: Option<Color>,
    colormap: Option<&ColorMap>,
    opacity: f64,
) -> Color {
    let base = color.unwrap_or_else(|| {
        let t = ((value - range.0) / (range.1 - range.0)).clamp(0.0, 1.0);
        colormap
            .and_then(|map| map.sample(t).ok())
            .unwrap_or(Color::from_rgb8(0x2E, 0x86, 0xAB))
    });
    color_with_opacity(base, opacity)
}

fn flow_highlight_color(color: Color) -> Color {
    let rgba = color.to_rgba8();
    let brighten = |channel: u8| channel.saturating_add(((255 - channel as u16) / 3) as u8);
    Color::from_rgba8(brighten(rgba.r), brighten(rgba.g), brighten(rgba.b), rgba.a)
}

fn flow_highlight_style(style: &StreamLinesStyle) -> StreamLinesStyle {
    let mut highlighted = style.clone();
    highlighted.width *= 1.6;
    highlighted.color = highlighted.color.map(flow_highlight_color);
    highlighted.colormap = highlighted.colormap.as_ref().and_then(|map| {
        let colors = map
            .colors(256)
            .ok()?
            .into_iter()
            .map(flow_highlight_color)
            .collect();
        ColorMap::from_colors(colors, None).ok()
    });
    highlighted
}

fn halton(mut index: usize, base: usize) -> f64 {
    let mut result = 0.0;
    let mut fraction = 1.0 / base as f64;
    while index > 0 {
        result += fraction * (index % base) as f64;
        index /= base;
        fraction /= base as f64;
    }
    result
}

fn function_is_reactive(function: &ReactiveFunction) -> bool {
    !function.inputs().is_empty()
}

fn normalized_local(position: [f64; 3], size: [f64; 3]) -> [f32; 3] {
    [
        ((position[0] - 0.5) * size[0]) as f32,
        ((position[1] - 0.5) * size[1]) as f32,
        ((position[2] - 0.5) * size[2]) as f32,
    ]
}

fn append_octahedron(
    vertices: &mut Vec<[f32; 3]>,
    indices: &mut Vec<u32>,
    center: [f32; 3],
    radius: f32,
) {
    let base = vertices.len() as u32;
    vertices.extend_from_slice(&[
        [center[0] + radius, center[1], center[2]],
        [center[0] - radius, center[1], center[2]],
        [center[0], center[1] + radius, center[2]],
        [center[0], center[1] - radius, center[2]],
        [center[0], center[1], center[2] + radius],
        [center[0], center[1], center[2] - radius],
    ]);
    for triangle in [
        [0, 2, 4],
        [2, 1, 4],
        [1, 3, 4],
        [3, 0, 4],
        [2, 0, 5],
        [1, 2, 5],
        [3, 1, 5],
        [0, 3, 5],
    ] {
        indices.extend(triangle.map(|index| base + index));
    }
}

fn append_box(vertices: &mut Vec<[f32; 3]>, indices: &mut Vec<u32>, min: [f32; 3], max: [f32; 3]) {
    let base = vertices.len() as u32;
    vertices.extend_from_slice(&[
        [min[0], min[1], min[2]],
        [max[0], min[1], min[2]],
        [max[0], max[1], min[2]],
        [min[0], max[1], min[2]],
        [min[0], min[1], max[2]],
        [max[0], min[1], max[2]],
        [max[0], max[1], max[2]],
        [min[0], max[1], max[2]],
    ]);
    for triangle in [
        [0, 2, 1],
        [0, 3, 2],
        [4, 5, 6],
        [4, 6, 7],
        [0, 1, 5],
        [0, 5, 4],
        [1, 2, 6],
        [1, 6, 5],
        [2, 3, 7],
        [2, 7, 6],
        [3, 0, 4],
        [3, 4, 7],
    ] {
        indices.extend(triangle.map(|index| base + index));
    }
}

#[derive(Debug, Clone)]
pub struct NumberLineHandle {
    pub(crate) root: DrawableHandle,
    pub(crate) line: NumberLine,
    pub(crate) layers: HashMap<SpaceLayer, DrawableHandle>,
}

impl NumberLineHandle {
    pub fn drawable(&self) -> &DrawableHandle {
        &self.root
    }

    pub fn coord(&self, value: f64) -> Result<CoordinateRef, VisualizationError> {
        Ok(CoordinateRef {
            space: self.root.id,
            local: DVec3::new(self.line.data_to_local(value)?, 0.0, 0.0),
        })
    }

    pub fn data_to_local(&self, value: f64) -> Result<f64, VisualizationError> {
        self.line.data_to_local(value).map_err(Into::into)
    }

    pub fn domain(&self) -> (f64, f64) {
        self.line.axis.domain()
    }

    /// Create a non-rendered reactive point in the line's local frame.
    pub fn point_ref(
        &self,
        value: ScalarSource,
        normal_offset: ScalarSource,
    ) -> Result<PointRef, VisualizationError> {
        Ok(PointRef(CanvasEndpoint::LocalNumberLine {
            space: self.root.id,
            axis: self.line.axis.clone(),
            length: self.line.length,
            value,
            normal_offset,
        }))
    }

    pub fn layer(&self, layer: SpaceLayer) -> Option<&DrawableHandle> {
        self.layers.get(&layer)
    }
}

#[derive(Debug, Clone)]
pub struct PolarSpaceHandle {
    pub(crate) root: DrawableHandle,
    pub(crate) space: PolarSpace,
    pub(crate) layers: HashMap<SpaceLayer, DrawableHandle>,
}

impl PolarSpaceHandle {
    pub fn drawable(&self) -> &DrawableHandle {
        &self.root
    }

    pub fn coord(&self, radius: f64, angle: f64) -> Result<CoordinateRef, VisualizationError> {
        let point = self.space.data_to_local(radius, angle)?;
        Ok(CoordinateRef {
            space: self.root.id,
            local: DVec3::new(point.x, point.y, 0.0),
        })
    }

    pub fn layer(&self, layer: SpaceLayer) -> Option<&DrawableHandle> {
        self.layers.get(&layer)
    }
}

impl CoordinateSpace3DHandle {
    pub fn drawable(&self) -> &DrawableHandle {
        &self.root
    }

    pub fn map(&self) -> &CoordinateMap3D {
        &self.map
    }

    pub fn data_to_local(&self, point: [f64; 3]) -> Result<[f64; 3], VisualizationError> {
        self.map.data_to_local(point).map_err(Into::into)
    }

    pub fn local_to_data(&self, point: [f64; 3]) -> Result<[f64; 3], VisualizationError> {
        self.map.local_to_data(point).map_err(Into::into)
    }

    pub fn layer(&self, layer: SpaceLayer) -> Option<&DrawableHandle> {
        self.layers.get(&layer)
    }

    pub fn move_to(self, point: [f64; 3]) -> Self {
        self.root.clone().move_to_3d(point[0], point[1], point[2]);
        self
    }

    pub fn scale_to(self, factor: f64) -> Self {
        self.root.clone().scale_to_3d(factor, factor, factor);
        self
    }
}

impl CoordinateSpaceHandle {
    pub fn drawable(&self) -> &DrawableHandle {
        &self.root
    }

    pub fn into_drawable(self) -> DrawableHandle {
        self.root
    }

    pub fn map(&self) -> &CoordinateMap2D {
        &self.map
    }

    pub fn coord(&self, x: f64, y: f64) -> Result<CoordinateRef, VisualizationError> {
        let point = self.map.data_to_local(x, y)?;
        Ok(CoordinateRef {
            space: self.view.id,
            local: DVec3::new(point.x, point.y, 0.0),
        })
    }

    pub fn data_to_local(&self, x: f64, y: f64) -> Result<(f64, f64), VisualizationError> {
        let point = self.map.data_to_local(x, y)?;
        Ok((point.x, point.y))
    }

    pub fn local_to_data(&self, x: f64, y: f64) -> Result<(f64, f64), VisualizationError> {
        Ok(self.map.local_to_data(Point::new(x, y))?)
    }

    pub fn layer(&self, layer: SpaceLayer) -> Option<&DrawableHandle> {
        self.layers.get(&layer)
    }

    pub fn move_to(self, x: f64, y: f64) -> Self {
        self.root.clone().move_to(x, y);
        self
    }

    pub fn scale_to(self, factor: f64) -> Self {
        self.root.clone().scale_to(factor);
        self
    }

    pub fn rotate_to(self, radians: f64) -> Self {
        self.root.clone().rotate_to(radians);
        self
    }

    fn view_transform(
        &self,
        x_domain: (f64, f64),
        y_domain: (f64, f64),
    ) -> Result<(f64, f64, DVec3), VisualizationError> {
        if !matches!(self.map.x.scale(), Scale::Linear | Scale::Time)
            || !matches!(self.map.y.scale(), Scale::Linear | Scale::Time)
        {
            return Err(VisualizationError::UnsupportedAnimatedView);
        }
        if !x_domain.0.is_finite()
            || !x_domain.1.is_finite()
            || !y_domain.0.is_finite()
            || !y_domain.1.is_finite()
            || x_domain.0 >= x_domain.1
            || y_domain.0 >= y_domain.1
        {
            return Err(VisualizationError::InvalidSize);
        }
        let original_x = self.map.x.domain();
        let original_y = self.map.y.domain();
        let scale_x = (original_x.1 - original_x.0) / (x_domain.1 - x_domain.0);
        let scale_y = (original_y.1 - original_y.0) / (y_domain.1 - y_domain.0);
        let center = self.map.data_to_local(
            (x_domain.0 + x_domain.1) * 0.5,
            (y_domain.0 + y_domain.1) * 0.5,
        )?;
        Ok((
            scale_x,
            scale_y,
            DVec3::new(-center.x * scale_x, -center.y * scale_y, 0.0),
        ))
    }

    /// Change the affine view window immediately at the current cursor.
    pub fn view_to(
        &self,
        x_domain: (f64, f64),
        y_domain: (f64, f64),
    ) -> Result<(), VisualizationError> {
        let (scale_x, scale_y, center) = self.view_transform(x_domain, y_domain)?;
        self.view.clone().scale_to_3d(scale_x, scale_y, 1.0);
        self.view.clone().move_to(center.x, center.y);
        Ok(())
    }

    /// Describe an affine view-window animation without touching the timeline.
    pub fn view_to_animation(
        &self,
        x_domain: (f64, f64),
        y_domain: (f64, f64),
    ) -> Result<super::types::Anim, VisualizationError> {
        let (scale_x, scale_y, center) = self.view_transform(x_domain, y_domain)?;
        Ok(self
            .view
            .animate()
            .scale_to_3d(scale_x, scale_y, 1.0)
            .move_to(center.x, center.y))
    }
}

impl DrawableHandle {
    pub fn at_coordinate(self, coordinate: CoordinateRef) -> Self {
        let mut state = self.state.lock().expect("canvas state poisoned");
        state.active_mut().ops.push(Op::PlaceAtCoordinate {
            space: coordinate.space,
            target: self.id,
            local: coordinate.local,
        });
        drop(state);
        self
    }
}

impl SceneModel {
    /// Create an animatable native scalar for reactive expressions.
    pub fn parameter(&mut self, initial: f64) -> Result<Parameter, VisualizationError> {
        if !initial.is_finite() {
            return Err(VisualizationError::InvalidParameter);
        }
        let handle = self.value_tracker(initial);
        let value = Arc::new(Mutex::new(initial));
        self.state
            .lock()
            .expect("canvas state poisoned")
            .parameter_values
            .insert(handle.id, value.clone());
        Ok(Parameter { handle, value })
    }

    /// Resolve the live mirrors required by a reactive callback evaluator.
    #[doc(hidden)]
    pub fn expression_parameter_values(
        &self,
        ids: &[ObjectId],
    ) -> Option<Vec<(ObjectId, Arc<Mutex<f64>>)>> {
        let state = self.state.lock().expect("canvas state poisoned");
        ids.iter()
            .map(|id| {
                state
                    .parameter_values
                    .get(id)
                    .cloned()
                    .map(|value| (*id, value))
            })
            .collect()
    }

    /// Materialize an immutable declarative chart into stable semantic layers.
    pub fn chart(&mut self, spec: ChartSpec) -> Result<ChartHandle, VisualizationError> {
        spec.validate()?;
        let mut axes = spec.resolved_axes()?;
        if spec.mark_spec().kind == MarkKind::Histogram && !axes.contains_key(&Channel::Y) {
            let column =
                chart_field(&spec, Channel::X).or_else(|_| chart_field(&spec, Channel::Y))?;
            let values: Vec<f64> = spec
                .data()
                .numeric_column(&column)
                .map_err(gaanim_visualization::ChartError::from)?
                .iter()
                .flatten()
                .copied()
                .collect();
            let bins = chart_option_usize(&spec, "bins", 20);
            let maximum = histogram(&values, bins)
                .and_then(|result| result.counts.into_iter().max())
                .unwrap_or(1)
                .max(1) as f64;
            axes.insert(Channel::Y, Axis::linear(0.0, maximum * 1.1)?);
        }
        let dimensions = spec.batch()?.dimensions;
        let width = (self.frame.width * 0.72).max(4.0);
        let height = (self.frame.height * 0.62).max(3.0);
        let guides = if spec.guides_specs().is_empty() {
            None
        } else {
            let titles: Vec<String> = spec
                .guides_specs()
                .values()
                .filter_map(|guide| match guide {
                    gaanim_visualization::GuideSpec::None => None,
                    gaanim_visualization::GuideSpec::Legend { title }
                    | gaanim_visualization::GuideSpec::ColorBar { title } => title.clone(),
                })
                .collect();
            if titles.is_empty() {
                None
            } else {
                let labels: Vec<_> = titles
                    .iter()
                    .enumerate()
                    .map(|(index, title)| {
                        self.text(title)
                            .move_to(width * 0.5 + 0.7, height * 0.5 - index as f64 * 0.34)
                    })
                    .collect();
                let refs: Vec<_> = labels.iter().collect();
                Some(self.group_no_center(&refs))
            }
        };

        if dimensions == 2 {
            let x = axes
                .get(&Channel::X)
                .cloned()
                .unwrap_or(Axis::linear(0.0, 1.0)?);
            let y = axes
                .get(&Channel::Y)
                .cloned()
                .unwrap_or(Axis::linear(0.0, 1.0)?);
            let space = self.coordinate_axes(x, y, Some(width), Some(height), true)?;
            if spec.mark_spec().kind == MarkKind::Bar {
                let (marks, labels) =
                    self.materialize_bar_chart(&space, &spec, chart_series_color(self))?;
                for layer in [
                    SpaceLayer::Axes,
                    SpaceLayer::Ticks,
                    SpaceLayer::Numbers,
                    SpaceLayer::Labels,
                ] {
                    if let Some(handle) = space.layer(layer) {
                        handle.clone().z_index(20);
                    }
                }
                let axes_layer = space.view.clone().z_index(20);
                let grid = space
                    .layer(SpaceLayer::MajorGrid)
                    .cloned()
                    .map(|layer| layer.z_index(-20));
                if let Some(guide) = &guides {
                    self.attach_to_space(&space, guide);
                }
                return Ok(ChartHandle {
                    root: space.drawable().clone(),
                    marks,
                    axes: axes_layer,
                    grid,
                    guides,
                    labels,
                    spec,
                });
            }
            let source = gaanim_visualization::DataSource::new(spec.data().clone());
            let mark = match spec.mark_spec().kind {
                MarkKind::Point => self.data_mark(
                    &space,
                    source,
                    DataMarkKind::Scatter {
                        x: chart_field(&spec, Channel::X)?,
                        y: chart_field(&spec, Channel::Y)?,
                        radius: chart_option_number(
                            &spec,
                            "radius",
                            chart_encoding_number(&spec, Channel::Size, 0.06),
                        ),
                        policy: NonFinitePolicy::Gap,
                    },
                )?,
                MarkKind::Line => self.data_mark(
                    &space,
                    source,
                    DataMarkKind::Line {
                        x: chart_field(&spec, Channel::X)?,
                        y: chart_field(&spec, Channel::Y)?,
                        policy: NonFinitePolicy::Gap,
                    },
                )?,
                MarkKind::Step => self.data_mark(
                    &space,
                    source,
                    DataMarkKind::Step {
                        x: chart_field(&spec, Channel::X)?,
                        y: chart_field(&spec, Channel::Y)?,
                        policy: NonFinitePolicy::Gap,
                    },
                )?,
                MarkKind::Area => self.data_mark(
                    &space,
                    source,
                    DataMarkKind::Area {
                        x: chart_field(&spec, Channel::X)?,
                        y: chart_field(&spec, Channel::Y)?,
                        baseline: chart_option_number(&spec, "baseline", 0.0),
                    },
                )?,
                MarkKind::Bar => self.data_mark(
                    &space,
                    source,
                    DataMarkKind::Bars {
                        x: chart_field(&spec, Channel::X)?,
                        y: chart_field(&spec, Channel::Y)?,
                        width: chart_option_number(&spec, "width", 0.8),
                        baseline: chart_option_number(&spec, "baseline", 0.0),
                    },
                )?,
                MarkKind::Histogram => self.data_mark(
                    &space,
                    source,
                    DataMarkKind::Histogram {
                        column: chart_field(&spec, Channel::X)
                            .or_else(|_| chart_field(&spec, Channel::Y))?,
                        bins: chart_option_usize(&spec, "bins", 20),
                    },
                )?,
                MarkKind::Box => self.data_mark(
                    &space,
                    source,
                    DataMarkKind::Box {
                        column: chart_field(&spec, Channel::Y)
                            .or_else(|_| chart_field(&spec, Channel::X))?,
                        center: chart_option_number(&spec, "center", 0.0),
                        width: chart_option_number(&spec, "width", 0.8),
                    },
                )?,
                MarkKind::Violin => self.data_mark(
                    &space,
                    source,
                    DataMarkKind::Violin {
                        column: chart_field(&spec, Channel::Y)
                            .or_else(|_| chart_field(&spec, Channel::X))?,
                        center: chart_option_number(&spec, "center", 0.0),
                        bandwidth: chart_option_number(&spec, "bandwidth", 0.3),
                        width: chart_option_number(&spec, "width", 0.8),
                    },
                )?,
                MarkKind::ErrorBar => self.data_mark(
                    &space,
                    source,
                    DataMarkKind::ErrorBars {
                        x: chart_field(&spec, Channel::X)?,
                        y: chart_field(&spec, Channel::Y)?,
                        low: chart_option_field(&spec, "low")?,
                        high: chart_option_field(&spec, "high")?,
                        cap_width: chart_option_number(&spec, "cap_width", 0.12),
                    },
                )?,
                MarkKind::Heatmap => self.heatmap_plot(
                    &space,
                    source,
                    chart_field(&spec, Channel::X)?,
                    chart_field(&spec, Channel::Y)?,
                    chart_field(&spec, Channel::Color)?,
                    [
                        chart_option_number(&spec, "cell_width", 1.0),
                        chart_option_number(&spec, "cell_height", 1.0),
                    ],
                    chart_option_usize(&spec, "bands", 12),
                )?,
                MarkKind::Surface => {
                    return Err(VisualizationError::UnsupportedChartMark3D(
                        MarkKind::Surface,
                    ));
                }
            };
            let mark = match spec.mark_spec().kind {
                // A chart without an explicit color encoding still needs a
                // visible semantic series color. Heatmap owns one fill per
                // quantized band, so its derived gradient must remain intact.
                MarkKind::Heatmap => mark,
                MarkKind::Line | MarkKind::Step | MarkKind::ErrorBar => mark.stroke(
                    chart_encoding_color(&spec).unwrap_or_else(|| chart_series_color(self)),
                    0.03,
                ),
                _ => mark
                    .fill(chart_encoding_color(&spec).unwrap_or_else(|| chart_series_color(self))),
            };
            let mark = mark
                .opacity(chart_encoding_number(&spec, Channel::Opacity, 1.0).clamp(0.0, 1.0) as f32)
                .z_index(0);
            for layer in [
                SpaceLayer::Axes,
                SpaceLayer::Ticks,
                SpaceLayer::Numbers,
                SpaceLayer::Labels,
            ] {
                if let Some(handle) = space.layer(layer) {
                    handle.clone().z_index(20);
                }
            }
            let axes_layer = space.view.clone().z_index(20);
            let grid = space
                .layer(SpaceLayer::MajorGrid)
                .cloned()
                .map(|layer| layer.z_index(-20));
            if let Some(guide) = &guides {
                self.attach_to_space(&space, guide);
            }
            Ok(ChartHandle {
                root: space.drawable().clone(),
                marks: mark,
                axes: axes_layer,
                grid,
                guides,
                spec,
                labels: None,
            })
        } else {
            let x = axes
                .get(&Channel::X)
                .cloned()
                .unwrap_or(Axis::linear(0.0, 1.0)?);
            let y = axes
                .get(&Channel::Y)
                .cloned()
                .unwrap_or(Axis::linear(0.0, 1.0)?);
            let z = axes
                .get(&Channel::Z)
                .cloned()
                .unwrap_or(Axis::linear(0.0, 1.0)?);
            let size = [10.0, 8.0, 6.0];
            let space = self.coordinate_axes_3d(x, y, z, size, true)?;
            let batch = spec.batch()?;
            let color = chart_series_color(self);
            let mark = match spec.mark_spec().kind {
                MarkKind::Point => {
                    let mut vertices = Vec::with_capacity(batch.data.len() * 6);
                    let mut indices = Vec::with_capacity(batch.data.len() * 24);
                    let mut colors = Vec::with_capacity(batch.data.len() * 6);
                    for datum in &batch.data {
                        if datum.position.iter().all(|value| value.is_finite()) {
                            append_octahedron(
                                &mut vertices,
                                &mut indices,
                                normalized_local(datum.position, size),
                                (datum.size * 0.0125).max(0.025) as f32,
                            );
                            colors.extend(std::iter::repeat_n(
                                color_with_opacity(datum.color.unwrap_or(color), datum.opacity),
                                6,
                            ));
                        }
                    }
                    self.surface_mesh_with_colors(vertices, indices, colors)
                }
                MarkKind::Line => {
                    let mut points = Vec::with_capacity(batch.data.len());
                    let mut colors = Vec::with_capacity(batch.data.len());
                    for datum in &batch.data {
                        if datum.position.iter().all(|value| value.is_finite()) {
                            points.push(normalized_local(datum.position, size));
                            colors.push(color_with_opacity(
                                datum.color.unwrap_or(color),
                                datum.opacity,
                            ));
                        }
                    }
                    self.polyline_3d_with_colors(points, colors)
                }
                MarkKind::Bar => {
                    let mut vertices = Vec::with_capacity(batch.data.len() * 8);
                    let mut indices = Vec::with_capacity(batch.data.len() * 36);
                    let mut colors = Vec::with_capacity(batch.data.len() * 8);
                    for datum in &batch.data {
                        let center = normalized_local(datum.position, size);
                        let half_x =
                            chart_option_number(&spec, "width", 0.08) as f32 * size[0] as f32 * 0.5;
                        let half_y =
                            chart_option_number(&spec, "depth", 0.08) as f32 * size[1] as f32 * 0.5;
                        append_box(
                            &mut vertices,
                            &mut indices,
                            [
                                center[0] - half_x,
                                center[1] - half_y,
                                -size[2] as f32 * 0.5,
                            ],
                            [center[0] + half_x, center[1] + half_y, center[2]],
                        );
                        colors.extend(std::iter::repeat_n(
                            color_with_opacity(datum.color.unwrap_or(color), datum.opacity),
                            8,
                        ));
                    }
                    self.surface_mesh_with_colors(vertices, indices, colors)
                }
                MarkKind::Surface | MarkKind::Heatmap => {
                    let mut xs: Vec<f64> =
                        batch.data.iter().map(|datum| datum.position[0]).collect();
                    let mut ys: Vec<f64> =
                        batch.data.iter().map(|datum| datum.position[1]).collect();
                    xs.sort_by(f64::total_cmp);
                    ys.sort_by(f64::total_cmp);
                    xs.dedup_by(|left, right| left.to_bits() == right.to_bits());
                    ys.dedup_by(|left, right| left.to_bits() == right.to_bits());
                    let mut vertices = Vec::with_capacity(batch.data.len());
                    let mut lookup = BTreeMap::new();
                    for datum in &batch.data {
                        let index = vertices.len() as u32;
                        vertices.push(normalized_local(datum.position, size));
                        lookup.insert(
                            (datum.position[0].to_bits(), datum.position[1].to_bits()),
                            index,
                        );
                    }
                    let mut indices = Vec::new();
                    for y_pair in ys.windows(2) {
                        for x_pair in xs.windows(2) {
                            let keys = [
                                (x_pair[0].to_bits(), y_pair[0].to_bits()),
                                (x_pair[1].to_bits(), y_pair[0].to_bits()),
                                (x_pair[0].to_bits(), y_pair[1].to_bits()),
                                (x_pair[1].to_bits(), y_pair[1].to_bits()),
                            ];
                            if let [Some(a), Some(b), Some(c), Some(d)] =
                                keys.map(|key| lookup.get(&key).copied())
                            {
                                indices.extend_from_slice(&[a, b, d, a, d, c]);
                            }
                        }
                    }
                    let colors = vertices
                        .iter()
                        .zip(&batch.data)
                        .map(|(vertex, datum)| {
                            let generated = || {
                                let t = (vertex[2] / size[2] as f32 + 0.5).clamp(0.0, 1.0);
                                Color::from_rgb8(
                                    (40.0 + t * 200.0) as u8,
                                    (80.0 + (1.0 - t) * 100.0) as u8,
                                    (220.0 - t * 140.0) as u8,
                                )
                            };
                            color_with_opacity(datum.color.unwrap_or_else(generated), datum.opacity)
                        })
                        .collect();
                    self.surface_mesh_with_colors(vertices, indices, colors)
                }
                kind => return Err(VisualizationError::UnsupportedChartMark3D(kind)),
            };
            self.attach_to_space_3d(&space, &mark);
            let axes_layer = space
                .layer(SpaceLayer::Axes)
                .cloned()
                .unwrap_or_else(|| space.drawable().clone());
            let grid = space.layer(SpaceLayer::MajorGrid).cloned();
            Ok(ChartHandle {
                root: space.drawable().clone(),
                marks: mark,
                axes: axes_layer,
                grid,
                guides,
                spec,
                labels: None,
            })
        }
    }

    /// Build categorical and numeric bars from the canonical batch.  A path
    /// is retained for each resolved fill colour, avoiding an entity per row
    /// while still honouring a per-datum colour encoding.
    fn materialize_bar_chart(
        &mut self,
        space: &CoordinateSpaceHandle,
        spec: &ChartSpec,
        default_color: Color,
    ) -> Result<(DrawableHandle, Option<DrawableHandle>), VisualizationError> {
        let batch = spec.batch()?;
        let width = chart_option_number(spec, "width", 0.8);
        let baseline = chart_option_number(spec, "baseline", 0.0);
        let baseline_normalized = space.map.y.normalize(baseline)?;
        let label_position = match spec.mark_spec().options.get("label_position") {
            Some(ConstantValue::Text(value)) => value.as_str(),
            _ => "outside",
        };
        let label_offset = chart_option_number(spec, "label_offset", 0.16);
        let label_color = match spec.mark_spec().options.get("label_color") {
            Some(ConstantValue::Color(color)) => Some(*color),
            _ => None,
        };
        let mut paths: BTreeMap<[u8; 4], (Color, BezPath)> = BTreeMap::new();
        let mut labels = Vec::new();
        let frame = space.map.frame;
        let category_band = match space.map.x.scale() {
            Scale::Category { values } => Some(frame.width / values.len() as f64),
            _ => None,
        };

        for datum in &batch.data {
            if !datum.position[0].is_finite() || !datum.position[1].is_finite() {
                continue;
            }
            let center_x = (datum.position[0] - 0.5) * frame.width;
            let center_y = (datum.position[1] - 0.5) * frame.height;
            let half_width = if let Some(band) = category_band {
                band * width * 0.5
            } else {
                let value = space.map.x.denormalize(datum.position[0])?;
                let left = space.map.x.normalize(value - width * 0.5)?;
                let right = space.map.x.normalize(value + width * 0.5)?;
                (right - left).abs() * frame.width * 0.5
            };
            let baseline_y = (baseline_normalized - 0.5) * frame.height;
            let color = color_with_opacity(datum.color.unwrap_or(default_color), datum.opacity);
            let rgba = color.to_rgba8();
            let path = paths
                .entry([rgba.r, rgba.g, rgba.b, rgba.a])
                .or_insert_with(|| (color, BezPath::new()));
            path.1.extend(
                Rect::new(
                    center_x - half_width,
                    baseline_y.min(center_y),
                    center_x + half_width,
                    baseline_y.max(center_y),
                )
                .to_path(0.1),
            );

            if let Some(text) = &datum.label {
                let sign = if datum.position[1] >= baseline_normalized {
                    1.0
                } else {
                    -1.0
                };
                let direction = if label_position == "inside" {
                    -sign
                } else {
                    sign
                };
                labels.push(
                    self.text(text)
                        .fill(label_color.unwrap_or(color))
                        .move_to(center_x, center_y + direction * label_offset)
                        .z_index(10),
                );
            }
        }

        let mut mark_paths = Vec::with_capacity(paths.len());
        for (_, (color, path)) in paths {
            let handle = self
                .visualization_path(path, frame.bounds(), color, 0.0, "ChartBars")
                .fill(color)
                .z_index(0);
            mark_paths.push(handle);
        }
        let mark_refs: Vec<_> = mark_paths.iter().collect();
        let marks = self.group_no_center(&mark_refs);
        self.attach_to_space(space, &marks);

        let labels = if labels.is_empty() {
            None
        } else {
            let label_refs: Vec<_> = labels.iter().collect();
            let labels = self.group_no_center(&label_refs);
            self.attach_to_space(space, &labels);
            Some(labels)
        };
        Ok((marks, labels))
    }

    fn visualization_path(
        &mut self,
        path: BezPath,
        bounds: gaanim_math::Bounds3D,
        color: Color,
        width: f64,
        name: &str,
    ) -> DrawableHandle {
        let path = SvgPath {
            id: name.to_owned(),
            path,
            bounds,
            fill: None,
            stroke: gaanim_scene::StrokeBrush::new(color, width),
        };
        let handle = self.spawn(SpawnKind::SvgPath(Box::new(path)));
        handle
            .spec
            .lock()
            .expect("visualization path spec poisoned")
            .theme_selector = Some("plot".into());
        handle
    }

    fn themed_axis_path(
        &mut self,
        path: BezPath,
        bounds: gaanim_math::Bounds3D,
        color: Color,
        width: f64,
        name: &str,
    ) -> DrawableHandle {
        let handle = self.visualization_path(path, bounds, color, width, name);
        handle
            .spec
            .lock()
            .expect("axis path spec poisoned")
            .theme_selector = None;
        handle
    }

    fn themed_axis(&self, axis: Axis) -> Axis {
        let Some(theme) = self.theme_style.as_ref() else {
            return axis;
        };
        let mut style = gaanim_visualization::AxisStyle {
            color: theme.palette.foreground,
            tick_color: theme.palette.foreground,
            number_color: theme.palette.foreground,
            label_color: theme.palette.foreground,
            ..Default::default()
        };
        if let Some(stroke) = theme
            .styles
            .get("axes/axis")
            .and_then(|rule| rule.stroke.as_ref())
        {
            if let Ok(gaanim_core::peniko::Brush::Solid(color)) = theme.resolve_paint(&stroke.paint)
            {
                style.color = color;
            }
            style.width = stroke.style.width;
        }
        if let Some(stroke) = theme
            .styles
            .get("axes/ticks")
            .and_then(|rule| rule.stroke.as_ref())
        {
            if let Ok(gaanim_core::peniko::Brush::Solid(color)) = theme.resolve_paint(&stroke.paint)
            {
                style.tick_color = color;
            }
            style.tick_width = stroke.style.width;
        }
        for (selector, target) in [
            ("axes/numbers", &mut style.number_color),
            ("axes/labels", &mut style.label_color),
        ] {
            if let Some(fill) = theme
                .styles
                .get(selector)
                .and_then(|rule| rule.fill.as_ref())
                && let Ok(gaanim_core::peniko::Brush::Solid(color)) = theme.resolve_paint(fill)
            {
                *target = color;
            }
        }
        axis.with_theme_style(style)
    }

    fn axis_text_size(&self, text: &str, scale: f64) -> (f64, f64) {
        let font_size =
            self.themed_text_config().roles[&gaanim_text::prelude::TextRole::Body].size * scale;
        let lines: Vec<&str> = text.lines().collect();
        let line_count = lines.len().max(1) as f64;
        let longest_line = lines
            .iter()
            .map(|line| line.chars().count())
            .max()
            .unwrap_or(0) as f64;
        // Axis text is intentionally measured conservatively here. Text shaping
        // happens later during compilation, but reserving 0.65em per character
        // keeps labels clear without creating a shaping pass for every tick.
        (
            longest_line * font_size * 0.65,
            line_count * font_size * 1.2,
        )
    }

    fn x_tick_label_extra_offset(&self, text: &str, scale: f64) -> f64 {
        const SINGLE_LINE_EXTRA: f64 = 0.04;
        let font_size =
            self.themed_text_config().roles[&gaanim_text::prelude::TextRole::Body].size * scale;
        let additional_lines = text.lines().count().saturating_sub(1) as f64;
        // Text is centered on its authored position. Shift it by half of each
        // added line so that a multiline label grows away from the axis while
        // its closest line keeps the same gap as a single-line label.
        SINGLE_LINE_EXTRA + additional_lines * font_size * 0.6
    }

    fn lay_out_cartesian_axis_labels(
        &self,
        space: &CartesianSpace,
        geometry: &mut SpaceGeometry2D,
        number_scale: f64,
        label_scale: f64,
    ) {
        const NUMBER_GAP: f64 = 0.12;
        const TITLE_GAP: f64 = 0.12;

        let x_cross = space.map.x.crossing_value();
        let y_cross = space.map.y.crossing_value();
        let Ok(axis_origin) = space.map.data_to_local(x_cross, y_cross) else {
            return;
        };
        let x_tick_labels: Vec<_> = space
            .map
            .x
            .ticks_values(7)
            .unwrap_or_default()
            .into_iter()
            .filter(|tick| space.visibility.x_numbers && tick.major && !tick.label.is_empty())
            .collect();
        let x_tick_height = x_tick_labels
            .iter()
            .map(|tick| self.axis_text_size(&tick.label, number_scale).1)
            .fold(0.0, f64::max);
        let x_tick_extra_offset = x_tick_labels
            .iter()
            .map(|tick| self.x_tick_label_extra_offset(&tick.label, number_scale))
            .fold(0.0, f64::max);
        let x_labels_direction = -1.0;
        for label in geometry.numbers.iter_mut().take(x_tick_labels.len()) {
            let distance = (label.position.y - axis_origin.y).abs()
                + self.x_tick_label_extra_offset(&label.text, number_scale);
            label.position.y = axis_origin.y + x_labels_direction * distance;
        }
        let y_tick_width = space
            .map
            .y
            .ticks_values(7)
            .unwrap_or_default()
            .into_iter()
            .filter(|tick| {
                space.visibility.y_numbers
                    && tick.major
                    && !tick.label.is_empty()
                    && tick.value != x_cross
            })
            .map(|tick| self.axis_text_size(&tick.label, number_scale).0)
            .fold(0.0, f64::max);
        geometry.labels.clear();
        if space.visibility.x_labels
            && let Some(label) = space.map.x.label_text()
        {
            let (_, height) = self.axis_text_size(label, label_scale);
            let position = space.map.x.label_position_value();
            let (x, y) = if position == AxisLabelPosition::Center {
                (
                    0.0,
                    axis_origin.y
                        - space.map.x.style_value().tick_length
                        - NUMBER_GAP
                        - x_tick_extra_offset
                        - x_tick_height * 0.5
                        - TITLE_GAP
                        - height * 0.5,
                )
            } else {
                (
                    axis_title_coordinate(position, space.map.frame.width, TITLE_GAP),
                    axis_origin.y,
                )
            };
            geometry.labels.push(gaanim_visualization::LabelGeometry {
                text: label.to_owned(),
                position: Point::new(x, y),
                rotation: 0.0,
                color: space.map.x.style_value().label_color,
            });
        }
        if space.visibility.y_labels
            && let Some(label) = space.map.y.label_text()
        {
            let (_, height) = self.axis_text_size(label, label_scale);
            let position = space.map.y.label_position_value();
            let (x, y, rotation) = if position == AxisLabelPosition::Center {
                (
                    axis_origin.x
                        - space.map.y.style_value().tick_length
                        - NUMBER_GAP
                        - y_tick_width
                        - TITLE_GAP
                        - height * 0.5,
                    0.0,
                    std::f64::consts::FRAC_PI_2,
                )
            } else {
                (
                    axis_origin.x,
                    axis_title_coordinate(position, space.map.frame.height, TITLE_GAP),
                    0.0,
                )
            };
            geometry.labels.push(gaanim_visualization::LabelGeometry {
                text: label.to_owned(),
                position: Point::new(x, y),
                rotation,
                color: space.map.y.style_value().label_color,
            });
        }
    }

    fn attach_to_space(&mut self, space: &CoordinateSpaceHandle, child: &DrawableHandle) {
        child
            .spec
            .lock()
            .expect("object spec poisoned")
            .exclude_from_parent_draw = true;
        self.state
            .lock()
            .expect("canvas state poisoned")
            .active_mut()
            .ops
            .push(Op::AttachToGroup {
                group: space.view.id,
                child: child.id,
            });
    }

    fn attach_to_space_3d(&mut self, space: &CoordinateSpace3DHandle, child: &DrawableHandle) {
        child
            .spec
            .lock()
            .expect("object spec poisoned")
            .exclude_from_parent_draw = true;
        self.state
            .lock()
            .expect("canvas state poisoned")
            .active_mut()
            .ops
            .push(Op::AttachToGroup {
                group: space.root.id,
                child: child.id,
            });
    }

    fn attach_to_number_line(&mut self, line: &NumberLineHandle, child: &DrawableHandle) {
        child
            .spec
            .lock()
            .expect("object spec poisoned")
            .exclude_from_parent_draw = true;
        self.state
            .lock()
            .expect("canvas state poisoned")
            .active_mut()
            .ops
            .push(Op::AttachToGroup {
                group: line.root.id,
                child: child.id,
            });
    }

    /// Build a typed 3D Cartesian coordinate space.
    pub fn coordinate_axes_3d(
        &mut self,
        x: Axis,
        y: Axis,
        z: Axis,
        size: [f64; 3],
        grid: bool,
    ) -> Result<CoordinateSpace3DHandle, VisualizationError> {
        self.coordinate_axes_3d_with_visibility(
            x,
            y,
            z,
            size,
            Cartesian3DVisibility {
                xy_grid: grid,
                xz_grid: grid,
                yz_grid: grid,
                ..Default::default()
            },
        )
    }

    /// Build a typed 3D Cartesian coordinate space with resolved component visibility.
    pub fn coordinate_axes_3d_with_visibility(
        &mut self,
        x: Axis,
        y: Axis,
        z: Axis,
        size: [f64; 3],
        visibility: Cartesian3DVisibility,
    ) -> Result<CoordinateSpace3DHandle, VisualizationError> {
        let x = self.themed_axis(x);
        let y = self.themed_axis(y);
        let z = self.themed_axis(z);
        let map = CoordinateMap3D::new(x.clone(), y.clone(), z.clone(), size)?;
        let min = [-size[0] * 0.5, -size[1] * 0.5, -size[2] * 0.5];
        let max = [size[0] * 0.5, size[1] * 0.5, size[2] * 0.5];
        let as_point = |point: [f64; 3]| [point[0] as f32, point[1] as f32, point[2] as f32];
        let mut axis_points = Vec::new();
        let mut axis_colors = Vec::new();
        let mut push_axis = |from: [f64; 3], to: [f64; 3], color: Color| {
            axis_points.extend_from_slice(&[as_point(from), as_point(to)]);
            axis_colors.extend_from_slice(&[color, color]);
        };
        if visibility.x_axis {
            push_axis(min, [max[0], min[1], min[2]], x.style_value().color);
        }
        if visibility.y_axis {
            push_axis(min, [min[0], max[1], min[2]], y.style_value().color);
        }
        if visibility.z_axis {
            push_axis(min, [min[0], min[1], max[2]], z.style_value().color);
        }
        let axes = self.line_segments_3d_with_colors(axis_points, axis_colors);

        let mut grid_points = Vec::new();
        let mut grid_colors = Vec::new();
        let mut tick_points = Vec::new();
        let mut tick_colors = Vec::new();
        let grid_color = self
            .theme_style
            .as_ref()
            .and_then(|theme| {
                theme
                    .styles
                    .get("axes/grid")
                    .and_then(|rule| rule.stroke.as_ref())
                    .and_then(|stroke| theme.resolve_paint(&stroke.paint).ok())
            })
            .and_then(|brush| match brush {
                gaanim_core::peniko::Brush::Solid(color) => Some(color),
                _ => None,
            })
            .unwrap_or(Color::from_rgb8(0x50, 0x50, 0x50));
        let tick_half = size.into_iter().fold(f64::INFINITY, f64::min) * 0.015;
        let mut numbers = Vec::new();
        let ticks_visible = [visibility.x_ticks, visibility.y_ticks, visibility.z_ticks];
        let numbers_visible = [
            visibility.x_numbers,
            visibility.y_numbers,
            visibility.z_numbers,
        ];
        for (dimension, axis) in [x.clone(), y.clone(), z.clone()].into_iter().enumerate() {
            for tick in axis.ticks_values(7)? {
                let normalized = axis.normalize(tick.value)?;
                let coordinate = min[dimension] + normalized * size[dimension];
                let tick_color = axis.style_value().tick_color;
                // The three axes already own the edges that start at the
                // minimum corner. Emitting the minimum tick's grid segments
                // duplicates those lines at exactly the same depth, causing
                // nondeterministic z-fighting when another 3D drawable enters
                // the transparent render phase.
                if tick.major && normalized > 1e-12 {
                    let mut lines = Vec::with_capacity(2);
                    match dimension {
                        0 => {
                            if visibility.xy_grid {
                                lines.push((
                                    [coordinate, min[1], min[2]],
                                    [coordinate, max[1], min[2]],
                                ));
                            }
                            if visibility.xz_grid {
                                lines.push((
                                    [coordinate, min[1], min[2]],
                                    [coordinate, min[1], max[2]],
                                ));
                            }
                        }
                        1 => {
                            if visibility.xy_grid {
                                lines.push((
                                    [min[0], coordinate, min[2]],
                                    [max[0], coordinate, min[2]],
                                ));
                            }
                            if visibility.yz_grid {
                                lines.push((
                                    [min[0], coordinate, min[2]],
                                    [min[0], coordinate, max[2]],
                                ));
                            }
                        }
                        _ => {
                            if visibility.xz_grid {
                                lines.push((
                                    [min[0], min[1], coordinate],
                                    [max[0], min[1], coordinate],
                                ));
                            }
                            if visibility.yz_grid {
                                lines.push((
                                    [min[0], min[1], coordinate],
                                    [min[0], max[1], coordinate],
                                ));
                            }
                        }
                    }
                    for (from, to) in lines {
                        grid_points.extend_from_slice(&[as_point(from), as_point(to)]);
                        // Use the opaque sRGB equivalent of the previous
                        // 50%-gray/38%-alpha tint over the default black
                        // background. Transparent line-list intersections
                        // have undefined fragment ordering on the GPU and
                        // produced a handful of alternating snapshot pixels.
                        grid_colors.extend_from_slice(&[grid_color, grid_color]);
                    }
                }
                let (from, to, label_position) = match dimension {
                    0 => (
                        [coordinate, min[1] - tick_half, min[2]],
                        [coordinate, min[1] + tick_half, min[2]],
                        [coordinate, min[1] - tick_half * 4.0, min[2]],
                    ),
                    1 => (
                        [min[0] - tick_half, coordinate, min[2]],
                        [min[0] + tick_half, coordinate, min[2]],
                        [min[0] - tick_half * 4.0, coordinate, min[2]],
                    ),
                    _ => (
                        [min[0] - tick_half, min[1], coordinate],
                        [min[0] + tick_half, min[1], coordinate],
                        [min[0] - tick_half * 4.0, min[1], coordinate],
                    ),
                };
                if ticks_visible[dimension] {
                    tick_points.extend_from_slice(&[as_point(from), as_point(to)]);
                    tick_colors.extend_from_slice(&[tick_color, tick_color]);
                }
                if numbers_visible[dimension] && tick.major && !tick.label.is_empty() {
                    numbers.push(
                        self.text(&tick.label)
                            .fill(axis.style_value().number_color)
                            .move_to_3d(label_position[0], label_position[1], label_position[2])
                            .billboard()
                            .scale_to(0.016),
                    );
                }
            }
        }
        let grid_handle = self.line_segments_3d_with_colors(grid_points, grid_colors);
        let ticks = self.line_segments_3d_with_colors(tick_points, tick_colors);
        let number_refs: Vec<_> = numbers.iter().collect();
        let numbers = self.group_no_center(&number_refs);
        let mut labels = Vec::new();
        for (axis, position, visible) in [
            (
                &x,
                [max[0] + tick_half * 5.0, min[1], min[2]],
                visibility.x_labels,
            ),
            (
                &y,
                [min[0], max[1] + tick_half * 5.0, min[2]],
                visibility.y_labels,
            ),
            (
                &z,
                [min[0], min[1], max[2] + tick_half * 5.0],
                visibility.z_labels,
            ),
        ] {
            if visible && let Some(label) = axis.label_text() {
                labels.push(
                    self.text(label)
                        .fill(axis.style_value().label_color)
                        .move_to_3d(position[0], position[1], position[2])
                        .billboard()
                        .scale_to(0.019),
                );
            }
        }
        let label_refs: Vec<_> = labels.iter().collect();
        let labels = self.group_no_center(&label_refs);
        let root = self.group_no_center(&[&grid_handle, &axes, &ticks, &numbers, &labels]);
        let layers = HashMap::from([
            (SpaceLayer::MajorGrid, grid_handle),
            (SpaceLayer::Axes, axes),
            (SpaceLayer::Ticks, ticks),
            (SpaceLayer::Numbers, numbers),
            (SpaceLayer::Labels, labels),
        ]);
        Ok(CoordinateSpace3DHandle { root, map, layers })
    }

    /// Sample a static Python/Rust callable once into a retained 3D mesh.
    pub fn surface_plot(
        &mut self,
        space: &CoordinateSpace3DHandle,
        resolution: [usize; 2],
        evaluator: impl FnMut(f64, f64) -> Option<f64>,
    ) -> Result<DrawableHandle, VisualizationError> {
        let mesh = sample_surface(&space.map, resolution, evaluator)?;
        let finite: Vec<f64> = mesh
            .values
            .iter()
            .copied()
            .filter(|value| value.is_finite())
            .collect();
        let minimum = finite.iter().copied().reduce(f64::min).unwrap_or(0.0);
        let maximum = finite.iter().copied().reduce(f64::max).unwrap_or(1.0);
        let span = (maximum - minimum).max(f64::EPSILON);
        let colors: Vec<Color> = mesh
            .values
            .iter()
            .map(|value| {
                let t = if value.is_finite() {
                    ((value - minimum) / span).clamp(0.0, 1.0)
                } else {
                    0.0
                };
                Color::from_rgba8(
                    (0x20 as f64 + t * 0xD0 as f64) as u8,
                    (0x60 as f64 + (1.0 - (2.0 * t - 1.0).abs()) * 0x90 as f64) as u8,
                    (0xD0 as f64 - t * 0x90 as f64) as u8,
                    if value.is_finite() { 0xFF } else { 0 },
                )
            })
            .collect();
        let mut wire_points = Vec::new();
        let mut wire_colors = Vec::new();
        let index = |x: usize, y: usize| y * resolution[0] + x;
        let mut edge = |from: usize, to: usize| {
            let start = mesh.vertices[from];
            let end = mesh.vertices[to];
            if start.iter().all(|value| value.is_finite())
                && end.iter().all(|value| value.is_finite())
            {
                wire_points.extend_from_slice(&[start, end]);
                wire_colors.extend_from_slice(&[colors[from], colors[to]]);
            }
        };
        for y in 0..resolution[1] {
            for x in 0..resolution[0] {
                if x + 1 < resolution[0] {
                    edge(index(x, y), index(x + 1, y));
                }
                if y + 1 < resolution[1] {
                    edge(index(x, y), index(x, y + 1));
                }
            }
        }

        let fill = self.surface_mesh_with_colors(mesh.vertices, mesh.indices, colors);
        let wire = self.line_segments_3d_with_colors(wire_points, wire_colors);
        let handle = self.group(&[&fill, &wire]);
        self.attach_to_space_3d(space, &handle);
        Ok(handle)
    }

    /// Sample a static parametric 3D curve once into a retained polyline.
    pub fn parametric_plot_3d(
        &mut self,
        space: &CoordinateSpace3DHandle,
        domain: (f64, f64),
        samples: usize,
        mut evaluator: impl FnMut(f64) -> Option<[f64; 3]>,
    ) -> Result<DrawableHandle, VisualizationError> {
        if samples < 2 || !domain.0.is_finite() || !domain.1.is_finite() || domain.0 >= domain.1 {
            return Err(gaanim_visualization::SamplingError::InvalidDomain.into());
        }
        let mut points = Vec::with_capacity(samples);
        for index in 0..samples {
            let progress = index as f64 / (samples - 1) as f64;
            let parameter = domain.0 + (domain.1 - domain.0) * progress;
            if let Some(point) =
                evaluator(parameter).and_then(|point| space.map.data_to_local(point).ok())
            {
                points.push([point[0] as f32, point[1] as f32, point[2] as f32]);
            }
        }
        if points.len() < 2 {
            return Err(gaanim_visualization::SamplingError::TooFewSamples.into());
        }
        let handle = self.polyline_3d(points);
        self.attach_to_space_3d(space, &handle);
        Ok(handle)
    }

    /// Build Cartesian axes with typed reusable axis specs.
    pub fn coordinate_axes(
        &mut self,
        x: Axis,
        y: Axis,
        width: Option<f64>,
        height: Option<f64>,
        grid: bool,
    ) -> Result<CoordinateSpaceHandle, VisualizationError> {
        self.coordinate_axes_with_visibility(
            x,
            y,
            width,
            height,
            CartesianVisibility {
                x_grid: grid,
                y_grid: grid,
                ..Default::default()
            },
        )
    }

    /// Build Cartesian axes with resolved global and per-axis visibility.
    pub fn coordinate_axes_with_visibility(
        &mut self,
        x: Axis,
        y: Axis,
        width: Option<f64>,
        height: Option<f64>,
        visibility: CartesianVisibility,
    ) -> Result<CoordinateSpaceHandle, VisualizationError> {
        let x = self.themed_axis(x);
        let y = self.themed_axis(y);
        let themed_grid_color = self.theme_style.as_ref().map(|theme| {
            let grid_color = theme
                .styles
                .get("axes/grid")
                .and_then(|style| style.stroke.as_ref())
                .and_then(|stroke| theme.resolve_paint(&stroke.paint).ok())
                .and_then(|brush| match brush {
                    gaanim_core::peniko::Brush::Solid(color) => Some(color),
                    _ => None,
                })
                .unwrap_or(theme.palette.rule);
            grid_color
        });
        let safe = self.safe_frame();
        let frame = PlotFrame::new(
            width.unwrap_or_else(|| safe.width()),
            height.unwrap_or_else(|| safe.height()),
        )?;
        let mut space = CartesianSpace::axes(x, y, frame).with_visibility(visibility);
        if let Some(grid_color) = themed_grid_color {
            space.grid_color = grid_color;
            let rgba = grid_color.to_rgba8();
            space.minor_grid_color = Color::from_rgba8(rgba.r, rgba.g, rgba.b, rgba.a / 2);
        }
        let mut geometry = space.geometry()?;
        let number_scale = self
            .theme_style
            .as_ref()
            .and_then(|theme| theme.styles.get("axes/numbers"))
            .and_then(|style| style.text.as_ref())
            .and_then(|style| style.size)
            .map(|size| {
                size / self.themed_text_config().roles[&gaanim_text::prelude::TextRole::Body].size
            })
            .unwrap_or(1.0);
        let label_scale = self
            .theme_style
            .as_ref()
            .and_then(|theme| theme.styles.get("axes/labels"))
            .and_then(|style| style.text.as_ref())
            .and_then(|style| style.size)
            .map(|size| {
                size / self.themed_text_config().roles[&gaanim_text::prelude::TextRole::Body].size
            })
            .unwrap_or(1.125);
        self.lay_out_cartesian_axis_labels(&space, &mut geometry, number_scale, label_scale);
        let mut layers = HashMap::new();
        let grid_major = self.visualization_path(
            geometry.major_grid,
            geometry.bounds,
            space.grid_color,
            0.01,
            "CoordinateMajorGrid",
        );
        grid_major
            .spec
            .lock()
            .expect("grid spec poisoned")
            .theme_selector = Some("axes/grid".into());
        layers.insert(SpaceLayer::MajorGrid, grid_major.clone());
        let grid_minor = self.visualization_path(
            geometry.minor_grid,
            geometry.bounds,
            space.minor_grid_color,
            0.006,
            "CoordinateMinorGrid",
        );
        grid_minor
            .spec
            .lock()
            .expect("minor grid spec poisoned")
            .theme_selector = Some("axes/minor_grid".into());
        layers.insert(SpaceLayer::MinorGrid, grid_minor.clone());
        let axis_color = space.map.x.style_value().color;
        let axes = self.themed_axis_path(
            geometry.axes,
            geometry.bounds,
            axis_color,
            space.map.x.style_value().width,
            "CoordinateAxes",
        );
        layers.insert(SpaceLayer::Axes, axes.clone());
        let ticks = self.themed_axis_path(
            geometry.ticks,
            geometry.bounds,
            space.map.x.style_value().tick_color,
            space.map.x.style_value().tick_width,
            "CoordinateTicks",
        );
        layers.insert(SpaceLayer::Ticks, ticks.clone());

        let number_handles: Vec<DrawableHandle> = geometry
            .numbers
            .iter()
            .map(|label| {
                self.text(&label.text)
                    .fill(label.color)
                    .scale_to(number_scale)
                    .move_to(label.position.x, label.position.y)
            })
            .collect();
        let number_refs: Vec<&DrawableHandle> = number_handles.iter().collect();
        let numbers = self.group(&number_refs);
        layers.insert(SpaceLayer::Numbers, numbers.clone());

        let mut label_handles = Vec::with_capacity(geometry.labels.len());
        let mut label_index = 0;
        if space.visibility.x_labels && space.map.x.label_text().is_some() {
            let label = &geometry.labels[label_index];
            let handle = self
                .text(&label.text)
                .fill(label.color)
                .scale_to(label_scale);
            let handle = match space.map.x.label_position_value() {
                AxisLabelPosition::Start => {
                    handle.at_anchor(label.position.x, label.position.y, Anchor::Right)
                }
                AxisLabelPosition::Center => handle.move_to(label.position.x, label.position.y),
                AxisLabelPosition::End => {
                    handle.at_anchor(label.position.x, label.position.y, Anchor::Left)
                }
            };
            label_handles.push(handle.rotate_to(label.rotation));
            label_index += 1;
        }
        if space.visibility.y_labels && space.map.y.label_text().is_some() {
            let label = &geometry.labels[label_index];
            let handle = self
                .text(&label.text)
                .fill(label.color)
                .scale_to(label_scale);
            let handle = match space.map.y.label_position_value() {
                AxisLabelPosition::Start => {
                    handle.at_anchor(label.position.x, label.position.y, Anchor::Top)
                }
                AxisLabelPosition::Center => handle.move_to(label.position.x, label.position.y),
                AxisLabelPosition::End => {
                    handle.at_anchor(label.position.x, label.position.y, Anchor::Bottom)
                }
            };
            label_handles.push(handle.rotate_to(label.rotation));
        }
        let label_refs: Vec<&DrawableHandle> = label_handles.iter().collect();
        let labels = self.group(&label_refs);
        layers.insert(SpaceLayer::Labels, labels.clone());

        let members = [&grid_major, &grid_minor, &axes, &ticks, &numbers, &labels];
        let view = self.group_no_center(&members);
        let root = self.group_no_center(&[&view]);
        Ok(CoordinateSpaceHandle {
            root,
            view,
            map: space.map,
            layers,
        })
    }

    pub fn coordinate_number_line(
        &mut self,
        axis: Axis,
        length: Option<f64>,
    ) -> Result<NumberLineHandle, VisualizationError> {
        self.coordinate_number_line_with_visibility(axis, length, NumberLineVisibility::default())
    }

    pub fn coordinate_number_line_with_visibility(
        &mut self,
        axis: Axis,
        length: Option<f64>,
        visibility: NumberLineVisibility,
    ) -> Result<NumberLineHandle, VisualizationError> {
        let axis = self.themed_axis(axis);
        let length = length.unwrap_or_else(|| self.safe_frame().width());
        let line = NumberLine::new(axis.clone(), length)?;
        let style = axis.style_value();
        let body_size = self.themed_text_config().roles[&gaanim_text::prelude::TextRole::Body].size;
        let number_scale = self
            .theme_style
            .as_ref()
            .and_then(|theme| theme.styles.get("axes/numbers"))
            .and_then(|style| style.text.as_ref())
            .and_then(|style| style.size)
            .map(|size| size / body_size)
            .unwrap_or(1.0);
        let label_scale = self
            .theme_style
            .as_ref()
            .and_then(|theme| theme.styles.get("axes/labels"))
            .and_then(|style| style.text.as_ref())
            .and_then(|style| style.size)
            .map(|size| size / body_size)
            .unwrap_or(1.125);
        let mut axis_path = BezPath::new();
        if visibility.axis {
            axis_path.move_to(Point::new(-length * 0.5, 0.0));
            axis_path.line_to(Point::new(length * 0.5, 0.0));
        }
        let bounds = gaanim_math::Bounds3D::new_2d(
            -length * 0.5,
            -style.tick_length - 0.56,
            length * 0.5,
            style.tick_length + 0.32,
        );
        let axis_handle = self.themed_axis_path(
            axis_path,
            bounds,
            style.tick_color,
            style.width,
            "NumberLineAxis",
        );
        let mut tick_path = BezPath::new();
        let mut number_handles = Vec::new();
        for tick in axis.ticks_values(9)? {
            let x = line.data_to_local(tick.value)?;
            let half = style.tick_length * if tick.major { 0.5 } else { 0.3 };
            if visibility.ticks {
                tick_path.move_to(Point::new(x, -half));
                tick_path.line_to(Point::new(x, half));
            }
            if visibility.numbers && tick.major && !tick.label.is_empty() {
                number_handles.push(
                    self.text(&tick.label)
                        .fill(style.number_color)
                        .scale_to(number_scale)
                        .move_to(x, -style.tick_length - 0.22),
                );
            }
        }
        let ticks = self.themed_axis_path(
            tick_path,
            bounds,
            style.color,
            style.tick_width,
            "NumberLineTicks",
        );
        let number_refs: Vec<&DrawableHandle> = number_handles.iter().collect();
        let numbers = self.group(&number_refs);
        let labels = if visibility.labels
            && let Some(label) = axis.label_text()
        {
            let label = self
                .text(label)
                .fill(style.label_color)
                .scale_to(label_scale)
                .move_to(length * 0.5 + 0.30, 0.0);
            self.group(&[&label])
        } else {
            self.group(&[])
        };
        // A coordinate space must preserve its authored origin. Centering this
        // group would make `coord(0)` depend on label bounds.
        let root = self.group_no_center(&[&axis_handle, &ticks, &numbers, &labels]);
        let layers = HashMap::from([
            (SpaceLayer::Axes, axis_handle),
            (SpaceLayer::Ticks, ticks),
            (SpaceLayer::Numbers, numbers),
            (SpaceLayer::Labels, labels),
        ]);
        Ok(NumberLineHandle { root, line, layers })
    }

    pub fn coordinate_polar_plane(
        &mut self,
        radial: Axis,
        radius: f64,
        angle_divisions: usize,
    ) -> Result<PolarSpaceHandle, VisualizationError> {
        self.coordinate_polar_plane_with_visibility(
            radial,
            radius,
            angle_divisions,
            PolarVisibility::default(),
        )
    }

    pub fn coordinate_polar_plane_with_visibility(
        &mut self,
        radial: Axis,
        radius: f64,
        angle_divisions: usize,
        visibility: PolarVisibility,
    ) -> Result<PolarSpaceHandle, VisualizationError> {
        if angle_divisions < 3 {
            return Err(VisualizationError::InvalidSize);
        }
        let radial = self.themed_axis(radial);
        let space = PolarSpace::new(radial.clone(), radius)?;
        let style = radial.style_value();
        let body_size = self.themed_text_config().roles[&gaanim_text::prelude::TextRole::Body].size;
        let number_scale = self
            .theme_style
            .as_ref()
            .and_then(|theme| theme.styles.get("axes/numbers"))
            .and_then(|style| style.text.as_ref())
            .and_then(|style| style.size)
            .map(|size| size / body_size)
            .unwrap_or(1.0);
        let bounds = gaanim_math::Bounds3D::new_2d(-radius, -radius, radius, radius);
        let mut grid_path = BezPath::new();
        let mut numbers_handles = Vec::new();
        for tick in radial.ticks_values(7)? {
            let ring_radius = space.data_to_local(tick.value, 0.0)?.x.abs();
            if ring_radius > f64::EPSILON {
                if visibility.rings {
                    grid_path.extend(Circle::new(Point::ORIGIN, ring_radius).to_path(0.1));
                }
                if visibility.numbers && tick.major && !tick.label.is_empty() {
                    numbers_handles.push(
                        self.text(&tick.label)
                            .fill(style.number_color)
                            .scale_to(number_scale)
                            .move_to(ring_radius, -20.0),
                    );
                }
            }
        }
        if visibility.spokes {
            for index in 0..angle_divisions {
                let angle = std::f64::consts::TAU * index as f64 / angle_divisions as f64;
                grid_path.move_to(Point::ORIGIN);
                grid_path.line_to(Point::new(radius * angle.cos(), radius * angle.sin()));
            }
        }
        let grid_color = self
            .theme_style
            .as_ref()
            .and_then(|theme| {
                theme
                    .styles
                    .get("axes/grid")
                    .and_then(|rule| rule.stroke.as_ref())
                    .and_then(|stroke| theme.resolve_paint(&stroke.paint).ok())
            })
            .and_then(|brush| match brush {
                gaanim_core::peniko::Brush::Solid(color) => Some(color),
                _ => None,
            })
            .unwrap_or(Color::from_rgb8(0xC0, 0xC0, 0xC0));
        let grid = self.themed_axis_path(grid_path, bounds, grid_color, 1.0, "PolarGrid");
        let mut axes_path = BezPath::new();
        if visibility.axes {
            axes_path.move_to(Point::new(-radius, 0.0));
            axes_path.line_to(Point::new(radius, 0.0));
            axes_path.move_to(Point::new(0.0, -radius));
            axes_path.line_to(Point::new(0.0, radius));
        }
        let axes = self.themed_axis_path(axes_path, bounds, style.color, style.width, "PolarAxes");
        let number_refs: Vec<&DrawableHandle> = numbers_handles.iter().collect();
        let numbers = self.group(&number_refs);
        let label_scale = self
            .theme_style
            .as_ref()
            .and_then(|theme| theme.styles.get("axes/labels"))
            .and_then(|style| style.text.as_ref())
            .and_then(|style| style.size)
            .map(|size| size / body_size)
            .unwrap_or(1.125);
        let labels = if visibility.labels
            && let Some(label) = radial.label_text()
        {
            let label = self
                .text(label)
                .fill(style.label_color)
                .scale_to(label_scale)
                .at_anchor(radius + 0.30, 0.0, Anchor::Left);
            self.group(&[&label])
        } else {
            self.group(&[])
        };
        let root = self.group(&[&grid, &axes, &numbers, &labels]);
        let layers = HashMap::from([
            (SpaceLayer::MajorGrid, grid),
            (SpaceLayer::Axes, axes),
            (SpaceLayer::Numbers, numbers),
            (SpaceLayer::Labels, labels),
        ]);
        Ok(PolarSpaceHandle {
            root,
            space,
            layers,
        })
    }

    pub fn polar_plot(
        &mut self,
        space: &PolarSpaceHandle,
        domain: (f64, f64),
        samples: usize,
        mut evaluator: impl FnMut(f64) -> Option<f64>,
    ) -> Result<DrawableHandle, VisualizationError> {
        if samples < 2 || !domain.0.is_finite() || !domain.1.is_finite() || domain.0 >= domain.1 {
            return Err(gaanim_visualization::SamplingError::InvalidDomain.into());
        }
        let mut path = BezPath::new();
        let mut drawing = false;
        for index in 0..samples {
            let progress = index as f64 / (samples - 1) as f64;
            let angle = domain.0 + (domain.1 - domain.0) * progress;
            let point = evaluator(angle)
                .filter(|radius| radius.is_finite())
                .and_then(|radius| space.space.data_to_local(radius, angle).ok());
            if let Some(point) = point {
                if drawing {
                    path.line_to(point);
                } else {
                    path.move_to(point);
                    drawing = true;
                }
            } else {
                drawing = false;
            }
        }
        let handle = self.visualization_path(
            path,
            gaanim_math::Bounds3D::new_2d(
                -space.space.max_radius,
                -space.space.max_radius,
                space.space.max_radius,
                space.space.max_radius,
            ),
            Color::from_rgb8(0x19, 0x32, 0x64),
            3.0,
            "PolarPlot",
        );
        handle
            .spec
            .lock()
            .expect("object spec poisoned")
            .exclude_from_parent_draw = true;
        self.state
            .lock()
            .expect("canvas state poisoned")
            .active_mut()
            .ops
            .push(Op::AttachToGroup {
                group: space.root.id,
                child: handle.id,
            });
        Ok(handle)
    }

    fn validate_reactive_function_owner(
        &self,
        function: &ReactiveFunction,
    ) -> Result<(), VisualizationError> {
        let state = self.state.lock().expect("canvas state poisoned");
        if function
            .scene_owners()
            .iter()
            .any(|owner| *owner != state.scene_id)
            || function
                .parameter_ids()
                .iter()
                .any(|id| !state.parameter_values.contains_key(id))
        {
            return Err(VisualizationError::InvalidParameter);
        }
        Ok(())
    }

    #[doc(hidden)]
    pub fn reactive_plot(
        &mut self,
        space: &CoordinateSpaceHandle,
        function: ReactiveFunction,
        domain: (f64, f64),
        sampling: Sampling,
    ) -> Result<DrawableHandle, VisualizationError> {
        self.validate_reactive_function_owner(&function)?;
        if !domain.0.is_finite() || !domain.1.is_finite() || domain.0 >= domain.1 {
            return Err(gaanim_visualization::SamplingError::InvalidDomain.into());
        }
        if function.coordinate_arity() != 1 || function.output_arity() != 1 {
            return Err(gaanim_visualization::SamplingError::InvalidDomain.into());
        }
        let handle = self.spawn(SpawnKind::ReactivePlot {
            map: space.map.clone(),
            function,
            domain,
            reveal: None,
            sampling,
        });
        self.attach_to_space(space, &handle);
        Ok(handle)
    }

    #[doc(hidden)]
    pub fn reactive_parametric_plot(
        &mut self,
        space: &CoordinateSpaceHandle,
        function: ReactiveFunction,
        domain: (f64, f64),
        sampling: Sampling,
    ) -> Result<DrawableHandle, VisualizationError> {
        self.validate_reactive_function_owner(&function)?;
        if !domain.0.is_finite() || !domain.1.is_finite() || domain.0 >= domain.1 {
            return Err(gaanim_visualization::SamplingError::InvalidDomain.into());
        }
        if function.coordinate_arity() != 1 || function.output_arity() != 2 {
            return Err(gaanim_visualization::SamplingError::InvalidDomain.into());
        }
        let handle = self.spawn(SpawnKind::ReactiveParametric2D {
            map: space.map.clone(),
            function,
            domain,
            sampling,
        });
        self.attach_to_space(space, &handle);
        Ok(handle)
    }

    #[doc(hidden)]
    pub fn reactive_parametric_plot_3d(
        &mut self,
        space: &CoordinateSpace3DHandle,
        function: ReactiveFunction,
        domain: (f64, f64),
        samples: usize,
    ) -> Result<DrawableHandle, VisualizationError> {
        self.validate_reactive_function_owner(&function)?;
        if samples < 2 || !domain.0.is_finite() || !domain.1.is_finite() || domain.0 >= domain.1 {
            return Err(gaanim_visualization::SamplingError::InvalidDomain.into());
        }
        if function.coordinate_arity() != 1 || function.output_arity() != 3 {
            return Err(gaanim_visualization::SamplingError::InvalidDomain.into());
        }
        let handle = self.spawn(SpawnKind::ReactiveParametric3D {
            map: space.map.clone(),
            function,
            domain,
            samples,
        });
        self.attach_to_space_3d(space, &handle);
        Ok(handle)
    }

    #[doc(hidden)]
    pub fn reactive_surface_plot(
        &mut self,
        space: &CoordinateSpace3DHandle,
        function: ReactiveFunction,
        resolution: [usize; 2],
    ) -> Result<DrawableHandle, VisualizationError> {
        self.validate_reactive_function_owner(&function)?;
        if resolution[0] < 2 || resolution[1] < 2 {
            return Err(gaanim_visualization::SamplingError::TooFewSamples.into());
        }
        if function.coordinate_arity() != 2 || function.output_arity() != 1 {
            return Err(gaanim_visualization::SamplingError::InvalidDomain.into());
        }
        let handle = self.spawn(SpawnKind::ReactiveSurface3D {
            map: space.map.clone(),
            function,
            resolution,
        });
        self.attach_to_space_3d(space, &handle);
        Ok(handle)
    }

    /// Plot a dimensionless scalar function perpendicular to a number line.
    /// Values `-1` and `1` map to `-normal_scale` and `normal_scale` local units.
    #[doc(hidden)]
    pub fn number_line_reactive_plot(
        &mut self,
        line: &NumberLineHandle,
        function: ReactiveFunction,
        domain: (f64, f64),
        normal_scale: f64,
        reveal: Option<ScalarSource>,
        sampling: Sampling,
    ) -> Result<DrawableHandle, VisualizationError> {
        self.validate_reactive_function_owner(&function)?;
        if !normal_scale.is_finite() || normal_scale <= 0.0 {
            return Err(VisualizationError::InvalidSize);
        }
        let map = CoordinateMap2D::new(
            line.line.axis.clone(),
            Axis::linear(-1.0, 1.0)?,
            PlotFrame::new(line.line.length, normal_scale * 2.0)?,
        );
        if function.coordinate_arity() != 1 || function.output_arity() != 1 {
            return Err(gaanim_visualization::SamplingError::InvalidDomain.into());
        }
        if !domain.0.is_finite() || !domain.1.is_finite() || domain.0 >= domain.1 {
            return Err(gaanim_visualization::SamplingError::InvalidDomain.into());
        }
        let handle = self.spawn(SpawnKind::ReactivePlot {
            map,
            function,
            domain,
            reveal,
            sampling,
        });
        self.attach_to_number_line(line, &handle);
        Ok(handle)
    }

    /// Create numeric glyphs sourced from a deterministic scalar callable.
    #[doc(hidden)]
    pub fn reactive_readout(
        &mut self,
        source: ScalarSource,
        format: impl Into<String>,
        prefix: impl Into<String>,
        suffix: impl Into<String>,
        invalid: impl Into<String>,
        font_size: Option<f64>,
    ) -> DrawableHandle {
        self.spawn(SpawnKind::ReactiveReadout {
            source,
            format: format.into(),
            prefix: prefix.into(),
            suffix: suffix.into(),
            invalid: invalid.into(),
            font_size,
        })
    }

    pub fn function_plot(
        &mut self,
        space: &CoordinateSpaceHandle,
        domain: (f64, f64),
        sampling: Sampling,
        evaluator: impl FnMut(f64) -> Option<f64>,
    ) -> Result<DrawableHandle, VisualizationError> {
        let sampled = sample_function(&space.map, domain, sampling, evaluator)?;
        let handle = self.visualization_path(
            sampled.to_bez_path(),
            space.map.frame.bounds(),
            Color::from_rgb8(0x19, 0x32, 0x64),
            3.0,
            "FunctionPlot",
        );
        self.attach_to_space(space, &handle);
        Ok(handle)
    }

    pub fn parametric_plot(
        &mut self,
        space: &CoordinateSpaceHandle,
        domain: (f64, f64),
        sampling: Sampling,
        evaluator: impl FnMut(f64) -> Option<(f64, f64)>,
    ) -> Result<DrawableHandle, VisualizationError> {
        let sampled = sample_parametric(&space.map, domain, sampling, evaluator)?;
        let handle = self.visualization_path(
            sampled.to_bez_path(),
            space.map.frame.bounds(),
            Color::from_rgb8(0x19, 0x32, 0x64),
            3.0,
            "ParametricPlot",
        );
        self.attach_to_space(space, &handle);
        Ok(handle)
    }

    pub fn implicit_plot(
        &mut self,
        space: &CoordinateSpaceHandle,
        resolution: [usize; 2],
        evaluator: impl FnMut(f64, f64) -> Option<f64>,
    ) -> Result<DrawableHandle, VisualizationError> {
        let sampled = implicit_contours(&space.map, resolution, evaluator)?;
        let handle = self.visualization_path(
            sampled.to_bez_path(),
            space.map.frame.bounds(),
            Color::from_rgb8(0x19, 0x32, 0x64),
            3.0,
            "ImplicitPlot",
        );
        self.attach_to_space(space, &handle);
        Ok(handle)
    }

    pub fn contour_plot(
        &mut self,
        space: &CoordinateSpaceHandle,
        levels: &[f64],
        resolution: [usize; 2],
        mut evaluator: impl FnMut(f64, f64) -> Option<f64>,
    ) -> Result<DrawableHandle, VisualizationError> {
        if levels.is_empty() || levels.iter().any(|level| !level.is_finite()) {
            return Err(VisualizationError::EmptyData);
        }
        let mut path = BezPath::new();
        for level in levels {
            let sampled = implicit_contours(&space.map, resolution, |x, y| {
                evaluator(x, y).map(|value| value - level)
            })?;
            path.extend(sampled.to_bez_path());
        }
        let handle = self.visualization_path(
            path,
            space.map.frame.bounds(),
            Color::from_rgb8(0x19, 0x32, 0x64),
            2.0,
            "ContourPlot",
        );
        self.attach_to_space(space, &handle);
        Ok(handle)
    }

    /// Draw one data-space segment while preserving the space hierarchy.
    pub fn coordinate_segment(
        &mut self,
        space: &CoordinateSpaceHandle,
        start: (f64, f64),
        end: (f64, f64),
    ) -> Result<DrawableHandle, VisualizationError> {
        let start = space.map.data_to_local(start.0, start.1)?;
        let end = space.map.data_to_local(end.0, end.1)?;
        let mut path = BezPath::new();
        path.move_to(start);
        path.line_to(end);
        let handle = self.visualization_path(
            path,
            space.map.frame.bounds(),
            Color::from_rgb8(0xE5, 0x4B, 0x4B),
            2.5,
            "CoordinateSegment",
        );
        self.attach_to_space(space, &handle);
        Ok(handle)
    }

    pub fn vector_field_2d(
        &self,
        space: &CoordinateSpaceHandle,
        evaluator: impl Fn([f64; 2]) -> Option<[f64; 2]> + Send + Sync + 'static,
    ) -> VectorField2DHandle {
        VectorField2DHandle {
            space: space.clone(),
            field: FieldModel::new(evaluator),
            function: None,
        }
    }

    #[doc(hidden)]
    pub fn vector_field_2d_reactive(
        &self,
        space: &CoordinateSpaceHandle,
        function: ReactiveFunction,
    ) -> Result<VectorField2DHandle, VisualizationError> {
        self.validate_reactive_function_owner(&function)?;
        if function.coordinate_arity() != 2 || function.output_arity() != 2 {
            return Err(VisualizationError::InvalidParameter);
        }
        let mut ids = function.parameter_ids();
        ids.sort_unstable();
        ids.dedup();
        let mirrors = self
            .expression_parameter_values(&ids)
            .ok_or(VisualizationError::InvalidParameter)?;
        let evaluator_function = function.clone();
        let time = self.current_time();
        Ok(VectorField2DHandle {
            space: space.clone(),
            field: FieldModel::new(move |[x, y]| {
                let values = evaluator_function
                    .evaluate(&[x, y], time, |logical| {
                        mirrors
                            .iter()
                            .find_map(|(id, value)| (*id == logical).then_some(value))
                            .map(|value| *value.lock().expect("parameter poisoned"))
                    })
                    .ok()?;
                Some([values[0], values[1]])
            }),
            function: Some(function),
        })
    }

    pub fn vector_field_3d(
        &self,
        space: &CoordinateSpace3DHandle,
        evaluator: impl Fn([f64; 3]) -> Option<[f64; 3]> + Send + Sync + 'static,
    ) -> VectorField3DHandle {
        VectorField3DHandle {
            space: space.clone(),
            field: FieldModel::new(evaluator),
            function: None,
        }
    }

    #[doc(hidden)]
    pub fn vector_field_3d_reactive(
        &self,
        space: &CoordinateSpace3DHandle,
        function: ReactiveFunction,
    ) -> Result<VectorField3DHandle, VisualizationError> {
        self.validate_reactive_function_owner(&function)?;
        if function.coordinate_arity() != 3 || function.output_arity() != 3 {
            return Err(VisualizationError::InvalidParameter);
        }
        let mut ids = function.parameter_ids();
        ids.sort_unstable();
        ids.dedup();
        let mirrors = self
            .expression_parameter_values(&ids)
            .ok_or(VisualizationError::InvalidParameter)?;
        let evaluator_function = function.clone();
        let time = self.current_time();
        Ok(VectorField3DHandle {
            space: space.clone(),
            field: FieldModel::new(move |[x, y, z]| {
                let values = evaluator_function
                    .evaluate(&[x, y, z], time, |logical| {
                        mirrors
                            .iter()
                            .find_map(|(id, value)| (*id == logical).then_some(value))
                            .map(|value| *value.lock().expect("parameter poisoned"))
                    })
                    .ok()?;
                Some([values[0], values[1], values[2]])
            }),
            function: Some(function),
        })
    }

    pub fn arrow_vector_field_2d(
        &mut self,
        field: &VectorField2DHandle,
        resolution: [usize; 2],
        options: ArrowFieldOptions,
    ) -> Result<ArrowVectorFieldHandle, VisualizationError> {
        if !options.min_length.is_finite()
            || options.min_length < 0.0
            || !options.max_length.is_finite()
            || options.max_length <= 0.0
            || options.min_length > options.max_length
            || !options.length_scale.is_finite()
            || options.length_scale <= 0.0
            || !options.width.is_finite()
            || options.width <= 0.0
        {
            return Err(VisualizationError::InvalidSize);
        }
        let domains = [field.space.map.x.domain(), field.space.map.y.domain()];
        let samples = field.field.sample_grid(domains, resolution)?;
        let range = value_range(
            samples.iter().map(|sample| sample.magnitude),
            options.color_range,
        )
        .ok_or(VisualizationError::InvalidSize)?;
        let mut arrows = Vec::with_capacity(samples.len());
        for sample in samples {
            let start = field
                .space
                .map
                .data_to_local(sample.position[0], sample.position[1])?;
            let displaced = field.space.map.data_to_local(
                sample.position[0] + sample.vector[0],
                sample.position[1] + sample.vector[1],
            )?;
            let delta = displaced - start;
            let raw_length = delta.hypot() * options.length_scale;
            if raw_length <= f64::EPSILON {
                continue;
            }
            let length = raw_length.clamp(options.min_length, options.max_length);
            let direction = delta / delta.hypot();
            let end = start + direction * length;
            let tip_length = options
                .tip_length
                .unwrap_or((length * 0.3).clamp(5.0, 12.0));
            let tip_width = options.tip_width.unwrap_or(tip_length * 0.8);
            let perpendicular = gaanim_core::kurbo::Vec2::new(-direction.y, direction.x);
            let shoulder = end - direction * tip_length;
            let mut path = BezPath::new();
            path.move_to(start);
            path.line_to(end);
            path.move_to(shoulder + perpendicular * (tip_width * 0.5));
            path.line_to(end);
            path.line_to(shoulder - perpendicular * (tip_width * 0.5));
            let color = mapped_color(
                sample.magnitude,
                range,
                options.color,
                options.colormap.as_ref(),
                1.0,
            );
            let glyph = self
                .visualization_path(
                    path,
                    field.space.map.frame.bounds(),
                    color,
                    options.width,
                    "ArrowVectorFieldGlyph",
                )
                .stroke(color, options.width);
            if let Some(function) = &field.function
                && function_is_reactive(function)
            {
                self.state
                    .lock()
                    .expect("canvas state poisoned")
                    .active_mut()
                    .ops
                    .push(Op::AttachReactiveArrowField2D {
                        target: glyph.id,
                        function: function.clone(),
                        position: sample.position,
                        map: field.space.map.clone(),
                        options: options.clone(),
                        color_range: range,
                    });
            }
            arrows.push(glyph);
        }
        if arrows.is_empty() {
            return Err(gaanim_visualization::VectorFieldError::Empty.into());
        }
        let members: Vec<_> = arrows.iter().collect();
        let drawable = self.group_no_center(&members);
        self.attach_to_space(&field.space, &drawable);
        Ok(ArrowVectorFieldHandle { drawable })
    }

    pub fn arrow_vector_field_3d(
        &mut self,
        field: &VectorField3DHandle,
        resolution: [usize; 3],
        mut options: ArrowFieldOptions,
    ) -> Result<ArrowVectorFieldHandle, VisualizationError> {
        if (options.max_length - ArrowFieldOptions::default().max_length).abs() < f64::EPSILON {
            options.max_length = 24.0;
        }
        if !options.max_length.is_finite() || options.max_length <= 0.0 {
            return Err(VisualizationError::InvalidSize);
        }
        let domains = [
            field.space.map.x.domain(),
            field.space.map.y.domain(),
            field.space.map.z.domain(),
        ];
        let samples = field.field.sample_grid(domains, resolution)?;
        let range = value_range(
            samples.iter().map(|sample| sample.magnitude),
            options.color_range,
        )
        .ok_or(VisualizationError::InvalidSize)?;
        let mut points = Vec::new();
        let mut colors = Vec::new();
        for sample in samples {
            let start = DVec3::from_array(field.space.map.data_to_local(sample.position)?);
            let displaced = DVec3::from_array(field.space.map.data_to_local([
                sample.position[0] + sample.vector[0],
                sample.position[1] + sample.vector[1],
                sample.position[2] + sample.vector[2],
            ])?);
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
                .unwrap_or((length * 0.3).clamp(4.0, 10.0));
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
                    from.to_array().map(|v| v as f32),
                    to.to_array().map(|v| v as f32),
                ]);
            }
            let color = mapped_color(
                sample.magnitude,
                range,
                options.color,
                options.colormap.as_ref(),
                1.0,
            );
            colors.extend(std::iter::repeat_n(color, 6));
        }
        if points.is_empty() {
            return Err(gaanim_visualization::VectorFieldError::Empty.into());
        }
        let drawable = self.line_segments_3d_with_colors(points, colors);
        if let Some(function) = &field.function
            && function_is_reactive(function)
        {
            self.state
                .lock()
                .expect("canvas state poisoned")
                .active_mut()
                .ops
                .push(Op::AttachReactiveArrowField3D {
                    target: drawable.id,
                    function: function.clone(),
                    resolution,
                    map: field.space.map.clone(),
                    options: options.clone(),
                    color_range: range,
                });
        }
        self.attach_to_space_3d(&field.space, &drawable);
        Ok(ArrowVectorFieldHandle { drawable })
    }

    pub fn stream_lines_2d(
        &mut self,
        field: &VectorField2DHandle,
        seed_resolution: [usize; 2],
        style: StreamLinesStyle,
    ) -> Result<StreamLinesHandle, VisualizationError> {
        if !style.width.is_finite()
            || style.width <= 0.0
            || !style.opacity.is_finite()
            || !(0.0..=1.0).contains(&style.opacity)
        {
            return Err(VisualizationError::InvalidSize);
        }
        let domains = [field.space.map.x.domain(), field.space.map.y.domain()];
        let integrated = field
            .field
            .streamlines(domains, seed_resolution, style.integration)?;
        let range = value_range(
            integrated
                .iter()
                .flat_map(|line| line.speeds.iter().copied()),
            style.color_range,
        )
        .ok_or(VisualizationError::InvalidSize)?;
        let mut lines = Vec::with_capacity(integrated.len());
        let mut flow_lines = Vec::with_capacity(integrated.len());
        for line in integrated {
            let seed = line.seed;
            let mut path = BezPath::new();
            for (index, point) in line.points.iter().enumerate() {
                let local = field.space.map.data_to_local(point[0], point[1])?;
                if index == 0 {
                    path.move_to(local);
                } else {
                    path.line_to(local);
                }
            }
            let speed = line.speeds.iter().sum::<f64>() / line.speeds.len() as f64;
            let color = mapped_color(
                speed,
                range,
                style.color,
                style.colormap.as_ref(),
                style.opacity,
            );
            let handle = self
                .visualization_path(
                    path.clone(),
                    field.space.map.frame.bounds(),
                    color,
                    style.width,
                    "StreamLine",
                )
                .stroke(color, style.width);
            let highlight = flow_highlight_color(color);
            let flow_handle = self
                .visualization_path(
                    path,
                    field.space.map.frame.bounds(),
                    highlight,
                    style.width * 1.6,
                    "StreamLineFlow",
                )
                .stroke(highlight, style.width * 1.6)
                .opacity(0.0);
            if let Some(function) = &field.function
                && function_is_reactive(function)
            {
                let flow_style = flow_highlight_style(&style);
                let mut state = self.state.lock().expect("canvas state poisoned");
                for (target, target_style) in
                    [(handle.id, style.clone()), (flow_handle.id, flow_style)]
                {
                    state.active_mut().ops.push(Op::AttachReactiveStreamLine2D {
                        target,
                        function: function.clone(),
                        seed,
                        map: field.space.map.clone(),
                        style: target_style,
                        color_range: range,
                    });
                }
            }
            lines.push(handle);
            flow_lines.push(flow_handle);
        }
        let members: Vec<_> = lines.iter().collect();
        let drawable = self.group_no_center(&members);
        self.attach_to_space(&field.space, &drawable);
        let flow_members: Vec<_> = flow_lines.iter().collect();
        let flow_drawable = self.group_no_center(&flow_members);
        self.attach_to_space(&field.space, &flow_drawable);
        Ok(StreamLinesHandle {
            drawable,
            lines,
            flow_lines,
        })
    }

    pub fn stream_lines_3d(
        &mut self,
        field: &VectorField3DHandle,
        seed_resolution: [usize; 3],
        style: StreamLinesStyle,
    ) -> Result<StreamLinesHandle, VisualizationError> {
        let domains = [
            field.space.map.x.domain(),
            field.space.map.y.domain(),
            field.space.map.z.domain(),
        ];
        let integrated = field
            .field
            .streamlines(domains, seed_resolution, style.integration)?;
        let range = value_range(
            integrated
                .iter()
                .flat_map(|line| line.speeds.iter().copied()),
            style.color_range,
        )
        .ok_or(VisualizationError::InvalidSize)?;
        let mut lines = Vec::with_capacity(integrated.len());
        let mut flow_lines = Vec::with_capacity(integrated.len());
        for line in integrated {
            let seed = line.seed;
            let points = line
                .points
                .iter()
                .map(|point| field.space.map.data_to_local(*point))
                .collect::<Result<Vec<_>, _>>()?
                .into_iter()
                .map(|point| point.map(|value| value as f32))
                .collect::<Vec<_>>();
            let colors: Vec<Color> = line
                .speeds
                .iter()
                .map(|speed| {
                    mapped_color(
                        *speed,
                        range,
                        style.color,
                        style.colormap.as_ref(),
                        style.opacity,
                    )
                })
                .collect();
            let flow_colors = colors
                .iter()
                .copied()
                .map(flow_highlight_color)
                .collect::<Vec<_>>();
            let handle = self.polyline_3d_with_colors(points.clone(), colors);
            let flow_handle = self
                .polyline_3d_with_colors(points, flow_colors)
                .opacity(0.0);
            if let Some(function) = &field.function
                && function_is_reactive(function)
            {
                let flow_style = flow_highlight_style(&style);
                let mut state = self.state.lock().expect("canvas state poisoned");
                for (target, target_style) in
                    [(handle.id, style.clone()), (flow_handle.id, flow_style)]
                {
                    state.active_mut().ops.push(Op::AttachReactiveStreamLine3D {
                        target,
                        function: function.clone(),
                        seed,
                        map: field.space.map.clone(),
                        style: target_style,
                        color_range: range,
                    });
                }
            }
            lines.push(handle);
            flow_lines.push(flow_handle);
        }
        let members: Vec<_> = lines.iter().collect();
        let drawable = self.group_no_center(&members);
        self.attach_to_space_3d(&field.space, &drawable);
        let flow_members: Vec<_> = flow_lines.iter().collect();
        let flow_drawable = self.group_no_center(&flow_members);
        self.attach_to_space_3d(&field.space, &flow_drawable);
        Ok(StreamLinesHandle {
            drawable,
            lines,
            flow_lines,
        })
    }

    pub fn advect_2d(
        &mut self,
        field: &VectorField2DHandle,
        target: &DrawableHandle,
        seed: [f64; 2],
        integration: StreamlineOptions,
        duration: f64,
    ) -> Result<super::types::Anim, VisualizationError> {
        if !duration.is_finite() || duration <= 0.0 {
            return Err(VisualizationError::InvalidSize);
        }
        let domains = [field.space.map.x.domain(), field.space.map.y.domain()];
        let line = field
            .field
            .integrate(seed, domains, integration)
            .ok_or(gaanim_visualization::VectorFieldError::Empty)?;
        let mut path = BezPath::new();
        for (index, point) in line.points.into_iter().enumerate() {
            let local = field.space.map.data_to_local(point[0], point[1])?;
            if index == 0 {
                path.move_to(local);
            } else {
                path.line_to(local);
            }
        }
        self.attach_to_space(&field.space, target);
        Ok(target.move_along_path(path).duration(duration))
    }

    pub fn advect_3d(
        &mut self,
        field: &VectorField3DHandle,
        target: &DrawableHandle,
        seed: [f64; 3],
        integration: StreamlineOptions,
        duration: f64,
    ) -> Result<super::types::Anim, VisualizationError> {
        if !duration.is_finite() || duration <= 0.0 {
            return Err(VisualizationError::InvalidSize);
        }
        let domains = [
            field.space.map.x.domain(),
            field.space.map.y.domain(),
            field.space.map.z.domain(),
        ];
        let line = field
            .field
            .integrate(seed, domains, integration)
            .ok_or(gaanim_visualization::VectorFieldError::Empty)?;
        let points = line
            .points
            .into_iter()
            .map(|point| field.space.map.data_to_local(point).map(DVec3::from_array))
            .collect::<Result<Vec<_>, _>>()?;
        self.attach_to_space_3d(&field.space, target);
        Ok(target.move_along_path_3d(points).duration(duration))
    }

    pub fn flow_particles_2d(
        &mut self,
        field: &VectorField2DHandle,
        count: usize,
        options: FlowParticleOptions,
    ) -> Result<FlowParticlesHandle, VisualizationError> {
        if count == 0
            || !options.radius.is_finite()
            || options.radius <= 0.0
            || !options.duration.is_finite()
            || options.duration <= 0.0
        {
            return Err(VisualizationError::InvalidSize);
        }
        let domains = [field.space.map.x.domain(), field.space.map.y.domain()];
        let seeds = (1..=count)
            .map(|index| {
                [
                    domains[0].0 + (domains[0].1 - domains[0].0) * halton(index, 2),
                    domains[1].0 + (domains[1].1 - domains[1].0) * halton(index, 3),
                ]
            })
            .collect::<Vec<_>>();
        let range = value_range(
            seeds
                .iter()
                .filter_map(|seed| field.field.evaluate(*seed).map(|sample| sample.magnitude)),
            options.color_range,
        )
        .ok_or(gaanim_visualization::VectorFieldError::Empty)?;
        let mut particles = Vec::new();
        let mut animations = Vec::new();
        for seed in seeds {
            let Some(line) = field.field.integrate(seed, domains, options.integration) else {
                continue;
            };
            let speed = line.speeds.first().copied().unwrap_or(0.0);
            let color = mapped_color(
                speed,
                range,
                options.color,
                options.colormap.as_ref(),
                options.opacity,
            );
            let mut path = BezPath::new();
            for (index, point) in line.points.into_iter().enumerate() {
                let local = field.space.map.data_to_local(point[0], point[1])?;
                if index == 0 {
                    path.move_to(local);
                } else {
                    path.line_to(local);
                }
            }
            let start = gaanim_math::get_point_at_alpha(&path, 0.0);
            let particle = self
                .circle(options.radius)
                .fill(color)
                .no_stroke()
                .move_to(start.x, start.y);
            particle.defer_visibility_until_play();
            animations.push(particle.move_along_path(path).duration(options.duration));
            particles.push(particle);
        }
        if particles.is_empty() {
            return Err(gaanim_visualization::VectorFieldError::Empty.into());
        }
        let members = particles.iter().collect::<Vec<_>>();
        let drawable = self.group_no_center(&members);
        self.attach_to_space(&field.space, &drawable);
        Ok(FlowParticlesHandle {
            drawable,
            animations,
        })
    }

    pub fn flow_particles_3d(
        &mut self,
        field: &VectorField3DHandle,
        count: usize,
        options: FlowParticleOptions,
    ) -> Result<FlowParticlesHandle, VisualizationError> {
        if count == 0
            || !options.radius.is_finite()
            || options.radius <= 0.0
            || !options.duration.is_finite()
            || options.duration <= 0.0
        {
            return Err(VisualizationError::InvalidSize);
        }
        let domains = [
            field.space.map.x.domain(),
            field.space.map.y.domain(),
            field.space.map.z.domain(),
        ];
        let seeds = (1..=count)
            .map(|index| {
                [
                    domains[0].0 + (domains[0].1 - domains[0].0) * halton(index, 2),
                    domains[1].0 + (domains[1].1 - domains[1].0) * halton(index, 3),
                    domains[2].0 + (domains[2].1 - domains[2].0) * halton(index, 5),
                ]
            })
            .collect::<Vec<_>>();
        let range = value_range(
            seeds
                .iter()
                .filter_map(|seed| field.field.evaluate(*seed).map(|sample| sample.magnitude)),
            options.color_range,
        )
        .ok_or(gaanim_visualization::VectorFieldError::Empty)?;
        let mut particles = Vec::new();
        let mut animations = Vec::new();
        for seed in seeds {
            let Some(line) = field.field.integrate(seed, domains, options.integration) else {
                continue;
            };
            let speed = line.speeds.first().copied().unwrap_or(0.0);
            let color = mapped_color(
                speed,
                range,
                options.color,
                options.colormap.as_ref(),
                options.opacity,
            );
            let points = line
                .points
                .into_iter()
                .map(|point| field.space.map.data_to_local(point).map(DVec3::from_array))
                .collect::<Result<Vec<_>, _>>()?;
            let start = points[0];
            let particle = self
                .sphere(
                    options.radius,
                    12,
                    8,
                    gaanim_scene::Material3D::matte(color),
                )
                .map_err(|_| VisualizationError::InvalidSize)?
                .move_to_3d(start.x, start.y, start.z);
            particle.defer_visibility_until_play();
            animations.push(
                particle
                    .move_along_path_3d(points)
                    .duration(options.duration),
            );
            particles.push(particle);
        }
        if particles.is_empty() {
            return Err(gaanim_visualization::VectorFieldError::Empty.into());
        }
        let members = particles.iter().collect::<Vec<_>>();
        let drawable = self.group_no_center(&members);
        self.attach_to_space_3d(&field.space, &drawable);
        Ok(FlowParticlesHandle {
            drawable,
            animations,
        })
    }

    pub fn data_line(
        &mut self,
        space: &CoordinateSpaceHandle,
        x: &[Option<f64>],
        y: &[Option<f64>],
        step: bool,
        area: Option<f64>,
        policy: NonFinitePolicy,
    ) -> Result<DrawableHandle, VisualizationError> {
        if x.len() != y.len() || x.is_empty() {
            return Err(VisualizationError::LengthMismatch);
        }
        let path = if let Some(baseline) = area {
            area_path(&space.map, x, y, baseline)?
        } else if step {
            step_path(&space.map, x, y, policy)?
        } else {
            line_path(&space.map, x, y, policy)?
        };
        let handle = self.visualization_path(
            path,
            space.map.frame.bounds(),
            Color::from_rgb8(0x19, 0x32, 0x64),
            3.0,
            "DataLine",
        );
        self.attach_to_space(space, &handle);
        Ok(handle)
    }

    pub fn data_mark(
        &mut self,
        space: &CoordinateSpaceHandle,
        source: gaanim_visualization::DataSource,
        kind: gaanim_visualization::DataMarkKind,
    ) -> Result<DrawableHandle, VisualizationError> {
        gaanim_visualization::data_mark_path(&space.map, &source.snapshot(), &kind).map_err(
            |error| match error {
                gaanim_visualization::MarkError::Axis(error) => VisualizationError::Axis(error),
                gaanim_visualization::MarkError::Data(_) => VisualizationError::LengthMismatch,
                gaanim_visualization::MarkError::Empty => VisualizationError::EmptyData,
            },
        )?;
        let handle = self.spawn(SpawnKind::DataMark {
            map: space.map.clone(),
            source,
            kind,
        });
        self.attach_to_space(space, &handle);
        Ok(handle)
    }

    pub fn heatmap_plot(
        &mut self,
        space: &CoordinateSpaceHandle,
        source: gaanim_visualization::DataSource,
        x: impl Into<String>,
        y: impl Into<String>,
        value: impl Into<String>,
        cell_size: [f64; 2],
        bands: usize,
    ) -> Result<DrawableHandle, VisualizationError> {
        if bands < 2
            || !cell_size[0].is_finite()
            || !cell_size[1].is_finite()
            || cell_size[0] <= 0.0
            || cell_size[1] <= 0.0
        {
            return Err(VisualizationError::InvalidSize);
        }
        let x = x.into();
        let y = y.into();
        let value = value.into();
        let mut handles = Vec::with_capacity(bands);
        for band in 0..bands {
            let kind = gaanim_visualization::DataMarkKind::HeatmapBand {
                x: x.clone(),
                y: y.clone(),
                value: value.clone(),
                cell_width: cell_size[0],
                cell_height: cell_size[1],
                band,
                bands,
            };
            gaanim_visualization::data_mark_path(&space.map, &source.snapshot(), &kind).map_err(
                |error| match error {
                    gaanim_visualization::MarkError::Axis(error) => VisualizationError::Axis(error),
                    gaanim_visualization::MarkError::Data(_) => VisualizationError::LengthMismatch,
                    gaanim_visualization::MarkError::Empty => VisualizationError::EmptyData,
                },
            )?;
            let progress = band as f64 / (bands - 1) as f64;
            let palette = self
                .theme_style
                .as_ref()
                .map(|theme| theme.heatmap.as_slice())
                .filter(|palette| palette.len() >= 2);
            let color = if let Some(palette) = palette {
                let scaled = progress * (palette.len() - 1) as f64;
                let index = scaled.floor() as usize;
                let next = (index + 1).min(palette.len() - 1);
                let t = scaled - index as f64;
                let left = palette[index].to_rgba8();
                let right = palette[next].to_rgba8();
                let channel =
                    |a: u8, b: u8| (f64::from(a) + (f64::from(b) - f64::from(a)) * t).round() as u8;
                Color::from_rgba8(
                    channel(left.r, right.r),
                    channel(left.g, right.g),
                    channel(left.b, right.b),
                    channel(left.a, right.a),
                )
            } else {
                Color::from_rgba8(
                    (0x30 as f64 + progress * 0x30 as f64) as u8,
                    (0x90 as f64 - progress * 0x60 as f64) as u8,
                    (0xF0 as f64 - progress * 0x80 as f64) as u8,
                    0xFF,
                )
            };
            handles.push(
                self.spawn(SpawnKind::DataMark {
                    map: space.map.clone(),
                    source: source.clone(),
                    kind,
                })
                .fill(color),
            );
        }
        let refs: Vec<&DrawableHandle> = handles.iter().collect();
        let handle = self.group(&refs);
        self.attach_to_space(space, &handle);
        Ok(handle)
    }

    pub fn data_scatter(
        &mut self,
        space: &CoordinateSpaceHandle,
        x: &[Option<f64>],
        y: &[Option<f64>],
        radius: f64,
        policy: NonFinitePolicy,
    ) -> Result<DrawableHandle, VisualizationError> {
        if x.len() != y.len() || x.is_empty() {
            return Err(VisualizationError::LengthMismatch);
        }
        if !radius.is_finite() || radius <= 0.0 {
            return Err(VisualizationError::InvalidSize);
        }
        let points = scatter_points(&space.map, x, y, policy)?;
        let series_color = self
            .theme_style
            .as_ref()
            .and_then(|theme| theme.series.first().copied());
        let marks: Vec<DrawableHandle> = points
            .iter()
            .map(|point| {
                let mark = self.dot(radius).move_to(point.x, point.y);
                if let Some(color) = series_color {
                    mark.fill(color)
                } else {
                    mark
                }
            })
            .collect();
        let refs: Vec<&DrawableHandle> = marks.iter().collect();
        let handle = self.group(&refs);
        self.attach_to_space(space, &handle);
        Ok(handle)
    }

    pub fn data_bars(
        &mut self,
        space: &CoordinateSpaceHandle,
        x: &[f64],
        values: &[f64],
        width: f64,
        baseline: f64,
    ) -> Result<DrawableHandle, VisualizationError> {
        if x.len() != values.len() || x.is_empty() {
            return Err(VisualizationError::LengthMismatch);
        }
        let rects = bars(&space.map, x, values, width, baseline)?;
        let series_color = self
            .theme_style
            .as_ref()
            .and_then(|theme| theme.series.first().copied());
        let marks: Vec<DrawableHandle> = rects
            .iter()
            .map(|rect| {
                let mark = self
                    .rect(rect.max.x - rect.min.x, rect.max.y - rect.min.y)
                    .move_to(
                        (rect.min.x + rect.max.x) * 0.5,
                        (rect.min.y + rect.max.y) * 0.5,
                    );
                if let Some(color) = series_color {
                    mark.fill(color)
                } else {
                    mark
                }
            })
            .collect();
        let refs: Vec<&DrawableHandle> = marks.iter().collect();
        let handle = self.group(&refs);
        self.attach_to_space(space, &handle);
        Ok(handle)
    }

    pub fn data_error_bars(
        &mut self,
        space: &CoordinateSpaceHandle,
        x: &[f64],
        y: &[f64],
        low: &[f64],
        high: &[f64],
        cap_width: f64,
    ) -> Result<DrawableHandle, VisualizationError> {
        if x.len() != y.len() || x.len() != low.len() || x.len() != high.len() || x.is_empty() {
            return Err(VisualizationError::LengthMismatch);
        }
        let path = error_bar_path(&space.map, x, y, low, high, cap_width)?;
        let handle = self.visualization_path(
            path,
            space.map.frame.bounds(),
            Color::from_rgb8(0x20, 0x20, 0x20),
            2.0,
            "ErrorBars",
        );
        self.attach_to_space(space, &handle);
        Ok(handle)
    }

    pub fn histogram_plot(
        &mut self,
        space: &CoordinateSpaceHandle,
        values: &[f64],
        bins_count: usize,
    ) -> Result<DrawableHandle, VisualizationError> {
        let result = histogram(values, bins_count).ok_or(VisualizationError::EmptyData)?;
        let centers: Vec<f64> = result
            .edges
            .windows(2)
            .map(|edge| (edge[0] + edge[1]) * 0.5)
            .collect();
        let counts: Vec<f64> = result.counts.iter().map(|count| *count as f64).collect();
        let width = result.edges[1] - result.edges[0];
        self.data_bars(space, &centers, &counts, width, 0.0)
    }

    pub fn box_plot(
        &mut self,
        space: &CoordinateSpaceHandle,
        center: f64,
        values: &[f64],
        width: f64,
    ) -> Result<DrawableHandle, VisualizationError> {
        let stats = box_stats(values).ok_or(VisualizationError::EmptyData)?;
        let left = space.map.data_to_local(center - width * 0.5, stats.q1)?;
        let right = space.map.data_to_local(center + width * 0.5, stats.q3)?;
        let mut marks = vec![
            self.rect((right.x - left.x).abs(), (right.y - left.y).abs())
                .move_to((left.x + right.x) * 0.5, (left.y + right.y) * 0.5),
        ];
        let median = space.map.data_to_local(center, stats.median)?;
        let minimum = space.map.data_to_local(center, stats.minimum)?;
        let maximum = space.map.data_to_local(center, stats.maximum)?;
        marks.push(self.line(left.x, median.y, right.x, median.y));
        marks.push(self.line(median.x, minimum.y, median.x, maximum.y));
        let refs: Vec<&DrawableHandle> = marks.iter().collect();
        let handle = self.group(&refs);
        self.attach_to_space(space, &handle);
        Ok(handle)
    }

    pub fn violin_plot(
        &mut self,
        space: &CoordinateSpaceHandle,
        center: f64,
        values: &[f64],
        bandwidth: f64,
        width: f64,
    ) -> Result<DrawableHandle, VisualizationError> {
        let path = violin_path(&space.map, center, values, bandwidth, width, 64)?;
        let handle = self.visualization_path(
            path,
            space.map.frame.bounds(),
            Color::from_rgb8(0x9B, 0x59, 0xB6),
            2.0,
            "Violin",
        );
        self.attach_to_space(space, &handle);
        Ok(handle)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn layer_is_empty(handle: &DrawableHandle) -> bool {
        let spec = handle.spec.lock().expect("layer spec poisoned");
        match &spec.kind {
            SpawnKind::SvgPath(path) => path.path.elements().is_empty(),
            SpawnKind::LineSegments3D { points, .. } => points.is_empty(),
            SpawnKind::Group(children) | SpawnKind::GroupNoCenter(children) => children.is_empty(),
            other => panic!("unexpected semantic layer kind: {other:?}"),
        }
    }

    fn group_child_translations(canvas: &SceneModel, group: &DrawableHandle) -> Vec<DVec3> {
        let group_spec = group.spec.lock().expect("group spec poisoned");
        let children = match &group_spec.kind {
            SpawnKind::Group(children) | SpawnKind::GroupNoCenter(children) => children,
            _ => panic!("expected a group"),
        };
        let children = children.clone();
        drop(group_spec);
        let state = canvas.state.lock().expect("canvas state poisoned");
        children
            .iter()
            .map(|id| {
                let spec = state
                    .active()
                    .ops
                    .iter()
                    .find_map(|op| match op {
                        Op::Spawn(spec) if spec.lock().expect("object spec poisoned").id == *id => {
                            Some(spec.clone())
                        }
                        _ => None,
                    })
                    .expect("label must have a spawn spec");
                spec.lock()
                    .expect("label spec poisoned")
                    .layout_ops
                    .iter()
                    .rev()
                    .find_map(|op| match op {
                        super::super::types::LayoutOp::SetTranslation(value) => Some(*value),
                        super::super::types::LayoutOp::MoveAnchorTo { target, .. } => Some(*target),
                        _ => None,
                    })
                    .expect("label must have an authored translation")
            })
            .collect()
    }

    #[test]
    fn cartesian_visibility_keeps_disabled_layers_available_and_mapping_stable() {
        let mut canvas = SceneModel::new(640, 360);
        let x = Axis::linear(-2.0, 2.0)
            .unwrap()
            .ticks(1.0)
            .unwrap()
            .minor_ticks(2)
            .label("x");
        let y = Axis::linear(-1.0, 1.0)
            .unwrap()
            .ticks(0.5)
            .unwrap()
            .label("y");
        let space = canvas
            .coordinate_axes_with_visibility(
                x,
                y,
                Some(400.0),
                Some(200.0),
                CartesianVisibility {
                    x_grid: true,
                    y_grid: false,
                    x_axis: false,
                    y_axis: false,
                    x_ticks: false,
                    y_ticks: false,
                    x_numbers: false,
                    y_numbers: false,
                    x_labels: false,
                    y_labels: false,
                },
            )
            .unwrap();

        assert_eq!(space.data_to_local(1.0, 0.5).unwrap(), (100.0, 50.0));
        assert!(!layer_is_empty(space.layer(SpaceLayer::MajorGrid).unwrap()));
        assert!(!layer_is_empty(space.layer(SpaceLayer::MinorGrid).unwrap()));
        for layer in [
            SpaceLayer::Axes,
            SpaceLayer::Ticks,
            SpaceLayer::Numbers,
            SpaceLayer::Labels,
        ] {
            assert!(layer_is_empty(space.layer(layer).unwrap()));
        }
    }

    #[test]
    fn three_dimensional_visibility_filters_grid_planes_and_annotations() {
        let mut canvas = SceneModel::new(640, 360);
        let space = canvas
            .coordinate_axes_3d_with_visibility(
                Axis::linear(-2.0, 2.0)
                    .unwrap()
                    .ticks(1.0)
                    .unwrap()
                    .label("x"),
                Axis::linear(-2.0, 2.0)
                    .unwrap()
                    .ticks(1.0)
                    .unwrap()
                    .label("y"),
                Axis::linear(-2.0, 2.0)
                    .unwrap()
                    .ticks(1.0)
                    .unwrap()
                    .label("z"),
                [4.0, 4.0, 4.0],
                Cartesian3DVisibility {
                    x_axis: false,
                    y_axis: false,
                    z_axis: false,
                    xy_grid: true,
                    xz_grid: false,
                    yz_grid: false,
                    x_ticks: false,
                    y_ticks: false,
                    z_ticks: false,
                    x_numbers: false,
                    y_numbers: false,
                    z_numbers: false,
                    x_labels: false,
                    y_labels: false,
                    z_labels: false,
                },
            )
            .unwrap();

        let grid = space.layer(SpaceLayer::MajorGrid).unwrap();
        let spec = grid.spec.lock().expect("grid spec poisoned");
        let SpawnKind::LineSegments3D { points, .. } = &spec.kind else {
            panic!("expected a 3D grid layer")
        };
        assert!(!points.is_empty());
        assert!(
            points
                .iter()
                .all(|point| (point[2] + 2.0).abs() < f32::EPSILON)
        );
        drop(spec);
        for layer in [
            SpaceLayer::Axes,
            SpaceLayer::Ticks,
            SpaceLayer::Numbers,
            SpaceLayer::Labels,
        ] {
            assert!(layer_is_empty(space.layer(layer).unwrap()));
        }
    }

    #[test]
    fn polar_and_number_line_visibility_keep_empty_layers_addressable() {
        let mut canvas = SceneModel::new(640, 360);
        let polar = canvas
            .coordinate_polar_plane_with_visibility(
                Axis::linear(0.0, 4.0)
                    .unwrap()
                    .ticks(1.0)
                    .unwrap()
                    .label("r"),
                160.0,
                8,
                PolarVisibility {
                    rings: false,
                    spokes: false,
                    axes: false,
                    numbers: false,
                    labels: false,
                },
            )
            .unwrap();
        for layer in [
            SpaceLayer::MajorGrid,
            SpaceLayer::Axes,
            SpaceLayer::Numbers,
            SpaceLayer::Labels,
        ] {
            assert!(layer_is_empty(polar.layer(layer).unwrap()));
        }

        let spokes_only = canvas
            .coordinate_polar_plane_with_visibility(
                Axis::linear(0.0, 4.0)
                    .unwrap()
                    .ticks(1.0)
                    .unwrap()
                    .label("r"),
                160.0,
                8,
                PolarVisibility {
                    rings: false,
                    spokes: true,
                    axes: false,
                    numbers: false,
                    labels: true,
                },
            )
            .unwrap();
        let grid = spokes_only.layer(SpaceLayer::MajorGrid).unwrap();
        let spec = grid.spec.lock().expect("polar grid spec poisoned");
        let SpawnKind::SvgPath(path) = &spec.kind else {
            panic!("expected a polar grid path")
        };
        assert_eq!(path.path.elements().len(), 16);
        drop(spec);
        assert!(!layer_is_empty(
            spokes_only.layer(SpaceLayer::Labels).unwrap()
        ));

        let line = canvas
            .coordinate_number_line_with_visibility(
                Axis::linear(0.0, 4.0)
                    .unwrap()
                    .ticks(1.0)
                    .unwrap()
                    .label("t"),
                Some(400.0),
                NumberLineVisibility {
                    axis: false,
                    ticks: false,
                    numbers: false,
                    labels: false,
                },
            )
            .unwrap();
        for layer in [
            SpaceLayer::Axes,
            SpaceLayer::Ticks,
            SpaceLayer::Numbers,
            SpaceLayer::Labels,
        ] {
            assert!(layer_is_empty(line.layer(layer).unwrap()));
        }
    }

    fn group_child_anchors(canvas: &SceneModel, group: &DrawableHandle) -> Vec<Option<Anchor>> {
        let group_spec = group.spec.lock().expect("group spec poisoned");
        let children = match &group_spec.kind {
            SpawnKind::Group(children) | SpawnKind::GroupNoCenter(children) => children.clone(),
            _ => panic!("expected a group"),
        };
        drop(group_spec);
        let state = canvas.state.lock().expect("canvas state poisoned");
        children
            .iter()
            .map(|id| {
                let spec = state
                    .active()
                    .ops
                    .iter()
                    .find_map(|op| match op {
                        Op::Spawn(spec) if spec.lock().expect("object spec poisoned").id == *id => {
                            Some(spec.clone())
                        }
                        _ => None,
                    })
                    .expect("label must have a spawn spec");
                spec.lock()
                    .expect("label spec poisoned")
                    .layout_ops
                    .iter()
                    .rev()
                    .find_map(|op| match op {
                        super::super::types::LayoutOp::MoveAnchorTo { anchor, .. } => Some(*anchor),
                        _ => None,
                    })
            })
            .collect()
    }

    fn group_child_rotations(canvas: &SceneModel, group: &DrawableHandle) -> Vec<f64> {
        let group_spec = group.spec.lock().expect("group spec poisoned");
        let children = match &group_spec.kind {
            SpawnKind::Group(children) | SpawnKind::GroupNoCenter(children) => children,
            _ => panic!("expected a group"),
        };
        let children = children.clone();
        drop(group_spec);
        let state = canvas.state.lock().expect("canvas state poisoned");
        children
            .iter()
            .map(|id| {
                let spec = state
                    .active()
                    .ops
                    .iter()
                    .find_map(|op| match op {
                        Op::Spawn(spec) if spec.lock().expect("object spec poisoned").id == *id => {
                            Some(spec.clone())
                        }
                        _ => None,
                    })
                    .expect("label must have a spawn spec");
                spec.lock()
                    .expect("label spec poisoned")
                    .layout_ops
                    .iter()
                    .rev()
                    .find_map(|op| match op {
                        super::super::types::LayoutOp::SetRotation(value) => Some(*value),
                        _ => None,
                    })
                    .expect("label must have an authored rotation")
            })
            .collect()
    }

    #[test]
    fn cartesian_axis_titles_default_beyond_positive_ends_and_support_multiline_text() {
        let mut canvas = SceneModel::new(640, 360);
        let space = canvas
            .coordinate_axes(
                Axis::category(["Ladrillo\no bloque".into(), "Piedra\ncon barro".into()])
                    .unwrap()
                    .label("Material predominante"),
                Axis::linear(0.0, 70.0)
                    .unwrap()
                    .ticks(10.0)
                    .unwrap()
                    .label("Viviendas\nprueba (%)"),
                Some(640.0),
                Some(360.0),
                true,
            )
            .unwrap();
        let labels = group_child_translations(
            &canvas,
            space.layer(SpaceLayer::Labels).expect("axis titles"),
        );
        let rotations = group_child_rotations(
            &canvas,
            space.layer(SpaceLayer::Labels).expect("axis titles"),
        );
        let anchors = group_child_anchors(
            &canvas,
            space.layer(SpaceLayer::Labels).expect("axis titles"),
        );

        assert_eq!(labels.len(), 2);
        assert!((labels[0].x - 320.12).abs() < 1e-9);
        assert!((labels[1].y - 180.12).abs() < 1e-9);
        assert_eq!(anchors, [Some(Anchor::Left), Some(Anchor::Bottom)]);
        assert_eq!(rotations, [0.0, 0.0], "axis titles remain upright");
        let axis_origin = space
            .map
            .data_to_local(space.map.x.crossing_value(), space.map.y.crossing_value())
            .unwrap();
        assert!((labels[0].y - axis_origin.y).abs() < 1e-9);
        assert!((labels[1].x - axis_origin.x).abs() < 1e-9);
    }

    #[test]
    fn multiline_category_ticks_align_their_first_line_with_single_line_ticks() {
        let mut canvas = SceneModel::new(400, 200);
        let space = canvas
            .coordinate_axes(
                Axis::category(["Una línea".into(), "Dos\nlíneas".into()]).unwrap(),
                Axis::linear(0.0, 1.0).unwrap(),
                Some(400.0),
                Some(200.0),
                true,
            )
            .unwrap();
        let numbers = group_child_translations(
            &canvas,
            space.layer(SpaceLayer::Numbers).expect("axis tick labels"),
        );

        let expected_offset = canvas.x_tick_label_extra_offset("Dos\nlíneas", 1.0)
            - canvas.x_tick_label_extra_offset("Una línea", 1.0);
        assert!(
            ((numbers[0].y - numbers[1].y) - expected_offset).abs() < 1e-9,
            "multiline labels grow away from the axis without changing their nearest-line gap"
        );
    }

    #[test]
    fn single_line_x_tick_labels_use_a_logical_gap() {
        let canvas = SceneModel::new(16.0, 9.0);
        assert_eq!(canvas.x_tick_label_extra_offset("Categoría", 1.0), 0.04);
    }

    #[test]
    fn centered_axis_titles_use_conventional_outer_sides() {
        let mut canvas = SceneModel::new(400, 200);
        let space = canvas
            .coordinate_axes(
                Axis::category(["Una línea".into(), "Dos\nlíneas".into()])
                    .unwrap()
                    .label("Categoría")
                    .label_position(AxisLabelPosition::Center),
                Axis::linear(-10.0, 10.0)
                    .unwrap()
                    .label("Valor")
                    .label_position(AxisLabelPosition::Center),
                Some(400.0),
                Some(200.0),
                true,
            )
            .unwrap();
        let numbers = group_child_translations(
            &canvas,
            space.layer(SpaceLayer::Numbers).expect("axis tick labels"),
        );
        let labels = group_child_translations(
            &canvas,
            space.layer(SpaceLayer::Labels).expect("axis titles"),
        );
        let rotations = group_child_rotations(
            &canvas,
            space.layer(SpaceLayer::Labels).expect("axis titles"),
        );

        assert!(numbers[0].y < 0.0 && numbers[1].y < numbers[0].y);
        assert_eq!(labels.len(), 2);
        assert!(
            labels[0].y < numbers[1].y,
            "centered x title clears tick labels below"
        );
        assert!(
            labels[1].x < -200.0,
            "centered y title stays left of the axis"
        );
        assert!(labels[0].x.abs() < 1e-9 && labels[1].y.abs() < 1e-9);
        assert_eq!(rotations[0], 0.0);
        assert!((rotations[1] - std::f64::consts::FRAC_PI_2).abs() < 1e-12);
    }

    #[test]
    fn cartesian_axis_titles_can_move_to_the_axis_end() {
        let mut canvas = SceneModel::new(400, 200);
        let space = canvas
            .coordinate_axes(
                Axis::linear(0.0, 4.0)
                    .unwrap()
                    .label("x")
                    .label_position(AxisLabelPosition::End),
                Axis::linear(0.0, 2.0)
                    .unwrap()
                    .label("y")
                    .label_position(AxisLabelPosition::End),
                Some(400.0),
                Some(200.0),
                true,
            )
            .unwrap();
        let labels = group_child_translations(
            &canvas,
            space.layer(SpaceLayer::Labels).expect("axis titles"),
        );
        let rotations = group_child_rotations(
            &canvas,
            space.layer(SpaceLayer::Labels).expect("axis titles"),
        );
        let anchors = group_child_anchors(
            &canvas,
            space.layer(SpaceLayer::Labels).expect("axis titles"),
        );

        assert_eq!(labels.len(), 2);
        assert_eq!(rotations, [0.0, 0.0]);
        assert_eq!(labels[0].x, 200.12);
        assert_eq!(labels[1].y, 100.12);
        assert_eq!(anchors, [Some(Anchor::Left), Some(Anchor::Bottom)]);
    }

    fn svg_stroke_color(handle: &DrawableHandle) -> Color {
        let spec = handle.spec.lock().expect("object spec poisoned");
        let SpawnKind::SvgPath(path) = &spec.kind else {
            panic!("expected an SVG path")
        };
        let Some(gaanim_core::peniko::Brush::Solid(color)) = &path.stroke.brush else {
            panic!("expected a solid stroke")
        };
        *color
    }

    #[test]
    fn paper_theme_colors_number_line_parts_unless_authored() {
        let mut canvas = SceneModel::new(640, 360);
        canvas.set_theme("paper").unwrap();
        let themed = canvas
            .coordinate_number_line(Axis::linear(0.0, 4.0).unwrap(), Some(400.0))
            .unwrap();
        assert_eq!(
            svg_stroke_color(themed.layer(SpaceLayer::Axes).unwrap()),
            Color::BLACK
        );
        assert_eq!(
            svg_stroke_color(themed.layer(SpaceLayer::Ticks).unwrap()),
            Color::BLACK
        );

        let width_only = canvas
            .coordinate_number_line(
                Axis::linear(0.0, 4.0)
                    .unwrap()
                    .style_patch(gaanim_visualization::AxisStylePatch {
                        width: Some(7.0),
                        ..Default::default()
                    }),
                Some(400.0),
            )
            .unwrap();
        assert_eq!(
            svg_stroke_color(width_only.layer(SpaceLayer::Axes).unwrap()),
            Color::BLACK,
            "a non-color override must not disconnect the axis from its theme color"
        );

        let authored = Color::from_rgb8(0xA1, 0x23, 0x45);
        let overridden = canvas
            .coordinate_number_line(
                Axis::linear(0.0, 4.0)
                    .unwrap()
                    .style_patch(gaanim_visualization::AxisStylePatch {
                        color: Some(authored),
                        tick_color: Some(authored),
                        ..Default::default()
                    }),
                Some(400.0),
            )
            .unwrap();
        assert_eq!(
            svg_stroke_color(overridden.layer(SpaceLayer::Axes).unwrap()),
            authored
        );
        assert_eq!(
            svg_stroke_color(overridden.layer(SpaceLayer::Ticks).unwrap()),
            authored
        );
    }

    #[test]
    fn number_line_point_ref_uses_reactive_local_coordinates() {
        let mut canvas = SceneModel::new(640, 360);
        let line = canvas
            .coordinate_number_line(
                Axis::linear(0.0, std::f64::consts::TAU).unwrap(),
                Some(600.0),
            )
            .unwrap();
        let point = line
            .point_ref(
                ScalarSource::constant(std::f64::consts::PI),
                ScalarSource::constant(42.0),
            )
            .unwrap();

        let CanvasEndpoint::LocalNumberLine {
            space,
            axis,
            length,
            value,
            normal_offset,
        } = point.0
        else {
            panic!("number-line points must stay in the line's local frame");
        };
        assert_eq!(space, line.drawable().id);
        assert_eq!(axis.domain(), (0.0, std::f64::consts::TAU));
        assert_eq!(length, 600.0);
        assert_eq!(value.evaluate(0.0, |_| None).unwrap(), std::f64::consts::PI);
        assert_eq!(normal_offset.evaluate(0.0, |_| None).unwrap(), 42.0);
    }

    #[test]
    fn number_line_default_placement_keeps_its_authored_axis_origin() {
        let mut canvas = SceneModel::new(640, 360);
        let line = canvas
            .coordinate_number_line(
                Axis::linear(0.0, std::f64::consts::TAU)
                    .unwrap()
                    .ticks(std::f64::consts::PI)
                    .unwrap(),
                Some(600.0),
            )
            .unwrap();
        line.drawable().clone().move_to_default(-250.0, 0.0);

        let mut world = bevy::prelude::World::new();
        world.insert_resource(gaanim_timeline::timeline::Timeline::new());
        world.insert_resource(gaanim_text::font::FontRegistry::new());
        world.insert_resource(gaanim_text::prelude::TextConfig::default());
        canvas.compile(&mut world);
        world.flush();

        let transform = world
            .query::<(
                &gaanim_math::SpatialTransform,
                Option<&bevy::prelude::ChildOf>,
            )>()
            .iter(&world)
            .find_map(|(transform, parent)| parent.is_none().then_some(transform))
            .expect("number-line root must be compiled");
        assert_eq!(transform.translation, DVec3::new(-250.0, 0.0, 0.0));
    }

    #[test]
    fn number_line_function_uses_one_reactive_path() {
        let mut canvas = SceneModel::new(640, 360);
        let amplitude = canvas.parameter(1.0).unwrap();
        let line = canvas
            .coordinate_number_line(Axis::linear(0.0, 6.0).unwrap(), Some(480.0))
            .unwrap();
        canvas
            .number_line_reactive_plot(
                &line,
                ReactiveFunction::new(
                    1,
                    1,
                    vec![gaanim_animation::ReactiveInput::Signal(
                        amplitude.drawable().id,
                    )],
                    |values| Ok(vec![values[1] * values[0].sin()]),
                ),
                (0.0, 6.0),
                80.0,
                None,
                Sampling::Fixed { samples: 128 },
            )
            .unwrap();

        let mut world = bevy::prelude::World::new();
        world.insert_resource(gaanim_timeline::timeline::Timeline::new());
        world.insert_resource(gaanim_text::font::FontRegistry::new());
        world.insert_resource(gaanim_text::prelude::TextConfig::default());
        canvas.compile(&mut world);
        world.flush();

        assert_eq!(
            world
                .query::<&gaanim_animation::AlwaysRedrawRegen>()
                .iter(&world)
                .count(),
            1,
            "a sampled number-line function must remain one retained path",
        );
    }

    #[test]
    fn animated_view_targets_internal_view_and_preserves_layout_root() {
        let mut canvas = SceneModel::new(640, 360);
        let space = canvas
            .coordinate_axes(
                Axis::linear(-4.0, 4.0).unwrap(),
                Axis::linear(-3.0, 3.0).unwrap(),
                Some(520.0),
                Some(280.0),
                true,
            )
            .unwrap()
            .move_to(37.0, -18.0);

        let coordinate = space.coord(1.0, 2.0).unwrap();
        let animation = space
            .view_to_animation((-2.0, 2.0), (-1.5, 1.5))
            .unwrap()
            .duration(1.2);

        assert_ne!(space.root.id, space.view.id);
        assert_eq!(coordinate.space, space.view.id);
        assert_eq!(animation.inner.target, space.view.id);
    }

    #[test]
    fn animated_view_rejects_non_affine_scales() {
        let mut canvas = SceneModel::new(640, 360);
        let space = canvas
            .coordinate_axes(
                Axis::log(0.1, 10.0, 10.0).unwrap(),
                Axis::linear(-3.0, 3.0).unwrap(),
                Some(520.0),
                Some(280.0),
                false,
            )
            .unwrap();

        assert!(matches!(
            space.view_to_animation((0.2, 5.0), (-1.0, 1.0)),
            Err(VisualizationError::UnsupportedAnimatedView)
        ));
    }

    #[test]
    fn parameter_drives_scalar_source_and_animation() {
        let mut canvas = SceneModel::new(640, 360);
        let parameter = canvas.parameter(1.5).unwrap();
        parameter.set(2.25).unwrap();

        assert_eq!(
            parameter
                .source()
                .evaluate(0.0, |id| (id == parameter.drawable().id)
                    .then_some(parameter.current()))
                .unwrap(),
            2.25
        );

        let animation = parameter.animate().set(4.0);
        assert_eq!(animation.inner.target, parameter.drawable().id);
        assert_eq!(parameter.current(), 2.25);
        assert!(matches!(
            parameter.set(f64::NAN),
            Err(VisualizationError::InvalidParameter)
        ));
    }

    #[test]
    fn three_dimensional_lines_honor_stroke_color() {
        let mut canvas = SceneModel::new(640, 360);
        let expected = Color::from_rgb8(0xE7, 0x4C, 0x3C);
        canvas
            .polyline_3d(vec![[0.0, 0.0, 0.0], [1.0, 1.0, 1.0]])
            .stroke(expected, 3.0);

        let mut world = bevy::prelude::World::new();
        world.insert_resource(gaanim_timeline::timeline::Timeline::new());
        world.insert_resource(gaanim_text::font::FontRegistry::new());
        world.insert_resource(gaanim_text::prelude::TextConfig::default());
        canvas.compile(&mut world);
        world.flush();

        let colors: Vec<Color> = world
            .query::<&gaanim_scene::LineListData>()
            .iter(&world)
            .map(|line| line.color)
            .collect();
        assert_eq!(colors, [expected]);
    }

    #[test]
    fn three_dimensional_grid_does_not_duplicate_axis_edges() {
        let mut canvas = SceneModel::new(640, 360);
        let _space = canvas
            .coordinate_axes_3d(
                Axis::linear(-2.0, 2.0).unwrap().ticks(1.0).unwrap(),
                Axis::linear(-2.0, 2.0).unwrap().ticks(1.0).unwrap(),
                Axis::linear(-2.0, 2.0).unwrap().ticks(1.0).unwrap(),
                [4.0, 4.0, 4.0],
                true,
            )
            .unwrap();
        let mut world = bevy::prelude::World::new();
        world.insert_resource(gaanim_timeline::timeline::Timeline::new());
        world.insert_resource(gaanim_text::font::FontRegistry::new());
        world.insert_resource(gaanim_text::prelude::TextConfig::default());
        canvas.compile(&mut world);
        world.flush();

        let mut lines = world.query::<&gaanim_scene::LineListData>();
        let compiled: Vec<_> = lines.iter(&world).cloned().collect();
        let axes = compiled
            .iter()
            .find(|line| {
                line.points.len() == 6
                    && line
                        .colors
                        .as_ref()
                        .is_some_and(|colors| colors.iter().all(|color| color[3] >= 0.999))
            })
            .unwrap();
        let grid = compiled
            .iter()
            .max_by_key(|line| line.points.len())
            .unwrap();
        assert!(
            grid.colors
                .as_ref()
                .is_some_and(|colors| colors.iter().all(|color| color[3] >= 0.999))
        );

        for axis in axes.points.chunks_exact(2) {
            assert!(grid.points.chunks_exact(2).all(|segment| {
                segment != axis && !(segment[0] == axis[1] && segment[1] == axis[0])
            }));
        }
    }

    #[test]
    fn native_three_dimensional_spawns_select_hybrid_rendering() {
        let mut canvas = SceneModel::new(640, 360);
        assert!(!canvas.has_native_3d_content());

        canvas.polyline_3d(vec![[0.0, 0.0, 0.0], [1.0, 1.0, 1.0]]);

        assert!(canvas.has_native_3d_content());
    }

    #[test]
    fn surfaces_include_a_batched_colored_wireframe_fallback() {
        let mut canvas = SceneModel::new(640, 360);
        let space = canvas
            .coordinate_axes_3d(
                Axis::linear(-2.0, 2.0).unwrap(),
                Axis::linear(-2.0, 2.0).unwrap(),
                Axis::linear(-2.0, 2.0).unwrap(),
                [4.0, 4.0, 4.0],
                false,
            )
            .unwrap();
        canvas
            .surface_plot(&space, [4, 4], |x, y| Some(x * y * 0.25))
            .unwrap();

        let mut world = bevy::prelude::World::new();
        world.insert_resource(gaanim_timeline::timeline::Timeline::new());
        world.insert_resource(gaanim_text::font::FontRegistry::new());
        world.insert_resource(gaanim_text::prelude::TextConfig::default());
        canvas.compile(&mut world);
        world.flush();

        assert_eq!(
            world
                .query::<&gaanim_scene::TriangleMeshData>()
                .iter(&world)
                .count(),
            1
        );
        assert!(
            world
                .query::<&gaanim_scene::LineListData>()
                .iter(&world)
                .any(|line| line.colors.as_ref().is_some_and(|colors| {
                    !line.strip && !line.points.is_empty() && colors.len() == line.points.len()
                }))
        );
    }

    #[test]
    fn declarative_scatter_keeps_one_reactive_batch_for_ten_thousand_rows() {
        let mut canvas = SceneModel::new(1920, 1080);
        let table = gaanim_visualization::DataTable::numeric([
            (
                "x".to_owned(),
                (0..10_000).map(|index| index as f64).collect(),
            ),
            (
                "y".to_owned(),
                (0..10_000)
                    .map(|index| (index as f64 * 0.01).sin())
                    .collect(),
            ),
        ])
        .unwrap();
        let spec = ChartSpec::new(table, None)
            .unwrap()
            .mark(MarkKind::Point, BTreeMap::new())
            .encode(Channel::X, Encoding::field("x"))
            .unwrap()
            .encode(Channel::Y, Encoding::field("y"))
            .unwrap();

        let chart = canvas.chart(spec).unwrap();
        assert_eq!(chart.layer("marks").unwrap().id, chart.marks.id);
        assert!(!canvas.has_native_3d_content());

        let mut world = bevy::prelude::World::new();
        world.insert_resource(gaanim_timeline::timeline::Timeline::new());
        world.insert_resource(gaanim_text::font::FontRegistry::new());
        world.insert_resource(gaanim_text::prelude::TextConfig::default());
        canvas.compile(&mut world);
        world.flush();

        assert_eq!(
            world
                .query::<&gaanim_animation::AlwaysRedrawRegen>()
                .iter(&world)
                .count(),
            1,
            "row count must not change the number of reactive mark entities",
        );
    }

    #[test]
    fn three_dimensional_axes_accept_nonlinear_scales_and_expose_layers() {
        let mut canvas = SceneModel::new(1280, 720);
        let space = canvas
            .coordinate_axes_3d(
                Axis::log(0.1, 1_000.0, 10.0).unwrap(),
                Axis::symlog(-100.0, 100.0, 10.0, 1.0).unwrap(),
                Axis::power(0.0, 16.0, 0.5).unwrap(),
                [10.0, 8.0, 6.0],
                true,
            )
            .unwrap();

        for layer in [
            SpaceLayer::MajorGrid,
            SpaceLayer::Axes,
            SpaceLayer::Ticks,
            SpaceLayer::Numbers,
            SpaceLayer::Labels,
        ] {
            assert!(space.layer(layer).is_some());
        }
        let point = [10.0, -12.5, 9.0];
        let local = space.data_to_local(point).unwrap();
        let restored = space.local_to_data(local).unwrap();
        for (actual, expected) in restored.into_iter().zip(point) {
            assert!((actual - expected).abs() < 1e-9);
        }
    }

    #[test]
    fn declarative_marks_receive_a_visible_default_series_style() {
        let table = gaanim_visualization::DataTable::numeric([
            ("x".to_owned(), vec![0.0, 1.0, 2.0]),
            ("y".to_owned(), vec![1.0, 2.0, 1.5]),
        ])
        .unwrap();
        let base = ChartSpec::new(table, None)
            .unwrap()
            .encode(Channel::X, Encoding::field("x"))
            .unwrap()
            .encode(Channel::Y, Encoding::field("y"))
            .unwrap();

        let mut point_canvas = SceneModel::new(640, 360);
        let points = point_canvas.chart(base.clone()).unwrap();
        assert!(
            points
                .marks
                .spec
                .lock()
                .expect("point mark spec poisoned")
                .fill
                .is_some(),
            "point marks without a color encoding must still be visible",
        );

        let mut line_canvas = SceneModel::new(640, 360);
        let line = line_canvas
            .chart(base.mark(MarkKind::Line, BTreeMap::new()))
            .unwrap();
        assert!(
            line.marks
                .spec
                .lock()
                .expect("line mark spec poisoned")
                .stroke
                .is_some(),
            "line marks without a color encoding must still be visible",
        );
    }

    #[test]
    fn declarative_axes_render_above_bar_marks() {
        let table = gaanim_visualization::DataTable::numeric([
            ("x".to_owned(), vec![0.0, 1.0, 2.0]),
            ("y".to_owned(), vec![3.0, 2.0, 1.0]),
        ])
        .unwrap();
        let spec = ChartSpec::new(table, None)
            .unwrap()
            .mark(MarkKind::Bar, BTreeMap::new())
            .encode(Channel::X, Encoding::field("x"))
            .unwrap()
            .encode(Channel::Y, Encoding::field("y"))
            .unwrap();

        let mut canvas = SceneModel::new(640, 360);
        let chart = canvas.chart(spec).unwrap();
        let mark_z = chart
            .marks
            .spec
            .lock()
            .expect("bar mark spec poisoned")
            .z_index;
        let axis_z = chart.axes.spec.lock().expect("axes spec poisoned").z_index;
        assert!(
            axis_z > mark_z,
            "semantic axes must remain readable when a bar intersects them",
        );
    }

    #[test]
    fn chart_axes_layer_targets_the_complete_structural_view() {
        let table = gaanim_visualization::DataTable::numeric([
            ("x".to_owned(), vec![0.0, 1.0, 2.0]),
            ("y".to_owned(), vec![3.0, 2.0, 1.0]),
        ])
        .unwrap();
        let spec = ChartSpec::new(table, None)
            .unwrap()
            .mark(MarkKind::Bar, BTreeMap::new())
            .encode(Channel::X, Encoding::field("x"))
            .unwrap()
            .encode(Channel::Y, Encoding::field("y"))
            .unwrap();

        let mut canvas = SceneModel::new(640, 360);
        let chart = canvas.chart(spec).unwrap();
        let axes = chart.layer("axes").expect("chart axes layer");
        assert!(
            matches!(
                axes.spec.lock().expect("axes spec poisoned").kind,
                SpawnKind::GroupNoCenter(_)
            ),
            "the chart axes layer must include grid, axes, ticks, numbers, and labels",
        );
    }

    #[test]
    fn categorical_bars_batch_by_resolved_color_and_expose_labels() {
        let table = gaanim_visualization::DataTable::new([
            (
                "method".to_owned(),
                gaanim_visualization::Column::Text(vec![
                    Some("baseline".into()),
                    Some("cached".into()),
                    Some("gpu".into()),
                ]),
            ),
            (
                "elapsed".to_owned(),
                gaanim_visualization::Column::Numeric(vec![Some(48.0), Some(-12.0), Some(21.0)]),
            ),
            (
                "kind".to_owned(),
                gaanim_visualization::Column::Text(vec![
                    Some("slow".into()),
                    Some("slow".into()),
                    Some("fast".into()),
                ]),
            ),
        ])
        .unwrap();
        let spec = ChartSpec::new(table, None)
            .unwrap()
            .mark(
                MarkKind::Bar,
                BTreeMap::from([
                    (
                        "label_position".into(),
                        ConstantValue::Text("inside".into()),
                    ),
                    ("label_offset".into(), ConstantValue::Number(12.0)),
                ]),
            )
            .encode(Channel::X, Encoding::field("method"))
            .unwrap()
            .encode(Channel::Y, Encoding::field("elapsed"))
            .unwrap()
            .encode(Channel::Label, Encoding::field("elapsed"))
            .unwrap()
            .encode(
                Channel::Color,
                Encoding::scaled_field(
                    "kind",
                    gaanim_visualization::ScaleSpec::category(["slow".into(), "fast".into()])
                        .unwrap()
                        .colors([
                            Color::from_rgb8(0x33, 0x66, 0xCC),
                            Color::from_rgb8(0xDD, 0x55, 0x55),
                        ]),
                ),
            )
            .unwrap();

        let mut canvas = SceneModel::new(640, 360);
        let chart = canvas.chart(spec).unwrap();
        assert!(chart.layer("labels").is_some());
        let mark_spec = chart.marks.spec.lock().expect("bar mark spec poisoned");
        let SpawnKind::GroupNoCenter(batches) = &mark_spec.kind else {
            panic!("coloured bars must be grouped vector batches");
        };
        assert_eq!(batches.len(), 2, "equal resolved colours share one batch");
    }

    #[test]
    fn bar_labels_follow_signed_extremes_inside_and_outside() {
        let table = gaanim_visualization::DataTable::new([
            (
                "method".to_owned(),
                gaanim_visualization::Column::Text(vec![Some("up".into()), Some("down".into())]),
            ),
            (
                "value".to_owned(),
                gaanim_visualization::Column::Numeric(vec![Some(40.0), Some(-10.0)]),
            ),
        ])
        .unwrap();
        let base = ChartSpec::new(table, None)
            .unwrap()
            .encode(Channel::X, Encoding::field("method"))
            .unwrap()
            .encode(Channel::Y, Encoding::field("value"))
            .unwrap()
            .encode(Channel::Label, Encoding::field("value"))
            .unwrap()
            .axis(Channel::Y, Axis::linear(-20.0, 50.0).unwrap())
            .unwrap();

        let mut outside_canvas = SceneModel::new(640, 360);
        let outside = outside_canvas
            .chart(base.clone().mark(
                MarkKind::Bar,
                BTreeMap::from([("label_offset".into(), ConstantValue::Number(12.0))]),
            ))
            .unwrap();
        let outside_labels = group_child_translations(
            &outside_canvas,
            outside.layer("labels").expect("labels should materialize"),
        );

        let mut inside_canvas = SceneModel::new(640, 360);
        let inside = inside_canvas
            .chart(base.mark(
                MarkKind::Bar,
                BTreeMap::from([
                    (
                        "label_position".into(),
                        ConstantValue::Text("inside".into()),
                    ),
                    ("label_offset".into(), ConstantValue::Number(12.0)),
                ]),
            ))
            .unwrap();
        let inside_labels = group_child_translations(
            &inside_canvas,
            inside.layer("labels").expect("labels should materialize"),
        );

        assert!(
            outside_labels[0].y > inside_labels[0].y,
            "positive labels invert inside"
        );
        assert!(
            outside_labels[1].y < inside_labels[1].y,
            "negative labels invert inside"
        );
        assert_eq!(
            outside_labels[0].y - inside_labels[0].y,
            24.0,
            "the positive label moves by twice its offset",
        );
        assert_eq!(
            inside_labels[1].y - outside_labels[1].y,
            24.0,
            "the negative label moves by twice its offset",
        );
    }

    #[test]
    fn cross_renderer_chart_transition_uses_hierarchy_aware_handoff() {
        let table = gaanim_visualization::DataTable::numeric([
            ("x".to_owned(), vec![0.0, 1.0, 0.0, 1.0]),
            ("y".to_owned(), vec![0.0, 0.0, 1.0, 1.0]),
            ("value".to_owned(), vec![0.0, 1.0, 1.0, 0.0]),
        ])
        .unwrap();
        let heatmap = ChartSpec::new(table.clone(), None)
            .unwrap()
            .mark(MarkKind::Heatmap, BTreeMap::new())
            .encode(Channel::X, Encoding::field("x"))
            .unwrap()
            .encode(Channel::Y, Encoding::field("y"))
            .unwrap()
            .encode(Channel::Color, Encoding::field("value"))
            .unwrap();
        let surface = ChartSpec::new(table, None)
            .unwrap()
            .mark(MarkKind::Surface, BTreeMap::new())
            .encode(Channel::X, Encoding::field("x"))
            .unwrap()
            .encode(Channel::Y, Encoding::field("y"))
            .unwrap()
            .encode(Channel::Z, Encoding::field("value"))
            .unwrap();

        let mut canvas = SceneModel::new(640, 360);
        let source = canvas.chart(heatmap).unwrap();
        let target = canvas.chart(surface).unwrap();
        let animation = source
            .transition_to(&target, MatchPolicy::Index, TransitionFallback::Error)
            .unwrap();
        assert!(matches!(
            animation.inner.anim_type,
            crate::anim::AnimationType::FadeTransform { .. }
        ));
    }

    #[test]
    fn coordinate_space_write_reveals_semantic_layers_in_parallel() {
        let mut canvas = SceneModel::new(640, 360);
        let space = canvas
            .coordinate_axes(
                Axis::linear(-2.0, 2.0).unwrap(),
                Axis::linear(-1.0, 1.0).unwrap(),
                Some(400.0),
                Some(200.0),
                true,
            )
            .unwrap();

        let animation = space
            .drawable()
            .animate()
            .write()
            .duration(1.5)
            .lag_ratio(0.0);
        let crate::anim::AnimationType::Write { config } = animation.inner.anim_type else {
            panic!("coordinate-space write should remain a Write animation");
        };
        assert_eq!(config.lag_ratio, Some(0.0));
        assert_eq!(animation.inner.duration, 1.5);
    }

    #[test]
    fn composed_vector_field_materializes_arrows_streams_and_particles() {
        let mut canvas = SceneModel::new(640, 360);
        let space = canvas
            .coordinate_axes(
                Axis::linear(-2.0, 2.0).unwrap(),
                Axis::linear(-2.0, 2.0).unwrap(),
                Some(400.0),
                Some(300.0),
                false,
            )
            .unwrap();
        let field = canvas.vector_field_2d(&space, |[x, y]| Some([-y, x]));
        let arrows = canvas
            .arrow_vector_field_2d(&field, [5, 5], ArrowFieldOptions::default())
            .unwrap();
        let style = StreamLinesStyle {
            integration: StreamlineOptions {
                max_time: 0.3,
                ..Default::default()
            },
            ..Default::default()
        };
        let streams = canvas.stream_lines_2d(&field, [4, 4], style).unwrap();
        let particles = canvas
            .flow_particles_2d(
                &field,
                4,
                FlowParticleOptions {
                    duration: 0.5,
                    integration: StreamlineOptions {
                        direction: gaanim_visualization::StreamDirection::Forward,
                        max_time: 0.25,
                        ..Default::default()
                    },
                    ..Default::default()
                },
            )
            .unwrap();

        assert_ne!(arrows.drawable().id, streams.drawable().id);
        let stream_flow = streams.flow(0.5, 0.1);
        assert!(!stream_flow.is_empty());
        assert_eq!(stream_flow.len(), streams.flow_lines.len() * 2);
        assert!(stream_flow.iter().all(|animation| {
            streams
                .flow_lines
                .iter()
                .any(|line| line.id == animation.inner.target)
                && streams
                    .lines
                    .iter()
                    .all(|line| line.id != animation.inner.target)
        }));
        assert!(streams.flow_lines.iter().all(|line| {
            let spec = line.spec.lock().unwrap();
            spec.opacity == 0.0 && spec.opacity_overridden
        }));
        assert_eq!(
            stream_flow
                .iter()
                .filter(|animation| matches!(
                    animation.inner.anim_type,
                    crate::anim::AnimationType::ShowPassingFlash { .. }
                ))
                .count(),
            streams.flow_lines.len()
        );
        assert!(matches!(
            streams
                .drawable()
                .animate()
                .write()
                .duration(0.5)
                .inner
                .anim_type,
            crate::anim::AnimationType::Write { .. }
        ));
        assert!(matches!(
            particles
                .drawable()
                .animate()
                .create()
                .duration(0.5)
                .inner
                .anim_type,
            crate::anim::AnimationType::Create { .. }
        ));
        assert!(matches!(
            particles
                .drawable()
                .animate()
                .fade_out()
                .duration(0.5)
                .inner
                .anim_type,
            crate::anim::AnimationType::FadeOut
        ));
        assert_eq!(particles.flow().len(), 4);
        assert!(particles.flow().iter().all(|animation| matches!(
            animation.inner.anim_type,
            crate::anim::AnimationType::MoveAlongPath { .. }
        )));
        let particle_ids = match &particles.drawable.spec.lock().unwrap().kind {
            SpawnKind::GroupNoCenter(children) => children.clone(),
            other => panic!("expected particle group, got {other:?}"),
        };
        let state = canvas.state.lock().unwrap();
        assert!(particle_ids.iter().all(|id| state.active().ops.iter().any(
            |op| matches!(op, Op::Spawn(spec) if spec.lock().unwrap().id == *id && spec.lock().unwrap().defer_visibility_until_play)
        )));
    }

    #[test]
    fn vector_field_explicit_colors_win_over_theme_plot_rules() {
        fn authored_and_resolved_stroke(
            handle: &DrawableHandle,
            theme: &crate::canvas::CanvasTheme,
        ) -> (gaanim_core::peniko::Brush, gaanim_core::peniko::Brush) {
            let authored = handle.spec.lock().expect("field spec poisoned").clone();
            let authored_stroke = match &authored.kind {
                SpawnKind::SvgPath(path) => path
                    .stroke
                    .brush
                    .clone()
                    .expect("field paths have a stroke"),
                other => panic!("expected an SVG field path, got {other:?}"),
            };
            let resolved = theme.resolve_object(&authored).unwrap();
            let resolved_stroke = resolved
                .stroke
                .expect("the resolved field path should retain a stroke")
                .0;
            (authored_stroke, resolved_stroke)
        }

        let mut canvas = SceneModel::new(640, 360);
        canvas.set_theme("technical").unwrap();
        let theme = canvas.theme_style.clone().unwrap();
        let space = canvas
            .coordinate_axes(
                Axis::linear(-2.0, 2.0).unwrap(),
                Axis::linear(-2.0, 2.0).unwrap(),
                Some(400.0),
                Some(300.0),
                false,
            )
            .unwrap();
        let field = canvas.vector_field_2d(&space, |[x, y]| Some([-y, x]));
        let arrows = canvas
            .arrow_vector_field_2d(&field, [3, 3], ArrowFieldOptions::default())
            .unwrap();
        let streams = canvas
            .stream_lines_2d(
                &field,
                [3, 3],
                StreamLinesStyle {
                    integration: StreamlineOptions {
                        max_time: 0.2,
                        ..Default::default()
                    },
                    ..Default::default()
                },
            )
            .unwrap();

        let arrow_id = match &arrows.drawable.spec.lock().unwrap().kind {
            SpawnKind::GroupNoCenter(children) => children[0],
            other => panic!("expected arrow group, got {other:?}"),
        };
        let arrow_spec = canvas
            .state
            .lock()
            .unwrap()
            .active()
            .ops
            .iter()
            .find_map(|op| match op {
                Op::Spawn(spec) if spec.lock().unwrap().id == arrow_id => Some(spec.clone()),
                _ => None,
            })
            .expect("arrow glyph should have a spawn spec");
        let arrow = DrawableHandle::new(
            arrow_id,
            arrow_spec.lock().unwrap().kind.clone(),
            arrows.drawable.state.clone(),
            arrows.drawable.segment_idx,
        );
        *arrow.spec.lock().unwrap() = arrow_spec.lock().unwrap().clone();
        let (authored, resolved) = authored_and_resolved_stroke(&arrow, &theme);
        assert_eq!(resolved, authored, "theme replaced an arrow colormap color");

        let (authored, resolved) = authored_and_resolved_stroke(&streams.lines[0], &theme);
        assert_eq!(
            resolved, authored,
            "theme replaced a streamline colormap color"
        );
    }

    #[test]
    fn three_dimensional_advection_uses_native_polyline_lens() {
        let mut canvas = SceneModel::new(640, 360);
        let space = canvas
            .coordinate_axes_3d(
                Axis::linear(-2.0, 2.0).unwrap(),
                Axis::linear(-2.0, 2.0).unwrap(),
                Axis::linear(-2.0, 2.0).unwrap(),
                [4.0, 4.0, 4.0],
                false,
            )
            .unwrap();
        let field = canvas.vector_field_3d(&space, |[x, y, z]| Some([-y, x, 0.2 - z]));
        let target = canvas
            .sphere(0.05, 8, 6, gaanim_scene::Material3D::matte(Color::WHITE))
            .unwrap();
        let animation = canvas
            .advect_3d(
                &field,
                &target,
                [1.0, 0.0, 0.0],
                StreamlineOptions {
                    direction: gaanim_visualization::StreamDirection::Forward,
                    max_time: 0.3,
                    ..Default::default()
                },
                0.5,
            )
            .unwrap();
        assert!(matches!(
            animation.inner.anim_type,
            crate::anim::AnimationType::MoveAlongPath3D { .. }
        ));
    }

    #[test]
    fn parameter_fields_compile_geometry_regenerators() {
        let mut canvas = SceneModel::new(640, 360);
        let parameter = canvas.parameter(1.0).unwrap();
        let space = canvas
            .coordinate_axes(
                Axis::linear(-2.0, 2.0).unwrap(),
                Axis::linear(-2.0, 2.0).unwrap(),
                Some(400.0),
                Some(300.0),
                false,
            )
            .unwrap();
        let field = canvas
            .vector_field_2d_reactive(
                &space,
                ReactiveFunction::new(
                    2,
                    2,
                    vec![gaanim_animation::ReactiveInput::Signal(
                        parameter.drawable().id,
                    )],
                    |values| Ok(vec![-values[1] * values[2], values[0] * values[2]]),
                ),
            )
            .unwrap();
        canvas
            .arrow_vector_field_2d(&field, [3, 3], ArrowFieldOptions::default())
            .unwrap();
        canvas
            .stream_lines_2d(
                &field,
                [3, 3],
                StreamLinesStyle {
                    integration: StreamlineOptions {
                        max_time: 0.2,
                        ..Default::default()
                    },
                    ..Default::default()
                },
            )
            .unwrap();
        let space_3d = canvas
            .coordinate_axes_3d(
                Axis::linear(-1.0, 1.0).unwrap(),
                Axis::linear(-1.0, 1.0).unwrap(),
                Axis::linear(-1.0, 1.0).unwrap(),
                [2.0, 2.0, 2.0],
                false,
            )
            .unwrap();
        let field_3d = canvas
            .vector_field_3d_reactive(
                &space_3d,
                ReactiveFunction::new(
                    3,
                    3,
                    vec![gaanim_animation::ReactiveInput::Signal(
                        parameter.drawable().id,
                    )],
                    |values| {
                        Ok(vec![
                            -values[1] * values[3],
                            values[0] * values[3],
                            -values[2] * values[3],
                        ])
                    },
                ),
            )
            .unwrap();
        canvas
            .arrow_vector_field_3d(&field_3d, [2, 2, 2], ArrowFieldOptions::default())
            .unwrap();

        let mut world = bevy::prelude::World::new();
        world.insert_resource(gaanim_timeline::timeline::Timeline::new());
        world.insert_resource(gaanim_text::font::FontRegistry::new());
        world.insert_resource(gaanim_text::prelude::TextConfig::default());
        canvas.compile(&mut world);
        world.flush();

        let regenerators = world
            .query::<&gaanim_animation::Updater>()
            .iter(&world)
            .count();
        assert!(
            regenerators > 1,
            "every field representation should remain reactive"
        );
    }

    #[test]
    fn reactive_parametric_curves_and_surfaces_compile_regenerators() {
        let mut canvas = SceneModel::new(640, 360);
        let parameter = canvas.parameter(1.0).unwrap();
        let input = vec![gaanim_animation::ReactiveInput::Signal(
            parameter.drawable().id,
        )];
        let space = canvas
            .coordinate_axes(
                Axis::linear(-2.0, 2.0).unwrap(),
                Axis::linear(-2.0, 2.0).unwrap(),
                Some(400.0),
                Some(300.0),
                false,
            )
            .unwrap();
        canvas
            .reactive_parametric_plot(
                &space,
                ReactiveFunction::new(1, 2, input.clone(), |values| {
                    Ok(vec![values[0], values[0] * values[1]])
                }),
                (-1.0, 1.0),
                Sampling::Fixed { samples: 16 },
            )
            .unwrap();
        let space_3d = canvas
            .coordinate_axes_3d(
                Axis::linear(-1.0, 1.0).unwrap(),
                Axis::linear(-1.0, 1.0).unwrap(),
                Axis::linear(-1.0, 1.0).unwrap(),
                [2.0, 2.0, 2.0],
                false,
            )
            .unwrap();
        canvas
            .reactive_parametric_plot_3d(
                &space_3d,
                ReactiveFunction::new(1, 3, input.clone(), |values| {
                    Ok(vec![values[0], values[0] * values[1], 0.0])
                }),
                (-1.0, 1.0),
                16,
            )
            .unwrap();
        canvas
            .reactive_surface_plot(
                &space_3d,
                ReactiveFunction::new(2, 1, input, |values| {
                    Ok(vec![values[2] * values[0] * values[1]])
                }),
                [4, 4],
            )
            .unwrap();

        let mut world = bevy::prelude::World::new();
        world.insert_resource(gaanim_timeline::timeline::Timeline::new());
        world.insert_resource(gaanim_text::font::FontRegistry::new());
        world.insert_resource(gaanim_text::prelude::TextConfig::default());
        canvas.compile(&mut world);
        world.flush();

        assert_eq!(
            world
                .query::<&gaanim_animation::AlwaysRedrawRegen>()
                .iter(&world)
                .count(),
            1
        );
        assert_eq!(
            world
                .query::<&gaanim_animation::ReactiveLineRegen>()
                .iter(&world)
                .count(),
            1
        );
        assert_eq!(
            world
                .query::<&gaanim_animation::ReactiveMeshRegen>()
                .iter(&world)
                .count(),
            1
        );
    }
}
