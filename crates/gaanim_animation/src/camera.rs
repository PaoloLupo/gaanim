//! Reactive camera bindings evaluated after scene updaters and layout.

use bevy::prelude::{Component, World};
use gaanim_core::glam::{DQuat, DVec3};

use crate::updaters::{TrackingEndpoint, TrackingScalar, resolve_tracking_endpoint};

/// Inclusive-start/exclusive-end activation window for a persistent binding.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CameraBindingWindow {
    pub start: f64,
    pub end: Option<f64>,
}

impl CameraBindingWindow {
    pub fn contains(self, time: f64) -> bool {
        self.start <= time && self.end.is_none_or(|end| time < end)
    }
}

/// Typed channels controlled by one persistent camera binding.
#[derive(Debug, Clone)]
pub enum CameraBindingKind {
    TwoD {
        center: Option<TrackingEndpoint>,
        zoom: Option<TrackingScalar>,
        rotation: Option<TrackingScalar>,
    },
    ThreeD {
        eye: Option<TrackingEndpoint>,
        target: Option<TrackingEndpoint>,
        fov_y: Option<TrackingScalar>,
        up: DVec3,
    },
}

/// Non-rendered ECS component describing a deterministic camera constraint.
#[derive(Component, Debug, Clone)]
pub struct CameraBinding {
    pub order: u64,
    pub kind: CameraBindingKind,
    pub influence: TrackingScalar,
    pub windows: Vec<CameraBindingWindow>,
}

fn look_at_rotation(eye: DVec3, target: DVec3, up: DVec3) -> Option<DQuat> {
    let direction = target - eye;
    if direction.length_squared() <= 1e-18
        || up.length_squared() <= 1e-18
        || direction.normalize().cross(up.normalize()).length_squared() <= 1e-18
    {
        return None;
    }
    let view = gaanim_core::glam::dcamera::rh::view::look_at_mat4(eye, target, up);
    Some(view.inverse().to_scale_rotation_translation().1)
}

/// Apply active persistent bindings in stable creation order.
///
/// Each binding only touches its declared channels. Invalid runtime values
/// are rejected for that evaluation rather than silently clamped.
pub fn apply_camera_bindings(world: &mut World, time: f64) {
    let mut bindings = {
        let mut query = world.query::<&CameraBinding>();
        query
            .iter(world)
            .filter(|binding| binding.windows.iter().any(|window| window.contains(time)))
            .cloned()
            .collect::<Vec<_>>()
    };
    bindings.sort_by_key(|binding| binding.order);

    for binding in bindings {
        let Some(influence) = binding
            .influence
            .evaluate(world)
            .filter(|value| value.is_finite() && (0.0..=1.0).contains(value))
        else {
            continue;
        };
        if influence <= 0.0 {
            continue;
        }

        match binding.kind {
            CameraBindingKind::TwoD {
                center,
                zoom,
                rotation,
            } => {
                let center = center
                    .as_ref()
                    .and_then(|endpoint| resolve_tracking_endpoint(endpoint, world));
                let zoom = zoom.as_ref().and_then(|value| value.evaluate(world));
                let angle = rotation.as_ref().and_then(|value| value.evaluate(world));
                let Some(mut camera) = world.get_resource_mut::<gaanim_math::Camera>() else {
                    continue;
                };
                if let Some(center) = center.filter(|center| center.is_finite()) {
                    camera.position.x += (center.x - camera.position.x) * influence;
                    camera.position.y += (center.y - camera.position.y) * influence;
                }
                let current_zoom = match camera.projection {
                    gaanim_math::Projection::Orthographic { zoom } => zoom,
                    gaanim_math::Projection::Perspective { .. } => 1.0,
                };
                let target_zoom = zoom.filter(|value| value.is_finite() && *value > 0.0);
                camera.projection = gaanim_math::Projection::Orthographic {
                    zoom: target_zoom
                        .map(|value| current_zoom + (value - current_zoom) * influence)
                        .unwrap_or(current_zoom),
                };
                if let Some(angle) = angle.filter(|value| value.is_finite()) {
                    camera.rotation = camera
                        .rotation
                        .slerp(DQuat::from_rotation_z(angle), influence);
                }
            }
            CameraBindingKind::ThreeD {
                eye,
                target,
                fov_y,
                up,
            } => {
                let eye = eye
                    .as_ref()
                    .and_then(|endpoint| resolve_tracking_endpoint(endpoint, world));
                let target = target
                    .as_ref()
                    .and_then(|endpoint| resolve_tracking_endpoint(endpoint, world));
                let fov_y = fov_y.as_ref().and_then(|value| value.evaluate(world));
                let Some(mut camera) = world.get_resource_mut::<gaanim_math::Camera>() else {
                    continue;
                };
                let fallback_eye =
                    if (camera.position - camera.target).length_squared() <= f64::EPSILON {
                        camera.target + DVec3::Z * 10.0
                    } else {
                        camera.position
                    };
                let resolved_eye = eye.unwrap_or(fallback_eye);
                let resolved_target = target.unwrap_or(camera.target);
                if let Some(rotation) = look_at_rotation(resolved_eye, resolved_target, up) {
                    camera.position = camera.position.lerp(resolved_eye, influence);
                    camera.target = camera.target.lerp(resolved_target, influence);
                    camera.rotation = camera.rotation.slerp(rotation, influence);
                    camera.up = camera.up.lerp(up, influence).normalize_or_zero();
                }
                let (current_fov, near, far) = match camera.projection {
                    gaanim_math::Projection::Perspective { fov_y, near, far } => (fov_y, near, far),
                    gaanim_math::Projection::Orthographic { .. } => {
                        (std::f64::consts::FRAC_PI_4, 0.1, 1000.0)
                    }
                };
                let target_fov = fov_y
                    .filter(|value| {
                        value.is_finite() && *value > 0.0 && *value < std::f64::consts::PI
                    })
                    .unwrap_or(current_fov);
                camera.projection = gaanim_math::Projection::Perspective {
                    fov_y: current_fov + (target_fov - current_fov) * influence,
                    near,
                    far,
                };
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn scalar(value: f64) -> TrackingScalar {
        TrackingScalar {
            source: crate::reactive::ScalarSource::constant(value),
            parameters: Vec::new(),
        }
    }

    fn active() -> Vec<CameraBindingWindow> {
        vec![CameraBindingWindow {
            start: 0.0,
            end: None,
        }]
    }

    #[test]
    fn bindings_compose_by_channel_creation_order_and_influence() {
        let mut world = World::new();
        world.insert_resource(gaanim_math::Camera::ortho_2d(1280, 720));
        world.spawn(CameraBinding {
            order: 0,
            kind: CameraBindingKind::TwoD {
                center: Some(TrackingEndpoint::Static(DVec3::new(10.0, 0.0, 0.0))),
                zoom: None,
                rotation: None,
            },
            influence: scalar(1.0),
            windows: active(),
        });
        world.spawn(CameraBinding {
            order: 1,
            kind: CameraBindingKind::TwoD {
                center: Some(TrackingEndpoint::Static(DVec3::new(20.0, 0.0, 0.0))),
                zoom: Some(scalar(2.0)),
                rotation: None,
            },
            influence: scalar(0.5),
            windows: active(),
        });

        apply_camera_bindings(&mut world, 1.0);
        let camera = world.resource::<gaanim_math::Camera>();
        assert_eq!(camera.position.x, 15.0);
        assert_eq!(
            camera.projection,
            gaanim_math::Projection::Orthographic { zoom: 1.5 }
        );
    }

    #[test]
    fn invalid_influence_is_rejected_instead_of_clamped() {
        let mut world = World::new();
        world.insert_resource(gaanim_math::Camera::ortho_2d(1280, 720));
        world.spawn(CameraBinding {
            order: 0,
            kind: CameraBindingKind::TwoD {
                center: Some(TrackingEndpoint::Static(DVec3::new(10.0, 0.0, 0.0))),
                zoom: None,
                rotation: None,
            },
            influence: scalar(2.0),
            windows: active(),
        });
        apply_camera_bindings(&mut world, 1.0);
        assert_eq!(
            world.resource::<gaanim_math::Camera>().position,
            DVec3::ZERO
        );
    }

    #[test]
    fn fov_only_3d_binding_uses_a_valid_default_eye() {
        let mut world = World::new();
        world.insert_resource(gaanim_math::Camera::ortho_2d(1280, 720));
        world.spawn(CameraBinding {
            order: 0,
            kind: CameraBindingKind::ThreeD {
                eye: None,
                target: None,
                fov_y: Some(scalar(1.0)),
                up: DVec3::Y,
            },
            influence: scalar(1.0),
            windows: active(),
        });
        apply_camera_bindings(&mut world, 0.0);
        let camera = world.resource::<gaanim_math::Camera>();
        assert!(camera.validate().is_ok());
        assert_eq!(camera.position, DVec3::Z * 10.0);
    }
}
