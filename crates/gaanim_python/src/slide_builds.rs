//! Slide components that keep their parts: `scene.slides.bullets` returns a
//! `BulletList` and `scene.slides.table` a `SlideTable`, both `Drawable`s
//! whose items, rows and cells stay addressable and can be revealed one per
//! presentation step with `build`.
use gaanim_api::canvas::DrawableHandle;
use pyo3::{
    exceptions::{PyIndexError, PyKeyError, PyTypeError, PyValueError},
    prelude::*,
    types::{PyList, PyTuple},
};

use crate::pycanvas::PyScene;
use crate::pydrawable::PyDrawable;

/// Longest stop name `build` derives from an item's text.
const STOP_NAME_CHARS: usize = 40;

/// A bullet list from `scene.slides.bullets`: one `Drawable` whose points
/// stay addressable.
#[pyclass(name = "BulletList", module = "gaanim_core", extends = PyDrawable)]
pub struct PyBulletList {
    pub(crate) items: Vec<DrawableHandle>,
    pub(crate) labels: Vec<String>,
    pub(crate) scene: Py<PyScene>,
}

/// A table from `scene.slides.table`: one `Drawable` whose header, rows and
/// cells stay addressable.
#[pyclass(name = "SlideTable", module = "gaanim_core", extends = PyDrawable)]
pub struct PySlideTable {
    pub(crate) header: DrawableHandle,
    pub(crate) header_cells: Vec<DrawableHandle>,
    pub(crate) headers: Vec<String>,
    /// One group per data row, holding its cells.
    pub(crate) rows: Vec<DrawableHandle>,
    /// `cells[row][column]`, data rows only.
    pub(crate) cells: Vec<Vec<DrawableHandle>>,
    pub(crate) texts: Vec<Vec<String>>,
    pub(crate) scene: Py<PyScene>,
}

fn drawable(handle: &DrawableHandle) -> PyDrawable {
    PyDrawable(handle.clone())
}

/// A stop name from a step's text: its first line, at most
/// [`STOP_NAME_CHARS`] characters, with an ellipsis when cut.
fn stop_name(text: &str) -> String {
    let line = text.lines().next().unwrap_or("").trim();
    if line.chars().count() <= STOP_NAME_CHARS {
        return line.to_owned();
    }
    let mut cut: String = line.chars().take(STOP_NAME_CHARS - 1).collect();
    cut.truncate(cut.trim_end().len());
    cut.push('…');
    cut
}

/// Resolve a Python index (negative counts from the end) into `0..len`.
fn resolve_index(index: isize, len: usize, what: &str) -> PyResult<usize> {
    let resolved = if index < 0 {
        index + len as isize
    } else {
        index
    };
    if resolved < 0 || resolved as usize >= len {
        return Err(PyIndexError::new_err(format!(
            "{what} index {index} is out of range for {len} {what}s"
        )));
    }
    Ok(resolved as usize)
}

/// Step names: the caller's, one per step, or derived from each step's text.
fn step_names(names: Option<Vec<String>>, texts: Vec<String>) -> PyResult<Vec<String>> {
    match names {
        Some(names) => {
            if names.len() != texts.len() {
                return Err(PyValueError::new_err(format!(
                    "names must give one name per step: {} steps, {} names",
                    texts.len(),
                    names.len()
                )));
            }
            if names.iter().any(|name| name.trim().is_empty()) {
                return Err(PyValueError::new_err("step names must not be empty"));
            }
            Ok(names)
        }
        None => Ok(texts.iter().map(|text| stop_name(text)).collect()),
    }
}

/// `handle.animate.<effect>(*args).duration(duration)`.
fn animation<'py>(
    py: Python<'py>,
    handle: &DrawableHandle,
    effect: &str,
    args: Bound<'py, PyTuple>,
    duration: f64,
) -> PyResult<Bound<'py, PyAny>> {
    Bound::new(py, drawable(handle))?
        .getattr("animate")?
        .call_method1(effect, args)?
        .call_method1("duration", (duration,))
}

/// Reveal each step in turn on `scene`: fade its drawables in, fade the
/// previous step to `dim`, then stop under its name.
fn play_steps(
    py: Python<'_>,
    scene: &Py<PyScene>,
    steps: &[Vec<DrawableHandle>],
    names: &[String],
    dim: Option<f64>,
    duration: f64,
    stop: bool,
) -> PyResult<()> {
    if !duration.is_finite() || duration <= 0.0 {
        return Err(PyValueError::new_err(
            "duration must be finite and positive",
        ));
    }
    if let Some(dim) = dim
        && (!dim.is_finite() || !(0.0..=1.0).contains(&dim))
    {
        return Err(PyValueError::new_err("dim must be between 0 and 1"));
    }
    let scene = scene.bind(py);
    let no_args = PyTuple::empty(py);
    for (index, step) in steps.iter().enumerate() {
        let mut animations = Vec::with_capacity(step.len() * 2);
        for handle in step {
            animations.push(animation(py, handle, "fade_in", no_args.clone(), duration)?);
        }
        if let (Some(dim), Some(previous)) = (dim, index.checked_sub(1).map(|i| &steps[i]))
            && dim < 1.0
        {
            for handle in previous {
                animations.push(animation(
                    py,
                    handle,
                    "opacity",
                    PyTuple::new(py, [dim])?,
                    duration,
                )?);
            }
        }
        scene.call_method1("play", (PyList::new(py, animations)?,))?;
        if stop {
            scene.call_method1("stop", (names[index].as_str(),))?;
        }
    }
    Ok(())
}

#[pymethods]
impl PyBulletList {
    /// One `Drawable` per point, each its bullet and its text, in order.
    #[getter]
    fn items<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyTuple>> {
        crate::custom::ensure_authoring_allowed()?;
        PyTuple::new(py, self.items.iter().map(drawable))
    }

    /// Reveal the points one per step; see `gaanim_core.pyi`.
    #[pyo3(signature = (*, dim=None, duration=0.4, names=None, stop=true))]
    fn build(
        &self,
        py: Python<'_>,
        dim: Option<f64>,
        duration: f64,
        names: Option<Vec<String>>,
        stop: bool,
    ) -> PyResult<()> {
        crate::custom::ensure_authoring_allowed()?;
        let names = step_names(names, self.labels.clone())?;
        let steps: Vec<Vec<DrawableHandle>> =
            self.items.iter().map(|item| vec![item.clone()]).collect();
        play_steps(py, &self.scene, &steps, &names, dim, duration, stop)
    }
}

impl PySlideTable {
    fn column_index(&self, column: &Bound<'_, PyAny>) -> PyResult<usize> {
        if let Ok(index) = column.extract::<isize>() {
            return resolve_index(index, self.headers.len(), "column");
        }
        if let Ok(name) = column.extract::<String>() {
            return self
                .headers
                .iter()
                .position(|header| *header == name)
                .ok_or_else(|| {
                    PyKeyError::new_err(format!(
                        "no column named {name:?}; the columns are {:?}",
                        self.headers
                    ))
                });
        }
        Err(PyTypeError::new_err(
            "a column is an index or a header name",
        ))
    }
}

#[pymethods]
impl PySlideTable {
    /// `(rows, columns)` of the data, without the header row.
    #[getter]
    fn shape(&self) -> (usize, usize) {
        (self.rows.len(), self.headers.len())
    }

    /// The header row: a `Drawable` holding the header texts.
    #[getter]
    fn header(&self) -> PyResult<PyDrawable> {
        crate::custom::ensure_authoring_allowed()?;
        Ok(drawable(&self.header))
    }

    /// Data row `index` (0 is the first row under the header).
    fn row(&self, index: isize) -> PyResult<PyDrawable> {
        crate::custom::ensure_authoring_allowed()?;
        Ok(drawable(
            &self.rows[resolve_index(index, self.rows.len(), "row")?],
        ))
    }

    /// The text of data row `row` in `column`, an index or a header name.
    fn cell(&self, row: isize, column: &Bound<'_, PyAny>) -> PyResult<PyDrawable> {
        crate::custom::ensure_authoring_allowed()?;
        let row = resolve_index(row, self.rows.len(), "row")?;
        Ok(drawable(&self.cells[row][self.column_index(column)?]))
    }

    /// The cells of `column` (an index or a header name), header excluded.
    fn column<'py>(
        &self,
        py: Python<'py>,
        column: &Bound<'py, PyAny>,
    ) -> PyResult<Bound<'py, PyTuple>> {
        crate::custom::ensure_authoring_allowed()?;
        let column = self.column_index(column)?;
        PyTuple::new(py, self.cells.iter().map(|row| drawable(&row[column])))
    }

    /// The header text of `column`, an index or a header name.
    fn header_cell(&self, column: &Bound<'_, PyAny>) -> PyResult<PyDrawable> {
        crate::custom::ensure_authoring_allowed()?;
        Ok(drawable(&self.header_cells[self.column_index(column)?]))
    }

    /// Reveal the rows (or columns) one per step; see `gaanim_core.pyi`.
    #[pyo3(signature = (*, by="row", dim=None, duration=0.4, names=None, stop=true))]
    fn build(
        &self,
        py: Python<'_>,
        by: &str,
        dim: Option<f64>,
        duration: f64,
        names: Option<Vec<String>>,
        stop: bool,
    ) -> PyResult<()> {
        crate::custom::ensure_authoring_allowed()?;
        let (steps, texts): (Vec<Vec<DrawableHandle>>, Vec<String>) = match by {
            "row" => (
                self.rows.iter().map(|row| vec![row.clone()]).collect(),
                self.texts.iter().map(|row| row[0].clone()).collect(),
            ),
            "column" => (
                (0..self.headers.len())
                    .map(|column| self.cells.iter().map(|row| row[column].clone()).collect())
                    .collect(),
                self.headers.clone(),
            ),
            other => {
                return Err(PyValueError::new_err(format!(
                    "by must be 'row' or 'column', not {other:?}"
                )));
            }
        };
        let names = step_names(names, texts)?;
        play_steps(py, &self.scene, &steps, &names, dim, duration, stop)
    }
}

#[cfg(test)]
mod tests {
    use super::stop_name;

    #[test]
    fn stop_names_keep_short_text() {
        assert_eq!(stop_name("  Measure  "), "Measure");
    }

    #[test]
    fn stop_names_take_the_first_line() {
        assert_eq!(stop_name("Model\nsecond line"), "Model");
    }

    #[test]
    fn stop_names_cut_long_text_with_an_ellipsis() {
        let name = stop_name(&"word ".repeat(20));
        assert_eq!(name, format!("{}word…", "word ".repeat(7)));
    }

    #[test]
    fn stop_names_do_not_end_in_a_space_before_the_ellipsis() {
        // The first 39 characters end on the space after the 13th word.
        let name = stop_name(&"ab ".repeat(15));
        assert_eq!(name, format!("{}ab…", "ab ".repeat(12)));
    }
}
