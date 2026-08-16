use bevy::animation::graph::{AnimationGraph, AnimationNodeIndex};
use bevy::prelude::{Component, Entity, Handle, Resource};
use gaanim_core::kurbo::{Affine, BezPath, Stroke};
use gaanim_core::peniko::{Brush, ImageBrush};
use gaanim_math::Bounds3D;
use std::path::PathBuf;
use std::sync::Arc;

/// Represents the fill style of a visual Mobject.
///
/// Contains an optional `peniko::Brush` which directly represents solid colors,
/// linear/radial/conic gradients, or image-backed textures.
#[derive(Component, Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Default)]
pub struct FillBrush(pub Option<Brush>);

impl FillBrush {
    /// Creates a solid color fill.
    pub fn color(color: gaanim_core::peniko::Color) -> Self {
        Self(Some(Brush::Solid(color)))
    }

    /// Creates a generic brush-backed fill.
    pub fn brush(brush: Brush) -> Self {
        Self(Some(brush))
    }

    /// Creates a transparent/no-fill style.
    pub fn transparent() -> Self {
        Self(None)
    }
}

/// A decoded raster image drawn by the Vello backend.
///
/// The image owns a reference-counted pixel blob, so cloned mobjects and
/// renderer fragments share the same decoded texture data. `local_transform`
/// positions the image relative to the mobject origin before the regular
/// spatial transform hierarchy is applied.
#[derive(Component, Debug, Clone, PartialEq)]
pub struct RasterImage {
    pub image: Option<ImageBrush>,
    pub local_transform: Affine,
}

impl RasterImage {
    /// A vector mobject without raster content.
    pub fn none() -> Self {
        Self {
            image: None,
            local_transform: Affine::IDENTITY,
        }
    }

    /// Raster content positioned in the mobject's local coordinates.
    pub fn new(image: ImageBrush, local_transform: Affine) -> Self {
        Self {
            image: Some(image),
            local_transform,
        }
    }
}

/// Represents the outline stroke style of a visual Mobject.
#[derive(Component, Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Default)]
pub struct StrokeBrush {
    /// The outline color or brush. `None` represents no outline.
    pub brush: Option<Brush>,
    /// The geometric line properties (width, caps, joins, miter limit, dash patterns).
    pub style: Stroke,
}

impl StrokeBrush {
    /// Creates a new solid color stroke with a given width.
    pub fn new(color: gaanim_core::peniko::Color, width: f64) -> Self {
        Self {
            brush: Some(Brush::Solid(color)),
            style: Stroke::new(width),
        }
    }

    /// Creates a transparent/no-stroke outline.
    pub fn transparent() -> Self {
        Self {
            brush: None,
            style: Stroke::default(),
        }
    }
}

/// Local opacity of a single Mobject.
#[derive(Component, Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Opacity(pub f32);

impl Default for Opacity {
    fn default() -> Self {
        Self(1.0)
    }
}

/// Propagated global opacity used by the rendering backend.
///
/// Automatically computed by the hierarchy propagation systems in `gaanim_scene`.
/// For instance, a child with `Opacity(0.8)` under a parent with `GlobalOpacity(0.5)`
/// will be computed to have `GlobalOpacity(0.4)`.
#[derive(Component, Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct GlobalOpacity(pub f32);

impl Default for GlobalOpacity {
    fn default() -> Self {
        Self(1.0)
    }
}

/// A 2D vector geometry represented as a shared Bézier path (`Arc<kurbo::BezPath>`).
#[derive(Component, Debug, Clone, Default, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Path2D(pub Arc<BezPath>);

/// A mirror component that caches the original, unmodified Bézier path
/// of a Mobject for time-based trimming / interpolation during writing/drawing.
#[derive(Component, Debug, Clone, Default, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct PathSource(pub Arc<BezPath>);

/// The computed local bounding box of a Mobject.
#[derive(Component, Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Default)]
pub struct LocalBounds(pub Bounds3D);

/// Local Y coordinate of a text object's typographic baseline.
///
/// Unlike visual bounds, this metric is stable across ascenders, descenders,
/// subscripts, and glyph changes, allowing independently compiled text runs to
/// share a true baseline.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct TextBaseline(pub f64);

/// The computed world bounding box of a Mobject in world coordinates.
///
/// Propagated automatically down the scene hierarchy.
#[derive(Component, Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Default)]
pub struct WorldBounds(pub Bounds3D);

/// Deterministic rendering order of Mobjects.
///
/// Resolves ordering bugs by coupling the typical `z_index` with a monotonically
/// increasing `creation_order` counter. When sorting entities for extraction and composition,
/// the `creation_order` serves as a clean tie-breaker when `z_index` matches, respecting
/// the exact programmatic construction sequence.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Default)]
pub struct RenderOrder {
    /// Manual depth index (higher renders on top).
    pub z_index: i32,
    /// Monotonically increasing creation counter.
    pub creation_order: u64,
}

/// Marker component indicating that the Mobject is visible and should be extracted for rendering.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Visible;

/// Marker component indicating that the entity represents an organizational group of children.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct GroupMarker;

/// User-facing descriptive tag for Mobject classification and query filtering.
#[derive(Component, Debug, Clone, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ObjectTag(pub String);

/// The target rendering backend layer for a given entity.
///
/// Serves as the foundation for the hybrid 2D/3D pipeline, instructing the compositor
/// which render pass should draw this entity.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Default)]
pub enum RenderLayer {
    /// Vector rendering backend using Vello (Default).
    #[default]
    Vello2D,
    /// Future 3D rasterization pipeline (wgpu-backed meshes).
    Wgpu3D,
    /// Overlay layer drawn in screen space on top of all cameras (HUD/Editor UI).
    Overlay,
}

/// A Bevy ECS component wrapping the zero-dependency `ObjectId`.
///
/// This resolves Bevy integration and Orphan Rule constraints while preserving
/// the isolation of `gaanim_core`.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct MobjectId(pub gaanim_core::ObjectId);

/// A metadata component attached to individual glyph and shape entities of text or equations,
/// tracking their character value, sequence index, and source range.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct TextSpan {
    /// The character represented by this entity (e.g. 'E', '=', '+', 'x', '2').
    pub character: char,
    /// The 0-indexed character sequence index within the flat string representation.
    pub char_index: usize,
    /// The source span range in the original source markup text.
    pub source_range: core::range::Range<usize>,
}

/// Billboard component: makes an entity always face the camera (for 3D labels).
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Billboard;

/// HUD overlay marker: entity is rendered in screen-space overlay layer.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct HudOverlay;

/// 3D mesh marker for PBR rendering (used to distinguish Wgpu3D meshes from Vello2D paths).
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Mesh3DMarker;

/// Marker for a Bevy presentation camera that must follow the scene camera rig.
///
/// Editor inspection cameras intentionally consume [`gaanim_math::ResolvedCamera`],
/// while presenter/export views ignore editor overrides but retain timeline camera
/// bindings and temporary rig constraints.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct AuthoritativeCameraView;

/// Stable Gaanim wrapper placed immediately above a native glTF node.
///
/// Blender-authored transforms remain on the native node while manual Gaanim
/// transforms are written to this wrapper, so both layers compose.
#[derive(Component, Debug, Clone, PartialEq, Eq)]
pub struct GltfNodeWrapper {
    pub node_index: usize,
    pub path: String,
}

/// One stable wrapper binding belonging to an imported glTF model.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GltfNodeBinding {
    pub node_index: usize,
    pub path: String,
    pub wrapper: Entity,
}

/// Deferred source metadata for a native Bevy glTF scene instance.
#[derive(Component, Debug, Clone)]
pub struct GltfModelRoot {
    pub path: PathBuf,
    pub scene_index: usize,
    pub nodes: Vec<GltfNodeBinding>,
    pub animation_names: Vec<String>,
}

/// Bevy handle used while a glTF asset is loaded asynchronously.
#[derive(Component, Debug, Clone)]
pub struct GltfAssetHandle(pub Handle<bevy::gltf::Gltf>);

/// Animation graph and player entities created for a ready glTF instance.
#[derive(Component, Debug, Clone)]
pub struct GltfAnimationState {
    pub graph: Handle<AnimationGraph>,
    pub nodes: Vec<AnimationNodeIndex>,
    pub players: Vec<Entity>,
}

/// Marker indicating that native nodes, wrappers and animations are linked.
#[derive(Component, Debug, Clone, Copy, Default)]
pub struct GltfModelReady;

/// Marker for the single neutral light supplied by Gaanim to imported models.
#[derive(Component, Debug, Clone, Copy, Default)]
pub struct GaanimDefault3dLight;

/// Renderer-neutral PBR parameters owned by a Gaanim 3D primitive.
///
/// Colors remain native `peniko::Color` values until the Bevy material is built,
/// keeping the public scene description independent from Bevy's renderer types.
#[derive(Component, Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Material3D {
    pub color: gaanim_core::peniko::Color,
    pub roughness: f32,
    pub metallic: f32,
    pub emissive: gaanim_core::peniko::Color,
    pub emissive_strength: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum Material3DError {
    #[error("roughness and metallic must be finite values in the 0..=1 range")]
    InvalidSurface,
    #[error("emissive strength must be finite and non-negative")]
    InvalidEmissiveStrength,
}

impl Material3D {
    pub fn new(
        color: gaanim_core::peniko::Color,
        roughness: f32,
        metallic: f32,
        emissive: Option<gaanim_core::peniko::Color>,
        emissive_strength: f32,
    ) -> Result<Self, Material3DError> {
        if !roughness.is_finite()
            || !metallic.is_finite()
            || !(0.0..=1.0).contains(&roughness)
            || !(0.0..=1.0).contains(&metallic)
        {
            return Err(Material3DError::InvalidSurface);
        }
        if !emissive_strength.is_finite() || emissive_strength < 0.0 {
            return Err(Material3DError::InvalidEmissiveStrength);
        }
        Ok(Self {
            color,
            roughness,
            metallic,
            emissive: emissive.unwrap_or(gaanim_core::peniko::Color::TRANSPARENT),
            emissive_strength,
        })
    }

    pub fn matte(color: gaanim_core::peniko::Color) -> Self {
        Self {
            color,
            ..Self::default()
        }
    }

    pub fn metal(color: gaanim_core::peniko::Color) -> Self {
        Self {
            color,
            roughness: 0.22,
            metallic: 1.0,
            ..Self::default()
        }
    }

    pub fn emissive(
        color: gaanim_core::peniko::Color,
        strength: f32,
    ) -> Result<Self, Material3DError> {
        Self::new(color, 0.45, 0.0, Some(color), strength)
    }

    pub fn lerp(self, other: Self, t: f64) -> Self {
        if t <= 0.0 {
            return self;
        }
        if t >= 1.0 {
            return other;
        }
        Self {
            color: gaanim_core::interpolate_color(self.color, other.color, t),
            roughness: self.roughness + (other.roughness - self.roughness) * t as f32,
            metallic: self.metallic + (other.metallic - self.metallic) * t as f32,
            emissive: gaanim_core::interpolate_color(self.emissive, other.emissive, t),
            emissive_strength: self.emissive_strength
                + (other.emissive_strength - self.emissive_strength) * t as f32,
        }
    }
}

impl Default for Material3D {
    fn default() -> Self {
        Self {
            color: gaanim_core::peniko::Color::WHITE,
            roughness: 0.55,
            metallic: 0.0,
            emissive: gaanim_core::peniko::Color::TRANSPARENT,
            emissive_strength: 0.0,
        }
    }
}

/// Baseline alpha mode for a Gaanim-owned PBR material.
#[derive(Component, Debug, Clone, Copy)]
pub struct Material3DBaseline {
    pub alpha: f32,
    pub alpha_mode: bevy::render::alpha::AlphaMode,
}

/// Scene-level automatic lighting policy for native 3D content.
#[derive(Resource, Debug, Clone, Copy, PartialEq)]
pub struct Lighting3D {
    pub enabled: bool,
    pub intensity: f32,
    pub shadows: bool,
}

impl Default for Lighting3D {
    fn default() -> Self {
        Self {
            enabled: true,
            intensity: 1.0,
            shadows: true,
        }
    }
}

/// Original material properties retained while wrapper opacity is animated.
#[derive(Component, Debug, Clone, Copy)]
pub struct GltfMaterialBaseline {
    pub alpha: f32,
    pub alpha_mode: bevy::render::alpha::AlphaMode,
}

/// Raw triangle mesh data to be converted to Bevy `Mesh3d` at runtime.
#[derive(Component, Debug, Clone)]
pub struct TriangleMeshData {
    pub vertices: Vec<[f32; 3]>,
    pub indices: Vec<u32>,
    /// Optional explicit vertex normals. Missing/invalid normals are generated.
    pub normals: Option<Vec<[f32; 3]>>,
    /// Optional UV0 coordinates. Missing/invalid UVs fall back to zeroes.
    pub uvs: Option<Vec<[f32; 2]>>,
    pub color: Option<gaanim_core::peniko::Color>,
    pub colors: Option<Vec<[f32; 4]>>,
    /// `None` preserves the historical unlit surface-mesh behavior.
    pub material: Option<Material3D>,
}

/// Raw line list data (pairs of points) to be converted to Bevy `Mesh3d` line list.
#[derive(Component, Debug, Clone)]
pub struct LineListData {
    pub points: Vec<[f32; 3]>,
    /// Indices as line pairs. If None, points are sequential pairs.
    pub indices: Option<Vec<u32>>,
    /// `true` connects every point as a strip; `false` consumes point pairs.
    pub strip: bool,
    pub color: gaanim_core::peniko::Color,
    /// Optional per-vertex RGBA colors (linear, 0..1). If Some, length must match `points`.
    /// When present the renderer uses vertex colors instead of the uniform `color`.
    pub colors: Option<Vec<[f32; 4]>>,
}

/// Immutable source geometry retained while `PathCompletion` reveals a 3D line.
#[derive(Component, Debug, Clone)]
pub struct LineListSource(pub LineListData);

#[cfg(test)]
mod material_3d_tests {
    use super::*;

    #[test]
    fn material_validates_pbr_ranges() {
        assert!(Material3D::new(gaanim_core::peniko::Color::WHITE, -0.1, 0.0, None, 0.0).is_err());
        assert!(Material3D::new(gaanim_core::peniko::Color::WHITE, 0.5, 1.1, None, 0.0).is_err());
        assert!(Material3D::new(gaanim_core::peniko::Color::WHITE, 0.5, 0.0, None, -1.0).is_err());
        assert!(Material3D::new(gaanim_core::peniko::Color::WHITE, 0.5, 0.0, None, 2.0).is_ok());
    }

    #[test]
    fn material_lerp_reaches_exact_endpoints() {
        let from = Material3D::matte(gaanim_core::peniko::Color::BLACK);
        let to = Material3D::new(
            gaanim_core::peniko::Color::WHITE,
            0.1,
            0.9,
            Some(gaanim_core::peniko::Color::WHITE),
            3.0,
        )
        .unwrap();
        assert_eq!(from.lerp(to, 0.0), from);
        assert_eq!(from.lerp(to, 1.0), to);
    }
}
