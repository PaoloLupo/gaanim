//! Semantic metadata and handles for authored canvas segments.

use std::path::PathBuf;
use std::sync::Arc;

use super::ops::SharedCanvasState;

/// Reusable visual identity automatically added to presentation segments.
#[derive(Debug, Clone, PartialEq)]
pub struct PresentationBrand {
    pub logo: Option<PathBuf>,
    pub footer: Option<String>,
    pub slide_numbers: bool,
    pub rule: bool,
    pub show_on_cover: bool,
    pub logo_scale: f64,
}

impl Default for PresentationBrand {
    fn default() -> Self {
        Self {
            logo: None,
            footer: None,
            slide_numbers: true,
            rule: true,
            show_on_cover: false,
            logo_scale: 1.0,
        }
    }
}

/// Stable identifier for a segment within one canvas.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SegmentId(pub(crate) u32);

impl SegmentId {
    pub const fn raw(self) -> u32 {
        self.0
    }
}

/// A named or anonymous interactive pause authored inside a segment.
#[derive(Debug, Clone, PartialEq)]
pub struct SegmentStop {
    pub name: Option<String>,
    /// Absolute time on the compiled canvas timeline.
    pub time: f64,
    /// Length of the ambient loop that plays while the stop rests.
    pub ambient: Option<f64>,
    /// The audience poll shown while a presentation rests here.
    pub poll: Option<StopPoll>,
}

/// Most answers a poll accepts: they must fit on a phone and on the slide.
pub const MAX_POLL_OPTIONS: usize = 6;

/// An audience poll authored with [`SceneModel::poll`](super::SceneModel::poll).
#[derive(Debug, Clone, PartialEq)]
pub struct StopPoll {
    pub question: String,
    pub options: Vec<String>,
}

impl StopPoll {
    /// Trim and validate a question and its answers.
    pub fn new(
        question: impl Into<String>,
        options: impl IntoIterator<Item = impl Into<String>>,
    ) -> Result<Self, SegmentError> {
        let question = question.into().trim().to_string();
        if question.is_empty() {
            return Err(SegmentError::EmptyPollQuestion);
        }
        let options: Vec<String> = options
            .into_iter()
            .map(|option| option.into().trim().to_string())
            .collect();
        if !(2..=MAX_POLL_OPTIONS).contains(&options.len()) {
            return Err(SegmentError::PollOptionCount {
                count: options.len(),
            });
        }
        if options.iter().any(String::is_empty) {
            return Err(SegmentError::EmptyPollOption);
        }
        for (index, option) in options.iter().enumerate() {
            if options[..index].contains(option) {
                return Err(SegmentError::DuplicatePollOption {
                    option: option.clone(),
                });
            }
        }
        Ok(Self { question, options })
    }
}

/// A named instant on the global timeline authored with `scene.marker`.
#[derive(Debug, Clone, PartialEq)]
pub struct SceneMarker {
    pub name: String,
    /// Absolute time on the compiled canvas timeline.
    pub time: f64,
    /// Name of the segment that was active when the marker was authored.
    pub segment: String,
}

/// Absolute metadata for one authored segment.
#[derive(Debug, Clone, PartialEq)]
pub struct SegmentSpec {
    pub id: SegmentId,
    pub name: String,
    pub notes: Option<String>,
    /// Optional Python template name used to author this segment.
    pub template: Option<String>,
    pub start_time: f64,
    pub end_time: f64,
    pub stops: Vec<SegmentStop>,
}

/// Ordered semantic description of all segments authored in a canvas.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SegmentManifest {
    pub segments: Vec<SegmentSpec>,
}

impl SegmentManifest {
    pub fn duration(&self) -> f64 {
        self.segments.last().map_or(0.0, |segment| segment.end_time)
    }
}

/// Stable handle returned by [`SceneModel::segment`](super::SceneModel::segment).
#[derive(Clone)]
pub struct SegmentHandle {
    pub(crate) id: SegmentId,
    state: SharedCanvasState,
}

impl std::fmt::Debug for SegmentHandle {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("SegmentHandle")
            .field("id", &self.id)
            .finish_non_exhaustive()
    }
}

impl SegmentHandle {
    pub(crate) fn new(id: SegmentId, state: SharedCanvasState) -> Self {
        Self { id, state }
    }

    pub const fn id(&self) -> SegmentId {
        self.id
    }

    pub(crate) fn belongs_to(&self, state: &SharedCanvasState) -> bool {
        Arc::ptr_eq(&self.state, state)
    }
}

/// Errors raised while defining or selecting segments and stops.
#[derive(Debug, thiserror::Error)]
pub enum SegmentError {
    #[error("segment names must not be empty")]
    EmptyName,
    #[error("a segment named '{name}' already exists")]
    DuplicateName { name: String },
    #[error("the first segment cannot define an incoming transition")]
    FirstTransition,
    #[error("stop names must not be empty")]
    EmptyStopName,
    #[error("a stop already exists at {time:.6}s in the active segment")]
    DuplicateStopTime { time: f64 },
    #[error("segment belongs to a different Scene")]
    ForeignSegment,
    #[error("invalid segment post-process: {0}")]
    InvalidPostProcess(String),
    #[error("segment links must point from an earlier segment to a later segment")]
    InvalidLink,
    #[error("segment {id:?} does not exist")]
    UnknownSegment { id: SegmentId },
    #[error("could not create segment branding: {message}")]
    BrandAsset { message: String },
    #[error("marker names must not be empty")]
    EmptyMarkerName,
    #[error("marker name {name:?} is a number; marker names must be distinguishable from seconds")]
    NumericMarkerName { name: String },
    #[error("a marker named {name:?} already exists at {time:.6}s")]
    DuplicateMarker { name: String, time: f64 },
    #[error("poll questions must not be empty")]
    EmptyPollQuestion,
    #[error("a poll needs between 2 and {MAX_POLL_OPTIONS} answers, got {count}")]
    PollOptionCount { count: usize },
    #[error("poll answers must not be empty")]
    EmptyPollOption,
    #[error("the poll answer {option:?} appears twice")]
    DuplicatePollOption { option: String },
}
