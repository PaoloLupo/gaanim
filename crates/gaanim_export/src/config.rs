use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use crate::encoder::{EncodingSpeed, ExportFormat, VideoEncoder};

/// A source file mixed into the exported video.
///
/// Times are expressed in the scene timeline. A track with `start_time = 2.0`
/// begins two seconds after the start of the scene, independently of an export
/// range selected later.
#[derive(Debug, Clone)]
pub struct AudioTrack {
    pub path: PathBuf,
    pub start_time: f64,
    /// Optional output-timeline duration after playback-rate conversion.
    pub duration: Option<f64>,
    pub volume: f64,
    pub fade_in: f64,
    pub fade_out: f64,
    pub source_offset: f64,
    pub source_duration: Option<f64>,
    pub speed: f64,
    pub looping: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum AudioTrackError {
    #[error("audio file '{path}' does not exist or is not a file")]
    InvalidPath { path: PathBuf },
    #[error("{name} must be a finite non-negative number")]
    InvalidNumber { name: &'static str },
    #[error("fade_out requires an explicit track duration")]
    FadeOutNeedsDuration,
    #[error("fade duration cannot exceed the track duration")]
    FadeExceedsDuration,
}

impl AudioTrack {
    pub fn new(
        path: impl Into<PathBuf>,
        start_time: f64,
        duration: Option<f64>,
        volume: f64,
        fade_in: f64,
        fade_out: f64,
    ) -> Result<Self, AudioTrackError> {
        let path = path.into();
        if !path.is_file() {
            return Err(AudioTrackError::InvalidPath { path });
        }
        for (name, value) in [
            ("start_time", start_time),
            ("volume", volume),
            ("fade_in", fade_in),
            ("fade_out", fade_out),
        ] {
            if !value.is_finite() || value < 0.0 {
                return Err(AudioTrackError::InvalidNumber { name });
            }
        }
        if let Some(duration) = duration {
            if !duration.is_finite() || duration <= 0.0 {
                return Err(AudioTrackError::InvalidNumber { name: "duration" });
            }
            if fade_in + fade_out > duration {
                return Err(AudioTrackError::FadeExceedsDuration);
            }
        } else if fade_out > 0.0 {
            return Err(AudioTrackError::FadeOutNeedsDuration);
        }
        Ok(Self {
            path,
            start_time,
            duration,
            volume,
            fade_in,
            fade_out,
            source_offset: 0.0,
            source_duration: duration,
            speed: 1.0,
            looping: false,
        })
    }

    /// Build a track sourced from an embedded media stream.
    pub fn from_media(
        path: impl Into<PathBuf>,
        start_time: f64,
        source_offset: f64,
        source_duration: f64,
        speed: f64,
        looping: bool,
        volume: f64,
    ) -> Result<Self, AudioTrackError> {
        if !source_offset.is_finite() || source_offset < 0.0 {
            return Err(AudioTrackError::InvalidNumber {
                name: "source_offset",
            });
        }
        for (name, value) in [("source_duration", source_duration), ("speed", speed)] {
            if !value.is_finite() || value <= 0.0 {
                return Err(AudioTrackError::InvalidNumber { name });
            }
        }
        let output_duration = (!looping).then_some(source_duration / speed);
        let mut track = Self::new(path, start_time, output_duration, volume, 0.0, 0.0)?;
        track.source_offset = source_offset;
        track.source_duration = Some(source_duration);
        track.speed = speed;
        track.looping = looping;
        Ok(track)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AspectRatioPreset {
    Youtube,
    TikTok,
    Instagram,
    Custom,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QualityPreset {
    Draft,
    Standard,
    Production,
}

impl QualityPreset {
    pub fn encoding_speed(self) -> EncodingSpeed {
        match self {
            Self::Draft => EncodingSpeed::Fast,
            Self::Standard => EncodingSpeed::Balanced,
            Self::Production => EncodingSpeed::Best,
        }
    }
}

/// Thread- and process-agnostic progress information for an export.
///
/// The editor owns one instance and passes a clone through `ExportConfig`.
/// The exporter updates it from its render loop while the UI reads snapshots
/// from the Bevy/Egui thread.
#[derive(Clone, Debug, Default)]
pub struct ExportTelemetry {
    current_frame: Arc<AtomicU64>,
    total_frames: Arc<AtomicU64>,
    encoder: Arc<Mutex<Option<String>>>,
    logs: Arc<Mutex<Vec<String>>>,
}

impl ExportTelemetry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_total_frames(&self, total_frames: u64) {
        self.total_frames.store(total_frames, Ordering::Relaxed);
        self.current_frame.store(0, Ordering::Relaxed);
    }

    pub fn set_current_frame(&self, current_frame: u64) {
        self.current_frame.store(current_frame, Ordering::Relaxed);
    }

    pub fn set_progress(&self, current_frame: u64, total_frames: u64) {
        self.total_frames.store(total_frames, Ordering::Relaxed);
        self.current_frame.store(current_frame, Ordering::Relaxed);
    }

    pub fn progress(&self) -> (u64, u64) {
        (
            self.current_frame.load(Ordering::Relaxed),
            self.total_frames.load(Ordering::Relaxed),
        )
    }

    pub fn set_encoder(&self, encoder: impl Into<String>) {
        *self
            .encoder
            .lock()
            .expect("export telemetry encoder poisoned") = Some(encoder.into());
    }

    pub fn encoder(&self) -> Option<String> {
        self.encoder
            .lock()
            .expect("export telemetry encoder poisoned")
            .clone()
    }

    pub fn push_log(&self, line: impl Into<String>) {
        let mut logs = self.logs.lock().expect("export telemetry log poisoned");
        logs.push(line.into());
        // Keep a runaway FFmpeg/Bevy stream from growing the editor forever.
        const MAX_LOG_LINES: usize = 2_000;
        if logs.len() > MAX_LOG_LINES {
            let remove = logs.len() - MAX_LOG_LINES;
            logs.drain(..remove);
        }
    }

    pub fn logs(&self) -> Vec<String> {
        self.logs
            .lock()
            .expect("export telemetry log poisoned")
            .clone()
    }
}

#[derive(Debug, Clone)]
pub struct ExportConfig {
    pub output_path: String,
    pub format: ExportFormat,
    pub aspect_ratio: AspectRatioPreset,
    pub quality: QualityPreset,
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    pub transparent: bool,

    pub start_time: Option<f64>,
    pub end_time: Option<f64>,

    pub crf: u32,

    pub encoding_speed: EncodingSpeed,
    pub video_encoder: VideoEncoder,
    pub headless: bool,
    pub audio_tracks: Vec<AudioTrack>,
    pub telemetry: Option<ExportTelemetry>,
}

impl Default for ExportConfig {
    fn default() -> Self {
        Self {
            output_path: "output.mp4".to_string(),
            format: ExportFormat::Mp4,
            aspect_ratio: AspectRatioPreset::Youtube,
            quality: QualityPreset::Standard,
            width: 1920,
            height: 1080,
            fps: 60,
            transparent: false,
            start_time: None,
            end_time: None,
            crf: 18,
            encoding_speed: EncodingSpeed::Balanced,
            video_encoder: VideoEncoder::Libx264,
            headless: false,
            audio_tracks: Vec::new(),
            telemetry: None,
        }
    }
}

impl ExportConfig {
    pub fn new(output_path: &str) -> Self {
        let path = std::path::Path::new(output_path);
        let mut config = Self {
            output_path: output_path.to_string(),
            ..Default::default()
        };
        if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
            match ext.to_lowercase().as_str() {
                "webm" => {
                    config.format = ExportFormat::Webm;
                    config.transparent = true;
                }
                "webp" => {
                    config.format = ExportFormat::Webp;
                    config.fps = 30;
                }
                "gif" => {
                    config.format = ExportFormat::Gif;
                    config.fps = 30;
                }
                "png" => {
                    config.format = ExportFormat::PngSequence;
                }
                _ => {
                    config.format = ExportFormat::Mp4;
                }
            }
        }
        config
    }

    pub fn apply_presets(mut self) -> Self {
        self.encoding_speed = self.quality.encoding_speed();

        match self.quality {
            QualityPreset::Draft => {
                self.fps = 30;
                self.crf = 24;
            }
            QualityPreset::Standard => {
                self.fps = 60;
                self.crf = 18;
            }
            QualityPreset::Production => {
                self.fps = 60;
                self.crf = 14;
            }
        }
        self
    }

    pub fn with_aspect_ratio(mut self, preset: AspectRatioPreset) -> Self {
        self.aspect_ratio = preset;
        self.apply_presets()
    }

    pub fn with_quality(mut self, preset: QualityPreset) -> Self {
        self.quality = preset;
        self.apply_presets()
    }

    pub fn with_segment(mut self, start: f64, end: f64) -> Self {
        self.start_time = Some(start);
        self.end_time = Some(end);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::{AudioTrack, ExportConfig, ExportTelemetry};
    use crate::encoder::VideoEncoder;

    #[test]
    fn presets_preserve_the_requested_software_encoder() {
        let config = ExportConfig::default().apply_presets();

        assert_eq!(config.video_encoder, VideoEncoder::Libx264);
    }

    #[test]
    fn telemetry_clones_share_progress_and_logs() {
        let telemetry = ExportTelemetry::new();
        let worker_view = telemetry.clone();

        worker_view.set_total_frames(12);
        worker_view.set_current_frame(7);
        worker_view.set_encoder("NVIDIA (NVENC)");
        worker_view.push_log("frame progress");

        assert_eq!(telemetry.progress(), (7, 12));
        assert_eq!(telemetry.encoder().as_deref(), Some("NVIDIA (NVENC)"));
        assert_eq!(telemetry.logs(), vec!["frame progress"]);
    }

    #[test]
    fn media_audio_track_maps_source_time_to_scene_time() {
        let path =
            std::env::temp_dir().join(format!("gaanim-audio-track-test-{}", std::process::id()));
        std::fs::write(&path, b"fixture").unwrap();
        let track = AudioTrack::from_media(&path, 3.0, 2.0, 4.0, 2.0, false, 0.75)
            .expect("valid media track");
        assert_eq!(track.start_time, 3.0);
        assert_eq!(track.duration, Some(2.0));
        assert_eq!(track.source_offset, 2.0);
        assert_eq!(track.source_duration, Some(4.0));
        assert_eq!(track.speed, 2.0);
        assert!(!track.looping);
        let _ = std::fs::remove_file(path);
    }
}
