//! Recorder preferences shared by every project of this user.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

const SETTINGS_FILE: &str = "narration.json";

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
        }
    }
}

impl NarrationSettings {
    fn path() -> Option<PathBuf> {
        gaanim_project::user_data_dir().map(|dir| dir.join(SETTINGS_FILE))
    }

    pub(crate) fn load() -> Self {
        Self::path()
            .and_then(|path| std::fs::read_to_string(path).ok())
            .and_then(|source| serde_json::from_str(&source).ok())
            .unwrap_or_default()
    }

    pub(crate) fn save(&self) -> Result<(), String> {
        let path = Self::path().ok_or("no se encontró la carpeta de datos de Gaanim")?;
        let source = serde_json::to_string_pretty(self).map_err(|error| error.to_string())?;
        gaanim_media::narration::write_atomically(&path, source.as_bytes())
            .map_err(|error| error.to_string())
    }

    /// The executable and model to run, or why transcription is unavailable.
    pub(crate) fn whisper(&self) -> Result<(PathBuf, PathBuf), String> {
        let cli = if self.whisper_cli.trim().is_empty() {
            find_on_path("whisper-cli")
                .ok_or("no se encontró whisper-cli en el PATH; indica su ruta")?
        } else {
            let path = PathBuf::from(self.whisper_cli.trim());
            if !path.is_file() {
                return Err(format!("no existe {}", path.display()));
            }
            path
        };
        let model = PathBuf::from(self.whisper_model.trim());
        if self.whisper_model.trim().is_empty() {
            return Err("elige un modelo de whisper.cpp (ggml-*.bin)".to_string());
        }
        if !model.is_file() {
            return Err(format!("no existe el modelo {}", model.display()));
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
        assert!(settings.whisper().unwrap_err().contains("modelo"));
        settings.whisper_model = directory
            .path()
            .join("ggml-base.bin")
            .to_string_lossy()
            .into_owned();
        assert!(
            settings
                .whisper()
                .unwrap_err()
                .contains("no existe el modelo")
        );
        std::fs::write(&settings.whisper_model, "").unwrap();
        assert_eq!(
            settings.whisper().unwrap(),
            (cli, PathBuf::from(&settings.whisper_model))
        );
    }
}
