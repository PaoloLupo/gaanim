use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, Mutex};

use gaanim_core::ObjectId;
use gaanim_core::glam::DVec3;
use gaanim_core::kurbo::{BezPath, Circle, Point, Rect, Shape};
use gaanim_core::peniko::Color;
use gaanim_expr::Expr;
use gaanim_objects::prelude::SvgPath;
use gaanim_visualization::{
    Axis, CartesianSpace, Channel, ChartSpec, ConstantValue, CoordinateMap2D, CoordinateMap3D,
    DataMarkKind, Encoding, MarkKind, MatchPolicy, NonFinitePolicy, NumberLine, PlotFrame,
    PolarSpace, Sampling, Scale, SpaceLayer, TransitionFallback, area_path, bars, box_stats,
    error_bar_path, histogram, implicit_contours, line_path, sample_function, sample_parametric,
    sample_surface, sample_vector_field, scatter_points, step_path, violin_path,
};

use super::ops::Op;
use super::{Canvas, CanvasEndpoint, DrawableHandle, PointRef, SpawnKind};

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum VisualizationError {
    #[error(transparent)]
    Axis(#[from] gaanim_visualization::AxisError),
    #[error(transparent)]
    Sampling(#[from] gaanim_visualization::SamplingError),
    #[error(transparent)]
    Chart(#[from] gaanim_visualization::ChartError),
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

/// A point expressed in one coordinate space's local data mapping.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CoordinateRef {
    pub space: ObjectId,
    pub local: DVec3,
}

/// Animatable scalar that can be embedded directly in a native expression.
#[derive(Debug, Clone)]
pub struct Parameter {
    handle: DrawableHandle,
    value: Arc<Mutex<f64>>,
}

impl Parameter {
    pub fn drawable(&self) -> &DrawableHandle {
        &self.handle
    }

    /// Internal expression representation used by the language bindings.
    /// Public callers compose parameters directly rather than depending on
    /// the expression AST.
    pub fn expression(&self) -> Expr {
        Expr::parameter(self.handle.id)
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

    pub fn animate_to(&self, value: f64) -> Result<super::types::Anim, VisualizationError> {
        if !value.is_finite() {
            return Err(VisualizationError::InvalidParameter);
        }
        *self.value.lock().expect("parameter poisoned") = value;
        Ok(self.handle.animate_value_to(value))
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

    pub fn at(self, x: f64, y: f64) -> Self {
        self.root.clone().at(x, y);
        self
    }

    pub fn at_3d(self, x: f64, y: f64, z: f64) -> Self {
        self.root.clone().at_3d(x, y, z);
        self
    }

    pub fn scaled(self, factor: f64) -> Self {
        self.root.clone().scaled(factor);
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

fn chart_series_color(canvas: &Canvas) -> Color {
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
        value: Expr,
        normal_offset: Expr,
    ) -> Result<PointRef, VisualizationError> {
        Ok(PointRef(CanvasEndpoint::LocalExpression {
            space: self.root.id,
            x: self.line.data_to_local_expr(value)?,
            y: normal_offset,
            z: Expr::constant(0.0),
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

    pub fn at(self, point: [f64; 3]) -> Self {
        self.root.clone().at_3d(point[0], point[1], point[2]);
        self
    }

    pub fn scaled(self, factor: f64) -> Self {
        self.root.clone().scaled_3d(factor, factor, factor);
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

    pub fn at(self, x: f64, y: f64) -> Self {
        self.root.clone().at(x, y);
        self
    }

    pub fn scaled(self, factor: f64) -> Self {
        self.root.clone().scaled(factor);
        self
    }

    pub fn rotated(self, radians: f64) -> Self {
        self.root.clone().rotated(radians);
        self
    }

    /// Write axes, guides, ticks, numbers, and labels concurrently.
    ///
    /// A normal group `Write` staggers visual leaves. Coordinate spaces use a
    /// zero lag so their semantic layers are constructed together, like a
    /// Manim number plane.
    pub fn write(&self, duration: Option<f64>) -> super::types::Anim {
        self.root.write(duration).lag_ratio(0.0)
    }

    /// Animate an affine view window. Associated plots and marks are children
    /// of the space's internal view group, so they remain aligned throughout
    /// the view change without overwriting layout transforms on the root.
    pub fn animate_view(
        &self,
        x_domain: (f64, f64),
        y_domain: (f64, f64),
        duration: f64,
    ) -> Result<Vec<super::types::Anim>, VisualizationError> {
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
            || !duration.is_finite()
            || duration < 0.0
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
        Ok(vec![
            self.view
                .scale_to_3d(scale_x, scale_y, 1.0)
                .duration(duration),
            self.view
                .move_to(-center.x * scale_x, -center.y * scale_y)
                .duration(duration),
        ])
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

impl Canvas {
    /// Create an animatable native scalar for reactive expressions.
    pub fn parameter(&mut self, initial: f64) -> Result<Parameter, VisualizationError> {
        if !initial.is_finite() {
            return Err(VisualizationError::InvalidParameter);
        }
        Ok(Parameter {
            handle: self.value_tracker(initial),
            value: Arc::new(Mutex::new(initial)),
        })
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
        let width = (self.width as f64 * 0.72).max(320.0);
        let height = (self.height as f64 * 0.62).max(220.0);
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
                            .at(width * 0.5 + 70.0, height * 0.5 - index as f64 * 34.0)
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
                let axes_layer = space
                    .layer(SpaceLayer::Axes)
                    .cloned()
                    .unwrap_or_else(|| space.drawable().clone())
                    .z_index(20);
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
                            chart_encoding_number(&spec, Channel::Size, 6.0),
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
                    3.0,
                ),
                _ => mark
                    .fill(chart_encoding_color(&spec).unwrap_or_else(|| chart_series_color(self))),
            };
            let mark = mark
                .opacity(chart_encoding_number(&spec, Channel::Opacity, 1.0).clamp(0.0, 1.0) as f32)
                .z_index(0);
            let axes_layer = space
                .layer(SpaceLayer::Axes)
                .cloned()
                .unwrap_or_else(|| space.drawable().clone())
                .z_index(20);
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
        let label_offset = chart_option_number(spec, "label_offset", 16.0);
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
                        .at(center_x, center_y + direction * label_offset)
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
        push_axis(min, [max[0], min[1], min[2]], x.style_value().color);
        push_axis(min, [min[0], max[1], min[2]], y.style_value().color);
        push_axis(min, [min[0], min[1], max[2]], z.style_value().color);
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
                if tick.major && grid && normalized > 1e-12 {
                    let lines = match dimension {
                        0 => [
                            ([coordinate, min[1], min[2]], [coordinate, max[1], min[2]]),
                            ([coordinate, min[1], min[2]], [coordinate, min[1], max[2]]),
                        ],
                        1 => [
                            ([min[0], coordinate, min[2]], [max[0], coordinate, min[2]]),
                            ([min[0], coordinate, min[2]], [min[0], coordinate, max[2]]),
                        ],
                        _ => [
                            ([min[0], min[1], coordinate], [max[0], min[1], coordinate]),
                            ([min[0], min[1], coordinate], [min[0], max[1], coordinate]),
                        ],
                    };
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
                tick_points.extend_from_slice(&[as_point(from), as_point(to)]);
                tick_colors.extend_from_slice(&[tick_color, tick_color]);
                if tick.major && !tick.label.is_empty() {
                    numbers.push(
                        self.text(&tick.label)
                            .fill(axis.style_value().number_color)
                            .at_3d(label_position[0], label_position[1], label_position[2])
                            .billboard()
                            .scaled(0.016),
                    );
                }
            }
        }
        let grid_handle = self.line_segments_3d_with_colors(grid_points, grid_colors);
        let ticks = self.line_segments_3d_with_colors(tick_points, tick_colors);
        let number_refs: Vec<_> = numbers.iter().collect();
        let numbers = self.group_no_center(&number_refs);
        let mut labels = Vec::new();
        for (axis, position) in [
            (&x, [max[0] + tick_half * 5.0, min[1], min[2]]),
            (&y, [min[0], max[1] + tick_half * 5.0, min[2]]),
            (&z, [min[0], min[1], max[2] + tick_half * 5.0]),
        ] {
            if let Some(label) = axis.label_text() {
                labels.push(
                    self.text(label)
                        .fill(axis.style_value().label_color)
                        .at_3d(position[0], position[1], position[2])
                        .billboard()
                        .scaled(0.019),
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

    pub fn vector_field_plot_3d(
        &mut self,
        space: &CoordinateSpace3DHandle,
        resolution: [usize; 3],
        max_length: f64,
        mut evaluator: impl FnMut(f64, f64, f64) -> Option<[f64; 3]>,
    ) -> Result<DrawableHandle, VisualizationError> {
        if resolution.iter().any(|count| *count < 2) || !max_length.is_finite() || max_length <= 0.0
        {
            return Err(VisualizationError::InvalidSize);
        }
        let domains = [
            space.map.x.domain(),
            space.map.y.domain(),
            space.map.z.domain(),
        ];
        let mut points = Vec::new();
        for iz in 0..resolution[2] {
            for iy in 0..resolution[1] {
                for ix in 0..resolution[0] {
                    let data = [
                        domains[0].0
                            + (domains[0].1 - domains[0].0) * ix as f64
                                / (resolution[0] - 1) as f64,
                        domains[1].0
                            + (domains[1].1 - domains[1].0) * iy as f64
                                / (resolution[1] - 1) as f64,
                        domains[2].0
                            + (domains[2].1 - domains[2].0) * iz as f64
                                / (resolution[2] - 1) as f64,
                    ];
                    let Some(vector) = evaluator(data[0], data[1], data[2]) else {
                        continue;
                    };
                    if vector.iter().any(|value| !value.is_finite()) {
                        continue;
                    }
                    let start = space.map.data_to_local(data)?;
                    let displaced = [
                        data[0] + vector[0],
                        data[1] + vector[1],
                        data[2] + vector[2],
                    ];
                    let Ok(displaced) = space.map.data_to_local(displaced) else {
                        continue;
                    };
                    let direction = [
                        displaced[0] - start[0],
                        displaced[1] - start[1],
                        displaced[2] - start[2],
                    ];
                    let magnitude = (direction[0] * direction[0]
                        + direction[1] * direction[1]
                        + direction[2] * direction[2])
                        .sqrt();
                    if magnitude <= f64::EPSILON {
                        continue;
                    }
                    let length = magnitude.min(max_length);
                    let end = [
                        start[0] + direction[0] / magnitude * length,
                        start[1] + direction[1] / magnitude * length,
                        start[2] + direction[2] / magnitude * length,
                    ];
                    points.push([start[0] as f32, start[1] as f32, start[2] as f32]);
                    points.push([end[0] as f32, end[1] as f32, end[2] as f32]);
                }
            }
        }
        let handle = self
            .line_segments_3d(points)
            .fill(Color::from_rgb8(0x2E, 0x86, 0xAB));
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
        let mut space = if grid {
            CartesianSpace::number_plane(x, y, frame)
        } else {
            CartesianSpace::axes(x, y, frame)
        };
        if let Some(grid_color) = themed_grid_color {
            space.grid_color = grid_color;
            let rgba = grid_color.to_rgba8();
            space.minor_grid_color = Color::from_rgba8(rgba.r, rgba.g, rgba.b, rgba.a / 2);
        }
        let geometry = space.geometry()?;
        let mut layers = HashMap::new();
        let grid_major = self.visualization_path(
            geometry.major_grid,
            geometry.bounds,
            space.grid_color,
            1.0,
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
            0.6,
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
        let number_handles: Vec<DrawableHandle> = geometry
            .numbers
            .iter()
            .map(|label| {
                self.text(&label.text)
                    .fill(label.color)
                    .scaled(number_scale)
                    .at(label.position.x, label.position.y)
            })
            .collect();
        let number_refs: Vec<&DrawableHandle> = number_handles.iter().collect();
        let numbers = self.group(&number_refs);
        layers.insert(SpaceLayer::Numbers, numbers.clone());

        let label_handles: Vec<DrawableHandle> = geometry
            .labels
            .iter()
            .map(|label| {
                self.text(&label.text)
                    .fill(label.color)
                    .scaled(label_scale)
                    .at(label.position.x, label.position.y)
            })
            .collect();
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
        axis_path.move_to(Point::new(-length * 0.5, 0.0));
        axis_path.line_to(Point::new(length * 0.5, 0.0));
        let bounds = gaanim_math::Bounds3D::new_2d(
            -length * 0.5,
            -style.tick_length - 56.0,
            length * 0.5,
            style.tick_length + 32.0,
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
            tick_path.move_to(Point::new(x, -half));
            tick_path.line_to(Point::new(x, half));
            if tick.major && !tick.label.is_empty() {
                number_handles.push(
                    self.text(&tick.label)
                        .fill(style.number_color)
                        .scaled(number_scale)
                        .at(x, -style.tick_length - 22.0),
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
        let labels = if let Some(label) = axis.label_text() {
            let label = self
                .text(label)
                .fill(style.label_color)
                .scaled(label_scale)
                .at(length * 0.5 + 30.0, 0.0);
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
                grid_path.extend(Circle::new(Point::ORIGIN, ring_radius).to_path(0.1));
                if tick.major && !tick.label.is_empty() {
                    numbers_handles.push(
                        self.text(&tick.label)
                            .fill(style.number_color)
                            .scaled(number_scale)
                            .at(ring_radius, -20.0),
                    );
                }
            }
        }
        for index in 0..angle_divisions {
            let angle = std::f64::consts::TAU * index as f64 / angle_divisions as f64;
            grid_path.move_to(Point::ORIGIN);
            grid_path.line_to(Point::new(radius * angle.cos(), radius * angle.sin()));
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
        axes_path.move_to(Point::new(-radius, 0.0));
        axes_path.line_to(Point::new(radius, 0.0));
        axes_path.move_to(Point::new(0.0, -radius));
        axes_path.line_to(Point::new(0.0, radius));
        let axes = self.themed_axis_path(axes_path, bounds, style.color, style.width, "PolarAxes");
        let number_refs: Vec<&DrawableHandle> = numbers_handles.iter().collect();
        let numbers = self.group(&number_refs);
        let root = self.group(&[&grid, &axes, &numbers]);
        let layers = HashMap::from([
            (SpaceLayer::MajorGrid, grid),
            (SpaceLayer::Axes, axes),
            (SpaceLayer::Numbers, numbers),
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

    pub fn expression_plot(
        &mut self,
        space: &CoordinateSpaceHandle,
        expression: Expr,
        variable: impl Into<String>,
        domain: (f64, f64),
        sampling: Sampling,
    ) -> Result<DrawableHandle, VisualizationError> {
        if !domain.0.is_finite() || !domain.1.is_finite() || domain.0 >= domain.1 {
            return Err(gaanim_visualization::SamplingError::InvalidDomain.into());
        }
        let variable = variable.into();
        if variable.trim().is_empty() {
            return Err(gaanim_visualization::SamplingError::InvalidDomain.into());
        }
        let handle = self.spawn(SpawnKind::ExpressionPlot {
            map: space.map.clone(),
            expression,
            variable,
            domain,
            reveal: None,
            sampling,
        });
        self.attach_to_space(space, &handle);
        Ok(handle)
    }

    /// Plot a dimensionless scalar function perpendicular to a number line.
    /// Values `-1` and `1` map to `-normal_scale` and `normal_scale` local units.
    pub fn number_line_expression_plot(
        &mut self,
        line: &NumberLineHandle,
        expression: Expr,
        variable: impl Into<String>,
        domain: (f64, f64),
        normal_scale: f64,
        reveal: Option<Expr>,
        sampling: Sampling,
    ) -> Result<DrawableHandle, VisualizationError> {
        if !normal_scale.is_finite() || normal_scale <= 0.0 {
            return Err(VisualizationError::InvalidSize);
        }
        let map = CoordinateMap2D::new(
            line.line.axis.clone(),
            Axis::linear(-1.0, 1.0)?,
            PlotFrame::new(line.line.length, normal_scale * 2.0)?,
        );
        let variable = variable.into();
        if variable.trim().is_empty() {
            return Err(gaanim_visualization::SamplingError::InvalidDomain.into());
        }
        if !domain.0.is_finite() || !domain.1.is_finite() || domain.0 >= domain.1 {
            return Err(gaanim_visualization::SamplingError::InvalidDomain.into());
        }
        let handle = self.spawn(SpawnKind::ExpressionPlot {
            map,
            expression,
            variable,
            domain,
            reveal,
            sampling,
        });
        self.attach_to_number_line(line, &handle);
        Ok(handle)
    }

    /// Create a numeric glyph path sourced from a native expression. The
    /// compiler resolves parameter entities and installs the visualizer-phase
    /// update system, so Python is only involved while constructing the scene.
    pub fn expression_readout(
        &mut self,
        expression: Expr,
        format: impl Into<String>,
        prefix: impl Into<String>,
        suffix: impl Into<String>,
        invalid: impl Into<String>,
        font_size: Option<f64>,
    ) -> DrawableHandle {
        self.spawn(SpawnKind::ExpressionReadout {
            expression,
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

    pub fn vector_field_plot(
        &mut self,
        space: &CoordinateSpaceHandle,
        resolution: [usize; 2],
        max_length: f64,
        evaluator: impl FnMut(f64, f64) -> Option<(f64, f64)>,
    ) -> Result<DrawableHandle, VisualizationError> {
        let glyphs = sample_vector_field(&space.map, resolution, max_length, evaluator)?;
        let mut path = BezPath::new();
        for glyph in glyphs {
            path.move_to(glyph.start);
            path.line_to(glyph.end);
        }
        let handle = self.visualization_path(
            path,
            space.map.frame.bounds(),
            Color::from_rgb8(0x2E, 0x86, 0xAB),
            2.0,
            "VectorField",
        );
        self.attach_to_space(space, &handle);
        Ok(handle)
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
                let mark = self.dot(radius).at(point.x, point.y);
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
                    .at(
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
                .at((left.x + right.x) * 0.5, (left.y + right.y) * 0.5),
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

    fn group_child_translations(canvas: &Canvas, group: &DrawableHandle) -> Vec<DVec3> {
        let group_spec = group.spec.lock().expect("group spec poisoned");
        let SpawnKind::GroupNoCenter(children) = &group_spec.kind else {
            panic!("expected an uncentered group");
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
                        _ => None,
                    })
                    .expect("label must have an authored translation")
            })
            .collect()
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
        let mut canvas = Canvas::new(640, 360);
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
        let mut canvas = Canvas::new(640, 360);
        let line = canvas
            .coordinate_number_line(
                Axis::linear(0.0, std::f64::consts::TAU).unwrap(),
                Some(600.0),
            )
            .unwrap();
        let point = line
            .point_ref(Expr::constant(std::f64::consts::PI), Expr::constant(42.0))
            .unwrap();

        let CanvasEndpoint::LocalExpression { space, x, y, z } = point.0 else {
            panic!("number-line points must stay in the line's local frame");
        };
        assert_eq!(space, line.drawable().id);
        let context = gaanim_expr::EvalContext::new();
        assert!(x.eval(&context).unwrap().abs() < 1e-10);
        assert!((y.eval(&context).unwrap() - 42.0).abs() < 1e-10);
        assert!(z.eval(&context).unwrap().abs() < 1e-10);
    }

    #[test]
    fn number_line_default_placement_keeps_its_authored_axis_origin() {
        let mut canvas = Canvas::new(640, 360);
        let line = canvas
            .coordinate_number_line(
                Axis::linear(0.0, std::f64::consts::TAU)
                    .unwrap()
                    .ticks(std::f64::consts::PI)
                    .unwrap(),
                Some(600.0),
            )
            .unwrap();
        line.drawable().clone().at_default(-250.0, 0.0);

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
    fn number_line_function_uses_one_native_reactive_path() {
        let mut canvas = Canvas::new(640, 360);
        let amplitude = canvas.parameter(1.0).unwrap();
        let line = canvas
            .coordinate_number_line(Axis::linear(0.0, 6.0).unwrap(), Some(480.0))
            .unwrap();
        canvas
            .number_line_expression_plot(
                &line,
                amplitude.expression() * Expr::variable("x").sin(),
                "x",
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
        let mut canvas = Canvas::new(640, 360);
        let space = canvas
            .coordinate_axes(
                Axis::linear(-4.0, 4.0).unwrap(),
                Axis::linear(-3.0, 3.0).unwrap(),
                Some(520.0),
                Some(280.0),
                true,
            )
            .unwrap()
            .at(37.0, -18.0);

        let coordinate = space.coord(1.0, 2.0).unwrap();
        let animations = space.animate_view((-2.0, 2.0), (-1.5, 1.5), 1.2).unwrap();

        assert_ne!(space.root.id, space.view.id);
        assert_eq!(coordinate.space, space.view.id);
        assert_eq!(animations.len(), 2);
        assert!(
            animations
                .iter()
                .all(|animation| animation.inner.target == space.view.id)
        );
    }

    #[test]
    fn animated_view_rejects_non_affine_scales() {
        let mut canvas = Canvas::new(640, 360);
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
            space.animate_view((0.2, 5.0), (-1.0, 1.0), 1.0),
            Err(VisualizationError::UnsupportedAnimatedView)
        ));
    }

    #[test]
    fn parameter_drives_native_expression_and_animation() {
        let mut canvas = Canvas::new(640, 360);
        let parameter = canvas.parameter(1.5).unwrap();
        parameter.set(2.25).unwrap();

        let context = gaanim_expr::EvalContext::new()
            .with_parameter(parameter.drawable().id, parameter.current());
        assert_eq!(parameter.expression().eval(&context).unwrap(), 2.25);

        let animation = parameter.animate_to(4.0).unwrap();
        assert_eq!(animation.inner.target, parameter.drawable().id);
        assert_eq!(parameter.current(), 4.0);
        assert!(matches!(
            parameter.set(f64::NAN),
            Err(VisualizationError::InvalidParameter)
        ));
    }

    #[test]
    fn three_dimensional_lines_honor_stroke_color() {
        let mut canvas = Canvas::new(640, 360);
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
        let mut canvas = Canvas::new(640, 360);
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
        let mut canvas = Canvas::new(640, 360);
        assert!(!canvas.has_native_3d_content());

        canvas.polyline_3d(vec![[0.0, 0.0, 0.0], [1.0, 1.0, 1.0]]);

        assert!(canvas.has_native_3d_content());
    }

    #[test]
    fn surfaces_include_a_batched_colored_wireframe_fallback() {
        let mut canvas = Canvas::new(640, 360);
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
        let mut canvas = Canvas::new(1920, 1080);
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
        let mut canvas = Canvas::new(1280, 720);
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

        let mut point_canvas = Canvas::new(640, 360);
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

        let mut line_canvas = Canvas::new(640, 360);
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

        let mut canvas = Canvas::new(640, 360);
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

        let mut canvas = Canvas::new(640, 360);
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

        let mut outside_canvas = Canvas::new(640, 360);
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

        let mut inside_canvas = Canvas::new(640, 360);
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

        let mut canvas = Canvas::new(640, 360);
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
        let mut canvas = Canvas::new(640, 360);
        let space = canvas
            .coordinate_axes(
                Axis::linear(-2.0, 2.0).unwrap(),
                Axis::linear(-1.0, 1.0).unwrap(),
                Some(400.0),
                Some(200.0),
                true,
            )
            .unwrap();

        let animation = space.write(Some(1.5));
        let crate::anim::AnimationType::Write { config } = animation.inner.anim_type else {
            panic!("coordinate-space write should remain a Write animation");
        };
        assert_eq!(config.lag_ratio, Some(0.0));
        assert_eq!(animation.inner.duration, 1.5);
    }
}
