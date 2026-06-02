use crate::encoder::ExportFormat;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AspectRatioPreset {
    Youtube,     // 16:9 (1920x1080)
    TikTok,      // 9:16 (1080x1920) - Vertical
    Instagram,   // 1:1 (1080x1080) - Square
    Custom,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QualityPreset {
    Draft,      // 480p, 30fps, fast H.264
    Standard,   // 1080p, 60fps, medium compression
    Production, // 4K, 60fps, slower/higher quality compression
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
    
    // Segment rendering (highly useful for iterative educational content production!)
    pub start_time: Option<f64>,
    pub end_time: Option<f64>,
    
    // Encoder settings
    pub crf: u32,
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
            crf: 18, // High quality, visually lossless
        }
    }
}

impl ExportConfig {
    pub fn new(output_path: &str) -> Self {
        let mut config = Self::default();
        config.output_path = output_path.to_string();
        
        // Auto-detect format from extension
        let path = std::path::Path::new(output_path);
        if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
            match ext.to_lowercase().as_str() {
                "webm" => {
                    config.format = ExportFormat::Webm;
                    config.transparent = true; // WebM defaults to transparent capability
                }
                "gif" => {
                    config.format = ExportFormat::Gif;
                    config.fps = 30; // GIFs are usually 30fps or less
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

    /// Apply preset adjustments to width, height, and fps.
    pub fn apply_presets(mut self) -> Self {
        // Apply Quality preset first
        match self.quality {
            QualityPreset::Draft => {
                self.fps = 30;
                self.crf = 24;
                match self.aspect_ratio {
                    AspectRatioPreset::Youtube => { self.width = 854; self.height = 480; }
                    AspectRatioPreset::TikTok => { self.width = 480; self.height = 854; }
                    AspectRatioPreset::Instagram => { self.width = 480; self.height = 480; }
                    AspectRatioPreset::Custom => {}
                }
            }
            QualityPreset::Standard => {
                self.fps = 60;
                self.crf = 18;
                match self.aspect_ratio {
                    AspectRatioPreset::Youtube => { self.width = 1920; self.height = 1080; }
                    AspectRatioPreset::TikTok => { self.width = 1080; self.height = 1920; }
                    AspectRatioPreset::Instagram => { self.width = 1080; self.height = 1080; }
                    AspectRatioPreset::Custom => {}
                }
            }
            QualityPreset::Production => {
                self.fps = 60;
                self.crf = 14; // Extreme high quality
                match self.aspect_ratio {
                    AspectRatioPreset::Youtube => { self.width = 3840; self.height = 2160; }
                    AspectRatioPreset::TikTok => { self.width = 2160; self.height = 3840; }
                    AspectRatioPreset::Instagram => { self.width = 2160; self.height = 2160; }
                    AspectRatioPreset::Custom => {}
                }
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
