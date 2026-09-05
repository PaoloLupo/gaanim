//! Absolute reactive drawable channels and deterministic scalar-source targets.
use std::collections::HashMap;

use bevy::prelude::{Component, Entity, Resource, World};
use gaanim_core::{
    ObjectId,
    glam::{DQuat, DVec3, EulerRot},
};
use gaanim_math::{RateFunc, SpatialTransform};
use gaanim_scene::Opacity;

use crate::{AnimatableLens, FloatSignal, PlaybackState, ScalarSource};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PropertyChannel {
    Translation,
    Rotation,
    Scale,
    Opacity,
}

impl PropertyChannel {
    pub fn name(self) -> &'static str {
        match self {
            Self::Translation => "translation",
            Self::Rotation => "rotation",
            Self::Scale => "scale",
            Self::Opacity => "opacity",
        }
    }
}

/// Absolute channel value. Coordinates and angles use scene units and radians.
#[derive(Debug, Clone)]
pub enum PropertySources {
    TextTranslation {
        values: [ScalarSource; 3],
        horizontal: f64,
        center_multiline: bool,
    },
    Translation {
        values: [ScalarSource; 3],
        anchor: Option<DVec3>,
    },
    Rotation([ScalarSource; 3]),
    Scale([ScalarSource; 3]),
    Opacity(ScalarSource),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PropertyValue {
    Translation(DVec3),
    Rotation(DQuat),
    Scale(DVec3),
    Opacity(f32),
}

impl PropertySources {
    pub fn channel(&self) -> PropertyChannel {
        match self {
            Self::TextTranslation { .. } | Self::Translation { .. } => PropertyChannel::Translation,
            Self::Rotation(_) => PropertyChannel::Rotation,
            Self::Scale(_) => PropertyChannel::Scale,
            Self::Opacity(_) => PropertyChannel::Opacity,
        }
    }
    pub fn sources(&self) -> &[ScalarSource] {
        match self {
            Self::TextTranslation { values, .. }
            | Self::Translation { values, .. }
            | Self::Rotation(values)
            | Self::Scale(values) => values,
            Self::Opacity(value) => std::slice::from_ref(value),
        }
    }
    pub fn is_constant(&self) -> bool {
        self.sources()
            .iter()
            .all(|source| source.constant_value().is_some())
    }
    pub fn evaluate(
        &self,
        time: f64,
        mut signal: impl FnMut(ObjectId) -> Option<f64>,
    ) -> Result<PropertyValue, String> {
        let values = self
            .sources()
            .iter()
            .map(|source| {
                source
                    .evaluate(time, &mut signal)
                    .map_err(|error| error.to_string())
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(match self {
            Self::TextTranslation { .. } | Self::Translation { .. } => {
                PropertyValue::Translation(DVec3::new(values[0], values[1], values[2]))
            }
            Self::Rotation(_) => PropertyValue::Rotation(DQuat::from_euler(
                EulerRot::XYZ,
                values[0],
                values[1],
                values[2],
            )),
            Self::Scale(_) => PropertyValue::Scale(DVec3::new(values[0], values[1], values[2])),
            Self::Opacity(_) => PropertyValue::Opacity(values[0].clamp(0.0, 1.0) as f32),
        })
    }
}

impl PropertyValue {
    pub fn read(world: &World, target: Entity, channel: PropertyChannel) -> Option<Self> {
        if channel == PropertyChannel::Opacity {
            return world
                .get::<Opacity>(target)
                .map(|value| Self::Opacity(value.0));
        }
        let transform = world.get::<SpatialTransform>(target)?;
        Some(match channel {
            PropertyChannel::Translation => Self::Translation(transform.translation),
            PropertyChannel::Rotation => Self::Rotation(transform.rotation),
            PropertyChannel::Scale => Self::Scale(transform.scale),
            PropertyChannel::Opacity => unreachable!(),
        })
    }
    pub fn interpolate(self, to: Self, alpha: f64) -> Self {
        match (self, to) {
            (Self::Translation(a), Self::Translation(b)) => Self::Translation(a.lerp(b, alpha)),
            (Self::Scale(a), Self::Scale(b)) => Self::Scale(a.lerp(b, alpha)),
            (Self::Rotation(a), Self::Rotation(b)) => Self::Rotation(a.slerp(b, alpha)),
            (Self::Opacity(a), Self::Opacity(b)) => Self::Opacity(a + (b - a) * alpha as f32),
            _ => self,
        }
    }
    pub fn apply(self, world: &mut World, target: Entity) {
        if let Self::Opacity(value) = self {
            if let Some(mut opacity) = world.get_mut::<Opacity>(target) {
                if opacity.0 != value {
                    opacity.0 = value;
                }
            }
        } else if let Some(mut transform) = world.get_mut::<SpatialTransform>(target) {
            let mut next = *transform;
            match self {
                Self::Translation(value) => next.translation = value,
                Self::Rotation(value) => next.rotation = value,
                Self::Scale(value) => next.scale = value,
                Self::Opacity(_) => unreachable!(),
            }
            if *transform != next {
                *transform = next;
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct PropertyParameter {
    pub logical: ObjectId,
    pub native: ObjectId,
    pub entity: Entity,
    pub initial: f64,
}

#[derive(Debug, Clone)]
pub struct ResolvedPropertySources {
    pub sources: PropertySources,
    pub parameters: Vec<PropertyParameter>,
    /// Offset of the selected local anchor after the authored rotation/scale.
    pub anchor_offset: DVec3,
    pub local_anchor: Option<DVec3>,
}

impl ResolvedPropertySources {
    pub fn evaluate(
        &self,
        world: &World,
        time: f64,
        frozen: bool,
    ) -> Result<PropertyValue, String> {
        if frozen
            && self.parameters.iter().any(|parameter| {
                world.get::<crate::Updater>(parameter.entity).is_some()
                    && world
                        .get::<crate::SampledSeriesDriver>(parameter.entity)
                        .is_none()
            })
        {
            return Err("frozen scalar targets cannot reconstruct a custom signal updater; use an explicit Parameter animation or sampled signal".into());
        }
        let value = self.sources.evaluate(time, |logical| {
            let parameter = self
                .parameters
                .iter()
                .find(|parameter| parameter.logical == logical)?;
            if frozen {
                if let Some(driver) = world
                    .get::<crate::SampledSeriesDriver>(parameter.entity)
                    .filter(|driver| driver.property == crate::SampledProperty::Signal)
                {
                    let mut driver = driver.clone();
                    driver.stop_at = world
                        .get_resource::<PropertySignalStops>()
                        .and_then(|stops| stops.0.get(&parameter.native).copied());
                    return Some(driver.sample_at(time));
                }
                Some(
                    world
                        .get_resource::<PropertySignalTimeline>()
                        .and_then(|history| history.sample(parameter.native, time))
                        .unwrap_or(parameter.initial),
                )
            } else {
                if let Some(driver) = world
                    .get::<crate::SampledSeriesDriver>(parameter.entity)
                    .filter(|driver| driver.property == crate::SampledProperty::Signal)
                {
                    let mut driver = driver.clone();
                    if let Some(stops) = world.get_resource::<PropertySignalStops>() {
                        driver.stop_at = stops.0.get(&parameter.native).copied();
                    }
                    return Some(driver.sample_at(time));
                }
                world
                    .get::<FloatSignal>(parameter.entity)
                    .map(|value| value.value)
            }
        })?;
        Ok(match value {
            PropertyValue::Translation(value) => {
                PropertyValue::Translation(value - self.anchor_offset)
            }
            other => other,
        })
    }
}

/// Signal clips copied from the timeline for exact source evaluation at a clip's start.
#[derive(Debug, Clone)]
pub struct PropertySignalClip {
    pub start: f64,
    pub duration: f64,
    pub from: f64,
    pub to: f64,
    pub rate: RateFunc,
}

#[derive(Resource, Debug, Default)]
pub struct PropertySignalTimeline(pub HashMap<ObjectId, Vec<PropertySignalClip>>);

#[derive(Resource, Debug, Default)]
pub struct PropertySignalStops(pub HashMap<ObjectId, f64>);

#[derive(Debug, Clone)]
pub struct PropertyBindingDiagnostic {
    pub target: ObjectId,
    pub time: f64,
    pub message: String,
}

#[derive(Resource, Debug, Default)]
pub struct PropertyBindingDiagnostics(pub Vec<PropertyBindingDiagnostic>);

impl PropertyBindingDiagnostics {
    pub fn first_error(&self) -> Option<String> {
        self.0.first().map(|error| {
            format!(
                "reactive property {:?} failed at {} seconds: {}",
                error.target, error.time, error.message
            )
        })
    }
}

fn report_property_error(world: &mut World, target: Entity, time: f64, message: String) {
    let Some(target) = world.get::<gaanim_scene::MobjectId>(target).map(|id| id.0) else {
        return;
    };
    let mut diagnostics = world.get_resource_or_insert_with(PropertyBindingDiagnostics::default);
    if !diagnostics
        .0
        .iter()
        .any(|error| error.target == target && error.message == message)
    {
        eprintln!("reactive property {target:?} failed at {time}: {message}");
        diagnostics.0.push(PropertyBindingDiagnostic {
            target,
            time,
            message,
        });
    }
}

impl PropertySignalTimeline {
    pub fn sample(&self, id: ObjectId, time: f64) -> Option<f64> {
        let clips = self.0.get(&id)?;
        let mut value = clips.first()?.from;
        for clip in clips {
            if clip.start > time {
                break;
            }
            let alpha = if clip.duration <= 0.0 {
                1.0
            } else {
                ((time - clip.start) / clip.duration).clamp(0.0, 1.0)
            };
            value = clip.from + (clip.to - clip.from) * clip.rate.evaluate(alpha);
        }
        Some(value)
    }
}

#[derive(Component, Debug, Clone)]
pub struct PropertyBinding {
    pub target: Entity,
    pub source: ResolvedPropertySources,
    pub start: f64,
    pub end: Option<f64>,
    pub fallback: PropertyValue,
}

pub fn property_binding_system(world: &mut World) {
    let time = world
        .get_resource::<PlaybackState>()
        .map_or(0.0, |state| state.current_time);
    apply_property_bindings(world, time);
}

pub fn apply_property_bindings(world: &mut World, time: f64) {
    let mut updates = {
        let mut query = world.query::<&PropertyBinding>();
        query
            .iter(world)
            .filter(|binding| time >= binding.start && binding.end.is_none_or(|end| time < end))
            .map(|binding| {
                (
                    binding.target,
                    binding.fallback,
                    binding.source.evaluate(world, time, false),
                    binding.source.local_anchor,
                    binding.source.anchor_offset,
                )
            })
            .collect::<Vec<_>>()
    };
    // Apply scale and rotation first so a bound visual/text anchor uses the
    // same instant's transform, including when both channels are reactive.
    updates.sort_by_key(|(_, fallback, _, _, _)| matches!(fallback, PropertyValue::Translation(_)));
    for (target, fallback, result, local_anchor, anchor_offset) in updates {
        match result {
            Ok(mut value) => {
                if let (PropertyValue::Translation(position), Some(local), Some(transform)) =
                    (value, local_anchor, world.get::<SpatialTransform>(target))
                {
                    let offset =
                        transform.to_mat4().transform_point3(local) - transform.translation;
                    value = PropertyValue::Translation(position + anchor_offset - offset);
                }
                value.apply(world, target);
            }
            Err(message) => {
                fallback.apply(world, target);
                report_property_error(world, target, time, message);
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct PropertySourceLens {
    pub from: PropertyValue,
    pub to: ResolvedPropertySources,
    pub start: f64,
    pub previous: Option<std::sync::Arc<PropertySourceLens>>,
    pub continuation: Option<(PropertyValue, bool)>,
    pub end_alpha: f64,
    /// Snapshot shared by playback lenses and the timeline's exact-start capture.
    pub frozen: std::sync::Arc<std::sync::Mutex<Option<FrozenProperty>>>,
    /// Same-scene visual anchor, resolved from the clip-start world snapshot.
    pub endpoint: Option<crate::TrackingEndpoint>,
}

#[derive(Debug, Clone)]
pub struct FrozenProperty {
    pub from: PropertyValue,
    pub to: Result<PropertyValue, String>,
}

/// Prevent pending clips from overwriting the state being captured at their start.
#[derive(Resource)]
pub struct PreparingPropertySources;

impl PropertySourceLens {
    pub fn value_at(&self, world: &World, alpha: f64) -> Result<PropertyValue, String> {
        if let Some(frozen) = self
            .frozen
            .lock()
            .expect("property snapshot poisoned")
            .as_ref()
        {
            return Ok(frozen.from.interpolate(frozen.to.clone()?, alpha));
        }
        let from = if let Some(previous) = &self.previous {
            previous.value_at(world, previous.end_alpha)?
        } else {
            self.from
        };
        let to = if let Some((to, relative)) = self.continuation {
            if relative {
                match (from, to) {
                    (PropertyValue::Translation(from), PropertyValue::Translation(delta)) => {
                        PropertyValue::Translation(from + delta)
                    }
                    (PropertyValue::Rotation(from), PropertyValue::Rotation(delta)) => {
                        PropertyValue::Rotation((from * delta).normalize())
                    }
                    (PropertyValue::Scale(from), PropertyValue::Scale(factor)) => {
                        PropertyValue::Scale(from * factor)
                    }
                    _ => to,
                }
            } else {
                to
            }
        } else {
            self.to.evaluate(world, self.start, true)?
        };
        Ok(from.interpolate(to, alpha))
    }

    pub fn capture_start(&self, world: &World, target: Entity) {
        let from = match self.from {
            PropertyValue::Translation(_) => world
                .get::<SpatialTransform>(target)
                .map(|t| PropertyValue::Translation(t.translation)),
            PropertyValue::Rotation(_) => world
                .get::<SpatialTransform>(target)
                .map(|t| PropertyValue::Rotation(t.rotation)),
            PropertyValue::Scale(_) => world
                .get::<SpatialTransform>(target)
                .map(|t| PropertyValue::Scale(t.scale)),
            PropertyValue::Opacity(_) => world
                .get::<Opacity>(target)
                .map(|o| PropertyValue::Opacity(o.0)),
        }
        .unwrap_or(self.from);
        let to = if let Some(endpoint) = &self.endpoint {
            crate::resolve_tracking_endpoint(endpoint, world)
                .map(|point| {
                    let local = world
                        .get::<bevy::prelude::ChildOf>(target)
                        .map_or(point, |parent| {
                            crate::tracking_world_to_local(parent.parent(), point, world)
                        });
                    let offset = self
                        .to
                        .local_anchor
                        .and_then(|anchor| {
                            world.get::<SpatialTransform>(target).map(|transform| {
                                transform.to_mat4().transform_point3(anchor) - transform.translation
                            })
                        })
                        .unwrap_or(self.to.anchor_offset);
                    PropertyValue::Translation(local - offset)
                })
                .ok_or_else(|| {
                    "animation anchor could not be resolved at the clip start".to_owned()
                })
        } else if let Some((destination, relative)) = self.continuation {
            Ok(if relative {
                match (from, destination) {
                    (PropertyValue::Translation(a), PropertyValue::Translation(b)) => {
                        PropertyValue::Translation(a + b)
                    }
                    (PropertyValue::Rotation(a), PropertyValue::Rotation(b)) => {
                        PropertyValue::Rotation((a * b).normalize())
                    }
                    (PropertyValue::Scale(a), PropertyValue::Scale(b)) => {
                        PropertyValue::Scale(a * b)
                    }
                    _ => destination,
                }
            } else {
                destination
            })
        } else {
            self.to.evaluate(world, self.start, false).map(|value| {
                if let (PropertyValue::Translation(position), Some(anchor), Some(transform)) = (
                    value,
                    self.to.local_anchor,
                    world.get::<SpatialTransform>(target),
                ) {
                    PropertyValue::Translation(
                        position + self.to.anchor_offset
                            - (transform.to_mat4().transform_point3(anchor)
                                - transform.translation),
                    )
                } else {
                    value
                }
            })
        };
        *self.frozen.lock().expect("property snapshot poisoned") =
            Some(FrozenProperty { from, to });
    }
}

impl AnimatableLens for PropertySourceLens {
    fn interpolate(&self, world: &mut World, target: Entity, alpha: f64) {
        if world.contains_resource::<PreparingPropertySources>()
            && self
                .frozen
                .lock()
                .expect("property snapshot poisoned")
                .is_none()
        {
            return;
        }
        match self.value_at(world, alpha) {
            Ok(value) => value.apply(world, target),
            Err(message) => {
                let fallback = self
                    .frozen
                    .lock()
                    .expect("property snapshot poisoned")
                    .as_ref()
                    .map_or(self.from, |frozen| frozen.from);
                fallback.apply(world, target);
                report_property_error(world, target, self.start, message);
            }
        }
    }
    fn clone_box(&self) -> Box<dyn AnimatableLens> {
        Box::new(self.clone())
    }
    fn type_name(&self) -> &'static str {
        "PropertySource"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn sampled_signal_targets_freeze_exactly_and_failures_restore_a_fixed_fallback() {
        let id = ObjectId::from_raw(42);
        let mut world = World::new();
        let driver = crate::SampledSeriesDriver::new(
            vec![0.0, 2.0],
            vec![0.0, 1.0],
            crate::SampledProperty::Signal,
            crate::SampledInterpolation::Linear,
            1.0,
            0.0,
        )
        .unwrap();
        let signal = world.spawn((FloatSignal::new(99.0), driver)).id();
        let sampled = ResolvedPropertySources {
            sources: PropertySources::Opacity(ScalarSource::signal(id)),
            parameters: vec![PropertyParameter {
                logical: id,
                native: id,
                entity: signal,
                initial: 0.0,
            }],
            anchor_offset: DVec3::ZERO,
            local_anchor: None,
        };
        assert_eq!(
            sampled.evaluate(&world, 1.0, true).unwrap(),
            PropertyValue::Opacity(0.5)
        );
        let target = world
            .spawn((Opacity(1.0), gaanim_scene::MobjectId(id)))
            .id();
        let function =
            crate::ReactiveFunction::new(0, 1, vec![crate::ReactiveInput::Time], |args| {
                if args[0] > 0.5 {
                    Err("outside domain".into())
                } else {
                    Ok(vec![args[0]])
                }
            });
        world.spawn(PropertyBinding {
            target,
            source: ResolvedPropertySources {
                sources: PropertySources::Opacity(ScalarSource::function(function).unwrap()),
                parameters: vec![],
                anchor_offset: DVec3::ZERO,
                local_anchor: None,
            },
            start: 0.0,
            end: None,
            fallback: PropertyValue::Opacity(1.0),
        });
        for time in [0.75, 0.25, 0.75] {
            apply_property_bindings(&mut world, time);
            assert_eq!(
                world.get::<Opacity>(target).unwrap().0,
                if time > 0.5 { 1.0 } else { time as f32 }
            );
        }
        assert!(
            world
                .resource::<PropertyBindingDiagnostics>()
                .first_error()
                .unwrap()
                .contains("outside domain")
        );
    }
    #[test]
    fn source_destination_samples_signal_at_delayed_start() {
        let id = ObjectId::from_raw(1);
        let mut world = World::new();
        let signal = world.spawn(FloatSignal::new(100.0)).id();
        let target = world
            .spawn((SpatialTransform::default(), Opacity(1.0)))
            .id();
        world.insert_resource(PropertySignalTimeline(HashMap::from([(
            id,
            vec![PropertySignalClip {
                start: 0.0,
                duration: 4.0,
                from: 0.0,
                to: 8.0,
                rate: RateFunc::Linear,
            }],
        )])));
        let source = ResolvedPropertySources {
            sources: PropertySources::Translation {
                values: [ScalarSource::signal(id), 0.0.into(), 0.0.into()],
                anchor: None,
            },
            parameters: vec![PropertyParameter {
                logical: id,
                native: id,
                entity: signal,
                initial: 0.0,
            }],
            anchor_offset: DVec3::ZERO,
            local_anchor: None,
        };
        let lens = PropertySourceLens {
            from: PropertyValue::Translation(DVec3::ZERO),
            to: source,
            start: 1.0,
            previous: None,
            continuation: None,
            end_alpha: 1.0,

            frozen: Default::default(),
            endpoint: None,
        };
        for alpha in [1.0, 0.5, 1.0] {
            lens.interpolate(&mut world, target, alpha);
            assert_eq!(
                world.get::<SpatialTransform>(target).unwrap().translation.x,
                2.0 * alpha
            );
        }
    }
    #[test]
    fn binding_windows_are_reversible_and_end_exclusive() {
        let mut world = World::new();
        let target = world.spawn(Opacity(1.0)).id();
        world.spawn(PropertyBinding {
            target,
            source: ResolvedPropertySources {
                sources: PropertySources::Opacity(ScalarSource::Time),
                parameters: vec![],
                anchor_offset: DVec3::ZERO,
                local_anchor: None,
            },
            start: 0.2,
            end: Some(0.8),
            fallback: PropertyValue::Opacity(1.0),
        });
        for (time, expected) in [(0.5, 0.5), (0.8, 1.0), (0.3, 0.3)] {
            world.get_mut::<Opacity>(target).unwrap().0 = 1.0;
            apply_property_bindings(&mut world, time);
            assert_eq!(world.get::<Opacity>(target).unwrap().0, expected);
        }
    }
}
