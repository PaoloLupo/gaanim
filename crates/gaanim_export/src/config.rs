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

/// Highest frame rate a quality preset gives GIF and WebP output: animated
/// images grow with every frame and gain little visibly above 30 fps.
const ANIMATED_IMAGE_MAX_FPS: u32 = 30;

impl QualityPreset {
    pub fn encoding_speed(self) -> EncodingSpeed {
        match self {
            Self::Draft => EncodingSpeed::Fast,
            Self::Standard => EncodingSpeed::Balanced,
            Self::Production => EncodingSpeed::Best,
        }
    }

    /// Frame rate this preset exports `format` at.
    pub fn fps_for(self, format: ExportFormat) -> u32 {
        let fps = match self {
            Self::Draft => 30,
            Self::Standard | Self::Production => 60,
        };
        match format {
            ExportFormat::Gif | ExportFormat::Webp => fps.min(ANIMATED_IMAGE_MAX_FPS),
            _ => fps,
        }
    }

    pub fn crf(self) -> u32 {
        match self {
            Self::Draft => 24,
            Self::Standard => 18,
            Self::Production => 14,
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
    /// Render without a window, overlapping the CPU and the GPU (the
    /// default); `false` renders in a visible export viewport window.
    pub headless: bool,
    pub audio_tracks: Vec<AudioTrack>,
    pub telemetry: Option<ExportTelemetry>,
    /// Quality and format whose preset last set `fps`, `crf` and
    /// `encoding_speed`; `apply_presets` keeps later explicit values until
    /// either changes.
    applied_preset: Option<(QualityPreset, ExportFormat)>,
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
            // The direct path: no window, frames overlapped with the GPU.
            headless: true,
            audio_tracks: Vec::new(),
            telemetry: None,
            // The values above are the Standard preset for MP4.
            applied_preset: Some((QualityPreset::Standard, ExportFormat::Mp4)),
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
                }
                "gif" => {
                    config.format = ExportFormat::Gif;
                }
                "png" => {
                    config.format = ExportFormat::PngSequence;
                }
                _ => {
                    config.format = ExportFormat::Mp4;
                }
            }
        }
        config.applied_preset = None;
        config.apply_presets()
    }

    /// Set `fps`, `crf` and `encoding_speed` from the quality preset for the
    /// current format. Exporters call this again, so it changes nothing while
    /// the quality and format it was last applied for are unchanged: values
    /// set explicitly after a preset survive the export.
    pub fn apply_presets(mut self) -> Self {
        let preset = (self.quality, self.format);
        if self.applied_preset == Some(preset) {
            return self;
        }
        self.encoding_speed = self.quality.encoding_speed();
        self.fps = self.quality.fps_for(self.format);
        self.crf = self.quality.crf();
        self.applied_preset = Some(preset);
        self
    }

    pub fn with_aspect_ratio(mut self, preset: AspectRatioPreset) -> Self {
        self.aspect_ratio = preset;
        self.apply_presets()
    }

    pub fn with_quality(mut self, preset: QualityPreset) -> Self {
        self.quality = preset;
        self.applied_preset = None;
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
    use super::{AudioTrack, ExportConfig, ExportTelemetry, QualityPreset};
    use crate::encoder::{ExportFormat, VideoEncoder};

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
    fn default_config_matches_the_standard_mp4_preset() {
        let default = ExportConfig::default();
        let applied = ExportConfig {
            applied_preset: None,
            ..ExportConfig::default()
        }
        .apply_presets();

        assert_eq!(default.fps, applied.fps);
        assert_eq!(default.crf, applied.crf);
        assert_eq!(default.encoding_speed, applied.encoding_speed);
    }

    #[test]
    fn configs_export_without_a_window_by_default() {
        assert!(ExportConfig::default().headless);
        for path in ["out.mp4", "out.webm", "out.gif", "out.png"] {
            assert!(ExportConfig::new(path).headless, "{path}");
        }
    }

    #[test]
    fn animated_image_presets_never_exceed_30_fps() {
        for path in ["out.gif", "out.webp"] {
            for quality in [
                QualityPreset::Draft,
                QualityPreset::Standard,
                QualityPreset::Production,
            ] {
                let config = ExportConfig::new(path)
                    .with_quality(quality)
                    .apply_presets();
                assert_eq!(config.fps, 30, "{path} {quality:?}");
                assert_eq!(config.crf, quality.crf(), "{path} {quality:?}");
            }
        }
        let video = ExportConfig::new("out.mp4").with_quality(QualityPreset::Production);
        assert_eq!(video.fps, 60);
    }

    #[test]
    fn exporter_presets_keep_explicit_fps_and_crf() {
        let mut config = ExportConfig::new("out.mp4").with_quality(QualityPreset::Standard);
        config.fps = 24;
        config.crf = 20;
        let config = config.apply_presets();
        assert_eq!((config.fps, config.crf), (24, 20));

        let mut config = ExportConfig::new("out.gif");
        config.fps = 12;
        assert_eq!(config.apply_presets().fps, 12);
    }

    #[test]
    fn presets_reapply_when_quality_or_format_changes() {
        let mut config = ExportConfig::new("out.mp4").with_quality(QualityPreset::Standard);
        config.format = ExportFormat::Gif;
        let mut config = config.apply_presets();
        assert_eq!(config.fps, 30);

        config.quality = QualityPreset::Draft;
        let mut config = config.apply_presets();
        assert_eq!(config.crf, 24);

        // An explicit `with_quality` always applies its preset.
        config.fps = 15;
        assert_eq!(config.with_quality(QualityPreset::Draft).fps, 30);
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
