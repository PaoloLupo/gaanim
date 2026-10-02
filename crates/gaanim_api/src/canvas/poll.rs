//! Audience polls, quizzes, the game's leaderboard and its audience:
//! [`SceneModel::poll`], [`SceneModel::quiz`], [`SceneModel::leaderboard`],
//! [`SceneModel::audience`].
//!
//! A poll gives the scene its data and leaves the presentation to it: the
//! session code and address, a QR code drawable, live values (votes, share,
//! percent, total, a quiz's seconds left) as parameters any reactive
//! drawable can follow, and bars whose length follows an answer. The
//! leaderboard gives the players' nicknames as live text, their scores as
//! parameters and bars. Outside a live presentation every value comes from
//! the scene's rehearsal ([`SceneModel::rehearsal`]), a made-up audience
//! that plays the same way on every preview and export.

use std::sync::Arc;

use gaanim_animation::polls::{
    BarSource, BarSpec, LiveTextSource, PollBar, PollMeasure, PollSource, PollValue, TeamMeasure,
    TextAlign, leader_fraction,
};
use gaanim_animation::rehearsal::{Lean, MAX_PLAYERS, RehearsalSpec};
use gaanim_animation::{SampledInterpolation, SampledProperty};
use gaanim_core::peniko::Color;
pub use gaanim_timeline::timeline::GateCondition;
pub use gaanim_timeline::timeline::PollImage;
use qrcodegen::{QrCode, QrCodeEcc};

use super::SceneModel;
use super::drawable::DrawableHandle;
use super::ops::{Op, SharedCanvasState};
use super::types::{CurveControl, CurveElement, SpawnKind};
use super::visualization::{Parameter, parameter_in};

/// Most teams a game has: their buttons must fit on a phone.
pub const MAX_TEAMS: usize = 6;
/// Longest label of what a roster asks, as the relay accepts it.
pub const MAX_ASK: usize = 40;
/// Colors teams get when the scene gives none, in order.
pub const TEAM_COLORS: [&str; MAX_TEAMS] = [
    "#ff4f8b", "#2fb8ff", "#ff9f1c", "#2ed47a", "#9b5de5", "#00c2c7",
];

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
    #[error("rehearse needs one weight per answer: {expected}, got {got}")]
    LeanLength { expected: usize, got: usize },
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

/// How far past its `reveal()` a quiz counts as revealed: the playhead
/// resting on a stop the reveal shares has not revealed it yet.
pub const REVEAL_AFTER: f64 = 1e-4;

/// Scoring of a quiz as authored.
#[derive(Debug, Clone)]
pub(crate) struct QuizRecord {
    /// The right answers, in order.
    pub correct: Vec<usize>,
    pub time: u32,
    pub points: u32,
    pub reveal: Option<(usize, f64)>,
    /// The parameters [`PollHandle::revealed`] gave, set to 1 on reveal.
    pub revealed: Vec<Parameter>,
}

/// A poll as authored: its window is `open` to `close` (or the end of the
/// segment where it opened), each a segment index and a local cursor.
#[derive(Debug, Clone)]
pub(crate) struct PollRecord {
    pub id: String,
    pub question: String,
    pub options: Vec<String>,
    /// How the rehearsal answers it.
    pub lean: Lean,
    pub open: (usize, f64),
    pub close: Option<(usize, f64)>,
    pub quiz: Option<QuizRecord>,
    pub multiple: bool,
    pub image: Option<PollImage>,
}

/// How a poll asks: several answers at once, a picture.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PollStyle {
    /// Players may choose several answers.
    pub multiple: bool,
    /// A picture phones show above the question.
    pub image: Option<PollImage>,
}

/// The colors of a poll's answers on phones, in order (the sixth answer
/// is the last).
pub const ANSWER_COLORS: [&str; MAX_POLL_OPTIONS] = [
    "#d63a50", "#2f6fd0", "#a86f00", "#23824a", "#7a48c7", "#137f89",
];

/// The shapes of a poll's answers on phones, as SVG paths in a 24 unit box
/// (y down): triangle, diamond, circle, square, star and hexagon.
const ANSWER_SHAPES: [&str; MAX_POLL_OPTIONS] = [
    "M12 3l9.5 17h-19z",
    "M12 2l9 10-9 10-9-10z",
    "M21.5 12a9.5 9.5 0 1 1-19 0a9.5 9.5 0 1 1 19 0z",
    "M5 3h14a2 2 0 0 1 2 2v14a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2z",
    "M12 2.5l2.9 6.1 6.6.8-4.9 4.6 1.3 6.6L12 17.3l-5.9 3.3 1.3-6.6L2.5 9.4l6.6-.8z",
    "M7 3h10l5 9-5 9H7l-5-9z",
];

/// The shape of answer `answer` on phones, `size` units tall and centered
/// on the origin, y up.
fn answer_shape(answer: usize, size: f64) -> Vec<CurveElement> {
    use gaanim_core::kurbo::{BezPath, PathEl};
    let path = BezPath::from_svg(ANSWER_SHAPES[answer % ANSWER_SHAPES.len()]).unwrap_or_default();
    let scale = size / 20.0;
    let point = |p: gaanim_core::kurbo::Point| ((p.x - 12.0) * scale, (12.0 - p.y) * scale);
    path.elements()
        .iter()
        .flat_map(|element| match *element {
            PathEl::MoveTo(p) => vec![CurveElement::Move {
                to: point(p),
                relative: false,
            }],
            PathEl::LineTo(p) => vec![CurveElement::Line {
                to: point(p),
                relative: false,
            }],
            PathEl::QuadTo(c, p) => vec![CurveElement::Quad {
                control: CurveControl::Point(point(c)),
                to: point(p),
                relative: false,
            }],
            PathEl::CurveTo(a, b, p) => vec![CurveElement::Cubic {
                control_start: CurveControl::Point(point(a)),
                control_end: CurveControl::Point(point(b)),
                to: point(p),
                relative: false,
            }],
            PathEl::ClosePath => vec![CurveElement::Close { smooth: false }],
        })
        .collect()
}

/// Longest side of a poll's picture on phones, in pixels.
pub const POLL_IMAGE_SIZE: u32 = 1024;

/// Read the picture at `path` for phones: scaled to fit [`POLL_IMAGE_SIZE`],
/// as a JPEG (or a PNG when it is transparent).
pub fn poll_image(path: &std::path::Path) -> Result<PollImage, PollError> {
    let image = image::open(path).map_err(|error| {
        PollError::Invalid(format!(
            "could not read the image {}: {error}",
            path.display()
        ))
    })?;
    let image = if image.width().max(image.height()) > POLL_IMAGE_SIZE {
        image.resize(
            POLL_IMAGE_SIZE,
            POLL_IMAGE_SIZE,
            image::imageops::FilterType::Triangle,
        )
    } else {
        image
    };
    let transparent =
        image.color().has_alpha() && image.to_rgba8().pixels().any(|pixel| pixel.0[3] < 255);
    let mut bytes = Vec::new();
    let mime = if transparent {
        image
            .write_to(
                &mut std::io::Cursor::new(&mut bytes),
                image::ImageFormat::Png,
            )
            .map_err(|error| PollError::Invalid(error.to_string()))?;
        "image/png"
    } else {
        image::codecs::jpeg::JpegEncoder::new_with_quality(&mut bytes, 82)
            .encode_image(&image.to_rgb8())
            .map_err(|error| PollError::Invalid(error.to_string()))?;
        "image/jpeg"
    };
    // FNV-1a 64: stable everywhere.
    let hash = bytes.iter().fold(0xcbf2_9ce4_8422_2325u64, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x0000_0100_0000_01b3)
    });
    Ok(PollImage {
        hash: format!("{hash:016x}"),
        mime: mime.into(),
        bytes: bytes.into(),
    })
}

/// Stable id on the relay: the poll's position and a hash of its text, so
/// editing a question starts it from zero while re-running keeps its votes.
fn poll_id(
    index: usize,
    question: &str,
    options: &[String],
    quiz: Option<&QuizRecord>,
    style: &PollStyle,
) -> String {
    // FNV-1a: stable across platforms and releases, unlike `DefaultHasher`.
    let mut hash: u32 = 0x811c_9dc5;
    // A single right answer keeps the ids polls had before multiple choice.
    let scoring = quiz.map(|quiz| {
        let correct = quiz
            .correct
            .iter()
            .map(usize::to_string)
            .collect::<Vec<_>>()
            .join(",");
        format!("{correct}/{}/{}", quiz.time, quiz.points)
    });
    let manner = (style.multiple || style.image.is_some()).then(|| {
        format!(
            "{}/{}",
            style.multiple,
            style.image.as_ref().map_or("", |image| image.hash.as_str())
        )
    });
    let texts = std::iter::once(question)
        .chain(options.iter().map(String::as_str))
        .chain(scoring.as_deref())
        .chain(manner.as_deref());
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

    /// Describe the made-up audience that plays the scene's polls outside a
    /// live presentation: previews, exports and snapshots show it joining,
    /// answering and climbing the leaderboard, the same every time.
    pub fn rehearsal(&mut self, spec: RehearsalSpec) -> Result<(), PollError> {
        let names: Vec<String> = spec
            .names
            .iter()
            .map(|name| name.trim().to_string())
            .collect();
        if !(1..=MAX_PLAYERS).contains(&names.len()) {
            return Err(PollError::Invalid(format!(
                "a rehearsal has between 1 and {MAX_PLAYERS} players, got {}",
                names.len()
            )));
        }
        for (index, name) in names.iter().enumerate() {
            if name.is_empty() {
                return Err(PollError::Invalid("player names must not be empty".into()));
            }
            if names[..index]
                .iter()
                .any(|other| other.to_lowercase() == name.to_lowercase())
            {
                return Err(PollError::Invalid(format!(
                    "the player name {name:?} appears twice"
                )));
            }
        }
        for (value, name) in [(spec.skill, "skill"), (spec.speed, "speed")]
            .into_iter()
            .chain(spec.team_skill.iter().map(|skill| (*skill, "skill")))
        {
            if !(0.0..=1.0).contains(&value) {
                return Err(PollError::Invalid(format!(
                    "{name} goes from 0 to 1, got {value}"
                )));
            }
        }
        if let Some(arrive) = spec.arrive
            && !(arrive.is_finite() && arrive >= 0.0)
        {
            return Err(PollError::Invalid(format!(
                "arrive must be zero or positive seconds, got {arrive}"
            )));
        }
        self.state.lock().expect("canvas state poisoned").rehearsal =
            RehearsalSpec { names, ..spec };
        Ok(())
    }

    /// Play the game in teams: each player joins one of `names`, dealt to
    /// the smallest team, or chosen on the phone with `choose`. `colors`
    /// (`#rrggbb`, one per team) tint the phones; `None` picks
    /// [`TEAM_COLORS`]. A scene has one set of teams.
    pub fn teams(
        &mut self,
        names: Vec<String>,
        colors: Option<Vec<String>>,
        choose: bool,
    ) -> Result<TeamsHandle, PollError> {
        let names: Vec<String> = names.iter().map(|name| name.trim().to_string()).collect();
        if !(2..=MAX_TEAMS).contains(&names.len()) {
            return Err(PollError::Invalid(format!(
                "a game has between 2 and {MAX_TEAMS} teams, got {}",
                names.len()
            )));
        }
        for (index, name) in names.iter().enumerate() {
            if name.is_empty() || name.chars().count() > 20 {
                return Err(PollError::Invalid(format!(
                    "team names have 1 to 20 characters, got {name:?}"
                )));
            }
            if names[..index]
                .iter()
                .any(|other| other.to_lowercase() == name.to_lowercase())
            {
                return Err(PollError::Invalid(format!(
                    "the team name {name:?} appears twice"
                )));
            }
        }
        let colors = match colors {
            Some(colors) => colors
                .iter()
                .map(|color| color.trim().to_lowercase())
                .collect(),
            None => TEAM_COLORS[..names.len()]
                .iter()
                .map(|color| color.to_string())
                .collect::<Vec<_>>(),
        };
        let hex = |color: &str| {
            color.len() == 7
                && color.starts_with('#')
                && color[1..].chars().all(|ch| ch.is_ascii_hexdigit())
        };
        if colors.len() != names.len() || !colors.iter().all(|color| hex(color)) {
            return Err(PollError::Invalid(format!(
                "give one #rrggbb color per team, got {colors:?}"
            )));
        }
        let info = gaanim_timeline::timeline::TeamsInfo {
            names,
            colors,
            choose,
        };
        let mut state = self.state.lock().expect("canvas state poisoned");
        if state.poll_session.is_none() {
            return Err(PollError::NoSession);
        }
        if state
            .poll_teams
            .as_ref()
            .is_some_and(|teams| *teams != info)
        {
            return Err(PollError::Invalid(
                "the scene already has other teams; a game has one set".into(),
            ));
        }
        state.poll_teams = Some(info.clone());
        drop(state);
        Ok(TeamsHandle {
            state: self.state.clone(),
            info,
        })
    }

    /// Ask each player for one more thing when joining, such as a student
    /// code or a full name, besides the nickname. Only the saved results
    /// show it, never the screen. A scene asks one thing.
    pub fn roster(&mut self, ask: String, required: bool) -> Result<(), PollError> {
        let label = ask.trim().to_string();
        if label.is_empty() || label.chars().count() > MAX_ASK {
            return Err(PollError::Invalid(format!(
                "a roster asks for something of 1 to {MAX_ASK} characters, got {label:?}"
            )));
        }
        let info = gaanim_timeline::timeline::AskInfo { label, required };
        let mut state = self.state.lock().expect("canvas state poisoned");
        if state.poll_session.is_none() {
            return Err(PollError::NoSession);
        }
        if state.poll_ask.as_ref().is_some_and(|asked| *asked != info) {
            return Err(PollError::Invalid(
                "the scene already asks for something else; a roster asks one thing".into(),
            ));
        }
        state.poll_ask = Some(info);
        Ok(())
    }

    /// The made-up audience of [`Self::rehearsal`].
    pub fn rehearsal_spec(&self) -> RehearsalSpec {
        self.state
            .lock()
            .expect("canvas state poisoned")
            .rehearsal
            .clone()
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
    /// `lean` is how the rehearsal votes (weights per answer, or made up).
    pub fn poll(
        &mut self,
        question: impl Into<String>,
        options: impl IntoIterator<Item = impl Into<String>>,
        lean: Lean,
        style: PollStyle,
    ) -> Result<PollHandle, PollError> {
        if matches!(lean, Lean::Right(_)) {
            return Err(PollError::Invalid(
                "only a quiz has a right answer to rehearse; give a poll weights".into(),
            ));
        }
        self.open_poll(question.into(), options, lean, None, style)
    }

    /// Open a quiz at the cursor: a poll with right answers, `time` seconds
    /// to answer once and up to `points` for a fast right answer. Several
    /// right answers make it multiple choice: an answer is right when it
    /// chose all of them and no other.
    pub fn quiz(
        &mut self,
        question: impl Into<String>,
        options: impl IntoIterator<Item = impl Into<String>>,
        correct: Vec<usize>,
        time: u32,
        points: u32,
        lean: Lean,
        style: PollStyle,
    ) -> Result<PollHandle, PollError> {
        let mut correct = correct;
        correct.sort_unstable();
        correct.dedup();
        if correct.is_empty() {
            return Err(PollError::Invalid("a quiz needs a right answer".into()));
        }
        let style = PollStyle {
            multiple: style.multiple || correct.len() > 1,
            ..style
        };
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
            revealed: Vec::new(),
        };
        self.open_poll(question.into(), options, lean, Some(quiz), style)
    }

    fn open_poll(
        &mut self,
        question: String,
        options: impl IntoIterator<Item = impl Into<String>>,
        lean: Lean,
        quiz: Option<QuizRecord>,
        style: PollStyle,
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
        if let Some(answer) = quiz
            .iter()
            .flat_map(|quiz| &quiz.correct)
            .find(|answer| **answer >= options.len())
        {
            return Err(PollError::UnknownAnswer {
                answer: *answer,
                count: options.len(),
            });
        }
        match &lean {
            Lean::Auto => {}
            Lean::Right(share) if (0.0..=1.0).contains(share) => {}
            Lean::Right(share) => {
                return Err(PollError::Invalid(format!(
                    "the share that answers right goes from 0 to 1, got {share}"
                )));
            }
            Lean::Weights(weights) => {
                if weights.len() != options.len() {
                    return Err(PollError::LeanLength {
                        expected: options.len(),
                        got: weights.len(),
                    });
                }
                if weights
                    .iter()
                    .any(|weight| !(weight.is_finite() && *weight >= 0.0))
                    || weights.iter().sum::<f64>() <= 0.0
                {
                    return Err(PollError::Invalid(format!(
                        "rehearse weights must be zero or positive, and not all zero, got {weights:?}"
                    )));
                }
            }
        }
        let mut state = self.state.lock().expect("canvas state poisoned");
        if state.poll_session.is_none() {
            return Err(PollError::NoSession);
        }
        let index = state.polls.len();
        let open = (state.active_idx, state.active().cursor);
        state.polls.push(PollRecord {
            id: poll_id(index, &question, &options, quiz.as_ref(), &style),
            question,
            options,
            lean,
            open,
            close: None,
            quiz,
            multiple: style.multiple,
            image: style.image,
        });
        Ok(PollHandle {
            index,
            state: self.state.clone(),
        })
    }

    /// The game's leaderboard: the players of every quiz in this scene,
    /// best first. Text uses the theme's foreground.
    pub fn leaderboard(&mut self) -> LeaderboardHandle {
        let color = self
            .theme_style
            .as_ref()
            .map_or(Color::WHITE, |theme| theme.palette.foreground);
        LeaderboardHandle {
            state: self.state.clone(),
            color,
        }
    }

    /// How many stops the active segment has: pass it to
    /// [`Self::gate_stop`] after authoring the next one.
    pub fn stop_count(&self) -> usize {
        self.state
            .lock()
            .expect("canvas state poisoned")
            .active()
            .stops
            .len()
    }

    /// Make the stop authored after the segment had `stops_before` stops
    /// advance by itself once `until` holds while presenting. Nothing
    /// happens when no stop was authored (a live narration take holds
    /// instead of stopping).
    pub fn gate_stop(&mut self, stops_before: usize, until: GateCondition) {
        let mut state = self.state.lock().expect("canvas state poisoned");
        let segment = state.active_idx;
        if let Some(time) = state.active().stops.get(stops_before).map(|stop| stop.time) {
            state.stop_gates.push((segment, time, until));
        }
    }

    /// The game's audience: the players in the order they joined, as data
    /// for the scene to arrange and animate. A scene that uses it asks each
    /// phone for a nickname as soon as it opens the page, so a lobby fills
    /// before the first question. Text uses the theme's foreground.
    pub fn audience(&mut self) -> Result<AudienceHandle, PollError> {
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
            if state.poll_lobby_at.is_none() {
                state.poll_lobby_at = Some((state.active_idx, state.active().cursor));
            }
        }
        Ok(AudienceHandle {
            state: self.state.clone(),
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

    /// The right answers of a quiz; `None` for a poll.
    pub fn correct(&self) -> Option<Vec<usize>> {
        self.record().quiz.map(|quiz| quiz.correct)
    }

    /// Whether players may choose several answers.
    pub fn multiple(&self) -> bool {
        self.record().multiple
    }

    /// The picture phones show above the question.
    pub fn image(&self) -> Option<PollImage> {
        self.record().image
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

    pub(crate) fn check_answer(&self, answer: usize) -> Result<(), PollError> {
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
            vec![measure.value(&vec![0; record.options.len()])],
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
        let preview = spec.fraction(answer, &vec![0; record.options.len()]);
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

    /// The shape answer `answer` has on phones (a triangle, diamond,
    /// circle, square, star or hexagon), `size` units tall, centered on the
    /// origin and filled with the answer's color.
    pub fn icon(&self, answer: usize, size: f64) -> Result<DrawableHandle, PollError> {
        self.check_answer(answer)?;
        if !(size.is_finite() && size > 0.0) {
            return Err(PollError::Invalid(format!(
                "icon size must be positive, got {size}"
            )));
        }
        let handle = super::canvas_impl::spawn_in(
            &self.state,
            SpawnKind::Curve(answer_shape(answer, size)),
            true,
        );
        let color = Color::from_rgba8(
            u8::from_str_radix(&ANSWER_COLORS[answer][1..3], 16).unwrap_or(0),
            u8::from_str_radix(&ANSWER_COLORS[answer][3..5], 16).unwrap_or(0),
            u8::from_str_radix(&ANSWER_COLORS[answer][5..7], 16).unwrap_or(0),
            255,
        );
        Ok(handle.fill(color).no_stroke())
    }

    /// The color answer `answer` has on phones, `#rrggbb`.
    pub fn color(&self, answer: usize) -> Result<&'static str, PollError> {
        self.check_answer(answer)?;
        Ok(ANSWER_COLORS[answer])
    }

    /// A condition for [`SceneModel::gate_stop`]: at least `at_least`
    /// answers, or answers from at least `share` (0 to 1) of the audience
    /// (the players for a quiz, the phones on the voting page for a poll).
    /// Exactly one of the two.
    pub fn answered(
        &self,
        at_least: Option<u32>,
        share: Option<f64>,
    ) -> Result<GateCondition, PollError> {
        let record = self.record();
        match (at_least, share) {
            (Some(count), None) => Ok(GateCondition::Answers {
                poll: record.id,
                count,
            }),
            (None, Some(share)) if share > 0.0 && share <= 1.0 => Ok(GateCondition::AnswerShare {
                poll: record.id,
                share,
                players: record.quiz.is_some(),
            }),
            (None, Some(share)) => Err(PollError::Invalid(format!(
                "share must be above 0 and at most 1, got {share}"
            ))),
            _ => Err(PollError::Invalid(
                "give exactly one of at_least and share".into(),
            )),
        }
    }

    /// A condition for [`SceneModel::gate_stop`]: the quiz is out of time.
    pub fn time_up(&self) -> Result<GateCondition, PollError> {
        let record = self.record();
        if record.quiz.is_none() {
            return Err(PollError::NotQuiz);
        }
        Ok(GateCondition::TimeUp { poll: record.id })
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
        let revealed = {
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
            quiz.revealed.clone()
        };
        // Past the reveal, not on it: resting on a stop the reveal shares
        // keeps the quiz open, as the presentation does.
        for parameter in revealed {
            parameter
                .drawable()
                .drive_from_samples(
                    vec![0.0, REVEAL_AFTER],
                    vec![0.0, 1.0],
                    SampledProperty::Signal,
                    SampledInterpolation::Step,
                    1.0,
                    0.0,
                )
                .map_err(|_| PollError::Invalid("could not drive the reveal".into()))?;
        }
        Ok(())
    }

    /// 0 until the quiz [`reveal`](Self::reveal)s its answer, then 1, so a
    /// scene can keep its results hidden until then. It is a moment of the
    /// timeline, the same in previews, exports and live: a presentation
    /// reveals when it reaches it.
    pub fn revealed(&self) -> Result<Parameter, PollError> {
        {
            let state = self.state.lock().expect("canvas state poisoned");
            let quiz = state.polls[self.index]
                .quiz
                .as_ref()
                .ok_or(PollError::NotQuiz)?;
            if quiz.reveal.is_some() {
                return Err(PollError::Invalid(
                    "call revealed() before the quiz's reveal()".into(),
                ));
            }
        }
        let parameter = parameter_in(&self.state, 0.0)
            .map_err(|error| PollError::Invalid(error.to_string()))?;
        if let Some(quiz) = self.state.lock().expect("canvas state poisoned").polls[self.index]
            .quiz
            .as_mut()
        {
            quiz.revealed.push(parameter.clone());
        }
        Ok(parameter)
    }
}

/// The game's leaderboard, from [`SceneModel::leaderboard`].
#[derive(Clone)]
pub struct LeaderboardHandle {
    state: SharedCanvasState,
    color: Color,
}

impl std::fmt::Debug for LeaderboardHandle {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("LeaderboardHandle")
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
    /// The nickname at `rank` (0 for the leader) as live text; empty when
    /// fewer players joined.
    pub fn name(&self, rank: usize, options: LiveTextOptions) -> Result<DrawableHandle, PollError> {
        live_text(
            &self.state,
            self.color,
            LiveTextSource::LeaderName { rank },
            Arc::from(""),
            options,
        )
    }

    /// The score of the player at `rank`, live while presenting.
    pub fn points(&self, rank: usize) -> Result<Parameter, PollError> {
        live_parameter(
            &self.state,
            PollSource::LeaderScore { rank },
            vec![0.0],
            vec![0.0],
            SampledInterpolation::Step,
        )
    }

    /// How many players joined.
    pub fn players(&self) -> Result<Parameter, PollError> {
        live_parameter(
            &self.state,
            PollSource::Players,
            vec![0.0],
            vec![0.0],
            SampledInterpolation::Step,
        )
    }

    /// A bar whose length is the score at `rank` against the leader's.
    pub fn bar(&self, rank: usize, options: PollBarOptions) -> Result<DrawableHandle, PollError> {
        let spec = options.spec()?;
        Ok(spawn_bar(
            &self.state,
            spec,
            BarSource::Leader { rank },
            leader_fraction([], rank),
        ))
    }
}

/// The game's teams, from [`SceneModel::teams`]: each has an index, in the
/// order given.
#[derive(Clone)]
pub struct TeamsHandle {
    state: SharedCanvasState,
    info: gaanim_timeline::timeline::TeamsInfo,
}

impl std::fmt::Debug for TeamsHandle {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("TeamsHandle")
            .field("names", &self.info.names)
            .finish_non_exhaustive()
    }
}

impl TeamsHandle {
    pub fn names(&self) -> Vec<String> {
        self.info.names.clone()
    }

    pub fn colors(&self) -> Vec<String> {
        self.info.colors.clone()
    }

    /// Whether players choose their team on the phone.
    pub fn choose(&self) -> bool {
        self.info.choose
    }

    fn check(&self, team: usize) -> Result<(), PollError> {
        if team < self.info.names.len() {
            Ok(())
        } else {
            Err(PollError::Invalid(format!(
                "team {team} does not exist; the game has {} teams",
                self.info.names.len()
            )))
        }
    }

    fn value(&self, team: usize, measure: TeamMeasure) -> Result<Parameter, PollError> {
        self.check(team)?;
        live_parameter(
            &self.state,
            PollSource::Team { team, measure },
            vec![0.0],
            vec![0.0],
            SampledInterpolation::Step,
        )
    }

    /// The points of `team`'s players added up.
    pub fn score(&self, team: usize) -> Result<Parameter, PollError> {
        self.value(team, TeamMeasure::Score)
    }

    /// How many players `team` has.
    pub fn players(&self, team: usize) -> Result<Parameter, PollError> {
        self.value(team, TeamMeasure::Players)
    }

    /// `team`'s points per player, 0 while it has none.
    pub fn average(&self, team: usize) -> Result<Parameter, PollError> {
        self.value(team, TeamMeasure::Average)
    }

    /// The index of the team leading on points (the first on a tie).
    pub fn leader(&self) -> Result<Parameter, PollError> {
        live_parameter(
            &self.state,
            PollSource::LeadingTeam,
            vec![0.0],
            vec![0.0],
            SampledInterpolation::Step,
        )
    }

    /// A bar whose length is `team`'s points against the leading team's.
    pub fn bar(&self, team: usize, options: PollBarOptions) -> Result<DrawableHandle, PollError> {
        self.check(team)?;
        let spec = options.spec()?;
        Ok(spawn_bar(&self.state, spec, BarSource::Team { team }, 0.0))
    }
}

/// The game's audience, from [`SceneModel::audience`]: each player has a
/// slot, its place in joining order (0 for the first to join).
#[derive(Clone)]
pub struct AudienceHandle {
    state: SharedCanvasState,
    color: Color,
}

impl std::fmt::Debug for AudienceHandle {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("AudienceHandle")
            .finish_non_exhaustive()
    }
}

impl AudienceHandle {
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
        live_text(
            &self.state,
            self.color,
            LiveTextSource::AudienceName { slot },
            Arc::from(""),
            options,
        )
    }

    /// A condition for [`SceneModel::gate_stop`]: at least `count` players
    /// joined.
    pub fn at_least(&self, count: u32) -> GateCondition {
        GateCondition::Players { count }
    }

    /// How many players joined.
    pub fn count(&self) -> Result<Parameter, PollError> {
        live_parameter(
            &self.state,
            PollSource::Players,
            vec![0.0],
            vec![0.0],
            SampledInterpolation::Step,
        )
    }

    /// 1 once a player took `slot`, else 0: drive a slot's visibility.
    pub fn joined(&self, slot: usize) -> Result<Parameter, PollError> {
        live_parameter(
            &self.state,
            PollSource::AudienceJoined { slot },
            vec![0.0],
            vec![0.0],
            SampledInterpolation::Step,
        )
    }

    /// Seconds since the player in `slot` joined, up to
    /// [`gaanim_animation::polls::AUDIENCE_AGE_CAP`]; 0 while the slot is empty. Drive an entrance
    /// with it.
    pub fn age(&self, slot: usize) -> Result<Parameter, PollError> {
        live_parameter(
            &self.state,
            PollSource::AudienceAge { slot },
            vec![0.0],
            vec![0.0],
            SampledInterpolation::Step,
        )
    }
}

/// A number about one player, by slot in the audience or rank in the
/// leaderboard: their game so far, or what they answered on one poll.
#[derive(Debug, Clone, PartialEq)]
pub enum PlayerFact {
    Score,
    /// Quizzes answered right.
    Correct,
    /// Quizzes answered.
    Answered,
    /// Quizzes answered right in a row.
    Streak,
    /// 1 once they answered the poll.
    Responded,
    /// 1 if they chose this answer.
    Chose(usize),
    /// 1 if they answered the quiz right.
    Right,
    /// Points the answer earned.
    Earned,
    /// Seconds they took to answer.
    Time,
}

/// A parameter following `fact` about the player `who`; `poll` for the
/// facts about one poll.
fn player_parameter(
    state: &SharedCanvasState,
    who: gaanim_animation::polls::PlayerRef,
    fact: PlayerFact,
    poll: Option<&PollHandle>,
) -> Result<Parameter, PollError> {
    use gaanim_animation::polls::PlayerMeasure;
    let id = |poll: Option<&PollHandle>| -> Result<std::sync::Arc<str>, PollError> {
        poll.map(|poll| poll.id().into())
            .ok_or_else(|| PollError::Invalid("this fact is about a poll: pass one".into()))
    };
    let measure = match fact {
        PlayerFact::Score => PlayerMeasure::Score,
        PlayerFact::Correct => PlayerMeasure::Correct,
        PlayerFact::Answered => PlayerMeasure::Answered,
        PlayerFact::Streak => PlayerMeasure::Streak,
        PlayerFact::Responded => PlayerMeasure::Responded { poll: id(poll)? },
        PlayerFact::Chose(option) => {
            if let Some(poll) = poll {
                poll.check_answer(option)?;
            }
            PlayerMeasure::Chose {
                poll: id(poll)?,
                option,
            }
        }
        PlayerFact::Right => {
            if poll.is_some_and(|poll| poll.correct().is_none()) {
                return Err(PollError::NotQuiz);
            }
            PlayerMeasure::Right { poll: id(poll)? }
        }
        PlayerFact::Earned => PlayerMeasure::Points { poll: id(poll)? },
        PlayerFact::Time => PlayerMeasure::Time { poll: id(poll)? },
    };
    live_parameter(
        state,
        PollSource::Player { who, measure },
        vec![0.0],
        vec![0.0],
        SampledInterpolation::Step,
    )
}

impl AudienceHandle {
    /// `fact` about the player who joined `slot`-th; 0 while nobody did.
    pub fn player(
        &self,
        slot: usize,
        fact: PlayerFact,
        poll: Option<&PollHandle>,
    ) -> Result<Parameter, PollError> {
        player_parameter(
            &self.state,
            gaanim_animation::polls::PlayerRef::Slot(slot),
            fact,
            poll,
        )
    }
}

impl LeaderboardHandle {
    /// `fact` about the player at `rank`; 0 past the last one.
    pub fn player(
        &self,
        rank: usize,
        fact: PlayerFact,
        poll: Option<&PollHandle>,
    ) -> Result<Parameter, PollError> {
        player_parameter(
            &self.state,
            gaanim_animation::polls::PlayerRef::Rank(rank),
            fact,
            poll,
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
    fn polls_validate_their_question_answers_and_rehearsal() {
        let mut scene = scene();
        let error = |result: Result<PollHandle, PollError>| result.unwrap_err();
        assert_eq!(
            error(scene.poll(" ", ["A", "B"], Lean::Auto, PollStyle::default())),
            PollError::EmptyQuestion
        );
        assert_eq!(
            error(scene.poll("Q", ["A"], Lean::Auto, PollStyle::default())),
            PollError::OptionCount { count: 1 }
        );
        assert_eq!(
            error(scene.poll(
                "Q",
                ["1", "2", "3", "4", "5", "6", "7"],
                Lean::Auto,
                PollStyle::default()
            )),
            PollError::OptionCount { count: 7 }
        );
        assert_eq!(
            error(scene.poll("Q", ["A", " "], Lean::Auto, PollStyle::default())),
            PollError::EmptyOption
        );
        assert!(matches!(
            error(scene.poll("Q", ["A", "A "], Lean::Auto, PollStyle::default())),
            PollError::DuplicateOption { .. }
        ));
        assert_eq!(
            error(scene.poll(
                "Q",
                ["A", "B"],
                Lean::Weights(vec![1.0]),
                PollStyle::default()
            )),
            PollError::LeanLength {
                expected: 2,
                got: 1
            }
        );
        assert!(
            scene
                .poll(
                    "Q",
                    ["A", "B"],
                    Lean::Weights(vec![0.0, 0.0]),
                    PollStyle::default()
                )
                .is_err()
        );
        assert!(
            scene
                .poll("Q", ["A", "B"], Lean::Right(0.5), PollStyle::default())
                .is_err()
        );
        assert!(
            scene
                .quiz(
                    "Q",
                    ["A", "B"],
                    vec![0],
                    20,
                    1000,
                    Lean::Right(1.5),
                    PollStyle::default()
                )
                .is_err()
        );
        assert!(
            scene
                .quiz(
                    "Q",
                    ["A", "B"],
                    vec![0],
                    20,
                    1000,
                    Lean::Right(0.8),
                    PollStyle::default()
                )
                .is_ok()
        );
        let mut bare = SceneModel::new(640, 360);
        assert_eq!(
            error(bare.poll("Q", ["A", "B"], Lean::Auto, PollStyle::default())),
            PollError::NoSession
        );
    }

    #[test]
    fn quizzes_validate_their_answer_time_and_points() {
        let mut scene = scene();
        assert_eq!(
            scene
                .quiz(
                    "Q",
                    ["A", "B"],
                    vec![2],
                    20,
                    1000,
                    Lean::Auto,
                    PollStyle::default()
                )
                .unwrap_err(),
            PollError::UnknownAnswer {
                answer: 2,
                count: 2
            }
        );
        assert!(
            scene
                .quiz(
                    "Q",
                    ["A", "B"],
                    vec![0],
                    2,
                    1000,
                    Lean::Auto,
                    PollStyle::default()
                )
                .is_err()
        );
        assert!(
            scene
                .quiz(
                    "Q",
                    ["A", "B"],
                    vec![0],
                    20,
                    50,
                    Lean::Auto,
                    PollStyle::default()
                )
                .is_err()
        );
        let quiz = scene
            .quiz(
                "Q",
                ["A", "B"],
                vec![1],
                20,
                1000,
                Lean::Auto,
                PollStyle::default(),
            )
            .unwrap();
        assert_eq!((quiz.correct(), quiz.time()), (Some(vec![1]), Some(20)));
        assert!(quiz.id().starts_with("q0-"));
        assert!(quiz.reveal().is_ok());
        assert_eq!(quiz.reveal(), Err(PollError::AlreadyRevealed));
        let poll = scene
            .poll("Plain", ["A", "B"], Lean::Auto, PollStyle::default())
            .unwrap();
        assert_eq!(poll.correct(), None);
        assert_eq!(poll.reveal(), Err(PollError::NotQuiz));
        assert!(matches!(poll.remaining(), Err(PollError::NotQuiz)));
    }

    #[test]
    fn multiple_choice_quizzes_take_every_right_answer() {
        let mut scene = scene();
        let quiz = scene
            .quiz(
                "Q",
                ["A", "B", "C"],
                vec![2, 0, 2],
                20,
                1000,
                Lean::Auto,
                PollStyle::default(),
            )
            .unwrap();
        assert_eq!(quiz.correct(), Some(vec![0, 2]));
        assert!(quiz.multiple());
        assert!(
            scene
                .quiz(
                    "Q",
                    ["A", "B"],
                    vec![],
                    20,
                    1000,
                    Lean::Auto,
                    PollStyle::default()
                )
                .is_err()
        );
        assert!(
            scene
                .quiz(
                    "Q",
                    ["A", "B"],
                    vec![0, 5],
                    20,
                    1000,
                    Lean::Auto,
                    PollStyle::default()
                )
                .is_err()
        );
        let several = PollStyle {
            multiple: true,
            image: None,
        };
        let poll = scene.poll("P", ["A", "B"], Lean::Auto, several).unwrap();
        assert!(poll.multiple() && poll.correct().is_none());
        // The manner of asking is part of a poll's identity.
        assert_ne!(
            poll.id(),
            scene
                .poll("P", ["A", "B"], Lean::Auto, PollStyle::default())
                .unwrap()
                .id()
        );
        assert!(poll.icon(1, 0.5).is_ok());
        assert!(poll.icon(2, 0.5).is_err());
        assert_eq!(poll.color(1), Ok(ANSWER_COLORS[1]));
    }

    #[test]
    fn a_poll_picture_is_scaled_for_phones_and_named_by_its_bytes() {
        let directory =
            std::env::temp_dir().join(format!("gaanim-poll-image-{}", std::process::id()));
        std::fs::create_dir_all(&directory).unwrap();
        let path = directory.join("big.png");
        image::RgbImage::from_pixel(2000, 500, image::Rgb([20, 120, 220]))
            .save(&path)
            .unwrap();
        let picture = poll_image(&path).unwrap();
        assert_eq!(picture.mime, "image/jpeg");
        assert_eq!(picture.hash.len(), 16);
        let decoded = image::load_from_memory(&picture.bytes).unwrap();
        assert_eq!((decoded.width(), decoded.height()), (POLL_IMAGE_SIZE, 256));
        assert_eq!(poll_image(&path).unwrap().hash, picture.hash);
        assert!(poll_image(&directory.join("missing.png")).is_err());
        std::fs::remove_dir_all(&directory).ok();
    }

    #[test]
    fn poll_ids_are_stable_and_follow_the_text_and_scoring() {
        let options = ["A".to_string(), "B".to_string()];
        let quiz = |correct| QuizRecord {
            correct: vec![correct],
            time: 20,
            points: 1000,
            reveal: None,
            revealed: Vec::new(),
        };
        assert_eq!(
            poll_id(0, "Q", &options, None, &PollStyle::default()),
            poll_id(0, "Q", &options, None, &PollStyle::default())
        );
        assert_ne!(
            poll_id(0, "Q", &options, None, &PollStyle::default()),
            poll_id(0, "Q?", &options, None, &PollStyle::default())
        );
        assert_ne!(
            poll_id(0, "Q", &options, None, &PollStyle::default()),
            poll_id(1, "Q", &options, None, &PollStyle::default())
        );
        assert_ne!(
            poll_id(0, "Q", &options, Some(&quiz(0)), &PollStyle::default()),
            poll_id(0, "Q", &options, Some(&quiz(1)), &PollStyle::default())
        );
        assert!(poll_id(3, "¿Cuál?", &options, None, &PollStyle::default()).starts_with("p3-"));
    }

    #[test]
    fn a_poll_reports_its_session_and_rejects_unknown_answers() {
        let mut scene = scene();
        let poll = scene
            .poll(
                " ¿Cuál? ",
                ["x²", "2ˣ"],
                Lean::Weights(vec![3.0, 1.0]),
                PollStyle::default(),
            )
            .unwrap();
        assert_eq!(poll.question(), "¿Cuál?");
        assert_eq!(poll.code(), "ABC234");
        assert_eq!(poll.url(), "https://relay.example.dev/s/ABC234");
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
    fn the_leaderboard_validates_text() {
        let mut scene = scene();
        let board = scene.leaderboard();
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
    fn a_rehearsal_validates_its_crowd() {
        let mut scene = scene();
        let spec = |names: Vec<&str>| RehearsalSpec {
            names: names.into_iter().map(String::from).collect(),
            ..Default::default()
        };
        assert!(scene.rehearsal(spec(vec![])).is_err());
        assert!(scene.rehearsal(spec(vec!["Ana", " ana "])).is_err());
        assert!(scene.rehearsal(spec(vec!["Ana", " "])).is_err());
        let too_fast = RehearsalSpec {
            speed: 1.5,
            ..spec(vec!["Ana"])
        };
        assert!(scene.rehearsal(too_fast).is_err());
        let late = RehearsalSpec {
            arrive: Some(-1.0),
            ..spec(vec!["Ana"])
        };
        assert!(scene.rehearsal(late).is_err());
        scene.rehearsal(spec(vec![" Ana ", "Beto"])).unwrap();
        assert_eq!(scene.rehearsal_spec().names, ["Ana", "Beto"]);
    }

    #[test]
    fn teams_validate_their_names_and_colors() {
        let mut scene = scene();
        let names = |list: &[&str]| list.iter().map(|name| name.to_string()).collect();
        assert!(scene.teams(names(&["Solo"]), None, false).is_err());
        assert!(scene.teams(names(&["A", "a"]), None, false).is_err());
        assert!(scene.teams(names(&["A", ""]), None, false).is_err());
        let colors = |list: &[&str]| Some(list.iter().map(|color| color.to_string()).collect());
        assert!(
            scene
                .teams(names(&["A", "B"]), colors(&["#ff0000"]), false)
                .is_err()
        );
        assert!(
            scene
                .teams(names(&["A", "B"]), colors(&["red", "#0000ff"]), false)
                .is_err()
        );
        let teams = scene.teams(names(&[" Rojo ", "Azul"]), None, true).unwrap();
        assert_eq!(teams.names(), ["Rojo", "Azul"]);
        assert_eq!(teams.colors(), [TEAM_COLORS[0], TEAM_COLORS[1]]);
        assert!(teams.choose());
        assert!(teams.score(1).is_ok());
        assert!(teams.score(2).is_err());
        // The same teams again are fine; others are not.
        assert!(scene.teams(names(&["Rojo", "Azul"]), None, true).is_ok());
        assert!(scene.teams(names(&["Rojo", "Verde"]), None, true).is_err());
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
