//! `scene.poll`: audience poll data for the scene to present.

use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;

use gaanim_api::canvas::{
    AudienceHandle, BarDirection, BarScale, LeaderboardHandle, LiveTextOptions, PollBarOptions,
    PollError, PollHandle, TextAlign,
};

use crate::pydrawable::PyDrawable;
use crate::visualization::PyParameter;

pub(crate) fn poll_error(error: PollError) -> PyErr {
    PyValueError::new_err(error.to_string())
}

fn bar_options(
    length: f64,
    thickness: f64,
    direction: &str,
    scale: &str,
    radius: f64,
) -> PyResult<PollBarOptions> {
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
    Ok(PollBarOptions {
        length,
        thickness,
        radius,
        direction,
        scale,
    })
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

    /// `answer`'s share of all votes, from 0 to 100.
    fn percent(&self, answer: usize) -> PyResult<PyParameter> {
        crate::custom::ensure_authoring_allowed()?;
        let inner = self.inner.percent(answer).map_err(poll_error)?;
        Ok(PyParameter { inner })
    }

    /// Seconds left to answer a quiz.
    fn remaining(&self) -> PyResult<PyParameter> {
        crate::custom::ensure_authoring_allowed()?;
        let inner = self.inner.remaining().map_err(poll_error)?;
        Ok(PyParameter { inner })
    }

    /// Reveal a quiz's answer at the cursor.
    fn reveal(&self) -> PyResult<()> {
        crate::custom::ensure_authoring_allowed()?;
        self.inner.reveal().map_err(poll_error)
    }

    /// Whether the poll is a quiz.
    #[getter]
    fn is_quiz(&self) -> bool {
        self.inner.correct().is_some()
    }

    /// A quiz's correct answer; `None` for a poll.
    #[getter]
    fn correct(&self) -> Option<usize> {
        self.inner.correct()
    }

    /// Seconds a quiz gives to answer; `None` for a poll.
    #[getter]
    fn time(&self) -> Option<u32> {
        self.inner.time()
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
        let options = bar_options(length, thickness, direction, scale, radius)?;
        let handle = self.inner.bar(answer, options).map_err(poll_error)?;
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

fn text_options(
    size: Option<f64>,
    weight: Option<u16>,
    font: Option<String>,
    align: &str,
) -> PyResult<LiveTextOptions> {
    let align = TextAlign::from_name(align).ok_or_else(|| {
        PyValueError::new_err(format!(
            "align must be \"left\", \"center\" or \"right\", got {align:?}"
        ))
    })?;
    Ok(LiveTextOptions {
        size,
        weight,
        font,
        align,
    })
}

/// The game's leaderboard: the players of every quiz, best first, as data
/// for the scene to present as it likes.
#[pyclass(name = "Leaderboard", module = "gaanim_core", frozen)]
pub struct PyLeaderboard {
    pub(crate) inner: LeaderboardHandle,
}

#[pymethods]
impl PyLeaderboard {
    /// Nicknames and scores shown outside a live presentation, best first.
    #[getter]
    fn preview(&self) -> Vec<(String, u64)> {
        self.inner.preview()
    }

    /// The nickname at `rank` (0 for the leader) as live text.
    #[pyo3(signature = (rank, *, size=None, weight=None, font=None, align="left"))]
    fn name(
        &self,
        rank: usize,
        size: Option<f64>,
        weight: Option<u16>,
        font: Option<String>,
        align: &str,
    ) -> PyResult<PyDrawable> {
        crate::custom::ensure_authoring_allowed()?;
        let options = text_options(size, weight, font, align)?;
        let handle = self.inner.name(rank, options).map_err(poll_error)?;
        Ok(PyDrawable(handle))
    }

    /// The score of the player at `rank`.
    fn points(&self, rank: usize) -> PyResult<PyParameter> {
        crate::custom::ensure_authoring_allowed()?;
        let inner = self.inner.points(rank).map_err(poll_error)?;
        Ok(PyParameter { inner })
    }

    /// How many players joined.
    fn players(&self) -> PyResult<PyParameter> {
        crate::custom::ensure_authoring_allowed()?;
        let inner = self.inner.players().map_err(poll_error)?;
        Ok(PyParameter { inner })
    }

    /// A bar whose length is the score at `rank` against the leader's.
    #[pyo3(signature = (rank, *, length=6.0, thickness=0.5, direction="right", radius=0.0))]
    fn bar(
        &self,
        rank: usize,
        length: f64,
        thickness: f64,
        direction: &str,
        radius: f64,
    ) -> PyResult<PyDrawable> {
        crate::custom::ensure_authoring_allowed()?;
        let options = bar_options(length, thickness, direction, "leader", radius)?;
        let handle = self.inner.bar(rank, options).map_err(poll_error)?;
        Ok(PyDrawable(handle))
    }

    fn __repr__(&self) -> String {
        format!("Leaderboard(preview={:?})", self.inner.preview())
    }
}

/// The game's audience: the players in the order they joined, each in a
/// slot (0 for the first), as data for the scene to arrange and animate.
#[pyclass(name = "Audience", module = "gaanim_core", frozen)]
pub struct PyAudience {
    pub(crate) inner: AudienceHandle,
}

#[pymethods]
impl PyAudience {
    /// Nicknames shown outside a live presentation, in joining order.
    #[getter]
    fn preview(&self) -> Vec<String> {
        self.inner.preview()
    }

    /// The session code phones type.
    #[getter]
    fn code(&self) -> String {
        self.inner.code()
    }

    /// The address phones open to join.
    #[getter]
    fn url(&self) -> String {
        self.inner.url()
    }

    /// The QR code of `url`, `size` units on a side, filled black.
    #[pyo3(signature = (size=3.0))]
    fn qr(&self, size: f64) -> PyResult<PyDrawable> {
        crate::custom::ensure_authoring_allowed()?;
        let handle = self.inner.qr(size).map_err(poll_error)?;
        Ok(PyDrawable(handle))
    }

    /// The nickname in `slot` as live text; empty until that many joined.
    #[pyo3(signature = (slot, *, size=None, weight=None, font=None, align="center"))]
    fn name(
        &self,
        slot: usize,
        size: Option<f64>,
        weight: Option<u16>,
        font: Option<String>,
        align: &str,
    ) -> PyResult<PyDrawable> {
        crate::custom::ensure_authoring_allowed()?;
        let options = text_options(size, weight, font, align)?;
        let handle = self.inner.name(slot, options).map_err(poll_error)?;
        Ok(PyDrawable(handle))
    }

    /// How many players joined.
    fn count(&self) -> PyResult<PyParameter> {
        crate::custom::ensure_authoring_allowed()?;
        let inner = self.inner.count().map_err(poll_error)?;
        Ok(PyParameter { inner })
    }

    /// 1 once a player took `slot`, else 0.
    fn joined(&self, slot: usize) -> PyResult<PyParameter> {
        crate::custom::ensure_authoring_allowed()?;
        let inner = self.inner.joined(slot).map_err(poll_error)?;
        Ok(PyParameter { inner })
    }

    /// Seconds since the player in `slot` joined, up to 60; 0 while empty.
    fn age(&self, slot: usize) -> PyResult<PyParameter> {
        crate::custom::ensure_authoring_allowed()?;
        let inner = self.inner.age(slot).map_err(poll_error)?;
        Ok(PyParameter { inner })
    }

    fn __repr__(&self) -> String {
        format!(
            "Audience(preview={:?}, code={:?})",
            self.inner.preview(),
            self.inner.code()
        )
    }
}
