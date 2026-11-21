//! The pages of a slide deck: a page for every point where a presentation
//! rests (or only each slide's last), and optionally the speaker notes of
//! each slide. [`crate::pdf`] lays them out, on pages of their own or
//! several to a sheet of paper as a handout.

/// A slide of a deck: an authored segment, its speaker notes and the times
/// of its stops.
#[derive(Debug, Clone, PartialEq)]
pub struct DeckSlide {
    pub name: String,
    pub notes: Option<String>,
    pub stops: Vec<f64>,
}

impl DeckSlide {
    /// The notes, unless there are none to print.
    fn printed_notes(&self) -> Option<&str> {
        self.notes
            .as_deref()
            .map(str::trim)
            .filter(|notes| !notes.is_empty())
    }
}

/// A page of an exported deck.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DeckPage {
    /// The frame shown while the presentation rests at `time`, in the slide
    /// at index `slide`.
    Frame { time: f64, slide: usize },
    /// The speaker notes of the slide at index `slide`.
    Notes { slide: usize },
}

/// How a deck is printed: which of its resting points become pages, with
/// or without notes, and on what paper.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DeckLayout {
    /// `--notes`: a page with a slide's notes after its last page.
    pub notes: bool,
    /// `--per-slide`: only the last resting point of each slide.
    pub per_slide: bool,
    /// `--handout`: slides laid out on sheets of paper, one per slide.
    pub handout: Option<Handout>,
    /// `--paper`: the handout's sheet.
    pub paper: Paper,
}

impl DeckLayout {
    /// Whether a slide has one page only: `--per-slide`, or any handout.
    pub fn one_per_slide(&self) -> bool {
        self.per_slide || self.handout.is_some()
    }

    /// Whether notes have pages of their own: `--notes` outside a handout,
    /// which prints them beside the slide if at all.
    fn notes_pages(&self) -> bool {
        self.notes && self.handout.is_none()
    }
}

/// How a handout lays slides out on a sheet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Handout {
    /// `columns` × `rows` slides to a sheet, each over its name.
    Grid { columns: u8, rows: u8 },
    /// A slide to a sheet, its notes under it.
    Notes,
}

impl Handout {
    /// Most columns or rows of a handout's grid.
    pub const MAX_SIDE: u8 = 6;

    /// Parse `--handout`: `notes`, or columns by rows such as `2x3`.
    pub fn parse(text: &str) -> Result<Self, String> {
        let expected = || {
            format!(
                "--handout takes columns by rows such as 2x3 (each 1 to {}) or notes, not `{text}`",
                Self::MAX_SIDE
            )
        };
        let text = text.trim().to_ascii_lowercase();
        if text == "notes" {
            return Ok(Self::Notes);
        }
        let (columns, rows) = text.split_once(['x', '×']).ok_or_else(expected)?;
        let side = |side: &str| {
            side.trim()
                .parse::<u8>()
                .ok()
                .filter(|side| (1..=Self::MAX_SIDE).contains(side))
                .ok_or_else(expected)
        };
        Ok(Self::Grid {
            columns: side(columns)?,
            rows: side(rows)?,
        })
    }

    /// Slides on each sheet.
    pub fn per_sheet(self) -> usize {
        match self {
            Self::Grid { columns, rows } => usize::from(columns) * usize::from(rows),
            Self::Notes => 1,
        }
    }
}

/// The sheet a handout is printed on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Paper {
    #[default]
    A4,
    Letter,
}

impl Paper {
    /// Parse `--paper`: `a4` or `letter`.
    pub fn parse(text: &str) -> Result<Self, String> {
        match text.trim().to_ascii_lowercase().as_str() {
            "a4" => Ok(Self::A4),
            "letter" => Ok(Self::Letter),
            _ => Err(format!("--paper takes a4 or letter, not `{text}`")),
        }
    }

    /// Width and height in points, upright.
    pub fn size(self) -> (f64, f64) {
        match self {
            Self::A4 => (595.276, 841.89),
            Self::Letter => (612.0, 792.0),
        }
    }

    /// Its name for Typst and `--paper`.
    pub fn name(self) -> &'static str {
        match self {
            Self::A4 => "a4",
            Self::Letter => "letter",
        }
    }
}

/// Seconds within which the last stop rests at the end of the deck.
const END_EPSILON: f64 = 1e-6;

/// The pages of a deck lasting `duration` seconds, as a presenter moves
/// through it: the frame at every stop, in order, then the last frame unless
/// the last stop already rests there. With [`DeckLayout::one_per_slide`],
/// only the last of each slide's frames. With notes outside a handout, the
/// notes of a slide follow its last page; a slide without notes, or without
/// a page of its own, adds none.
pub fn deck_pages(slides: &[DeckSlide], duration: f64, layout: &DeckLayout) -> Vec<DeckPage> {
    let mut frames: Vec<(f64, usize)> = slides
        .iter()
        .enumerate()
        .flat_map(|(slide, spec)| spec.stops.iter().map(move |time| (*time, slide)))
        .collect();
    if frames
        .last()
        .is_none_or(|(time, _)| *time < duration - END_EPSILON)
    {
        frames.push((duration, slides.len().saturating_sub(1)));
    }
    let last_of_slide = |frames: &[(f64, usize)], index: usize, slide: usize| {
        frames.get(index + 1).is_none_or(|(_, next)| *next != slide)
    };
    if layout.one_per_slide() {
        frames = frames
            .iter()
            .enumerate()
            .filter(|&(index, &(_, slide))| last_of_slide(&frames, index, slide))
            .map(|(_, frame)| *frame)
            .collect();
    }
    let notes = layout.notes_pages();
    let mut pages = Vec::with_capacity(frames.len() * 2);
    for (index, &(time, slide)) in frames.iter().enumerate() {
        pages.push(DeckPage::Frame { time, slide });
        if notes
            && last_of_slide(&frames, index, slide)
            && slides
                .get(slide)
                .and_then(DeckSlide::printed_notes)
                .is_some()
        {
            pages.push(DeckPage::Notes { slide });
        }
    }
    pages
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOTES: DeckLayout = DeckLayout {
        notes: true,
        per_slide: false,
        handout: None,
        paper: Paper::A4,
    };

    fn slide(name: &str, notes: Option<&str>, stops: &[f64]) -> DeckSlide {
        DeckSlide {
            name: name.to_string(),
            notes: notes.map(str::to_string),
            stops: stops.to_vec(),
        }
    }

    #[test]
    fn decks_rest_at_every_stop_and_at_their_end() {
        let slides = [
            slide("Cover", Some("Say hello."), &[1.0]),
            slide("Plan", Some("One step per burn."), &[2.0, 3.0]),
            slide("Credits", Some("  "), &[]),
        ];
        let frame = |time, slide| DeckPage::Frame { time, slide };
        assert_eq!(
            deck_pages(&slides, 5.0, &DeckLayout::default()),
            [frame(1.0, 0), frame(2.0, 1), frame(3.0, 1), frame(5.0, 2)]
        );
        // Notes follow the last page of their slide; blank notes print none.
        assert_eq!(
            deck_pages(&slides, 5.0, &NOTES),
            [
                frame(1.0, 0),
                DeckPage::Notes { slide: 0 },
                frame(2.0, 1),
                frame(3.0, 1),
                DeckPage::Notes { slide: 1 },
                frame(5.0, 2),
            ]
        );
        // A deck whose last stop rests at its end has no extra page.
        let ending = [slide("Only", None, &[1.0, 4.0])];
        assert_eq!(
            deck_pages(&ending, 4.0, &NOTES),
            [frame(1.0, 0), frame(4.0, 0)]
        );
        // Without stops, the deck is its last frame.
        assert_eq!(deck_pages(&[], 2.0, &NOTES), [frame(2.0, 0)]);
    }

    #[test]
    fn per_slide_decks_keep_each_slide_s_last_resting_point() {
        let slides = [
            slide("Cover", Some("Hello."), &[1.0]),
            slide("Plan", Some("Steps."), &[2.0, 3.0, 4.0]),
            slide("Close", None, &[6.0]),
        ];
        let frame = |time, slide| DeckPage::Frame { time, slide };
        let per_slide = DeckLayout {
            per_slide: true,
            ..NOTES
        };
        assert_eq!(
            deck_pages(&slides, 6.0, &per_slide),
            [
                frame(1.0, 0),
                DeckPage::Notes { slide: 0 },
                frame(4.0, 1),
                DeckPage::Notes { slide: 1 },
                frame(6.0, 2),
            ]
        );
        // The end of the deck stands for its last slide's steps.
        let ending = [slide("Only", None, &[1.0, 2.0])];
        assert_eq!(deck_pages(&ending, 5.0, &per_slide), [frame(5.0, 0)]);
        // A handout prints one frame per slide, its notes beside it.
        let handout = DeckLayout {
            handout: Some(Handout::Grid {
                columns: 2,
                rows: 3,
            }),
            ..NOTES
        };
        assert_eq!(
            deck_pages(&slides, 6.0, &handout),
            [frame(1.0, 0), frame(4.0, 1), frame(6.0, 2)]
        );
    }

    #[test]
    fn handouts_and_papers_parse_from_their_options() {
        assert_eq!(
            Handout::parse("2x3"),
            Ok(Handout::Grid {
                columns: 2,
                rows: 3
            })
        );
        assert_eq!(
            Handout::parse(" 3X2 "),
            Ok(Handout::Grid {
                columns: 3,
                rows: 2
            })
        );
        assert_eq!(Handout::parse("notes"), Ok(Handout::Notes));
        for bad in ["", "6", "0x2", "7x1", "2x", "two"] {
            assert!(Handout::parse(bad).is_err(), "{bad}");
        }
        assert_eq!(
            Handout::Grid {
                columns: 2,
                rows: 3
            }
            .per_sheet(),
            6
        );
        assert_eq!(Paper::parse("Letter"), Ok(Paper::Letter));
        assert_eq!(Paper::parse("a4"), Ok(Paper::A4));
        assert!(Paper::parse("a3").is_err());
    }
}
