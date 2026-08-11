use bevy::prelude::Resource;
use gaanim_core::glam::{DMat4, DQuat, DVec2, DVec3};
use gaanim_core::kurbo::Affine;
use std::ops::{Deref, DerefMut};

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
/// and the active viewport dimensions. For perspective cameras `target` is the
/// orbit pivot / look-at point and `up` is the world up direction.
#[derive(Resource, Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Camera {
    /// Position in 3D world space. In 2D, the Z coordinate is typically 0.0.
    pub position: DVec3,
    /// Rotation represented as a Double-Precision Quaternion.
    pub rotation: DQuat,
    /// Orbit / look-at target in world space. Used by perspective orbit controls.
    pub target: DVec3,
    /// World up direction (usually Y-up).
    pub up: DVec3,
    /// The projection settings (orthographic or perspective).
    pub projection: Projection,
    /// Pixel width of the active rendering area.
    pub viewport_width: u32,
    /// Pixel height of the active rendering area.
    pub viewport_height: u32,
    /// Additional vertical pixel offset applied to the Vello transform center.
    /// Used by the editor to shift content above UI panels (positive = shift up).
    pub viewport_offset_y: f64,
    /// Additional scale factor applied to the Vello transform (on top of zoom).
    /// Used by the editor to fit content in the available area below UI panels.
    pub viewport_scale: f64,
}

/// Camera consumed by rendering, billboards, overlays and picking.
///
/// The authored [`Camera`] remains the timeline authority. Editor-only views
/// are composed here without changing authored scene state.
#[derive(Resource, Debug, Clone, Copy, PartialEq)]
pub struct ResolvedCamera(pub Camera);

impl Default for ResolvedCamera {
    fn default() -> Self {
        Self(Camera::ortho_2d(1280, 720))
    }
}

impl Deref for ResolvedCamera {
    type Target = Camera;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for ResolvedCamera {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

/// Optional presentation-only override, primarily owned by the editor.
#[derive(Resource, Debug, Clone, Copy, Default, PartialEq)]
pub struct CameraViewOverride(pub Option<Camera>);

impl Camera {
    /// Creates a default orthographic camera for a given viewport size.
    pub fn ortho_2d(width: u32, height: u32) -> Self {
        Self {
            position: DVec3::ZERO,
            rotation: DQuat::IDENTITY,
            target: DVec3::ZERO,
            up: DVec3::Y,
            projection: Projection::Orthographic { zoom: 1.0 },
            viewport_width: width,
            viewport_height: height,
            viewport_offset_y: 0.0,
            viewport_scale: 1.0,
        }
    }

    /// Creates a perspective camera for 3D coordinate rendering.
    pub fn perspective_3d(width: u32, height: u32, fov_y: f64) -> Self {
        Self {
            position: DVec3::new(0.0, 0.0, 10.0),
            rotation: DQuat::IDENTITY,
            target: DVec3::ZERO,
            up: DVec3::Y,
            projection: Projection::Perspective {
                fov_y,
                near: 0.1,
                far: 1000.0,
            },
            viewport_width: width,
            viewport_height: height,
            viewport_offset_y: 0.0,
            viewport_scale: 1.0,
        }
    }

    /// Sets this camera to look at `target` from `eye` with the given `up`.
    pub fn look_at(&mut self, eye: DVec3, target: DVec3, up: DVec3) {
        self.position = eye;
        self.target = target;
        self.up = up;
        let view = DMat4::look_at_rh(eye, target, up);
        // view = (T*R)^-1  => T*R = view.inverse()
        let cam_to_world = view.inverse();
        let (_scale, rot, _trans) = cam_to_world.to_scale_rotation_translation();
        self.rotation = rot;
    }

    /// Orbits around `target` by yaw/pitch deltas (radians). Pitch clamped to +-89°.
    pub fn orbit_around_target(&mut self, delta_yaw: f64, delta_pitch: f64) {
        let dir = self.position - self.target;
        let radius = dir.length().max(0.01);
        let mut yaw = f64::atan2(dir.x, dir.z);
        let mut pitch = (dir.y / radius).asin();
        yaw += delta_yaw;
        pitch = (pitch + delta_pitch).clamp(-1.5533, 1.5533); // ~89°
        let cp = pitch.cos();
        let sp = pitch.sin();
        let cy = yaw.cos();
        let sy = yaw.sin();
        let new_dir = DVec3::new(sy * cp, sp, cy * cp) * radius;
        self.position = self.target + new_dir;
        self.look_at(self.position, self.target, self.up);
    }

    /// Pan in screen space (pixels scaled to world).
    pub fn pan_screen_delta(&mut self, delta: DVec2) {
        // Right and up vectors from rotation
        let right = self.rotation * DVec3::X;
        let up_dir = self.rotation * DVec3::Y;
        // Scale delta by distance for perspective, or 1/zoom for ortho
        let scale = match self.projection {
            Projection::Perspective { .. } => (self.position - self.target).length() * 0.002,
            Projection::Orthographic { zoom } => 1.0 / (zoom * self.viewport_scale).max(0.1),
        };
        let move_vec = -right * delta.x * scale + up_dir * delta.y * scale;
        self.position += move_vec;
        self.target += move_vec;
    }

    /// Dolly (move closer/further to target) by factor (<1 closer, >1 further).
    pub fn dolly(&mut self, factor: f64) {
        let dir = self.position - self.target;
        let new_pos = self.target + dir * factor.clamp(0.1, 10.0);
        // Prevent crossing target
        if (new_pos - self.target).length() > 0.1 {
            self.position = new_pos;
        }
    }

    /// Set perspective projection parameters.
    pub fn set_perspective(&mut self, fov_y: f64, near: f64, far: f64) {
        self.projection = Projection::Perspective { fov_y, near, far };
    }

    /// Spherical coords (radius, yaw, pitch) relative to target.
    pub fn spherical(&self) -> (f64, f64, f64) {
        let dir = self.position - self.target;
        let r = dir.length();
        let yaw = f64::atan2(dir.x, dir.z);
        let pitch = if r > 1e-9 { (dir.y / r).asin() } else { 0.0 };
        (r, yaw, pitch)
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

    /// Extract the Z-axis rotation angle from the camera's quaternion.
    ///
    /// Like [`SpatialTransform::z_angle`], this avoids computing unused Euler angles.
    pub fn z_angle(&self) -> f64 {
        2.0 * f64::atan2(self.rotation.z, self.rotation.w)
    }

    /// Computes the 2D affine transformation matrix for Vello (only when Orthographic projection is used).
    ///
    /// Maps Y-up world coordinates into Vello's Y-down pixel coordinates.
    pub fn to_vello_transform(&self) -> Affine {
        let zoom = match self.projection {
            Projection::Orthographic { zoom } => zoom,
            _ => 1.0,
        };
        let effective_zoom = zoom * self.viewport_scale;
        let z_angle = self.z_angle();
        let hw = (self.viewport_width as f64) / 2.0;
        let hh = (self.viewport_height as f64) / 2.0 + self.viewport_offset_y;

        Affine::translate((hw, hh))
            * Affine::scale_non_uniform(effective_zoom, -effective_zoom)
            * Affine::rotate(-z_angle)
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

    /// Computes a world-space ray from a screen position (for 3D picking).
    pub fn screen_to_ray(&self, screen: DVec2) -> (DVec3, DVec3) {
        let view_proj = self.projection_matrix() * self.view_matrix();
        let inv = view_proj.inverse();
        let ndc_x = (screen.x / (self.viewport_width as f64)) * 2.0 - 1.0;
        let ndc_y = 1.0 - (screen.y / (self.viewport_height as f64)) * 2.0;
        let near = inv.project_point3(DVec3::new(ndc_x, ndc_y, -1.0));
        let far = inv.project_point3(DVec3::new(ndc_x, ndc_y, 1.0));
        let dir = (far - near).normalize_or_zero();
        (near, dir)
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
        // World origin is left and below the camera => (40, 70) in Y-down pixels.
        let p = kurbo::Point::new(0.0, 0.0);
        let t = affine * p;
        assert!((t.x - 40.0).abs() < 1e-9);
        assert!((t.y - 70.0).abs() < 1e-9);
    }

    #[test]
    fn camera_to_vello_maps_positive_world_y_upward() {
        let cam = Camera::ortho_2d(100, 100);
        let screen = cam.to_vello_transform() * kurbo::Point::new(0.0, 10.0);

        assert_eq!(screen, kurbo::Point::new(50.0, 40.0));
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

    #[test]
    fn debug_billboard_positions() {
        let mut cam = Camera::ortho_2d(1280, 720);
        cam.set_perspective(0.785, 0.1, 1000.0);
        cam.look_at(DVec3::new(8.0, 6.0, 8.0), DVec3::ZERO, DVec3::Y);
        cam.viewport_scale = 1.0;
        cam.viewport_offset_y = 0.0;
        let points = [
            DVec3::new(0.0, 0.0, 0.5),
            DVec3::new(658.0, 0.0, 0.0),
            DVec3::new(0.0, 658.0, 0.0),
            DVec3::new(0.0, 0.0, 358.0),
            DVec3::new(5.0, 0.0, 0.0),
            DVec3::new(-5.0, 0.0, 0.0),
            DVec3::new(0.0, 5.0, 0.0),
            DVec3::new(0.0, -5.0, 0.0),
            DVec3::new(0.0, 0.0, 3.0),
            DVec3::new(0.0, 0.0, -3.0),
        ];
        for pt in points {
            let screen = cam.world_to_screen(pt);
            println!("world {:?} -> screen {:?}", pt, screen);
            let eff = cam.viewport_scale.max(0.01);
            let hw = cam.viewport_width as f64 * 0.5;
            let hh = cam.viewport_height as f64 * 0.5 + cam.viewport_offset_y;
            let vello = gaanim_core::kurbo::Affine::translate((hw, hh))
                * gaanim_core::kurbo::Affine::scale_non_uniform(eff, -eff);
            let inv = vello.inverse();
            let vpos = inv * gaanim_core::kurbo::Point::new(screen.x, screen.y);
            println!("  vpos {:?}", vpos);
        }
    }
}
