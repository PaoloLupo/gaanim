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
}

/// Selected segment indices and the merged time ranges they cover.
#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedSelection {
    pub segments: Vec<usize>,
    pub ranges: Vec<(f64, f64)>,
}

impl SegmentSelection {
    /// Parse a comma-separated `--sections` list.
    pub fn parse_list(spec: &str) -> Result<Vec<String>, String> {
        let names: Vec<String> = spec
            .split(',')
            .map(|name| name.trim().to_string())
            .collect();
        if names.iter().any(String::is_empty) {
            return Err(format!(
                "invalid section list `{spec}`: use comma-separated names, e.g. results,conclusions"
            ));
        }
        Ok(names)
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
            let found: Vec<usize> = segments
                .iter()
                .enumerate()
                .filter(|(_, (segment, _, _))| segment_matches(segment, name))
                .map(|(index, _)| index)
                .collect();
            if found.is_empty() {
                return Err(unknown_segment(name, &segments));
            }
            Ok(found)
        };
        let mut selected = vec![false; segments.len()];
        for name in &self.sections {
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
    format!(
        "no segment or section named `{name}`; available: {}",
        choices.join(", ")
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
    fn sections_match_segment_names_and_section_keys_ignoring_case() {
        let selection = SegmentSelection {
            sections: vec!["resultados".into(), "CLOSE".into()],
            from: None,
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
        };
        assert_eq!(selection.resolve(deck.clone()).unwrap().segments, vec![0]);
        let from = SegmentSelection {
            sections: Vec::new(),
            from: Some("Problemática · traslado".into()),
        };
        assert_eq!(from.resolve(deck).unwrap().segments, vec![1, 2]);
    }

    #[test]
    fn non_adjacent_sections_keep_separate_ranges() {
        let selection = SegmentSelection {
            sections: vec!["intro".into(), "close".into()],
            from: None,
        };
        let resolved = selection.resolve(deck()).unwrap();
        assert_eq!(resolved.ranges, vec![(1.0, 2.0), (3.0, 5.0)]);
    }

    #[test]
    fn from_plays_to_the_end_and_trims_listed_sections() {
        let from = SegmentSelection {
            sections: Vec::new(),
            from: Some("Resultados".into()),
        };
        assert_eq!(from.resolve(deck()).unwrap().ranges, vec![(2.0, 5.0)]);
        let both = SegmentSelection {
            sections: vec!["intro".into(), "close".into()],
            from: Some("resultados".into()),
        };
        assert_eq!(both.resolve(deck()).unwrap().segments, vec![3, 4]);
    }

    #[test]
    fn unknown_names_list_segments_and_section_keys() {
        let selection = SegmentSelection {
            sections: vec!["missing".into()],
            from: None,
        };
        let error = selection.resolve(deck()).unwrap_err();
        assert!(error.contains("`missing`"), "{error}");
        assert!(
            error.contains("Portada, intro, Resultados, close"),
            "{error}"
        );
        assert!(SegmentSelection::parse_list("a,,b").is_err());
        assert_eq!(
            SegmentSelection::parse_list(" a , b ").unwrap(),
            vec!["a".to_string(), "b".to_string()]
        );
    }
}
