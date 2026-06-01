use bevy::prelude::{Component, Entity, Resource, World};
use gaanim_core::peniko::Color;
use gaanim_core::kurbo::BezPath;
use gaanim_math::{RateFunc, SpatialTransform};
use gaanim_scene::{FillBrush, Opacity, StrokeBrush};

/// Resource containing the current simulation delta time.
#[derive(Resource, Debug, Clone, Copy, Default)]
pub struct DeltaTime {
    /// Time delta in seconds.
    pub dt: f64,
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
    Translation { from: gaanim_core::glam::DVec3, to: gaanim_core::glam::DVec3 },
    Rotation { from: gaanim_core::glam::DQuat, to: gaanim_core::glam::DQuat },
    Scale { from: gaanim_core::glam::DVec3, to: gaanim_core::glam::DVec3 },

    // === Visuals ===
    Opacity { from: f32, to: f32 },
    FillColor { from: Color, to: Color },
    StrokeColor { from: Color, to: Color },
    StrokeWidth { from: f64, to: f64 },

    // === Geometry & Paths ===
    PathMorph { from: BezPath, to: BezPath, table: MorphTable },
    PathCompletion { from: f64, to: f64 },

    // === Camera ===
    CameraPosition { from: gaanim_core::glam::DVec3, to: gaanim_core::glam::DVec3 },
    CameraRotation { from: gaanim_core::glam::DQuat, to: gaanim_core::glam::DQuat },
    CameraZoom { from: f64, to: f64 },

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
            Self::PathMorph { .. } => write!(f, "PathMorph"),
            Self::PathCompletion { from, to } => write!(f, "PathCompletion({} -> {})", from, to),
            Self::CameraPosition { from, to } => write!(f, "CameraPosition({:?} -> {:?})", from, to),
            Self::CameraRotation { from, to } => write!(f, "CameraRotation({:?} -> {:?})", from, to),
            Self::CameraZoom { from, to } => write!(f, "CameraZoom({} -> {})", from, to),
            Self::Custom(c) => write!(f, "Custom({:?})", c.type_name()),
        }
    }
}

/// Extensible lens trait for custom third-party plugins.
pub trait AnimatableLens: Send + Sync + std::fmt::Debug + 'static {
    /// Interpolate the target Mobject's components directly using World access.
    fn interpolate(&self, world: &mut World, entity: Entity, t: f64);
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

/// Component representing the path drawing progress (0.0 to 1.0).
///
/// Highly useful for trace, writing, and path creation animations.
#[derive(Component, Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct PathCompletion(pub f64);

/// System: Evaluates all scheduled property tweens in the world.
///
/// Runs as an exclusive system with World access, supporting FFI/Python-safe
/// dynamic `AnimatableLens` plugins.
pub fn evaluate_tweens_system(world: &mut World) {
    let dt = if let Some(dt_res) = world.get_resource::<DeltaTime>() {
        dt_res.dt
    } else {
        0.0
    };

    let mut updates = Vec::new();
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
        updates.push((tween.target, lens.clone(), t));
    }

    // Apply computed property interpolations to targeted entities
    for (target_entity, lens, t) in updates {
        apply_lens_update(world, target_entity, &lens, t);
    }
}

fn apply_lens_update(world: &mut World, target: Entity, lens: &PropertyLens, t: f64) {
    match lens {
        PropertyLens::Translation { from, to } => {
            if let Some(mut transform) = world.get_mut::<SpatialTransform>(target) {
                transform.translation = from.lerp(*to, t);
            }
        }
        PropertyLens::Rotation { from, to } => {
            if let Some(mut transform) = world.get_mut::<SpatialTransform>(target) {
                transform.rotation = from.slerp(*to, t);
            }
        }
        PropertyLens::Scale { from, to } => {
            if let Some(mut transform) = world.get_mut::<SpatialTransform>(target) {
                transform.scale = from.lerp(*to, t);
            }
        }
        PropertyLens::Opacity { from, to } => {
            if let Some(mut opacity) = world.get_mut::<Opacity>(target) {
                opacity.0 = *from + (*to - *from) * t as f32;
            }
        }
        PropertyLens::FillColor { from, to } => {
            if let Some(mut fill) = world.get_mut::<FillBrush>(target) {
                let c = gaanim_core::interpolate_color(*from, *to, t);
                *fill = FillBrush::color(c);
            }
        }
        PropertyLens::StrokeColor { from, to } => {
            if let Some(mut stroke) = world.get_mut::<StrokeBrush>(target) {
                let c = gaanim_core::interpolate_color(*from, *to, t);
                stroke.brush = Some(gaanim_core::peniko::Brush::Solid(c));
            }
        }
        PropertyLens::StrokeWidth { from, to } => {
            if let Some(mut stroke) = world.get_mut::<StrokeBrush>(target) {
                stroke.style.width = *from + (*to - *from) * t;
            }
        }
        PropertyLens::PathCompletion { from, to } => {
            if let Some(mut completion) = world.get_mut::<PathCompletion>(target) {
                completion.0 = *from + (*to - *from) * t;
            }
        }
        PropertyLens::Custom(custom_lens) => {
            custom_lens.interpolate(world, target, t);
        }
        _ => {}
    }
}


