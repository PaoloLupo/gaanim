//! The pages of a slide deck: a page for every point where a presentation
//! rests, and optionally the speaker notes of each slide. [`crate::pdf`]
//! lays them out.

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

/// Seconds within which the last stop rests at the end of the deck.
const END_EPSILON: f64 = 1e-6;

/// The pages of a deck lasting `duration` seconds, as a presenter moves
/// through it: the frame at every stop, in order, then the last frame unless
/// the last stop already rests there. With `notes`, the notes of a slide
/// follow its last page; a slide without notes, or without a page of its
/// own, adds none.
pub fn deck_pages(slides: &[DeckSlide], duration: f64, notes: bool) -> Vec<DeckPage> {
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
    let mut pages = Vec::with_capacity(frames.len() * 2);
    for (index, &(time, slide)) in frames.iter().enumerate() {
        pages.push(DeckPage::Frame { time, slide });
        let last_of_slide = frames.get(index + 1).is_none_or(|(_, next)| *next != slide);
        if notes
            && last_of_slide
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
            deck_pages(&slides, 5.0, false),
            [frame(1.0, 0), frame(2.0, 1), frame(3.0, 1), frame(5.0, 2)]
        );
        // Notes follow the last page of their slide; blank notes print none.
        assert_eq!(
            deck_pages(&slides, 5.0, true),
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
            deck_pages(&ending, 4.0, true),
            [frame(1.0, 0), frame(4.0, 0)]
        );
        // Without stops, the deck is its last frame.
        assert_eq!(deck_pages(&[], 2.0, true), [frame(2.0, 0)]);
    }
}
