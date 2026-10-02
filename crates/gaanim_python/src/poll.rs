//! `scene.poll`: audience poll data for the scene to present.

use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;

use gaanim_api::canvas::{
    AudienceHandle, BarDirection, BarScale, GateCondition, LeaderboardHandle, LiveTextOptions,
    PollBarOptions, PollError, PollHandle, TeamsHandle, TextAlign,
};

use crate::pydrawable::PyDrawable;
use crate::visualization::PyParameter;

pub(crate) fn poll_error(error: PollError) -> PyErr {
    PyValueError::new_err(error.to_string())
}

/// How a poll's `rehearse=` leans: a share that answers a quiz right, or
/// one weight per answer.
pub(crate) fn lean(
    rehearse: Option<&Bound<'_, PyAny>>,
) -> PyResult<gaanim_animation::rehearsal::Lean> {
    use gaanim_animation::rehearsal::Lean;
    let Some(rehearse) = rehearse else {
        return Ok(Lean::Auto);
    };
    if let Ok(share) = rehearse.extract::<f64>() {
        return Ok(Lean::Right(share));
    }
    rehearse
        .extract::<Vec<f64>>()
        .map(Lean::Weights)
        .map_err(|_| {
            pyo3::exceptions::PyTypeError::new_err(
                "rehearse is the share that answers right (a quiz) or one weight per answer",
            )
        })
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
    /// A condition for `scene.stop(until=...)`: at least `at_least` answers,
    /// or answers from at least `share` (0 to 1) of the audience. Exactly
    /// one of the two.
    #[pyo3(signature = (*, at_least=None, share=None))]
    fn answered(&self, at_least: Option<u32>, share: Option<f64>) -> PyResult<PyCondition> {
        let inner = self.inner.answered(at_least, share).map_err(poll_error)?;
        Ok(PyCondition { inner })
    }

    /// A condition for `scene.stop(until=...)`: the quiz is out of time.
    fn time_up(&self) -> PyResult<PyCondition> {
        let inner = self.inner.time_up().map_err(poll_error)?;
        Ok(PyCondition { inner })
    }

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

    /// 0 until the quiz reveals its answer, then 1.
    fn revealed(&self) -> PyResult<PyParameter> {
        crate::custom::ensure_authoring_allowed()?;
        let inner = self.inner.revealed().map_err(poll_error)?;
        Ok(PyParameter { inner })
    }

    /// Whether the poll is a quiz.
    #[getter]
    fn is_quiz(&self) -> bool {
        self.inner.correct().is_some()
    }

    /// A quiz's right answer, or the list of them for a multiple choice
    /// quiz; `None` for a poll.
    #[getter]
    fn correct(&self, py: Python<'_>) -> PyResult<Option<Py<PyAny>>> {
        let Some(correct) = self.inner.correct() else {
            return Ok(None);
        };
        Ok(Some(match correct.as_slice() {
            [one] if !self.inner.multiple() => one.into_pyobject(py)?.into_any().unbind(),
            many => many.into_pyobject(py)?.into_any().unbind(),
        }))
    }

    /// The shape `answer` has on phones, `size` units tall, filled with its
    /// color.
    #[pyo3(signature = (answer, size=0.6))]
    fn icon(&self, answer: usize, size: f64) -> PyResult<PyDrawable> {
        crate::custom::ensure_authoring_allowed()?;
        let handle = self.inner.icon(answer, size).map_err(poll_error)?;
        Ok(PyDrawable(handle))
    }

    /// The color `answer` has on phones, `#rrggbb`.
    fn color(&self, answer: usize) -> PyResult<&'static str> {
        self.inner.color(answer).map_err(poll_error)
    }

    /// Whether players may choose several answers.
    #[getter]
    fn multiple(&self) -> bool {
        self.inner.multiple()
    }

    /// Whether phones show a picture above the question.
    #[getter]
    fn has_image(&self) -> bool {
        self.inner.image().is_some()
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

/// What the audience must do before a stop advances by itself, for
/// `scene.stop(until=...)`. `a | b` holds when either does, `a & b` when both
/// do.
#[pyclass(name = "Condition", module = "gaanim_core", frozen)]
pub struct PyCondition {
    pub(crate) inner: GateCondition,
}

impl PyCondition {
    fn join(
        left: &GateCondition,
        right: &GateCondition,
        make: fn(Vec<GateCondition>) -> GateCondition,
        unwrap: fn(&GateCondition) -> Option<&Vec<GateCondition>>,
    ) -> GateCondition {
        let mut parts = Vec::new();
        for side in [left, right] {
            match unwrap(side) {
                Some(inner) => parts.extend(inner.iter().cloned()),
                None => parts.push(side.clone()),
            }
        }
        make(parts)
    }
}

#[pymethods]
impl PyCondition {
    fn __or__(&self, other: &PyCondition) -> PyCondition {
        PyCondition {
            inner: Self::join(&self.inner, &other.inner, GateCondition::Any, |condition| {
                match condition {
                    GateCondition::Any(parts) => Some(parts),
                    _ => None,
                }
            }),
        }
    }

    fn __and__(&self, other: &PyCondition) -> PyCondition {
        PyCondition {
            inner: Self::join(&self.inner, &other.inner, GateCondition::All, |condition| {
                match condition {
                    GateCondition::All(parts) => Some(parts),
                    _ => None,
                }
            }),
        }
    }

    fn __repr__(&self) -> String {
        format!("Condition({:?})", self.inner)
    }
}

/// The game's leaderboard: the players of every quiz, best first, as data
/// for the scene to present as it likes.
#[pyclass(name = "Leaderboard", module = "gaanim_core", frozen)]
pub struct PyLeaderboard {
    pub(crate) inner: LeaderboardHandle,
}

impl PyLeaderboard {
    /// `fact` about the player, for the methods below.
    fn fact(
        &self,
        index: usize,
        fact: gaanim_api::canvas::PlayerFact,
        poll: Option<PyRef<'_, PyPoll>>,
    ) -> PyResult<PyParameter> {
        crate::custom::ensure_authoring_allowed()?;
        let inner = self
            .inner
            .player(index, fact, poll.as_ref().map(|poll| &poll.inner))
            .map_err(poll_error)?;
        Ok(PyParameter { inner })
    }
}

#[pymethods]
impl PyLeaderboard {
    /// Quizzes the player answered right.
    fn correct(&self, rank: usize) -> PyResult<PyParameter> {
        self.fact(rank, gaanim_api::canvas::PlayerFact::Correct, None)
    }

    /// Quizzes the player answered.
    fn answered(&self, rank: usize) -> PyResult<PyParameter> {
        self.fact(rank, gaanim_api::canvas::PlayerFact::Answered, None)
    }

    /// Quizzes the player answered right in a row.
    fn streak(&self, rank: usize) -> PyResult<PyParameter> {
        self.fact(rank, gaanim_api::canvas::PlayerFact::Streak, None)
    }

    /// 1 once the player answered `poll`, else 0.
    fn responded(&self, rank: usize, poll: PyRef<'_, PyPoll>) -> PyResult<PyParameter> {
        self.fact(rank, gaanim_api::canvas::PlayerFact::Responded, Some(poll))
    }

    /// 1 if the player chose `answer` on `poll`, else 0.
    fn chose(&self, rank: usize, poll: PyRef<'_, PyPoll>, answer: usize) -> PyResult<PyParameter> {
        self.fact(
            rank,
            gaanim_api::canvas::PlayerFact::Chose(answer),
            Some(poll),
        )
    }

    /// 1 if the player answered quiz `poll` right, else 0.
    fn right(&self, rank: usize, poll: PyRef<'_, PyPoll>) -> PyResult<PyParameter> {
        self.fact(rank, gaanim_api::canvas::PlayerFact::Right, Some(poll))
    }

    /// Points the player's answer to quiz `poll` earned.
    fn earned(&self, rank: usize, poll: PyRef<'_, PyPoll>) -> PyResult<PyParameter> {
        self.fact(rank, gaanim_api::canvas::PlayerFact::Earned, Some(poll))
    }

    /// Seconds the player took to answer quiz `poll`, 0 without an answer.
    fn answer_time(&self, rank: usize, poll: PyRef<'_, PyPoll>) -> PyResult<PyParameter> {
        self.fact(rank, gaanim_api::canvas::PlayerFact::Time, Some(poll))
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
        "Leaderboard()".to_string()
    }
}

/// The game's teams, as data for the scene to present as it likes.
#[pyclass(name = "Teams", module = "gaanim_core", frozen)]
pub struct PyTeams {
    pub(crate) inner: TeamsHandle,
}

#[pymethods]
impl PyTeams {
    #[getter]
    fn names(&self) -> Vec<String> {
        self.inner.names()
    }

    #[getter]
    fn colors(&self) -> Vec<String> {
        self.inner.colors()
    }

    /// Whether players choose their team on the phone.
    #[getter]
    fn choose(&self) -> bool {
        self.inner.choose()
    }

    fn __len__(&self) -> usize {
        self.inner.names().len()
    }

    /// The points of `team`'s players added up.
    fn score(&self, team: usize) -> PyResult<PyParameter> {
        crate::custom::ensure_authoring_allowed()?;
        let inner = self.inner.score(team).map_err(poll_error)?;
        Ok(PyParameter { inner })
    }

    /// How many players `team` has.
    fn players(&self, team: usize) -> PyResult<PyParameter> {
        crate::custom::ensure_authoring_allowed()?;
        let inner = self.inner.players(team).map_err(poll_error)?;
        Ok(PyParameter { inner })
    }

    /// `team`'s points per player.
    fn average(&self, team: usize) -> PyResult<PyParameter> {
        crate::custom::ensure_authoring_allowed()?;
        let inner = self.inner.average(team).map_err(poll_error)?;
        Ok(PyParameter { inner })
    }

    /// The index of the team leading on points.
    fn leader(&self) -> PyResult<PyParameter> {
        crate::custom::ensure_authoring_allowed()?;
        let inner = self.inner.leader().map_err(poll_error)?;
        Ok(PyParameter { inner })
    }

    /// A bar whose length is `team`'s points against the leading team's.
    #[pyo3(signature = (team, *, length=6.0, thickness=0.5, direction="right", radius=0.0))]
    fn bar(
        &self,
        team: usize,
        length: f64,
        thickness: f64,
        direction: &str,
        radius: f64,
    ) -> PyResult<PyDrawable> {
        crate::custom::ensure_authoring_allowed()?;
        let options = bar_options(length, thickness, direction, "leader", radius)?;
        let handle = self.inner.bar(team, options).map_err(poll_error)?;
        Ok(PyDrawable(handle))
    }

    fn __repr__(&self) -> String {
        format!("Teams({:?})", self.inner.names())
    }
}

/// The game's audience: the players in the order they joined, each in a
/// slot (0 for the first), as data for the scene to arrange and animate.
#[pyclass(name = "Audience", module = "gaanim_core", frozen)]
pub struct PyAudience {
    pub(crate) inner: AudienceHandle,
}

impl PyAudience {
    /// `fact` about the player, for the methods below.
    fn fact(
        &self,
        index: usize,
        fact: gaanim_api::canvas::PlayerFact,
        poll: Option<PyRef<'_, PyPoll>>,
    ) -> PyResult<PyParameter> {
        crate::custom::ensure_authoring_allowed()?;
        let inner = self
            .inner
            .player(index, fact, poll.as_ref().map(|poll| &poll.inner))
            .map_err(poll_error)?;
        Ok(PyParameter { inner })
    }
}

#[pymethods]
impl PyAudience {
    /// The player's points.
    fn score(&self, slot: usize) -> PyResult<PyParameter> {
        self.fact(slot, gaanim_api::canvas::PlayerFact::Score, None)
    }

    /// Quizzes the player answered right.
    fn correct(&self, slot: usize) -> PyResult<PyParameter> {
        self.fact(slot, gaanim_api::canvas::PlayerFact::Correct, None)
    }

    /// Quizzes the player answered.
    fn answered(&self, slot: usize) -> PyResult<PyParameter> {
        self.fact(slot, gaanim_api::canvas::PlayerFact::Answered, None)
    }

    /// Quizzes the player answered right in a row.
    fn streak(&self, slot: usize) -> PyResult<PyParameter> {
        self.fact(slot, gaanim_api::canvas::PlayerFact::Streak, None)
    }

    /// 1 once the player answered `poll`, else 0.
    fn responded(&self, slot: usize, poll: PyRef<'_, PyPoll>) -> PyResult<PyParameter> {
        self.fact(slot, gaanim_api::canvas::PlayerFact::Responded, Some(poll))
    }

    /// 1 if the player chose `answer` on `poll`, else 0.
    fn chose(&self, slot: usize, poll: PyRef<'_, PyPoll>, answer: usize) -> PyResult<PyParameter> {
        self.fact(
            slot,
            gaanim_api::canvas::PlayerFact::Chose(answer),
            Some(poll),
        )
    }

    /// 1 if the player answered quiz `poll` right, else 0.
    fn right(&self, slot: usize, poll: PyRef<'_, PyPoll>) -> PyResult<PyParameter> {
        self.fact(slot, gaanim_api::canvas::PlayerFact::Right, Some(poll))
    }

    /// Points the player's answer to quiz `poll` earned.
    fn earned(&self, slot: usize, poll: PyRef<'_, PyPoll>) -> PyResult<PyParameter> {
        self.fact(slot, gaanim_api::canvas::PlayerFact::Earned, Some(poll))
    }

    /// Seconds the player took to answer quiz `poll`, 0 without an answer.
    fn answer_time(&self, slot: usize, poll: PyRef<'_, PyPoll>) -> PyResult<PyParameter> {
        self.fact(slot, gaanim_api::canvas::PlayerFact::Time, Some(poll))
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

    /// A condition for `scene.stop(until=...)`: at least `count` players
    /// joined.
    fn at_least(&self, count: u32) -> PyCondition {
        PyCondition {
            inner: self.inner.at_least(count),
        }
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
        format!("Audience(code={:?})", self.inner.code())
    }
}
