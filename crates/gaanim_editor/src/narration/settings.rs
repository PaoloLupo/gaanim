//! Recorder preferences shared by every project of this user: the
//! `narration` section of the user's settings (see
//! [`gaanim_project::settings`]).

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// The section of the user's settings these preferences are saved in.
const SETTINGS_SECTION: &str = "narration";

/// Integrated loudness for voice in online video. YouTube plays at −14 LUFS
/// and podcasts use −16; −16 keeps speech clear without being pushed down.
pub(crate) const DEFAULT_LOUDNESS: f64 = -16.0;
/// Final loudness outside this range, in LUFS, sounds weak next to other
/// videos or louder than platforms play it.
pub(crate) const LOUDNESS_RANGE: (f64, f64) = (-20.0, -14.0);

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct NarrationSettings {
    /// `whisper-cli` executable; empty searches `PATH`.
    #[serde(default)]
    pub whisper_cli: String,
    /// ggml model file, such as `ggml-base.bin`.
    #[serde(default)]
    pub whisper_model: String,
    /// Spoken language code (`es`, `en`, …) or `auto`.
    #[serde(default = "default_language")]
    pub language: String,
    /// Transcribe every new take right after recording it.
    #[serde(default)]
    pub auto_transcribe: bool,
    /// Count down three seconds before recording starts.
    #[serde(default = "enabled")]
    pub countdown: bool,
    /// Level every new take to `loudness` with FFmpeg.
    #[serde(default = "enabled")]
    pub level_voice: bool,
    /// Integrated loudness target for takes, in LUFS.
    #[serde(default = "default_loudness")]
    pub loudness: f64,
    /// Locale Scribe transcribes dictations in. The token is never saved.
    #[serde(default = "default_scribe_language")]
    pub scribe_language: String,
    /// Send the slides' and stops' names to Scribe as vocabulary.
    #[serde(default)]
    pub scribe_vocabulary: bool,
    /// A dictation ends after this many minutes of audio.
    #[serde(default = "default_scribe_minutes")]
    pub scribe_max_minutes: u32,
    /// Words per minute the speaker says, measured from their dictations,
    /// which the prompter lights the script at.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub speaking_wpm: Option<f64>,
    /// Keep the audio of each recording of the talk, part by part, in
    /// Gaanim's data folder, to listen back to it.
    #[serde(default)]
    pub keep_talk_audio: bool,
    /// Sections of the narration panel the speaker folded.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub folded_sections: Vec<String>,
}

fn default_language() -> String {
    "es".to_string()
}

fn enabled() -> bool {
    true
}

fn default_loudness() -> f64 {
    DEFAULT_LOUDNESS
}

fn default_scribe_language() -> String {
    "es-ES".to_string()
}

fn default_scribe_minutes() -> u32 {
    60
}

impl Default for NarrationSettings {
    fn default() -> Self {
        Self {
            whisper_cli: String::new(),
            whisper_model: String::new(),
            language: default_language(),
            auto_transcribe: false,
            countdown: true,
            level_voice: true,
            loudness: DEFAULT_LOUDNESS,
            scribe_language: default_scribe_language(),
            scribe_vocabulary: false,
            scribe_max_minutes: default_scribe_minutes(),
            speaking_wpm: None,
            keep_talk_audio: false,
            folded_sections: Vec::new(),
        }
    }
}

impl NarrationSettings {
    pub(crate) fn load() -> Self {
        gaanim_project::settings::load(SETTINGS_SECTION)
    }

    pub(crate) fn save(&self) -> Result<(), String> {
        gaanim_project::settings::save(SETTINGS_SECTION, self)
    }

    /// The executable and model to run, or why transcription is unavailable.
    pub(crate) fn whisper(&self) -> Result<(PathBuf, PathBuf), String> {
        let cli = if self.whisper_cli.trim().is_empty() {
            find_on_path("whisper-cli")
                .ok_or("whisper-cli was not found on the PATH; enter its path")?
        } else {
            let path = PathBuf::from(self.whisper_cli.trim());
            if !path.is_file() {
                return Err(format!("{} does not exist", path.display()));
            }
            path
        };
        let model = PathBuf::from(self.whisper_model.trim());
        if self.whisper_model.trim().is_empty() {
            return Err("choose a whisper.cpp model (ggml-*.bin)".to_string());
        }
        if !model.is_file() {
            return Err(format!("the model {} does not exist", model.display()));
        }
        Ok((cli, model))
    }
}

fn find_on_path(program: &str) -> Option<PathBuf> {
    let names: Vec<String> = if cfg!(windows) {
        vec![format!("{program}.exe"), program.to_string()]
    } else {
        vec![program.to_string()]
    };
    std::env::split_paths(&std::env::var_os("PATH")?)
        .flat_map(|dir| names.iter().map(move |name| dir.join(name)))
        .find(|candidate| candidate.is_file())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_default_to_spanish_leveled_voice_with_a_countdown() {
        let settings: NarrationSettings = serde_json::from_str("{}").unwrap();
        assert_eq!(settings, NarrationSettings::default());
        assert_eq!(settings.language, "es");
        assert!(settings.countdown && !settings.auto_transcribe);
        assert!(settings.level_voice);
        assert_eq!(settings.loudness, -16.0);
    }

    #[test]
    fn whisper_reports_what_is_missing() {
        let directory = tempfile::tempdir().unwrap();
        let cli = directory.path().join("whisper-cli.exe");
        std::fs::write(&cli, "").unwrap();
        let mut settings = NarrationSettings {
            whisper_cli: cli.to_string_lossy().into_owned(),
            ..NarrationSettings::default()
        };
        assert!(settings.whisper().unwrap_err().contains("model"));
        settings.whisper_model = directory
            .path()
            .join("ggml-base.bin")
            .to_string_lossy()
            .into_owned();
        assert!(settings.whisper().unwrap_err().contains("does not exist"));
        std::fs::write(&settings.whisper_model, "").unwrap();
        assert_eq!(
            settings.whisper().unwrap(),
            (cli, PathBuf::from(&settings.whisper_model))
        );
    }
}
