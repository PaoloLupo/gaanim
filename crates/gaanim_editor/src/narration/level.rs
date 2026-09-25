//! Voice leveling for narration takes.
//!
//! A microphone take comes out as loud as the room and the gain knob made
//! it. Before a take joins the scene, FFmpeg's EBU R128 `loudnorm` filter
//! brings it to a fixed integrated loudness in two passes: the first measures
//! the take, the second applies one constant gain (so the delivery keeps its
//! dynamics) with a true-peak ceiling. A gentle high-pass removes rumble
//! below the voice first. The unleveled original is kept apart.

use std::path::{Path, PathBuf};
use std::process::Command;

use crossbeam_channel::Receiver;
use gaanim_media::narration::temporary_sibling;

use super::{run_tool, update_sidecar};

/// Highest true peak a leveled take may reach, in dBTP.
pub(crate) const TRUE_PEAK: f64 = -1.5;
/// Loudness range `loudnorm` falls back to when one gain cannot reach the
/// target within the peak ceiling.
const LOUDNESS_RANGE: f64 = 11.0;
/// Rumble, handling noise and plosive thumps live below this frequency.
const HIGH_PASS_HZ: u32 = 80;
/// Sample rate of leveled takes.
const SAMPLE_RATE: u32 = 48_000;

/// First-pass `loudnorm` measurements of a take.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Measurement {
    pub integrated: f64,
    pub true_peak: f64,
    pub range: f64,
    pub threshold: f64,
    pub offset: f64,
}

/// A leveling job running in the background.
pub(crate) struct Leveling {
    pub key: String,
    result: Receiver<Result<Measurement, String>>,
}

impl Leveling {
    /// The measured original once the job has finished.
    pub(crate) fn poll(&self) -> Option<Result<Measurement, String>> {
        self.result.try_recv().ok()
    }
}

/// Level `original` into `destination` at `target` LUFS and record the
/// target in `sidecar`. If leveling fails, the original is copied to
/// `destination` unchanged so the new take is never lost.
pub(crate) fn start_leveling(
    key: String,
    original: PathBuf,
    destination: PathBuf,
    sidecar: PathBuf,
    target: f64,
) -> Result<Leveling, String> {
    let (sender, result) = crossbeam_channel::bounded(1);
    let job_key = key.clone();
    std::thread::Builder::new()
        .name("gaanim-level".into())
        .spawn(move || {
            let outcome = level(&job_key, &original, &destination, target).and_then(|measured| {
                update_sidecar(&sidecar, |sidecar| sidecar.loudness = Some(target))?;
                Ok(measured)
            });
            if outcome.is_err() {
                let _ = copy_atomically(&original, &destination);
                let _ = update_sidecar(&sidecar, |sidecar| sidecar.loudness = None);
            }
            let _ = sender.send(outcome);
        })
        .map_err(|error| error.to_string())?;
    Ok(Leveling { key, result })
}

/// The filter chain for a pass: measuring without `measured`, applying one
/// linear gain with it.
pub(crate) fn loudnorm_filter(target: f64, measured: Option<&Measurement>) -> String {
    let mut filter = format!(
        "highpass=f={HIGH_PASS_HZ},loudnorm=I={target:.1}:TP={TRUE_PEAK:.1}:LRA={LOUDNESS_RANGE:.1}"
    );
    match measured {
        None => filter.push_str(":print_format=json"),
        Some(measured) => filter.push_str(&format!(
            ":measured_I={:.2}:measured_TP={:.2}:measured_LRA={:.2}:measured_thresh={:.2}:offset={:.2}:linear=true",
            measured.integrated,
            measured.true_peak,
            measured.range,
            measured.threshold,
            measured.offset
        )),
    }
    filter
}

/// Read the JSON block `loudnorm` prints at the end of its first pass.
pub(crate) fn parse_measurement(stderr: &str) -> Result<Measurement, String> {
    let start = stderr
        .rfind('{')
        .ok_or("FFmpeg no informó la sonoridad de la toma")?;
    let end = stderr[start..]
        .find('}')
        .map(|end| start + end + 1)
        .ok_or("respuesta de loudnorm incompleta")?;
    let values: std::collections::HashMap<String, String> =
        serde_json::from_str(&stderr[start..end]).map_err(|error| error.to_string())?;
    let value = |name: &str| -> Result<f64, String> {
        values
            .get(name)
            .and_then(|value| value.trim().parse::<f64>().ok())
            .filter(|value| value.is_finite())
            .ok_or_else(|| format!("la toma parece estar en silencio ({name} inválido)"))
    };
    Ok(Measurement {
        integrated: value("input_i")?,
        true_peak: value("input_tp")?,
        range: value("input_lra")?,
        threshold: value("input_thresh")?,
        offset: value("target_offset")?,
    })
}

fn level(
    key: &str,
    original: &Path,
    destination: &Path,
    target: f64,
) -> Result<Measurement, String> {
    let measured = run_tool(
        Command::new("ffmpeg")
            .args(["-hide_banner", "-nostats", "-i"])
            .arg(original)
            .arg("-af")
            .arg(loudnorm_filter(target, None))
            .args(["-f", "null", "-"]),
    )
    .map_err(|error| format!("FFmpeg no pudo medir «{key}»: {error}"))?;
    let measurement = parse_measurement(&String::from_utf8_lossy(&measured.stderr))?;
    let temporary = temporary_sibling(destination);
    let applied = run_tool(
        Command::new("ffmpeg")
            .args(["-hide_banner", "-loglevel", "error", "-y", "-i"])
            .arg(original)
            .arg("-af")
            .arg(loudnorm_filter(target, Some(&measurement)))
            .args(["-ar", &SAMPLE_RATE.to_string(), "-ac", "1"])
            .args(["-c:a", "pcm_s16le", "-f", "wav"])
            .arg(&temporary),
    )
    .map_err(|error| format!("FFmpeg no pudo nivelar «{key}»: {error}"));
    if let Err(error) = applied {
        let _ = std::fs::remove_file(&temporary);
        return Err(error);
    }
    std::fs::rename(&temporary, destination).map_err(|error| {
        let _ = std::fs::remove_file(&temporary);
        error.to_string()
    })?;
    Ok(measurement)
}

fn copy_atomically(source: &Path, destination: &Path) -> Result<(), String> {
    let bytes = std::fs::read(source).map_err(|error| error.to_string())?;
    gaanim_media::narration::write_atomically(destination, &bytes)
        .map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIRST_PASS: &str = r#"[Parsed_loudnorm_1 @ 000001]
{
	"input_i" : "-27.61",
	"input_tp" : "-4.47",
	"input_lra" : "5.40",
	"input_thresh" : "-38.02",
	"output_i" : "-16.52",
	"output_tp" : "-1.50",
	"output_lra" : "4.10",
	"output_thresh" : "-26.90",
	"normalization_type" : "dynamic",
	"target_offset" : "0.52"
}"#;

    #[test]
    fn first_pass_measurements_are_read_from_ffmpeg_output() {
        let measured = parse_measurement(FIRST_PASS).unwrap();
        assert_eq!(measured.integrated, -27.61);
        assert_eq!(measured.true_peak, -4.47);
        assert_eq!(measured.offset, 0.52);
        let silent = FIRST_PASS.replace("\"-27.61\"", "\"-inf\"");
        assert!(parse_measurement(&silent).unwrap_err().contains("silencio"));
        assert!(parse_measurement("no json").is_err());
    }

    #[test]
    fn second_pass_applies_one_linear_gain_under_the_peak_ceiling() {
        assert_eq!(
            loudnorm_filter(-16.0, None),
            "highpass=f=80,loudnorm=I=-16.0:TP=-1.5:LRA=11.0:print_format=json"
        );
        let measured = parse_measurement(FIRST_PASS).unwrap();
        let filter = loudnorm_filter(-16.0, Some(&measured));
        assert!(filter.contains("measured_I=-27.61"), "{filter}");
        assert!(filter.contains("offset=0.52:linear=true"), "{filter}");
    }

    #[test]
    fn a_take_is_leveled_to_the_target_when_ffmpeg_is_available() {
        if Command::new("ffmpeg").arg("-version").output().is_err() {
            eprintln!("skipping leveling check because FFmpeg is unavailable");
            return;
        }
        // A quiet voice-like signal: a 220 Hz tone with a slow swell.
        let directory = tempfile::tempdir().unwrap();
        let original = directory.path().join("original.wav");
        let samples: Vec<f32> = (0..48_000 * 3)
            .map(|index| {
                let time = index as f32 / 48_000.0;
                0.05 * (1.0 + 0.5 * (time * 2.0).sin())
                    * (std::f32::consts::TAU * 220.0 * time).sin()
            })
            .collect();
        gaanim_media::narration::write_wav_take(&original, &samples, 48_000).unwrap();
        let destination = directory.path().join("take.wav");
        let measured = level("prueba", &original, &destination, -16.0).unwrap();
        assert!(measured.integrated < -24.0, "{measured:?}");
        let check = run_tool(
            Command::new("ffmpeg")
                .args(["-hide_banner", "-nostats", "-i"])
                .arg(&destination)
                .arg("-af")
                .arg(loudnorm_filter(-16.0, None))
                .args(["-f", "null", "-"]),
        )
        .unwrap();
        let leveled = parse_measurement(&String::from_utf8_lossy(&check.stderr)).unwrap();
        assert!((leveled.integrated - -16.0).abs() < 1.0, "{leveled:?}");
        assert!(leveled.true_peak <= TRUE_PEAK + 0.3, "{leveled:?}");
    }
}
