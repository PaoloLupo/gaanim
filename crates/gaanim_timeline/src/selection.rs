//! Rehearsal selection of named segments: `--sections` and `--from`.
//!
//! The whole scene is still authored and compiled, so objects, camera and
//! theme before a selected segment are exactly those of a full run. Only
//! playback, stop navigation and stop capture are restricted to the selection.

use bevy::prelude::Resource;

/// Segments chosen by name. Empty when the whole timeline plays.
#[derive(Resource, Debug, Clone, Default, PartialEq, Eq)]
pub struct SegmentSelection {
    /// Play only these segments or sections, in timeline order.
    pub sections: Vec<String>,
    /// Play from this segment or section to the end.
    pub from: Option<String>,
    /// The `--sections` value as written, used whole when it names one
    /// segment exactly, so a name with commas needs no escaping.
    pub sections_value: Option<String>,
}

/// Selected segment indices and the merged time ranges they cover.
#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedSelection {
    pub segments: Vec<usize>,
    pub ranges: Vec<(f64, f64)>,
}

impl SegmentSelection {
    /// Parse a comma-separated `--sections` list. `\,` is a comma inside a
    /// name and `\\` a backslash: `Tiempo\, lugar,Cierre` is two names.
    pub fn parse_list(spec: &str) -> Result<Vec<String>, String> {
        let mut names = Vec::new();
        let mut name = String::new();
        let mut chars = spec.chars();
        while let Some(ch) = chars.next() {
            match ch {
                '\\' => match chars.next() {
                    Some(escaped @ (',' | '\\')) => name.push(escaped),
                    Some(other) => {
                        name.push('\\');
                        name.push(other);
                    }
                    None => name.push('\\'),
                },
                ',' => names.push(std::mem::take(&mut name)),
                _ => name.push(ch),
            }
        }
        names.push(name);
        let names: Vec<String> = names.iter().map(|name| name.trim().to_string()).collect();
        if names.iter().any(String::is_empty) {
            return Err(format!(
                "invalid section list `{spec}`: use comma-separated names, e.g. results,conclusions \
                 (write \\, for a comma inside a name)"
            ));
        }
        Ok(names)
    }

    /// Set the `--sections` list from its command-line value.
    pub fn set_sections(&mut self, spec: &str) -> Result<(), String> {
        self.sections = Self::parse_list(spec)?;
        self.sections_value = Some(spec.trim().to_string());
        Ok(())
    }

    pub fn is_empty(&self) -> bool {
        self.sections.is_empty() && self.from.is_none()
    }

    /// Resolve against `(name, start, end)` segments in timeline order.
    ///
    /// A name selects a segment whose whole name matches, every segment of a
    /// `Section` with that key (names `"<key> · <visit> · <step> · <title>"`),
    /// or the steps whose title matches, ignoring case. A name that selects
    /// nothing is an error listing the choices.
    pub fn resolve<'a>(
        &self,
        segments: impl IntoIterator<Item = (&'a str, f64, f64)>,
    ) -> Result<ResolvedSelection, String> {
        let segments: Vec<(&str, f64, f64)> = segments.into_iter().collect();
        let matching = |name: &str| -> Result<Vec<usize>, String> {
            let found = matching_names(name, &segments);
            if found.is_empty() {
                return Err(unknown_segment(name, &segments));
            }
            Ok(found)
        };
        let mut selected = vec![false; segments.len()];
        // A value that names one segment exactly is that name, commas and all.
        let whole = self
            .sections_value
            .as_ref()
            .filter(|value| self.sections.len() > 1 && !matching_names(value, &segments).is_empty())
            .map(|value| vec![value.clone()]);
        for name in whole.as_ref().unwrap_or(&self.sections) {
            for index in matching(name)? {
                selected[index] = true;
            }
        }
        if let Some(name) = &self.from {
            let first = matching(name)?[0];
            let keep_sections = !self.sections.is_empty();
            for (index, flag) in selected.iter_mut().enumerate() {
                // With both options, --from trims the listed sections.
                *flag = index >= first && (!keep_sections || *flag);
            }
        }
        let indices: Vec<usize> = (0..segments.len())
            .filter(|index| selected[*index])
            .collect();
        if indices.is_empty() {
            return Err("the selection leaves no segment to play".to_string());
        }
        let mut ranges: Vec<(f64, f64)> = Vec::new();
        for &index in &indices {
            let (_, start, end) = segments[index];
            match ranges.last_mut() {
                Some(last) if (start - last.1).abs() <= 1e-9 => last.1 = end,
                _ => ranges.push((start, end)),
            }
        }
        Ok(ResolvedSelection {
            segments: indices,
            ranges,
        })
    }
}

/// Indices of the segments `name` selects.
fn matching_names(name: &str, segments: &[(&str, f64, f64)]) -> Vec<usize> {
    segments
        .iter()
        .enumerate()
        .filter(|(_, (segment, _, _))| segment_matches(segment, name))
        .map(|(index, _)| index)
        .collect()
}

fn segment_matches(segment: &str, name: &str) -> bool {
    let segment = segment.to_lowercase();
    let name = name.to_lowercase();
    segment == name
        || segment.starts_with(&format!("{name} · "))
        || step_title(&segment) == Some(name.as_str())
}

/// Title of a generated `Section` step segment (`"<key> · <visit> · <step> ·
/// <title>"`), which may itself contain ` · `.
fn step_title(segment: &str) -> Option<&str> {
    let mut parts = segment.splitn(4, " · ");
    parts.next()?;
    let visit = parts.next()?;
    let step = parts.next()?;
    let title = parts.next()?;
    (visit.parse::<usize>().is_ok() && step.parse::<usize>().is_ok()).then_some(title)
}

fn unknown_segment(name: &str, segments: &[(&str, f64, f64)]) -> String {
    let mut choices: Vec<String> = Vec::new();
    for (segment, _, _) in segments {
        // Offer section keys once instead of every generated segment name.
        let choice = match segment.split_once(" · ") {
            Some((key, rest)) if rest.split(" · ").count() >= 3 => key.to_string(),
            _ => segment.to_string(),
        };
        if !choices.contains(&choice) {
            choices.push(choice);
        }
    }
    // Quote the choices: a name may itself contain commas.
    let listed: Vec<String> = choices.iter().map(|choice| format!("`{choice}`")).collect();
    let hint = if choices.iter().any(|choice| choice.contains(',')) {
        "; to select a name with commas, pass it alone or write its commas as \\,"
    } else {
        ""
    };
    format!(
        "no segment or section named `{name}`; available: {}{hint}",
        listed.join(", ")
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn deck() -> Vec<(&'static str, f64, f64)> {
        vec![
            ("Portada", 0.0, 1.0),
            ("intro · 1 · 1 · Contexto", 1.0, 2.0),
            ("Resultados", 2.0, 3.0),
            ("close · 1 · 1 · Cierre", 3.0, 4.0),
            ("close · 1 · 2 · Preguntas", 4.0, 5.0),
        ]
    }

    #[test]
    fn section_lists_escape_commas_inside_names() {
        assert_eq!(
            SegmentSelection::parse_list(r"Tiempo\, lugar y orientación, Cierre").unwrap(),
            vec!["Tiempo, lugar y orientación", "Cierre"]
        );
        assert_eq!(
            SegmentSelection::parse_list(r"a\\b,c").unwrap(),
            vec![r"a\b", "c"]
        );
        assert!(SegmentSelection::parse_list("a,,b").is_err());
    }

    #[test]
    fn a_whole_value_naming_one_segment_needs_no_escape() {
        let deck = vec![
            ("Portada", 0.0, 1.0),
            ("Tiempo, lugar y orientación", 1.0, 2.0),
            ("Cierre", 2.0, 3.0),
        ];
        let mut selection = SegmentSelection::default();
        selection
            .set_sections("Tiempo, lugar y orientación")
            .unwrap();
        assert_eq!(selection.resolve(deck.clone()).unwrap().segments, vec![1]);
        // A real list still splits, and an escaped comma joins.
        selection
            .set_sections(r"Tiempo\, lugar y orientación, Cierre")
            .unwrap();
        assert_eq!(
            selection.resolve(deck.clone()).unwrap().segments,
            vec![1, 2]
        );
        selection.set_sections("Portada, Cierre").unwrap();
        assert_eq!(
            selection.resolve(deck.clone()).unwrap().segments,
            vec![0, 2]
        );
        // Parts of a comma name that match nothing are reported, with a hint.
        selection.set_sections("Tiempo, Cierre").unwrap();
        let error = selection.resolve(deck).unwrap_err();
        assert!(error.contains("`Tiempo, lugar y orientación`"), "{error}");
        assert!(error.contains(r"write its commas as \,"), "{error}");
    }

    #[test]
    fn sections_match_segment_names_and_section_keys_ignoring_case() {
        let selection = SegmentSelection {
            sections: vec!["resultados".into(), "CLOSE".into()],
            from: None,
            sections_value: None,
        };
        let resolved = selection.resolve(deck()).unwrap();
        assert_eq!(resolved.segments, vec![2, 3, 4]);
        assert_eq!(resolved.ranges, vec![(2.0, 5.0)]);
    }

    #[test]
    fn step_titles_select_their_segments_even_with_separators() {
        let deck = vec![
            ("p · 1 · 1 · Problemática · país sísmico", 0.0, 1.0),
            ("p · 1 · 2 · Problemática · traslado", 1.0, 2.0),
            ("cierre", 2.0, 3.0),
        ];
        let selection = SegmentSelection {
            sections: vec!["problemática · PAÍS SÍSMICO".into()],
            from: None,
            sections_value: None,
        };
        assert_eq!(selection.resolve(deck.clone()).unwrap().segments, vec![0]);
        let from = SegmentSelection {
            sections: Vec::new(),
            from: Some("Problemática · traslado".into()),
            sections_value: None,
        };
        assert_eq!(from.resolve(deck).unwrap().segments, vec![1, 2]);
    }

    #[test]
    fn non_adjacent_sections_keep_separate_ranges() {
        let selection = SegmentSelection {
            sections: vec!["intro".into(), "close".into()],
            from: None,
            sections_value: None,
        };
        let resolved = selection.resolve(deck()).unwrap();
        assert_eq!(resolved.ranges, vec![(1.0, 2.0), (3.0, 5.0)]);
    }

    #[test]
    fn from_plays_to_the_end_and_trims_listed_sections() {
        let from = SegmentSelection {
            sections: Vec::new(),
            from: Some("Resultados".into()),
            sections_value: None,
        };
        assert_eq!(from.resolve(deck()).unwrap().ranges, vec![(2.0, 5.0)]);
        let both = SegmentSelection {
            sections: vec!["intro".into(), "close".into()],
            from: Some("resultados".into()),
            sections_value: None,
        };
        assert_eq!(both.resolve(deck()).unwrap().segments, vec![3, 4]);
    }

    #[test]
    fn unknown_names_list_segments_and_section_keys() {
        let selection = SegmentSelection {
            sections: vec!["missing".into()],
            from: None,
            sections_value: None,
        };
        let error = selection.resolve(deck()).unwrap_err();
        assert!(error.contains("`missing`"), "{error}");
        assert!(
            error.contains("`Portada`, `intro`, `Resultados`, `close`"),
            "{error}"
        );
        assert!(SegmentSelection::parse_list("a,,b").is_err());
        assert_eq!(
            SegmentSelection::parse_list(" a , b ").unwrap(),
            vec!["a".to_string(), "b".to_string()]
        );
    }
}
