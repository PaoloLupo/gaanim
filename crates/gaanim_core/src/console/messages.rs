//! Machine-readable messages, `--message-format json`: every status line,
//! diagnostic and progress report as one JSON object per line on stdout,
//! for editors and other tools.
//!
//! Each object has a `type`: `message` (a status line, with `level`,
//! `label`, `message` and the script `file`, `line` and `column` it is
//! about, null when unknown), `detail`, `hint`, and the types a command
//! adds (`diagnostic`, `check`, `progress`, `reload`). Tracebacks still go
//! to stderr for people.

use std::io::Write;
use std::sync::atomic::{AtomicBool, Ordering};

use super::{Level, ScriptLocation};

static JSON: AtomicBool = AtomicBool::new(false);

/// Print messages as JSON lines on stdout instead of text (see the module).
pub fn set_message_format_json(json: bool) {
    JSON.store(json, Ordering::Relaxed);
}

/// Whether messages are JSON lines (see [`set_message_format_json`]).
pub fn message_format_json() -> bool {
    JSON.load(Ordering::Relaxed)
}

/// The value of `--message-format`: `json`, or `human` (the default).
/// Returns whether it asks for JSON.
pub fn parse_message_format(value: Option<&str>) -> Result<bool, String> {
    match value.map(str::trim) {
        Some("json") => Ok(true),
        Some("human") => Ok(false),
        Some(other) => Err(format!(
            "--message-format takes json or human, not `{other}`"
        )),
        None => Err("--message-format requires json or human".to_string()),
    }
}

/// A value of a message field.
#[derive(Clone, Debug, PartialEq)]
pub enum Json {
    Null,
    Bool(bool),
    Number(f64),
    Text(String),
}

impl From<&str> for Json {
    fn from(value: &str) -> Self {
        Self::Text(value.to_owned())
    }
}

impl From<String> for Json {
    fn from(value: String) -> Self {
        Self::Text(value)
    }
}

impl From<bool> for Json {
    fn from(value: bool) -> Self {
        Self::Bool(value)
    }
}

impl From<f64> for Json {
    fn from(value: f64) -> Self {
        Self::Number(value)
    }
}

macro_rules! json_from_integer {
    ($($integer:ty),*) => {$(
        impl From<$integer> for Json {
            fn from(value: $integer) -> Self {
                Self::Number(value as f64)
            }
        }
    )*};
}
json_from_integer!(u32, u64, usize, i64);

impl<T: Into<Json>> From<Option<T>> for Json {
    fn from(value: Option<T>) -> Self {
        value.map_or(Self::Null, Into::into)
    }
}

impl Json {
    fn write(&self, out: &mut String) {
        match self {
            Self::Null => out.push_str("null"),
            Self::Bool(value) => out.push_str(if *value { "true" } else { "false" }),
            Self::Number(value) if value.is_finite() => out.push_str(&value.to_string()),
            Self::Number(_) => out.push_str("null"),
            Self::Text(text) => write_string(text, out),
        }
    }
}

fn write_string(text: &str, out: &mut String) {
    out.push('"');
    for ch in text.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            ch if (ch as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", ch as u32)),
            ch => out.push(ch),
        }
    }
    out.push('"');
}

/// One JSON object with `fields`, in order.
pub fn json_object(fields: &[(&str, Json)]) -> String {
    let mut out = String::from("{");
    for (index, (key, value)) in fields.iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        write_string(key, &mut out);
        out.push(':');
        value.write(&mut out);
    }
    out.push('}');
    out
}

/// Print one JSON object with `fields` as a line on stdout.
pub fn emit_json(fields: &[(&str, Json)]) {
    let line = json_object(fields);
    let mut stdout = std::io::stdout().lock();
    let _ = writeln!(stdout, "{line}");
    let _ = stdout.flush();
}

/// The name of `level` in messages.
pub fn level_name(level: Level) -> &'static str {
    match level {
        Level::Debug => "debug",
        Level::Info => "info",
        Level::Success => "success",
        Level::Warn => "warning",
        Level::Error => "error",
    }
}

/// The `file`, `line` and `column` fields of a message about `location`.
pub fn location_fields(location: Option<&ScriptLocation>) -> [(&'static str, Json); 3] {
    [
        (
            "file",
            location
                .map(|location| location.file.to_string_lossy().into_owned())
                .into(),
        ),
        ("line", location.map(|location| location.line).into()),
        // Script lines carry no column yet.
        ("column", Json::Null),
    ]
}

/// The `message` object of a status line.
pub fn message_object(
    level: Level,
    label: &str,
    message: &str,
    location: Option<&ScriptLocation>,
) -> String {
    let mut fields = vec![
        ("type", Json::from("message")),
        ("level", level_name(level).into()),
        ("label", label.into()),
        ("message", message.into()),
    ];
    fields.extend(location_fields(location));
    json_object(&fields)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn objects_escape_text_and_write_numbers_as_json() {
        assert_eq!(
            json_object(&[
                ("type", "progress".into()),
                ("current", 30_u64.into()),
                ("percent", 12.5.into()),
                ("done", false.into()),
                ("missing", Option::<u32>::None.into()),
                ("nan", f64::NAN.into()),
            ]),
            r#"{"type":"progress","current":30,"percent":12.5,"done":false,"missing":null,"nan":null}"#
        );
        assert_eq!(
            json_object(&[("message", "a \"b\"\\c\nd\u{1}".into())]),
            r#"{"message":"a \"b\"\\c\nd\u0001"}"#
        );
    }

    #[test]
    fn status_lines_become_message_objects() {
        let location = ScriptLocation::new("main.py", 12);
        assert_eq!(
            message_object(Level::Warn, "python", "careful", Some(&location)),
            r#"{"type":"message","level":"warning","label":"python","message":"careful","file":"main.py","line":12,"column":null}"#
        );
        assert_eq!(parse_message_format(Some("json")), Ok(true));
        assert_eq!(parse_message_format(Some("human")), Ok(false));
        assert!(parse_message_format(Some("yaml")).is_err());
        assert!(parse_message_format(None).is_err());
    }
}
