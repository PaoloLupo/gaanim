//! Audience polls, quizzes, the game's leaderboard and its audience:
//! [`SceneModel::poll`], [`SceneModel::quiz`], [`SceneModel::leaderboard`],
//! [`SceneModel::audience`].
//!
//! A poll gives the scene its data and leaves the presentation to it: the
//! session code and address, a QR code drawable, live values (votes, share,
//! percent, total, a quiz's seconds left) as parameters any reactive
//! drawable can follow, and bars whose length follows an answer. The
//! leaderboard gives the players' nicknames as live text, their scores as
//! parameters and bars. Outside a live presentation every value is its
//! preview, so previews and exports stay deterministic.

use std::sync::Arc;

use gaanim_animation::polls::{
    AUDIENCE_AGE_CAP, BarSource, BarSpec, LiveTextSource, PollBar, PollMeasure, PollSource,
    PollValue, TextAlign, leader_fraction,
};
use gaanim_animation::{SampledInterpolation, SampledProperty};
use gaanim_core::peniko::Color;
use qrcodegen::{QrCode, QrCodeEcc};

use super::SceneModel;
use super::drawable::DrawableHandle;
use super::ops::{Op, SharedCanvasState};
use super::types::{CurveElement, SpawnKind};
use super::visualization::{Parameter, parameter_in};

/// Most answers a poll takes: they must fit on a phone.
pub const MAX_POLL_OPTIONS: usize = 6;
/// Seconds a quiz may give to answer, as the relay accepts.
pub const QUIZ_TIME: std::ops::RangeInclusive<u32> = 5..=300;
/// Points a quiz may give for a correct answer, as the relay accepts.
pub const QUIZ_POINTS: std::ops::RangeInclusive<u32> = 100..=10_000;

/// Where a scene's polls take votes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PollSession {
    /// Relay address; `None` when none is set, and the QR code then points
    /// nowhere useful.
    pub relay: Option<String>,
    /// Six-character session code, the same for every poll of the scene.
    pub code: String,
}

impl PollSession {
    /// The address phones open.
    pub fn url(&self) -> String {
        let relay = self
            .relay
            .as_deref()
            .unwrap_or("https://relay-not-set.invalid");
        format!("{relay}/s/{}", self.code)
    }
}

/// Errors raised while authoring a poll.
#[derive(Debug, thiserror::Error, PartialEq)]
pub enum PollError {
    #[error("poll questions must not be empty")]
    EmptyQuestion,
    #[error("a poll needs between 2 and {MAX_POLL_OPTIONS} answers, got {count}")]
    OptionCount { count: usize },
    #[error("poll answers must not be empty")]
    EmptyOption,
    #[error("the poll answer {option:?} appears twice")]
    DuplicateOption { option: String },
    #[error("preview needs one count per answer: {expected}, got {got}")]
    PreviewLength { expected: usize, got: usize },
    #[error("answer {answer} does not exist; this poll has {count} answers")]
    UnknownAnswer { answer: usize, count: usize },
    #[error("the scene has no poll session")]
    NoSession,
    #[error("the poll is already closed")]
    AlreadyClosed,
    #[error("this poll is not a quiz; create it with scene.quiz")]
    NotQuiz,
    #[error("the quiz is already revealed")]
    AlreadyRevealed,
    #[error("{0}")]
    Invalid(String),
}

/// Scoring of a quiz as authored.
#[derive(Debug, Clone)]
pub(crate) struct QuizRecord {
    pub correct: usize,
    pub time: u32,
    pub points: u32,
    pub reveal: Option<(usize, f64)>,
}

/// A poll as authored: its window is `open` to `close` (or the end of the
/// segment where it opened), each a segment index and a local cursor.
#[derive(Debug, Clone)]
pub(crate) struct PollRecord {
    pub id: String,
    pub question: String,
    pub options: Vec<String>,
    pub preview: Arc<[u32]>,
    pub open: (usize, f64),
    pub close: Option<(usize, f64)>,
    pub quiz: Option<QuizRecord>,
}

/// Stable id on the relay: the poll's position and a hash of its text, so
/// editing a question starts it from zero while re-running keeps its votes.
fn poll_id(index: usize, question: &str, options: &[String], quiz: Option<&QuizRecord>) -> String {
    // FNV-1a: stable across platforms and releases, unlike `DefaultHasher`.
    let mut hash: u32 = 0x811c_9dc5;
    let scoring = quiz.map(|quiz| format!("{}/{}/{}", quiz.correct, quiz.time, quiz.points));
    let texts = std::iter::once(question)
        .chain(options.iter().map(String::as_str))
        .chain(scoring.as_deref());
    for text in texts {
        for byte in text.bytes().chain(std::iter::once(0)) {
            hash ^= u32::from(byte);
            hash = hash.wrapping_mul(0x0100_0193);
        }
    }
    let kind = if quiz.is_some() { "q" } else { "p" };
    format!("{kind}{index}-{hash:08x}")
}

/// The authoring clock at a segment's local cursor: segments before it
/// span their authored length.
fn authored_time(state: &super::ops::CanvasState, (segment, local): (usize, f64)) -> f64 {
    state.segments[..segment]
        .iter()
        .map(|segment| segment.cursor)
        .sum::<f64>()
        + local
}

impl SceneModel {
    /// Set the relay session the scene's polls take votes on. A host sets it
    /// before the script authors a poll.
    pub fn set_poll_session(&mut self, session: PollSession) {
        self.state
            .lock()
            .expect("canvas state poisoned")
            .poll_session = Some(session);
    }

    pub fn poll_session(&self) -> Option<PollSession> {
        self.state
            .lock()
            .expect("canvas state poisoned")
            .poll_session
            .clone()
    }

    /// Open a poll at the cursor. It takes votes while a presentation is
    /// between here and [`PollHandle::close`], or the end of this segment.
    /// `preview` counts stand in for votes outside a live presentation.
    pub fn poll(
        &mut self,
        question: impl Into<String>,
        options: impl IntoIterator<Item = impl Into<String>>,
        preview: Option<Vec<u32>>,
    ) -> Result<PollHandle, PollError> {
        self.open_poll(question.into(), options, preview, None)
    }

    /// Open a quiz at the cursor: a poll with a correct answer, `time`
    /// seconds to answer once and up to `points` for a fast correct answer.
    pub fn quiz(
        &mut self,
        question: impl Into<String>,
        options: impl IntoIterator<Item = impl Into<String>>,
        correct: usize,
        time: u32,
        points: u32,
        preview: Option<Vec<u32>>,
    ) -> Result<PollHandle, PollError> {
        if !QUIZ_TIME.contains(&time) {
            return Err(PollError::Invalid(format!(
                "a quiz gives between {} and {} seconds, got {time}",
                QUIZ_TIME.start(),
                QUIZ_TIME.end()
            )));
        }
        if !QUIZ_POINTS.contains(&points) {
            return Err(PollError::Invalid(format!(
                "a quiz gives between {} and {} points, got {points}",
                QUIZ_POINTS.start(),
                QUIZ_POINTS.end()
            )));
        }
        let quiz = QuizRecord {
            correct,
            time,
            points,
            reveal: None,
        };
        self.open_poll(question.into(), options, preview, Some(quiz))
    }

    fn open_poll(
        &mut self,
        question: String,
        options: impl IntoIterator<Item = impl Into<String>>,
        preview: Option<Vec<u32>>,
        quiz: Option<QuizRecord>,
    ) -> Result<PollHandle, PollError> {
        let question = question.trim().to_string();
        if question.is_empty() {
            return Err(PollError::EmptyQuestion);
        }
        let options: Vec<String> = options
            .into_iter()
            .map(|option| option.into().trim().to_string())
            .collect();
        if !(2..=MAX_POLL_OPTIONS).contains(&options.len()) {
            return Err(PollError::OptionCount {
                count: options.len(),
            });
        }
        if options.iter().any(String::is_empty) {
            return Err(PollError::EmptyOption);
        }
        for (index, option) in options.iter().enumerate() {
            if options[..index].contains(option) {
                return Err(PollError::DuplicateOption {
                    option: option.clone(),
                });
            }
        }
        if let Some(quiz) = &quiz
            && quiz.correct >= options.len()
        {
            return Err(PollError::UnknownAnswer {
                answer: quiz.correct,
                count: options.len(),
            });
        }
        let preview = preview.unwrap_or_else(|| vec![0; options.len()]);
        if preview.len() != options.len() {
            return Err(PollError::PreviewLength {
                expected: options.len(),
                got: preview.len(),
            });
        }
        let mut state = self.state.lock().expect("canvas state poisoned");
        if state.poll_session.is_none() {
            return Err(PollError::NoSession);
        }
        let index = state.polls.len();
        let open = (state.active_idx, state.active().cursor);
        state.polls.push(PollRecord {
            id: poll_id(index, &question, &options, quiz.as_ref()),
            question,
            options,
            preview: preview.into(),
            open,
            close: None,
            quiz,
        });
        Ok(PollHandle {
            index,
            state: self.state.clone(),
        })
    }

    /// The game's leaderboard: the players of every quiz in this scene,
    /// best first. `preview` names and scores stand in for players outside a
    /// live presentation. Text uses `color`, or the theme's foreground.
    pub fn leaderboard(&mut self, preview: Vec<(String, u64)>) -> LeaderboardHandle {
        let mut preview = preview;
        preview.sort_by(|a, b| b.1.cmp(&a.1));
        let color = self
            .theme_style
            .as_ref()
            .map_or(Color::WHITE, |theme| theme.palette.foreground);
        LeaderboardHandle {
            state: self.state.clone(),
            preview: preview
                .into_iter()
                .map(|(name, score)| (Arc::from(name.trim()), score))
                .collect(),
            color,
        }
    }

    /// The game's audience: the players in the order they joined, as data
    /// for the scene to arrange and animate. A scene that uses it asks each
    /// phone for a nickname as soon as it opens the page, so a lobby fills
    /// before the first question. `preview` nicknames stand in for players
    /// outside a live presentation. Text uses the theme's foreground.
    pub fn audience(&mut self, preview: Vec<String>) -> Result<AudienceHandle, PollError> {
        let color = self
            .theme_style
            .as_ref()
            .map_or(Color::WHITE, |theme| theme.palette.foreground);
        {
            let mut state = self.state.lock().expect("canvas state poisoned");
            if state.poll_session.is_none() {
                return Err(PollError::NoSession);
            }
            state.poll_lobby = true;
        }
        Ok(AudienceHandle {
            state: self.state.clone(),
            preview: preview
                .into_iter()
                .map(|name| Arc::from(name.trim()))
                .collect(),
            color,
        })
    }
}

/// A poll authored with [`SceneModel::poll`] or [`SceneModel::quiz`].
#[derive(Clone)]
pub struct PollHandle {
    index: usize,
    state: SharedCanvasState,
}

impl std::fmt::Debug for PollHandle {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("PollHandle")
            .field("index", &self.index)
            .finish_non_exhaustive()
    }
}

/// How [`PollHandle::bar`] and [`LeaderboardHandle::bar`] draw.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PollBarOptions {
    pub length: f64,
    pub thickness: f64,
    pub radius: f64,
    pub direction: gaanim_animation::polls::BarDirection,
    pub scale: gaanim_animation::polls::BarScale,
}

impl PollBarOptions {
    fn spec(&self) -> Result<BarSpec, PollError> {
        for (name, value) in [("length", self.length), ("thickness", self.thickness)] {
            if !(value.is_finite() && value > 0.0) {
                return Err(PollError::Invalid(format!(
                    "bar {name} must be positive, got {value}"
                )));
            }
        }
        if !(self.radius.is_finite() && self.radius >= 0.0) {
            return Err(PollError::Invalid(format!(
                "bar radius must not be negative, got {}",
                self.radius
            )));
        }
        Ok(BarSpec {
            length: self.length,
            thickness: self.thickness,
            radius: self.radius,
            direction: self.direction,
            scale: self.scale,
        })
    }
}

/// Spawn the rectangle of a bar and make it follow `source`.
fn spawn_bar(
    state: &SharedCanvasState,
    spec: BarSpec,
    source: BarSource,
    preview: f64,
) -> DrawableHandle {
    let full = spec.full();
    let handle =
        super::canvas_impl::spawn_in(state, SpawnKind::Rect(full.width(), full.height()), true);
    state
        .lock()
        .expect("canvas state poisoned")
        .active_mut()
        .ops
        .push(Op::AttachPollBar {
            target: handle.id,
            bar: PollBar {
                source,
                spec,
                preview,
                last: None,
            },
        });
    handle
}

/// A parameter whose value follows `source`: constant `preview` samples, or
/// a series such as a countdown, replaced by the live value while
/// presenting.
fn live_parameter(
    state: &SharedCanvasState,
    source: PollSource,
    times: Vec<f64>,
    values: Vec<f64>,
    interpolation: SampledInterpolation,
) -> Result<Parameter, PollError> {
    let parameter =
        parameter_in(state, values[0]).map_err(|error| PollError::Invalid(error.to_string()))?;
    parameter
        .drawable()
        .drive_from_samples(
            times,
            values.clone(),
            SampledProperty::Signal,
            interpolation,
            1.0,
            0.0,
        )
        .map_err(|_| PollError::Invalid("could not drive the poll value".into()))?;
    state
        .lock()
        .expect("canvas state poisoned")
        .active_mut()
        .ops
        .push(Op::AttachPollValue {
            target: parameter.drawable().id,
            value: PollValue {
                source,
                preview: values.into(),
            },
        });
    Ok(parameter)
}

impl PollHandle {
    fn record(&self) -> PollRecord {
        self.state.lock().expect("canvas state poisoned").polls[self.index].clone()
    }

    fn session(&self) -> PollSession {
        self.state
            .lock()
            .expect("canvas state poisoned")
            .poll_session
            .clone()
            .expect("a poll exists only with a session")
    }

    pub fn id(&self) -> String {
        self.record().id
    }

    pub fn question(&self) -> String {
        self.record().question
    }

    pub fn options(&self) -> Vec<String> {
        self.record().options
    }

    pub fn preview(&self) -> Vec<u32> {
        self.record().preview.to_vec()
    }

    /// The correct answer of a quiz; `None` for a poll.
    pub fn correct(&self) -> Option<usize> {
        self.record().quiz.map(|quiz| quiz.correct)
    }

    /// Seconds a quiz gives to answer; `None` for a poll.
    pub fn time(&self) -> Option<u32> {
        self.record().quiz.map(|quiz| quiz.time)
    }

    /// The session code phones type.
    pub fn code(&self) -> String {
        self.session().code
    }

    /// The address the QR code opens.
    pub fn url(&self) -> String {
        self.session().url()
    }

    fn check_answer(&self, answer: usize) -> Result<(), PollError> {
        let count = self.record().options.len();
        if answer < count {
            Ok(())
        } else {
            Err(PollError::UnknownAnswer { answer, count })
        }
    }

    fn value(&self, measure: PollMeasure) -> Result<Parameter, PollError> {
        let record = self.record();
        let source = PollSource::Poll {
            poll: record.id.as_str().into(),
            answers: record.options.len(),
            measure,
        };
        live_parameter(
            &self.state,
            source,
            vec![0.0],
            vec![measure.value(&record.preview)],
            SampledInterpolation::Step,
        )
    }

    /// Votes for `answer`, live while presenting.
    pub fn votes(&self, answer: usize) -> Result<Parameter, PollError> {
        self.check_answer(answer)?;
        self.value(PollMeasure::Votes(answer))
    }

    /// `answer`'s fraction of all votes, from 0 to 1.
    pub fn share(&self, answer: usize) -> Result<Parameter, PollError> {
        self.check_answer(answer)?;
        self.value(PollMeasure::Share(answer))
    }

    /// `answer`'s share of all votes, from 0 to 100.
    pub fn percent(&self, answer: usize) -> Result<Parameter, PollError> {
        self.check_answer(answer)?;
        self.value(PollMeasure::Percent(answer))
    }

    /// Votes for every answer.
    pub fn total(&self) -> Result<Parameter, PollError> {
        self.value(PollMeasure::Total)
    }

    /// Seconds left to answer a quiz. In previews and exports it counts
    /// down from where the quiz opens; while presenting it follows the
    /// relay's clock.
    pub fn remaining(&self) -> Result<Parameter, PollError> {
        let (record, elapsed) = {
            let state = self.state.lock().expect("canvas state poisoned");
            let record = state.polls[self.index].clone();
            let now = authored_time(&state, (state.active_idx, state.active().cursor));
            let elapsed = (now - authored_time(&state, record.open)).max(0.0);
            (record, elapsed)
        };
        let quiz = record.quiz.as_ref().ok_or(PollError::NotQuiz)?;
        let time = f64::from(quiz.time);
        let left = (time - elapsed).max(0.0);
        let (times, values) = if left > 0.0 {
            (vec![0.0, left], vec![left, 0.0])
        } else {
            (vec![0.0], vec![0.0])
        };
        live_parameter(
            &self.state,
            PollSource::Poll {
                poll: record.id.as_str().into(),
                answers: record.options.len(),
                measure: PollMeasure::Remaining { time },
            },
            times,
            values,
            SampledInterpolation::Linear,
        )
    }

    /// A bar whose length follows `answer`. Its bounds are the full-length
    /// box, centered on its position, and it grows from its start edge.
    pub fn bar(&self, answer: usize, options: PollBarOptions) -> Result<DrawableHandle, PollError> {
        self.check_answer(answer)?;
        let spec = options.spec()?;
        let record = self.record();
        let preview = spec.fraction(answer, &record.preview);
        Ok(spawn_bar(
            &self.state,
            spec,
            BarSource::Answer {
                poll: record.id.as_str().into(),
                answer,
                answers: record.options.len(),
            },
            preview,
        ))
    }

    /// The QR code of [`Self::url`], `size` scene units on a side, as one
    /// path of its dark modules centered on the origin, filled black. Put
    /// it on a light background with a margin of a few modules so phones
    /// read it.
    pub fn qr(&self, size: f64) -> Result<DrawableHandle, PollError> {
        if !(size.is_finite() && size > 0.0) {
            return Err(PollError::Invalid(format!(
                "QR size must be positive, got {size}"
            )));
        }
        let elements = qr_elements(&self.url(), size)?;
        let handle = super::canvas_impl::spawn_in(&self.state, SpawnKind::Curve(elements), true);
        Ok(handle.fill(Color::BLACK).no_stroke())
    }

    /// Stop taking votes at the cursor instead of at the end of the
    /// segment where the poll opened.
    pub fn close(&self) -> Result<(), PollError> {
        let mut state = self.state.lock().expect("canvas state poisoned");
        let at = (state.active_idx, state.active().cursor);
        let record = &mut state.polls[self.index];
        if record.close.is_some() {
            return Err(PollError::AlreadyClosed);
        }
        record.close = Some(at);
        Ok(())
    }

    /// Reveal a quiz's answer at the cursor: from here a presentation tells
    /// every phone whether it was right, and takes no more answers.
    pub fn reveal(&self) -> Result<(), PollError> {
        let mut state = self.state.lock().expect("canvas state poisoned");
        let at = (state.active_idx, state.active().cursor);
        let quiz = state.polls[self.index]
            .quiz
            .as_mut()
            .ok_or(PollError::NotQuiz)?;
        if quiz.reveal.is_some() {
            return Err(PollError::AlreadyRevealed);
        }
        quiz.reveal = Some(at);
        Ok(())
    }
}

/// The game's leaderboard, from [`SceneModel::leaderboard`].
#[derive(Clone)]
pub struct LeaderboardHandle {
    state: SharedCanvasState,
    preview: Vec<(Arc<str>, u64)>,
    color: Color,
}

impl std::fmt::Debug for LeaderboardHandle {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("LeaderboardHandle")
            .field("preview", &self.preview)
            .finish_non_exhaustive()
    }
}

/// How [`LeaderboardHandle::name`] sets its text.
#[derive(Debug, Clone, PartialEq)]
pub struct LiveTextOptions {
    /// Font size in scene units; `None` uses the theme's body size.
    pub size: Option<f64>,
    pub weight: Option<u16>,
    /// Font family; `None` uses the theme's body font.
    pub font: Option<String>,
    pub align: TextAlign,
}

impl LeaderboardHandle {
    fn preview_score(&self, rank: usize) -> u64 {
        self.preview.get(rank).map_or(0, |(_, score)| *score)
    }

    /// Nicknames and scores shown outside a live presentation, best first.
    pub fn preview(&self) -> Vec<(String, u64)> {
        self.preview
            .iter()
            .map(|(name, score)| (name.to_string(), *score))
            .collect()
    }

    /// The nickname at `rank` (0 for the leader) as live text; empty when
    /// fewer players joined.
    pub fn name(&self, rank: usize, options: LiveTextOptions) -> Result<DrawableHandle, PollError> {
        let preview = self
            .preview
            .get(rank)
            .map_or_else(|| Arc::from(""), |(name, _)| name.clone());
        live_text(
            &self.state,
            self.color,
            LiveTextSource::LeaderName { rank },
            preview,
            options,
        )
    }

    /// The score of the player at `rank`, live while presenting.
    pub fn points(&self, rank: usize) -> Result<Parameter, PollError> {
        live_parameter(
            &self.state,
            PollSource::LeaderScore { rank },
            vec![0.0],
            vec![self.preview_score(rank) as f64],
            SampledInterpolation::Step,
        )
    }

    /// How many players joined.
    pub fn players(&self) -> Result<Parameter, PollError> {
        live_parameter(
            &self.state,
            PollSource::Players,
            vec![0.0],
            vec![self.preview.len() as f64],
            SampledInterpolation::Step,
        )
    }

    /// A bar whose length is the score at `rank` against the leader's.
    pub fn bar(&self, rank: usize, options: PollBarOptions) -> Result<DrawableHandle, PollError> {
        let spec = options.spec()?;
        let preview = leader_fraction(self.preview.iter().map(|(_, score)| *score), rank);
        Ok(spawn_bar(
            &self.state,
            spec,
            BarSource::Leader { rank },
            preview,
        ))
    }
}

/// The game's audience, from [`SceneModel::audience`]: each player has a
/// slot, its place in joining order (0 for the first to join).
#[derive(Clone)]
pub struct AudienceHandle {
    state: SharedCanvasState,
    preview: Vec<Arc<str>>,
    color: Color,
}

impl std::fmt::Debug for AudienceHandle {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("AudienceHandle")
            .field("preview", &self.preview)
            .finish_non_exhaustive()
    }
}

impl AudienceHandle {
    /// Nicknames shown outside a live presentation, in joining order.
    pub fn preview(&self) -> Vec<String> {
        self.preview.iter().map(|name| name.to_string()).collect()
    }

    fn session(&self) -> PollSession {
        self.state
            .lock()
            .expect("canvas state poisoned")
            .poll_session
            .clone()
            .expect("an audience exists only with a session")
    }

    /// The session code phones type.
    pub fn code(&self) -> String {
        self.session().code
    }

    /// The address phones open to join.
    pub fn url(&self) -> String {
        self.session().url()
    }

    /// The QR code of [`Self::url`], as [`PollHandle::qr`] draws it.
    pub fn qr(&self, size: f64) -> Result<DrawableHandle, PollError> {
        if !(size.is_finite() && size > 0.0) {
            return Err(PollError::Invalid(format!(
                "QR size must be positive, got {size}"
            )));
        }
        let elements = qr_elements(&self.url(), size)?;
        let handle = super::canvas_impl::spawn_in(&self.state, SpawnKind::Curve(elements), true);
        Ok(handle.fill(Color::BLACK).no_stroke())
    }

    /// The nickname in `slot` as live text; empty until that many players
    /// joined.
    pub fn name(&self, slot: usize, options: LiveTextOptions) -> Result<DrawableHandle, PollError> {
        let preview = self
            .preview
            .get(slot)
            .cloned()
            .unwrap_or_else(|| Arc::from(""));
        live_text(
            &self.state,
            self.color,
            LiveTextSource::AudienceName { slot },
            preview,
            options,
        )
    }

    /// How many players joined.
    pub fn count(&self) -> Result<Parameter, PollError> {
        live_parameter(
            &self.state,
            PollSource::Players,
            vec![0.0],
            vec![self.preview.len() as f64],
            SampledInterpolation::Step,
        )
    }

    /// 1 once a player took `slot`, else 0: drive a slot's visibility.
    pub fn joined(&self, slot: usize) -> Result<Parameter, PollError> {
        let preview = if slot < self.preview.len() { 1.0 } else { 0.0 };
        live_parameter(
            &self.state,
            PollSource::AudienceJoined { slot },
            vec![0.0],
            vec![preview],
            SampledInterpolation::Step,
        )
    }

    /// Seconds since the player in `slot` joined, up to
    /// [`AUDIENCE_AGE_CAP`]; 0 while the slot is empty. Drive an entrance
    /// with it. Preview players count as long settled.
    pub fn age(&self, slot: usize) -> Result<Parameter, PollError> {
        let preview = if slot < self.preview.len() {
            AUDIENCE_AGE_CAP
        } else {
            0.0
        };
        live_parameter(
            &self.state,
            PollSource::AudienceAge { slot },
            vec![0.0],
            vec![preview],
            SampledInterpolation::Step,
        )
    }
}

/// Live text following `source`, `preview` outside a live presentation.
fn live_text(
    state: &SharedCanvasState,
    color: Color,
    source: LiveTextSource,
    preview: Arc<str>,
    options: LiveTextOptions,
) -> Result<DrawableHandle, PollError> {
    if let Some(size) = options.size
        && !(size.is_finite() && size > 0.0)
    {
        return Err(PollError::Invalid(format!(
            "text size must be positive, got {size}"
        )));
    }
    let handle = super::canvas_impl::spawn_in(state, SpawnKind::Curve(Vec::new()), true);
    let handle = handle.fill(color).no_stroke();
    state
        .lock()
        .expect("canvas state poisoned")
        .active_mut()
        .ops
        .push(Op::AttachLiveText {
            target: handle.id,
            source,
            preview,
            options,
        });
    Ok(handle)
}

/// The dark modules of the QR code of `text`, merged into one rectangle per
/// run of each row, `size` units wide and centered on the origin.
fn qr_elements(text: &str, size: f64) -> Result<Vec<CurveElement>, PollError> {
    let qr = QrCode::encode_text(text, QrCodeEcc::Medium)
        .map_err(|_| PollError::Invalid("the poll address is too long for a QR code".into()))?;
    let modules = qr.size();
    let cell = size / f64::from(modules);
    let half = size / 2.0;
    let mut elements = Vec::new();
    let mut rect = |x0: i32, x1: i32, y: i32| {
        let left = -half + f64::from(x0) * cell;
        let right = -half + f64::from(x1) * cell;
        let top = half - f64::from(y) * cell;
        let bottom = top - cell;
        for (index, point) in [(left, top), (right, top), (right, bottom), (left, bottom)]
            .into_iter()
            .enumerate()
        {
            elements.push(if index == 0 {
                CurveElement::Move {
                    to: point,
                    relative: false,
                }
            } else {
                CurveElement::Line {
                    to: point,
                    relative: false,
                }
            });
        }
        elements.push(CurveElement::Close { smooth: false });
    };
    for y in 0..modules {
        let mut start = None;
        for x in 0..=modules {
            match (start, x < modules && qr.get_module(x, y)) {
                (None, true) => start = Some(x),
                (Some(from), false) => {
                    rect(from, x, y);
                    start = None;
                }
                _ => {}
            }
        }
    }
    Ok(elements)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scene() -> SceneModel {
        let mut scene = SceneModel::new(640, 360);
        scene.set_poll_session(PollSession {
            relay: Some("https://relay.example.dev".into()),
            code: "ABC234".into(),
        });
        scene
    }

    #[test]
    fn polls_validate_their_question_answers_and_preview() {
        let mut scene = scene();
        let error = |result: Result<PollHandle, PollError>| result.unwrap_err();
        assert_eq!(
            error(scene.poll(" ", ["A", "B"], None)),
            PollError::EmptyQuestion
        );
        assert_eq!(
            error(scene.poll("Q", ["A"], None)),
            PollError::OptionCount { count: 1 }
        );
        assert_eq!(
            error(scene.poll("Q", ["1", "2", "3", "4", "5", "6", "7"], None)),
            PollError::OptionCount { count: 7 }
        );
        assert_eq!(
            error(scene.poll("Q", ["A", " "], None)),
            PollError::EmptyOption
        );
        assert!(matches!(
            error(scene.poll("Q", ["A", "A "], None)),
            PollError::DuplicateOption { .. }
        ));
        assert_eq!(
            error(scene.poll("Q", ["A", "B"], Some(vec![1]))),
            PollError::PreviewLength {
                expected: 2,
                got: 1
            }
        );
        let mut bare = SceneModel::new(640, 360);
        assert_eq!(
            error(bare.poll("Q", ["A", "B"], None)),
            PollError::NoSession
        );
    }

    #[test]
    fn quizzes_validate_their_answer_time_and_points() {
        let mut scene = scene();
        assert_eq!(
            scene.quiz("Q", ["A", "B"], 2, 20, 1000, None).unwrap_err(),
            PollError::UnknownAnswer {
                answer: 2,
                count: 2
            }
        );
        assert!(scene.quiz("Q", ["A", "B"], 0, 2, 1000, None).is_err());
        assert!(scene.quiz("Q", ["A", "B"], 0, 20, 50, None).is_err());
        let quiz = scene.quiz("Q", ["A", "B"], 1, 20, 1000, None).unwrap();
        assert_eq!((quiz.correct(), quiz.time()), (Some(1), Some(20)));
        assert!(quiz.id().starts_with("q0-"));
        assert!(quiz.reveal().is_ok());
        assert_eq!(quiz.reveal(), Err(PollError::AlreadyRevealed));
        let poll = scene.poll("Plain", ["A", "B"], None).unwrap();
        assert_eq!(poll.correct(), None);
        assert_eq!(poll.reveal(), Err(PollError::NotQuiz));
        assert!(matches!(poll.remaining(), Err(PollError::NotQuiz)));
    }

    #[test]
    fn poll_ids_are_stable_and_follow_the_text_and_scoring() {
        let options = ["A".to_string(), "B".to_string()];
        let quiz = |correct| QuizRecord {
            correct,
            time: 20,
            points: 1000,
            reveal: None,
        };
        assert_eq!(
            poll_id(0, "Q", &options, None),
            poll_id(0, "Q", &options, None)
        );
        assert_ne!(
            poll_id(0, "Q", &options, None),
            poll_id(0, "Q?", &options, None)
        );
        assert_ne!(
            poll_id(0, "Q", &options, None),
            poll_id(1, "Q", &options, None)
        );
        assert_ne!(
            poll_id(0, "Q", &options, Some(&quiz(0))),
            poll_id(0, "Q", &options, Some(&quiz(1)))
        );
        assert!(poll_id(3, "¿Cuál?", &options, None).starts_with("p3-"));
    }

    #[test]
    fn a_poll_reports_its_session_and_rejects_unknown_answers() {
        let mut scene = scene();
        let poll = scene
            .poll(" ¿Cuál? ", ["x²", "2ˣ"], Some(vec![3, 1]))
            .unwrap();
        assert_eq!(poll.question(), "¿Cuál?");
        assert_eq!(poll.code(), "ABC234");
        assert_eq!(poll.url(), "https://relay.example.dev/s/ABC234");
        assert_eq!(poll.preview(), [3, 1]);
        assert!(matches!(
            poll.votes(2),
            Err(PollError::UnknownAnswer {
                answer: 2,
                count: 2
            })
        ));
        assert!(poll.share(1).is_ok());
        assert!(poll.percent(0).is_ok());
        assert!(poll.close().is_ok());
        assert_eq!(poll.close(), Err(PollError::AlreadyClosed));
    }

    #[test]
    fn the_leaderboard_sorts_its_preview_and_validates_text() {
        let mut scene = scene();
        let board = scene.leaderboard(vec![("Beto".into(), 300), (" Ana ".into(), 900)]);
        assert_eq!(
            board.preview(),
            [("Ana".to_string(), 900), ("Beto".to_string(), 300)]
        );
        let options = |size| LiveTextOptions {
            size,
            weight: None,
            font: None,
            align: TextAlign::Left,
        };
        assert!(board.name(0, options(Some(0.5))).is_ok());
        assert!(board.name(7, options(None)).is_ok());
        assert!(board.name(0, options(Some(-1.0))).is_err());
        assert!(board.points(1).is_ok());
        assert!(board.players().is_ok());
    }

    #[test]
    fn the_qr_code_fills_its_size_centered_on_the_origin() {
        let elements = qr_elements("https://relay.example.dev/s/ABC234", 4.0).unwrap();
        let points: Vec<(f64, f64)> = elements
            .iter()
            .filter_map(|element| match element {
                CurveElement::Move { to, .. } | CurveElement::Line { to, .. } => Some(*to),
                _ => None,
            })
            .collect();
        let min_x = points
            .iter()
            .map(|point| point.0)
            .fold(f64::INFINITY, f64::min);
        let max_y = points
            .iter()
            .map(|point| point.1)
            .fold(f64::NEG_INFINITY, f64::max);
        // Finder patterns sit in three corners, so the code touches the edges.
        assert!((min_x + 2.0).abs() < 1e-9);
        assert!((max_y - 2.0).abs() < 1e-9);
    }
}
