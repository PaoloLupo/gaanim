pub mod config;
pub mod encoder;
pub mod exporter;
pub mod gpu;

pub mod prelude {
    pub use crate::config::{
        AspectRatioPreset, AudioTrack, AudioTrackError, ExportConfig, ExportTelemetry,
        QualityPreset,
    };
    pub use crate::encoder::{
        EncodingSpeed, ExportError, ExportFormat, VideoEncoder, detect_available_encoders,
        detect_best_encoder,
    };
    pub use crate::exporter::{
        CapturedFrame, capture_scene_direct, capture_scene_hybrid, export_scene,
        export_scene_direct,
    };
    pub use crate::gpu::GpuContext;
}
