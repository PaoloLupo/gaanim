//! Per-instance influence ("falloffs") that drive drawable channels.
//!
//! A [`FalloffExpr`] turns an instance's index, its distance to a target or a
//! seeded noise field into a value, and [`FalloffEffect`]s connect that value
//! to a channel: scale, rotation, opacity, offset, fill color or aiming at a
//! target. Every value is a pure function of timeline time and of the scene
//! state at that time, so playback, seeks and exports agree and no Python runs
//! per member and frame.
//!
//! Effects are applied to the local state right before hierarchy propagation
//! and removed right after (like [`crate::ProceduralMotion`]), so authored
//! values stay untouched. A fill color stays applied until the start of the
//! next frame, because exports extract after the frame.

use std::sync::Arc;

use bevy::prelude::*;
use gaanim_core::{
    ObjectId,
    glam::{DQuat, DVec2},
    peniko::{Brush, Color},
};
use gaanim_math::{Noise, RateFunc, SpatialTransform};
use gaanim_scene::{FillBrush, Opacity};

use crate::updaters::{PlaybackState, resolve_entity_bounds};

/// How the closeness to a target (1 at the target, 0 at the edge) becomes a value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FalloffShape {
    /// The closeness itself.
    Linear,
    /// Smoothstep: gentle at both ends.
    Smooth,
    /// The square: influence concentrates near the target.
    Sharp,
    /// The square root: influence reaches far and drops near the edge.
    Round,
}

impl FalloffShape {
    pub fn apply(self, closeness: f64) -> f64 {
        let c = closeness.clamp(0.0, 1.0);
        match self {
            Self::Linear => c,
            Self::Smooth => c * c * (3.0 - 2.0 * c),
            Self::Sharp => c * c,
            Self::Round => c.sqrt(),
        }
    }
}

/// What an influence is measured against.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FalloffTarget {
    /// A fixed point in scene coordinates.
    Point(DVec2),
    /// A scene object, resolved to its entity when the scene compiles.
    Object(ObjectId),
    /// A live entity: its bounds center, wherever animations put it.
    Entity(Entity),
}

/// A scalar influence, usually in `[0, 1]`, per instance and instant.
#[derive(Debug, Clone)]
pub enum FalloffExpr {
    Constant(f64),
    /// `index / (count - 1)` through an easing; the first instance is 0.
    Index {
        easing: RateFunc,
        reverse: bool,
    },
    /// 1 at the target, 0 at `radius` and beyond.
    Distance {
        target: FalloffTarget,
        radius: f64,
        shape: FalloffShape,
    },
    /// 0 at `from`, 1 at `to`, along the line between them.
    Linear {
        from: FalloffTarget,
        to: FalloffTarget,
        shape: FalloffShape,
    },
    /// Simplex noise over the instance's position and the time, in `[0, 1]`.
    Noise {
        noise: Arc<Noise>,
        /// Noise cycles per scene unit.
        scale: f64,
        /// How fast the field drifts, in noise units per second.
        speed: f64,
    },
    /// A function of the time alone, the same for every instance, such as
    /// an audio signal.
    Source(crate::ScalarSource),
    /// Maps `[0, 1]` to `[low, high]`.
    Remap {
        input: Box<FalloffExpr>,
        low: f64,
        high: f64,
    },
    Invert(Box<FalloffExpr>),
    Add(Box<FalloffExpr>, Box<FalloffExpr>),
    Sub(Box<FalloffExpr>, Box<FalloffExpr>),
    Mul(Box<FalloffExpr>, Box<FalloffExpr>),
    Min(Box<FalloffExpr>, Box<FalloffExpr>),
    Max(Box<FalloffExpr>, Box<FalloffExpr>),
}

/// What an instance looks like to an influence at one instant.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FalloffInput {
    pub index: usize,
    pub count: usize,
    /// The instance's center in scene coordinates.
    pub position: DVec2,
    pub time: f64,
}

impl FalloffExpr {
    /// The influence for `input`; `target` resolves a target to a point.
    pub fn evaluate(
        &self,
        input: &FalloffInput,
        target: &mut dyn FnMut(&FalloffTarget) -> Option<DVec2>,
    ) -> f64 {
        match self {
            Self::Constant(value) => *value,
            Self::Index { easing, reverse } => {
                let unit = if input.count <= 1 {
                    0.0
                } else {
                    input.index as f64 / (input.count - 1) as f64
                };
                easing.evaluate(if *reverse { 1.0 - unit } else { unit })
            }
            Self::Distance {
                target: goal,
                radius,
                shape,
            } => {
                let Some(point) = target(goal) else {
                    return 0.0;
                };
                let distance = input.position.distance(point);
                shape.apply(1.0 - distance / radius.max(f64::EPSILON))
            }
            Self::Linear { from, to, shape } => {
                let (Some(start), Some(end)) = (target(from), target(to)) else {
                    return 0.0;
                };
                let axis = end - start;
                let length_squared = axis.length_squared();
                if length_squared <= f64::EPSILON {
                    return 0.0;
                }
                shape.apply(((input.position - start).dot(axis) / length_squared).clamp(0.0, 1.0))
            }
            Self::Noise {
                noise,
                scale,
                speed,
            } => {
                let drift = input.time * speed;
                let sample = noise.sample(
                    input.position.x * scale + drift,
                    input.position.y * scale + drift * 0.618,
                );
                0.5 + 0.5 * sample
            }
            Self::Source(source) => source.evaluate(input.time, |_| None).unwrap_or(0.0),
            Self::Remap {
                input: inner,
                low,
                high,
            } => low + (high - low) * inner.evaluate(input, target),
            Self::Invert(inner) => 1.0 - inner.evaluate(input, target),
            Self::Add(a, b) => a.evaluate(input, target) + b.evaluate(input, target),
            Self::Sub(a, b) => a.evaluate(input, target) - b.evaluate(input, target),
            Self::Mul(a, b) => a.evaluate(input, target) * b.evaluate(input, target),
            Self::Min(a, b) => a.evaluate(input, target).min(b.evaluate(input, target)),
            Self::Max(a, b) => a.evaluate(input, target).max(b.evaluate(input, target)),
        }
    }

    /// Replaces every [`FalloffTarget::Object`] with the entity `resolve` finds.
    pub fn resolve_targets(&mut self, resolve: &mut dyn FnMut(ObjectId) -> Option<Entity>) {
        match self {
            Self::Distance { target, .. } => target.resolve(resolve),
            Self::Linear { from, to, .. } => {
                from.resolve(resolve);
                to.resolve(resolve);
            }
            Self::Remap { input, .. } | Self::Invert(input) => input.resolve_targets(resolve),
            Self::Add(a, b)
            | Self::Sub(a, b)
            | Self::Mul(a, b)
            | Self::Min(a, b)
            | Self::Max(a, b) => {
                a.resolve_targets(resolve);
                b.resolve_targets(resolve);
            }
            Self::Constant(_) | Self::Index { .. } | Self::Noise { .. } | Self::Source(_) => {}
        }
    }

    /// Every scene object this influence measures against.
    pub fn objects(&self, out: &mut Vec<ObjectId>) {
        match self {
            Self::Distance { target, .. } => target.object(out),
            Self::Linear { from, to, .. } => {
                from.object(out);
                to.object(out);
            }
            Self::Remap { input, .. } | Self::Invert(input) => input.objects(out),
            Self::Add(a, b)
            | Self::Sub(a, b)
            | Self::Mul(a, b)
            | Self::Min(a, b)
            | Self::Max(a, b) => {
                a.objects(out);
                b.objects(out);
            }
            Self::Constant(_) | Self::Index { .. } | Self::Noise { .. } | Self::Source(_) => {}
        }
    }
}

impl FalloffTarget {
    fn resolve(&mut self, resolve: &mut dyn FnMut(ObjectId) -> Option<Entity>) {
        if let Self::Object(id) = *self
            && let Some(entity) = resolve(id)
        {
            *self = Self::Entity(entity);
        }
    }

    fn object(&self, out: &mut Vec<ObjectId>) {
        if let Self::Object(id) = self {
            out.push(*id);
        }
    }
}

/// Evenly spaced colors sampled by a value in `[0, 1]`.
#[derive(Debug, Clone, PartialEq)]
pub struct ColorRamp(pub Vec<Color>);

impl ColorRamp {
    pub fn at(&self, value: f64) -> Color {
        let stops = &self.0;
        match stops.len() {
            0 => Color::TRANSPARENT,
            1 => stops[0],
            count => {
                let position = value.clamp(0.0, 1.0) * (count - 1) as f64;
                let index = (position.floor() as usize).min(count - 2);
                gaanim_core::interpolate_color(
                    stops[index],
                    stops[index + 1],
                    position - index as f64,
                )
            }
        }
    }
}

/// The channel a scalar influence drives.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FalloffChannel {
    /// Multiplies the scale.
    Scale,
    /// Added to the z rotation, in radians.
    Rotation,
    /// Multiplies the opacity.
    Opacity,
    /// Added to the x translation, in scene units.
    OffsetX,
    /// Added to the y translation, in scene units.
    OffsetY,
}

/// One connection between an influence and a channel.
#[derive(Debug, Clone)]
pub enum FalloffEffect {
    Scalar {
        channel: FalloffChannel,
        expr: FalloffExpr,
    },
    /// The fill takes the ramp's color at the influence.
    Fill { ramp: ColorRamp, expr: FalloffExpr },
    /// Turns the instance so its x axis points at the target, plus `offset`
    /// radians.
    LookAt { target: FalloffTarget, offset: f64 },
}

impl FalloffEffect {
    pub fn resolve_targets(&mut self, resolve: &mut dyn FnMut(ObjectId) -> Option<Entity>) {
        match self {
            Self::Scalar { expr, .. } | Self::Fill { expr, .. } => expr.resolve_targets(resolve),
            Self::LookAt { target, .. } => target.resolve(resolve),
        }
    }

    pub fn objects(&self, out: &mut Vec<ObjectId>) {
        match self {
            Self::Scalar { expr, .. } | Self::Fill { expr, .. } => expr.objects(out),
            Self::LookAt { target, .. } => target.object(out),
        }
    }
}

/// An effect active from `start` until `end`, in absolute timeline seconds.
#[derive(Debug, Clone)]
pub struct ScheduledEffect {
    pub effect: FalloffEffect,
    pub start: f64,
    pub end: Option<f64>,
}

/// The offset the active effects give one instance at one instant.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FalloffOffset {
    pub translation: DVec2,
    pub rotation: f64,
    pub scale: f64,
    pub opacity: f64,
    pub fill: Option<Color>,
}

impl Default for FalloffOffset {
    fn default() -> Self {
        Self {
            translation: DVec2::ZERO,
            rotation: 0.0,
            scale: 1.0,
            opacity: 1.0,
            fill: None,
        }
    }
}

/// The falloff effects on one instance, with its place among its siblings.
#[derive(Component, Debug, Clone, Default)]
pub struct FalloffDrive {
    pub index: usize,
    pub count: usize,
    pub effects: Vec<ScheduledEffect>,
    applied: Option<(SpatialTransform, Option<f32>)>,
}

impl FalloffDrive {
    pub fn new(index: usize, count: usize) -> Self {
        Self {
            index,
            count,
            ..Self::default()
        }
    }

    pub fn push(&mut self, effect: FalloffEffect, start: f64) {
        self.effects.push(ScheduledEffect {
            effect,
            start,
            end: None,
        });
    }

    /// Ends every open effect at `time`.
    pub fn stop_at(&mut self, time: f64) {
        for scheduled in &mut self.effects {
            if scheduled.end.is_none() && scheduled.start <= time {
                scheduled.end = Some(time);
            }
        }
    }

    fn is_active_at(&self, time: f64) -> bool {
        self.effects
            .iter()
            .any(|scheduled| scheduled.is_active_at(time))
    }

    /// Combined offset of the effects active at `time`.
    pub fn offset_at(
        &self,
        position: DVec2,
        time: f64,
        target: &mut dyn FnMut(&FalloffTarget) -> Option<DVec2>,
    ) -> FalloffOffset {
        let input = FalloffInput {
            index: self.index,
            count: self.count,
            position,
            time,
        };
        let mut offset = FalloffOffset::default();
        for scheduled in self
            .effects
            .iter()
            .filter(|scheduled| scheduled.is_active_at(time))
        {
            match &scheduled.effect {
                FalloffEffect::Scalar { channel, expr } => {
                    let value = expr.evaluate(&input, target);
                    match channel {
                        FalloffChannel::Scale => offset.scale *= value,
                        FalloffChannel::Rotation => offset.rotation += value,
                        FalloffChannel::Opacity => offset.opacity *= value,
                        FalloffChannel::OffsetX => offset.translation.x += value,
                        FalloffChannel::OffsetY => offset.translation.y += value,
                    }
                }
                FalloffEffect::Fill { ramp, expr } => {
                    offset.fill = Some(ramp.at(expr.evaluate(&input, target)));
                }
                FalloffEffect::LookAt {
                    target: goal,
                    offset: turn,
                } => {
                    if let Some(point) = target(goal) {
                        let toward = point - position;
                        if toward.length_squared() > f64::EPSILON {
                            offset.rotation += toward.y.atan2(toward.x) + turn;
                        }
                    }
                }
            }
        }
        offset
    }
}

impl ScheduledEffect {
    fn is_active_at(&self, time: f64) -> bool {
        time >= self.start && self.end.is_none_or(|end| time < end)
    }
}

/// Fills a falloff replaced, restored at the start of the next frame.
#[derive(Resource, Debug, Default)]
pub struct FalloffFillRestore(Vec<(Entity, Option<Brush>)>);

fn instance_center(entity: Entity, world: &World) -> DVec2 {
    resolve_entity_bounds(entity, world)
        .map(|bounds| {
            let center = bounds.center();
            DVec2::new(center.x, center.y)
        })
        .or_else(|| {
            world
                .get::<SpatialTransform>(entity)
                .map(|transform| DVec2::new(transform.translation.x, transform.translation.y))
        })
        .unwrap_or(DVec2::ZERO)
}

fn set_fill(
    world: &mut World,
    entity: Entity,
    color: Color,
    replaced: &mut Vec<(Entity, Option<Brush>)>,
) {
    if let Some(mut fill) = world.get_mut::<FillBrush>(entity)
        && fill.0.is_some()
    {
        replaced.push((entity, fill.0.take()));
        fill.0 = Some(Brush::Solid(color));
    }
    let children: Vec<Entity> = world
        .get::<Children>(entity)
        .map(|children| children.iter().collect())
        .unwrap_or_default();
    for child in children {
        set_fill(world, child, color, replaced);
    }
}

/// Adds the falloff effects to local state before propagation.
pub fn apply_falloff_system(world: &mut World) {
    let time = world
        .get_resource::<PlaybackState>()
        .map_or(0.0, |state| state.current_time);
    let instances: Vec<Entity> = world
        .query_filtered::<Entity, With<FalloffDrive>>()
        .iter(world)
        .collect();
    // Everything is measured from the authored state first, so an instance's
    // own effect never moves the point another instance measures against.
    let mut results = Vec::new();
    for entity in instances {
        let Some(drive) = world.get::<FalloffDrive>(entity) else {
            continue;
        };
        if !drive.is_active_at(time) {
            continue;
        }
        let position = instance_center(entity, world);
        let offset = drive.offset_at(position, time, &mut |target| match target {
            FalloffTarget::Point(point) => Some(*point),
            FalloffTarget::Entity(entity) => Some(instance_center(*entity, world)),
            FalloffTarget::Object(_) => None,
        });
        results.push((entity, offset));
    }
    let mut replaced = Vec::new();
    for (entity, offset) in results {
        let Some(base) = world.get::<SpatialTransform>(entity).copied() else {
            continue;
        };
        let authored_opacity = world.get::<Opacity>(entity).map(|opacity| opacity.0);
        if let Some(mut transform) = world.get_mut::<SpatialTransform>(entity) {
            transform.translation.x += offset.translation.x;
            transform.translation.y += offset.translation.y;
            transform.rotation = base.rotation * DQuat::from_rotation_z(offset.rotation);
            transform.scale = base.scale * offset.scale;
        }
        if let Some(mut opacity) = world.get_mut::<Opacity>(entity) {
            opacity.0 = (opacity.0 as f64 * offset.opacity).clamp(0.0, 1.0) as f32;
        }
        if let Some(mut drive) = world.get_mut::<FalloffDrive>(entity) {
            drive.applied = Some((base, authored_opacity));
        }
        if let Some(color) = offset.fill {
            set_fill(world, entity, color, &mut replaced);
        }
    }
    if !replaced.is_empty() {
        world.insert_resource(FalloffFillRestore(replaced));
    }
}

/// Restores the authored local state after propagation.
pub fn restore_falloff_system(
    mut query: Query<(
        &mut FalloffDrive,
        &mut SpatialTransform,
        Option<&mut Opacity>,
    )>,
) {
    for (mut drive, mut transform, opacity) in &mut query {
        let Some((base, base_opacity)) = drive.applied.take() else {
            continue;
        };
        *transform = base;
        if let (Some(mut opacity), Some(value)) = (opacity, base_opacity) {
            opacity.0 = value;
        }
    }
}

/// Restores the fills a falloff replaced, at the start of the next frame.
pub fn restore_falloff_fills_system(world: &mut World) {
    let Some(mut restore) = world.remove_resource::<FalloffFillRestore>() else {
        return;
    };
    for (entity, brush) in restore.0.drain(..) {
        if let Some(mut fill) = world.get_mut::<FillBrush>(entity) {
            fill.0 = brush;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input(index: usize, count: usize, x: f64, y: f64) -> FalloffInput {
        FalloffInput {
            index,
            count,
            position: DVec2::new(x, y),
            time: 0.0,
        }
    }

    fn no_targets(_: &FalloffTarget) -> Option<DVec2> {
        None
    }

    fn eval(expr: &FalloffExpr, input: &FalloffInput) -> f64 {
        expr.evaluate(input, &mut |target| match target {
            FalloffTarget::Point(point) => Some(*point),
            _ => None,
        })
    }

    #[test]
    fn index_runs_from_zero_to_one_and_can_reverse() {
        let expr = FalloffExpr::Index {
            easing: RateFunc::Linear,
            reverse: false,
        };
        assert_eq!(eval(&expr, &input(0, 5, 0.0, 0.0)), 0.0);
        assert_eq!(eval(&expr, &input(2, 5, 0.0, 0.0)), 0.5);
        assert_eq!(eval(&expr, &input(4, 5, 0.0, 0.0)), 1.0);
        assert_eq!(eval(&expr, &input(0, 1, 0.0, 0.0)), 0.0);
        let reversed = FalloffExpr::Index {
            easing: RateFunc::Linear,
            reverse: true,
        };
        assert_eq!(eval(&reversed, &input(0, 5, 0.0, 0.0)), 1.0);
    }

    #[test]
    fn distance_is_one_at_the_target_and_zero_at_the_radius() {
        let expr = FalloffExpr::Distance {
            target: FalloffTarget::Point(DVec2::new(1.0, 1.0)),
            radius: 2.0,
            shape: FalloffShape::Linear,
        };
        assert_eq!(eval(&expr, &input(0, 1, 1.0, 1.0)), 1.0);
        assert!((eval(&expr, &input(0, 1, 2.0, 1.0)) - 0.5).abs() < 1e-12);
        assert_eq!(eval(&expr, &input(0, 1, 3.0, 1.0)), 0.0);
        assert_eq!(eval(&expr, &input(0, 1, 9.0, 9.0)), 0.0);
    }

    #[test]
    fn shapes_agree_at_the_ends_and_differ_between() {
        for shape in [
            FalloffShape::Linear,
            FalloffShape::Smooth,
            FalloffShape::Sharp,
            FalloffShape::Round,
        ] {
            assert_eq!(shape.apply(0.0), 0.0);
            assert_eq!(shape.apply(1.0), 1.0);
            assert_eq!(shape.apply(-3.0), 0.0);
            assert_eq!(shape.apply(3.0), 1.0);
        }
        assert!(FalloffShape::Sharp.apply(0.5) < 0.5);
        assert!(FalloffShape::Round.apply(0.5) > 0.5);
        assert_eq!(FalloffShape::Smooth.apply(0.5), 0.5);
    }

    #[test]
    fn linear_projects_onto_the_line_between_two_points() {
        let expr = FalloffExpr::Linear {
            from: FalloffTarget::Point(DVec2::new(0.0, 0.0)),
            to: FalloffTarget::Point(DVec2::new(4.0, 0.0)),
            shape: FalloffShape::Linear,
        };
        assert_eq!(eval(&expr, &input(0, 1, -2.0, 5.0)), 0.0);
        assert_eq!(eval(&expr, &input(0, 1, 1.0, 5.0)), 0.25);
        assert_eq!(eval(&expr, &input(0, 1, 9.0, -5.0)), 1.0);
    }

    #[test]
    fn noise_is_seeded_bounded_and_moves_with_time() {
        let make = |seed| FalloffExpr::Noise {
            noise: Arc::new(Noise::new(seed, 1.0, 1.0, 2)),
            scale: 0.7,
            speed: 0.5,
        };
        let at = |expr: &FalloffExpr, x: f64, time: f64| {
            let mut sample = input(0, 1, x, 0.3);
            sample.time = time;
            eval(expr, &sample)
        };
        let (a, b) = (make(3), make(3));
        let other = make(4);
        let mut moved = false;
        let mut differs = false;
        for step in 0..40 {
            let x = step as f64 * 0.37 - 5.0;
            let value = at(&a, x, 1.0);
            assert!((0.0..=1.0).contains(&value));
            assert_eq!(value, at(&b, x, 1.0), "same seed, same value");
            moved |= (value - at(&a, x, 2.5)).abs() > 1e-6;
            differs |= (value - at(&other, x, 1.0)).abs() > 1e-6;
        }
        assert!(moved, "the field drifts over time");
        assert!(differs, "another seed gives another field");
    }

    #[test]
    fn expressions_combine() {
        let one = || Box::new(FalloffExpr::Constant(1.0));
        let three = || Box::new(FalloffExpr::Constant(3.0));
        let sample = input(0, 1, 0.0, 0.0);
        assert_eq!(eval(&FalloffExpr::Add(one(), three()), &sample), 4.0);
        assert_eq!(eval(&FalloffExpr::Sub(one(), three()), &sample), -2.0);
        assert_eq!(eval(&FalloffExpr::Mul(three(), three()), &sample), 9.0);
        assert_eq!(eval(&FalloffExpr::Min(one(), three()), &sample), 1.0);
        assert_eq!(eval(&FalloffExpr::Max(one(), three()), &sample), 3.0);
        assert_eq!(eval(&FalloffExpr::Invert(one()), &sample), 0.0);
        let remap = FalloffExpr::Remap {
            input: Box::new(FalloffExpr::Constant(0.5)),
            low: 1.0,
            high: 3.0,
        };
        assert_eq!(eval(&remap, &sample), 2.0);
    }

    #[test]
    fn a_missing_target_gives_no_influence() {
        let expr = FalloffExpr::Distance {
            target: FalloffTarget::Object(ObjectId::from_raw(7)),
            radius: 2.0,
            shape: FalloffShape::Linear,
        };
        assert_eq!(expr.evaluate(&input(0, 1, 0.0, 0.0), &mut no_targets), 0.0);
    }

    #[test]
    fn targets_resolve_to_entities_and_are_listed() {
        let id = ObjectId::from_raw(11);
        let mut expr = FalloffExpr::Add(
            Box::new(FalloffExpr::Distance {
                target: FalloffTarget::Object(id),
                radius: 1.0,
                shape: FalloffShape::Linear,
            }),
            Box::new(FalloffExpr::Constant(1.0)),
        );
        let mut listed = Vec::new();
        expr.objects(&mut listed);
        assert_eq!(listed, vec![id]);
        let entity = World::new().spawn_empty().id();
        expr.resolve_targets(&mut |object| (object == id).then_some(entity));
        let FalloffExpr::Add(left, _) = &expr else {
            unreachable!()
        };
        assert!(matches!(
            **left,
            FalloffExpr::Distance {
                target: FalloffTarget::Entity(found),
                ..
            } if found == entity
        ));
    }

    #[test]
    fn a_ramp_blends_between_evenly_spaced_colors() {
        let red = Color::from_rgb8(255, 0, 0);
        let blue = Color::from_rgb8(0, 0, 255);
        let ramp = ColorRamp(vec![red, blue]);
        assert_eq!(ramp.at(0.0), red);
        assert_eq!(ramp.at(1.0), blue);
        assert_eq!(ramp.at(-4.0), red);
        assert_eq!(ramp.at(4.0), blue);
        let middle = ramp.at(0.5).to_rgba8();
        assert!(middle.r > 100 && middle.r < 160 && middle.b > 100 && middle.b < 160);
        let three = ColorRamp(vec![red, blue, red]);
        assert_eq!(three.at(1.0), red);
        assert_eq!(three.at(0.5), blue);
    }

    fn world_with_instances(count: usize) -> (World, Vec<Entity>, Entity) {
        let mut world = World::new();
        world.insert_resource(PlaybackState::default());
        let cursor = world.spawn(SpatialTransform::new_2d(0.0, 0.0)).id();
        let instances = (0..count)
            .map(|index| {
                let mut drive = FalloffDrive::new(index, count);
                drive.push(
                    FalloffEffect::Scalar {
                        channel: FalloffChannel::Scale,
                        expr: FalloffExpr::Remap {
                            input: Box::new(FalloffExpr::Distance {
                                target: FalloffTarget::Entity(cursor),
                                radius: 3.0,
                                shape: FalloffShape::Linear,
                            }),
                            low: 1.0,
                            high: 2.0,
                        },
                    },
                    0.0,
                );
                world
                    .spawn((
                        SpatialTransform::new_2d(index as f64, 0.0),
                        Opacity(1.0),
                        drive,
                    ))
                    .id()
            })
            .collect();
        (world, instances, cursor)
    }

    fn run_frame(world: &mut World, time: f64) -> Vec<f64> {
        world.resource_mut::<PlaybackState>().current_time = time;
        apply_falloff_system(world);
        let scales = world
            .query::<&SpatialTransform>()
            .iter(world)
            .map(|transform| transform.scale.x)
            .collect();
        let mut restore = Schedule::default();
        restore.add_systems(restore_falloff_system);
        restore.run(world);
        scales
    }

    #[test]
    fn instances_scale_with_their_distance_and_the_state_is_restored() {
        let (mut world, instances, cursor) = world_with_instances(5);
        world
            .get_mut::<SpatialTransform>(cursor)
            .unwrap()
            .translation
            .x = 1.0;
        let scales = run_frame(&mut world, 0.0);
        // The cursor entity itself is the first transform spawned.
        let instance_scales = &scales[1..];
        assert!((instance_scales[1] - 2.0).abs() < 1e-12, "at the cursor");
        assert!(instance_scales[1] > instance_scales[0]);
        assert!(instance_scales[0] > instance_scales[4]);
        for entity in instances {
            assert_eq!(
                world.get::<SpatialTransform>(entity).unwrap().scale.x,
                1.0,
                "authored scale restored after the frame"
            );
        }
    }

    #[test]
    fn a_frame_depends_only_on_its_time_and_the_scene_state() {
        let (mut world, _, cursor) = world_with_instances(6);
        let record = |world: &mut World, x: f64, time: f64| {
            world
                .get_mut::<SpatialTransform>(cursor)
                .unwrap()
                .translation
                .x = x;
            run_frame(world, time)
        };
        // Playing forward, then seeking back to the first state.
        let first = record(&mut world, 1.0, 0.5);
        let _ = record(&mut world, 4.0, 2.0);
        let _ = record(&mut world, -3.0, 7.0);
        let again = record(&mut world, 1.0, 0.5);
        assert_eq!(first, again);
    }

    #[test]
    fn effects_only_apply_between_their_start_and_end() {
        let (mut world, instances, _) = world_with_instances(2);
        {
            let mut drive = world.get_mut::<FalloffDrive>(instances[0]).unwrap();
            drive.effects[0].start = 1.0;
            drive.stop_at(2.0);
        }
        assert_eq!(run_frame(&mut world, 0.5)[1], 1.0);
        assert!(run_frame(&mut world, 1.5)[1] > 1.0);
        assert_eq!(run_frame(&mut world, 2.0)[1], 1.0);
    }

    #[test]
    fn look_at_turns_the_instance_toward_the_target() {
        let mut world = World::new();
        world.insert_resource(PlaybackState::default());
        let mut drive = FalloffDrive::new(0, 1);
        drive.push(
            FalloffEffect::LookAt {
                target: FalloffTarget::Point(DVec2::new(0.0, 5.0)),
                offset: 0.0,
            },
            0.0,
        );
        let entity = world.spawn((SpatialTransform::identity(), drive)).id();
        apply_falloff_system(&mut world);
        let turned = world.get::<SpatialTransform>(entity).unwrap().rotation;
        let (_, _, z) = turned.to_euler(gaanim_core::glam::EulerRot::XYZ);
        assert!((z - std::f64::consts::FRAC_PI_2).abs() < 1e-9);
    }

    #[test]
    fn a_fill_takes_the_ramp_color_and_returns_to_the_authored_one() {
        let mut world = World::new();
        world.insert_resource(PlaybackState::default());
        let red = Color::from_rgb8(255, 0, 0);
        let blue = Color::from_rgb8(0, 0, 255);
        let mut drive = FalloffDrive::new(0, 1);
        drive.push(
            FalloffEffect::Fill {
                ramp: ColorRamp(vec![red, blue]),
                expr: FalloffExpr::Constant(1.0),
            },
            0.0,
        );
        let green = Color::from_rgb8(0, 255, 0);
        let entity = world
            .spawn((SpatialTransform::identity(), FillBrush::color(green), drive))
            .id();
        apply_falloff_system(&mut world);
        assert_eq!(
            world.get::<FillBrush>(entity).unwrap().0,
            Some(Brush::Solid(blue))
        );
        let mut restore = Schedule::default();
        restore.add_systems(restore_falloff_fills_system);
        restore.run(&mut world);
        assert_eq!(
            world.get::<FillBrush>(entity).unwrap().0,
            Some(Brush::Solid(green))
        );
    }

    #[test]
    fn a_twenty_by_twelve_grid_is_driven_within_a_frame_budget() {
        let mut world = World::new();
        world.insert_resource(PlaybackState::default());
        let cursor = world.spawn(SpatialTransform::new_2d(0.0, 0.0)).id();
        let ramp = ColorRamp(vec![
            Color::from_rgb8(0, 0, 255),
            Color::from_rgb8(255, 200, 0),
        ]);
        for index in 0..240 {
            let mut drive = FalloffDrive::new(index, 240);
            let near = FalloffExpr::Distance {
                target: FalloffTarget::Entity(cursor),
                radius: 2.5,
                shape: FalloffShape::Smooth,
            };
            drive.push(
                FalloffEffect::Scalar {
                    channel: FalloffChannel::Scale,
                    expr: FalloffExpr::Remap {
                        input: Box::new(near.clone()),
                        low: 1.0,
                        high: 1.9,
                    },
                },
                0.0,
            );
            drive.push(
                FalloffEffect::Fill {
                    ramp: ramp.clone(),
                    expr: near,
                },
                0.0,
            );
            drive.push(
                FalloffEffect::LookAt {
                    target: FalloffTarget::Entity(cursor),
                    offset: 0.0,
                },
                0.0,
            );
            world.spawn((
                SpatialTransform::new_2d((index % 20) as f64 * 0.7, (index / 20) as f64 * 0.7),
                Opacity(1.0),
                FillBrush::color(Color::from_rgb8(0, 0, 255)),
                drive,
            ));
        }
        let mut restore = Schedule::default();
        restore.add_systems((restore_falloff_system, restore_falloff_fills_system));
        let frames = 100;
        let started = std::time::Instant::now();
        for frame in 0..frames {
            world.resource_mut::<PlaybackState>().current_time = frame as f64 / 60.0;
            world
                .get_mut::<SpatialTransform>(cursor)
                .unwrap()
                .translation
                .x = frame as f64 * 0.1;
            apply_falloff_system(&mut world);
            restore.run(&mut world);
        }
        let per_frame = started.elapsed().as_secs_f64() * 1000.0 / frames as f64;
        println!("240 members, 3 effects: {per_frame:.3} ms per frame");
        // A frame at 60 fps is 16.7 ms; the budget is a tenth of it in a dev build.
        assert!(per_frame < 2.0, "{per_frame} ms per frame");
    }
}
