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
    #[error("segment links must point from an earlier segment to a later segment")]
    InvalidLink,
    #[error("segment {id:?} does not exist")]
    UnknownSegment { id: SegmentId },
    #[error("could not create segment branding: {message}")]
    BrandAsset { message: String },
}
