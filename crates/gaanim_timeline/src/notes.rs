//! Speaker notes cut at their stop marks.
//!
//! A line `[stop]` or `[stop name]` in a slide's notes marks where the
//! speaker advances: the text before the n-th mark is what they say while
//! the slide rests at its n-th stop, and the text after the last mark is
//! what follows that stop. Notes come from `notes=` or from the slide's
//! section of the narration script; Presenter View's prompter and the live
//! take's teleprompter show them cut this way.

/// A mark that ends a part of the notes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StopMark {
    /// The stop it names, `grafico` in `[stop grafico]`.
    pub name: Option<String>,
}

/// One part of a slide's notes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotesPart {
    /// What the speaker says, possibly empty.
    pub text: String,
    /// The mark after it, where the speaker advances; `None` for the text
    /// after the last mark.
    pub mark: Option<StopMark>,
}

/// The stop a line marks: `Some(None)` for `[stop]`, `Some(Some(name))` for
/// `[stop name]` (or `[stop: name]`), and `None` when the line is not a mark.
/// `stop` ignores case.
pub fn stop_mark(line: &str) -> Option<Option<&str>> {
    let inner = line.trim().strip_prefix('[')?.strip_suffix(']')?.trim();
    let keyword = inner.get(..4)?;
    if !keyword.eq_ignore_ascii_case("stop") {
        return None;
    }
    let rest = &inner[4..];
    // `[stopwatch]` is text, not a mark.
    if !rest.is_empty() && !rest.starts_with(|c: char| c.is_whitespace() || c == ':') {
        return None;
    }
    let name = rest.trim_start_matches(':').trim();
    Some((!name.is_empty()).then_some(name))
}

/// The line that marks the stop `name`, as Gaanim writes it.
pub fn mark_line(name: Option<&str>) -> String {
    match name.map(str::trim).filter(|name| !name.is_empty()) {
        Some(name) => format!("[stop {name}]"),
        None => "[stop]".to_owned(),
    }
}

/// Cut `notes` at their marks: a part ending at each mark, then the text
/// after the last one. Notes without marks are a single part.
pub fn split_notes(notes: &str) -> Vec<NotesPart> {
    let mut parts = Vec::new();
    let mut lines: Vec<&str> = Vec::new();
    for line in notes.lines() {
        match stop_mark(line) {
            Some(name) => {
                parts.push(NotesPart {
                    text: lines.join("\n").trim().to_owned(),
                    mark: Some(StopMark {
                        name: name.map(str::to_owned),
                    }),
                });
                lines.clear();
            }
            None => lines.push(line),
        }
    }
    parts.push(NotesPart {
        text: lines.join("\n").trim().to_owned(),
        mark: None,
    });
    parts
}

/// The marks in `notes`, in order.
pub fn marks(notes: &str) -> Vec<StopMark> {
    notes
        .lines()
        .filter_map(stop_mark)
        .map(|name| StopMark {
            name: name.map(str::to_owned),
        })
        .collect()
}

/// `notes` without their marks: what the speaker reads aloud.
pub fn spoken(notes: &str) -> String {
    split_notes(notes)
        .into_iter()
        .map(|part| part.text)
        .filter(|text| !text.is_empty())
        .collect::<Vec<_>>()
        .join("\n\n")
}

/// `notes` for print, where each mark becomes an arrow: `→ name`.
pub fn printable(notes: &str) -> String {
    notes
        .lines()
        .map(|line| match stop_mark(line) {
            Some(Some(name)) => format!("→ {name}"),
            Some(None) => "→".to_owned(),
            None => line.to_owned(),
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn marks_are_stop_lines_with_an_optional_name() {
        assert_eq!(stop_mark("[stop]"), Some(None));
        assert_eq!(stop_mark("  [STOP grafico] "), Some(Some("grafico")));
        assert_eq!(stop_mark("[stop: la tabla]"), Some(Some("la tabla")));
        assert_eq!(stop_mark("[stop ]"), Some(None));
        assert_eq!(stop_mark("[stopwatch]"), None);
        assert_eq!(stop_mark("Pulsa [stop] aquí"), None);
        assert_eq!(stop_mark("[sto]"), None);
        assert_eq!(stop_mark("[ñandú]"), None);
        assert_eq!(mark_line(Some(" grafico ")), "[stop grafico]");
        assert_eq!(mark_line(None), "[stop]");
        assert_eq!(mark_line(Some("")), "[stop]");
    }

    #[test]
    fn notes_split_at_their_marks() {
        let notes =
            "Miren el gráfico.\n\n[stop grafico]\nCrece cada año.\n[stop]\n\nY la conclusión.";
        let parts = split_notes(notes);
        assert_eq!(parts.len(), 3);
        assert_eq!(parts[0].text, "Miren el gráfico.");
        assert_eq!(
            parts[0].mark.as_ref().unwrap().name.as_deref(),
            Some("grafico")
        );
        assert_eq!(parts[1].text, "Crece cada año.");
        assert_eq!(parts[1].mark.as_ref().unwrap().name, None);
        assert_eq!(parts[2].text, "Y la conclusión.");
        assert_eq!(parts[2].mark, None);
        assert_eq!(marks(notes).len(), 2);
        assert_eq!(
            spoken(notes),
            "Miren el gráfico.\n\nCrece cada año.\n\nY la conclusión."
        );
        assert_eq!(
            printable("Uno\n[stop grafico]\nDos\n[stop]"),
            "Uno\n→ grafico\nDos\n→"
        );
        let single = split_notes("Sin marcas.");
        assert_eq!(single.len(), 1);
        assert_eq!(single[0].text, "Sin marcas.");
        // A mark that opens the notes leaves an empty part before it.
        assert_eq!(split_notes("[stop]\nDespués")[0].text, "");
    }
}
