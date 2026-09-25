//! Optional word timing with whisper.cpp.
//!
//! Gaanim does not bundle a speech model: it runs a `whisper-cli` executable
//! and a ggml model the user installed, converts the take to the 16 kHz mono
//! WAV whisper.cpp expects with FFmpeg, and stores the word timings in the
//! take's sidecar. Markers named after spoken words then resolve without
//! tapping them while recording.

use std::path::{Path, PathBuf};
use std::process::Command;

use crossbeam_channel::Receiver;
use gaanim_media::narration::parse_whisper_json;

use super::settings::NarrationSettings;
use super::{run_tool, update_sidecar};

/// A transcription running in the background.
pub(crate) struct Transcription {
    pub key: String,
    result: Receiver<Result<usize, String>>,
}

impl Transcription {
    /// The number of transcribed words once the job has finished.
    pub(crate) fn poll(&self) -> Option<Result<usize, String>> {
        self.result.try_recv().ok()
    }
}

/// Transcribe `take` and store its words in `sidecar`, keeping its markers.
pub(crate) fn start_transcription(
    key: String,
    take: PathBuf,
    sidecar: PathBuf,
    settings: &NarrationSettings,
) -> Result<Transcription, String> {
    let (cli, model) = settings.whisper()?;
    let language = match settings.language.trim() {
        "" => "auto".to_string(),
        language => language.to_string(),
    };
    let (sender, result) = crossbeam_channel::bounded(1);
    let job_key = key.clone();
    std::thread::Builder::new()
        .name("gaanim-whisper".into())
        .spawn(move || {
            let outcome = transcribe(&job_key, &take, &sidecar, &cli, &model, &language);
            let _ = sender.send(outcome);
        })
        .map_err(|error| error.to_string())?;
    Ok(Transcription { key, result })
}

fn transcribe(
    key: &str,
    take: &Path,
    sidecar_path: &Path,
    cli: &Path,
    model: &Path,
    language: &str,
) -> Result<usize, String> {
    let work = super::scratch_dir("whisper", key)?;
    let result = (|| {
        let input = work.join("take.wav");
        run_tool(
            Command::new("ffmpeg")
                .args(["-hide_banner", "-loglevel", "error", "-y", "-i"])
                .arg(take)
                .args(["-ar", "16000", "-ac", "1", "-c:a", "pcm_s16le"])
                .arg(&input),
        )
        .map_err(|error| format!("FFmpeg no pudo preparar la toma: {error}"))?;
        let prefix = work.join("take");
        run_tool(
            Command::new(cli)
                .arg("-m")
                .arg(model)
                .arg("-f")
                .arg(&input)
                .args(["-l", language, "-ml", "1", "-sow", "-oj", "-np", "-of"])
                .arg(&prefix),
        )
        .map_err(|error| format!("whisper-cli falló: {error}"))?;
        let json = std::fs::read_to_string(prefix.with_extension("json"))
            .map_err(|error| format!("whisper-cli no escribió su JSON: {error}"))?;
        let words = parse_whisper_json(&json)
            .map_err(|error| format!("respuesta de whisper-cli inválida: {error}"))?;
        let count = words.len();
        update_sidecar(sidecar_path, |sidecar| sidecar.words = words)?;
        Ok(count)
    })();
    let _ = std::fs::remove_dir_all(&work);
    result
}
