//! Narration takes: where a take lives, its sidecar of markers and word
//! timings, and how a named marker resolves to seconds inside the take.
//!
//! A take `<key>` is an audio file `narration/<key>.<ext>` plus an optional
//! JSON sidecar `narration/<key>.json`. The editor's recorder writes both;
//! Whisper transcripts and hand edits only touch the sidecar. Scripts never
//! carry durations: they name markers, and this module finds them.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Mutex, OnceLock};
use std::time::SystemTime;

use serde::{Deserialize, Serialize};

/// Folder, inside the scene's asset directory, that holds narration takes.
pub const NARRATION_DIR: &str = "narration";
/// Current sidecar format.
pub const SIDECAR_VERSION: u32 = 1;
/// Speaking rate assumed for text that has no recorded take (≈150 words/min).
pub const ESTIMATED_WORDS_PER_SECOND: f64 = 2.5;
/// Breath added after the estimated speech of an unrecorded take.
const ESTIMATE_TAIL: f64 = 0.4;
/// Length of an unrecorded take without any text to estimate from.
pub const PLACEHOLDER_DURATION: f64 = 2.0;
/// Audio formats accepted as takes, in lookup order. The recorder writes WAV.
const TAKE_EXTENSIONS: [&str; 7] = ["wav", "flac", "mp3", "m4a", "aac", "ogg", "opus"];

#[derive(Debug, thiserror::Error)]
pub enum NarrationError {
    #[error(
        "narration key {key:?} must be 1-80 letters, digits, '-', '_' or '.' and cannot start with '.'"
    )]
    InvalidKey { key: String },
    #[error("could not read narration sidecar '{path}': {message}")]
    Sidecar { path: PathBuf, message: String },
    #[error("could not read the duration of narration take '{path}': {message}")]
    Duration { path: PathBuf, message: String },
    #[error("could not write '{path}': {message}")]
    Write { path: PathBuf, message: String },
}

/// Reject keys that are not a plain, portable file stem.
pub fn validate_take_key(key: &str) -> Result<(), NarrationError> {
    let valid = !key.is_empty()
        && key.chars().count() <= 80
        && !key.starts_with('.')
        && key
            .chars()
            .all(|character| character.is_alphanumeric() || matches!(character, '-' | '_' | '.'));
    if valid {
        Ok(())
    } else {
        Err(NarrationError::InvalidKey {
            key: key.to_owned(),
        })
    }
}

/// Files of one take inside a narration directory.
#[derive(Debug, Clone, PartialEq)]
pub struct TakeFiles {
    /// The existing audio file, if the take was recorded.
    pub audio: Option<PathBuf>,
    /// Where the recorder writes a new take.
    pub destination: PathBuf,
    /// Markers, transcript and live holds.
    pub sidecar: PathBuf,
}

impl TakeFiles {
    pub fn locate(directory: &Path, key: &str) -> Self {
        let audio = TAKE_EXTENSIONS
            .iter()
            .map(|extension| directory.join(format!("{key}.{extension}")))
            .find(|path| path.is_file());
        Self {
            audio,
            destination: directory.join(format!("{key}.wav")),
            sidecar: directory.join(format!("{key}.json")),
        }
    }
}

/// One transcribed word, in seconds from the start of its take.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TakeWord {
    pub text: String,
    pub start: f64,
    pub end: f64,
}

/// Editable timing data stored beside a take as `<key>.json`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TakeSidecar {
    #[serde(default = "sidecar_version")]
    pub version: u32,
    /// Markers tapped while recording or edited by hand, in take seconds.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub markers: BTreeMap<String, f64>,
    /// Word timings from a transcription of this take.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub words: Vec<TakeWord>,
    /// Live takes only: seconds spent at each stop, in timeline order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub holds: Vec<f64>,
    /// Integrated loudness, in LUFS, the take was leveled to.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub loudness: Option<f64>,
}

fn sidecar_version() -> u32 {
    SIDECAR_VERSION
}

impl Default for TakeSidecar {
    fn default() -> Self {
        Self {
            version: SIDECAR_VERSION,
            markers: BTreeMap::new(),
            words: Vec::new(),
            holds: Vec::new(),
            loudness: None,
        }
    }
}

impl TakeSidecar {
    /// Read a sidecar; a missing file is an empty sidecar.
    pub fn load(path: &Path) -> Result<Self, NarrationError> {
        if !path.is_file() {
            return Ok(Self::default());
        }
        let error = |message: String| NarrationError::Sidecar {
            path: path.to_path_buf(),
            message,
        };
        let source = std::fs::read_to_string(path).map_err(|e| error(e.to_string()))?;
        let sidecar: Self = serde_json::from_str(&source).map_err(|e| error(e.to_string()))?;
        let finite = |value: f64| value.is_finite() && value >= 0.0;
        if !sidecar.markers.values().all(|time| finite(*time))
            || !sidecar.holds.iter().all(|hold| finite(*hold))
            || !sidecar
                .words
                .iter()
                .all(|word| finite(word.start) && finite(word.end))
        {
            return Err(error(
                "times must be finite non-negative seconds".to_string(),
            ));
        }
        Ok(sidecar)
    }

    /// Write the sidecar through a temporary file, so a watcher never reads
    /// a half-written JSON document.
    pub fn save(&self, path: &Path) -> Result<(), NarrationError> {
        let source = serde_json::to_string_pretty(self).map_err(|e| NarrationError::Write {
            path: path.to_path_buf(),
            message: e.to_string(),
        })?;
        write_atomically(path, source.as_bytes())
    }
}

/// Replace `path` with `bytes` via a `.tmp` sibling that file watchers ignore.
pub fn write_atomically(path: &Path, bytes: &[u8]) -> Result<(), NarrationError> {
    let error = |message: String| NarrationError::Write {
        path: path.to_path_buf(),
        message,
    };
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| error(e.to_string()))?;
    }
    let temporary = temporary_sibling(path);
    std::fs::write(&temporary, bytes).map_err(|e| error(e.to_string()))?;
    std::fs::rename(&temporary, path).map_err(|e| {
        let _ = std::fs::remove_file(&temporary);
        error(e.to_string())
    })
}

/// `take.wav` → `take.wav.tmp`.
pub fn temporary_sibling(path: &Path) -> PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(".tmp");
    path.with_file_name(name)
}

/// Write mono samples as a 16-bit PCM WAV take, atomically.
pub fn write_wav_take(
    path: &Path,
    samples: &[f32],
    sample_rate: u32,
) -> Result<(), NarrationError> {
    let error = |message: String| NarrationError::Write {
        path: path.to_path_buf(),
        message,
    };
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| error(e.to_string()))?;
    }
    let temporary = temporary_sibling(path);
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let write = || -> Result<(), hound::Error> {
        let mut writer = hound::WavWriter::create(&temporary, spec)?;
        for sample in samples {
            let value = (sample.clamp(-1.0, 1.0) * i16::MAX as f32).round() as i16;
            writer.write_sample(value)?;
        }
        writer.finalize()
    };
    if let Err(e) = write() {
        let _ = std::fs::remove_file(&temporary);
        return Err(error(e.to_string()));
    }
    std::fs::rename(&temporary, path).map_err(|e| {
        let _ = std::fs::remove_file(&temporary);
        error(e.to_string())
    })
}

type DurationKey = (PathBuf, u64, Option<SystemTime>);

fn duration_cache() -> &'static Mutex<HashMap<DurationKey, f64>> {
    static CACHE: OnceLock<Mutex<HashMap<DurationKey, f64>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Duration of a take in seconds. WAV headers are read directly; other
/// formats use ffprobe. Results are cached per file size and modification.
pub fn audio_duration(path: &Path) -> Result<f64, NarrationError> {
    let error = |message: String| NarrationError::Duration {
        path: path.to_path_buf(),
        message,
    };
    let metadata = std::fs::metadata(path).map_err(|e| error(e.to_string()))?;
    let key = (path.to_path_buf(), metadata.len(), metadata.modified().ok());
    if let Some(duration) = duration_cache()
        .lock()
        .expect("narration duration cache poisoned")
        .get(&key)
    {
        return Ok(*duration);
    }
    let is_wav = path
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("wav"));
    let duration = if is_wav {
        let reader = hound::WavReader::open(path).map_err(|e| error(e.to_string()))?;
        let spec = reader.spec();
        if spec.sample_rate == 0 {
            return Err(error("the WAV header has no sample rate".to_string()));
        }
        reader.duration() as f64 / spec.sample_rate as f64
    } else {
        let output = Command::new("ffprobe")
            .args([
                "-v",
                "error",
                "-show_entries",
                "format=duration",
                "-of",
                "default=noprint_wrappers=1:nokey=1",
            ])
            .arg(path)
            .output()
            .map_err(|e| error(format!("could not run ffprobe: {e}")))?;
        if !output.status.success() {
            return Err(error(
                String::from_utf8_lossy(&output.stderr).trim().to_string(),
            ));
        }
        String::from_utf8_lossy(&output.stdout)
            .trim()
            .parse::<f64>()
            .map_err(|e| error(e.to_string()))?
    };
    if !duration.is_finite() || duration <= 0.0 {
        return Err(error("the take is empty".to_string()));
    }
    duration_cache()
        .lock()
        .expect("narration duration cache poisoned")
        .insert(key, duration);
    Ok(duration)
}

/// Lowercase a word and drop accents and punctuation, so `"¿Derivada?"`
/// matches the marker `"derivada"`.
pub fn normalize_word(word: &str) -> String {
    word.chars()
        .flat_map(char::to_lowercase)
        .map(fold_accent)
        .filter(|character| character.is_alphanumeric())
        .collect()
}

fn fold_accent(character: char) -> char {
    match character {
        'á' | 'à' | 'ä' | 'â' | 'ã' | 'å' => 'a',
        'é' | 'è' | 'ë' | 'ê' => 'e',
        'í' | 'ì' | 'ï' | 'î' => 'i',
        'ó' | 'ò' | 'ö' | 'ô' | 'õ' => 'o',
        'ú' | 'ù' | 'ü' | 'û' => 'u',
        'ñ' => 'n',
        'ç' => 'c',
        other => other,
    }
}

/// Normalized words of a text, without empty tokens.
pub fn text_words(text: &str) -> Vec<String> {
    text.split_whitespace()
        .map(normalize_word)
        .filter(|word| !word.is_empty())
        .collect()
}

/// First index at or after `from` where `phrase` occurs in `words`.
pub fn find_phrase(words: &[String], phrase: &[String], from: usize) -> Option<usize> {
    if phrase.is_empty() || phrase.len() > words.len() {
        return None;
    }
    (from..=words.len() - phrase.len()).find(|&index| words[index..index + phrase.len()] == *phrase)
}

/// Length of an unrecorded take, estimated from its script.
pub fn estimate_duration(text: Option<&str>) -> f64 {
    let words = text.map(text_words).unwrap_or_default();
    if words.is_empty() {
        PLACEHOLDER_DURATION
    } else {
        words.len() as f64 / ESTIMATED_WORDS_PER_SECOND + ESTIMATE_TAIL
    }
}

/// Where a marker's time came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MarkerSource {
    /// Tapped while recording, or written into the sidecar by hand.
    Tapped,
    /// Found in the take's transcript.
    Transcript,
    /// Proportional position of the marker's words in the script.
    Estimated,
    /// Not found anywhere; the script continues without waiting.
    Missing,
}

impl MarkerSource {
    pub fn label(self) -> &'static str {
        match self {
            Self::Tapped => "tapped",
            Self::Transcript => "transcript",
            Self::Estimated => "estimated",
            Self::Missing => "missing",
        }
    }
}

/// Timing of one take while a script resolves its markers in order.
#[derive(Debug, Clone, PartialEq)]
pub struct TakeTiming {
    /// Whether an audio file backs this take.
    pub recorded: bool,
    /// Take length in seconds: measured when recorded, estimated otherwise.
    pub duration: f64,
    /// Whether word timings are available.
    pub transcribed: bool,
    markers: BTreeMap<String, f64>,
    transcript: Vec<(String, f64)>,
    text_words: Vec<String>,
    text_cursor: usize,
    transcript_cursor: usize,
    last_offset: f64,
}

impl TakeTiming {
    /// `recorded_duration` is `Some` when an audio file exists. Tapped
    /// markers and words describe that file, so they are ignored without it.
    pub fn new(recorded_duration: Option<f64>, sidecar: TakeSidecar, text: Option<&str>) -> Self {
        let recorded = recorded_duration.is_some();
        let duration = recorded_duration.unwrap_or_else(|| estimate_duration(text));
        let (markers, words) = if recorded {
            (sidecar.markers, sidecar.words)
        } else {
            (BTreeMap::new(), Vec::new())
        };
        let transcript: Vec<(String, f64)> = words
            .iter()
            .flat_map(|word| {
                text_words(&word.text)
                    .into_iter()
                    .map(move |token| (token, word.start))
            })
            .collect();
        Self {
            recorded,
            duration,
            transcribed: !transcript.is_empty(),
            markers,
            transcript,
            text_words: text.map(text_words).unwrap_or_default(),
            text_cursor: 0,
            transcript_cursor: 0,
            last_offset: 0.0,
        }
    }

    /// Resolve the next marker, in take seconds. Markers are searched in the
    /// order the script asks for them, so a repeated word matches the
    /// occurrence after the previous marker.
    pub fn resolve(&mut self, name: &str) -> (Option<f64>, MarkerSource) {
        let phrase = text_words(name);
        let tapped = self.markers.get(name).copied().or_else(|| {
            let normalized = phrase.concat();
            self.markers
                .iter()
                .find(|(key, _)| text_words(key).concat() == normalized)
                .map(|(_, time)| *time)
        });
        let text_hit = find_phrase(&self.text_words, &phrase, self.text_cursor);
        if let Some(index) = text_hit {
            self.text_cursor = index + phrase.len();
        }
        let resolved = if let Some(time) = tapped {
            Some((time, MarkerSource::Tapped))
        } else if let Some(index) = self.transcript_match(&phrase) {
            self.transcript_cursor = index + phrase.len();
            Some((self.transcript[index].1, MarkerSource::Transcript))
        } else {
            text_hit
                .filter(|_| !self.text_words.is_empty())
                .map(|index| {
                    let fraction = index as f64 / self.text_words.len() as f64;
                    let speech = if self.recorded {
                        self.duration
                    } else {
                        (self.duration - ESTIMATE_TAIL).max(0.0)
                    };
                    (speech * fraction, MarkerSource::Estimated)
                })
        };
        match resolved {
            Some((time, source)) => {
                let time = time.clamp(0.0, self.duration);
                self.last_offset = self.last_offset.max(time);
                (Some(time), source)
            }
            None => (None, MarkerSource::Missing),
        }
    }

    /// Index of `phrase` in the transcript after the previous match and at or
    /// after the latest resolved time.
    fn transcript_match(&self, phrase: &[String]) -> Option<usize> {
        if phrase.is_empty() || self.transcript.is_empty() {
            return None;
        }
        let tokens: Vec<String> = self.transcript.iter().map(|(t, _)| t.clone()).collect();
        let after_time = self
            .transcript
            .iter()
            .position(|(_, start)| *start + 1e-6 >= self.last_offset)
            .unwrap_or(tokens.len());
        find_phrase(&tokens, phrase, after_time.max(self.transcript_cursor))
    }
}

/// Narration script loaded automatically from the narration folder.
pub const DEFAULT_SCRIPT: &str = "script.md";

/// The text of one take in a narration script.
#[derive(Debug, Clone, PartialEq)]
pub struct ScriptSection {
    pub key: String,
    pub text: String,
    /// 1-based line of the section heading.
    pub line: usize,
}

/// Parse a narration script written in Markdown.
///
/// A level-2 heading names a take (`## intro`) and the text below it is that
/// take's script. A level-1 heading (`# Title`) ends the current take;
/// deeper headings are notes for the author and are not read. HTML comments
/// and `*` emphasis marks are dropped. Lines join with spaces and blank lines
/// separate paragraphs. An invalid or repeated key is an error.
pub fn parse_script(source: &str) -> Result<Vec<ScriptSection>, String> {
    let mut sections: Vec<ScriptSection> = Vec::new();
    let mut current: Option<(String, usize, Vec<String>)> = None;
    let mut paragraph = String::new();
    let mut in_comment = false;

    fn flush_paragraph(paragraph: &mut String, current: &mut Option<(String, usize, Vec<String>)>) {
        if let Some((_, _, paragraphs)) = current
            && !paragraph.trim().is_empty()
        {
            paragraphs.push(paragraph.trim().to_owned());
        }
        paragraph.clear();
    }
    fn close(
        current: &mut Option<(String, usize, Vec<String>)>,
        sections: &mut Vec<ScriptSection>,
    ) {
        if let Some((key, line, paragraphs)) = current.take() {
            sections.push(ScriptSection {
                key,
                text: paragraphs.join("\n\n"),
                line,
            });
        }
    }

    for (index, raw) in source.lines().enumerate() {
        // Drop HTML comments, which may span lines.
        let mut line = String::new();
        let mut rest = raw;
        loop {
            if in_comment {
                match rest.find("-->") {
                    Some(end) => {
                        rest = &rest[end + 3..];
                        in_comment = false;
                    }
                    None => break,
                }
            } else {
                match rest.find("<!--") {
                    Some(start) => {
                        line.push_str(&rest[..start]);
                        rest = &rest[start + 4..];
                        in_comment = true;
                    }
                    None => {
                        line.push_str(rest);
                        break;
                    }
                }
            }
        }
        let trimmed = line.trim();
        let level = trimmed
            .chars()
            .take_while(|character| *character == '#')
            .count();
        // Like Markdown, `#word` is text; a heading needs a space after its marks.
        let heading = level > 0
            && trimmed[level..]
                .chars()
                .next()
                .is_none_or(char::is_whitespace);
        if heading {
            if level > 2 {
                continue;
            }
            flush_paragraph(&mut paragraph, &mut current);
            close(&mut current, &mut sections);
            if level == 1 {
                continue;
            }
            let key = trimmed[level..].trim().trim_end_matches('#').trim();
            let line_number = index + 1;
            if validate_take_key(key).is_err() {
                return Err(format!(
                    "line {line_number}: {key:?} is not a take key; use letters, digits, '-', '_' or '.'"
                ));
            }
            if let Some(first) = sections.iter().find(|section| section.key == key) {
                return Err(format!(
                    "the key {key:?} appears twice (lines {} and {line_number})",
                    first.line
                ));
            }
            current = Some((key.to_owned(), line_number, Vec::new()));
        } else if trimmed.is_empty() {
            flush_paragraph(&mut paragraph, &mut current);
        } else if current.is_some() {
            if !paragraph.is_empty() {
                paragraph.push(' ');
            }
            paragraph.push_str(&trimmed.replace('*', ""));
        }
    }
    flush_paragraph(&mut paragraph, &mut current);
    close(&mut current, &mut sections);
    Ok(sections)
}

/// Markdown for a script with one section per `(key, text)`, as the editor
/// writes it the first time.
pub fn format_script(sections: &[(String, String)]) -> String {
    let mut source = String::from(
        "# Guion\n\n<!-- Cada encabezado \"## clave\" es el texto de scene.voiceover(\"clave\").\n     \
         Guarda el archivo y la escena se actualiza. -->\n",
    );
    for (key, text) in sections {
        source.push_str(&format!("\n## {key}\n\n"));
        if !text.trim().is_empty() {
            source.push_str(text.trim());
            source.push('\n');
        }
    }
    source
}

/// Parse the JSON that `whisper-cli -oj` writes. Segments holding several
/// words (when word splitting was not requested) share their time span
/// evenly between those words.
pub fn parse_whisper_json(source: &str) -> Result<Vec<TakeWord>, String> {
    let document: serde_json::Value =
        serde_json::from_str(source).map_err(|error| error.to_string())?;
    let segments = document
        .get("transcription")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| "missing \"transcription\" array".to_string())?;
    let mut words = Vec::new();
    for segment in segments {
        let text = segment
            .get("text")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default();
        let offset = |edge: &str| {
            segment
                .get("offsets")
                .and_then(|offsets| offsets.get(edge))
                .and_then(serde_json::Value::as_f64)
                .map(|milliseconds| milliseconds / 1000.0)
        };
        let (Some(from), Some(to)) = (offset("from"), offset("to")) else {
            return Err("a segment has no millisecond offsets".to_string());
        };
        let tokens: Vec<&str> = text
            .split_whitespace()
            .filter(|token| !normalize_word(token).is_empty())
            .collect();
        let span = (to - from).max(0.0) / tokens.len().max(1) as f64;
        for (index, token) in tokens.iter().enumerate() {
            let start = from + span * index as f64;
            words.push(TakeWord {
                text: (*token).to_owned(),
                start,
                end: start + span,
            });
        }
    }
    Ok(words)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words(pairs: &[(&str, f64)]) -> Vec<TakeWord> {
        pairs
            .iter()
            .map(|(text, start)| TakeWord {
                text: (*text).to_owned(),
                start: *start,
                end: start + 0.2,
            })
            .collect()
    }

    #[test]
    fn keys_are_portable_file_stems() {
        for key in ["intro", "escena-2", "a_b.c", "introducción"] {
            assert!(validate_take_key(key).is_ok(), "{key}");
        }
        for key in [
            "",
            ".hidden",
            "a/b",
            "a\\b",
            "dos palabras",
            &"x".repeat(81),
        ] {
            assert!(validate_take_key(key).is_err(), "{key}");
        }
    }

    #[test]
    fn words_ignore_case_accents_and_punctuation() {
        assert_eq!(normalize_word("¿Derivada?"), "derivada");
        assert_eq!(normalize_word("Función,"), "funcion");
        assert_eq!(
            text_words("Hoy vemos  la  PENDIENTE."),
            ["hoy", "vemos", "la", "pendiente"]
        );
        let haystack = text_words("la recta y la pendiente de la recta");
        assert_eq!(find_phrase(&haystack, &text_words("la recta"), 0), Some(0));
        assert_eq!(find_phrase(&haystack, &text_words("la recta"), 1), Some(6));
        assert_eq!(find_phrase(&haystack, &text_words("tangente"), 0), None);
    }

    #[test]
    fn unrecorded_takes_are_estimated_from_their_script() {
        assert_eq!(estimate_duration(None), PLACEHOLDER_DURATION);
        let text = "uno dos tres cuatro cinco seis siete ocho nueve diez";
        let duration = estimate_duration(Some(text));
        assert!((duration - (10.0 / ESTIMATED_WORDS_PER_SECOND + ESTIMATE_TAIL)).abs() < 1e-9);

        let mut timing = TakeTiming::new(None, TakeSidecar::default(), Some(text));
        assert!(!timing.recorded);
        let (time, source) = timing.resolve("seis");
        assert_eq!(source, MarkerSource::Estimated);
        assert!((time.unwrap() - 4.0 * 0.5).abs() < 1e-9);
        assert_eq!(timing.resolve("once"), (None, MarkerSource::Missing));
    }

    #[test]
    fn tapped_markers_win_then_transcript_then_script_position() {
        let mut sidecar = TakeSidecar::default();
        sidecar.markers.insert("recta".into(), 1.25);
        sidecar.words = words(&[
            ("La", 0.1),
            ("recta,", 0.4),
            ("su", 2.0),
            ("pendiente", 2.3),
            ("y", 3.0),
            ("otra", 3.1),
            ("pendiente.", 3.6),
        ]);
        let text = "La recta, su pendiente y otra pendiente. Fin del ejemplo";
        let mut timing = TakeTiming::new(Some(8.0), sidecar, Some(text));
        assert!(timing.recorded && timing.transcribed);
        assert_eq!(timing.resolve("recta"), (Some(1.25), MarkerSource::Tapped));
        assert_eq!(
            timing.resolve("Pendiente"),
            (Some(2.3), MarkerSource::Transcript)
        );
        // The repeated word matches the occurrence after the previous marker.
        assert_eq!(
            timing.resolve("pendiente"),
            (Some(3.6), MarkerSource::Transcript)
        );
        // Not spoken in the transcript: proportional position in the script.
        let (time, source) = timing.resolve("fin");
        assert_eq!(source, MarkerSource::Estimated);
        assert!((time.unwrap() - 8.0 * 7.0 / 10.0).abs() < 1e-9);
    }

    #[test]
    fn sidecar_data_is_ignored_without_its_take() {
        let mut sidecar = TakeSidecar::default();
        sidecar.markers.insert("recta".into(), 9.0);
        let timing = TakeTiming::new(None, sidecar, Some("la recta"));
        assert!(!timing.recorded);
        let mut timing = timing;
        assert_eq!(timing.resolve("recta").1, MarkerSource::Estimated);
    }

    #[test]
    fn sidecars_round_trip_atomically_and_reject_bad_times() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("narration/intro.json");
        let mut sidecar = TakeSidecar::default();
        sidecar.markers.insert("recta".into(), 1.5);
        sidecar.holds = vec![2.0, 0.5];
        sidecar.save(&path).unwrap();
        assert!(!temporary_sibling(&path).exists());
        assert_eq!(TakeSidecar::load(&path).unwrap(), sidecar);
        assert_eq!(
            TakeSidecar::load(&directory.path().join("missing.json")).unwrap(),
            TakeSidecar::default()
        );
        std::fs::write(&path, r#"{"markers": {"x": -1.0}}"#).unwrap();
        assert!(TakeSidecar::load(&path).is_err());
    }

    #[test]
    fn recorded_wav_takes_report_their_duration() {
        let directory = tempfile::tempdir().unwrap();
        let files = TakeFiles::locate(directory.path(), "intro");
        assert_eq!(files.audio, None);
        let samples = vec![0.25_f32; 24_000];
        write_wav_take(&files.destination, &samples, 16_000).unwrap();
        let files = TakeFiles::locate(directory.path(), "intro");
        assert_eq!(files.audio.as_deref(), Some(files.destination.as_path()));
        assert!((audio_duration(&files.destination).unwrap() - 1.5).abs() < 1e-9);
    }

    #[test]
    fn whisper_json_becomes_word_timings() {
        let source = r#"{
            "transcription": [
                {"offsets": {"from": 0, "to": 320}, "text": " Hoy"},
                {"offsets": {"from": 320, "to": 900}, "text": " la derivada."},
                {"offsets": {"from": 900, "to": 950}, "text": " ,"}
            ]
        }"#;
        let words = parse_whisper_json(source).unwrap();
        let texts: Vec<&str> = words.iter().map(|word| word.text.as_str()).collect();
        assert_eq!(texts, ["Hoy", "la", "derivada."]);
        assert!((words[1].start - 0.32).abs() < 1e-9);
        assert!((words[2].start - 0.61).abs() < 1e-9);
        assert!(parse_whisper_json("{}").is_err());
    }

    #[test]
    fn scripts_map_level_two_headings_to_takes() {
        let source = "# Mi video\n\
                      Notas sueltas que no se leen.\n\
                      \n\
                      ## intro\n\
                      Hoy vemos **qué es**\n\
                      una derivada. <!-- pausa -->\n\
                      \n\
                      ### nota para mí: sonreír\n\
                      Segundo párrafo.\n\
                      <!-- varias\n\
                      líneas -->\n\
                      ## cierre ##\n\
                      #hashtag al final\n";
        let sections = parse_script(source).unwrap();
        assert_eq!(sections.len(), 2);
        assert_eq!(sections[0].key, "intro");
        assert_eq!(sections[0].line, 4);
        assert_eq!(
            sections[0].text,
            "Hoy vemos qué es una derivada.\n\nSegundo párrafo."
        );
        assert_eq!(sections[1].key, "cierre");
        assert_eq!(sections[1].text, "#hashtag al final");
    }

    #[test]
    fn scripts_reject_invalid_and_repeated_keys() {
        let error = parse_script("## mi intro\ntexto").unwrap_err();
        assert!(error.contains("line 1"), "{error}");
        let error = parse_script("## a\nuno\n## a\ndos").unwrap_err();
        assert!(error.contains("twice") && error.contains("3"), "{error}");
    }

    #[test]
    fn formatted_scripts_parse_back_to_their_sections() {
        let sections = vec![
            ("intro".to_string(), "Hoy vemos la derivada.".to_string()),
            ("vacio".to_string(), String::new()),
        ];
        let parsed = parse_script(&format_script(&sections)).unwrap();
        let pairs: Vec<(String, String)> = parsed
            .into_iter()
            .map(|section| (section.key, section.text))
            .collect();
        assert_eq!(pairs, sections);
    }
}
