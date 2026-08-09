use crate::{Axis, AxisError, Column, CoordinateMap2D, DataError, DataTable, Scale};
use gaanim_core::kurbo::{BezPath, Circle, Point, Rect, Shape};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NonFinitePolicy {
    Gap,
    Drop,
    Error,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Histogram {
    pub edges: Vec<f64>,
    pub counts: Vec<usize>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BoxStats {
    pub minimum: f64,
    pub q1: f64,
    pub median: f64,
    pub q3: f64,
    pub maximum: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RectMark {
    pub min: Point,
    pub max: Point,
    pub value: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub enum DataMarkKind {
    Line {
        x: String,
        y: String,
        policy: NonFinitePolicy,
    },
    Step {
        x: String,
        y: String,
        policy: NonFinitePolicy,
    },
    Area {
        x: String,
        y: String,
        baseline: f64,
    },
    Scatter {
        x: String,
        y: String,
        radius: f64,
        policy: NonFinitePolicy,
    },
    Bars {
        x: String,
        y: String,
        width: f64,
        baseline: f64,
    },
    Histogram {
        column: String,
        bins: usize,
    },
    Box {
        column: String,
        center: f64,
        width: f64,
    },
    Violin {
        column: String,
        center: f64,
        bandwidth: f64,
        width: f64,
    },
    ErrorBars {
        x: String,
        y: String,
        low: String,
        high: String,
        cap_width: f64,
    },
    /// One quantized color band of a heatmap. A small fixed number of bands
    /// batches arbitrarily many cells without creating an entity per row.
    HeatmapBand {
        x: String,
        y: String,
        value: String,
        cell_width: f64,
        cell_height: f64,
        band: usize,
        bands: usize,
    },
}

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum MarkError {
    #[error(transparent)]
    Axis(#[from] AxisError),
    #[error(transparent)]
    Data(#[from] DataError),
    #[error("mark requires at least one finite value")]
    Empty,
}

fn numeric(table: &DataTable, name: &str) -> Result<Vec<Option<f64>>, MarkError> {
    Ok(table.numeric_column(name)?.to_vec())
}

fn axis_values(table: &DataTable, name: &str, axis: &Axis) -> Result<Vec<Option<f64>>, MarkError> {
    match table.column(name)? {
        Column::Numeric(values) => Ok(values.clone()),
        Column::Text(values) => match axis.scale() {
            Scale::Category { values: categories } => Ok(values
                .iter()
                .map(|value| {
                    value
                        .as_ref()
                        .and_then(|value| categories.iter().position(|category| category == value))
                        .map(|index| index as f64)
                })
                .collect()),
            _ => Err(DataError::ColumnTypeMismatch.into()),
        },
    }
}

fn finite(values: &[Option<f64>]) -> Vec<f64> {
    values
        .iter()
        .flatten()
        .copied()
        .filter(|value| value.is_finite())
        .collect()
}

/// Build one retained vector path for a table-backed mark. Keeping each mark
/// in one path makes DataSource updates cheap and avoids an ECS entity per row.
pub fn data_mark_path(
    map: &CoordinateMap2D,
    table: &DataTable,
    kind: &DataMarkKind,
) -> Result<BezPath, MarkError> {
    match kind {
        DataMarkKind::Line { x, y, policy } => Ok(line_path(
            map,
            &axis_values(table, x, &map.x)?,
            &axis_values(table, y, &map.y)?,
            *policy,
        )?),
        DataMarkKind::Step { x, y, policy } => Ok(step_path(
            map,
            &axis_values(table, x, &map.x)?,
            &axis_values(table, y, &map.y)?,
            *policy,
        )?),
        DataMarkKind::Area { x, y, baseline } => Ok(area_path(
            map,
            &axis_values(table, x, &map.x)?,
            &axis_values(table, y, &map.y)?,
            *baseline,
        )?),
        DataMarkKind::Scatter {
            x,
            y,
            radius,
            policy,
        } => {
            let points = scatter_points(
                map,
                &axis_values(table, x, &map.x)?,
                &axis_values(table, y, &map.y)?,
                *policy,
            )?;
            let mut path = BezPath::new();
            for point in points {
                path.extend(Circle::new(point, *radius).to_path(0.1));
            }
            Ok(path)
        }
        DataMarkKind::Bars {
            x,
            y,
            width,
            baseline,
        } => {
            let x = finite(&axis_values(table, x, &map.x)?);
            let y = finite(&axis_values(table, y, &map.y)?);
            if x.len() != y.len() {
                return Err(DataError::LengthMismatch.into());
            }
            let mut path = BezPath::new();
            for rect in bars(map, &x, &y, *width, *baseline)? {
                path.extend(Rect::new(rect.min.x, rect.min.y, rect.max.x, rect.max.y).to_path(0.1));
            }
            Ok(path)
        }
        DataMarkKind::Histogram { column, bins } => {
            let values = finite(&numeric(table, column)?);
            let histogram = histogram(&values, *bins).ok_or(MarkError::Empty)?;
            let centers: Vec<f64> = histogram
                .edges
                .windows(2)
                .map(|edge| (edge[0] + edge[1]) * 0.5)
                .collect();
            let counts: Vec<f64> = histogram.counts.iter().map(|count| *count as f64).collect();
            let width = histogram.edges[1] - histogram.edges[0];
            let mut path = BezPath::new();
            for rect in bars(map, &centers, &counts, width, 0.0)? {
                path.extend(Rect::new(rect.min.x, rect.min.y, rect.max.x, rect.max.y).to_path(0.1));
            }
            Ok(path)
        }
        DataMarkKind::Box {
            column,
            center,
            width,
        } => {
            let values = finite(&numeric(table, column)?);
            let stats = box_stats(&values).ok_or(MarkError::Empty)?;
            let left = map.data_to_local(center - width * 0.5, stats.q1)?;
            let right = map.data_to_local(center + width * 0.5, stats.q3)?;
            let median = map.data_to_local(*center, stats.median)?;
            let minimum = map.data_to_local(*center, stats.minimum)?;
            let maximum = map.data_to_local(*center, stats.maximum)?;
            let mut path = Rect::new(left.x, left.y, right.x, right.y).to_path(0.1);
            path.move_to(Point::new(left.x, median.y));
            path.line_to(Point::new(right.x, median.y));
            path.move_to(minimum);
            path.line_to(maximum);
            Ok(path)
        }
        DataMarkKind::Violin {
            column,
            center,
            bandwidth,
            width,
        } => {
            let values = finite(&numeric(table, column)?);
            Ok(violin_path(map, *center, &values, *bandwidth, *width, 64)?)
        }
        DataMarkKind::ErrorBars {
            x,
            y,
            low,
            high,
            cap_width,
        } => {
            let x = finite(&axis_values(table, x, &map.x)?);
            let y = finite(&axis_values(table, y, &map.y)?);
            let low = finite(&numeric(table, low)?);
            let high = finite(&numeric(table, high)?);
            if x.len() != y.len() || x.len() != low.len() || x.len() != high.len() {
                return Err(DataError::LengthMismatch.into());
            }
            Ok(error_bar_path(map, &x, &y, &low, &high, *cap_width)?)
        }
        DataMarkKind::HeatmapBand {
            x,
            y,
            value,
            cell_width,
            cell_height,
            band,
            bands,
        } => {
            if *bands == 0
                || *band >= *bands
                || !cell_width.is_finite()
                || !cell_height.is_finite()
                || *cell_width <= 0.0
                || *cell_height <= 0.0
            {
                return Err(DataError::LengthMismatch.into());
            }
            let x = axis_values(table, x, &map.x)?;
            let y = axis_values(table, y, &map.y)?;
            let value = numeric(table, value)?;
            if x.len() != y.len() || x.len() != value.len() {
                return Err(DataError::LengthMismatch.into());
            }
            let finite_values = finite(&value);
            let minimum = finite_values
                .iter()
                .copied()
                .reduce(f64::min)
                .ok_or(MarkError::Empty)?;
            let maximum = finite_values
                .iter()
                .copied()
                .reduce(f64::max)
                .ok_or(MarkError::Empty)?;
            let span = (maximum - minimum).max(f64::EPSILON);
            let mut path = BezPath::new();
            for ((x, y), value) in x.iter().zip(&y).zip(&value) {
                let Some((x, y, value)) =
                    x.zip(*y).zip(*value).map(|((x, y), value)| (x, y, value))
                else {
                    continue;
                };
                if !x.is_finite() || !y.is_finite() || !value.is_finite() {
                    continue;
                }
                let normalized = ((value - minimum) / span).clamp(0.0, 1.0);
                let value_band = (normalized * (*bands - 1) as f64).round() as usize;
                if value_band != *band {
                    continue;
                }
                let min = map.data_to_local(x - cell_width * 0.5, y - cell_height * 0.5)?;
                let max = map.data_to_local(x + cell_width * 0.5, y + cell_height * 0.5)?;
                path.extend(Rect::new(min.x, min.y, max.x, max.y).to_path(0.1));
            }
            Ok(path)
        }
    }
}

pub fn line_path(
    map: &CoordinateMap2D,
    x: &[Option<f64>],
    y: &[Option<f64>],
    policy: NonFinitePolicy,
) -> Result<BezPath, AxisError> {
    let mut path = BezPath::new();
    let mut drawing = false;
    for (x, y) in x.iter().zip(y) {
        let point = x
            .zip(*y)
            .filter(|(x, y)| x.is_finite() && y.is_finite())
            .and_then(|(x, y)| map.data_to_local(x, y).ok());
        match (point, policy) {
            (Some(point), _) if drawing => path.line_to(point),
            (Some(point), _) => {
                path.move_to(point);
                drawing = true;
            }
            (None, NonFinitePolicy::Gap) => drawing = false,
            (None, NonFinitePolicy::Drop) => {}
            (None, NonFinitePolicy::Error) => return Err(AxisError::OutOfDomain),
        }
    }
    Ok(path)
}

pub fn step_path(
    map: &CoordinateMap2D,
    x: &[Option<f64>],
    y: &[Option<f64>],
    policy: NonFinitePolicy,
) -> Result<BezPath, AxisError> {
    let mut path = BezPath::new();
    let mut previous: Option<Point> = None;
    for (x, y) in x.iter().zip(y) {
        let point = x
            .zip(*y)
            .filter(|(x, y)| x.is_finite() && y.is_finite())
            .and_then(|(x, y)| map.data_to_local(x, y).ok());
        match (point, policy) {
            (Some(point), _) => {
                if let Some(previous) = previous {
                    path.line_to(Point::new(point.x, previous.y));
                    path.line_to(point);
                } else {
                    path.move_to(point);
                }
                previous = Some(point);
            }
            (None, NonFinitePolicy::Gap) => previous = None,
            (None, NonFinitePolicy::Drop) => {}
            (None, NonFinitePolicy::Error) => return Err(AxisError::OutOfDomain),
        }
    }
    Ok(path)
}

pub fn area_path(
    map: &CoordinateMap2D,
    x: &[Option<f64>],
    y: &[Option<f64>],
    baseline: f64,
) -> Result<BezPath, AxisError> {
    let points: Vec<(f64, f64)> = x
        .iter()
        .zip(y)
        .filter_map(|(x, y)| x.zip(*y))
        .filter(|(x, y)| x.is_finite() && y.is_finite())
        .collect();
    let mut path = BezPath::new();
    let Some((first_x, _)) = points.first().copied() else {
        return Ok(path);
    };
    path.move_to(map.data_to_local(first_x, baseline)?);
    for (x, y) in &points {
        path.line_to(map.data_to_local(*x, *y)?);
    }
    let last_x = points.last().expect("non-empty points").0;
    path.line_to(map.data_to_local(last_x, baseline)?);
    path.close_path();
    Ok(path)
}

pub fn scatter_points(
    map: &CoordinateMap2D,
    x: &[Option<f64>],
    y: &[Option<f64>],
    policy: NonFinitePolicy,
) -> Result<Vec<Point>, AxisError> {
    let mut points = Vec::new();
    for (x, y) in x.iter().zip(y) {
        match x
            .zip(*y)
            .filter(|(x, y)| x.is_finite() && y.is_finite())
            .and_then(|(x, y)| map.data_to_local(x, y).ok())
        {
            Some(point) => points.push(point),
            None if policy == NonFinitePolicy::Error => return Err(AxisError::OutOfDomain),
            None => {}
        }
    }
    Ok(points)
}

pub fn bars(
    map: &CoordinateMap2D,
    x: &[f64],
    values: &[f64],
    width: f64,
    baseline: f64,
) -> Result<Vec<RectMark>, AxisError> {
    let mut rects = Vec::new();
    for (&x, &value) in x.iter().zip(values) {
        if !x.is_finite() || !value.is_finite() || !width.is_finite() || width <= 0.0 {
            return Err(AxisError::OutOfDomain);
        }
        let left = map.data_to_local(x - width * 0.5, baseline)?;
        let right = map.data_to_local(x + width * 0.5, value)?;
        rects.push(RectMark {
            min: Point::new(left.x.min(right.x), left.y.min(right.y)),
            max: Point::new(left.x.max(right.x), left.y.max(right.y)),
            value,
        });
    }
    Ok(rects)
}

pub fn error_bar_path(
    map: &CoordinateMap2D,
    x: &[f64],
    y: &[f64],
    error_low: &[f64],
    error_high: &[f64],
    cap_width: f64,
) -> Result<BezPath, AxisError> {
    let mut path = BezPath::new();
    for (((&x, &y), &low), &high) in x.iter().zip(y).zip(error_low).zip(error_high) {
        let bottom = map.data_to_local(x, y - low)?;
        let top = map.data_to_local(x, y + high)?;
        path.move_to(bottom);
        path.line_to(top);
        path.move_to(Point::new(bottom.x - cap_width * 0.5, bottom.y));
        path.line_to(Point::new(bottom.x + cap_width * 0.5, bottom.y));
        path.move_to(Point::new(top.x - cap_width * 0.5, top.y));
        path.line_to(Point::new(top.x + cap_width * 0.5, top.y));
    }
    Ok(path)
}

pub fn histogram(values: &[f64], bins: usize) -> Option<Histogram> {
    if bins == 0 {
        return None;
    }
    let values: Vec<f64> = values
        .iter()
        .copied()
        .filter(|value| value.is_finite())
        .collect();
    let min = values.iter().copied().min_by(f64::total_cmp)?;
    let max = values.iter().copied().max_by(f64::total_cmp)?;
    let span = (max - min).max(f64::EPSILON);
    let mut counts = vec![0usize; bins];
    for value in values {
        let index = (((value - min) / span) * bins as f64).floor() as usize;
        counts[index.min(bins - 1)] += 1;
    }
    let edges = (0..=bins)
        .map(|index| min + span * index as f64 / bins as f64)
        .collect();
    Some(Histogram { edges, counts })
}

pub fn box_stats(values: &[f64]) -> Option<BoxStats> {
    let mut values: Vec<f64> = values
        .iter()
        .copied()
        .filter(|value| value.is_finite())
        .collect();
    if values.is_empty() {
        return None;
    }
    values.sort_by(f64::total_cmp);
    let quantile = |q: f64| {
        let position = q * (values.len() - 1) as f64;
        let lower = position.floor() as usize;
        let upper = position.ceil() as usize;
        let t = position - lower as f64;
        values[lower] * (1.0 - t) + values[upper] * t
    };
    Some(BoxStats {
        minimum: values[0],
        q1: quantile(0.25),
        median: quantile(0.5),
        q3: quantile(0.75),
        maximum: *values.last().expect("non-empty values"),
    })
}

pub fn violin_path(
    map: &CoordinateMap2D,
    center: f64,
    values: &[f64],
    bandwidth: f64,
    width: f64,
    samples: usize,
) -> Result<BezPath, AxisError> {
    let values: Vec<f64> = values
        .iter()
        .copied()
        .filter(|value| value.is_finite())
        .collect();
    let mut path = BezPath::new();
    if values.is_empty() || samples < 2 || bandwidth <= 0.0 || width <= 0.0 {
        return Ok(path);
    }
    let min = values.iter().copied().min_by(f64::total_cmp).unwrap();
    let max = values.iter().copied().max_by(f64::total_cmp).unwrap();
    let density = |y: f64| {
        values
            .iter()
            .map(|value| {
                let z = (y - value) / bandwidth;
                (-0.5 * z * z).exp()
            })
            .sum::<f64>()
            / values.len() as f64
    };
    let samples_y: Vec<f64> = (0..samples)
        .map(|index| min + (max - min) * index as f64 / (samples - 1) as f64)
        .collect();
    let densities: Vec<f64> = samples_y.iter().map(|y| density(*y)).collect();
    let max_density = densities
        .iter()
        .copied()
        .fold(0.0, f64::max)
        .max(f64::EPSILON);
    for (index, (y, density)) in samples_y.iter().zip(&densities).enumerate() {
        let point = map.data_to_local(center - width * density / max_density * 0.5, *y)?;
        if index == 0 {
            path.move_to(point);
        } else {
            path.line_to(point);
        }
    }
    for (y, density) in samples_y.iter().zip(&densities).rev() {
        path.line_to(map.data_to_local(center + width * density / max_density * 0.5, *y)?);
    }
    path.close_path();
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Axis, PlotFrame};

    #[test]
    fn histogram_accounts_for_every_finite_value() {
        let histogram = histogram(&[0.0, 0.2, 0.8, 1.0, f64::NAN], 4).unwrap();
        assert_eq!(histogram.counts.iter().sum::<usize>(), 4);
    }

    #[test]
    fn box_stats_interpolate_quartiles() {
        let stats = box_stats(&[1.0, 2.0, 3.0, 4.0]).unwrap();
        assert_eq!(stats.median, 2.5);
        assert_eq!(stats.q1, 1.75);
        assert_eq!(stats.q3, 3.25);
    }

    #[test]
    fn line_gap_starts_a_new_subpath() {
        let map = CoordinateMap2D::new(
            Axis::linear(0.0, 3.0).unwrap(),
            Axis::linear(0.0, 3.0).unwrap(),
            PlotFrame::new(300.0, 300.0).unwrap(),
        );
        let path = line_path(
            &map,
            &[Some(0.0), Some(1.0), None, Some(3.0)],
            &[Some(0.0), Some(1.0), None, Some(3.0)],
            NonFinitePolicy::Gap,
        )
        .unwrap();
        assert_eq!(
            path.elements()
                .iter()
                .filter(|element| matches!(element, gaanim_core::kurbo::PathEl::MoveTo(_)))
                .count(),
            2
        );
    }

    #[test]
    fn categorical_columns_map_to_axis_positions() {
        let map = CoordinateMap2D::new(
            Axis::category(["A", "B", "C"].map(str::to_owned)).unwrap(),
            Axis::linear(0.0, 5.0).unwrap(),
            PlotFrame::new(300.0, 200.0).unwrap(),
        );
        let table = DataTable::new([
            (
                "category".to_owned(),
                Column::Text(vec![Some("A".to_owned()), Some("C".to_owned())]),
            ),
            (
                "value".to_owned(),
                Column::Numeric(vec![Some(1.0), Some(4.0)]),
            ),
        ])
        .unwrap();
        let path = data_mark_path(
            &map,
            &table,
            &DataMarkKind::Line {
                x: "category".to_owned(),
                y: "value".to_owned(),
                policy: NonFinitePolicy::Gap,
            },
        )
        .unwrap();
        assert_eq!(path.elements().len(), 2);
    }

    #[test]
    fn heatmap_bands_batch_cells_by_value() {
        let map = CoordinateMap2D::new(
            Axis::linear(0.0, 2.0).unwrap(),
            Axis::linear(0.0, 2.0).unwrap(),
            PlotFrame::new(200.0, 200.0).unwrap(),
        );
        let table = DataTable::numeric([
            ("x".to_owned(), vec![0.5, 1.5]),
            ("y".to_owned(), vec![0.5, 1.5]),
            ("value".to_owned(), vec![0.0, 1.0]),
        ])
        .unwrap();
        let low = data_mark_path(
            &map,
            &table,
            &DataMarkKind::HeatmapBand {
                x: "x".to_owned(),
                y: "y".to_owned(),
                value: "value".to_owned(),
                cell_width: 1.0,
                cell_height: 1.0,
                band: 0,
                bands: 2,
            },
        )
        .unwrap();
        assert!(!low.is_empty());
    }
}
