use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use gaanim_core::ObjectId;
use gaanim_core::glam::DVec3;
use gaanim_core::kurbo::{BezPath, Circle, Point, Shape};
use gaanim_core::peniko::Color;
use gaanim_expr::Expr;
use gaanim_objects::prelude::SvgPath;
use gaanim_visualization::{
    Axis, CartesianSpace, CoordinateMap2D, CoordinateMap3D, NonFinitePolicy, NumberLine, PlotFrame,
    PolarSpace, Sampling, Scale, SpaceLayer, area_path, bars, box_stats, error_bar_path, histogram,
    implicit_contours, line_path, sample_function, sample_parametric, sample_surface,
    sample_vector_field, scatter_points, step_path, violin_path,
};

use super::ops::Op;
use super::{Canvas, DrawableHandle, SpawnKind};

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum VisualizationError {
    #[error(transparent)]
    Axis(#[from] gaanim_visualization::AxisError),
    #[error(transparent)]
    Sampling(#[from] gaanim_visualization::SamplingError),
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

    /// Build a typed 3D Cartesian coordinate space.
    pub fn coordinate_axes_3d(
        &mut self,
        x: Axis,
        y: Axis,
        z: Axis,
        size: [f64; 3],
        grid: bool,
    ) -> Result<CoordinateSpace3DHandle, VisualizationError> {
        for axis in [&x, &y, &z] {
            if !matches!(axis.scale(), Scale::Linear | Scale::Time) {
                return Err(VisualizationError::Unsupported3DScale);
            }
        }
        let map = CoordinateMap3D::new(x.clone(), y.clone(), z.clone(), size)?;
        let tick_step = |axis: &Axis| -> Result<f64, VisualizationError> {
            let major: Vec<f64> = axis
                .ticks_values(7)?
                .into_iter()
                .filter(|tick| tick.major)
                .map(|tick| tick.value)
                .collect();
            Ok(major
                .windows(2)
                .next()
                .map(|pair| (pair[1] - pair[0]).abs())
                .filter(|step| *step > 0.0)
                .unwrap_or_else(|| {
                    let domain = axis.domain();
                    (domain.1 - domain.0) / 5.0
                }))
        };
        let xd = x.domain();
        let yd = y.domain();
        let zd = z.domain();
        let style = x.style_value();
        let config = super::types::Axes3DConfig {
            grid,
            xy_grid: grid,
            xz_grid: grid,
            yz_grid: grid,
            x_label: x.label_text().map(str::to_owned),
            y_label: y.label_text().map(str::to_owned),
            z_label: z.label_text().map(str::to_owned),
            axis_color: style.color,
            tick_color: style.color,
            number_color: style.number_color,
            label_color: style.label_color,
            axis_width: style.width,
            tick_width: style.tick_width,
            tick_length: style.tick_length,
            auto_fit: false,
            x_length: Some(size[0]),
            y_length: Some(size[1]),
            z_length: Some(size[2]),
            ..Default::default()
        };
        let root = self.axes_3d(
            (xd.0, xd.1, tick_step(&x)?),
            (yd.0, yd.1, tick_step(&y)?),
            (zd.0, zd.1, tick_step(&z)?),
            config,
        );
        Ok(CoordinateSpace3DHandle { root, map })
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
        mut x: Axis,
        mut y: Axis,
        width: Option<f64>,
        height: Option<f64>,
        grid: bool,
    ) -> Result<CoordinateSpaceHandle, VisualizationError> {
        let x_uses_default_style = x.style_value() == gaanim_visualization::AxisStyle::default();
        let y_uses_default_style = y.style_value() == gaanim_visualization::AxisStyle::default();
        let themed = self.theme_style.as_ref().map(|theme| {
            let mut axis_style = gaanim_visualization::AxisStyle {
                color: theme.palette.foreground,
                number_color: theme.palette.foreground,
                label_color: theme.palette.foreground,
                ..Default::default()
            };
            if let Some(stroke) = theme
                .styles
                .get("axes/axis")
                .and_then(|style| style.stroke.as_ref())
            {
                if let Ok(gaanim_core::peniko::Brush::Solid(color)) =
                    theme.resolve_paint(&stroke.paint)
                {
                    axis_style.color = color;
                }
                axis_style.width = stroke.style.width;
            }
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
            let number_style = theme
                .styles
                .get("axes/numbers")
                .and_then(|style| style.text.as_ref());
            let label_style = theme
                .styles
                .get("axes/labels")
                .and_then(|style| style.text.as_ref());
            if let Some(color) = number_style.and_then(|style| style.color) {
                axis_style.number_color = color;
            }
            if let Some(color) = label_style.and_then(|style| style.color) {
                axis_style.label_color = color;
            }
            (
                axis_style,
                grid_color,
                number_style.and_then(|style| style.size),
                label_style.and_then(|style| style.size),
            )
        });
        if let Some((style, _, _, _)) = &themed {
            if x.style_value() == gaanim_visualization::AxisStyle::default() {
                x = x.style(*style);
            }
            if y.style_value() == gaanim_visualization::AxisStyle::default() {
                y = y.style(*style);
            }
        }
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
        if let Some((_, grid_color, _, _)) = themed {
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
        let axes = self.visualization_path(
            geometry.axes,
            geometry.bounds,
            axis_color,
            space.map.x.style_value().width,
            "CoordinateAxes",
        );
        if x_uses_default_style && y_uses_default_style {
            axes.spec.lock().expect("axes spec poisoned").theme_selector = Some("axes/axis".into());
        }
        layers.insert(SpaceLayer::Axes, axes.clone());
        let ticks = self.visualization_path(
            geometry.ticks,
            geometry.bounds,
            space.map.x.style_value().color,
            space.map.x.style_value().tick_width,
            "CoordinateTicks",
        );
        if x_uses_default_style && y_uses_default_style {
            ticks
                .spec
                .lock()
                .expect("ticks spec poisoned")
                .theme_selector = Some("axes/ticks".into());
        }
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
            .unwrap_or(0.45);
        let label_scale = self
            .theme_style
            .as_ref()
            .and_then(|theme| theme.styles.get("axes/labels"))
            .and_then(|style| style.text.as_ref())
            .and_then(|style| style.size)
            .map(|size| {
                size / self.themed_text_config().roles[&gaanim_text::prelude::TextRole::Body].size
            })
            .unwrap_or(0.55);
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
        if x_uses_default_style && y_uses_default_style {
            for number in &number_handles {
                let mut spec = number.spec.lock().expect("number spec poisoned");
                spec.theme_selector = Some("axes/numbers".into());
                spec.fill_overridden = false;
            }
        }
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
        if x_uses_default_style && y_uses_default_style {
            for label in &label_handles {
                let mut spec = label.spec.lock().expect("label spec poisoned");
                spec.theme_selector = Some("axes/labels".into());
                spec.fill_overridden = false;
            }
        }
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
        let length = length.unwrap_or_else(|| self.safe_frame().width());
        let line = NumberLine::new(axis.clone(), length)?;
        let style = axis.style_value();
        let mut axis_path = BezPath::new();
        axis_path.move_to(Point::new(-length * 0.5, 0.0));
        axis_path.line_to(Point::new(length * 0.5, 0.0));
        let bounds = gaanim_math::Bounds3D::new_2d(
            -length * 0.5,
            -style.tick_length - 32.0,
            length * 0.5,
            style.tick_length + 32.0,
        );
        let axis_handle = self.visualization_path(
            axis_path,
            bounds,
            style.color,
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
                        .scaled(0.45)
                        .at(x, -style.tick_length - 14.0),
                );
            }
        }
        let ticks = self.visualization_path(
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
                .scaled(0.55)
                .at(length * 0.5 + 18.0, 0.0);
            self.group(&[&label])
        } else {
            self.group(&[])
        };
        let root = self.group(&[&axis_handle, &ticks, &numbers, &labels]);
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
        let space = PolarSpace::new(radial.clone(), radius)?;
        let style = radial.style_value();
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
                            .scaled(0.4)
                            .at(ring_radius, -12.0),
                    );
                }
            }
        }
        for index in 0..angle_divisions {
            let angle = std::f64::consts::TAU * index as f64 / angle_divisions as f64;
            grid_path.move_to(Point::ORIGIN);
            grid_path.line_to(Point::new(radius * angle.cos(), radius * angle.sin()));
        }
        let grid = self.visualization_path(
            grid_path,
            bounds,
            Color::from_rgb8(0xC0, 0xC0, 0xC0),
            1.0,
            "PolarGrid",
        );
        let mut axes_path = BezPath::new();
        axes_path.move_to(Point::new(-radius, 0.0));
        axes_path.line_to(Point::new(radius, 0.0));
        axes_path.move_to(Point::new(0.0, -radius));
        axes_path.line_to(Point::new(0.0, radius));
        let axes =
            self.visualization_path(axes_path, bounds, style.color, style.width, "PolarAxes");
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
            sampling,
        });
        self.attach_to_space(space, &handle);
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
}
