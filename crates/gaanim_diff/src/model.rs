use serde::{Deserialize, Serialize};

pub const MANIFEST_FILE: &str = "manifest.json";
pub const REPORT_FILE: &str = "report.json";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotManifest {
    pub schema_version: u32,
    pub width: u32,
    pub height: u32,
    pub snapshots: Vec<SnapshotEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotEntry {
    pub id: String,
    pub time_seconds: f64,
    pub file: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiffReport {
    pub schema_version: u32,
    pub passed: bool,
    pub pixel_threshold: u8,
    pub max_changed_ratio: f64,
    pub compared: usize,
    pub changed: usize,
    pub missing: usize,
    pub frames: Vec<FrameDiff>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FrameDiff {
    pub id: String,
    pub time_seconds: Option<f64>,
    pub status: FrameStatus,
    pub baseline_file: Option<String>,
    pub current_file: Option<String>,
    pub diff_file: Option<String>,
    pub baseline_size: Option<[u32; 2]>,
    pub current_size: Option<[u32; 2]>,
    pub changed_pixels: u64,
    pub total_pixels: u64,
    pub changed_ratio: f64,
    pub mean_absolute_error: f64,
    pub max_channel_delta: u8,
    pub change_bounds: Option<[u32; 4]>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FrameStatus {
    Unchanged,
    Changed,
    DimensionMismatch,
    MissingBaseline,
    MissingCurrent,
}

impl FrameStatus {
    pub fn is_failure(self) -> bool {
        !matches!(self, Self::Unchanged)
    }
}
