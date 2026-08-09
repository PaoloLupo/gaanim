//! Semantic metadata and handles for authored canvas segments.

use gaanim_math::Bounds3D;
use std::path::PathBuf;
use std::sync::Arc;

use super::LayoutRegion;
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

/// Built-in composition for an authored segment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SegmentLayout {
    #[default]
    Blank,
    Title,
    TitleContent,
    TwoColumns,
    Section,
    Closing,
}

impl SegmentLayout {
    pub fn parse(name: &str) -> Result<Self, SegmentError> {
        match name {
            "blank" => Ok(Self::Blank),
            "title" | "cover" => Ok(Self::Title),
            "title_content" | "content" | "agenda" => Ok(Self::TitleContent),
            "two_columns" | "comparison" => Ok(Self::TwoColumns),
            "section" | "divider" => Ok(Self::Section),
            "closing" | "conclusion" => Ok(Self::Closing),
            _ => Err(SegmentError::UnknownLayout {
                name: name.to_string(),
            }),
        }
    }

    pub const fn name(self) -> &'static str {
        match self {
            Self::Blank => "blank",
            Self::Title => "title",
            Self::TitleContent => "title_content",
            Self::TwoColumns => "two_columns",
            Self::Section => "section",
            Self::Closing => "closing",
        }
    }

    pub fn region(self, frame: Bounds3D, name: &str) -> Option<LayoutRegion> {
        let region = LayoutRegion { bounds: frame };
        let grid =
            |rows, columns, row, column| region.grid(rows, columns, 0.0, 0.0).cell(row, column);
        match (self, name) {
            (Self::Blank, "content") => Some(region),
            (Self::Title | Self::Closing, "title") => grid(3, 1, 0, 0),
            (Self::Title | Self::Closing, "subtitle") => grid(3, 1, 1, 0),
            (Self::Title | Self::Closing, "content") => grid(3, 1, 2, 0),
            (Self::TitleContent, "title") => grid(5, 1, 0, 0),
            (Self::TitleContent, "content") => region.grid(5, 1, 0.0, 0.0).area(1, 0, 4, 1),
            (Self::TwoColumns, "title") => grid(5, 1, 0, 0),
            (Self::TwoColumns, "left" | "before") => region
                .grid(5, 2, frame.height() * 0.03, frame.width() * 0.04)
                .area(1, 0, 4, 1),
            (Self::TwoColumns, "right" | "after") => region
                .grid(5, 2, frame.height() * 0.03, frame.width() * 0.04)
                .area(1, 1, 4, 1),
            (Self::Section, "eyebrow") => grid(4, 1, 0, 0),
            (Self::Section, "title") => grid(4, 1, 1, 0),
            (Self::Section, "subtitle") => grid(4, 1, 2, 0),
            (Self::Section, "content") => grid(4, 1, 3, 0),
            _ => None,
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
    pub layout: SegmentLayout,
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

/// Stable handle returned by [`Canvas::segment`](super::Canvas::segment).
#[derive(Clone)]
pub struct SegmentHandle {
    pub(crate) id: SegmentId,
    layout: SegmentLayout,
    frame: Bounds3D,
    state: SharedCanvasState,
}

impl std::fmt::Debug for SegmentHandle {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("SegmentHandle")
            .field("id", &self.id)
            .field("layout", &self.layout)
            .finish_non_exhaustive()
    }
}

impl SegmentHandle {
    pub(crate) fn new(
        id: SegmentId,
        layout: SegmentLayout,
        frame: Bounds3D,
        state: SharedCanvasState,
    ) -> Self {
        Self {
            id,
            layout,
            frame,
            state,
        }
    }

    pub const fn id(&self) -> SegmentId {
        self.id
    }

    pub const fn layout(&self) -> SegmentLayout {
        self.layout
    }

    pub fn region(&self, name: &str) -> Result<LayoutRegion, SegmentError> {
        self.layout
            .region(self.frame, name)
            .ok_or_else(|| SegmentError::UnknownRegion {
                layout: self.layout.name().to_string(),
                region: name.to_string(),
            })
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
    #[error("unknown segment layout '{name}'")]
    UnknownLayout { name: String },
    #[error("layout '{layout}' has no region named '{region}'")]
    UnknownRegion { layout: String, region: String },
    #[error("segment belongs to a different Scene")]
    ForeignSegment,
    #[error("segment links must point from an earlier segment to a later segment")]
    InvalidLink,
    #[error("segment {id:?} does not exist")]
    UnknownSegment { id: SegmentId },
    #[error("could not create segment branding: {message}")]
    BrandAsset { message: String },
}
