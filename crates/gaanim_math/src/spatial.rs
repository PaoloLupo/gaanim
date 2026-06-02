use bevy::prelude::Component;
use gaanim_core::glam::{DMat4, DQuat, DVec3};
use gaanim_core::kurbo::Affine;

/// A unified 2D/3D spatial transform representing translation, rotation, scale, and pivot (anchor).
///
/// This type represents coordinates and transforms in both 2D and 3D spaces,
/// mapping perfectly to Bevy's ECS architecture.
///
/// In 2D mode, the `z` coordinate of translation/scale/anchor remains 0 (or 1 for scale.z),
/// and the rotation is restricted to a quaternion representing rotation around the Z-axis.
#[derive(Component, Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct SpatialTransform {
    /// Translation in 3D space. For 2D, the Z coordinate should be 0.0.
    pub translation: DVec3,
    /// Rotation represented as a Double-Precision Quaternion.
    pub rotation: DQuat,
    /// Non-uniform scale in 3D space. For 2D, the Z scale should typically be 1.0.
    pub scale: DVec3,
    /// The pivot/anchor point in local space around which rotation and scaling occur.
    pub anchor: DVec3,
}

impl Default for SpatialTransform {
    fn default() -> Self {
        Self {
            translation: DVec3::ZERO,
            rotation: DQuat::IDENTITY,
            scale: DVec3::ONE,
            anchor: DVec3::ZERO,
        }
    }
}

impl SpatialTransform {
    /// Creates a new identity `SpatialTransform`.
    pub const fn identity() -> Self {
        Self {
            translation: DVec3::ZERO,
            rotation: DQuat::IDENTITY,
            scale: DVec3::ONE,
            anchor: DVec3::ZERO,
        }
    }

    /// Convenience constructor for a 2D spatial transform.
    pub fn new_2d(x: f64, y: f64) -> Self {
        Self {
            translation: DVec3::new(x, y, 0.0),
            rotation: DQuat::IDENTITY,
            scale: DVec3::ONE,
            anchor: DVec3::ZERO,
        }
    }

    /// Convenience constructor for a 3D spatial transform.
    pub fn new_3d(x: f64, y: f64, z: f64) -> Self {
        Self {
            translation: DVec3::new(x, y, z),
            rotation: DQuat::IDENTITY,
            scale: DVec3::ONE,
            anchor: DVec3::ZERO,
        }
    }

    /// Translates the transform by the given 2D offset.
    pub fn shift_2d(mut self, x: f64, y: f64) -> Self {
        self.translation.x += x;
        self.translation.y += y;
        self
    }

    /// Translates the transform by the given 3D vector.
    pub fn shift_3d(mut self, offset: DVec3) -> Self {
        self.translation += offset;
        self
    }

    /// Scales the transform uniformly across all axes.
    pub fn scale_uniform(mut self, s: f64) -> Self {
        self.scale *= s;
        self
    }

    /// Sets the rotation of the transform in 2D (rotation around the Z axis, in radians).
    pub fn with_rotation_2d(mut self, radians: f64) -> Self {
        self.rotation = DQuat::from_rotation_z(radians);
        self
    }

    /// Sets the rotation using 3D Euler angles (pitch, yaw, roll in radians).
    pub fn with_rotation_euler(mut self, pitch: f64, yaw: f64, roll: f64) -> Self {
        self.rotation = DQuat::from_euler(gaanim_core::glam::EulerRot::XYZ, pitch, yaw, roll);
        self
    }

    /// Sets the scale non-uniformly in 2D.
    pub fn with_scale_2d(mut self, sx: f64, sy: f64) -> Self {
        self.scale.x = sx;
        self.scale.y = sy;
        self
    }

    /// Sets the pivot/anchor point in local space.
    pub fn with_anchor(mut self, anchor: DVec3) -> Self {
        self.anchor = anchor;
        self
    }

    /// Extract the Z-axis rotation angle from the quaternion.
    ///
    /// In 2D mode the rotation is always around Z, so computing all three Euler
    /// angles and discarding X/Y is wasteful. This method directly computes only
    /// the Z angle from the quaternion's `z` and `w` components with a single
    /// `atan2` call.
    pub fn z_angle(&self) -> f64 {
        2.0 * f64::atan2(self.rotation.z, self.rotation.w)
    }

    /// Computes the 2D affine transformation matrix for Vello rendering.
    ///
    /// The transformation order takes the pivot/anchor into account:
    /// `translate(translation + anchor) * rotate(z_angle) * scale(scale.x, scale.y) * translate(-anchor)`
    pub fn to_affine_2d(&self) -> Affine {
        let z_angle = self.z_angle();

        Affine::translate((
            self.translation.x + self.anchor.x,
            self.translation.y + self.anchor.y,
        )) * Affine::rotate(z_angle)
            * Affine::scale_non_uniform(self.scale.x, self.scale.y)
            * Affine::translate((-self.anchor.x, -self.anchor.y))
    }

    /// Computes the 4x4 transformation matrix for a 3D rendering pipeline.
    ///
    /// Follows the pivot/anchor transformation sequence:
    /// `translate(translation + anchor) * rotate(rotation) * scale(scale) * translate(-anchor)`
    pub fn to_mat4(&self) -> DMat4 {
        let translate_pivot = DMat4::from_translation(self.translation + self.anchor);
        let rotate = DMat4::from_quat(self.rotation);
        let scale = DMat4::from_scale(self.scale);
        let translate_neg_pivot = DMat4::from_translation(-self.anchor);

        translate_pivot * rotate * scale * translate_neg_pivot
    }
}

/// The computed global spatial transform resulting from scene graph hierarchy propagation.
///
/// Stores the 2D `Affine` representation (for Vello) and, when the `dim3` feature
/// is enabled, the full `DMat4` representation for 3D coordinate spaces.
#[derive(Component, Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct GlobalSpatialTransform {
    /// Affine 2D representation for Vello vector rendering.
    pub affine_2d: Affine,
    /// 4x4 double-precision matrix for 3D coordinate spaces and cameras.
    #[cfg(feature = "dim3")]
    pub mat4: DMat4,
}

impl Default for GlobalSpatialTransform {
    fn default() -> Self {
        Self {
            affine_2d: Affine::IDENTITY,
            #[cfg(feature = "dim3")]
            mat4: DMat4::IDENTITY,
        }
    }
}

impl GlobalSpatialTransform {
    /// Computes a global transform directly from a local transform.
    pub fn from_local(local: &SpatialTransform) -> Self {
        Self {
            affine_2d: local.to_affine_2d(),
            #[cfg(feature = "dim3")]
            mat4: local.to_mat4(),
        }
    }

    /// Combines a parent global transform with a local child transform.
    pub fn from_parent_and_local(parent: &Self, local: &SpatialTransform) -> Self {
        Self {
            affine_2d: parent.affine_2d * local.to_affine_2d(),
            #[cfg(feature = "dim3")]
            mat4: parent.mat4 * local.to_mat4(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spatial_transform_default_is_identity() {
        let t = SpatialTransform::default();
        assert_eq!(t.translation, DVec3::ZERO);
        assert_eq!(t.rotation, DQuat::IDENTITY);
        assert_eq!(t.scale, DVec3::ONE);
        assert_eq!(t.anchor, DVec3::ZERO);
    }

    #[test]
    fn spatial_transform_new_2d() {
        let t = SpatialTransform::new_2d(10.0, -5.0);
        assert_eq!(t.translation, DVec3::new(10.0, -5.0, 0.0));
        assert_eq!(t.rotation, DQuat::IDENTITY);
        assert_eq!(t.scale, DVec3::ONE);
    }

    #[test]
    fn spatial_transform_shift_2d() {
        let t = SpatialTransform::new_2d(1.0, 2.0).shift_2d(3.0, 4.0);
        assert_eq!(t.translation, DVec3::new(4.0, 6.0, 0.0));
    }

    #[test]
    fn spatial_transform_scale_uniform() {
        let t = SpatialTransform::default().scale_uniform(2.0);
        assert_eq!(t.scale, DVec3::new(2.0, 2.0, 2.0));
    }

    #[test]
    fn spatial_transform_with_rotation_2d() {
        let t = SpatialTransform::default().with_rotation_2d(std::f64::consts::FRAC_PI_2);
        let (_, _, z) = t.rotation.to_euler(gaanim_core::glam::EulerRot::XYZ);
        assert!((z - std::f64::consts::FRAC_PI_2).abs() < 1e-9);
    }

    #[test]
    fn spatial_transform_to_affine_2d_identity() {
        let t = SpatialTransform::default();
        let affine = t.to_affine_2d();
        assert_eq!(affine, Affine::IDENTITY);
    }

    #[test]
    fn spatial_transform_to_affine_2d_translation() {
        let t = SpatialTransform::new_2d(5.0, -3.0);
        let affine = t.to_affine_2d();
        let point = kurbo::Point::new(0.0, 0.0);
        let transformed = affine * point;
        assert!((transformed.x - 5.0).abs() < 1e-9);
        assert!((transformed.y - (-3.0)).abs() < 1e-9);
    }

    #[test]
    fn spatial_transform_to_affine_2d_with_anchor() {
        let t = SpatialTransform::new_2d(0.0, 0.0)
            .with_anchor(DVec3::new(10.0, 0.0, 0.0))
            .with_rotation_2d(std::f64::consts::PI);
        let affine = t.to_affine_2d();
        let point = kurbo::Point::new(10.0, 0.0);
        let transformed = affine * point;
        // A 180° rotation around (10,0) should keep (10,0) fixed
        assert!((transformed.x - 10.0).abs() < 1e-9);
        assert!((transformed.y - 0.0).abs() < 1e-9);
    }

    #[cfg(feature = "dim3")]
    #[test]
    fn global_spatial_transform_from_local_matches_affine_and_mat4() {
        let local = SpatialTransform::new_2d(3.0, 4.0).scale_uniform(2.0);
        let global = GlobalSpatialTransform::from_local(&local);

        let expected_affine = local.to_affine_2d();
        let expected_mat4 = local.to_mat4();

        assert_eq!(global.affine_2d, expected_affine);
        assert_eq!(global.mat4, expected_mat4);
    }

    #[test]
    fn global_spatial_transform_hierarchy_composition() {
        let parent_local = SpatialTransform::new_2d(10.0, 0.0);
        let child_local = SpatialTransform::new_2d(5.0, 0.0);

        let parent_global = GlobalSpatialTransform::from_local(&parent_local);
        let child_global =
            GlobalSpatialTransform::from_parent_and_local(&parent_global, &child_local);

        // Child at local 5 under parent at 10 should be at world 15
        let point = kurbo::Point::new(0.0, 0.0);
        let transformed = child_global.affine_2d * point;
        assert!((transformed.x - 15.0).abs() < 1e-9);
    }
}
