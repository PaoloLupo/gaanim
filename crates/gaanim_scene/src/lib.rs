pub mod media_frame;
pub use media_frame::{ImageFit, MediaFrame};
pub mod axis_box;
pub mod components;
pub mod hierarchy;
pub mod model;
pub mod occlusion;
// Bevy's log and asset subsystems are left out of the playground's Pyodide
// extension (wasm32-unknown-emscripten); see Cargo.toml.
#[cfg(not(target_os = "emscripten"))]
pub mod logging;
pub mod prelude;
pub mod systems;
pub mod transition_frame;

pub use axis_box::{
    AxisBoxLayout, AxisBoxPart, AxisBoxRole, AxisBoxShown, AxisBoxSides, PlaneEdge,
};
/// Bevy's handlers of ECS errors no system or command handled, such as
/// [`ecs_error::warn`], for an app to choose with `App::set_error_handler`.
pub use bevy::ecs::error as ecs_error;
pub use components::{
    AuthoredCall, AuthoredIndex, AuthoredObject, AuthoredObjects, AxisLabelGap, Billboard,
    BillboardPlane, BillboardShift, CoordinateLabelOffset, CoordinateTickLevel, CoordinateViewRole,
    CoversOnPurpose, DepthOfField, DrawnIn3D, FillBrush, FillDirection, FillLevel, GlobalOpacity,
    GroupMarker, HudOverlay, InWorld3D, LayoutBackdrop, LayoutInspection, LayoutInspectionCell,
    LayoutInspectionFrame, LayoutInspectionKind, LayoutZoneRecord, LayoutZones, Light3D,
    LightKind3D, Lighting3D, LineListData, LineListSource, LocalBounds, Material3D,
    Material3DError, Mesh3DMarker, MeshBlend, MeshCopies, MeshInstances, MobjectId, ObjectTag,
    Opacity, ParallaxLayer, Path2D, PathRevealOrder, PathSource, PlaneClip, PlaneFillData,
    PlaneGlyph, PlaneImage, Presence, REVEAL_SPAN, RasterImage, RenderLayer, RenderOrder,
    SegmentContent, ShapeDeform, StepRate, StrokeBrush, TextAdvance, TextBaseline,
    TriangleMeshData, Visible, WorldBounds, ZLayer, stepped,
};
pub use hierarchy::{GaanimScenePlugin, SceneSet};
pub use model::{
    AlphaMode, MeshSkin, MeshSurface, MeshTexture, MorphTarget, MorphWeights, NormalMap,
    SkinPalette, SkinWeights, TextureWrap,
};
pub use occlusion::Occlusion;

/// Bevy's render visibility, which scene entities carry for Bevy's own
/// hierarchy; Gaanim's renderer reads [`Visible`] instead.
#[cfg(not(target_os = "emscripten"))]
pub use bevy::prelude::Visibility;

/// The playground's Pyodide extension (wasm32-unknown-emscripten) leaves out
/// Bevy's camera crate, which defines `Visibility`; this inert component with
/// the same variants takes its place there.
#[cfg(target_os = "emscripten")]
#[derive(bevy::prelude::Component, Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Visibility {
    #[default]
    Inherited,
    Hidden,
    Visible,
}

/// The color the canvas clears to before each frame.
#[cfg(not(target_os = "emscripten"))]
pub use bevy::prelude::ClearColor;

/// Bevy's `ClearColor` also lives in its camera crate; the playground's
/// Pyodide extension records it in bundles through this resource instead.
#[cfg(target_os = "emscripten")]
#[derive(bevy::prelude::Resource, Clone, Debug)]
pub struct ClearColor(pub bevy::prelude::Color);

#[cfg(target_os = "emscripten")]
impl Default for ClearColor {
    /// Bevy's default clear color.
    fn default() -> Self {
        Self(bevy::prelude::Color::srgb_u8(43, 44, 47))
    }
}
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
#[cfg(not(target_os = "emscripten"))]
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
