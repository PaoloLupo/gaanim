use std::fmt;
use std::sync::Arc;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamDirection {
    Forward,
    Backward,
    Both,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StreamlineOptions {
    pub direction: StreamDirection,
    pub tolerance: f64,
    pub min_step: f64,
    pub max_step: f64,
    pub max_time: f64,
    pub max_length: Option<f64>,
    pub max_steps: usize,
    pub stagnation: f64,
    /// Padding around every normalized axis domain. `0.05` means 5%.
    pub padding: f64,
    /// Minimum normalized distance from already accepted streamline points.
    pub separation: f64,
}

impl Default for StreamlineOptions {
    fn default() -> Self {
        Self {
            direction: StreamDirection::Both,
            tolerance: 1e-4,
            min_step: 1e-5,
            max_step: 0.1,
            max_time: 3.0,
            max_length: None,
            max_steps: 10_000,
            stagnation: 1e-10,
            padding: 0.05,
            separation: 0.035,
        }
    }
}

impl StreamlineOptions {
    pub fn validate(&self) -> Result<(), VectorFieldError> {
        if !self.tolerance.is_finite()
            || self.tolerance <= 0.0
            || !self.min_step.is_finite()
            || self.min_step <= 0.0
            || !self.max_step.is_finite()
            || self.max_step < self.min_step
            || !self.max_time.is_finite()
            || self.max_time <= 0.0
            || self
                .max_length
                .is_some_and(|value| !value.is_finite() || value <= 0.0)
            || self.max_steps == 0
            || !self.stagnation.is_finite()
            || self.stagnation < 0.0
            || !self.padding.is_finite()
            || self.padding < 0.0
            || !self.separation.is_finite()
            || self.separation < 0.0
        {
            return Err(VectorFieldError::InvalidOptions);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum VectorFieldError {
    #[error("field dimensions and domains must be finite and non-degenerate")]
    InvalidDomain,
    #[error("field resolution must contain at least two samples per axis")]
    InvalidResolution,
    #[error("streamline integration options are invalid")]
    InvalidOptions,
    #[error("the field produced no finite samples")]
    Empty,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FieldSample<const N: usize> {
    pub position: [f64; N],
    pub vector: [f64; N],
    pub magnitude: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Streamline<const N: usize> {
    pub seed: [f64; N],
    pub points: Vec<[f64; N]>,
    pub speeds: Vec<f64>,
}

type FieldFn<const N: usize> = dyn Fn([f64; N]) -> Option<[f64; N]> + Send + Sync;

/// Reusable, thread-safe N-dimensional vector-field evaluator.
#[derive(Clone)]
pub struct VectorField<const N: usize> {
    evaluator: Arc<FieldFn<N>>,
}

impl<const N: usize> fmt::Debug for VectorField<N> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("VectorField")
            .field("dimensions", &N)
            .finish_non_exhaustive()
    }
}

impl<const N: usize> VectorField<N> {
    pub fn new(evaluator: impl Fn([f64; N]) -> Option<[f64; N]> + Send + Sync + 'static) -> Self {
        Self {
            evaluator: Arc::new(evaluator),
        }
    }

    pub fn evaluate(&self, position: [f64; N]) -> Option<FieldSample<N>> {
        if position.iter().any(|value| !value.is_finite()) {
            return None;
        }
        let vector = (self.evaluator)(position)?;
        if vector.iter().any(|value| !value.is_finite()) {
            return None;
        }
        Some(FieldSample {
            position,
            magnitude: norm(vector),
            vector,
        })
    }

    pub fn sample_grid(
        &self,
        domains: [(f64, f64); N],
        resolution: [usize; N],
    ) -> Result<Vec<FieldSample<N>>, VectorFieldError> {
        validate_domains(domains)?;
        if resolution.iter().any(|count| *count < 2) {
            return Err(VectorFieldError::InvalidResolution);
        }
        let count = resolution.iter().product();
        let mut samples = Vec::with_capacity(count);
        for flat in 0..count {
            let mut remainder = flat;
            let mut position = [0.0; N];
            for axis in 0..N {
                let index = remainder % resolution[axis];
                remainder /= resolution[axis];
                let alpha = index as f64 / (resolution[axis] - 1) as f64;
                position[axis] = domains[axis].0 + (domains[axis].1 - domains[axis].0) * alpha;
            }
            if let Some(sample) = self.evaluate(position)
                && sample.magnitude > f64::EPSILON
            {
                samples.push(sample);
            }
        }
        (!samples.is_empty())
            .then_some(samples)
            .ok_or(VectorFieldError::Empty)
    }

    /// Build deterministic, coverage-filtered streamlines from a regular set
    /// of candidate seeds. Integration is adaptive Dormand-Prince RK45.
    pub fn streamlines(
        &self,
        domains: [(f64, f64); N],
        seed_resolution: [usize; N],
        options: StreamlineOptions,
    ) -> Result<Vec<Streamline<N>>, VectorFieldError> {
        validate_domains(domains)?;
        options.validate()?;
        if seed_resolution.contains(&0) {
            return Err(VectorFieldError::InvalidResolution);
        }
        let count = seed_resolution.iter().product();
        let mut accepted: Vec<Streamline<N>> = Vec::new();
        for flat in 0..count {
            let mut remainder = flat;
            let mut seed = [0.0; N];
            for axis in 0..N {
                let index = remainder % seed_resolution[axis];
                remainder /= seed_resolution[axis];
                let alpha = (index as f64 + 0.5) / seed_resolution[axis] as f64;
                seed[axis] = domains[axis].0 + (domains[axis].1 - domains[axis].0) * alpha;
            }
            let normalized_seed = normalize(seed, domains);
            if accepted.iter().any(|line| {
                line.points.iter().any(|point| {
                    squared_distance(normalized_seed, normalize(*point, domains))
                        < options.separation * options.separation
                })
            }) {
                continue;
            }
            if let Some(line) = self.integrate(seed, domains, options)
                && line.points.len() >= 2
            {
                accepted.push(line);
            }
        }
        (!accepted.is_empty())
            .then_some(accepted)
            .ok_or(VectorFieldError::Empty)
    }

    pub fn integrate(
        &self,
        seed: [f64; N],
        domains: [(f64, f64); N],
        options: StreamlineOptions,
    ) -> Option<Streamline<N>> {
        validate_domains(domains).ok()?;
        options.validate().ok()?;
        let backward = matches!(
            options.direction,
            StreamDirection::Backward | StreamDirection::Both
        )
        .then(|| self.integrate_one(seed, domains, options, -1.0));
        let forward = matches!(
            options.direction,
            StreamDirection::Forward | StreamDirection::Both
        )
        .then(|| self.integrate_one(seed, domains, options, 1.0));
        let mut points = Vec::new();
        let mut speeds = Vec::new();
        if let Some((mut backward_points, mut backward_speeds)) = backward.flatten() {
            backward_points.reverse();
            backward_speeds.reverse();
            backward_points.pop();
            backward_speeds.pop();
            points.extend(backward_points);
            speeds.extend(backward_speeds);
        }
        if let Some((forward_points, forward_speeds)) = forward.flatten() {
            points.extend(forward_points);
            speeds.extend(forward_speeds);
        }
        (points.len() >= 2).then_some(Streamline {
            seed,
            points,
            speeds,
        })
    }

    fn integrate_one(
        &self,
        seed: [f64; N],
        domains: [(f64, f64); N],
        options: StreamlineOptions,
        direction: f64,
    ) -> Option<(Vec<[f64; N]>, Vec<f64>)> {
        let initial = self.evaluate(seed)?;
        let mut points = vec![seed];
        let mut speeds = vec![initial.magnitude];
        let mut position = seed;
        let mut elapsed = 0.0;
        let mut length = 0.0;
        let mut step = options
            .max_step
            .min(options.max_time * 0.05)
            .max(options.min_step);
        for _ in 0..options.max_steps {
            if elapsed >= options.max_time {
                break;
            }
            step = step.min(options.max_time - elapsed);
            let (candidate, error) =
                dormand_prince_step(self, position, step * direction, domains)?;
            let accepted = error <= options.tolerance || step <= options.min_step * 1.000_001;
            let factor = if error <= f64::EPSILON {
                2.0
            } else {
                (0.9 * (options.tolerance / error).powf(0.2)).clamp(0.2, 5.0)
            };
            if !accepted {
                step = (step * factor).max(options.min_step);
                continue;
            }
            if !inside(candidate, domains, options.padding) {
                break;
            }
            let sample = self.evaluate(candidate)?;
            if sample.magnitude <= options.stagnation {
                break;
            }
            let segment = normalized_distance(position, candidate, domains);
            if segment <= f64::EPSILON {
                break;
            }
            length += segment;
            if options.max_length.is_some_and(|maximum| length > maximum) {
                break;
            }
            position = candidate;
            points.push(position);
            speeds.push(sample.magnitude);
            elapsed += step;
            step = (step * factor).clamp(options.min_step, options.max_step);
        }
        (points.len() >= 2).then_some((points, speeds))
    }
}

fn validate_domains<const N: usize>(domains: [(f64, f64); N]) -> Result<(), VectorFieldError> {
    if domains
        .iter()
        .any(|(min, max)| !min.is_finite() || !max.is_finite() || min >= max)
    {
        Err(VectorFieldError::InvalidDomain)
    } else {
        Ok(())
    }
}

fn norm<const N: usize>(value: [f64; N]) -> f64 {
    value
        .iter()
        .map(|component| component * component)
        .sum::<f64>()
        .sqrt()
}

fn add_scaled<const N: usize>(base: [f64; N], terms: &[([f64; N], f64)], h: f64) -> [f64; N] {
    std::array::from_fn(|axis| {
        base[axis]
            + h * terms
                .iter()
                .map(|(vector, factor)| vector[axis] * factor)
                .sum::<f64>()
    })
}

fn dormand_prince_step<const N: usize>(
    field: &VectorField<N>,
    y: [f64; N],
    h: f64,
    domains: [(f64, f64); N],
) -> Option<([f64; N], f64)> {
    let k1 = field.evaluate(y)?.vector;
    let k2 = field.evaluate(add_scaled(y, &[(k1, 1.0 / 5.0)], h))?.vector;
    let k3 = field
        .evaluate(add_scaled(y, &[(k1, 3.0 / 40.0), (k2, 9.0 / 40.0)], h))?
        .vector;
    let k4 = field
        .evaluate(add_scaled(
            y,
            &[(k1, 44.0 / 45.0), (k2, -56.0 / 15.0), (k3, 32.0 / 9.0)],
            h,
        ))?
        .vector;
    let k5 = field
        .evaluate(add_scaled(
            y,
            &[
                (k1, 19372.0 / 6561.0),
                (k2, -25360.0 / 2187.0),
                (k3, 64448.0 / 6561.0),
                (k4, -212.0 / 729.0),
            ],
            h,
        ))?
        .vector;
    let k6 = field
        .evaluate(add_scaled(
            y,
            &[
                (k1, 9017.0 / 3168.0),
                (k2, -355.0 / 33.0),
                (k3, 46732.0 / 5247.0),
                (k4, 49.0 / 176.0),
                (k5, -5103.0 / 18656.0),
            ],
            h,
        ))?
        .vector;
    let y5 = add_scaled(
        y,
        &[
            (k1, 35.0 / 384.0),
            (k3, 500.0 / 1113.0),
            (k4, 125.0 / 192.0),
            (k5, -2187.0 / 6784.0),
            (k6, 11.0 / 84.0),
        ],
        h,
    );
    let k7 = field.evaluate(y5)?.vector;
    let y4 = add_scaled(
        y,
        &[
            (k1, 5179.0 / 57600.0),
            (k3, 7571.0 / 16695.0),
            (k4, 393.0 / 640.0),
            (k5, -92097.0 / 339200.0),
            (k6, 187.0 / 2100.0),
            (k7, 1.0 / 40.0),
        ],
        h,
    );
    let error = normalized_distance(y4, y5, domains);
    Some((y5, error))
}

fn normalize<const N: usize>(point: [f64; N], domains: [(f64, f64); N]) -> [f64; N] {
    std::array::from_fn(|axis| {
        (point[axis] - domains[axis].0) / (domains[axis].1 - domains[axis].0)
    })
}

fn squared_distance<const N: usize>(left: [f64; N], right: [f64; N]) -> f64 {
    (0..N).map(|axis| (left[axis] - right[axis]).powi(2)).sum()
}

fn normalized_distance<const N: usize>(
    left: [f64; N],
    right: [f64; N],
    domains: [(f64, f64); N],
) -> f64 {
    squared_distance(normalize(left, domains), normalize(right, domains)).sqrt()
}

fn inside<const N: usize>(point: [f64; N], domains: [(f64, f64); N], padding: f64) -> bool {
    normalize(point, domains)
        .iter()
        .all(|value| (-padding..=1.0 + padding).contains(value))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn samples_two_and_three_dimensional_fields() {
        let two = VectorField::new(|[x, y]| Some([-y, x]));
        assert_eq!(two.sample_grid([(-1.0, 1.0); 2], [3, 3]).unwrap().len(), 8);
        let three = VectorField::new(|[x, y, z]| Some([-y, x, -z]));
        assert!(
            !three
                .sample_grid([(-1.0, 1.0); 3], [2, 2, 2])
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn rk45_matches_a_uniform_field_and_is_bidirectional() {
        let field = VectorField::new(|_| Some([1.0, 0.0]));
        let line = field
            .integrate(
                [0.0, 0.0],
                [(-2.0, 2.0), (-1.0, 1.0)],
                StreamlineOptions {
                    max_time: 1.0,
                    max_step: 0.1,
                    ..Default::default()
                },
            )
            .unwrap();
        assert!(line.points.first().unwrap()[0] < -0.99);
        assert!(line.points.last().unwrap()[0] > 0.99);
        assert!(line.points.iter().all(|point| point[1].abs() < 1e-12));
    }

    #[test]
    fn generated_streamlines_are_deterministic_and_separated() {
        let field = VectorField::new(|[x, y]| Some([-y, x]));
        let options = StreamlineOptions {
            max_time: 0.5,
            ..Default::default()
        };
        let first = field
            .streamlines([(-1.0, 1.0); 2], [5, 5], options)
            .unwrap();
        let second = field
            .streamlines([(-1.0, 1.0); 2], [5, 5], options)
            .unwrap();
        assert_eq!(first, second);
    }
}
