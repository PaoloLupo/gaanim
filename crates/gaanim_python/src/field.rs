//! Scalar fields sampled on a grid from Python, for isosurfaces and
//! volumes: a function of x, y and z called at each grid point or once
//! with numpy arrays of all of them, or a 3D array of values.

use gaanim_core::glam::DVec3;
use gaanim_objects::isosurface::SampledField;
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::PyBytes;

/// The most points a field is sampled at: 128 on each side.
const MAX_POINTS: usize = 128 * 128 * 128;

/// Points on each side of a grid: one count for all three, or one each.
#[derive(FromPyObject)]
pub(crate) enum Resolution {
    Each(usize),
    Sides((usize, usize, usize)),
}

impl Resolution {
    fn sides(&self) -> [usize; 3] {
        match *self {
            Self::Each(count) => [count; 3],
            Self::Sides((x, y, z)) => [x, y, z],
        }
    }
}

/// The box from `x_range`, `y_range` and `z_range`, each finite with start
/// below end.
fn bounds(ranges: [(f64, f64); 3]) -> PyResult<(DVec3, DVec3)> {
    if !ranges
        .iter()
        .all(|(start, end)| start.is_finite() && end.is_finite() && start < end)
    {
        return Err(PyValueError::new_err(
            "x_range, y_range and z_range must be finite with start < end",
        ));
    }
    Ok((
        DVec3::new(ranges[0].0, ranges[1].0, ranges[2].0),
        DVec3::new(ranges[0].1, ranges[1].1, ranges[2].1),
    ))
}

/// The values of `field` over the box of `ranges`: called at each of the
/// `resolution` points a side, x fastest (once, with numpy arrays of all
/// of them, when `vectorized`), or, when it is not callable, the values of
/// a 3D array indexed `[x][y][z]`, which sets the resolution.
pub(crate) fn sample(
    py: Python<'_>,
    field: &Bound<'_, PyAny>,
    ranges: [(f64, f64); 3],
    resolution: &Resolution,
    vectorized: bool,
) -> PyResult<SampledField> {
    let (min, max) = bounds(ranges)?;
    if !field.is_callable() {
        let (values, size) = match py.import("numpy") {
            Ok(numpy) => {
                let array = numpy.call_method1("asarray", (field, "<f4"))?;
                let shape: Vec<usize> = array.getattr("shape")?.extract()?;
                let [nx, ny, nz] = shape[..] else {
                    return Err(not_three_dimensional());
                };
                check_size([nx, ny, nz])?;
                let kwargs = pyo3::types::PyDict::new(py);
                kwargs.set_item("order", "F")?;
                let flat = array.call_method("ravel", (), Some(&kwargs))?;
                (floats(&flat.call_method0("tobytes")?)?, [nx, ny, nz])
            }
            // Without numpy, nested sequences.
            Err(_) => {
                let rows: Vec<Vec<Vec<f64>>> =
                    field.extract().map_err(|_| not_three_dimensional())?;
                let size = [
                    rows.len(),
                    rows.first().map_or(0, Vec::len),
                    rows.first().and_then(|row| row.first()).map_or(0, Vec::len),
                ];
                let rectangular = rows.iter().all(|row| {
                    row.len() == size[1] && row.iter().all(|column| column.len() == size[2])
                });
                if !rectangular {
                    return Err(not_three_dimensional());
                }
                check_size(size)?;
                let mut values = Vec::with_capacity(size[0] * size[1] * size[2]);
                for z in 0..size[2] {
                    for y in 0..size[1] {
                        for row in &rows {
                            values.push(row[y][z] as f32);
                        }
                    }
                }
                (values, size)
            }
        };
        return Ok(SampledField {
            values,
            size,
            min,
            max,
        });
    }
    let size = resolution.sides();
    check_size(size)?;
    let axis = |index: usize, count: usize, axis: usize| {
        let (start, end) = ranges[axis];
        start + (end - start) * index as f64 / (count - 1) as f64
    };
    let values = if vectorized {
        let numpy = py.import("numpy")?;
        let line = |axis: usize| -> PyResult<Bound<'_, PyAny>> {
            let (start, end) = ranges[axis];
            numpy.call_method1("linspace", (start, end, size[axis]))
        };
        // Arrays of shape (nz, ny, nx), so that x runs fastest.
        let kwargs = pyo3::types::PyDict::new(py);
        kwargs.set_item("indexing", "ij")?;
        let grids = numpy.call_method("meshgrid", (line(2)?, line(1)?, line(0)?), Some(&kwargs))?;
        let (z, y, x): (Bound<'_, PyAny>, Bound<'_, PyAny>, Bound<'_, PyAny>) = grids.extract()?;
        let result = field.call1((x, y, z))?;
        let array = numpy.call_method1("asarray", (result, "<f4"))?;
        let array = numpy.call_method1("broadcast_to", (array, (size[2], size[1], size[0])))?;
        let flat = numpy.call_method1("ascontiguousarray", (array,))?;
        floats(&flat.call_method0("tobytes")?)?
    } else {
        let mut values = Vec::with_capacity(size[0] * size[1] * size[2]);
        for z in 0..size[2] {
            for y in 0..size[1] {
                for x in 0..size[0] {
                    let point = (
                        axis(x, size[0], 0),
                        axis(y, size[1], 1),
                        axis(z, size[2], 2),
                    );
                    values.push(field.call1(point)?.extract::<f64>()? as f32);
                }
            }
        }
        values
    };
    Ok(SampledField {
        values,
        size,
        min,
        max,
    })
}

fn not_three_dimensional() -> PyErr {
    PyValueError::new_err("a field of values must be a 3D array indexed [x][y][z]")
}

fn check_size(size: [usize; 3]) -> PyResult<()> {
    let points = size
        .iter()
        .try_fold(1_usize, |all, &side| all.checked_mul(side));
    if size.iter().any(|&side| side < 2) || points.is_none_or(|points| points > MAX_POINTS) {
        return Err(PyValueError::new_err(format!(
            "a field needs at least 2 points on each side and at most {MAX_POINTS} in all"
        )));
    }
    Ok(())
}

/// Little-endian 32-bit floats from numpy's bytes.
fn floats(bytes: &Bound<'_, PyAny>) -> PyResult<Vec<f32>> {
    let bytes = bytes.cast::<PyBytes>()?.as_bytes();
    Ok(bytes
        .as_chunks::<4>()
        .0
        .iter()
        .map(|chunk| f32::from_le_bytes(*chunk))
        .collect())
}
