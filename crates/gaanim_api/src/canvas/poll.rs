//! Audience polls: [`SceneModel::poll`].
//!
//! A poll gives the scene its data and leaves the presentation to it: the
//! session code and address, a QR code drawable, live values (votes, share,
//! total) as parameters any reactive drawable can follow, and bars whose
//! length follows an answer. Outside a live presentation the values are the
//! poll's preview counts, so previews and exports stay deterministic.

use std::sync::Arc;

use gaanim_animation::polls::{BarSpec, PollBar, PollMeasure, PollValue};
use gaanim_animation::{SampledInterpolation, SampledProperty};
use qrcodegen::{QrCode, QrCodeEcc};

use super::SceneModel;
use super::drawable::DrawableHandle;
use super::ops::{Op, SharedCanvasState};
use super::types::{CurveElement, SpawnKind};
use super::visualization::{Parameter, parameter_in};

/// Most answers a poll takes: they must fit on a phone.
pub const MAX_POLL_OPTIONS: usize = 6;

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
    #[error("{0}")]
    Invalid(String),
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
}

/// Stable id on the relay: the poll's position and a hash of its text, so
/// editing a question starts it from zero while re-running keeps its votes.
fn poll_id(index: usize, question: &str, options: &[String]) -> String {
    // FNV-1a: stable across platforms and releases, unlike `DefaultHasher`.
    let mut hash: u32 = 0x811c_9dc5;
    for text in std::iter::once(question).chain(options.iter().map(String::as_str)) {
        for byte in text.bytes().chain(std::iter::once(0)) {
            hash ^= u32::from(byte);
            hash = hash.wrapping_mul(0x0100_0193);
        }
    }
    format!("p{index}-{hash:08x}")
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
        let question = question.into().trim().to_string();
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
            id: poll_id(index, &question, &options),
            question,
            options,
            preview: preview.into(),
            open,
            close: None,
        });
        Ok(PollHandle {
            index,
            state: self.state.clone(),
        })
    }
}

/// A poll authored with [`SceneModel::poll`].
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

/// How [`PollHandle::bar`] draws.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PollBarOptions {
    pub length: f64,
    pub thickness: f64,
    pub radius: f64,
    pub direction: gaanim_animation::polls::BarDirection,
    pub scale: gaanim_animation::polls::BarScale,
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
        let initial = measure.value(&record.preview);
        let parameter = parameter_in(&self.state, initial)
            .map_err(|error| PollError::Invalid(error.to_string()))?;
        parameter
            .drawable()
            .drive_from_samples(
                vec![0.0],
                vec![initial],
                SampledProperty::Signal,
                SampledInterpolation::Step,
                1.0,
                0.0,
            )
            .map_err(|_| PollError::Invalid("could not drive the poll value".into()))?;
        self.state
            .lock()
            .expect("canvas state poisoned")
            .active_mut()
            .ops
            .push(Op::AttachPollValue {
                target: parameter.drawable().id,
                value: PollValue {
                    poll: record.id.into(),
                    measure,
                    preview: record.preview,
                },
            });
        Ok(parameter)
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

    /// Votes for every answer.
    pub fn total(&self) -> Result<Parameter, PollError> {
        self.value(PollMeasure::Total)
    }

    /// A bar whose length follows `answer`. Its bounds are the full-length
    /// box, centered on its position, and it grows from its start edge.
    pub fn bar(&self, answer: usize, options: PollBarOptions) -> Result<DrawableHandle, PollError> {
        self.check_answer(answer)?;
        for (name, value) in [("length", options.length), ("thickness", options.thickness)] {
            if !(value.is_finite() && value > 0.0) {
                return Err(PollError::Invalid(format!(
                    "bar {name} must be positive, got {value}"
                )));
            }
        }
        if !(options.radius.is_finite() && options.radius >= 0.0) {
            return Err(PollError::Invalid(format!(
                "bar radius must not be negative, got {}",
                options.radius
            )));
        }
        let record = self.record();
        let spec = BarSpec {
            length: options.length,
            thickness: options.thickness,
            radius: options.radius,
            direction: options.direction,
            scale: options.scale,
        };
        let full = spec.full();
        let handle = super::canvas_impl::spawn_in(
            &self.state,
            SpawnKind::Rect(full.width(), full.height()),
            true,
        );
        self.state
            .lock()
            .expect("canvas state poisoned")
            .active_mut()
            .ops
            .push(Op::AttachPollBar {
                target: handle.id,
                bar: PollBar {
                    poll: record.id.into(),
                    answer,
                    preview: record.preview,
                    spec,
                    last: None,
                },
            });
        Ok(handle)
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
        Ok(handle.fill(gaanim_core::peniko::Color::BLACK).no_stroke())
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
    fn poll_ids_are_stable_and_follow_the_text() {
        let options = ["A".to_string(), "B".to_string()];
        assert_eq!(poll_id(0, "Q", &options), poll_id(0, "Q", &options));
        assert_ne!(poll_id(0, "Q", &options), poll_id(0, "Q?", &options));
        assert_ne!(poll_id(0, "Q", &options), poll_id(1, "Q", &options));
        assert!(poll_id(3, "¿Cuál?", &options).starts_with("p3-"));
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
        assert!(poll.close().is_ok());
        assert_eq!(poll.close(), Err(PollError::AlreadyClosed));
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
