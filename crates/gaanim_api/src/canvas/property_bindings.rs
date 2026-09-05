use std::collections::HashMap;

use gaanim_animation::{
    PropertyBinding, PropertyChannel, PropertyParameter, PropertySources, PropertyValue,
    ResolvedPropertySources,
};
use gaanim_core::{ObjectId, glam::DVec3};

use super::ops::Op;
use super::{Anim, DrawableHandle};
use crate::anim::{AnimationType, PropertySourceTarget};
use crate::builder::SceneBuilder;

impl From<&super::Parameter> for gaanim_animation::ScalarSource {
    fn from(parameter: &super::Parameter) -> Self {
        parameter.source()
    }
}

impl From<super::Parameter> for gaanim_animation::ScalarSource {
    fn from(parameter: super::Parameter) -> Self {
        parameter.source()
    }
}

impl DrawableHandle {
    pub fn bind_text_position(
        self,
        values: [gaanim_animation::ScalarSource; 3],
        anchor: gaanim_text::prelude::TextAnchor,
        center_multiline: bool,
    ) -> Result<Self, String> {
        let horizontal = match anchor {
            gaanim_text::prelude::TextAnchor::BaselineLeft => -1.0,
            gaanim_text::prelude::TextAnchor::BaselineCenter => 0.0,
            gaanim_text::prelude::TextAnchor::BaselineRight => 1.0,
        };
        self.bind_property(PropertySources::TextTranslation {
            values,
            horizontal,
            center_multiline,
        })
    }
    pub fn property_is_bound(&self, channel: PropertyChannel) -> bool {
        self.state
            .lock()
            .expect("canvas state poisoned")
            .bound_properties
            .contains(&(self.id, channel))
    }

    pub(crate) fn clear_property_binding(&self, channel: PropertyChannel) {
        let mut state = self.state.lock().expect("canvas state poisoned");
        if state.bound_properties.remove(&(self.id, channel)) {
            state.active_mut().ops.push(Op::ClearPropertyBinding {
                target: self.id,
                channel,
            });
        }
    }

    /// Bind an absolute channel to explicit scalar sources from this scene.
    pub fn bind_property(self, sources: PropertySources) -> Result<Self, String> {
        let channel = sources.channel();
        if channel != PropertyChannel::Opacity
            && (self.layout_owner().is_some() || self.is_live_derived_geometry())
        {
            return Err("layout or live derived geometry owns this drawable's transform".into());
        }
        let mut state = self.state.lock().expect("canvas state poisoned");
        if sources
            .sources()
            .iter()
            .flat_map(|source| source.scene_owners())
            .any(|owner| *owner != state.scene_id)
        {
            return Err("reactive inputs must belong to this Scene".into());
        }
        if sources
            .sources()
            .iter()
            .flat_map(|source| source.parameter_ids())
            .any(|id| !state.parameter_values.contains_key(&id))
        {
            return Err("reactive inputs must belong to this Scene".into());
        }
        // A binding is an authored timeline operation; retain the declaration
        // state before later fixed setters create reversible cuts over it.
        state.freeze_spawn_specs();
        state.bound_properties.insert((self.id, channel));
        state.active_mut().ops.push(Op::SetPropertyBinding {
            target: self.id,
            sources,
        });
        drop(state);
        Ok(self)
    }
}

impl Anim {
    /// The owning drawable identity, used by language bindings to validate sources.
    pub fn property_drawable(&self) -> Option<DrawableHandle> {
        let state = self.owner.as_ref()?.clone();
        let spec = state
            .lock()
            .ok()?
            .object_specs
            .get(&self.inner.target)?
            .clone();
        let kind = spec.lock().ok()?.kind.clone();
        Some(DrawableHandle::new(self.inner.target, kind, state, 0))
    }

    /// Freeze a scalar-source destination at this animation's scheduled start.
    pub fn property_source(self, sources: PropertySources) -> Result<Self, String> {
        if self.property_target_is_text_selection() {
            return Err("TextSelection.animate requires fixed fill and opacity values".into());
        }
        let channel = sources.channel();
        let drawable = self
            .property_drawable()
            .ok_or("property sources require a Drawable animation proxy")?;
        if drawable.property_is_bound(channel) {
            return Err(format!(
                "{} is reactively bound; animate its Parameter or assign a fixed value first",
                channel.name()
            ));
        }
        {
            let state = drawable.state.lock().expect("canvas state poisoned");
            if sources
                .sources()
                .iter()
                .flat_map(|source| source.scene_owners())
                .any(|owner| *owner != state.scene_id)
            {
                return Err("reactive inputs must belong to this Scene".into());
            }
            if sources
                .sources()
                .iter()
                .flat_map(|source| source.parameter_ids())
                .any(|id| !state.parameter_values.contains_key(&id))
            {
                return Err("reactive inputs must belong to this Scene".into());
            }
        }
        if channel != PropertyChannel::Opacity && !self.property_position_is_free() {
            return Err("layout or live derived geometry owns this drawable's transform".into());
        }
        Ok(self.update_properties(|properties| {
            match channel {
                PropertyChannel::Translation => properties.translation = None,
                PropertyChannel::Rotation => properties.rotation = None,
                PropertyChannel::Scale => properties.scale = None,
                PropertyChannel::Opacity => properties.opacity = None,
            }
            properties
                .source_targets
                .retain(|target| target.sources.channel() != channel);
            properties
                .source_targets
                .push(PropertySourceTarget::new(sources));
        }))
    }
}

impl SceneBuilder<'_, '_, '_> {
    pub(crate) fn try_schedule_anchor_source(
        &mut self,
        anim: &crate::anim::AnimationBuilder,
        track: gaanim_timeline::clip::TrackId,
    ) -> bool {
        let AnimationType::TranslateToAnchorPoint { point } = &anim.anim_type else {
            return false;
        };
        let Some(reference) = self.states.get(point.object) else {
            return false;
        };
        let endpoint = gaanim_animation::TrackingEndpoint::EntityAnchor {
            entity: reference.entity,
            normalized: point.normalized,
            offset: point.offset,
        };
        let local = reference.bounds.center()
            + reference.bounds.size() * 0.5 * point.normalized
            + point.offset;
        let provisional = self
            .get_world_transform(point.object)
            .to_mat4()
            .transform_point3(local);
        let sources = PropertySourceTarget::new(PropertySources::Translation {
            values: [
                provisional.x.into(),
                provisional.y.into(),
                provisional.z.into(),
            ],
            anchor: Some(DVec3::ZERO),
        });
        let from = self.property_source_from(anim.target, PropertyChannel::Translation);
        let to = self.resolve_property_sources(anim.target, &sources);
        if let Some(state) = self.states.get_mut(anim.target) {
            state.transform.translation = provisional - to.anchor_offset;
        }
        let start = self.current_time + anim.delay;
        let lens = gaanim_animation::PropertySourceLens {
            from,
            to,
            start,
            previous: None,
            continuation: None,
            end_alpha: anim.rate_func.evaluate(1.0),
            frozen: Default::default(),
            endpoint: Some(endpoint),
        };
        self.property_source_cursors.insert(
            (anim.target, PropertyChannel::Translation),
            std::sync::Arc::new(lens.clone()),
        );
        self.timeline.add_clip(
            track,
            start,
            anim.duration,
            gaanim_timeline::clip::ClipPayload::Animation(gaanim_timeline::clip::AnimationSpec {
                target: anim.target,
                lens: gaanim_timeline::clip::PropertyLensSpec::PropertySource(lens),
                rate_func: anim.rate_func.clone(),
                delay: 0.0,
                label: self.current_label.clone(),
            }),
        );
        true
    }

    pub(crate) fn try_schedule_property_continuation(
        &mut self,
        anim: &crate::anim::AnimationBuilder,
        track: gaanim_timeline::clip::TrackId,
    ) -> bool {
        use gaanim_animation::{PropertySourceLens, PropertyValue as V};
        let channel = match anim.anim_type {
            AnimationType::TranslateTo { .. }
            | AnimationType::TranslateBy { .. }
            | AnimationType::TranslateAnchorTo { .. }
            | AnimationType::TranslateToAnchorPoint { .. } => PropertyChannel::Translation,
            AnimationType::RotateTo { .. }
            | AnimationType::RotateBy { pivot: None, .. }
            | AnimationType::RotateBy3D { .. } => PropertyChannel::Rotation,
            AnimationType::ScaleTo { .. }
            | AnimationType::ScaleUniform { .. }
            | AnimationType::ScaleBy3D { .. } => PropertyChannel::Scale,
            AnimationType::FadeTo { .. } | AnimationType::FadeIn | AnimationType::FadeOut => {
                PropertyChannel::Opacity
            }
            _ => return false,
        };
        let Some(previous) = self
            .property_source_cursors
            .get(&(anim.target, channel))
            .cloned()
        else {
            return false;
        };
        let from = self.property_source_from(anim.target, channel);
        let state = self
            .states
            .get(anim.target)
            .expect("property target exists");
        let (destination, relative) = match anim.anim_type {
            AnimationType::TranslateTo { to } => (V::Translation(to), false),
            AnimationType::TranslateAnchorTo { to, anchor } => (
                V::Translation(
                    gaanim_layout::compute_move_to(state.bounds, &state.transform, to, anchor)
                        .translation,
                ),
                false,
            ),
            AnimationType::TranslateToAnchorPoint { point } => {
                let Some(reference) = self.states.get(point.object) else {
                    return false;
                };
                let local = reference.bounds.center()
                    + reference.bounds.size() * 0.5 * point.normalized
                    + point.offset;
                let to = self
                    .get_world_transform(point.object)
                    .to_mat4()
                    .transform_point3(local);
                (
                    V::Translation(
                        gaanim_layout::compute_move_to(
                            state.bounds,
                            &state.transform,
                            to,
                            gaanim_layout::Anchor::Center,
                        )
                        .translation,
                    ),
                    false,
                )
            }
            AnimationType::TranslateBy { delta } => (V::Translation(delta), true),
            AnimationType::RotateTo { to } => (V::Rotation(to), false),
            AnimationType::RotateBy { angle_radians, .. } => (
                V::Rotation(gaanim_core::glam::DQuat::from_rotation_z(angle_radians)),
                true,
            ),
            AnimationType::RotateBy3D { delta } => (V::Rotation(delta), true),
            AnimationType::ScaleTo { to } => (V::Scale(to), false),
            AnimationType::ScaleUniform { factor } => (V::Scale(DVec3::splat(factor)), true),
            AnimationType::ScaleBy3D { factor } => (V::Scale(factor), true),
            AnimationType::FadeTo { to } => (V::Opacity(to), false),
            AnimationType::FadeIn => (V::Opacity(1.0), false),
            AnimationType::FadeOut => (V::Opacity(0.0), false),
            _ => return false,
        };
        let provisional = if relative {
            match (from, destination) {
                (V::Translation(a), V::Translation(b)) => V::Translation(a + b),
                (V::Rotation(a), V::Rotation(b)) => V::Rotation((a * b).normalize()),
                (V::Scale(a), V::Scale(b)) => V::Scale(a * b),
                _ => destination,
            }
        } else {
            destination
        };
        let state = self
            .states
            .get_mut(anim.target)
            .expect("property target exists");
        match provisional {
            V::Translation(value) => state.transform.translation = value,
            V::Rotation(value) => state.transform.rotation = value,
            V::Scale(value) => state.transform.scale = value,
            V::Opacity(value) => state.opacity = value,
        }
        let start = self.current_time + anim.delay;
        let lens = PropertySourceLens {
            from,
            to: previous.to.clone(),
            start,
            previous: Some(previous),
            continuation: Some((destination, relative)),
            end_alpha: anim.rate_func.evaluate(1.0),

            frozen: Default::default(),
            endpoint: None,
        };
        self.property_source_cursors
            .insert((anim.target, channel), std::sync::Arc::new(lens.clone()));
        self.timeline.add_clip(
            track,
            start,
            anim.duration,
            gaanim_timeline::clip::ClipPayload::Animation(gaanim_timeline::clip::AnimationSpec {
                target: anim.target,
                lens: gaanim_timeline::clip::PropertyLensSpec::PropertySource(lens),
                rate_func: anim.rate_func.clone(),
                delay: 0.0,
                label: None,
            }),
        );
        true
    }

    pub(crate) fn resolve_property_sources(
        &self,
        target: ObjectId,
        source: &PropertySourceTarget,
    ) -> ResolvedPropertySources {
        let parameters = source
            .parameters
            .iter()
            .filter_map(|&(logical, native)| {
                Some(PropertyParameter {
                    logical,
                    native,
                    entity: self.states.get(native)?.entity,
                    initial: self.float_signals.get(&native).copied().unwrap_or(0.0),
                })
            })
            .collect();
        let local_anchor = match (&source.sources, self.states.get(target)) {
            (
                PropertySources::TextTranslation {
                    horizontal,
                    center_multiline,
                    ..
                },
                Some(state),
            ) => {
                let local = self
                    .text_metrics
                    .get(&target)
                    .filter(|metrics| metrics.line_count > 0)
                    .map(|metrics| {
                        if *center_multiline && metrics.line_count > 1 {
                            state.bounds.center()
                        } else {
                            DVec3::new(
                                state.bounds.center().x + state.bounds.size().x * 0.5 * horizontal,
                                metrics.first_baseline,
                                state.bounds.center().z,
                            )
                        }
                    })
                    .unwrap_or_else(|| state.bounds.center());
                Some(local)
            }
            (
                PropertySources::Translation {
                    anchor: Some(anchor),
                    ..
                },
                Some(state),
            ) => {
                let local = state.bounds.center() + state.bounds.size() * 0.5 * *anchor;
                Some(local)
            }
            _ => None,
        };
        let anchor_offset = local_anchor
            .zip(self.states.get(target))
            .map(|(local, state)| {
                state.transform.to_mat4().transform_point3(local) - state.transform.translation
            })
            .unwrap_or(DVec3::ZERO);
        ResolvedPropertySources {
            sources: source.sources.clone(),
            parameters,
            anchor_offset,
            local_anchor,
        }
    }

    pub(crate) fn clear_compiled_property_binding(
        &mut self,
        target: ObjectId,
        channel: PropertyChannel,
    ) {
        if let Some((entity, mut binding)) = self.property_bindings.remove(&(target, channel)) {
            binding.end = Some(self.current_time);
            self.commands.entity(entity).insert(binding);
        }
    }

    pub(crate) fn compile_property_binding(
        &mut self,
        target: ObjectId,
        sources: &PropertySources,
        ids: &HashMap<ObjectId, ObjectId>,
    ) {
        let channel = sources.channel();
        self.clear_compiled_property_binding(target, channel);
        let mut source = PropertySourceTarget::new(sources.clone());
        for (_, native) in &mut source.parameters {
            *native = *ids.get(native).unwrap_or(native);
        }
        let source = self.resolve_property_sources(target, &source);
        let Some(state) = self.states.get(target) else {
            return;
        };
        let binding = PropertyBinding {
            target: state.entity,
            source,
            start: self.current_time,
            end: None,
            fallback: self.property_source_from(target, channel),
        };
        let entity = self.commands.spawn(binding.clone()).id();
        self.property_bindings
            .insert((target, channel), (entity, binding));
    }

    pub(crate) fn property_source_from(
        &self,
        target: ObjectId,
        channel: PropertyChannel,
    ) -> PropertyValue {
        let state = self.states.get(target).expect("property target exists");
        match channel {
            PropertyChannel::Translation => PropertyValue::Translation(state.transform.translation),
            PropertyChannel::Rotation => PropertyValue::Rotation(state.transform.rotation),
            PropertyChannel::Scale => PropertyValue::Scale(state.transform.scale),
            PropertyChannel::Opacity => PropertyValue::Opacity(state.opacity),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::canvas::SceneModel;
    use bevy::prelude::{Entity, With, Without, World};
    use gaanim_animation::FloatSignal;
    use gaanim_math::{RateFunc, SpatialTransform};
    use gaanim_scene::Opacity;
    use gaanim_timeline::{snapshot::WorldSnapshot, timeline::Timeline};

    fn compile(canvas: &SceneModel) -> (World, Timeline, Entity) {
        let mut world = World::new();
        world.insert_resource(Timeline::new());
        world.insert_resource(gaanim_text::font::FontRegistry::new());
        world.insert_resource(gaanim_text::prelude::TextConfig::default());
        canvas.compile(&mut world);
        world.flush();
        let target = world
            .query_filtered::<Entity, (With<Opacity>, Without<FloatSignal>)>()
            .iter(&world)
            .next()
            .unwrap();
        let mut timeline = world.remove_resource::<Timeline>().unwrap();
        timeline.add_keyframe(0.0, WorldSnapshot::capture(&mut world));
        (world, timeline, target)
    }

    #[test]
    fn delayed_source_target_and_following_relative_clip_use_exact_start_values() {
        let mut canvas = SceneModel::new(320, 180);
        let parameter = canvas.parameter(0.0).unwrap();
        let dot = canvas.circle(0.1);
        let movement = dot
            .animate()
            .move_to(&parameter, 0.0)
            .delay(1.0)
            .duration(1.0)
            .rate_func(RateFunc::Linear);
        canvas
            .play_items(vec![
                parameter
                    .animate()
                    .set(8.0)
                    .duration(4.0)
                    .rate_func(RateFunc::Linear)
                    .into(),
                movement.into(),
            ])
            .unwrap();
        canvas
            .play_items(vec![
                dot.animate()
                    .shift_by(10.0, 0.0)
                    .duration(1.0)
                    .rate_func(RateFunc::Linear)
                    .into(),
            ])
            .unwrap();
        let (mut world, mut timeline, entity) = compile(&canvas);
        for (time, expected) in [(1.5, 1.0), (5.0, 12.0), (4.5, 7.0), (0.0, 0.0), (2.0, 2.0)] {
            timeline.seek(&mut world, time);
            assert!(
                (world.get::<SpatialTransform>(entity).unwrap().translation.x - expected).abs()
                    < 1e-9,
                "time {time}"
            );
        }
    }

    #[test]
    fn delayed_drawable_destination_is_captured_at_its_actual_start() {
        let mut canvas = SceneModel::new(320, 180);
        let dot = canvas.circle(0.1);
        let destination = canvas.circle(0.2);
        canvas
            .play_items(vec![
                destination
                    .animate()
                    .move_to(8.0, 0.0)
                    .duration(4.0)
                    .rate_func(RateFunc::Linear)
                    .into(),
                dot.animate()
                    .move_to_drawable(&destination)
                    .unwrap()
                    .delay(1.0)
                    .duration(1.0)
                    .rate_func(RateFunc::Linear)
                    .into(),
            ])
            .unwrap();
        canvas
            .play_items(vec![
                dot.animate()
                    .shift_by(10.0, 0.0)
                    .duration(1.0)
                    .rate_func(RateFunc::Linear)
                    .into(),
            ])
            .unwrap();
        let (mut world, mut timeline, entity) = compile(&canvas);
        for (time, expected) in [
            (0.0, 0.0),
            (1.5, 1.0),
            (5.0, 12.0),
            (4.5, 7.0),
            (1.5, 1.0),
            (0.0, 0.0),
        ] {
            timeline.seek(&mut world, time);
            assert!(
                (world.get::<SpatialTransform>(entity).unwrap().translation.x - expected).abs()
                    < 1e-9,
                "time {time}"
            );
        }
    }

    #[test]
    fn replacement_binding_and_fixed_cut_restore_at_exact_boundaries() {
        let mut canvas = SceneModel::new(320, 180);
        let first = canvas.parameter(0.2).unwrap();
        let second = canvas.parameter(0.8).unwrap();
        let dot = canvas
            .circle(0.1)
            .bind_property(PropertySources::Opacity(first.source()))
            .unwrap();
        canvas.wait(1.0);
        let dot = dot
            .bind_property(PropertySources::Opacity(second.source()))
            .unwrap();
        canvas.wait(1.0);
        dot.opacity(0.4);
        canvas.wait(1.0);
        let (mut world, mut timeline, entity) = compile(&canvas);
        for (time, expected) in [
            (0.0, 0.2),
            (0.999999, 0.2),
            (1.0, 0.8),
            (2.0, 0.4),
            (1.5, 0.8),
            (0.0, 0.2),
        ] {
            timeline.seek(&mut world, time);
            assert!(
                (world.get::<Opacity>(entity).unwrap().0 as f64 - expected).abs() < 1e-6,
                "time {time}"
            );
        }
    }

    #[test]
    fn nested_rust_sources_retain_scene_ownership_even_when_local_ids_match() {
        let mut first = SceneModel::new(320, 180);
        let foreign = first.parameter(0.5).unwrap();
        let mut second = SceneModel::new(320, 180);
        let local = second.parameter(0.5).unwrap();
        assert_eq!(foreign.drawable().id, local.drawable().id);
        let source = gaanim_animation::ScalarSource::Function(
            gaanim_animation::ReactiveFunction::from_sources(0, 1, vec![foreign.source()], |v| {
                Ok(vec![v[0]])
            })
            .map_scalar(|v| v * 2.0)
            .unwrap(),
        );
        let dot = second.circle(0.1);
        assert!(
            dot.clone()
                .bind_property(PropertySources::Opacity(source.clone()))
                .is_err()
        );
        assert!(
            dot.animate()
                .property_source(PropertySources::Opacity(source))
                .is_err()
        );
    }

    #[test]
    fn rust_absolute_setters_accept_mixed_sources_and_fixed_values_end_them() {
        let mut canvas = SceneModel::new(320, 180);
        let p = canvas.parameter(0.5).unwrap();
        let dot = canvas
            .circle(0.1)
            .move_to_3d(&p, 2.0, 3.0)
            .rotate_to_3d(0.0, 0.0, &p)
            .scale_to_3d(1.0, &p, 1.0)
            .opacity(&p);
        canvas.wait(1.0);
        dot.move_to_3d(4.0, 5.0, 6.0)
            .rotate_to(0.0)
            .scale_to(1.0)
            .opacity(1.0_f32);
        canvas.wait(1.0);
        let (mut world, mut timeline, entity) = compile(&canvas);
        for time in [0.5, 1.5, 0.5] {
            timeline.seek(&mut world, time);
            let transform = world.get::<SpatialTransform>(entity).unwrap();
            if time < 1.0 {
                assert_eq!(transform.translation, DVec3::new(0.5, 2.0, 3.0));
                assert_eq!(transform.scale, DVec3::new(1.0, 0.5, 1.0));
                assert!(
                    (transform.rotation - gaanim_core::glam::DQuat::from_rotation_z(0.5)).length()
                        < 1e-9
                );
                assert_eq!(world.get::<Opacity>(entity).unwrap().0, 0.5);
            } else {
                assert_eq!(transform.translation, DVec3::new(4.0, 5.0, 6.0));
                assert_eq!(transform.scale, DVec3::ONE);
                assert_eq!(world.get::<Opacity>(entity).unwrap().0, 1.0);
            }
        }
    }

    #[test]
    fn fixed_setter_replaces_binding_with_reversible_cut() {
        let mut canvas = SceneModel::new(320, 180);
        let parameter = canvas.parameter(0.2).unwrap();
        let dot = canvas
            .circle(0.1)
            .bind_property(PropertySources::Opacity(parameter.source()))
            .unwrap();
        assert!(
            canvas
                .play_items(vec![dot.animate().opacity(0.5).into()])
                .is_err()
        );
        canvas
            .play_items(vec![
                parameter
                    .animate()
                    .set(0.8)
                    .duration(1.0)
                    .rate_func(RateFunc::Linear)
                    .into(),
            ])
            .unwrap();
        dot.opacity(0.4);
        canvas.wait(1.0);
        let (mut world, mut timeline, entity) = compile(&canvas);
        for (time, expected) in [(0.5, 0.5), (1.5, 0.4), (0.25, 0.35)] {
            timeline.seek(&mut world, time);
            assert!((world.get::<Opacity>(entity).unwrap().0 as f64 - expected).abs() < 1e-6);
        }
    }
}
