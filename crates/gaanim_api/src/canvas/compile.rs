//! Compile — replay Canvas ops into SceneBuilder/Bevy.

use std::collections::HashMap;

use bevy::prelude::*;
use gaanim_core::ObjectId;
use gaanim_core::glam::DVec3;
use gaanim_core::kurbo::Point;
use gaanim_math::SpatialTransform;
use gaanim_scene::{FillBrush, Opacity, RenderOrder, StrokeBrush, Visible};
use gaanim_timeline::clip::SceneId;
use gaanim_timeline::timeline::Timeline;

use crate::anim::{AnimationBuilder, AnimationType};
use crate::builder::{MobjectRef, SceneBuilder};
use crate::canvas::canvas_impl::Canvas;
use crate::canvas::ops::{Op, Segment};
use crate::canvas::types::{ObjectSpec, SpawnKind};

impl Canvas {
    pub fn compile_into<'w, 's>(
        &self,
        commands: &mut Commands<'w, 's>,
        timeline: &mut Timeline,
        font_registry: &gaanim_text::font::FontRegistry,
        text_config: &gaanim_text::prelude::TextConfig,
    ) {
        let segments = self
            .state
            .lock()
            .expect("canvas state poisoned")
            .segments
            .clone();
        let mut builder = SceneBuilder::new(commands, timeline, font_registry, text_config);
        let mut scene_ids: Vec<SceneId> = Vec::new();
        let mut id_map: HashMap<ObjectId, ObjectId> = HashMap::new();

        for seg in &segments {
            scene_ids.push(builder.begin_scene(&seg.name));
            Self::replay_seg(&mut builder, seg, &mut id_map);
            builder.end_scene();
        }

        for (i, seg) in segments.iter().enumerate() {
            if let Some(prev) = seg.prev_segment
                && prev < i
                && i < scene_ids.len()
                && prev < scene_ids.len()
                && let Some(tr) = &seg.transition
            {
                builder
                    .timeline
                    .connect(scene_ids[prev], scene_ids[i], tr.clone());
            }
        }

        if let Some(bg) = self.background {
            let r = bg.to_rgba8();
            builder
                .commands
                .insert_resource(ClearColor(Color::srgb_u8(r.r, r.g, r.b)));
        }
    }

    pub fn compile(&self, world: &mut World) {
        let mut timeline = world
            .remove_resource::<Timeline>()
            .expect("Timeline missing");
        let font_registry = world
            .remove_resource::<gaanim_text::font::FontRegistry>()
            .expect("FontRegistry missing");
        let text_config = world
            .remove_resource::<gaanim_text::prelude::TextConfig>()
            .expect("TextConfig missing");
        let mut commands = world.commands();
        self.compile_into(&mut commands, &mut timeline, &font_registry, &text_config);
        world.insert_resource(timeline);
        world.insert_resource(font_registry);
        world.insert_resource(text_config);
    }

    fn replay_seg(
        builder: &mut SceneBuilder,
        seg: &Segment,
        id_map: &mut HashMap<ObjectId, ObjectId>,
    ) {
        for op in &seg.ops {
            match op {
                Op::Spawn(spec) => {
                    let spec = spec.lock().expect("object spec poisoned").clone();
                    let actual = Self::spawn_one(builder, &spec, id_map);
                    id_map.insert(spec.id, actual.id);
                }
                Op::Animate { anim, active } => {
                    if *active && let Some(anim) = Self::remap_anim(anim, id_map) {
                        builder.play(anim);
                    }
                }
                Op::Play(anims) => {
                    let remapped: Vec<AnimationBuilder> = anims
                        .iter()
                        .filter_map(|anim| Self::remap_anim(anim, id_map))
                        .collect();
                    builder.play_parallel(remapped);
                }
                Op::Wait(d) => builder.wait(*d),
                Op::Slide => builder.slide(),
                Op::Show(id) => {
                    if let Some(id) = id_map.get(id).copied()
                        && let Some(st) = builder.states.get_mut(id)
                    {
                        builder.commands.entity(st.entity).insert(Visible);
                    }
                }
                Op::Hide(id) => {
                    if let Some(id) = id_map.get(id).copied()
                        && let Some(st) = builder.states.get_mut(id)
                    {
                        builder.commands.entity(st.entity).remove::<Visible>();
                    }
                }
                Op::Remove(id) => {
                    if let Some(id) = id_map.get(id).copied()
                        && let Some(st) = builder.states.get(id)
                    {
                        builder.commands.entity(st.entity).despawn();
                    }
                }
            }
        }
    }

    fn remap_anim(
        anim: &AnimationBuilder,
        id_map: &HashMap<ObjectId, ObjectId>,
    ) -> Option<AnimationBuilder> {
        let target = *id_map.get(&anim.target)?;
        let anim_type = match &anim.anim_type {
            AnimationType::FadeTransform { target } => AnimationType::FadeTransform {
                target: *id_map.get(target)?,
            },
            other => other.clone(),
        };
        Some(AnimationBuilder {
            target,
            anim_type,
            duration: anim.duration,
            rate_func: anim.rate_func.clone(),
        })
    }

    fn spawn_one(
        builder: &mut SceneBuilder,
        spec: &ObjectSpec,
        id_map: &HashMap<ObjectId, ObjectId>,
    ) -> MobjectRef {
        match &spec.kind {
            SpawnKind::Circle(r) => {
                let b = builder.circle(*r);
                Self::finish_spawn_builder(b, spec)
            }
            SpawnKind::Rect(w, h) => {
                let b = builder.rectangle(*w, *h);
                Self::finish_spawn_builder(b, spec)
            }
            SpawnKind::RoundedRect(w, h, r) => {
                let b = builder.rounded_rect(*w, *h, *r);
                Self::finish_spawn_builder(b, spec)
            }
            SpawnKind::Square(sz) => {
                let b = builder.square(*sz);
                Self::finish_spawn_builder(b, spec)
            }
            SpawnKind::Dot(r) => {
                let b = builder.dot(*r);
                Self::finish_spawn_builder(b, spec)
            }
            SpawnKind::Ellipse(rx, ry) => {
                let b = builder.ellipse(*rx, *ry);
                Self::finish_spawn_builder(b, spec)
            }
            SpawnKind::Line(x1, y1, x2, y2) => {
                let b = builder.line(Point::new(*x1, *y1), Point::new(*x2, *y2));
                Self::finish_spawn_builder(b, spec)
            }
            SpawnKind::Arrow(x1, y1, x2, y2) => {
                let b = builder.arrow(Point::new(*x1, *y1), Point::new(*x2, *y2));
                Self::finish_spawn_builder(b, spec)
            }
            SpawnKind::Text(t) => {
                let mr = builder.text(t, "Inter", 48.0);
                Self::post_apply(builder, mr.id, spec);
                mr
            }
            SpawnKind::Title(t) => {
                let mr = builder.text(t, "Inter", 64.0);
                Self::post_apply(builder, mr.id, spec);
                mr
            }
            SpawnKind::Subtitle(t) => {
                let mr = builder.text(t, "Inter", 36.0);
                Self::post_apply(builder, mr.id, spec);
                mr
            }
            SpawnKind::Equation(f) => {
                let mr = builder.equation(f);
                Self::post_apply(builder, mr.id, spec);
                mr
            }
            SpawnKind::Group(ids) => {
                let refs: Vec<MobjectRef> = ids
                    .iter()
                    .filter_map(|id| id_map.get(id).copied().map(|id| MobjectRef { id }))
                    .collect();
                let mr = builder.group(&refs);
                Self::post_apply(builder, mr.id, spec);
                mr
            }
        }
    }

    fn finish_spawn_builder<'b, 'w, 's, 'a>(
        mut b: crate::builder::MobjectSpawnBuilder<'b, 'w, 's, 'a>,
        spec: &ObjectSpec,
    ) -> MobjectRef {
        if let Some((c, w)) = spec.stroke {
            b = b.stroke(c, w);
        }
        if let Some(ref f) = spec.fill {
            b = b.fill_brush(f.clone());
        }
        b = b.opacity(spec.opacity).z_index(spec.z_index);
        if spec.position != DVec3::ZERO {
            b = b.translate(spec.position.x, spec.position.y);
        }
        b.spawn()
    }

    fn post_apply(builder: &mut SceneBuilder, id: ObjectId, spec: &ObjectSpec) {
        if let Some(st) = builder.states.get_mut(id) {
            if let Some((c, w)) = spec.stroke {
                let sb = StrokeBrush::new(c, w);
                st.stroke = sb.clone();
                builder.commands.entity(st.entity).insert(sb);
            }
            if let Some(ref f) = spec.fill {
                st.fill = Some(f.clone());
                builder
                    .commands
                    .entity(st.entity)
                    .insert(FillBrush(Some(f.clone())));
            }
            if spec.opacity != 1.0 {
                st.opacity = spec.opacity;
                builder
                    .commands
                    .entity(st.entity)
                    .insert(Opacity(spec.opacity));
            }
            if spec.position != DVec3::ZERO {
                let transform = SpatialTransform {
                    translation: spec.position,
                    ..Default::default()
                };
                st.transform = transform;
                builder.commands.entity(st.entity).insert(transform);
            }
            if spec.z_index != 0 {
                builder.commands.entity(st.entity).insert(RenderOrder {
                    z_index: spec.z_index,
                    ..Default::default()
                });
            }
        }
    }
}
