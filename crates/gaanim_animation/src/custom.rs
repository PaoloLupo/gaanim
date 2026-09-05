//! Pure property callbacks evaluated at the exact requested animation progress.

use std::sync::Arc;

use bevy::prelude::Resource;
use gaanim_core::{
    ObjectId,
    glam::{DQuat, DVec3},
    peniko::Brush,
};
use gaanim_math::SpatialTransform;
use gaanim_scene::{
    FillBrush, Opacity, StrokeBrush,
    prelude::{Entity, World},
};

use crate::AnimatableLens;

/// The complete set of properties a custom animation is allowed to write.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum CustomChannel {
    Position,
    Rotation,
    Scale,
    Opacity,
    Fill,
    Stroke,
    StrokeWidth,
}

impl CustomChannel {
    pub fn name(self) -> &'static str {
        match self {
            Self::Position => "position",
            Self::Rotation => "rotation",
            Self::Scale => "scale",
            Self::Opacity => "opacity",
            Self::Fill => "fill",
            Self::Stroke => "stroke",
            Self::StrokeWidth => "stroke_width",
        }
    }

    /// Existing timeline conflict vocabulary, shared with native animations.
    pub fn timeline_channel(self) -> &'static str {
        match self {
            Self::Position => "translation",
            Self::Stroke => "stroke_color",
            _ => self.name(),
        }
    }

    pub fn is_paint(self) -> bool {
        matches!(self, Self::Fill | Self::Stroke | Self::StrokeWidth)
    }
}

/// Absolute local values returned by a pure callback. Exactly the declared
/// channels must be populated; validation completes before any value is applied.
#[derive(Debug, Clone, Default)]
pub struct CustomValues {
    pub position: Option<DVec3>,
    /// Absolute rotation about the local Z axis, in radians.
    pub rotation: Option<f64>,
    pub scale: Option<DVec3>,
    pub opacity: Option<f32>,
    pub fill: Option<Brush>,
    pub stroke: Option<Brush>,
    pub stroke_width: Option<f64>,
}

impl CustomValues {
    fn present(&self, channel: CustomChannel) -> bool {
        match channel {
            CustomChannel::Position => self.position.is_some(),
            CustomChannel::Rotation => self.rotation.is_some(),
            CustomChannel::Scale => self.scale.is_some(),
            CustomChannel::Opacity => self.opacity.is_some(),
            CustomChannel::Fill => self.fill.is_some(),
            CustomChannel::Stroke => self.stroke.is_some(),
            CustomChannel::StrokeWidth => self.stroke_width.is_some(),
        }
    }

    fn validate(&self, channels: &[CustomChannel]) -> Result<(), String> {
        for channel in [
            CustomChannel::Position,
            CustomChannel::Rotation,
            CustomChannel::Scale,
            CustomChannel::Opacity,
            CustomChannel::Fill,
            CustomChannel::Stroke,
            CustomChannel::StrokeWidth,
        ] {
            if self.present(channel) != channels.contains(&channel) {
                return Err(format!(
                    "callback output must contain exactly its declared channels; mismatch for '{}'",
                    channel.name()
                ));
            }
        }
        if self.position.is_some_and(|value| !value.is_finite())
            || self.rotation.is_some_and(|value| !value.is_finite())
            || self.scale.is_some_and(|value| !value.is_finite())
            || self
                .opacity
                .is_some_and(|value| !value.is_finite() || !(0.0..=1.0).contains(&value))
            || self
                .stroke_width
                .is_some_and(|value| !value.is_finite() || value < 0.0)
        {
            return Err("custom values must be finite, opacity within [0, 1], and stroke_width non-negative".into());
        }
        for paint in self.fill.iter().chain(self.stroke.iter()) {
            crate::paint::validate_paint(paint).map_err(str::to_owned)?;
        }
        Ok(())
    }
}

type Callback = Arc<dyn Fn(f64) -> Result<CustomValues, String> + Send + Sync + 'static>;

/// Runtime-exact extension point, with explicit ownership of property channels.
#[derive(Clone)]
pub struct CustomAnimation {
    channels: Arc<[CustomChannel]>,
    callback: Callback,
}

impl std::fmt::Debug for CustomAnimation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CustomAnimation")
            .field("channels", &self.channels)
            .finish_non_exhaustive()
    }
}

impl CustomAnimation {
    pub fn new(
        channels: Vec<CustomChannel>,
        callback: impl Fn(f64) -> Result<CustomValues, String> + Send + Sync + 'static,
    ) -> Result<Self, String> {
        if channels.is_empty() {
            return Err("custom animation requires at least one channel".into());
        }
        for (index, channel) in channels.iter().enumerate() {
            if channels[..index].contains(channel) {
                return Err(format!("duplicate custom channel '{}'", channel.name()));
            }
        }
        Ok(Self {
            channels: channels.into(),
            callback: Arc::new(callback),
        })
    }

    pub fn channels(&self) -> &[CustomChannel] {
        &self.channels
    }

    pub fn evaluate(&self, alpha: f64) -> Result<CustomValues, String> {
        if !alpha.is_finite() {
            return Err("custom progress must be finite".into());
        }
        let values = (self.callback)(alpha)?;
        values.validate(&self.channels)?;
        Ok(values)
    }
}

/// Frozen state immediately before the clip, used on callback failure.
#[derive(Debug, Clone)]
pub struct CustomBaseline {
    pub transform: SpatialTransform,
    pub opacity: f32,
    pub fill: Option<Brush>,
    pub stroke: StrokeBrush,
}

impl CustomBaseline {
    /// Update the compiler's cursor without accumulating relative mutations.
    pub fn apply_values(&mut self, values: &CustomValues, paint_only: bool) {
        if !paint_only {
            if let Some(value) = values.position {
                self.transform.translation = value;
            }
            if let Some(value) = values.rotation {
                self.transform.rotation = DQuat::from_rotation_z(value);
            }
            if let Some(value) = values.scale {
                self.transform.scale = value;
            }
            if let Some(value) = values.opacity {
                self.opacity = value;
            }
        }
        if let Some(value) = &values.fill {
            self.fill = Some(value.clone());
        }
        if let Some(value) = &values.stroke {
            self.stroke.brush = Some(value.clone());
        }
        if let Some(value) = values.stroke_width {
            self.stroke.style.width = value;
        }
    }
}

/// A diagnostic remains available to preview hosts and makes export fail.
#[derive(Debug, Clone)]
pub struct CustomAnimationDiagnostic {
    pub target: ObjectId,
    pub alpha: f64,
    pub message: String,
}

#[derive(Debug, Default, Resource)]
pub struct CustomAnimationDiagnostics(pub Vec<CustomAnimationDiagnostic>);

impl CustomAnimationDiagnostics {
    pub fn first_error(&self) -> Option<String> {
        self.0.first().map(|error| {
            format!(
                "custom animation {:?} failed at alpha {}: {}",
                error.target, error.alpha, error.message
            )
        })
    }
}

/// Shared application path for playback tweens and arbitrary timeline seeks.
#[derive(Debug, Clone)]
pub struct CustomPropertyLens {
    pub animation: CustomAnimation,
    pub baseline: CustomBaseline,
    pub target: ObjectId,
    /// Text/group descendants receive their parent's paint channels only.
    pub paint_only: bool,
    pub frozen_baseline: Arc<std::sync::Mutex<Option<CustomBaseline>>>,
}

impl CustomPropertyLens {
    pub fn apply(&self, world: &mut World, entity: Entity, alpha: f64) {
        if world.contains_resource::<crate::PreparingPropertySources>()
            && self
                .frozen_baseline
                .lock()
                .expect("custom baseline poisoned")
                .is_none()
        {
            return;
        }
        let mut state = self
            .frozen_baseline
            .lock()
            .expect("custom baseline poisoned")
            .clone()
            .unwrap_or_else(|| self.baseline.clone());
        match self.animation.evaluate(alpha) {
            Ok(values) => state.apply_values(&values, self.paint_only),
            Err(message) => {
                let mut diagnostics =
                    world.get_resource_or_insert_with(CustomAnimationDiagnostics::default);
                if !diagnostics
                    .0
                    .iter()
                    .any(|error| error.target == self.target && error.message == message)
                {
                    eprintln!(
                        "custom animation {:?} failed at alpha {alpha}: {message}",
                        self.target
                    );
                    diagnostics.0.push(CustomAnimationDiagnostic {
                        target: self.target,
                        alpha,
                        message,
                    });
                }
            }
        }
        // Apply only owned channels, including on failure. Other simultaneous
        // clips remain intact and an invalid result never partially applies.
        for channel in self.animation.channels().iter().copied() {
            if self.paint_only && !channel.is_paint() {
                continue;
            }
            match channel {
                CustomChannel::Position => {
                    if let Some(mut transform) = world.get_mut::<SpatialTransform>(entity) {
                        transform.translation = state.transform.translation;
                    }
                }
                CustomChannel::Rotation => {
                    if let Some(mut transform) = world.get_mut::<SpatialTransform>(entity) {
                        transform.rotation = state.transform.rotation;
                    }
                }
                CustomChannel::Scale => {
                    if let Some(mut transform) = world.get_mut::<SpatialTransform>(entity) {
                        transform.scale = state.transform.scale;
                    }
                }
                CustomChannel::Opacity => {
                    if let Ok(mut entity) = world.get_entity_mut(entity) {
                        entity.insert(Opacity(state.opacity));
                    }
                }
                CustomChannel::Fill => {
                    if let Ok(mut entity) = world.get_entity_mut(entity) {
                        entity.insert(FillBrush(state.fill.clone()));
                    }
                }
                CustomChannel::Stroke => {
                    if let Some(mut stroke) = world.get_mut::<StrokeBrush>(entity) {
                        stroke.brush.clone_from(&state.stroke.brush);
                    }
                }
                CustomChannel::StrokeWidth => {
                    if let Some(mut stroke) = world.get_mut::<StrokeBrush>(entity) {
                        stroke.style.width = state.stroke.style.width;
                    }
                }
            }
        }
    }
}

impl CustomPropertyLens {
    pub fn capture_start(&self, world: &World, target: Entity) {
        let baseline = CustomBaseline {
            transform: world
                .get::<SpatialTransform>(target)
                .copied()
                .unwrap_or(self.baseline.transform),
            opacity: world
                .get::<Opacity>(target)
                .map_or(self.baseline.opacity, |opacity| opacity.0),
            fill: world
                .get::<FillBrush>(target)
                .and_then(|fill| fill.0.clone()),
            stroke: world
                .get::<StrokeBrush>(target)
                .cloned()
                .unwrap_or_else(|| self.baseline.stroke.clone()),
        };
        *self
            .frozen_baseline
            .lock()
            .expect("custom baseline poisoned") = Some(baseline);
    }
}

impl AnimatableLens for CustomPropertyLens {
    fn interpolate(&self, world: &mut World, entity: Entity, t: f64) {
        self.apply(world, entity, t);
    }
    fn clone_box(&self) -> Box<dyn AnimatableLens> {
        Box::new(self.clone())
    }
    fn type_name(&self) -> &'static str {
        "CustomProperties"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn callback_validation_is_atomic_and_requires_exact_channels() {
        assert!(CustomAnimation::new(vec![], |_| Ok(CustomValues::default())).is_err());
        assert!(
            CustomAnimation::new(vec![CustomChannel::Opacity; 2], |_| Ok(
                CustomValues::default()
            ))
            .is_err()
        );
        let missing =
            CustomAnimation::new(
                vec![CustomChannel::Opacity],
                |_| Ok(CustomValues::default()),
            )
            .unwrap();
        assert!(missing.evaluate(0.3).is_err());
        let invalid = CustomAnimation::new(
            vec![CustomChannel::Position, CustomChannel::Opacity],
            |_| {
                Ok(CustomValues {
                    position: Some(DVec3::ONE),
                    opacity: Some(f32::NAN),
                    ..Default::default()
                })
            },
        )
        .unwrap();
        assert!(invalid.evaluate(0.3).is_err());
    }

    #[test]
    fn invalid_multi_channel_result_restores_owned_channels_without_touching_scale() {
        let animation = CustomAnimation::new(
            vec![CustomChannel::Position, CustomChannel::Opacity],
            |_| {
                Ok(CustomValues {
                    position: Some(DVec3::splat(99.0)),
                    opacity: Some(f32::NAN),
                    ..Default::default()
                })
            },
        )
        .unwrap();
        let baseline = CustomBaseline {
            transform: SpatialTransform::default(),
            opacity: 0.8,
            fill: None,
            stroke: StrokeBrush::default(),
        };
        let lens = CustomPropertyLens {
            animation,
            baseline,
            target: ObjectId::from_raw(1),
            paint_only: false,

            frozen_baseline: Default::default(),
        };
        let mut world = World::new();
        let entity = world
            .spawn((
                SpatialTransform {
                    translation: DVec3::ONE,
                    scale: DVec3::splat(3.0),
                    ..Default::default()
                },
                Opacity(0.2),
            ))
            .id();
        lens.apply(&mut world, entity, 0.35);
        let transform = world.get::<SpatialTransform>(entity).unwrap();
        assert_eq!(transform.translation, DVec3::ZERO);
        assert_eq!(transform.scale, DVec3::splat(3.0));
        assert_eq!(world.get::<Opacity>(entity).unwrap().0, 0.8);
        assert!(
            world
                .resource::<CustomAnimationDiagnostics>()
                .first_error()
                .is_some()
        );
    }

    #[test]
    fn exact_callback_repeated_seek_and_error_restore_owned_baseline() {
        let animation = CustomAnimation::new(
            vec![CustomChannel::Position, CustomChannel::Opacity],
            |alpha| {
                if alpha == 0.7 {
                    return Err("bad sample".into());
                }
                Ok(CustomValues {
                    position: Some(DVec3::new(alpha * alpha, alpha.sin(), 0.0)),
                    opacity: Some(alpha as f32),
                    ..Default::default()
                })
            },
        )
        .unwrap();
        let baseline = CustomBaseline {
            transform: SpatialTransform::default(),
            opacity: 0.8,
            fill: None,
            stroke: StrokeBrush::default(),
        };
        let lens = CustomPropertyLens {
            animation,
            baseline,
            target: ObjectId::from_raw(1),
            paint_only: false,

            frozen_baseline: Default::default(),
        };
        let mut world = World::new();
        let entity = world
            .spawn((
                SpatialTransform::default(),
                Opacity(0.8),
                StrokeBrush::default(),
            ))
            .id();
        for alpha in [0.12345, 1.0, 0.7, 0.0, 0.12345] {
            lens.apply(&mut world, entity, alpha);
            let transform = world.get::<SpatialTransform>(entity).unwrap();
            if alpha == 0.7 {
                assert_eq!(transform.translation, DVec3::ZERO);
                assert_eq!(world.get::<Opacity>(entity).unwrap().0, 0.8);
            } else {
                assert_eq!(
                    transform.translation,
                    DVec3::new(alpha * alpha, alpha.sin(), 0.0)
                );
            }
        }
        assert_eq!(world.resource::<CustomAnimationDiagnostics>().0.len(), 1);
    }
}
