use crate::{AxisError, CoordinateMap2D, CoordinateMap3D};
use gaanim_core::kurbo::{BezPath, Point};
use gaanim_expr::{EvalContext, Expr};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Sampling {
    Fixed {
        samples: usize,
    },
    Adaptive {
        min_samples: usize,
        max_depth: usize,
        tolerance: f64,
    },
}

impl Default for Sampling {
    fn default() -> Self {
        Self::Adaptive {
            min_samples: 32,
            max_depth: 8,
            tolerance: 0.75,
        }
    }
}

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum SamplingError {
    #[error("sample count must be at least two")]
    TooFewSamples,
    #[error("sampling domain must be finite with minimum < maximum")]
    InvalidDomain,
    #[error("sampling tolerance must be finite and positive")]
    InvalidTolerance,
    #[error(transparent)]
    Axis(#[from] AxisError),
}

#[derive(Debug, Clone, Default)]
pub struct SampledPath {
    pub segments: Vec<Vec<Point>>,
}

impl SampledPath {
    pub fn to_bez_path(&self) -> BezPath {
        let mut path = BezPath::new();
        for segment in &self.segments {
            if let Some(first) = segment.first() {
                path.move_to(*first);
                for point in &segment[1..] {
                    path.line_to(*point);
                }
            }
        }
        path
    }

    pub fn point_count(&self) -> usize {
        self.segments.iter().map(Vec::len).sum()
    }
}

fn validate_sampling(sampling: Sampling) -> Result<(), SamplingError> {
    match sampling {
        Sampling::Fixed { samples } if samples < 2 => Err(SamplingError::TooFewSamples),
        Sampling::Adaptive {
            min_samples,
            tolerance: _,
            ..
        } if min_samples < 2 => Err(SamplingError::TooFewSamples),
        Sampling::Adaptive { tolerance, .. } if !tolerance.is_finite() || tolerance <= 0.0 => {
            Err(SamplingError::InvalidTolerance)
        }
        _ => Ok(()),
    }
}

fn valid_domain(domain: (f64, f64)) -> Result<(), SamplingError> {
    if !domain.0.is_finite() || !domain.1.is_finite() || domain.0 >= domain.1 {
        Err(SamplingError::InvalidDomain)
    } else {
        Ok(())
    }
}

fn map_sample(
    map: &CoordinateMap2D,
    x: f64,
    evaluator: &mut impl FnMut(f64) -> Option<f64>,
) -> Option<Point> {
    let y = evaluator(x)?;
    if !y.is_finite() {
        return None;
    }
    map.data_to_local(x, y).ok()
}

fn should_break(previous: Point, next: Point, map: &CoordinateMap2D) -> bool {
    let jump = (next.y - previous.y).abs();
    jump > map.frame.height * 1.5 || !next.x.is_finite() || !next.y.is_finite()
}

fn push_sample(
    output: &mut SampledPath,
    current: &mut Vec<Point>,
    point: Option<Point>,
    map: &CoordinateMap2D,
) {
    match point {
        Some(point)
            if current
                .last()
                .is_none_or(|previous| !should_break(*previous, point, map)) =>
        {
            current.push(point)
        }
        Some(point) => {
            if current.len() >= 2 {
                output.segments.push(std::mem::take(current));
            } else {
                current.clear();
            }
            current.push(point);
        }
        None => {
            if current.len() >= 2 {
                output.segments.push(std::mem::take(current));
            } else {
                current.clear();
            }
        }
    }
}

pub fn sample_function(
    map: &CoordinateMap2D,
    domain: (f64, f64),
    sampling: Sampling,
    mut evaluator: impl FnMut(f64) -> Option<f64>,
) -> Result<SampledPath, SamplingError> {
    valid_domain(domain)?;
    validate_sampling(sampling)?;
    let mut output = SampledPath::default();
    let mut current = Vec::new();
    match sampling {
        Sampling::Fixed { samples } => {
            for index in 0..samples {
                let t = index as f64 / (samples - 1) as f64;
                let x = domain.0 + (domain.1 - domain.0) * t;
                let point = map_sample(map, x, &mut evaluator);
                push_sample(&mut output, &mut current, point, map);
            }
        }
        Sampling::Adaptive {
            min_samples,
            max_depth,
            tolerance,
        } => {
            #[allow(clippy::too_many_arguments)]
            fn refine(
                map: &CoordinateMap2D,
                evaluator: &mut impl FnMut(f64) -> Option<f64>,
                left_x: f64,
                left: Option<Point>,
                right_x: f64,
                right: Option<Point>,
                depth: usize,
                tolerance: f64,
                points: &mut Vec<(f64, Option<Point>)>,
            ) {
                if depth == 0 || left.is_none() || right.is_none() {
                    points.push((right_x, right));
                    return;
                }
                let mid_x = (left_x + right_x) * 0.5;
                let mid = map_sample(map, mid_x, evaluator);
                let should_refine = match (left, mid, right) {
                    (Some(left), Some(mid), Some(right)) => {
                        let chord_mid =
                            Point::new((left.x + right.x) * 0.5, (left.y + right.y) * 0.5);
                        let error =
                            ((mid.x - chord_mid.x).powi(2) + (mid.y - chord_mid.y).powi(2)).sqrt();
                        error > tolerance
                            || should_break(left, mid, map)
                            || should_break(mid, right, map)
                    }
                    _ => true,
                };
                if should_refine {
                    refine(
                        map,
                        evaluator,
                        left_x,
                        left,
                        mid_x,
                        mid,
                        depth - 1,
                        tolerance,
                        points,
                    );
                    refine(
                        map,
                        evaluator,
                        mid_x,
                        mid,
                        right_x,
                        right,
                        depth - 1,
                        tolerance,
                        points,
                    );
                } else {
                    points.push((right_x, right));
                }
            }

            let mut points = Vec::new();
            let first = map_sample(map, domain.0, &mut evaluator);
            points.push((domain.0, first));
            for index in 0..(min_samples - 1) {
                let left_t = index as f64 / (min_samples - 1) as f64;
                let right_t = (index + 1) as f64 / (min_samples - 1) as f64;
                let left_x = domain.0 + (domain.1 - domain.0) * left_t;
                let right_x = domain.0 + (domain.1 - domain.0) * right_t;
                let left = points.last().and_then(|(_, point)| *point);
                let right = map_sample(map, right_x, &mut evaluator);
                refine(
                    map,
                    &mut evaluator,
                    left_x,
                    left,
                    right_x,
                    right,
                    max_depth,
                    tolerance,
                    &mut points,
                );
            }
            for (_, point) in points {
                push_sample(&mut output, &mut current, point, map);
            }
        }
    }
    if current.len() >= 2 {
        output.segments.push(current);
    }
    Ok(output)
}

pub fn sample_expression(
    map: &CoordinateMap2D,
    expression: &Expr,
    variable: &str,
    domain: (f64, f64),
    sampling: Sampling,
    context: &EvalContext,
) -> Result<SampledPath, SamplingError> {
    let mut local_context = context.clone();
    sample_function(map, domain, sampling, |value| {
        local_context.set_variable(variable, value);
        expression.eval(&local_context).ok()
    })
}

pub fn sample_parametric(
    map: &CoordinateMap2D,
    domain: (f64, f64),
    sampling: Sampling,
    mut evaluator: impl FnMut(f64) -> Option<(f64, f64)>,
) -> Result<SampledPath, SamplingError> {
    valid_domain(domain)?;
    validate_sampling(sampling)?;
    let samples = match sampling {
        Sampling::Fixed { samples } => samples,
        Sampling::Adaptive {
            min_samples,
            max_depth,
            ..
        } => min_samples.saturating_mul(1usize << max_depth.min(5)),
    }
    .max(2);
    let mut output = SampledPath::default();
    let mut current = Vec::new();
    for index in 0..samples {
        let progress = index as f64 / (samples - 1) as f64;
        let parameter = domain.0 + (domain.1 - domain.0) * progress;
        let point = evaluator(parameter)
            .filter(|(x, y)| x.is_finite() && y.is_finite())
            .and_then(|(x, y)| map.data_to_local(x, y).ok());
        push_sample(&mut output, &mut current, point, map);
    }
    if current.len() >= 2 {
        output.segments.push(current);
    }
    Ok(output)
}

/// Extract implicit contours with marching squares. `resolution` is the number
/// of cells per axis, not the number of sample points.
pub fn implicit_contours(
    map: &CoordinateMap2D,
    resolution: [usize; 2],
    mut evaluator: impl FnMut(f64, f64) -> Option<f64>,
) -> Result<SampledPath, SamplingError> {
    if resolution[0] == 0 || resolution[1] == 0 {
        return Err(SamplingError::TooFewSamples);
    }
    let (x_min, x_max) = map.x.domain();
    let (y_min, y_max) = map.y.domain();
    let mut output = SampledPath::default();
    let interpolate = |a: (f64, f64, f64), b: (f64, f64, f64)| {
        let denominator = a.2 - b.2;
        let t = if denominator.abs() <= f64::EPSILON {
            0.5
        } else {
            a.2 / denominator
        };
        (a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t)
    };
    for ix in 0..resolution[0] {
        for iy in 0..resolution[1] {
            let x0 = x_min + (x_max - x_min) * ix as f64 / resolution[0] as f64;
            let x1 = x_min + (x_max - x_min) * (ix + 1) as f64 / resolution[0] as f64;
            let y0 = y_min + (y_max - y_min) * iy as f64 / resolution[1] as f64;
            let y1 = y_min + (y_max - y_min) * (iy + 1) as f64 / resolution[1] as f64;
            let Some(values) = [
                evaluator(x0, y0).map(|v| (x0, y0, v)),
                evaluator(x1, y0).map(|v| (x1, y0, v)),
                evaluator(x1, y1).map(|v| (x1, y1, v)),
                evaluator(x0, y1).map(|v| (x0, y1, v)),
            ]
            .into_iter()
            .collect::<Option<Vec<_>>>() else {
                continue;
            };
            let edges = [(0, 1), (1, 2), (2, 3), (3, 0)];
            let intersections: Vec<(f64, f64)> = edges
                .into_iter()
                .filter(|(a, b)| (values[*a].2 <= 0.0) != (values[*b].2 <= 0.0))
                .map(|(a, b)| interpolate(values[a], values[b]))
                .collect();
            for pair in intersections.chunks_exact(2) {
                if let (Ok(start), Ok(end)) = (
                    map.data_to_local(pair[0].0, pair[0].1),
                    map.data_to_local(pair[1].0, pair[1].1),
                ) {
                    output.segments.push(vec![start, end]);
                }
            }
        }
    }
    Ok(output)
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VectorGlyph {
    pub start: Point,
    pub end: Point,
    pub magnitude: f64,
}

pub fn sample_vector_field(
    map: &CoordinateMap2D,
    resolution: [usize; 2],
    max_length: f64,
    mut evaluator: impl FnMut(f64, f64) -> Option<(f64, f64)>,
) -> Result<Vec<VectorGlyph>, SamplingError> {
    if resolution.iter().any(|value| *value < 2) || !max_length.is_finite() || max_length <= 0.0 {
        return Err(SamplingError::TooFewSamples);
    }
    let (x_min, x_max) = map.x.domain();
    let (y_min, y_max) = map.y.domain();
    let mut glyphs = Vec::new();
    for ix in 0..resolution[0] {
        for iy in 0..resolution[1] {
            let x = x_min + (x_max - x_min) * ix as f64 / (resolution[0] - 1) as f64;
            let y = y_min + (y_max - y_min) * iy as f64 / (resolution[1] - 1) as f64;
            let Some((vx, vy)) = evaluator(x, y) else {
                continue;
            };
            let magnitude = vx.hypot(vy);
            if !magnitude.is_finite() || magnitude <= f64::EPSILON {
                continue;
            }
            let start = map.data_to_local(x, y)?;
            let length = magnitude.min(max_length);
            let nx = vx / magnitude * length;
            let ny = vy / magnitude * length;
            // Interpret max_length in local units so glyphs remain legible on
            // nonlinear or asymmetric coordinate spaces.
            let end = Point::new(start.x + nx, start.y + ny);
            glyphs.push(VectorGlyph {
                start,
                end,
                magnitude,
            });
        }
    }
    Ok(glyphs)
}

#[derive(Debug, Clone, Default)]
pub struct SurfaceMesh {
    pub vertices: Vec<[f32; 3]>,
    pub indices: Vec<u32>,
    pub values: Vec<f64>,
}

pub fn sample_surface(
    map: &CoordinateMap3D,
    resolution: [usize; 2],
    mut evaluator: impl FnMut(f64, f64) -> Option<f64>,
) -> Result<SurfaceMesh, SamplingError> {
    if resolution.iter().any(|value| *value < 2) {
        return Err(SamplingError::TooFewSamples);
    }
    let (x_min, x_max) = map.x.domain();
    let (y_min, y_max) = map.y.domain();
    let mut mesh = SurfaceMesh::default();
    for iy in 0..resolution[1] {
        for ix in 0..resolution[0] {
            let x = x_min + (x_max - x_min) * ix as f64 / (resolution[0] - 1) as f64;
            let y = y_min + (y_max - y_min) * iy as f64 / (resolution[1] - 1) as f64;
            let z = evaluator(x, y)
                .filter(|value| value.is_finite())
                .unwrap_or(f64::NAN);
            let local = if z.is_finite() {
                map.data_to_local([x, y, z]).ok()
            } else {
                None
            };
            mesh.vertices.push(
                local
                    .map(|point| [point[0] as f32, point[1] as f32, point[2] as f32])
                    .unwrap_or([f32::NAN; 3]),
            );
            mesh.values.push(z);
        }
    }
    for iy in 0..resolution[1] - 1 {
        for ix in 0..resolution[0] - 1 {
            let a = (iy * resolution[0] + ix) as u32;
            let b = a + 1;
            let c = a + resolution[0] as u32;
            let d = c + 1;
            if [a, b, c, d]
                .iter()
                .all(|index| mesh.vertices[*index as usize][0].is_finite())
            {
                mesh.indices.extend_from_slice(&[a, b, d, a, d, c]);
            }
        }
    }
    Ok(mesh)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Axis, PlotFrame};

    fn map() -> CoordinateMap2D {
        CoordinateMap2D::new(
            Axis::linear(-2.0, 2.0).unwrap(),
            Axis::linear(-2.0, 2.0).unwrap(),
            PlotFrame::new(400.0, 240.0).unwrap(),
        )
    }

    #[test]
    fn adaptive_sampling_adds_points_on_curvature() {
        let sampled = sample_function(
            &map(),
            (-2.0, 2.0),
            Sampling::Adaptive {
                min_samples: 8,
                max_depth: 8,
                tolerance: 0.05,
            },
            |x| Some((6.0 * x).sin()),
        )
        .unwrap();
        assert!(sampled.point_count() > 32);
    }

    #[test]
    fn non_finite_samples_split_paths() {
        let sampled = sample_function(&map(), (-2.0, 2.0), Sampling::Fixed { samples: 101 }, |x| {
            (x.abs() > 0.05).then_some(1.0 / x)
        })
        .unwrap();
        assert!(sampled.segments.len() >= 2);
    }

    #[test]
    fn implicit_circle_produces_segments() {
        let contour =
            implicit_contours(&map(), [32, 32], |x, y| Some(x * x + y * y - 1.0)).unwrap();
        assert!(contour.point_count() > 20);
    }
}
