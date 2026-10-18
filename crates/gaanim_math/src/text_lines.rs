//! The rendered lines of a text, found from the boxes of its glyphs.
//!
//! Text markers and spotlights cover a selection one line at a time. Both
//! measure the glyphs in a frame where the text's lines run along x (its
//! rotation undone), group them here, and turn what they draw back with
//! the text.

use gaanim_core::glam::DVec2;

/// A glyph's box in a frame where its text's lines run along x.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LineGlyph {
    pub min: DVec2,
    pub max: DVec2,
    pub selected: bool,
}

/// One rendered line: the vertical extent of all its glyphs and the
/// horizontal extent of its selected ones, if any.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TextLine {
    pub min_y: f64,
    pub max_y: f64,
    pub selected_x: Option<(f64, f64)>,
}

/// Groups `glyphs` into rendered lines by vertical overlap, top line first.
///
/// A glyph joins the line it overlaps most, by at least 35% of its own
/// height; otherwise it starts a new line. Empty or non-finite boxes are
/// ignored.
pub fn group_lines(glyphs: &[LineGlyph]) -> Vec<TextLine> {
    let mut order: Vec<&LineGlyph> = glyphs
        .iter()
        .filter(|glyph| {
            glyph.min.is_finite()
                && glyph.max.is_finite()
                && glyph.max.x > glyph.min.x
                && glyph.max.y > glyph.min.y
        })
        .collect();
    order.sort_by(|a, b| (b.min.y + b.max.y).total_cmp(&(a.min.y + a.max.y)));

    let mut lines: Vec<TextLine> = Vec::new();
    for glyph in order {
        let height = glyph.max.y - glyph.min.y;
        let best = lines
            .iter()
            .enumerate()
            .map(|(index, line)| {
                let overlap = glyph.max.y.min(line.max_y) - glyph.min.y.max(line.min_y);
                (index, overlap)
            })
            .filter(|(_, overlap)| *overlap >= 0.35 * height)
            .max_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(index, _)| index);
        let line = match best {
            Some(index) => &mut lines[index],
            None => {
                lines.push(TextLine {
                    min_y: glyph.min.y,
                    max_y: glyph.max.y,
                    selected_x: None,
                });
                lines.last_mut().expect("line was just pushed")
            }
        };
        line.min_y = line.min_y.min(glyph.min.y);
        line.max_y = line.max_y.max(glyph.max.y);
        if glyph.selected {
            line.selected_x = Some(match line.selected_x {
                Some((low, high)) => (low.min(glyph.min.x), high.max(glyph.max.x)),
                None => (glyph.min.x, glyph.max.x),
            });
        }
    }
    lines.sort_by(|a, b| (b.min_y + b.max_y).total_cmp(&(a.min_y + a.max_y)));
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    fn glyph(x: f64, y: f64, width: f64, height: f64, selected: bool) -> LineGlyph {
        LineGlyph {
            min: DVec2::new(x, y),
            max: DVec2::new(x + width, y + height),
            selected,
        }
    }

    #[test]
    fn glyphs_group_into_lines_top_first() {
        let lines = group_lines(&[
            glyph(0.0, -0.8, 0.2, 0.4, true),
            glyph(0.0, 0.0, 0.2, 0.4, false),
            glyph(0.3, -0.05, 0.2, 0.45, true),
            glyph(0.3, -0.8, 0.2, 0.4, false),
            glyph(0.0, f64::NAN, 0.2, 0.4, true),
        ]);
        assert_eq!(lines.len(), 2);
        assert_eq!((lines[0].min_y, lines[0].max_y), (-0.05, 0.4));
        assert_eq!(lines[0].selected_x, Some((0.3, 0.5)));
        assert_eq!(lines[1].selected_x, Some((0.0, 0.2)));
        assert!(group_lines(&[glyph(0.0, 0.0, 0.0, 0.0, true)]).is_empty());
    }
}
