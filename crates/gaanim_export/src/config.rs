use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use crate::encoder::{EncodingSpeed, ExportFormat, VideoEncoder};
pub use gaanim_media::{AudioTrack, AudioTrackError};

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

/// Mapping used when output pixels do not share the authored frame aspect.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OutputFit {
    /// Reject an aspect mismatch larger than one raster pixel.
    #[default]
    Error,
    /// Preserve the whole frame and letterbox the remainder.
    Contain,
    /// Fill the output and crop logical frame edges.
    Cover,
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
    pub fit: OutputFit,

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
            fit: OutputFit::Error,
            start_time: None,
            end_time: None,
            crf: 18,
            encoding_speed: EncodingSpeed::Balanced,
            video_encoder: VideoEncoder::Auto,
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
    fn presets_preserve_automatic_and_explicit_encoder_selection() {
        let automatic = ExportConfig::default().apply_presets();
        let explicit = ExportConfig {
            video_encoder: VideoEncoder::Libx264,
            ..ExportConfig::default()
        }
        .apply_presets();

        assert_eq!(automatic.video_encoder, VideoEncoder::Auto);
        assert_eq!(explicit.video_encoder, VideoEncoder::Libx264);
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
