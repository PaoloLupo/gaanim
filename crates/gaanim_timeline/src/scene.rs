use bevy::prelude::Component;
use gaanim_core::glam::{DQuat, DVec3};
use gaanim_core::peniko::Color;

use crate::clip::{SceneId, TrackId};

/// Tags an entity as belonging to a specific scene.
///
/// Used for per-scene visibility toggling during seek. Entities without
/// this component are considered "global" and remain visible across all scenes.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct SceneMember(pub SceneId);

/// Per-scene camera state override.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct CameraState {
    pub position: DVec3,
    pub rotation: DQuat,
    pub zoom: f64,
}

/// Metadata for a single scene in the timeline.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct SceneMetadata {
    pub id: SceneId,
    pub name: String,
    pub tracks: Vec<TrackId>,
    pub camera_override: Option<CameraState>,
    pub background_override: Option<Color>,
}
