//! The user's settings: one `settings.json` in Gaanim's data folder, shared
//! by every project, with one section per area (`narration`, `prompter`,
//! `preview`, `integrations`, `polls`, `updates`). Each crate owns the type
//! of its sections and reads and writes them by name; sections it does not
//! know are kept as they are.
//!
//! Settings that older versions kept in files of their own (`narration.json`
//! and `poll-relay.txt`) move into their section the first time the file is
//! read, and those files are removed.

use std::path::{Path, PathBuf};

use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::{Map, Value};

/// The settings file in the user data folder.
pub const FILE: &str = "settings.json";

/// Files older versions kept settings in, and the section each moves to.
const LEGACY_NARRATION: &str = "narration.json";
const LEGACY_RELAY: &str = "poll-relay.txt";

/// Where the settings are saved, if the system has a user data folder.
pub fn path() -> Option<PathBuf> {
    crate::user_data_dir().map(|dir| dir.join(FILE))
}

/// The `section` of the user's settings, or its defaults when it is missing,
/// cannot be read or does not match `T`.
pub fn load<T: DeserializeOwned + Default>(section: &str) -> T {
    crate::user_data_dir()
        .map(|dir| load_in(&dir, section))
        .unwrap_or_default()
}

/// Save `value` as the `section` of the user's settings, keeping the others.
pub fn save<T: Serialize>(section: &str, value: &T) -> Result<(), String> {
    let dir = crate::user_data_dir().ok_or("could not find the user data directory")?;
    save_in(&dir, section, value)
}

pub(crate) fn load_in<T: DeserializeOwned + Default>(dir: &Path, section: &str) -> T {
    read_document(dir)
        .remove(section)
        .and_then(|value| serde_json::from_value(value).ok())
        .unwrap_or_default()
}

pub(crate) fn save_in<T: Serialize>(dir: &Path, section: &str, value: &T) -> Result<(), String> {
    let mut document = read_document(dir);
    let value = serde_json::to_value(value)
        .map_err(|error| format!("could not encode the {section} settings: {error}"))?;
    document.insert(section.to_owned(), value);
    write_document(dir, &document)
}

/// The settings document, after moving in what older versions saved apart.
fn read_document(dir: &Path) -> Map<String, Value> {
    let file = dir.join(FILE);
    let mut document = std::fs::read_to_string(&file)
        .ok()
        .and_then(|source| serde_json::from_str::<Value>(&source).ok())
        .and_then(|value| match value {
            Value::Object(map) => Some(map),
            _ => None,
        })
        .unwrap_or_default();
    let mut migrated = Vec::new();

    let narration = dir.join(LEGACY_NARRATION);
    if let Ok(source) = std::fs::read_to_string(&narration) {
        if let Ok(Value::Object(old)) = serde_json::from_str::<Value>(&source) {
            document.entry("narration").or_insert(Value::Object(old));
        }
        migrated.push(narration);
    }
    let relay = dir.join(LEGACY_RELAY);
    if let Ok(source) = std::fs::read_to_string(&relay) {
        if let Ok(url) = crate::relay::normalize(&source) {
            let polls = document
                .entry("polls")
                .or_insert_with(|| Value::Object(Map::new()));
            if let Value::Object(polls) = polls {
                polls.entry("relay").or_insert(Value::String(url));
            }
        }
        migrated.push(relay);
    }

    // The old files go only once their settings are safely in the new one.
    if !migrated.is_empty() && write_document(dir, &document).is_ok() {
        for old in migrated {
            let _ = std::fs::remove_file(old);
        }
    }
    document
}

fn write_document(dir: &Path, document: &Map<String, Value>) -> Result<(), String> {
    std::fs::create_dir_all(dir)
        .map_err(|error| format!("could not create {}: {error}", dir.display()))?;
    let text = serde_json::to_string_pretty(document)
        .map_err(|error| format!("could not encode the settings: {error}"))?;
    // Written beside it and renamed, so a crash never leaves half a file.
    let file = dir.join(FILE);
    let temporary = dir.join(format!("{FILE}.tmp"));
    std::fs::write(&temporary, format!("{text}\n"))
        .map_err(|error| format!("could not write {}: {error}", temporary.display()))?;
    std::fs::rename(&temporary, &file)
        .map_err(|error| format!("could not write {}: {error}", file.display()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;

    #[derive(Debug, Default, PartialEq, Serialize, Deserialize)]
    struct Prompter {
        #[serde(default)]
        wpm: f64,
    }

    #[test]
    fn sections_are_saved_side_by_side() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(
            load_in::<Prompter>(dir.path(), "prompter"),
            Prompter::default()
        );
        save_in(dir.path(), "prompter", &Prompter { wpm: 130.0 }).unwrap();
        save_in(dir.path(), "other", &serde_json::json!({ "kept": true })).unwrap();
        assert_eq!(load_in::<Prompter>(dir.path(), "prompter").wpm, 130.0);
        let document = read_document(dir.path());
        assert_eq!(document["other"]["kept"], Value::Bool(true));
        assert!(!dir.path().join("settings.json.tmp").exists());
    }

    #[test]
    fn unreadable_sections_fall_back_to_defaults() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join(FILE), "{\"prompter\": {\"wpm\": \"fast\"}}").unwrap();
        assert_eq!(
            load_in::<Prompter>(dir.path(), "prompter"),
            Prompter::default()
        );
        std::fs::write(dir.path().join(FILE), "not json").unwrap();
        assert_eq!(
            load_in::<Prompter>(dir.path(), "prompter"),
            Prompter::default()
        );
    }

    #[test]
    fn older_files_move_into_their_sections() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join(LEGACY_NARRATION),
            "{\"loudness\": -18.0, \"speaking_wpm\": 141.0}",
        )
        .unwrap();
        std::fs::write(dir.path().join(LEGACY_RELAY), "https://relay.example/\n").unwrap();

        let document = read_document(dir.path());
        assert_eq!(
            document["narration"]["speaking_wpm"],
            serde_json::json!(141.0)
        );
        assert_eq!(
            document["polls"]["relay"],
            serde_json::json!("https://relay.example")
        );
        assert!(!dir.path().join(LEGACY_NARRATION).exists());
        assert!(!dir.path().join(LEGACY_RELAY).exists());
        // Read again, it comes from settings.json.
        assert_eq!(read_document(dir.path()), document);
    }

    #[test]
    fn a_section_already_saved_wins_over_an_older_file() {
        let dir = tempfile::tempdir().unwrap();
        save_in(
            dir.path(),
            "narration",
            &serde_json::json!({ "loudness": -16.0 }),
        )
        .unwrap();
        std::fs::write(dir.path().join(LEGACY_NARRATION), "{\"loudness\": -20.0}").unwrap();
        let document = read_document(dir.path());
        assert_eq!(document["narration"]["loudness"], serde_json::json!(-16.0));
        assert!(!dir.path().join(LEGACY_NARRATION).exists());
    }
}
