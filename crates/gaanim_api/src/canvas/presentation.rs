//! Semantic presentation metadata built on top of the timeline breakpoints.

use gaanim_math::Bounds3D;

use super::LayoutRegion;

/// Built-in composition for a semantic slide.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SlideTemplate {
    #[default]
    Blank,
    Title,
    TitleContent,
    TwoColumns,
    Section,
    Closing,
}

impl SlideTemplate {
    pub fn parse(name: &str) -> Result<Self, PresentationError> {
        match name {
            "blank" => Ok(Self::Blank),
            "title" => Ok(Self::Title),
            "title_content" => Ok(Self::TitleContent),
            "two_columns" => Ok(Self::TwoColumns),
            "section" => Ok(Self::Section),
            "closing" => Ok(Self::Closing),
            _ => Err(PresentationError::UnknownTemplate {
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

    /// Resolve a named region inside the canvas safe frame.
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
            (Self::TwoColumns, "left") => region
                .grid(5, 2, frame.height() * 0.03, frame.width() * 0.04)
                .area(1, 0, 4, 1),
            (Self::TwoColumns, "right") => region
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

/// Stable identifier for a slide within one [`PresentationManifest`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SlideId(pub(crate) u32);

impl SlideId {
    /// Numeric identity preserved across the compiled presentation metadata.
    pub const fn raw(self) -> u32 {
        self.0
    }
}

/// A named pause inside a slide.
#[derive(Debug, Clone, PartialEq)]
pub struct SlideStep {
    pub name: Option<String>,
    pub time: f64,
}

/// Metadata for one presentation slide.
#[derive(Debug, Clone, PartialEq)]
pub struct SlideSpec {
    pub id: SlideId,
    pub name: String,
    pub notes: Option<String>,
    pub template: SlideTemplate,
    pub start_time: f64,
    pub end_time: Option<f64>,
    pub steps: Vec<SlideStep>,
}

/// Ordered semantic description of the slides authored in a canvas.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PresentationManifest {
    pub slides: Vec<SlideSpec>,
    next_id: u32,
}

impl PresentationManifest {
    pub(crate) fn start_slide(
        &mut self,
        name: String,
        notes: Option<String>,
        template: SlideTemplate,
        start_time: f64,
    ) -> Result<SlideId, PresentationError> {
        if name.trim().is_empty() {
            return Err(PresentationError::EmptySlideName);
        }
        if self.slides.iter().any(|slide| slide.name == name) {
            return Err(PresentationError::DuplicateSlideName { name });
        }

        if let Some(previous) = self.slides.last_mut() {
            previous.end_time = Some(start_time);
        }
        let id = SlideId(self.next_id);
        self.next_id += 1;
        self.slides.push(SlideSpec {
            id,
            name,
            notes,
            template,
            start_time,
            end_time: None,
            steps: Vec::new(),
        });
        Ok(id)
    }

    pub(crate) fn add_step(
        &mut self,
        id: SlideId,
        name: Option<String>,
        time: f64,
    ) -> Result<(), PresentationError> {
        if name.as_deref().is_some_and(|name| name.trim().is_empty()) {
            return Err(PresentationError::EmptyStepName);
        }
        let Some(slide) = self.slides.last_mut() else {
            return Err(PresentationError::UnknownSlide { id });
        };
        if slide.id != id {
            return Err(PresentationError::InactiveSlide { id });
        }
        slide.steps.push(SlideStep { name, time });
        Ok(())
    }

    pub(crate) fn finalize(&mut self, end_time: f64) {
        if let Some(slide) = self.slides.last_mut() {
            slide.end_time = Some(end_time);
        }
    }
}

/// Errors raised while defining semantic slides.
#[derive(Debug, thiserror::Error)]
pub enum PresentationError {
    #[error("slide names must not be empty")]
    EmptySlideName,
    #[error("a slide named '{name}' already exists")]
    DuplicateSlideName { name: String },
    #[error("step names must not be empty")]
    EmptyStepName,
    #[error("unknown slide template '{name}'")]
    UnknownTemplate { name: String },
    #[error("template '{template}' has no region named '{region}'")]
    UnknownRegion { template: String, region: String },
    #[error("slide {id:?} does not exist")]
    UnknownSlide { id: SlideId },
    #[error("slide {id:?} is no longer active")]
    InactiveSlide { id: SlideId },
}
