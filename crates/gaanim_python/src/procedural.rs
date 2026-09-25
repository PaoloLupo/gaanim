//! Seeded randomness for authoring. The stream is the same `SeededRng` the
//! runtime uses, so values drawn here match native procedural effects.

use gaanim_math::SeededRng;
use pyo3::exceptions::{PyIndexError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::PyList;

#[pyclass(name = "Random", module = "gaanim_core")]
pub struct PyRandom {
    rng: SeededRng,
    seed: u64,
}

impl PyRandom {
    pub(crate) fn new(seed: u64) -> Self {
        Self {
            rng: SeededRng::new(seed),
            seed,
        }
    }

    fn finite(name: &str, value: f64) -> PyResult<()> {
        if value.is_finite() {
            Ok(())
        } else {
            Err(PyValueError::new_err(format!("{name} must be finite")))
        }
    }
}

#[pymethods]
impl PyRandom {
    #[getter]
    fn seed(&self) -> u64 {
        self.seed
    }

    /// Uniform float in `[low, high)`.
    #[pyo3(signature = (low=0.0, high=1.0))]
    fn uniform(&mut self, low: f64, high: f64) -> PyResult<f64> {
        Self::finite("low", low)?;
        Self::finite("high", high)?;
        if high < low {
            return Err(PyValueError::new_err("high must not be below low"));
        }
        Ok(self.rng.uniform(low, high))
    }

    /// Normal deviate with `mean` and non-negative `std`.
    #[pyo3(signature = (mean=0.0, std=1.0))]
    fn gauss(&mut self, mean: f64, std: f64) -> PyResult<f64> {
        Self::finite("mean", mean)?;
        Self::finite("std", std)?;
        if std < 0.0 {
            return Err(PyValueError::new_err("std must be non-negative"));
        }
        Ok(self.rng.gauss(mean, std))
    }

    /// Uniform integer in `[low, high)`.
    fn integer(&mut self, low: i64, high: i64) -> PyResult<i64> {
        if high <= low {
            return Err(PyValueError::new_err("high must be greater than low"));
        }
        Ok(self.rng.integer(low, high))
    }

    /// One element of a non-empty sequence.
    fn choice(&mut self, items: &Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
        let length = items.len()?;
        if length == 0 {
            return Err(PyIndexError::new_err(
                "cannot choose from an empty sequence",
            ));
        }
        let index = self.rng.integer(0, length as i64) as usize;
        Ok(items.get_item(index)?.unbind())
    }

    /// Shuffle a list in place.
    fn shuffle(&mut self, items: &Bound<'_, PyList>) -> PyResult<()> {
        let mut values: Vec<Py<PyAny>> = items.iter().map(Bound::unbind).collect();
        self.rng.shuffle(&mut values);
        for (index, value) in values.into_iter().enumerate() {
            items.set_item(index, value)?;
        }
        Ok(())
    }

    fn __repr__(&self) -> String {
        format!("Random(seed={})", self.seed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn random_matches_the_native_stream_and_validates() {
        let mut random = PyRandom::new(42);
        let mut native = SeededRng::new(42);
        for _ in 0..50 {
            assert_eq!(
                random.uniform(-6.0, 6.0).unwrap(),
                native.uniform(-6.0, 6.0)
            );
        }
        assert!(random.uniform(2.0, 1.0).is_err());
        assert!(random.gauss(0.0, -1.0).is_err());
        assert!(random.integer(3, 3).is_err());
        assert!((0..100).all(|_| (0..4).contains(&random.integer(0, 4).unwrap())));
    }
}
