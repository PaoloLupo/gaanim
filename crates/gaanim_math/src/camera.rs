use bevy::prelude::Resource;
use gaanim_core::glam::{DMat4, DQuat, DVec2, DVec3};
use gaanim_core::kurbo::Affine;

/// Extensible camera projection types.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Projection {
    /// 2D Orthographic projection with a scale factor.
    Orthographic {
        /// Zoom multiplier. 1.0 represents the default scaling.
        zoom: f64,
    },
    /// 3D Perspective projection (used for 3D plugins).
    Perspective {
        /// Vertical field of view in radians.
        fov_y: f64,
        /// Minimum rendering depth.
        near: f64,
        /// Maximum rendering depth.
        far: f64,
    },
}

/// A dimension-agnostic camera supporting both 2D Vector (Vello) and 3D Raster (wgpu) rendering.
///
/// This serves as a global scene resource that defines the viewpoint, rotation, zoom/fov,
/// and the active viewport dimensions.
#[derive(Resource, Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Camera {
    /// Position in 3D world space. In 2D, the Z coordinate is typically 0.0.
    pub position: DVec3,
    /// Rotation represented as a Double-Precision Quaternion.
    pub rotation: DQuat,
    /// The projection settings (orthographic or perspective).
    pub projection: Projection,
    /// Pixel width of the active rendering area.
    pub viewport_width: u32,
    /// Pixel height of the active rendering area.
    pub viewport_height: u32,
}

impl Camera {
    /// Creates a default orthographic camera for a given viewport size.
    pub fn ortho_2d(width: u32, height: u32) -> Self {
        Self {
            position: DVec3::ZERO,
            rotation: DQuat::IDENTITY,
            projection: Projection::Orthographic { zoom: 1.0 },
            viewport_width: width,
            viewport_height: height,
        }
    }

    /// Creates a perspective camera for 3D coordinate rendering.
    pub fn perspective_3d(width: u32, height: u32, fov_y: f64) -> Self {
        Self {
            position: DVec3::new(0.0, 0.0, 10.0),
            rotation: DQuat::IDENTITY,
            projection: Projection::Perspective {
                fov_y,
                near: 0.1,
                far: 1000.0,
            },
            viewport_width: width,
            viewport_height: height,
        }
    }

    /// Computes the double-precision view matrix.
    pub fn view_matrix(&self) -> DMat4 {
        DMat4::from_rotation_translation(self.rotation, self.position).inverse()
    }

    /// Computes the double-precision projection matrix.
    pub fn projection_matrix(&self) -> DMat4 {
        match self.projection {
            Projection::Orthographic { zoom } => {
                let hw = (self.viewport_width as f64) / (2.0 * zoom);
                let hh = (self.viewport_height as f64) / (2.0 * zoom);
                DMat4::orthographic_rh(-hw, hw, -hh, hh, -1000.0, 1000.0)
            }
            Projection::Perspective { fov_y, near, far } => {
                let aspect = (self.viewport_width as f64) / (self.viewport_height as f64);
                DMat4::perspective_rh(fov_y, aspect, near, far)
            }
        }
    }

    /// Computes the 2D affine transformation matrix for Vello (only when Orthographic projection is used).
    ///
    /// Maps coordinates from world space into centered pixel coordinates:
    /// `translate(hw, hh) * rotate(-z_angle) * scale(zoom) * translate(-cam_pos.x, -cam_pos.y)`
    pub fn to_vello_transform(&self) -> Affine {
        let zoom = match self.projection {
            Projection::Orthographic { zoom } => zoom,
            _ => 1.0,
        };
        let (_, _, z_angle) = self.rotation.to_euler(gaanim_core::glam::EulerRot::XYZ);
        let hw = (self.viewport_width as f64) / 2.0;
        let hh = (self.viewport_height as f64) / 2.0;

        Affine::translate((hw, hh))
            * Affine::rotate(-z_angle)
            * Affine::scale(zoom)
            * Affine::translate((-self.position.x, -self.position.y))
    }

    /// Converts a world coordinate into screen coordinates (pixels measured from top-left corner).
    pub fn world_to_screen(&self, world: DVec3) -> DVec2 {
        let view_proj = self.projection_matrix() * self.view_matrix();
        let ndc = view_proj.project_point3(world);

        let screen_x = (ndc.x + 1.0) * 0.5 * (self.viewport_width as f64);
        let screen_y = (1.0 - ndc.y) * 0.5 * (self.viewport_height as f64);

        DVec2::new(screen_x, screen_y)
    }

    /// Converts screen pixel coordinates (measured from top-left corner) back into a world coordinate on the Z = 0 plane.
    pub fn screen_to_world(&self, screen: DVec2) -> DVec3 {
        let view_proj = self.projection_matrix() * self.view_matrix();
        let inv_view_proj = view_proj.inverse();

        let ndc_x = (screen.x / (self.viewport_width as f64)) * 2.0 - 1.0;
        let ndc_y = 1.0 - (screen.y / (self.viewport_height as f64)) * 2.0;

        let near_world = inv_view_proj.project_point3(DVec3::new(ndc_x, ndc_y, -1.0));
        let far_world = inv_view_proj.project_point3(DVec3::new(ndc_x, ndc_y, 1.0));

        let dir_z = far_world.z - near_world.z;
        if dir_z.abs() < 1e-6 {
            near_world
        } else {
            let t = -near_world.z / dir_z;
            near_world + (far_world - near_world) * t
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn camera_ortho_2d_default() {
        let cam = Camera::ortho_2d(1280, 720);
        assert_eq!(cam.position, DVec3::ZERO);
        assert_eq!(cam.rotation, DQuat::IDENTITY);
        assert_eq!(cam.viewport_width, 1280);
        assert_eq!(cam.viewport_height, 720);
        assert!(matches!(
            cam.projection,
            Projection::Orthographic { zoom: 1.0 }
        ));
    }

    #[test]
    fn camera_to_vello_transform_identity() {
        let cam = Camera::ortho_2d(100, 100);
        let affine = cam.to_vello_transform();
        // Origin (0,0) in world space should map to center of viewport (50,50)
        let p = kurbo::Point::new(0.0, 0.0);
        let t = affine * p;
        assert!((t.x - 50.0).abs() < 1e-9);
        assert!((t.y - 50.0).abs() < 1e-9);
    }

    #[test]
    fn camera_to_vello_transform_with_translation() {
        let mut cam = Camera::ortho_2d(100, 100);
        cam.position = DVec3::new(10.0, 20.0, 0.0);
        let affine = cam.to_vello_transform();
        // World origin shifted by (-10, -20), then centered => (40, 30)
        let p = kurbo::Point::new(0.0, 0.0);
        let t = affine * p;
        assert!((t.x - 40.0).abs() < 1e-9);
        assert!((t.y - 30.0).abs() < 1e-9);
    }

    #[test]
    fn camera_world_to_screen_roundtrip() {
        let cam = Camera::ortho_2d(100, 100);
        let world = DVec3::new(10.0, 20.0, 0.0);
        let screen = cam.world_to_screen(world);
        let back = cam.screen_to_world(screen);
        assert!((back.x - world.x).abs() < 1e-6);
        assert!((back.y - world.y).abs() < 1e-6);
    }

    #[test]
    fn camera_projection_matrix_ortho() {
        let cam = Camera::ortho_2d(100, 100);
        let proj = cam.projection_matrix();
        // In ortho projection, a point at origin should remain near origin in clip space
        let clip = proj.project_point3(DVec3::ZERO);
        assert!(clip.x.abs() < 1e-9);
        assert!(clip.y.abs() < 1e-9);
    }

    #[test]
    fn camera_perspective_3d_projection_matrix() {
        let cam = Camera::perspective_3d(1920, 1080, std::f64::consts::FRAC_PI_4);
        let proj = cam.projection_matrix();
        // Ensure projection matrix is not identity and has expected structure
        assert_ne!(proj, DMat4::IDENTITY);
    }
}
