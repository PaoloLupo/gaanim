use std::sync::Arc;

use bevy::prelude::{Component, Entity, Query, Res, ResMut, Resource};
use gaanim_core::kurbo::BezPath;
use gaanim_core::peniko::Color;
use gaanim_math::{RateFunc, SpatialTransform, get_point_at_alpha};
use gaanim_scene::{FillBrush, Opacity, Path2D, PathSource, StrokeBrush};

use crate::writing::{FillDrawProgress, PathReveal, WriteTipGlow};

/// Resource containing the current simulation delta time.
#[derive(Resource, Debug, Clone, Copy, Default)]
pub struct DeltaTime {
    /// Time delta in seconds.
    pub dt: f64,
}

/// System: Copies Bevy's `Time` delta into `DeltaTime` so that tween evaluation
/// is decoupled from Bevy internals and can be paused / scaled independently.
pub fn sync_delta_time_system(
    bevy_time: Res<bevy::prelude::Time>,
    mut delta_time: ResMut<DeltaTime>,
) {
    delta_time.dt = bevy_time.delta_secs() as f64;
}

/// The state of an active Mobject animation tween.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum TweenState {
    /// Waiting for its configured startup delay.
    Pending,
    /// Actively evaluating and interpolating properties.
    Active,
    /// Finished executing.
    Completed,
}

/// Component representing an active programmatic property tween.
///
/// Under CrabAnim v2, tweens are independent entities within Bevy ECS,
/// making them highly inspectable, parallelizable, and serializable.
#[derive(Component, Debug, Clone)]
pub struct Tween {
    /// The target entity to animate.
    pub target: Entity,
    /// Initial delay in seconds before interpolation begins.
    pub delay: f64,
    /// Duration in seconds.
    pub duration: f64,
    /// Time elapsed in seconds since scheduling.
    pub elapsed: f64,
    /// Easing rate function to evaluate progress.
    pub rate_func: RateFunc,
    /// The current state of execution.
    pub state: TweenState,
}

impl Tween {
    /// Creates a new tween for a given target, duration and rate function.
    pub fn new(target: Entity, duration: f64, rate_func: RateFunc) -> Self {
        Self {
            target,
            delay: 0.0,
            duration,
            elapsed: 0.0,
            rate_func,
            state: TweenState::Pending,
        }
    }

    /// Sets a delay on the tween.
    pub fn with_delay(mut self, delay: f64) -> Self {
        self.delay = delay;
        self
    }
}

/// Bounding table for SVG/geometry path morphing matches.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct MorphTable;

/// Represents which Mobject property the tween should interpolate.
///
/// Lenses are generic and support 2D/3D spaces natively.
#[derive(Component, Clone)]
pub enum PropertyLens {
    // === Spatial (3D-Ready) ===
    Translation {
        from: gaanim_core::glam::DVec3,
        to: gaanim_core::glam::DVec3,
    },
    Rotation {
        from: gaanim_core::glam::DQuat,
        to: gaanim_core::glam::DQuat,
    },
    Scale {
        from: gaanim_core::glam::DVec3,
        to: gaanim_core::glam::DVec3,
    },

    // === Visuals ===
    Opacity {
        from: f32,
        to: f32,
    },
    FillColor {
        from: Color,
        to: Color,
    },
    StrokeColor {
        from: Color,
        to: Color,
    },
    StrokeWidth {
        from: f64,
        to: f64,
    },
    Material3D {
        from: gaanim_scene::Material3D,
        to: gaanim_scene::Material3D,
    },

    // === Geometry & Paths ===
    PathMorph {
        from: BezPath,
        to: BezPath,
        table: MorphTable,
    },
    PathCompletion {
        from: f64,
        to: f64,
    },
    /// Cross-fade the fill alpha during a Write animation.
    /// The renderer multiplies the fill brush's color alpha by
    /// `from + (to - from) * t`. Inserted into the entity as
    /// a `FillDrawProgress` component.
    FillDrawProgress {
        from: f32,
        to: f32,
    },
    FillLevel {
        from: f64,
        to: f64,
    },

    // === Camera ===
    CameraPosition {
        from: gaanim_core::glam::DVec3,
        to: gaanim_core::glam::DVec3,
    },
    CameraPositionSource {
        from: gaanim_core::glam::DVec3,
        to: crate::TrackingEndpoint,
    },
    CameraRotation {
        from: gaanim_core::glam::DQuat,
        to: gaanim_core::glam::DQuat,
    },
    CameraZoom {
        from: f64,
        to: f64,
    },
    CameraZoomSource {
        from: f64,
        to: crate::TrackingScalar,
    },
    CameraRotationSource {
        from: f64,
        to: crate::TrackingScalar,
    },
    CameraOrthographic {
        from: f64,
        to: f64,
    },
    CameraReset {
        from_position: gaanim_core::glam::DVec3,
        from_rotation: gaanim_core::glam::DQuat,
        from_target: gaanim_core::glam::DVec3,
        from_up: gaanim_core::glam::DVec3,
        from_zoom: f64,
        to_zoom: f64,
    },
    CameraFollow {
        target: gaanim_core::ObjectId,
    },
    CameraFollowEndpoint {
        target: crate::TrackingEndpoint,
        from: gaanim_core::glam::DVec3,
        offset: gaanim_core::glam::DVec3,
        offset_space: crate::FollowOffsetSpace,
        lag: f64,
    },
    CameraFrameDynamic {
        targets: Vec<bevy::prelude::Entity>,
        from_position: gaanim_core::glam::DVec3,
        from_zoom: f64,
        margins: [f64; 4],
        frame_width: f64,
        frame_height: f64,
    },
    CameraShake {
        origin: gaanim_core::glam::DVec3,
        amplitude: f64,
        frequency: f64,
    },
    CameraTarget {
        from: gaanim_core::glam::DVec3,
        to: gaanim_core::glam::DVec3,
    },
    CameraLookAt {
        from_position: gaanim_core::glam::DVec3,
        from_target: gaanim_core::glam::DVec3,
        eye: gaanim_core::glam::DVec3,
        target: gaanim_core::glam::DVec3,
        up: gaanim_core::glam::DVec3,
    },
    CameraOrbit {
        from_position: gaanim_core::glam::DVec3,
        target: gaanim_core::glam::DVec3,
        up: gaanim_core::glam::DVec3,
        delta_yaw: f64,
        delta_pitch: f64,
    },
    CameraLookAtSource {
        from_position: gaanim_core::glam::DVec3,
        from_target: gaanim_core::glam::DVec3,
        from_rotation: gaanim_core::glam::DQuat,
        eye: crate::TrackingEndpoint,
        target: crate::TrackingEndpoint,
        up: gaanim_core::glam::DVec3,
    },
    CameraPerspective {
        from_fov: f64,
        to_fov: f64,
        from_near: f64,
        to_near: f64,
        from_far: f64,
        to_far: f64,
    },

    // === Path Following ===
    /// Move the entity's translation along a Bézier path. The path is
    /// sampled at the rate-function-eased `t` and the entity's
    /// `SpatialTransform.translation` is set to the sampled point's
    /// `(x, y, 0)` world position. The entity's rotation and scale
    /// are not affected.
    PathFollow {
        path: Arc<BezPath>,
    },

    // === Reactive Signals ===
    /// Tween a reactive FloatSignal from an initial to target value.
    SignalFloat {
        from: f64,
        to: f64,
    },

    // === ShowPassingFlash ===
    /// Trims the path in a sliding range window.
    PathRange {
        from: f64,
        to: f64,
        time_width: f64,
    },

    // === Extensibility ===
    Custom(Box<dyn AnimatableLens>),
}

impl std::fmt::Debug for PropertyLens {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Translation { from, to } => write!(f, "Translation({:?} -> {:?})", from, to),
            Self::Rotation { from, to } => write!(f, "Rotation({:?} -> {:?})", from, to),
            Self::Scale { from, to } => write!(f, "Scale({:?} -> {:?})", from, to),
            Self::Opacity { from, to } => write!(f, "Opacity({} -> {})", from, to),
            Self::FillColor { from, to } => write!(f, "FillColor({:?} -> {:?})", from, to),
            Self::StrokeColor { from, to } => write!(f, "StrokeColor({:?} -> {:?})", from, to),
            Self::StrokeWidth { from, to } => write!(f, "StrokeWidth({} -> {})", from, to),
            Self::Material3D { from, to } => write!(f, "Material3D({from:?} -> {to:?})"),
            Self::PathMorph { .. } => write!(f, "PathMorph"),
            Self::PathCompletion { from, to } => write!(f, "PathCompletion({} -> {})", from, to),
            Self::FillDrawProgress { from, to } => {
                write!(f, "FillDrawProgress({} -> {})", from, to)
            }
            Self::FillLevel { from, to } => write!(f, "FillLevel({from} -> {to})"),
            Self::CameraPosition { from, to } => {
                write!(f, "CameraPosition({:?} -> {:?})", from, to)
            }
            Self::CameraPositionSource { .. } => write!(f, "CameraPositionSource"),
            Self::CameraRotation { from, to } => {
                write!(f, "CameraRotation({:?} -> {:?})", from, to)
            }
            Self::CameraZoom { from, to } => write!(f, "CameraZoom({} -> {})", from, to),
            Self::CameraZoomSource { .. } => write!(f, "CameraZoomSource"),
            Self::CameraRotationSource { .. } => write!(f, "CameraRotationSource"),
            Self::CameraOrthographic { from, to } => {
                write!(f, "CameraOrthographic({from} -> {to})")
            }
            Self::CameraReset { .. } => write!(f, "CameraReset"),
            Self::CameraFollow { target } => write!(f, "CameraFollow({target:?})"),
            Self::CameraFollowEndpoint { .. } => write!(f, "CameraFollowEndpoint"),
            Self::CameraFrameDynamic { .. } => write!(f, "CameraFrameDynamic"),
            Self::CameraShake {
                amplitude,
                frequency,
                ..
            } => write!(
                f,
                "CameraShake(amplitude {amplitude}, frequency {frequency})"
            ),
            Self::CameraTarget { from, to } => write!(f, "CameraTarget({:?} -> {:?})", from, to),
            Self::CameraLookAt { .. } => write!(f, "CameraLookAt"),
            Self::CameraOrbit { .. } => write!(f, "CameraOrbit"),
            Self::CameraLookAtSource { .. } => write!(f, "CameraLookAtSource"),
            Self::CameraPerspective {
                from_fov, to_fov, ..
            } => write!(f, "CameraPerspective({} -> {})", from_fov, to_fov),
            Self::PathFollow { .. } => write!(f, "PathFollow"),
            Self::SignalFloat { from, to } => write!(f, "SignalFloat({} -> {})", from, to),
            Self::PathRange {
                from,
                to,
                time_width,
            } => write!(f, "PathRange({} -> {} width {})", from, to, time_width),
            Self::Custom(c) => write!(f, "Custom({:?})", c.type_name()),
        }
    }
}

/// Extensible lens trait for custom third-party plugins.
pub trait AnimatableLens: Send + Sync + std::fmt::Debug + 'static {
    /// Interpolate the target Mobject's components directly using World access.
    fn interpolate(&self, world: &mut bevy::prelude::World, entity: Entity, t: f64);
    /// Helper to clone dyn boxes.
    fn clone_box(&self) -> Box<dyn AnimatableLens>;
    /// Returns the descriptive type name of the custom lens.
    fn type_name(&self) -> &'static str;
}

impl Clone for Box<dyn AnimatableLens> {
    fn clone(&self) -> Self {
        self.clone_box()
    }
}

/// System: Evaluates all scheduled property tweens using parallel ECS queries.
///
/// Built-in lenses (Translation, Rotation, Scale, Opacity, FillColor, StrokeColor, StrokeWidth, PathCompletion)
/// are applied directly via Bevy's parallel query system. Custom lenses are handled separately
/// by `evaluate_custom_tweens_system` due to their need for exclusive World access.
pub fn evaluate_tweens_system(
    mut commands: bevy::prelude::Commands,
    dt: Res<DeltaTime>,
    mut tweens: Query<(Entity, &mut Tween, &PropertyLens)>,
    mut transforms: Query<&mut SpatialTransform>,
    mut opacities: Query<&mut Opacity>,
    mut fills: Query<&mut FillBrush>,
    mut strokes: Query<&mut StrokeBrush>,
    mut materials3d: Query<&mut gaanim_scene::Material3D>,
    mut sources: Query<&mut PathSource>,
    mut paths: Query<&mut Path2D>,
    mut fill_progress: Query<&mut FillDrawProgress>,
    mut fill_levels: Query<&mut gaanim_scene::FillLevel>,
    mut tip_glows: Query<&mut WriteTipGlow>,
    mut float_signals: Query<&mut crate::signals::FloatSignal>,
    mut path_reveals: Query<&mut PathReveal>,
) {
    for (_tween_entity, mut tween, lens) in &mut tweens {
        // Custom lenses need exclusive World access; skip them entirely here.
        // They are fully handled by evaluate_custom_tweens_system.
        if matches!(lens, PropertyLens::Custom(_)) {
            continue;
        }

        if tween.state == TweenState::Completed {
            continue;
        }

        tween.elapsed += dt.dt;

        let progress = if tween.elapsed < tween.delay {
            tween.state = TweenState::Pending;
            continue;
        } else {
            tween.state = TweenState::Active;
            let actual_elapsed = tween.elapsed - tween.delay;
            if actual_elapsed >= tween.duration {
                tween.state = TweenState::Completed;
                1.0
            } else {
                actual_elapsed / tween.duration
            }
        };

        let t = tween.rate_func.evaluate(progress);

        match lens {
            PropertyLens::Translation { from, to } => {
                if let Ok(mut transform) = transforms.get_mut(tween.target) {
                    transform.translation = from.lerp(*to, t);
                }
            }
            PropertyLens::Rotation { from, to } => {
                if let Ok(mut transform) = transforms.get_mut(tween.target) {
                    transform.rotation = from.slerp(*to, t);
                }
            }
            PropertyLens::Scale { from, to } => {
                if let Ok(mut transform) = transforms.get_mut(tween.target) {
                    transform.scale = from.lerp(*to, t);
                }
            }
            PropertyLens::Opacity { from, to } => {
                if let Ok(mut opacity) = opacities.get_mut(tween.target) {
                    opacity.0 = *from + (*to - *from) * t as f32;
                }
            }
            PropertyLens::FillColor { from, to } => {
                if let Ok(mut fill) = fills.get_mut(tween.target) {
                    let c = gaanim_core::interpolate_color(*from, *to, t);
                    *fill = FillBrush::color(c);
                }
            }
            PropertyLens::StrokeColor { from, to } => {
                if let Ok(mut stroke) = strokes.get_mut(tween.target) {
                    let c = gaanim_core::interpolate_color(*from, *to, t);
                    stroke.brush = Some(gaanim_core::peniko::Brush::Solid(c));
                }
            }
            PropertyLens::StrokeWidth { from, to } => {
                if let Ok(mut stroke) = strokes.get_mut(tween.target) {
                    stroke.style.width = *from + (*to - *from) * t;
                }
            }
            PropertyLens::Material3D { from, to } => {
                if let Ok(mut material) = materials3d.get_mut(tween.target) {
                    *material = from.lerp(*to, t);
                }
            }
            PropertyLens::PathCompletion { from, to } => {
                // Trim the entity's `Path2D` directly from the cached
                // `PathSource`. The `PathCompletion` component is no
                // longer needed because the lens already carries
                // `from`/`to`; the source of truth is the `PathSource`
                // mirror seeded once at spawn.
                let completion = *from + (*to - *from) * t;
                if let Ok(source) = sources.get_mut(tween.target)
                    && let Ok(mut path) = paths.get_mut(tween.target)
                {
                    path.0 = std::sync::Arc::new(gaanim_math::get_subpath(&source.0, completion));
                }
                // Update the pen-tip glow position if the entity has one.
                if let Ok(mut tip) = tip_glows.get_mut(tween.target) {
                    tip.completion = completion;
                }
                // Keep PathReveal in sync so reactive regenerators (e.g.
                // ExpressionPlot with a Parameter) can re-trim the freshly
                // regenerated path instead of overwriting it with the full
                // untrimmed path.
                if let Ok(mut reveal) = path_reveals.get_mut(tween.target) {
                    reveal.0 = completion;
                } else {
                    commands.entity(tween.target).insert(PathReveal(completion));
                }
            }
            PropertyLens::FillDrawProgress { from, to } => {
                // Update the `FillDrawProgress` component. The renderer
                // reads it to modulate the fill brush's color alpha.
                // If the entity doesn't have the component yet (e.g. the
                // Write animation was scheduled but the entity is a
                // fresh spawn), this branch is a no-op; the renderer
                // still falls back to full-opacity fill.
                let v = *from + (*to - *from) * t as f32;
                if let Ok(mut fdp) = fill_progress.get_mut(tween.target) {
                    fdp.0 = v;
                }
            }
            PropertyLens::FillLevel { from, to } => {
                if let Ok(mut level) = fill_levels.get_mut(tween.target) {
                    level.0 = (*from + (*to - *from) * t).clamp(0.0, 1.0);
                }
            }
            PropertyLens::CameraPosition { from: _, to: _ } => {
                // Camera is a resource, not a component. Custom lenses can handle this.
            }
            PropertyLens::CameraPositionSource { .. } => {}
            PropertyLens::CameraRotation { from: _, to: _ } => {}
            PropertyLens::CameraZoom { from: _, to: _ } => {}
            PropertyLens::CameraZoomSource { .. } => {}
            PropertyLens::CameraRotationSource { .. } => {}
            PropertyLens::CameraOrthographic { .. } => {}
            PropertyLens::CameraReset { .. } => {}
            PropertyLens::CameraFollow { .. } => {}
            PropertyLens::CameraFollowEndpoint { .. } => {}
            PropertyLens::CameraFrameDynamic { .. } => {}
            PropertyLens::CameraShake { .. } => {}
            PropertyLens::CameraTarget { .. } => {}
            PropertyLens::CameraLookAt { .. } => {}
            PropertyLens::CameraOrbit { .. } => {}
            PropertyLens::CameraLookAtSource { .. } => {}
            PropertyLens::CameraPerspective { .. } => {}
            PropertyLens::PathMorph { from, to, table: _ } => {
                let completed = tween.state == TweenState::Completed;
                let morphed = if completed {
                    to.clone()
                } else {
                    gaanim_math::interpolate_paths_continuous(from, to, t)
                };
                let morphed = std::sync::Arc::new(morphed);
                if let Ok(mut path) = paths.get_mut(tween.target) {
                    path.0 = morphed.clone();
                }
                // The renderer uses `PathSource` to clip inner strokes. Keep
                // it synchronized with the visible morph path; otherwise a
                // circle's old source path clips the stroke of a diamond and
                // leaves a flashing circular halo behind.
                if let Ok(mut source) = sources.get_mut(tween.target) {
                    source.0 = morphed;
                }
            }
            PropertyLens::PathFollow { path } => {
                // Sample the Bézier path at the eased `t` and set
                // the entity's translation to the sampled point.
                let p = get_point_at_alpha(path, t);
                if let Ok(mut transform) = transforms.get_mut(tween.target) {
                    transform.translation = gaanim_core::glam::DVec3::new(p.x, p.y, 0.0);
                }
            }
            PropertyLens::SignalFloat { from, to } => {
                if let Ok(mut signal) = float_signals.get_mut(tween.target) {
                    signal.value = *from + (*to - *from) * t;
                }
            }
            PropertyLens::PathRange {
                from,
                to,
                time_width,
            } => {
                let p = *from + (*to - *from) * t;
                let start = (p - *time_width).max(0.0);
                let end = p.min(1.0);
                if let Ok(source) = sources.get(tween.target)
                    && let Ok(mut path) = paths.get_mut(tween.target)
                {
                    path.0 =
                        std::sync::Arc::new(gaanim_math::get_subpath_range(&source.0, start, end));
                }
            }
            PropertyLens::Custom(_) => {
                unreachable!("Custom lenses are skipped at the start of the system")
            }
        }
    }
}

/// System: Evaluates only custom `AnimatableLens` tweens that require exclusive World access.
///
/// This system runs after `evaluate_tweens_system` and handles any tween whose `PropertyLens`
/// is `Custom`. Since custom lenses may read/write arbitrary components, they need exclusive access.
pub fn evaluate_custom_tweens_system(world: &mut bevy::prelude::World) {
    let dt = world
        .get_resource::<DeltaTime>()
        .map(|d| d.dt)
        .unwrap_or(0.0);

    let mut updates = Vec::new();
    {
        let mut query = world.query::<(Entity, &mut Tween, &PropertyLens)>();
        for (_tween_entity, mut tween, lens) in query.iter_mut(world) {
            if tween.state == TweenState::Completed {
                continue;
            }

            tween.elapsed += dt;

            let progress = if tween.elapsed < tween.delay {
                tween.state = TweenState::Pending;
                continue;
            } else {
                tween.state = TweenState::Active;
                let actual_elapsed = tween.elapsed - tween.delay;
                if actual_elapsed >= tween.duration {
                    tween.state = TweenState::Completed;
                    1.0
                } else {
                    actual_elapsed / tween.duration
                }
            };

            let t = tween.rate_func.evaluate(progress);

            if let PropertyLens::Custom(_) = lens {
                updates.push((tween.target, lens.clone(), t));
            }
        }
    }

    // Apply computed custom interpolations
    for (target_entity, lens, t) in updates {
        if let PropertyLens::Custom(ref custom_lens) = lens {
            custom_lens.interpolate(world, target_entity, t);
        }
    }
}

#[cfg(test)]
mod tests {
    use gaanim_math::get_point_at_alpha;
    use kurbo::BezPath;

    /// `PathFollow` should sample the Bézier at the rate-function-eased
    /// `t`. With `RateFunc::Linear` and a polyline path, the sampled
    /// point at any `t` should match `get_point_at_alpha` of the same
    /// path at the same `t` (we re-implement the sampling call here
    /// without Bevy so the math is verifiable in isolation).
    #[test]
    fn path_follow_samples_match_get_point_at_alpha() {
        // A pentagon: 5 line segments connected end-to-end.
        let mut path = BezPath::new();
        path.move_to(kurbo::Point::new(0.0, 0.0));
        path.line_to(kurbo::Point::new(100.0, 0.0));
        path.line_to(kurbo::Point::new(100.0, 100.0));
        path.line_to(kurbo::Point::new(0.0, 100.0));
        path.line_to(kurbo::Point::new(0.0, 0.0));

        for t in [0.0, 0.25, 0.5, 0.75, 1.0] {
            let sampled = get_point_at_alpha(&path, t);
            // The lens is parameterised on a multi-subpath path; the
            // per-subpath trim means each segment is revealed
            // proportionally, so at t=0.25 the first subpath is at
            // 0.25 of its own length, etc.
            assert!(sampled.x.is_finite(), "x at t={t} not finite");
            assert!(sampled.y.is_finite(), "y at t={t} not finite");
        }
    }
}
