pub mod components;
pub mod hierarchy;
pub mod prelude;
pub mod systems;

pub use components::{
    AuthoritativeCameraView, Billboard, FillBrush, GaanimDefault3dLight, GlobalOpacity,
    GltfAnimationState, GltfAssetHandle, GltfMaterialBaseline, GltfModelReady, GltfModelRoot,
    GltfNodeBinding, GltfNodeWrapper, GroupMarker, HudOverlay, Lighting3D, LineListData,
    LocalBounds, Material3D, Material3DBaseline, Material3DError, Mesh3DMarker, MobjectId,
    ObjectTag, Opacity, Path2D, PathSource, RasterImage, RenderLayer, RenderOrder, StrokeBrush,
    TriangleMeshData, Visible, WorldBounds,
};
pub use hierarchy::{GaanimScenePlugin, SceneSet};
pub use systems::{opacity_propagation_system, sync_new_opacities, transform_propagation_system};

/// Asset configuration used by official Gaanim hosts.
///
/// Gaanim resolves user-selected local assets to canonical absolute paths and
/// requests them through `AssetServer::load_override`. `Deny` permits those
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
