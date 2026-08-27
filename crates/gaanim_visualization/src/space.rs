use crate::{Axis, AxisError, AxisLabelPosition};
use gaanim_core::kurbo::{BezPath, Point};
use gaanim_core::peniko::Color;
use gaanim_math::Bounds3D;

fn axis_title_coordinate(position: AxisLabelPosition, extent: f64) -> f64 {
    match position {
        AxisLabelPosition::Start => -extent * 0.5,
        AxisLabelPosition::Center => 0.0,
        AxisLabelPosition::End => extent * 0.5,
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlotFrame {
    pub width: f64,
    pub height: f64,
}

impl PlotFrame {
    pub fn new(width: f64, height: f64) -> Result<Self, AxisError> {
        if !width.is_finite() || !height.is_finite() || width <= 0.0 || height <= 0.0 {
            return Err(AxisError::InvalidDomain);
        }
        Ok(Self { width, height })
    }

    pub fn bounds(self) -> Bounds3D {
        Bounds3D::new_2d(
            -self.width * 0.5,
            -self.height * 0.5,
            self.width * 0.5,
            self.height * 0.5,
        )
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct CoordinateMap2D {
    pub x: Axis,
    pub y: Axis,
    pub frame: PlotFrame,
}

impl CoordinateMap2D {
    pub fn new(x: Axis, y: Axis, frame: PlotFrame) -> Self {
        Self { x, y, frame }
    }

    pub fn data_to_local(&self, x: f64, y: f64) -> Result<Point, AxisError> {
        let nx = self.x.normalize(x)?;
        let ny = self.y.normalize(y)?;
        Ok(Point::new(
            (nx - 0.5) * self.frame.width,
            (ny - 0.5) * self.frame.height,
        ))
    }

    pub fn local_to_data(&self, point: Point) -> Result<(f64, f64), AxisError> {
        Ok((
            self.x.denormalize(point.x / self.frame.width + 0.5)?,
            self.y.denormalize(point.y / self.frame.height + 0.5)?,
        ))
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct CoordinateMap3D {
    pub x: Axis,
    pub y: Axis,
    pub z: Axis,
    pub size: [f64; 3],
}

impl CoordinateMap3D {
    pub fn new(x: Axis, y: Axis, z: Axis, size: [f64; 3]) -> Result<Self, AxisError> {
        if size.iter().any(|value| !value.is_finite() || *value <= 0.0) {
            return Err(AxisError::InvalidDomain);
        }
        Ok(Self { x, y, z, size })
    }

    pub fn data_to_local(&self, point: [f64; 3]) -> Result<[f64; 3], AxisError> {
        Ok([
            (self.x.normalize(point[0])? - 0.5) * self.size[0],
            (self.y.normalize(point[1])? - 0.5) * self.size[1],
            (self.z.normalize(point[2])? - 0.5) * self.size[2],
        ])
    }

    pub fn local_to_data(&self, point: [f64; 3]) -> Result<[f64; 3], AxisError> {
        Ok([
            self.x.denormalize(point[0] / self.size[0] + 0.5)?,
            self.y.denormalize(point[1] / self.size[1] + 0.5)?,
            self.z.denormalize(point[2] / self.size[2] + 0.5)?,
        ])
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SpaceLayer {
    MajorGrid,
    MinorGrid,
    Axes,
    Ticks,
    Numbers,
    Labels,
}

/// Resolved visibility of every semantic component in a 2D Cartesian space.
///
/// The Python and facade layers resolve global switches and per-axis overrides
/// before constructing this value, so geometry generation only needs concrete
/// booleans.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CartesianVisibility {
    pub x_axis: bool,
    pub y_axis: bool,
    pub x_grid: bool,
    pub y_grid: bool,
    pub x_ticks: bool,
    pub y_ticks: bool,
    pub x_numbers: bool,
    pub y_numbers: bool,
    pub x_labels: bool,
    pub y_labels: bool,
}

impl Default for CartesianVisibility {
    fn default() -> Self {
        Self {
            x_axis: true,
            y_axis: true,
            x_grid: true,
            y_grid: true,
            x_ticks: true,
            y_ticks: true,
            x_numbers: true,
            y_numbers: true,
            x_labels: true,
            y_labels: true,
        }
    }
}

/// Resolved visibility of a typed 3D Cartesian space.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cartesian3DVisibility {
    pub x_axis: bool,
    pub y_axis: bool,
    pub z_axis: bool,
    pub xy_grid: bool,
    pub xz_grid: bool,
    pub yz_grid: bool,
    pub x_ticks: bool,
    pub y_ticks: bool,
    pub z_ticks: bool,
    pub x_numbers: bool,
    pub y_numbers: bool,
    pub z_numbers: bool,
    pub x_labels: bool,
    pub y_labels: bool,
    pub z_labels: bool,
}

impl Default for Cartesian3DVisibility {
    fn default() -> Self {
        Self {
            x_axis: true,
            y_axis: true,
            z_axis: true,
            xy_grid: true,
            xz_grid: true,
            yz_grid: true,
            x_ticks: true,
            y_ticks: true,
            z_ticks: true,
            x_numbers: true,
            y_numbers: true,
            z_numbers: true,
            x_labels: true,
            y_labels: true,
            z_labels: true,
        }
    }
}

/// Resolved visibility of the semantic parts of a polar space.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PolarVisibility {
    pub rings: bool,
    pub spokes: bool,
    pub axes: bool,
    pub numbers: bool,
    pub labels: bool,
}

/// Resolved visibility of a typed number line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NumberLineVisibility {
    pub axis: bool,
    pub ticks: bool,
    pub numbers: bool,
    pub labels: bool,
}

impl Default for NumberLineVisibility {
    fn default() -> Self {
        Self {
            axis: true,
            ticks: true,
            numbers: true,
            labels: true,
        }
    }
}

impl Default for PolarVisibility {
    fn default() -> Self {
        Self {
            rings: true,
            spokes: true,
            axes: true,
            numbers: true,
            labels: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct LabelGeometry {
    pub text: String,
    pub position: Point,
    pub rotation: f64,
    pub color: Color,
}

#[derive(Debug, Clone)]
pub struct SpaceGeometry2D {
    pub major_grid: BezPath,
    pub minor_grid: BezPath,
    pub axes: BezPath,
    pub ticks: BezPath,
    pub numbers: Vec<LabelGeometry>,
    pub labels: Vec<LabelGeometry>,
    pub bounds: Bounds3D,
}

impl SpaceGeometry2D {
    pub fn layer_path(&self, layer: SpaceLayer) -> Option<&BezPath> {
        match layer {
            SpaceLayer::MajorGrid => Some(&self.major_grid),
            SpaceLayer::MinorGrid => Some(&self.minor_grid),
            SpaceLayer::Axes => Some(&self.axes),
            SpaceLayer::Ticks => Some(&self.ticks),
            SpaceLayer::Numbers | SpaceLayer::Labels => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct CartesianSpace {
    pub map: CoordinateMap2D,
    pub visibility: CartesianVisibility,
    pub grid_color: Color,
    pub minor_grid_color: Color,
}

impl CartesianSpace {
    pub fn axes(x: Axis, y: Axis, frame: PlotFrame) -> Self {
        Self {
            map: CoordinateMap2D::new(x, y, frame),
            visibility: CartesianVisibility {
                x_grid: false,
                y_grid: false,
                ..Default::default()
            },
            grid_color: Color::from_rgb8(0xC0, 0xC0, 0xC0),
            minor_grid_color: Color::from_rgb8(0xE0, 0xE0, 0xE0),
        }
    }

    pub fn number_plane(x: Axis, y: Axis, frame: PlotFrame) -> Self {
        Self::axes(x, y, frame).grid(true)
    }

    pub fn grid(mut self, enabled: bool) -> Self {
        self.visibility.x_grid = enabled;
        self.visibility.y_grid = enabled;
        self
    }

    pub fn numbers(mut self, enabled: bool) -> Self {
        self.visibility.x_numbers = enabled;
        self.visibility.y_numbers = enabled;
        self
    }

    pub fn labels(mut self, enabled: bool) -> Self {
        self.visibility.x_labels = enabled;
        self.visibility.y_labels = enabled;
        self
    }

    pub fn axes_visible(mut self, enabled: bool) -> Self {
        self.visibility.x_axis = enabled;
        self.visibility.y_axis = enabled;
        self
    }

    pub fn ticks(mut self, enabled: bool) -> Self {
        self.visibility.x_ticks = enabled;
        self.visibility.y_ticks = enabled;
        self
    }

    pub fn with_visibility(mut self, visibility: CartesianVisibility) -> Self {
        self.visibility = visibility;
        self
    }

    pub fn geometry(&self) -> Result<SpaceGeometry2D, AxisError> {
        let mut major_grid = BezPath::new();
        let mut minor_grid = BezPath::new();
        let mut axes = BezPath::new();
        let mut tick_path = BezPath::new();
        let mut numbers = Vec::new();
        let mut labels = Vec::new();
        let frame = self.map.frame;
        let x_ticks = self.map.x.ticks_values(7)?;
        let y_ticks = self.map.y.ticks_values(7)?;
        let x_cross = self.map.x.crossing_value();
        let y_cross = self.map.y.crossing_value();
        let x_axis_y = self.map.data_to_local(x_cross, y_cross)?.y;
        let y_axis_x = self.map.data_to_local(x_cross, y_cross)?.x;

        for tick in &x_ticks {
            let x = self.map.data_to_local(tick.value, y_cross)?.x;
            if self.visibility.x_grid {
                let target = if tick.major {
                    &mut major_grid
                } else {
                    &mut minor_grid
                };
                // Write Cartesian x-guides from the visual top edge toward
                // the bottom, matching Manim's number-plane reveal.
                target.move_to(Point::new(x, frame.height * 0.5));
                target.line_to(Point::new(x, -frame.height * 0.5));
            }
            let style = self.map.x.style_value();
            if self.visibility.x_ticks {
                let half = style.tick_length * if tick.major { 0.5 } else { 0.3 };
                tick_path.move_to(Point::new(x, x_axis_y - half));
                tick_path.line_to(Point::new(x, x_axis_y + half));
            }
            if self.visibility.x_numbers && tick.major && !tick.label.is_empty() {
                numbers.push(LabelGeometry {
                    text: tick.label.clone(),
                    position: Point::new(x, x_axis_y - style.tick_length - 12.0),
                    rotation: 0.0,
                    color: style.number_color,
                });
            }
        }

        for tick in &y_ticks {
            let y = self.map.data_to_local(x_cross, tick.value)?.y;
            if self.visibility.y_grid {
                let target = if tick.major {
                    &mut major_grid
                } else {
                    &mut minor_grid
                };
                target.move_to(Point::new(-frame.width * 0.5, y));
                target.line_to(Point::new(frame.width * 0.5, y));
            }
            let style = self.map.y.style_value();
            if self.visibility.y_ticks {
                let half = style.tick_length * if tick.major { 0.5 } else { 0.3 };
                tick_path.move_to(Point::new(y_axis_x - half, y));
                tick_path.line_to(Point::new(y_axis_x + half, y));
            }
            if self.visibility.y_numbers
                && tick.major
                && !tick.label.is_empty()
                && tick.value != x_cross
            {
                numbers.push(LabelGeometry {
                    text: tick.label.clone(),
                    position: Point::new(y_axis_x - style.tick_length - 12.0, y),
                    rotation: 0.0,
                    color: style.number_color,
                });
            }
        }

        if self.visibility.x_axis {
            axes.move_to(Point::new(-frame.width * 0.5, x_axis_y));
            axes.line_to(Point::new(frame.width * 0.5, x_axis_y));
        }
        if self.visibility.y_axis {
            axes.move_to(Point::new(y_axis_x, -frame.height * 0.5));
            axes.line_to(Point::new(y_axis_x, frame.height * 0.5));
        }

        if self.visibility.x_labels
            && let Some(label) = self.map.x.label_text()
        {
            let position = self.map.x.label_position_value();
            labels.push(LabelGeometry {
                text: label.to_owned(),
                position: if position == AxisLabelPosition::Center {
                    Point::new(0.0, x_axis_y - self.map.x.style_value().tick_length - 12.0)
                } else {
                    Point::new(axis_title_coordinate(position, frame.width), x_axis_y)
                },
                rotation: 0.0,
                color: self.map.x.style_value().label_color,
            });
        }
        if self.visibility.y_labels
            && let Some(label) = self.map.y.label_text()
        {
            let position = self.map.y.label_position_value();
            labels.push(LabelGeometry {
                text: label.to_owned(),
                position: if position == AxisLabelPosition::Center {
                    Point::new(y_axis_x - self.map.y.style_value().tick_length - 12.0, 0.0)
                } else {
                    Point::new(y_axis_x, axis_title_coordinate(position, frame.height))
                },
                rotation: if position == AxisLabelPosition::Center {
                    std::f64::consts::FRAC_PI_2
                } else {
                    0.0
                },
                color: self.map.y.style_value().label_color,
            });
        }

        Ok(SpaceGeometry2D {
            major_grid,
            minor_grid,
            axes,
            ticks: tick_path,
            numbers,
            labels,
            bounds: frame.bounds(),
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct NumberLine {
    pub axis: Axis,
    pub length: f64,
}

impl NumberLine {
    pub fn new(axis: Axis, length: f64) -> Result<Self, AxisError> {
        if !length.is_finite() || length <= 0.0 {
            return Err(AxisError::InvalidDomain);
        }
        Ok(Self { axis, length })
    }

    pub fn data_to_local(&self, value: f64) -> Result<f64, AxisError> {
        Ok((self.axis.normalize(value)? - 0.5) * self.length)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct PolarSpace {
    pub radial: Axis,
    pub max_radius: f64,
}

impl PolarSpace {
    pub fn new(radial: Axis, max_radius: f64) -> Result<Self, AxisError> {
        if !max_radius.is_finite() || max_radius <= 0.0 {
            return Err(AxisError::InvalidDomain);
        }
        Ok(Self { radial, max_radius })
    }

    pub fn data_to_local(&self, radius: f64, angle: f64) -> Result<Point, AxisError> {
        let radius = self.radial.normalize(radius)? * self.max_radius;
        Ok(Point::new(radius * angle.cos(), radius * angle.sin()))
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ComplexSpace {
    pub map: CoordinateMap2D,
}

impl ComplexSpace {
    pub fn data_to_local(&self, value: (f64, f64)) -> Result<Point, AxisError> {
        self.map.data_to_local(value.0, value.1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn coordinate_map_round_trips_without_canvas_constants() {
        let map = CoordinateMap2D::new(
            Axis::linear(-2.0, 6.0).unwrap(),
            Axis::log(0.1, 100.0, 10.0).unwrap(),
            PlotFrame::new(912.0, 513.0).unwrap(),
        );
        let local = map.data_to_local(1.5, 7.0).unwrap();
        let data = map.local_to_data(local).unwrap();
        assert!((data.0 - 1.5).abs() < 1e-10);
        assert!((data.1 - 7.0).abs() < 1e-10);
    }

    #[test]
    fn cartesian_geometry_has_separate_layers() {
        let x = Axis::linear(-2.0, 2.0).unwrap().ticks(1.0).unwrap();
        let y = Axis::linear(-1.0, 1.0).unwrap().ticks(0.5).unwrap();
        let geometry = CartesianSpace::number_plane(x, y, PlotFrame::new(400.0, 200.0).unwrap())
            .geometry()
            .unwrap();
        assert!(!geometry.axes.is_empty());
        assert!(!geometry.major_grid.is_empty());
        assert!(!geometry.ticks.is_empty());
        assert!(!geometry.numbers.is_empty());
    }

    #[test]
    fn cartesian_grid_paths_use_axis_specific_write_directions() {
        use gaanim_core::kurbo::PathEl;

        let x = Axis::linear(-1.0, 1.0).unwrap().ticks(1.0).unwrap();
        let y = Axis::linear(-1.0, 1.0).unwrap().ticks(1.0).unwrap();
        let geometry = CartesianSpace::number_plane(x, y, PlotFrame::new(200.0, 100.0).unwrap())
            .geometry()
            .unwrap();
        let elements = geometry.major_grid.elements();

        assert!(matches!(elements[0], PathEl::MoveTo(point) if point.y == 50.0));
        assert!(matches!(elements[1], PathEl::LineTo(point) if point.y == -50.0));

        let horizontal = elements
            .windows(2)
            .find(|pair| {
                matches!(pair, [PathEl::MoveTo(start), PathEl::LineTo(end)] if start.y == end.y)
            })
            .expect("major grid should contain a horizontal guide");
        assert!(
            matches!(horizontal, [PathEl::MoveTo(start), PathEl::LineTo(end)] if start.x == -100.0 && end.x == 100.0)
        );
    }

    #[test]
    fn cartesian_visibility_filters_each_axis_without_changing_the_map() {
        use gaanim_core::kurbo::PathEl;

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
            .minor_ticks(2)
            .label("y");
        let space = CartesianSpace::number_plane(x, y, PlotFrame::new(400.0, 200.0).unwrap())
            .with_visibility(CartesianVisibility {
                x_axis: false,
                y_axis: true,
                x_grid: true,
                y_grid: false,
                x_ticks: false,
                y_ticks: true,
                x_numbers: false,
                y_numbers: true,
                x_labels: false,
                y_labels: true,
            });
        let mapped = space.map.data_to_local(1.0, 0.5).unwrap();
        let geometry = space.geometry().unwrap();

        assert_eq!(mapped, Point::new(100.0, 50.0));
        assert!(geometry.major_grid.elements().chunks_exact(2).all(|segment| {
            matches!(segment, [PathEl::MoveTo(start), PathEl::LineTo(end)] if start.x == end.x)
        }));
        assert!(!geometry.minor_grid.is_empty());
        assert!(matches!(
            geometry.axes.elements(),
            [PathEl::MoveTo(start), PathEl::LineTo(end)] if start.x == end.x
        ));
        assert!(geometry.ticks.elements().chunks_exact(2).all(|segment| {
            matches!(segment, [PathEl::MoveTo(start), PathEl::LineTo(end)] if start.y == end.y)
        }));
        assert!(!geometry.numbers.is_empty());
        assert!(geometry.numbers.iter().all(|label| label.position.x < 0.0));
        assert_eq!(
            geometry
                .labels
                .iter()
                .map(|label| label.text.as_str())
                .collect::<Vec<_>>(),
            ["y"]
        );
    }

    #[test]
    fn disabled_cartesian_components_produce_empty_semantic_geometry() {
        let visibility = CartesianVisibility {
            x_axis: false,
            y_axis: false,
            x_grid: false,
            y_grid: false,
            x_ticks: false,
            y_ticks: false,
            x_numbers: false,
            y_numbers: false,
            x_labels: false,
            y_labels: false,
        };
        let geometry = CartesianSpace::number_plane(
            Axis::linear(-1.0, 1.0).unwrap().label("x"),
            Axis::linear(-1.0, 1.0).unwrap().label("y"),
            PlotFrame::new(200.0, 100.0).unwrap(),
        )
        .with_visibility(visibility)
        .geometry()
        .unwrap();

        assert!(geometry.major_grid.is_empty());
        assert!(geometry.minor_grid.is_empty());
        assert!(geometry.axes.is_empty());
        assert!(geometry.ticks.is_empty());
        assert!(geometry.numbers.is_empty());
        assert!(geometry.labels.is_empty());
    }
}
