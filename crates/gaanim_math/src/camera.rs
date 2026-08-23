use bevy::prelude::Resource;
use gaanim_core::glam::{DMat4, DQuat, DVec2, DVec3};
use gaanim_core::kurbo::Affine;
use std::ops::{Deref, DerefMut};

/// Invalid authored camera or host viewport state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum CameraValidationError {
    #[error("camera values must be finite")]
    NonFinite,
    #[error("camera viewport must be non-empty")]
    EmptyViewport,
    #[error("orthographic zoom must be greater than zero")]
    InvalidZoom,
    #[error("perspective projection requires 0 < fov_y < pi")]
    InvalidFov,
    #[error("perspective clipping requires 0 < near < far")]
    InvalidClipping,
    #[error("camera eye and target must differ")]
    CoincidentEyeTarget,
    #[error("camera up must be non-zero and non-collinear with the view direction")]
    InvalidUp,
    #[error("viewport scale must be finite and greater than zero")]
    InvalidViewportScale,
}

/// Presentation-only mapping from the logical canvas into a host viewport.
///
/// This state is deliberately separate from [`Camera`]: editor fit and panel
/// offsets must never become authored timeline state or leak into snapshots.
#[derive(Resource, Debug, Clone, Copy, PartialEq)]
pub struct CameraViewport {
    /// Scale applied after the authored projection to fit the host viewport.
    pub scale: f64,
    /// Vertical host-pixel offset of the logical canvas centre.
    pub offset_y: f64,
}

impl Default for CameraViewport {
    fn default() -> Self {
        Self {
            scale: 1.0,
            offset_y: 0.0,
        }
    }
}

impl CameraViewport {
    pub fn validate(self) -> Result<(), CameraValidationError> {
        if !self.scale.is_finite() || self.scale <= 0.0 {
            return Err(CameraValidationError::InvalidViewportScale);
        }
        if !self.offset_y.is_finite() {
            return Err(CameraValidationError::NonFinite);
        }
        Ok(())
    }
}

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

/// Complete authored camera pose independent of canvas or host viewport size.
///
/// A pose can be saved and reused across scenes with different output
/// resolutions. Applying it to a [`Camera`] preserves that camera's logical
/// viewport dimensions.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct CameraPose {
    pub position: DVec3,
    pub rotation: DQuat,
    pub target: DVec3,
    pub up: DVec3,
    pub projection: Projection,
}

impl CameraPose {
    /// Build and validate an orthographic 2D pose.
    pub fn orthographic_2d(
        center: DVec2,
        zoom: f64,
        rotation: f64,
    ) -> Result<Self, CameraValidationError> {
        if !center.is_finite() || !rotation.is_finite() {
            return Err(CameraValidationError::NonFinite);
        }
        if !zoom.is_finite() || zoom <= 0.0 {
            return Err(CameraValidationError::InvalidZoom);
        }
        Ok(Self {
            position: center.extend(0.0),
            rotation: DQuat::from_rotation_z(rotation),
            target: DVec3::ZERO,
            up: DVec3::Y,
            projection: Projection::Orthographic { zoom },
        })
    }

    /// Build and validate a perspective look-at pose.
    pub fn perspective_3d(
        eye: DVec3,
        target: DVec3,
        up: DVec3,
        fov_y: f64,
        near: f64,
        far: f64,
    ) -> Result<Self, CameraValidationError> {
        Camera::validate_look_at(eye, target, up)?;
        Camera::validate_perspective(fov_y, near, far)?;
        let view = DMat4::look_at_rh(eye, target, up);
        let rotation = view.inverse().to_scale_rotation_translation().1;
        Ok(Self {
            position: eye,
            rotation,
            target,
            up: up.normalize(),
            projection: Projection::Perspective { fov_y, near, far },
        })
    }

    /// Validate all pose and projection invariants.
    pub fn validate(&self) -> Result<(), CameraValidationError> {
        if !self.position.is_finite()
            || !self.rotation.is_finite()
            || !self.target.is_finite()
            || !self.up.is_finite()
        {
            return Err(CameraValidationError::NonFinite);
        }
        match self.projection {
            Projection::Orthographic { zoom } => {
                if !zoom.is_finite() || zoom <= 0.0 {
                    return Err(CameraValidationError::InvalidZoom);
                }
            }
            Projection::Perspective { fov_y, near, far } => {
                Camera::validate_perspective(fov_y, near, far)?;
                Camera::validate_look_at(self.position, self.target, self.up)?;
            }
        }
        Ok(())
    }

    /// Interpolate complete authored state using the destination projection.
    ///
    /// Cross-projection transitions select the destination projection at the
    /// start and interpolate from its conventional default parameters.
    pub fn interpolate(self, to: Self, t: f64) -> Self {
        let t = t.clamp(0.0, 1.0);
        let up = self.up.lerp(to.up, t).normalize_or(to.up);
        let projection = match (self.projection, to.projection) {
            (Projection::Orthographic { zoom: from }, Projection::Orthographic { zoom: to }) => {
                Projection::Orthographic {
                    zoom: from + (to - from) * t,
                }
            }
            (
                Projection::Perspective {
                    fov_y: from_fov,
                    near: from_near,
                    far: from_far,
                },
                Projection::Perspective {
                    fov_y: to_fov,
                    near: to_near,
                    far: to_far,
                },
            ) => Projection::Perspective {
                fov_y: from_fov + (to_fov - from_fov) * t,
                near: from_near + (to_near - from_near) * t,
                far: from_far + (to_far - from_far) * t,
            },
            (_, Projection::Orthographic { zoom }) => Projection::Orthographic {
                zoom: 1.0 + (zoom - 1.0) * t,
            },
            (_, Projection::Perspective { fov_y, near, far }) => Projection::Perspective {
                fov_y: std::f64::consts::FRAC_PI_4 + (fov_y - std::f64::consts::FRAC_PI_4) * t,
                near: 0.1 + (near - 0.1) * t,
                far: 1000.0 + (far - 1000.0) * t,
            },
        };
        Self {
            position: self.position.lerp(to.position, t),
            rotation: self.rotation.slerp(to.rotation, t),
            target: self.target.lerp(to.target, t),
            up,
            projection,
        }
    }
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
}

/// Camera consumed by rendering, billboards, overlays and picking.
///
/// The authored [`Camera`] remains the timeline authority. Editor-only views
/// are composed here without changing authored scene state.
#[derive(Resource, Debug, Clone, Copy, PartialEq)]
pub struct ResolvedCamera {
    /// Final pose/projection selected for rendering.
    pub camera: Camera,
    /// Host-only viewport mapping applied after the authored camera.
    pub viewport: CameraViewport,
}

/// Per-frame working camera after native constraints but before editor override.
/// This is derived from [`Camera`] and is never authored or snapshotted.
#[derive(Resource, Debug, Clone, Copy, PartialEq)]
pub struct CameraRigCamera(pub Camera);

impl ResolvedCamera {
    pub const fn new(camera: Camera, viewport: CameraViewport) -> Self {
        Self { camera, viewport }
    }
}

impl Default for ResolvedCamera {
    fn default() -> Self {
        Self::new(Camera::ortho_2d(1280, 720), CameraViewport::default())
    }
}

impl Deref for ResolvedCamera {
    type Target = Camera;

    fn deref(&self) -> &Self::Target {
        &self.camera
    }
}

impl DerefMut for ResolvedCamera {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.camera
    }
}

/// Optional presentation-only override, primarily owned by the editor.
#[derive(Resource, Debug, Clone, Copy, Default, PartialEq)]
pub struct CameraViewOverride(pub Option<Camera>);

impl Camera {
    /// Return the authored pose without logical viewport dimensions.
    pub const fn pose(&self) -> CameraPose {
        CameraPose {
            position: self.position,
            rotation: self.rotation,
            target: self.target,
            up: self.up,
            projection: self.projection,
        }
    }

    /// Apply a validated pose while preserving logical viewport dimensions.
    pub fn apply_pose(&mut self, pose: CameraPose) -> Result<(), CameraValidationError> {
        pose.validate()?;
        self.position = pose.position;
        self.rotation = pose.rotation;
        self.target = pose.target;
        self.up = pose.up;
        self.projection = pose.projection;
        Ok(())
    }

    /// Validate all authored pose, projection, and logical viewport invariants.
    pub fn validate(&self) -> Result<(), CameraValidationError> {
        if !self.position.is_finite()
            || !self.rotation.is_finite()
            || !self.target.is_finite()
            || !self.up.is_finite()
        {
            return Err(CameraValidationError::NonFinite);
        }
        if self.viewport_width == 0 || self.viewport_height == 0 {
            return Err(CameraValidationError::EmptyViewport);
        }
        match self.projection {
            Projection::Orthographic { zoom } => {
                if !zoom.is_finite() || zoom <= 0.0 {
                    return Err(CameraValidationError::InvalidZoom);
                }
            }
            Projection::Perspective { fov_y, near, far } => {
                Self::validate_perspective(fov_y, near, far)?;
                Self::validate_look_at(self.position, self.target, self.up)?;
            }
        }
        Ok(())
    }

    pub fn validate_look_at(
        eye: DVec3,
        target: DVec3,
        up: DVec3,
    ) -> Result<(), CameraValidationError> {
        if !eye.is_finite() || !target.is_finite() || !up.is_finite() {
            return Err(CameraValidationError::NonFinite);
        }
        let direction = target - eye;
        if direction.length_squared() <= f64::EPSILON {
            return Err(CameraValidationError::CoincidentEyeTarget);
        }
        if up.length_squared() <= f64::EPSILON
            || direction.cross(up).length_squared() <= f64::EPSILON
        {
            return Err(CameraValidationError::InvalidUp);
        }
        Ok(())
    }

    pub fn validate_perspective(
        fov_y: f64,
        near: f64,
        far: f64,
    ) -> Result<(), CameraValidationError> {
        if !fov_y.is_finite() || !near.is_finite() || !far.is_finite() {
            return Err(CameraValidationError::NonFinite);
        }
        if !(0.0 < fov_y && fov_y < std::f64::consts::PI) {
            return Err(CameraValidationError::InvalidFov);
        }
        if !(0.0 < near && near < far) {
            return Err(CameraValidationError::InvalidClipping);
        }
        Ok(())
    }

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
        }
    }

    /// Sets this camera to look at `target` from `eye` with the given `up`.
    pub fn look_at(
        &mut self,
        eye: DVec3,
        target: DVec3,
        up: DVec3,
    ) -> Result<(), CameraValidationError> {
        Self::validate_look_at(eye, target, up)?;
        self.position = eye;
        self.target = target;
        self.up = up;
        let view = DMat4::look_at_rh(eye, target, up);
        // view = (T*R)^-1  => T*R = view.inverse()
        let cam_to_world = view.inverse();
        let (_scale, rot, _trans) = cam_to_world.to_scale_rotation_translation();
        self.rotation = rot;
        Ok(())
    }

    /// Orbits around `target` by yaw/pitch deltas (radians).
    pub fn orbit_around_target(
        &mut self,
        delta_yaw: f64,
        delta_pitch: f64,
    ) -> Result<(), CameraValidationError> {
        if !delta_yaw.is_finite() || !delta_pitch.is_finite() {
            return Err(CameraValidationError::NonFinite);
        }
        let authored_dir = self.position - self.target;
        // The default orthographic pose has no eye distance. Give its first
        // orbit a deterministic seed direction; subsequent orbits use the
        // authored radius exactly.
        let (dir, radius) = if authored_dir.length_squared() <= f64::EPSILON {
            (DVec3::Z * 0.01, 0.01)
        } else {
            (authored_dir, authored_dir.length())
        };
        let mut yaw = f64::atan2(dir.x, dir.z);
        let mut pitch = (dir.y / radius).asin();
        yaw += delta_yaw;
        pitch += delta_pitch;
        let cp = pitch.cos();
        let sp = pitch.sin();
        let cy = yaw.cos();
        let sy = yaw.sin();
        let new_dir = DVec3::new(sy * cp, sp, cy * cp) * radius;
        self.look_at(self.target + new_dir, self.target, self.up)
    }

    /// Pan in screen space (pixels scaled to world).
    pub fn pan_screen_delta(&mut self, delta: DVec2) {
        self.pan_screen_delta_with_viewport(delta, CameraViewport::default());
    }

    /// Pan in screen space using the current host viewport fit.
    pub fn pan_screen_delta_with_viewport(&mut self, delta: DVec2, viewport: CameraViewport) {
        // Right and up vectors from rotation
        let right = self.rotation * DVec3::X;
        let up_dir = self.rotation * DVec3::Y;
        // Scale delta by distance for perspective, or 1/zoom for ortho
        let scale = match self.projection {
            Projection::Perspective { .. } => (self.position - self.target).length() * 0.002,
            Projection::Orthographic { zoom } => 1.0 / (zoom * viewport.scale).max(0.1),
        };
        let move_vec = -right * delta.x * scale + up_dir * delta.y * scale;
        self.position += move_vec;
        self.target += move_vec;
    }

    /// Dolly (move closer/further to target) by factor (<1 closer, >1 further).
    pub fn dolly(&mut self, factor: f64) -> Result<(), CameraValidationError> {
        if !factor.is_finite() || factor <= 0.0 {
            return Err(CameraValidationError::NonFinite);
        }
        let dir = self.position - self.target;
        if dir.length_squared() <= f64::EPSILON {
            return Err(CameraValidationError::CoincidentEyeTarget);
        }
        self.position = self.target + dir * factor;
        Ok(())
    }

    /// Set perspective projection parameters.
    pub fn set_perspective(
        &mut self,
        fov_y: f64,
        near: f64,
        far: f64,
    ) -> Result<(), CameraValidationError> {
        Self::validate_perspective(fov_y, near, far)?;
        self.projection = Projection::Perspective { fov_y, near, far };
        Ok(())
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
        self.to_vello_transform_with_viewport(CameraViewport::default())
    }

    /// Computes the Vello transform including presentation-only viewport fit.
    pub fn to_vello_transform_with_viewport(&self, viewport: CameraViewport) -> Affine {
        let zoom = match self.projection {
            Projection::Orthographic { zoom } => zoom,
            _ => 1.0,
        };
        let effective_zoom = zoom * viewport.scale;
        let z_angle = self.z_angle();
        let hw = (self.viewport_width as f64) / 2.0;
        let hh = (self.viewport_height as f64) / 2.0 + viewport.offset_y;

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

    /// Convert world coordinates through the authored camera and host viewport fit.
    pub fn world_to_screen_with_viewport(&self, world: DVec3, viewport: CameraViewport) -> DVec2 {
        let logical = self.world_to_screen(world);
        let center = DVec2::new(
            self.viewport_width as f64 * 0.5,
            self.viewport_height as f64 * 0.5,
        );
        center + DVec2::new(0.0, viewport.offset_y) + (logical - center) * viewport.scale
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

    /// Invert [`Camera::world_to_screen_with_viewport`] on the Z=0 plane.
    pub fn screen_to_world_with_viewport(&self, screen: DVec2, viewport: CameraViewport) -> DVec3 {
        let center = DVec2::new(
            self.viewport_width as f64 * 0.5,
            self.viewport_height as f64 * 0.5,
        );
        let logical =
            center + (screen - center - DVec2::new(0.0, viewport.offset_y)) / viewport.scale;
        self.screen_to_world(logical)
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

    /// Compute a world ray after removing the host viewport fit.
    pub fn screen_to_ray_with_viewport(
        &self,
        screen: DVec2,
        viewport: CameraViewport,
    ) -> (DVec3, DVec3) {
        let center = DVec2::new(
            self.viewport_width as f64 * 0.5,
            self.viewport_height as f64 * 0.5,
        );
        let logical =
            center + (screen - center - DVec2::new(0.0, viewport.offset_y)) / viewport.scale;
        self.screen_to_ray(logical)
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
    fn camera_pose_roundtrip_preserves_viewport() {
        let mut camera = Camera::ortho_2d(960, 540);
        let pose = CameraPose::perspective_3d(
            DVec3::new(7.0, 5.0, 6.0),
            DVec3::ZERO,
            DVec3::Y,
            0.8,
            0.1,
            500.0,
        )
        .unwrap();
        camera.apply_pose(pose).unwrap();
        assert_eq!(camera.pose(), pose);
        assert_eq!((camera.viewport_width, camera.viewport_height), (960, 540));
    }

    #[test]
    fn camera_pose_cross_projection_uses_destination_projection() {
        let from = CameraPose::orthographic_2d(DVec2::ZERO, 2.0, 0.0).unwrap();
        let to = CameraPose::perspective_3d(
            DVec3::new(0.0, 0.0, 10.0),
            DVec3::ZERO,
            DVec3::Y,
            0.9,
            0.2,
            800.0,
        )
        .unwrap();
        let middle = from.interpolate(to, 0.5);
        assert!(matches!(middle.projection, Projection::Perspective { .. }));
        assert_eq!(from.interpolate(to, 1.0), to);
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
    fn camera_scaled_viewport_roundtrip_orthographic() {
        let cam = Camera::ortho_2d(1280, 720);
        let viewport = CameraViewport {
            scale: 0.73,
            offset_y: 41.0,
        };
        let world = DVec3::new(137.0, -88.0, 0.0);
        let screen = cam.world_to_screen_with_viewport(world, viewport);
        let restored = cam.screen_to_world_with_viewport(screen, viewport);
        assert!((restored - world).length() < 1e-8);
    }

    #[test]
    fn camera_scaled_viewport_roundtrip_perspective_and_ray() {
        let mut cam = Camera::perspective_3d(1280, 720, std::f64::consts::FRAC_PI_4);
        cam.look_at(DVec3::new(4.0, 3.0, 10.0), DVec3::ZERO, DVec3::Y)
            .unwrap();
        let viewport = CameraViewport {
            scale: 0.8,
            offset_y: -26.0,
        };
        let world = DVec3::new(1.0, -0.5, 0.0);
        let screen = cam.world_to_screen_with_viewport(world, viewport);
        let restored = cam.screen_to_world_with_viewport(screen, viewport);
        assert!((restored - world).length() < 1e-7);
        let (origin, direction) = cam.screen_to_ray_with_viewport(screen, viewport);
        assert!((world - origin).cross(direction).length() < 1e-7);
    }

    #[test]
    fn camera_validation_rejects_degenerate_pose_and_projection() {
        assert_eq!(
            Camera::validate_look_at(DVec3::ZERO, DVec3::ZERO, DVec3::Y),
            Err(CameraValidationError::CoincidentEyeTarget)
        );
        assert_eq!(
            Camera::validate_look_at(DVec3::Z, DVec3::ZERO, DVec3::Z),
            Err(CameraValidationError::InvalidUp)
        );
        assert_eq!(
            Camera::validate_perspective(std::f64::consts::PI, 0.1, 100.0),
            Err(CameraValidationError::InvalidFov)
        );
        assert_eq!(
            Camera::validate_perspective(1.0, 1.0, 0.5),
            Err(CameraValidationError::InvalidClipping)
        );
    }

    #[test]
    fn camera_orbit_preserves_target_radius_and_look_rotation() {
        let target = DVec3::new(1.0, -0.5, 2.0);
        let mut cam = Camera::perspective_3d(1280, 720, 0.8);
        cam.look_at(DVec3::new(10.0, 7.0, 13.0), target, DVec3::Y)
            .unwrap();
        let radius = (cam.position - target).length();

        cam.orbit_around_target(0.8, 0.2).unwrap();

        assert!(cam.position.is_finite());
        assert!(cam.rotation.is_finite());
        assert_eq!(cam.target, target);
        assert!(((cam.position - target).length() - radius).abs() < 1e-10);
        let forward = cam.rotation * -DVec3::Z;
        let expected = (target - cam.position).normalize();
        assert!(forward.dot(expected) > 1.0 - 1e-10);
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
        cam.set_perspective(0.785, 0.1, 1000.0).unwrap();
        cam.look_at(DVec3::new(8.0, 6.0, 8.0), DVec3::ZERO, DVec3::Y)
            .unwrap();
        let viewport = CameraViewport::default();
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
            let eff = viewport.scale.max(0.01);
            let hw = cam.viewport_width as f64 * 0.5;
            let hh = cam.viewport_height as f64 * 0.5 + viewport.offset_y;
            let vello = gaanim_core::kurbo::Affine::translate((hw, hh))
                * gaanim_core::kurbo::Affine::scale_non_uniform(eff, -eff);
            let inv = vello.inverse();
            let vpos = inv * gaanim_core::kurbo::Point::new(screen.x, screen.y);
            println!("  vpos {:?}", vpos);
        }
    }
}
