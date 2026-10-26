//! How one version of a code block's source becomes another, and which
//! glyphs of a block draw each part of its source.
//!
//! A token is a run of letters, digits and underscores, or any other visible
//! character on its own; whitespace separates tokens without being one. An
//! edit keeps every line that stays the same up to its indentation, in
//! order, so lines that only move up, down or sideways keep all their
//! tokens. Between two kept lines, the lines that changed are compared token
//! by token.

use std::ops::Range;

/// Largest table, in cells, a longest common subsequence is computed with.
/// Beyond it only the common start and end are kept.
const LCS_CELLS: usize = 4_000_000;

/// Byte ranges of the tokens of `source`, in order.
pub fn tokens(source: &str) -> Vec<Range<usize>> {
    let mut tokens = Vec::new();
    let mut word: Option<usize> = None;
    for (offset, character) in source.char_indices() {
        if is_word(character) {
            word.get_or_insert(offset);
            continue;
        }
        if let Some(start) = word.take() {
            tokens.push(start..offset);
        }
        if !character.is_whitespace() {
            tokens.push(offset..offset + character.len_utf8());
        }
    }
    if let Some(start) = word {
        tokens.push(start..source.len());
    }
    tokens
}

fn is_word(character: char) -> bool {
    character.is_alphanumeric() || character == '_'
}

/// How the tokens of one source become the tokens of another.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CodeDiff {
    /// Tokens both sources have: their byte ranges in the old source and in
    /// the new one, in order.
    pub kept: Vec<(Range<usize>, Range<usize>)>,
    /// Tokens of the old source that the new one drops.
    pub removed: Vec<Range<usize>>,
    /// Tokens of the new source that the old one lacks.
    pub added: Vec<Range<usize>>,
}

/// The tokens `old` and `new` share, and those only one of them has.
pub fn diff(old: &str, new: &str) -> CodeDiff {
    let old_tokens = tokens(old);
    let new_tokens = tokens(new);
    let old_lines = lines(old, &old_tokens);
    let new_lines = lines(new, &new_tokens);
    let old_keys: Vec<&str> = old_lines.iter().map(|line| line.key).collect();
    let new_keys: Vec<&str> = new_lines.iter().map(|line| line.key).collect();

    let mut result = CodeDiff::default();
    let compare = |old_span: Range<usize>, new_span: Range<usize>, result: &mut CodeDiff| {
        let old_texts: Vec<&str> = old_tokens[old_span.clone()]
            .iter()
            .map(|range| &old[range.clone()])
            .collect();
        let new_texts: Vec<&str> = new_tokens[new_span.clone()]
            .iter()
            .map(|range| &new[range.clone()])
            .collect();
        let pairs = longest_common_subsequence(&old_texts, &new_texts);
        let (mut old_next, mut new_next) = (0, 0);
        for (old_index, new_index) in pairs
            .into_iter()
            .chain([(old_texts.len(), new_texts.len())])
        {
            result.removed.extend(
                old_tokens[old_span.start + old_next..old_span.start + old_index]
                    .iter()
                    .cloned(),
            );
            result.added.extend(
                new_tokens[new_span.start + new_next..new_span.start + new_index]
                    .iter()
                    .cloned(),
            );
            if old_index < old_texts.len() {
                result.kept.push((
                    old_tokens[old_span.start + old_index].clone(),
                    new_tokens[new_span.start + new_index].clone(),
                ));
            }
            (old_next, new_next) = (old_index + 1, new_index + 1);
        }
    };

    // Kept lines anchor the comparison: the tokens between two of them are
    // compared on their own, so a common token never pairs across a kept
    // line.
    let (mut old_token, mut new_token) = (0, 0);
    for (old_line, new_line) in longest_common_subsequence(&old_keys, &new_keys) {
        let (old_line, new_line) = (&old_lines[old_line], &new_lines[new_line]);
        compare(
            old_token..old_line.tokens.start,
            new_token..new_line.tokens.start,
            &mut result,
        );
        for (old_index, new_index) in old_line.tokens.clone().zip(new_line.tokens.clone()) {
            result
                .kept
                .push((old_tokens[old_index].clone(), new_tokens[new_index].clone()));
        }
        (old_token, new_token) = (old_line.tokens.end, new_line.tokens.end);
    }
    compare(
        old_token..old_tokens.len(),
        new_token..new_tokens.len(),
        &mut result,
    );
    result
}

/// A line with tokens: its text without indentation or trailing space, and
/// the indices of its tokens.
struct Line<'a> {
    key: &'a str,
    tokens: Range<usize>,
}

fn lines<'a>(source: &'a str, tokens: &[Range<usize>]) -> Vec<Line<'a>> {
    let mut lines = Vec::new();
    let mut next = 0;
    let mut start = 0;
    for text in source.split_inclusive('\n') {
        let end = start + text.len();
        let first = next;
        while next < tokens.len() && tokens[next].start < end {
            next += 1;
        }
        if next > first {
            lines.push(Line {
                key: text.trim(),
                tokens: first..next,
            });
        }
        start = end;
    }
    lines
}

/// Index pairs of a longest common subsequence of `a` and `b`, in order.
///
/// When the part between their common start and end is larger than
/// [`LCS_CELLS`], only the common start and end pair.
fn longest_common_subsequence<T: PartialEq>(a: &[T], b: &[T]) -> Vec<(usize, usize)> {
    let prefix = a.iter().zip(b).take_while(|(a, b)| a == b).count();
    let suffix = a[prefix..]
        .iter()
        .rev()
        .zip(b[prefix..].iter().rev())
        .take_while(|(a, b)| a == b)
        .count();
    let (a_middle, b_middle) = (&a[prefix..a.len() - suffix], &b[prefix..b.len() - suffix]);
    let mut pairs: Vec<(usize, usize)> = (0..prefix).map(|index| (index, index)).collect();
    let (rows, columns) = (a_middle.len(), b_middle.len());
    if rows > 0 && columns > 0 && (rows + 1).saturating_mul(columns + 1) <= LCS_CELLS {
        // `lengths[i][j]` is the length of a longest common subsequence of
        // `a_middle[i..]` and `b_middle[j..]`.
        let width = columns + 1;
        let mut lengths = vec![0u32; (rows + 1) * width];
        for i in (0..rows).rev() {
            for j in (0..columns).rev() {
                lengths[i * width + j] = if a_middle[i] == b_middle[j] {
                    lengths[(i + 1) * width + j + 1] + 1
                } else {
                    lengths[(i + 1) * width + j].max(lengths[i * width + j + 1])
                };
            }
        }
        let (mut i, mut j) = (0, 0);
        while i < rows && j < columns {
            if a_middle[i] == b_middle[j] {
                pairs.push((prefix + i, prefix + j));
                i += 1;
                j += 1;
            } else if lengths[(i + 1) * width + j] >= lengths[i * width + j + 1] {
                i += 1;
            } else {
                j += 1;
            }
        }
    }
    pairs.extend((0..suffix).map(|index| (a.len() - suffix + index, b.len() - suffix + index)));
    pairs
}

/// For each range of `source` in `ranges`, the indices of the glyphs that
/// draw its characters, in order and without repeats.
///
/// `glyphs` gives the characters each glyph of the block draws, in reading
/// order: one for most glyphs, several for a ligature. Whitespace draws no
/// glyph and is skipped on both sides. When the glyphs draw exactly the
/// visible characters of `source`, each character finds its own glyph;
/// otherwise the two are aligned by their longest common subsequence and a
/// character no glyph draws belongs to none.
pub fn source_glyphs<G, C>(source: &str, glyphs: G, ranges: &[Range<usize>]) -> Vec<Vec<usize>>
where
    G: IntoIterator<Item = C>,
    C: IntoIterator<Item = char>,
{
    let drawn: Vec<(char, usize)> = glyphs
        .into_iter()
        .enumerate()
        .flat_map(|(glyph, characters)| {
            characters
                .into_iter()
                .filter(|character| !character.is_whitespace())
                .map(move |character| (character, glyph))
        })
        .collect();
    let visible: Vec<(usize, char)> = source
        .char_indices()
        .filter(|(_, character)| !character.is_whitespace())
        .collect();
    // The glyph of each visible character of `source`.
    let mut glyph_of: Vec<Option<usize>> = vec![None; visible.len()];
    let same = visible.len() == drawn.len()
        && visible
            .iter()
            .zip(&drawn)
            .all(|((_, character), (glyph_character, _))| character == glyph_character);
    if same {
        for (index, (_, glyph)) in drawn.iter().enumerate() {
            glyph_of[index] = Some(*glyph);
        }
    } else {
        let source_characters: Vec<char> =
            visible.iter().map(|(_, character)| *character).collect();
        let drawn_characters: Vec<char> = drawn.iter().map(|(character, _)| *character).collect();
        for (source, glyph) in longest_common_subsequence(&source_characters, &drawn_characters) {
            glyph_of[source] = Some(drawn[glyph].1);
        }
    }
    ranges
        .iter()
        .map(|range| {
            let first = visible.partition_point(|(offset, _)| *offset < range.start);
            let mut found: Vec<usize> = Vec::new();
            for ((offset, _), glyph) in visible[first..].iter().zip(&glyph_of[first..]) {
                if *offset >= range.end {
                    break;
                }
                if let Some(glyph) = *glyph
                    && !found.contains(&glyph)
                {
                    found.push(glyph);
                }
            }
            found
        })
        .collect()
}

/// Number of lines of `source`; a final newline ends the last line rather
/// than starting one.
pub fn line_count(source: &str) -> usize {
    source.split_inclusive('\n').count()
}

/// Byte range of lines `first` to `last` of `source`, counted from 1 and
/// both included, without the newline that ends the last one; `None` when
/// `first` is 0 or after `last`, or `last` is past the last line.
pub fn line_range(source: &str, first: usize, last: usize) -> Option<Range<usize>> {
    if first == 0 || first > last || last > line_count(source) {
        return None;
    }
    let mut start = 0;
    let mut range = None;
    for (index, text) in source.split_inclusive('\n').enumerate() {
        let number = index + 1;
        let end = start + text.trim_end_matches(['\n', '\r']).len();
        if number == first {
            range = Some(start..end);
        }
        if number == last {
            return range.map(|range| range.start..end);
        }
        start += text.len();
    }
    None
}

/// Byte ranges of the occurrences of `text` in `source`, left to right and
/// without overlapping; none for an empty `text`.
///
/// Where `text` starts or ends with a letter, digit or underscore, an
/// occurrence must not continue a word there, so `dt` is found in `dt * 2`
/// but not in `dtype`.
pub fn occurrences(source: &str, text: &str) -> Vec<Range<usize>> {
    if text.is_empty() {
        return Vec::new();
    }
    let starts_word = text.chars().next().is_some_and(is_word);
    let ends_word = text.chars().next_back().is_some_and(is_word);
    source
        .match_indices(text)
        .map(|(start, found)| start..start + found.len())
        .filter(|range| {
            let before = source[..range.start].chars().next_back();
            let after = source[range.end..].chars().next();
            !(starts_word && before.is_some_and(is_word))
                && !(ends_word && after.is_some_and(is_word))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn texts<'a>(source: &'a str, ranges: &[Range<usize>]) -> Vec<&'a str> {
        ranges.iter().map(|range| &source[range.clone()]).collect()
    }

    fn kept<'a>(old: &'a str, new: &'a str, diff: &CodeDiff) -> Vec<(&'a str, &'a str)> {
        diff.kept
            .iter()
            .map(|(a, b)| (&old[a.clone()], &new[b.clone()]))
            .collect()
    }

    #[test]
    fn tokens_are_words_and_single_visible_characters() {
        let source = "def area(r_1):\n    return 3.14 * r_1 ** 2  # é";
        assert_eq!(
            texts(source, &tokens(source)),
            [
                "def", "area", "(", "r_1", ")", ":", "return", "3", ".", "14", "*", "r_1", "*",
                "*", "2", "#", "é"
            ]
        );
        assert!(tokens("  \n\t ").is_empty());
    }

    #[test]
    fn a_changed_token_leaves_and_enters_while_the_rest_stays() {
        let (old, new) = ("x = 1 + y", "x = 2 + y");
        let diff = diff(old, new);
        assert_eq!(
            kept(old, new, &diff),
            [("x", "x"), ("=", "="), ("+", "+"), ("y", "y")]
        );
        assert_eq!(texts(old, &diff.removed), ["1"]);
        assert_eq!(texts(new, &diff.added), ["2"]);
    }

    #[test]
    fn inserted_lines_enter_and_the_lines_around_them_keep_their_tokens() {
        let old = "a = 1\nprint(a)\n";
        let new = "a = 1\nb = a * 2\nprint(a)\n";
        let diff = diff(old, new);
        assert!(diff.removed.is_empty());
        assert_eq!(texts(new, &diff.added), ["b", "=", "a", "*", "2"]);
        assert_eq!(diff.kept.len(), tokens(old).len());
        // `print(a)` keeps its own tokens: its `a` is not the new line's.
        let print_a = diff
            .kept
            .iter()
            .find(|(from, _)| from.start == old.find("a)").unwrap())
            .unwrap();
        assert_eq!(print_a.1.start, new.find("a)").unwrap());
    }

    #[test]
    fn a_reindented_line_is_kept_whole() {
        let old = "for x in xs:\nprint(x)";
        let new = "for x in xs:\n    if x:\n        print(x)";
        let diff = diff(old, new);
        assert!(diff.removed.is_empty());
        assert_eq!(texts(new, &diff.added), ["if", "x", ":"]);
        let print = diff
            .kept
            .iter()
            .find(|(from, _)| &old[from.clone()] == "print")
            .unwrap();
        assert_eq!(&new[print.1.clone()], "print");
    }

    #[test]
    fn changed_lines_compare_token_by_token_between_kept_lines() {
        let old = "def f(a):\n    return a + 1\n";
        let new = "def f(a, b):\n    return a + b\n";
        let diff = diff(old, new);
        assert_eq!(texts(old, &diff.removed), ["1"]);
        assert_eq!(texts(new, &diff.added), [",", "b", "b"]);
        assert_eq!(diff.kept.len(), tokens(old).len() - 1);
    }

    #[test]
    fn identical_and_empty_sources() {
        let source = "let x = 1;";
        let same = diff(source, source);
        assert_eq!(same.kept.len(), 5);
        assert!(same.removed.is_empty() && same.added.is_empty());
        let from_empty = diff("", source);
        assert_eq!(from_empty.added.len(), 5);
        assert!(from_empty.kept.is_empty());
        assert_eq!(diff(source, "").removed.len(), 5);
    }

    #[test]
    fn an_oversized_comparison_keeps_its_common_start_and_end() {
        let a: Vec<usize> = (0..3000).collect();
        let mut b: Vec<usize> = (0..3000).map(|value| value + 10_000).collect();
        b[0] = 0;
        b[2999] = 2999;
        let pairs = longest_common_subsequence(&a, &b);
        assert_eq!(pairs, [(0, 0), (2999, 2999)]);
    }

    #[test]
    fn source_glyphs_follow_visible_characters() {
        let source = "fn add(a, b) {\n    a + b\n}";
        let glyphs: Vec<Vec<char>> = source
            .chars()
            .filter(|character| !character.is_whitespace())
            .map(|character| vec![character])
            .collect();
        let ranges = [
            0..2,
            source.find("add").unwrap()..source.find("add").unwrap() + 3,
            source.rfind('b').unwrap()..source.len(),
        ];
        assert_eq!(
            source_glyphs(source, glyphs, &ranges),
            [vec![0, 1], vec![2, 3, 4], vec![13, 14]]
        );
    }

    #[test]
    fn a_ligature_glyph_draws_every_character_it_covers() {
        // `->` drawn as one glyph.
        let source = "a -> b";
        let glyphs = [vec!['a'], vec!['-', '>'], vec!['b']];
        let ranges = [2..3, 3..4, 5..6];
        assert_eq!(
            source_glyphs(source, glyphs, &ranges),
            [vec![1], vec![1], vec![2]]
        );
    }

    #[test]
    fn source_glyphs_align_when_glyphs_differ_from_the_source() {
        // The block drops the `?` it cannot draw.
        let source = "a?b";
        let glyphs = [vec!['a'], vec!['b']];
        assert_eq!(
            source_glyphs(source, glyphs, &[0..1, 1..2, 2..3]),
            [vec![0], vec![], vec![1]]
        );
    }

    #[test]
    fn a_code_block_draws_each_visible_character_of_its_source() {
        let source = "fn main() -> i32 {\n    let ok = a != b && c <= d; // é\n\tok as i32\n}";
        let markup = crate::code::CodeMarkup {
            source,
            language: "rust",
            font_size: 20.0,
            width: 800.0,
            font: None,
            foreground: gaanim_core::peniko::Color::WHITE,
            highlight: crate::code::CodeHighlight::Off,
        }
        .typst();
        let glyphs = crate::typst_compiler::compiled_glyph_characters(&markup);
        // The bundled font draws `->`, `!=`, `<=` or `//` with contextual
        // alternates: one character's glyph draws both and the other draws
        // nothing. Every glyph still draws a character of the source, in order.
        let whole = 0..source.len();
        let [all] = source_glyphs(source, glyphs.clone(), std::slice::from_ref(&whole))
            .try_into()
            .unwrap();
        assert_eq!(all, (0..glyphs.len()).collect::<Vec<_>>());
        let main = source.find("main").unwrap();
        let token = main..main + 4;
        let tokens = source_glyphs(source, glyphs.clone(), std::slice::from_ref(&token));
        let drawn: String = tokens[0]
            .iter()
            .flat_map(|glyph| glyphs[*glyph].clone())
            .collect();
        assert_eq!(drawn, "main");
    }

    #[test]
    fn lines_and_occurrences() {
        let source = "one\ntwo\r\nthree\n";
        assert_eq!(line_count(source), 3);
        assert_eq!(line_range(source, 2, 2).map(|r| &source[r]), Some("two"));
        assert_eq!(
            line_range(source, 1, 3).map(|r| &source[r]),
            Some("one\ntwo\r\nthree")
        );
        assert_eq!(line_range(source, 0, 1), None);
        assert_eq!(line_range(source, 2, 1), None);
        assert_eq!(line_range(source, 3, 4), None);
        assert_eq!(occurrences("dt * dt + dt", "dt"), [0..2, 5..7, 10..12]);
        assert_eq!(occurrences("dtype = dt + (dt)", "dt"), [8..10, 14..16]);
        assert_eq!(occurrences("a**2 + b**2", "**"), [1..3, 8..10]);
        assert_eq!(occurrences("x.y.z", ".y"), vec![1..3]);
        assert!(occurrences("abc", "").is_empty());
    }
}
