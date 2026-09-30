//! `scene.poll`: audience poll data for the scene to present.

use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;

use gaanim_api::canvas::{BarDirection, BarScale, PollBarOptions, PollError, PollHandle};

use crate::pydrawable::PyDrawable;
use crate::visualization::PyParameter;

pub(crate) fn poll_error(error: PollError) -> PyErr {
    PyValueError::new_err(error.to_string())
}

/// An audience poll: the data of a question the audience answers from their
/// phones, for the scene to present as it likes.
#[pyclass(name = "Poll", module = "gaanim_core", frozen)]
pub struct PyPoll {
    pub(crate) inner: PollHandle,
}

#[pymethods]
impl PyPoll {
    /// Stable id of the poll on the relay.
    #[getter]
    fn id(&self) -> String {
        self.inner.id()
    }

    #[getter]
    fn question(&self) -> String {
        self.inner.question()
    }

    #[getter]
    fn options(&self) -> Vec<String> {
        self.inner.options()
    }

    /// Counts shown outside a live presentation.
    #[getter]
    fn preview(&self) -> Vec<u32> {
        self.inner.preview()
    }

    /// Six-character code phones type at the relay's address.
    #[getter]
    fn code(&self) -> String {
        self.inner.code()
    }

    /// Address the QR code opens.
    #[getter]
    fn url(&self) -> String {
        self.inner.url()
    }

    /// Votes for `answer` (0 for the first), live while presenting.
    fn votes(&self, answer: usize) -> PyResult<PyParameter> {
        crate::custom::ensure_authoring_allowed()?;
        let inner = self.inner.votes(answer).map_err(poll_error)?;
        Ok(PyParameter { inner })
    }

    /// `answer`'s fraction of all votes, from 0 to 1.
    fn share(&self, answer: usize) -> PyResult<PyParameter> {
        crate::custom::ensure_authoring_allowed()?;
        let inner = self.inner.share(answer).map_err(poll_error)?;
        Ok(PyParameter { inner })
    }

    /// Votes for every answer.
    fn total(&self) -> PyResult<PyParameter> {
        crate::custom::ensure_authoring_allowed()?;
        let inner = self.inner.total().map_err(poll_error)?;
        Ok(PyParameter { inner })
    }

    /// A bar whose length follows `answer`.
    #[pyo3(signature = (answer, *, length=6.0, thickness=0.5, direction="right", scale="leader", radius=0.0))]
    fn bar(
        &self,
        answer: usize,
        length: f64,
        thickness: f64,
        direction: &str,
        scale: &str,
        radius: f64,
    ) -> PyResult<PyDrawable> {
        crate::custom::ensure_authoring_allowed()?;
        let direction = BarDirection::from_name(direction).ok_or_else(|| {
            PyValueError::new_err(format!(
                "direction must be \"right\", \"left\", \"up\" or \"down\", got {direction:?}"
            ))
        })?;
        let scale = BarScale::from_name(scale).ok_or_else(|| {
            PyValueError::new_err(format!(
                "scale must be \"leader\" or \"total\", got {scale:?}"
            ))
        })?;
        let handle = self
            .inner
            .bar(
                answer,
                PollBarOptions {
                    length,
                    thickness,
                    radius,
                    direction,
                    scale,
                },
            )
            .map_err(poll_error)?;
        Ok(PyDrawable(handle))
    }

    /// The QR code of `url`, `size` scene units on a side.
    #[pyo3(signature = (size=3.0))]
    fn qr(&self, size: f64) -> PyResult<PyDrawable> {
        crate::custom::ensure_authoring_allowed()?;
        Ok(PyDrawable(self.inner.qr(size).map_err(poll_error)?))
    }

    /// Stop taking votes at the cursor.
    fn close(&self) -> PyResult<()> {
        crate::custom::ensure_authoring_allowed()?;
        self.inner.close().map_err(poll_error)
    }

    fn __repr__(&self) -> String {
        format!(
            "Poll({:?}, {:?}, code={:?})",
            self.inner.question(),
            self.inner.options(),
            self.inner.code()
        )
    }
}
