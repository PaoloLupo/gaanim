//! Pure timing, segmentation and layout helpers for typewriter and scramble
//! text motion.
//!
//! Everything here is a function of its inputs (text, glyph boxes, seed,
//! index, time), so the authoring layer and the timeline lenses compute the
//! same keystroke schedule, the same scrambled glyphs and the same cursor
//! positions for any seek.

use std::ops::Range;

use gaanim_core::kurbo::Rect;
use gaanim_math::SeededRng;
use unicode_segmentation::UnicodeSegmentation;

/// Extended grapheme clusters of `text`, the unit a keystroke reveals.
pub fn graphemes(text: &str) -> Vec<&str> {
    text.graphemes(true).collect()
}

/// Number of leading graphemes `a` and `b` share.
pub fn common_prefix_graphemes(a: &str, b: &str) -> usize {
    a.graphemes(true)
        .zip(b.graphemes(true))
        .take_while(|(left, right)| left == right)
        .count()
}

/// The first `count` graphemes of `text`.
pub fn grapheme_prefix(text: &str, count: usize) -> &str {
    let end = text
        .grapheme_indices(true)
        .nth(count)
        .map_or(text.len(), |(offset, _)| offset);
    &text[..end]
}

/// A deterministic 64-bit hash of `(seed, a, b)`.
pub fn motion_hash(seed: u64, a: u64, b: u64) -> u64 {
    let mixed = seed
        ^ a.wrapping_add(1).wrapping_mul(0x9E37_79B9_7F4A_7C15)
        ^ b.wrapping_add(1)
            .wrapping_mul(0xC2B2_AE3D_27D4_EB4F)
            .rotate_left(31);
    SeededRng::new(mixed).next_u64()
}

/// Uniform value in `[0, 1)` for `(seed, a, b)`.
fn motion_unit(seed: u64, a: u64, b: u64) -> f64 {
    (motion_hash(seed, a, b) >> 11) as f64 * (1.0 / (1u64 << 53) as f64)
}

/// Seconds between keystrokes when typing grapheme `index`: `1 / cps`
/// scaled by a seeded factor in `[1 - jitter, 1 + jitter]`.
pub fn keystroke_interval(index: usize, cps: f64, jitter: f64, seed: u64) -> f64 {
    let spread = 2.0 * motion_unit(seed, index as u64, 0x7970) - 1.0;
    (1.0 + jitter.clamp(0.0, 0.95) * spread) / cps
}

/// Seconds, from the start of a motion, at which each timed keystroke lands.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct TypingPlan {
    /// Time of each deletion, removing the last visible grapheme first.
    pub delete_at: Vec<f64>,
    /// Time of each typed grapheme, in order.
    pub type_at: Vec<f64>,
    /// Natural duration: the last keystroke (0 without keystrokes).
    pub duration: f64,
}

impl TypingPlan {
    /// Deletes `deletions` graphemes at a steady `delete_cps`, then types the
    /// graphemes in `typed` at `cps` with seeded jitter. With no jitter,
    /// ⌊t·cps⌋ graphemes are visible `t` seconds into the typing.
    pub fn new(
        deletions: usize,
        delete_cps: f64,
        typed: Range<usize>,
        cps: f64,
        jitter: f64,
        seed: u64,
    ) -> Self {
        let mut time = 0.0;
        let delete_at = (0..deletions)
            .map(|_| {
                time += 1.0 / delete_cps;
                time
            })
            .collect();
        let type_at = typed
            .map(|index| {
                time += keystroke_interval(index, cps, jitter, seed);
                time
            })
            .collect();
        Self {
            delete_at,
            type_at,
            duration: time,
        }
    }

    /// Keystroke time as a progress fraction of a motion lasting `duration`.
    pub fn fraction(&self, time: f64, duration: f64) -> f64 {
        if self.duration <= 0.0 || duration <= 0.0 {
            0.0
        } else {
            (time / self.duration).clamp(0.0, 1.0)
        }
    }

    /// Progress at which deleting ends and typing begins.
    pub fn switch_fraction(&self, duration: f64) -> f64 {
        self.delete_at
            .last()
            .map_or(0.0, |time| self.fraction(*time, duration))
    }
}

/// Default length of a scramble: `reveal_delay` plus 0.05 s per settling
/// grapheme, at least 0.6 s.
pub fn scramble_duration(settling: usize, reveal_delay: f64) -> f64 {
    reveal_delay.max(0.0) + (0.05 * settling as f64).max(0.6)
}

/// Progress at which each of `count` scrambled positions settles, left to
/// right after `delay_fraction` of the motion.
pub fn settle_fractions(count: usize, delay_fraction: f64) -> Vec<f64> {
    let delay = delay_fraction.clamp(0.0, 1.0);
    (0..count)
        .map(|index| delay + (1.0 - delay) * (index + 1) as f64 / count as f64)
        .collect()
}

/// Charset entry shown at `position` during scramble step `step`.
pub fn scramble_pick(seed: u64, position: usize, step: i64, len: usize) -> usize {
    if len == 0 {
        return 0;
    }
    (motion_hash(seed, position as u64, step as u64) % len as u64) as usize
}

/// Graphemes of a named charset (`"upper"`, `"lower"`, `"digits"`, `"hex"`,
/// `"symbols"`) or of a literal string such as `"01"`. Whitespace is ignored.
pub fn charset_graphemes(charset: &str) -> Result<Vec<String>, String> {
    let literal = match charset {
        "upper" => "ABCDEFGHIJKLMNOPQRSTUVWXYZ",
        "lower" => "abcdefghijklmnopqrstuvwxyz",
        "digits" => "0123456789",
        "hex" => "0123456789ABCDEF",
        "symbols" => "!#$%&*+-=?@^_~<>/\\|[]{}",
        other => other,
    };
    let mut glyphs: Vec<String> = Vec::new();
    for grapheme in literal.graphemes(true) {
        if !grapheme.trim().is_empty() && !glyphs.iter().any(|known| known == grapheme) {
            glyphs.push(grapheme.to_string());
        }
    }
    if glyphs.is_empty() {
        Err("charset must name a preset (upper, lower, digits, hex, symbols) or contain visible characters".into())
    } else {
        Ok(glyphs)
    }
}

/// Where one grapheme sits in laid-out text.
#[derive(Debug, Clone, PartialEq)]
pub struct GraphemeCell {
    /// Indices of the glyphs (in the caller's glyph list) that draw it.
    pub glyphs: Vec<usize>,
    /// Pen position before the grapheme.
    pub pen_before: f64,
    /// Pen position after the grapheme, where a cursor follows it.
    pub pen_after: f64,
    /// Horizontal center of its ink (of its cell without ink).
    pub center: f64,
    /// Baseline of its line.
    pub baseline: f64,
}

/// Graphemes of laid-out text mapped to their glyphs, pens and baselines.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct TypingLayout {
    pub graphemes: Vec<String>,
    pub cells: Vec<GraphemeCell>,
    pub first_baseline: f64,
}

/// Distance between ink and pen on either side of a glyph, in ems.
const BEARING_EM: f64 = 0.04;
/// Advance of a space without neighbouring ink, in ems.
const SPACE_EM: f64 = 0.25;

impl TypingLayout {
    /// Align `glyphs` (source character and ink box, in reading order) with
    /// the graphemes of `rendered`. Lines are recovered from the ink boxes;
    /// the first line's baseline is `first_baseline` (y-up) and `em` is the
    /// font size in the same units.
    pub fn build(rendered: &str, glyphs: &[(char, Rect)], first_baseline: f64, em: f64) -> Self {
        let graphemes: Vec<String> = rendered.graphemes(true).map(str::to_string).collect();
        let count = graphemes.len();
        let owner = assign_glyphs(&graphemes, glyphs);
        let mut members: Vec<Vec<usize>> = vec![Vec::new(); count];
        for (glyph, grapheme) in owner.iter().enumerate() {
            if let Some(grapheme) = grapheme {
                members[*grapheme].push(glyph);
            }
        }
        let ink: Vec<Option<Rect>> = members
            .iter()
            .map(|glyph_ids| {
                glyph_ids
                    .iter()
                    .map(|glyph| glyphs[*glyph].1)
                    .reduce(|a, b| a.union(b))
            })
            .collect();

        // Lines from ink centres: a new line starts well below the current one.
        let mut line_of: Vec<Option<usize>> = vec![None; count];
        let mut line_centers: Vec<(f64, usize)> = Vec::new();
        for (index, rect) in ink.iter().enumerate() {
            let Some(rect) = rect else { continue };
            let center = rect.center().y;
            let same_line = line_centers.last().is_some_and(|(sum, n)| {
                let mean = sum / *n as f64;
                (center - mean).abs() < 0.6 * em
            });
            if !same_line {
                line_centers.push((0.0, 0));
            }
            let line = line_centers.len() - 1;
            line_centers[line].0 += center;
            line_centers[line].1 += 1;
            line_of[index] = Some(line);
        }
        let line_count = line_centers.len().max(1);
        let mut bottoms: Vec<Vec<f64>> = vec![Vec::new(); line_count];
        for (index, rect) in ink.iter().enumerate() {
            if let (Some(rect), Some(line)) = (rect, line_of[index]) {
                bottoms[line].push(rect.y0);
            }
        }
        let median = |values: &mut Vec<f64>| {
            values.sort_by(f64::total_cmp);
            values.get(values.len() / 2).copied()
        };
        let reference = median(&mut bottoms[0].clone()).unwrap_or(0.0);
        let baselines: Vec<f64> = bottoms
            .iter_mut()
            .map(|values| first_baseline + median(values).map_or(0.0, |m| m - reference))
            .collect();

        let bearing = BEARING_EM * em;
        let space = SPACE_EM * em;
        let mut cells: Vec<GraphemeCell> = (0..count)
            .map(|index| match ink[index] {
                Some(rect) => GraphemeCell {
                    glyphs: members[index].clone(),
                    pen_before: rect.x0 - bearing,
                    pen_after: rect.x1 + bearing,
                    center: rect.center().x,
                    baseline: baselines[line_of[index].unwrap_or(0)],
                },
                None => GraphemeCell {
                    glyphs: Vec::new(),
                    pen_before: 0.0,
                    pen_after: 0.0,
                    center: 0.0,
                    baseline: first_baseline,
                },
            })
            .collect();

        // Glyph-less graphemes (spaces, newlines, ligature tails) share the
        // gap between their inked neighbours or extend from one of them.
        let mut index = 0;
        while index < count {
            if ink[index].is_some() {
                index += 1;
                continue;
            }
            let start = index;
            while index < count && ink[index].is_none() {
                index += 1;
            }
            let end = index;
            let previous = start.checked_sub(1);
            let next = (end < count).then_some(end);
            // Up to the first newline the run trails the previous line.
            let split = (start..end)
                .find(|grapheme| graphemes[*grapheme].contains(['\n', '\r']))
                .unwrap_or(end);
            let same_line =
                matches!((previous, next), (Some(p), Some(n)) if line_of[p] == line_of[n]);
            if let (Some(p), Some(n), true) = (previous, next, same_line && split == end) {
                let from = cells[p].pen_after;
                let to = cells[n].pen_before.max(from);
                let step = (to - from) / (end - start) as f64;
                for (offset, grapheme) in (start..end).enumerate() {
                    let before = from + step * offset as f64;
                    let cell = &mut cells[grapheme];
                    cell.pen_before = before;
                    cell.pen_after = before + step;
                    cell.center = before + step * 0.5;
                    cell.baseline = cells_baseline(&baselines, &line_of, p);
                }
                continue;
            }
            if let Some(p) = previous {
                let mut pen = cells[p].pen_after;
                let baseline = cells_baseline(&baselines, &line_of, p);
                for grapheme in start..split {
                    let cell = &mut cells[grapheme];
                    cell.pen_before = pen;
                    cell.pen_after = pen + space;
                    cell.center = pen + space * 0.5;
                    cell.baseline = baseline;
                    pen += space;
                }
            }
            // The rest leads into the next inked grapheme; a newline itself
            // occupies no width, so the pen restarts at the next line.
            let leading = if previous.is_some() { split } else { start };
            let width_of = |grapheme: usize| {
                if graphemes[grapheme].contains(['\n', '\r']) {
                    0.0
                } else {
                    space
                }
            };
            let total: f64 = (leading..end).map(width_of).sum();
            let (mut pen, baseline) = match next {
                Some(n) => (
                    cells[n].pen_before - total,
                    cells_baseline(&baselines, &line_of, n),
                ),
                None => (0.0, first_baseline),
            };
            for grapheme in leading..end {
                let width = width_of(grapheme);
                let cell = &mut cells[grapheme];
                cell.pen_before = pen;
                cell.pen_after = pen + width;
                cell.center = pen + width * 0.5;
                cell.baseline = baseline;
                pen += width;
            }
        }

        Self {
            graphemes,
            cells,
            first_baseline,
        }
    }

    /// Pen `(x, baseline)` of a cursor after the first `count` graphemes.
    pub fn cursor_after(&self, count: usize) -> (f64, f64) {
        match count.checked_sub(1).and_then(|last| self.cells.get(last)) {
            Some(cell) => (cell.pen_after, cell.baseline),
            None => self
                .cells
                .first()
                .map_or((0.0, self.first_baseline), |cell| {
                    (cell.pen_before, cell.baseline)
                }),
        }
    }
}

fn cells_baseline(baselines: &[f64], line_of: &[Option<usize>], index: usize) -> f64 {
    baselines[line_of[index].unwrap_or(0)]
}

/// Owning grapheme of each glyph. Glyphs map one-to-one onto the visible
/// graphemes when the counts agree; otherwise (ligatures, hyphenation) each
/// glyph's character is matched a few characters ahead, and an unmatched
/// glyph joins its predecessor's grapheme.
fn assign_glyphs(graphemes: &[String], glyphs: &[(char, Rect)]) -> Vec<Option<usize>> {
    let visible: Vec<usize> = graphemes
        .iter()
        .enumerate()
        .filter(|(_, grapheme)| !grapheme.trim().is_empty())
        .map(|(index, _)| index)
        .collect();
    if visible.len() == glyphs.len() {
        return visible.into_iter().map(Some).collect();
    }
    const LOOKAHEAD: usize = 6;
    let characters: Vec<(char, usize)> = graphemes
        .iter()
        .enumerate()
        .flat_map(|(index, grapheme)| grapheme.chars().map(move |c| (c, index)))
        .filter(|(c, _)| !c.is_whitespace())
        .collect();
    let mut cursor = 0;
    let mut previous: Option<usize> = None;
    glyphs
        .iter()
        .map(|(character, _)| {
            let found = characters
                .get(cursor..)
                .unwrap_or_default()
                .iter()
                .take(LOOKAHEAD)
                .position(|(candidate, _)| candidate == character);
            let owner = match found {
                Some(offset) => {
                    cursor += offset + 1;
                    Some(characters[cursor - 1].1)
                }
                None => previous.or_else(|| characters.get(cursor).map(|(_, owner)| *owner)),
            };
            previous = owner.or(previous);
            owner
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn boxes(text: &str, y: f64) -> Vec<(char, Rect)> {
        text.chars()
            .enumerate()
            .filter(|(_, c)| !c.is_whitespace())
            .map(|(index, c)| {
                let x = index as f64 * 0.5;
                (c, Rect::new(x + 0.05, y, x + 0.45, y + 0.7))
            })
            .collect()
    }

    #[test]
    fn keystrokes_follow_cps_and_jitter_deterministically() {
        let plain = TypingPlan::new(0, 18.0, 0..5, 10.0, 0.0, 3);
        assert_eq!(plain.type_at.len(), 5);
        for (index, time) in plain.type_at.iter().enumerate() {
            assert!((time - (index + 1) as f64 / 10.0).abs() < 1e-12);
        }
        assert!((plain.duration - 0.5).abs() < 1e-12);
        let jittered = TypingPlan::new(0, 18.0, 0..40, 10.0, 0.2, 7);
        assert_eq!(jittered, TypingPlan::new(0, 18.0, 0..40, 10.0, 0.2, 7));
        assert_ne!(jittered, TypingPlan::new(0, 18.0, 0..40, 10.0, 0.2, 8));
        let mut previous = 0.0;
        for time in &jittered.type_at {
            let interval = time - previous;
            assert!(
                (0.08 - 1e-12..=0.12 + 1e-12).contains(&interval),
                "{interval}"
            );
            previous = *time;
        }
        let retype = TypingPlan::new(3, 20.0, 5..7, 10.0, 0.0, 0);
        assert_eq!(retype.delete_at.len(), 3);
        assert!((retype.switch_fraction(1.0) - 0.15 / 0.35).abs() < 1e-12);
        assert_eq!(retype.fraction(retype.duration, 2.0), 1.0);
    }

    #[test]
    fn scramble_picks_and_settles_are_pure() {
        let picks: Vec<usize> = (0..50).map(|step| scramble_pick(4, 2, step, 26)).collect();
        assert_eq!(
            picks,
            (0..50)
                .map(|step| scramble_pick(4, 2, step, 26))
                .collect::<Vec<_>>()
        );
        assert!(picks.iter().all(|pick| *pick < 26));
        assert!(picks.windows(2).any(|pair| pair[0] != pair[1]));
        assert_ne!(
            (0..20)
                .map(|step| scramble_pick(4, 3, step, 26))
                .collect::<Vec<_>>(),
            picks[..20]
        );
        let settles = settle_fractions(4, 0.2);
        for (settle, expected) in settles.iter().zip([0.4, 0.6, 0.8, 1.0]) {
            assert!((settle - expected).abs() < 1e-12);
        }
        assert_eq!(settles[3], 1.0);
        assert!((scramble_duration(4, 0.3) - 0.9).abs() < 1e-12);
    }

    #[test]
    fn charsets_resolve_presets_and_literals() {
        assert_eq!(charset_graphemes("upper").unwrap().len(), 26);
        assert_eq!(charset_graphemes("hex").unwrap().len(), 16);
        assert_eq!(charset_graphemes("01").unwrap(), vec!["0", "1"]);
        assert_eq!(charset_graphemes("a a").unwrap(), vec!["a"]);
        assert!(charset_graphemes("  ").is_err());
        assert_eq!(common_prefix_graphemes("gaanim render", "gaanim export"), 7);
        assert_eq!(grapheme_prefix("clímax", 3), "clí");
    }

    #[test]
    fn layout_maps_glyphs_pens_and_lines() {
        let layout = TypingLayout::build("ab c", &boxes("ab c", 0.0), 0.0, 1.0);
        assert_eq!(layout.cells.len(), 4);
        assert_eq!(layout.cells[0].glyphs, vec![0]);
        assert_eq!(layout.cells[2].glyphs, Vec::<usize>::new());
        // The space fills the gap between "b" and "c".
        assert!((layout.cells[2].pen_before - layout.cells[1].pen_after).abs() < 1e-12);
        assert!((layout.cells[2].pen_after - layout.cells[3].pen_before).abs() < 1e-12);
        assert_eq!(layout.cursor_after(0), (layout.cells[0].pen_before, 0.0));
        assert_eq!(layout.cursor_after(4).0, layout.cells[3].pen_after);
        // A trailing space advances past the last glyph.
        let trailing = TypingLayout::build("ab ", &boxes("ab ", 0.0), 0.0, 1.0);
        assert!(trailing.cursor_after(3).0 > trailing.cursor_after(2).0);

        // A second line gets its own baseline.
        let mut glyphs = boxes("ab", 0.0);
        glyphs.extend(boxes("cd", -1.4));
        let lines = TypingLayout::build("ab\ncd", &glyphs, 0.1, 1.0);
        assert_eq!(lines.cells[3].glyphs, vec![2]);
        assert!((lines.cells[3].baseline - (0.1 - 1.4)).abs() < 1e-12);
        assert!((lines.cursor_after(3).1 - (0.1 - 1.4)).abs() < 1e-12);
        assert!(lines.cursor_after(3).0 < lines.cursor_after(2).0);
    }

    #[test]
    fn ligatures_attach_to_their_first_grapheme() {
        // "fi" drawn by one glyph whose source character is 'f'.
        let glyphs = vec![
            ('f', Rect::new(0.0, 0.0, 0.9, 0.7)),
            ('x', Rect::new(1.0, 0.0, 1.4, 0.7)),
        ];
        let layout = TypingLayout::build("fix", &glyphs, 0.0, 1.0);
        assert_eq!(layout.cells[0].glyphs, vec![0]);
        assert!(layout.cells[1].glyphs.is_empty());
        assert_eq!(layout.cells[2].glyphs, vec![1]);
    }
}
