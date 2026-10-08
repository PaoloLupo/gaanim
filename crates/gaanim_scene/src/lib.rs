pub mod media_frame;
pub use media_frame::{ImageFit, MediaFrame};
pub mod components;
pub mod hierarchy;
pub mod logging;
pub mod prelude;
pub mod systems;
pub mod transition_frame;

pub use components::{
    Billboard, CoordinateLabelOffset, CoordinateTickLevel, CoordinateViewRole, CoversOnPurpose,
    DepthOfField, FillBrush, FillDirection, FillLevel, GlobalOpacity, GroupMarker, HudOverlay,
    LayoutBackdrop, LayoutInspection, LayoutInspectionCell, LayoutInspectionFrame,
    LayoutInspectionKind, LayoutZoneRecord, LayoutZones, Lighting3D, LineListData, LineListSource,
    LocalBounds, Material3D, Material3DError, Mesh3DMarker, MobjectId, ObjectTag, Opacity,
    ParallaxLayer, Path2D, PathRevealOrder, PathSource, Presence, RasterImage, RenderLayer,
    RenderOrder, SegmentContent, ShapeDeform, StrokeBrush, TextBaseline, TriangleMeshData, Visible,
    WorldBounds, ZLayer,
};
pub use hierarchy::{GaanimScenePlugin, SceneSet};
pub use systems::{
    hud_pin, opacity_propagation_system, parallax_pin, pin_hud_overlays_system,
    pin_parallax_layers_system, sync_new_opacities, transform_propagation_system, world_hud_pin,
    world_parallax_pin,
};
pub use transition_frame::{
    SceneTransitionFrame, TransitionMask, TransitionOverlayLayer, TransitionShader,
    TransitionShaderFrame, TransitionSide,
};

/// Asset configuration used by official Gaanim hosts.
///
/// Gaanim resolves user-selected local assets to canonical absolute paths and
/// requests them through an override-enabled `AssetServer` load builder. `Deny` permits those
/// explicit override requests while continuing to reject ordinary loads from
/// outside registered asset roots.
pub fn gaanim_asset_plugin() -> bevy::asset::AssetPlugin {
    bevy::asset::AssetPlugin {
        unapproved_path_mode: bevy::asset::UnapprovedPathMode::Deny,
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn official_asset_plugin_allows_only_explicit_external_overrides() {
        assert!(matches!(
            super::gaanim_asset_plugin().unapproved_path_mode,
            bevy::asset::UnapprovedPathMode::Deny
        ));
    }
}
