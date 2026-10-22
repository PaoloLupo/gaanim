//! What an imported model's primitives add to plain triangle meshes: a
//! base-color texture read through the mesh's UVs, a normal map, how its
//! alpha is read, whether it shows both faces, the joints and weights that
//! skin it, and its morph targets.
//!
//! A skinned primitive carries [`MeshSkin`]: the entities of its joints and
//! their inverse bind matrices. Each frame, after transforms propagate,
//! [`update_skin_palettes_system`] writes its [`SkinPalette`], the matrix of
//! each joint relative to the mesh, which the renderer's vertex shader
//! blends by the weights. The mesh itself stays uploaded once: only the
//! palette, some tens of matrices, changes from frame to frame.
//!
//! A primitive with morph targets carries [`MorphWeights`], one per target,
//! which its model's animations or a script write. The renderer uploads the
//! targets' displacements once and adds them, times their weights, to the
//! vertices it draws, before the skin moves them: a frame changes only the
//! weights.

use std::sync::Arc;

use bevy::prelude::{Changed, Component, Entity, Query};
use gaanim_core::glam::DMat4;
use gaanim_core::peniko::ImageData;
use gaanim_math::GlobalSpatialTransform;

/// Most joints a skin moves: the renderer binds a palette of this many
/// matrices per draw.
pub const MAX_JOINTS: usize = 256;

/// How a texture repeats past the 0..1 range of its coordinates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum TextureWrap {
    #[default]
    Repeat,
    MirroredRepeat,
    ClampToEdge,
}

/// A base-color texture: sRGB pixels, multiplied with the surface's base
/// color.
#[derive(Debug, Clone, PartialEq)]
pub struct MeshTexture {
    /// Straight-alpha RGBA8 pixels in sRGB.
    pub image: ImageData,
    /// How it repeats along u and v.
    pub wrap: [TextureWrap; 2],
    /// Nearest-texel magnification, for pixel art; otherwise linear.
    pub nearest: bool,
}

/// How a surface reads its alpha.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum AlphaMode {
    /// Fully opaque: alpha is ignored.
    #[default]
    Opaque,
    /// Opaque where alpha reaches the cutoff, absent elsewhere, as leaves
    /// cut out of a card.
    Mask(f32),
    /// Blended over what lies behind, as one translucent layer.
    Blend,
}

/// The joints and weights of a skinned mesh's vertices: up to four joints
/// each, indices into its [`MeshSkin`], with weights that sum to one.
#[derive(Debug, Clone, PartialEq)]
pub struct SkinWeights {
    pub joints: Vec<[u16; 4]>,
    pub weights: Vec<[f32; 4]>,
}

/// A tangent-space normal map, read through the mesh's UVs: each texel
/// tilts the surface's normal toward the vertices' tangents and bitangents,
/// so a flat face shades as if it held the bumps the map was baked from.
#[derive(Debug, Clone, PartialEq)]
pub struct NormalMap {
    /// Linear RGBA8 texels, red, green and blue holding x, y and z from -1
    /// (0) to 1 (255): along the tangent, the bitangent and the normal.
    pub texture: MeshTexture,
    /// What x and y are multiplied by: 0 leaves the normals as they are.
    pub scale: f32,
    /// A tangent per vertex, pointing where u grows: `w`, 1 or -1, is the
    /// sign of the bitangent `cross(normal, tangent) * w`, which points up
    /// the image, where green grows: where v shrinks, since glTF's v runs
    /// down it.
    pub tangents: Vec<[f32; 4]>,
}

/// One morph target of a mesh: where each vertex moves, and how its normal
/// turns, at a weight of one. A mesh draws its vertices plus each target's
/// displacements times the target's weight ([`MorphWeights`]).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct MorphTarget {
    /// A displacement per vertex, in the mesh's coordinates.
    pub positions: Vec<[f32; 3]>,
    /// What each vertex's normal gains, before it is normalized.
    pub normals: Option<Vec<[f32; 3]>>,
}

/// What an imported model's primitive adds to [`crate::TriangleMeshData`].
#[derive(Debug, Clone, PartialEq, Default)]
pub struct MeshSurface {
    pub texture: Option<MeshTexture>,
    pub normal_map: Option<NormalMap>,
    pub alpha: AlphaMode,
    /// Shows its back faces too; lit meshes otherwise hide them.
    pub double_sided: bool,
    pub skin: Option<SkinWeights>,
    /// Its morph targets, each with a displacement per vertex.
    pub morph_targets: Vec<MorphTarget>,
}

impl MeshSurface {
    /// Whether the surface draws translucent pixels, before the opacity of
    /// its drawable and the alpha of its colors.
    pub fn blends(&self) -> bool {
        matches!(self.alpha, AlphaMode::Blend)
    }
}

/// The joints that skin a mesh: an entity per joint, whose world matrix
/// moves the vertices weighted to it, and the inverse of its world matrix
/// in the pose the mesh was bound in.
#[derive(Component, Debug, Clone)]
pub struct MeshSkin {
    pub joints: Arc<[Entity]>,
    pub inverse_bind: Arc<[DMat4]>,
}

/// The matrices of a skinned mesh's joints this frame, relative to the
/// mesh (see [`update_skin_palettes_system`]), column-major, which the
/// renderer blends by each vertex's weights.
#[derive(Component, Debug, Clone, Default, PartialEq)]
pub struct SkinPalette(pub Arc<[[f32; 16]]>);

/// The weights of a mesh's morph targets (see [`MorphTarget`]), one per
/// target, as its model's animations or a script set them this frame.
#[derive(Component, Debug, Clone, Default, PartialEq)]
pub struct MorphWeights(pub Arc<[f32]>);

/// The palette of a mesh at world matrix `mesh` whose joints are at world
/// matrices `joints` and were bound with `inverse_bind`: each joint's matrix
/// in the mesh's space, so that the mesh's own matrix places the result.
/// A mesh with no inverse (scaled to nothing) draws nothing, and gets the
/// identity.
pub fn skin_palette(mesh: DMat4, joints: &[DMat4], inverse_bind: &[DMat4]) -> Vec<[f32; 16]> {
    let to_mesh = if mesh.determinant().abs() > 1e-18 {
        mesh.inverse()
    } else {
        DMat4::IDENTITY
    };
    joints
        .iter()
        .zip(inverse_bind)
        .take(MAX_JOINTS)
        .map(|(joint, bind)| {
            let matrix = to_mesh * *joint * *bind;
            let matrix = if matrix.is_finite() {
                matrix
            } else {
                DMat4::IDENTITY
            };
            matrix.to_cols_array().map(|value| value as f32)
        })
        .collect()
}

/// Writes the [`SkinPalette`] of every skinned mesh from the world matrices
/// of its joints, once transforms have propagated. A palette that did not
/// change is left alone, so recordings keep it and the renderer redraws
/// nothing.
pub fn update_skin_palettes_system(
    mut meshes: Query<(
        &MeshSkin,
        &GlobalSpatialTransform,
        Option<&mut SkinPalette>,
        Entity,
    )>,
    joints: Query<&GlobalSpatialTransform>,
    moved: Query<(), Changed<GlobalSpatialTransform>>,
    mut commands: bevy::prelude::Commands,
) {
    for (skin, mesh, palette, entity) in &mut meshes {
        let stale = palette.is_none()
            || moved.contains(entity)
            || skin.joints.iter().any(|joint| moved.contains(*joint));
        if !stale {
            continue;
        }
        let world: Vec<DMat4> = skin
            .joints
            .iter()
            .map(|joint| {
                joints
                    .get(*joint)
                    .map_or(DMat4::IDENTITY, |transform| transform.mat4)
            })
            .collect();
        let matrices = skin_palette(mesh.mat4, &world, &skin.inverse_bind);
        match palette {
            Some(mut palette) => {
                if palette.0[..] != matrices[..] {
                    palette.0 = matrices.into();
                }
            }
            None => {
                commands.entity(entity).insert(SkinPalette(matrices.into()));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gaanim_core::glam::{DQuat, DVec3};

    #[test]
    fn a_joint_at_its_bind_pose_draws_the_vertices_where_they_were_bound() {
        // glTF ignores the skinned mesh's own matrix: at the bind pose its
        // vertices lie where they were modelled, whatever that matrix,
        // which the renderer applies after the palette.
        let mesh = DMat4::from_translation(DVec3::new(2.0, 0.0, 0.0));
        let joint = DMat4::from_rotation_translation(
            DQuat::from_rotation_z(0.7),
            DVec3::new(1.0, 1.0, 0.0),
        );
        let palette = skin_palette(mesh, &[joint], &[joint.inverse()]);
        let matrix = DMat4::from_cols_array(&palette[0].map(f64::from));
        assert!((mesh * matrix).abs_diff_eq(DMat4::IDENTITY, 1e-6));
    }

    #[test]
    fn a_joint_that_moves_moves_its_vertices_in_the_mesh_space() {
        // The mesh's node is ignored: a joint moved by one unit up moves
        // its vertices by one unit up in the world, whatever the mesh's
        // own matrix, which the renderer applies after the palette.
        let mesh = DMat4::from_scale(DVec3::splat(2.0));
        let bind = DMat4::IDENTITY;
        let joint = DMat4::from_translation(DVec3::Y);
        let palette = skin_palette(mesh, &[joint], &[bind.inverse()]);
        let matrix = DMat4::from_cols_array(&palette[0].map(f64::from));
        let world = mesh * matrix;
        let moved = world.transform_point3(DVec3::ZERO);
        assert!(moved.abs_diff_eq(DVec3::Y, 1e-6), "{moved}");
    }

    #[test]
    fn a_mesh_scaled_to_nothing_gets_finite_matrices() {
        let palette = skin_palette(DMat4::ZERO, &[DMat4::IDENTITY], &[DMat4::IDENTITY]);
        assert!(palette[0].iter().all(|value| value.is_finite()));
    }
}
