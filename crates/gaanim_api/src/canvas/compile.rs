//! Compile — replay Canvas ops into SceneBuilder/Bevy.

use std::collections::HashMap;

use bevy::prelude::*;
use gaanim_core::ObjectId;
use gaanim_core::glam::DVec3;
use gaanim_core::kurbo::{Point, Vec2};
use gaanim_core::peniko::Color as PenikoColor;
use gaanim_math::Bounds3D;
use gaanim_scene::{FillBrush, Opacity, RenderOrder, StrokeBrush, Visible};
use gaanim_timeline::clip::SceneId;
use gaanim_timeline::timeline::Timeline;

use crate::anim::{AnimationBuilder, AnimationType};
use crate::builder::{MobjectRef, MobjectState, SceneBuilder};
use crate::canvas::canvas_impl::Canvas;
use crate::canvas::ops::{CanvasEndpoint, Op, Segment};
use crate::canvas::types::{LayoutOp, ObjectSpec, SpawnKind};

use gaanim_animation::{
    PointOnCurve, PositionBinding, TangentOnCurve, TracedPath, TrackingEndpoint, TrackingLine,
    Updater,
};
use gaanim_math::SpatialTransform;

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
        let mut camera_position = DVec3::ZERO;
        let mut camera_zoom = 1.0;
        let mut camera_rotation = gaanim_core::glam::DQuat::IDENTITY;
        // Raw bounds for the canvas background (visual, no margin).
        let raw_bounds = self.units.frame_bounds(self.width, self.height);
        // Inset bounds for layout operations (to_edge, to_corner respect margin).
        let m = &self.margin;
        let frame_bounds = Bounds3D::new_2d(
            raw_bounds.min.x + m.left,
            raw_bounds.min.y + m.bottom,
            raw_bounds.max.x - m.right,
            raw_bounds.max.y - m.top,
        );

        for seg in &segments {
            scene_ids.push(builder.begin_scene(&seg.name));
            Self::replay_seg(
                &mut builder,
                seg,
                &mut id_map,
                frame_bounds,
                text_config,
                &mut camera_position,
                &mut camera_zoom,
                &mut camera_rotation,
            );
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

        // Insert canvas background resource so the renderer draws a visible
        // canvas boundary, distinguishing the canvas area from the window.
        // Uses raw_bounds (no margin) — the visual background covers the full canvas.
        let bg_color = self.background.unwrap_or(gaanim_core::peniko::Color::WHITE);
        builder
            .commands
            .insert_resource(gaanim_renderer::pipeline::CanvasBackground {
                color: bg_color,
                bounds: raw_bounds,
            });

        // Clear with the canvas color as well. The drawable background is
        // world-space geometry and can be rotated by the camera; using the
        // same clear color prevents Bevy's window clear color from showing
        // through at the viewport edges during that rotation.
        let rgba = bg_color.to_rgba8();
        builder
            .commands
            .insert_resource(ClearColor(Color::srgba_u8(rgba.r, rgba.g, rgba.b, rgba.a)));
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
        frame_bounds: Bounds3D,
        text_config: &gaanim_text::prelude::TextConfig,
        camera_position: &mut DVec3,
        camera_zoom: &mut f64,
        camera_rotation: &mut gaanim_core::glam::DQuat,
    ) {
        for op in &seg.ops {
            match op {
                Op::Spawn(spec) => {
                    let spec = spec.lock().expect("object spec poisoned").clone();
                    let actual = Self::spawn_one(builder, &spec, id_map, frame_bounds, text_config);
                    id_map.insert(spec.id, actual.id);
                }
                Op::Animate { anim, active } => {
                    if *active {
                        if let Some(anim) = Self::remap_anim(anim, id_map) {
                            builder.play(anim);
                        }
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
                Op::CameraPosition { to, duration, .. } => {
                    builder.timeline.add_clip(
                        builder.default_track,
                        builder.current_time,
                        *duration,
                        gaanim_timeline::clip::ClipPayload::Animation(
                            gaanim_timeline::clip::AnimationSpec {
                                target: gaanim_core::ObjectId::from_parts(0, 1),
                                lens: gaanim_timeline::clip::PropertyLensSpec::CameraPosition {
                                    from: *camera_position,
                                    to: *to,
                                },
                                rate_func: gaanim_math::RateFunc::Smooth,
                                delay: 0.0,
                                label: None,
                            },
                        ),
                    );
                    *camera_position = *to;
                    builder.wait(*duration);
                }
                Op::CameraZoom { to, duration, .. } => {
                    builder.timeline.add_clip(
                        builder.default_track,
                        builder.current_time,
                        *duration,
                        gaanim_timeline::clip::ClipPayload::Animation(
                            gaanim_timeline::clip::AnimationSpec {
                                target: gaanim_core::ObjectId::from_parts(0, 1),
                                lens: gaanim_timeline::clip::PropertyLensSpec::CameraZoom {
                                    from: *camera_zoom,
                                    to: *to,
                                },
                                rate_func: gaanim_math::RateFunc::Smooth,
                                delay: 0.0,
                                label: None,
                            },
                        ),
                    );
                    *camera_zoom = *to;
                    builder.wait(*duration);
                }
                Op::CameraRotation { to, duration } => {
                    builder.timeline.add_clip(
                        builder.default_track,
                        builder.current_time,
                        *duration,
                        gaanim_timeline::clip::ClipPayload::Animation(
                            gaanim_timeline::clip::AnimationSpec {
                                target: gaanim_core::ObjectId::from_parts(0, 1),
                                lens: gaanim_timeline::clip::PropertyLensSpec::CameraRotation {
                                    from: *camera_rotation,
                                    to: *to,
                                },
                                rate_func: gaanim_math::RateFunc::Smooth,
                                delay: 0.0,
                                label: None,
                            },
                        ),
                    );
                    *camera_rotation = *to;
                    builder.wait(*duration);
                }
                Op::CameraFrame {
                    target,
                    margin,
                    duration,
                } => {
                    let Some(actual) = id_map.get(target).copied() else {
                        continue;
                    };
                    let Some(state) = builder.states.get(actual) else {
                        continue;
                    };
                    let bounds = state
                        .bounds
                        .transform_2d(&builder.get_world_transform(actual).to_affine_2d());
                    let width = (bounds.width() + margin * 2.0).max(1.0);
                    let height = (bounds.height() + margin * 2.0).max(1.0);
                    let zoom = (frame_bounds.width() / width)
                        .min(frame_bounds.height() / height)
                        .max(0.01);
                    let center = bounds.center();
                    for lens in [
                        gaanim_timeline::clip::PropertyLensSpec::CameraPosition {
                            from: *camera_position,
                            to: center,
                        },
                        gaanim_timeline::clip::PropertyLensSpec::CameraZoom {
                            from: *camera_zoom,
                            to: zoom,
                        },
                    ] {
                        builder.timeline.add_clip(
                            builder.default_track,
                            builder.current_time,
                            *duration,
                            gaanim_timeline::clip::ClipPayload::Animation(
                                gaanim_timeline::clip::AnimationSpec {
                                    target: gaanim_core::ObjectId::from_parts(0, 1),
                                    lens,
                                    rate_func: gaanim_math::RateFunc::Smooth,
                                    delay: 0.0,
                                    label: None,
                                },
                            ),
                        );
                    }
                    *camera_position = center;
                    *camera_zoom = zoom;
                    builder.wait(*duration);
                }
                Op::CameraFollow { target, duration } => {
                    let Some(actual) = id_map.get(target).copied() else {
                        continue;
                    };
                    builder.timeline.add_clip(
                        builder.default_track,
                        builder.current_time,
                        *duration,
                        gaanim_timeline::clip::ClipPayload::Animation(
                            gaanim_timeline::clip::AnimationSpec {
                                target: gaanim_core::ObjectId::from_parts(0, 1),
                                lens: gaanim_timeline::clip::PropertyLensSpec::CameraFollow {
                                    target: actual,
                                },
                                rate_func: gaanim_math::RateFunc::Linear,
                                delay: 0.0,
                                label: None,
                            },
                        ),
                    );
                    if let Some(state) = builder.states.get(actual) {
                        camera_position.x = state.transform.translation.x;
                        camera_position.y = state.transform.translation.y;
                    }
                    builder.wait(*duration);
                }
                Op::CameraShake {
                    amplitude,
                    frequency,
                    duration,
                } => {
                    builder.timeline.add_clip(
                        builder.default_track,
                        builder.current_time,
                        *duration,
                        gaanim_timeline::clip::ClipPayload::Animation(
                            gaanim_timeline::clip::AnimationSpec {
                                target: gaanim_core::ObjectId::from_parts(0, 1),
                                lens: gaanim_timeline::clip::PropertyLensSpec::CameraShake {
                                    origin: *camera_position,
                                    amplitude: *amplitude,
                                    frequency: *frequency,
                                },
                                rate_func: gaanim_math::RateFunc::Linear,
                                delay: 0.0,
                                label: None,
                            },
                        ),
                    );
                    builder.wait(*duration);
                }
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

                // -- Reactive ops --
                Op::AttachUpdater { target, preset } => {
                    if let Some(target_id) = id_map.get(target).copied()
                        && let Some(st) = builder.states.get(target_id)
                    {
                        let updater: Updater = preset.clone().into_updater();
                        builder.commands.entity(st.entity).insert(updater);
                    }
                }

                Op::RemoveUpdater(target) => {
                    if let Some(target_id) = id_map.get(target).copied() {
                        builder.schedule_remove_updater(target_id);
                    }
                }

                Op::AttachTracedPath {
                    target,
                    source,
                    min_distance,
                    max_points,
                } => {
                    if let Some(target_id) = id_map.get(target).copied()
                        && let Some(source_id) = id_map.get(source).copied()
                        && let Some(target_st) = builder.states.get(target_id)
                        && let Some(source_st) = builder.states.get(source_id)
                    {
                        let traced = TracedPath::new(source_st.entity, *min_distance, *max_points);
                        builder.commands.entity(target_st.entity).insert(traced);
                    }
                }

                Op::AttachPositionBinding {
                    target,
                    source,
                    axes,
                } => {
                    if let Some(target_id) = id_map.get(target).copied()
                        && let Some(source_id) = id_map.get(source).copied()
                        && let Some(target_st) = builder.states.get(target_id)
                        && let Some(source_st) = builder.states.get(source_id)
                    {
                        let binding = PositionBinding::new(source_st.entity, *axes);
                        builder.commands.entity(target_st.entity).insert(binding);
                    }
                }

                Op::AttachPositionFollow {
                    target,
                    source,
                    offset,
                } => {
                    if let Some(target_id) = id_map.get(target).copied()
                        && let Some(source_id) = id_map.get(source).copied()
                        && let Some(target_st) = builder.states.get(target_id)
                        && let Some(source_st) = builder.states.get(source_id)
                    {
                        builder.commands.entity(target_st.entity).insert(
                            PositionBinding::with_offset(
                                source_st.entity,
                                gaanim_animation::AxisMask::XY,
                                *offset,
                            ),
                        );
                    }
                }

                Op::AttachTrackingLine { target, from, to } => {
                    if let Some(target_id) = id_map.get(target).copied()
                        && let Some(st) = builder.states.get(target_id)
                    {
                        let resolve_endpoint = |ep: &CanvasEndpoint| -> TrackingEndpoint {
                            match ep {
                                CanvasEndpoint::Static(pos) => TrackingEndpoint::Static(*pos),
                                CanvasEndpoint::Entity(oid) => {
                                    if let Some(rid) = id_map.get(oid).copied() {
                                        if let Some(s) = builder.states.get(rid) {
                                            TrackingEndpoint::Entity(s.entity)
                                        } else {
                                            TrackingEndpoint::Static(DVec3::ZERO)
                                        }
                                    } else {
                                        TrackingEndpoint::Static(DVec3::ZERO)
                                    }
                                }
                            }
                        };
                        let line = TrackingLine::new(resolve_endpoint(from), resolve_endpoint(to));
                        builder.commands.entity(st.entity).insert(line);
                    }
                }

                Op::AttachTrackingSpring {
                    target,
                    from,
                    to,
                    coils,
                    amplitude,
                } => {
                    if let Some(target_id) = id_map.get(target).copied()
                        && let Some(st) = builder.states.get(target_id)
                    {
                        let resolve_endpoint = |ep: &CanvasEndpoint| -> TrackingEndpoint {
                            match ep {
                                CanvasEndpoint::Static(pos) => TrackingEndpoint::Static(*pos),
                                CanvasEndpoint::Entity(oid) => id_map
                                    .get(oid)
                                    .and_then(|rid| builder.states.get(*rid))
                                    .map(|state| TrackingEndpoint::Entity(state.entity))
                                    .unwrap_or(TrackingEndpoint::Static(DVec3::ZERO)),
                            }
                        };
                        let from = resolve_endpoint(from);
                        let to = resolve_endpoint(to);
                        let coils = *coils;
                        let amplitude = *amplitude;
                        let redraw = gaanim_animation::AlwaysRedrawRegen::new(move |world| {
                            let endpoint_position = |endpoint: &TrackingEndpoint| match endpoint {
                                TrackingEndpoint::Static(position) => *position,
                                TrackingEndpoint::Entity(entity) => world
                                    .get::<SpatialTransform>(*entity)
                                    .map(|transform| transform.translation)
                                    .unwrap_or(DVec3::ZERO),
                            };
                            let from = endpoint_position(&from);
                            let to = endpoint_position(&to);
                            gaanim_objects::primitives::spring_path(
                                Point::new(from.x, from.y),
                                Point::new(to.x, to.y),
                                coils,
                                amplitude,
                            )
                        });
                        builder.commands.entity(st.entity).insert(redraw);
                    }
                }

                Op::AttachTrackingDimension {
                    target,
                    from,
                    to,
                    offset,
                } => {
                    if let Some(target_id) = id_map.get(target).copied()
                        && let Some(st) = builder.states.get(target_id)
                    {
                        let resolve_endpoint = |ep: &CanvasEndpoint| -> TrackingEndpoint {
                            match ep {
                                CanvasEndpoint::Static(pos) => TrackingEndpoint::Static(*pos),
                                CanvasEndpoint::Entity(oid) => id_map
                                    .get(oid)
                                    .and_then(|rid| builder.states.get(*rid))
                                    .map(|state| TrackingEndpoint::Entity(state.entity))
                                    .unwrap_or(TrackingEndpoint::Static(DVec3::ZERO)),
                            }
                        };
                        let from = resolve_endpoint(from);
                        let to = resolve_endpoint(to);
                        let offset = *offset;
                        let redraw = gaanim_animation::AlwaysRedrawRegen::new(move |world| {
                            let endpoint_position = |endpoint: &TrackingEndpoint| match endpoint {
                                TrackingEndpoint::Static(position) => *position,
                                TrackingEndpoint::Entity(entity) => world
                                    .get::<SpatialTransform>(*entity)
                                    .map(|transform| transform.translation)
                                    .unwrap_or(DVec3::ZERO),
                            };
                            let from = endpoint_position(&from);
                            let to = endpoint_position(&to);
                            gaanim_objects::primitives::dimension_path(
                                Point::new(from.x, from.y),
                                Point::new(to.x, to.y),
                                offset,
                            )
                        });
                        builder.commands.entity(st.entity).insert(redraw);
                    }
                }

                Op::AttachTrackerArc {
                    target,
                    tracker,
                    center,
                    radius,
                    start_angle,
                    sweep_scale,
                    sweep_offset,
                } => {
                    if let Some(target_id) = id_map.get(target).copied()
                        && let Some(tracker_id) = id_map.get(tracker).copied()
                        && let Some(target_st) = builder.states.get(target_id)
                        && let Some(tracker_st) = builder.states.get(tracker_id)
                    {
                        let tracker_entity = tracker_st.entity;
                        let center = Point::new(center.0, center.1);
                        let radius = *radius;
                        let start_angle = *start_angle;
                        let sweep_scale = *sweep_scale;
                        let sweep_offset = *sweep_offset;
                        let redraw = gaanim_animation::AlwaysRedrawRegen::new(move |world| {
                            let value = world
                                .get::<gaanim_animation::FloatSignal>(tracker_entity)
                                .map(|signal| signal.value)
                                .unwrap_or(0.0);
                            gaanim_objects::primitives::curved_arrow_arc(
                                gaanim_core::ObjectId::from_raw(0),
                                center,
                                radius,
                                start_angle,
                                value * sweep_scale + sweep_offset,
                            )
                            .path
                            .0
                            .as_ref()
                            .clone()
                        });
                        builder.commands.entity(target_st.entity).insert(redraw);
                    }
                }
                Op::AttachPointOnCurve {
                    target,
                    curve,
                    tracker,
                } => {
                    if let Some(target_id) = id_map.get(target).copied()
                        && let Some(curve_id) = id_map.get(curve).copied()
                        && let Some(tracker_id) = id_map.get(tracker).copied()
                        && let Some(target_st) = builder.states.get(target_id)
                        && let Some(curve_st) = builder.states.get(curve_id)
                        && let Some(tracker_st) = builder.states.get(tracker_id)
                    {
                        builder
                            .commands
                            .entity(target_st.entity)
                            .insert(PointOnCurve::new(curve_st.entity, tracker_st.entity));
                    }
                }
                Op::AttachTangentOnCurve {
                    target,
                    curve,
                    tracker,
                } => {
                    if let Some(target_id) = id_map.get(target).copied()
                        && let Some(curve_id) = id_map.get(curve).copied()
                        && let Some(tracker_id) = id_map.get(tracker).copied()
                        && let Some(target_st) = builder.states.get(target_id)
                        && let Some(curve_st) = builder.states.get(curve_id)
                        && let Some(tracker_st) = builder.states.get(tracker_id)
                    {
                        builder
                            .commands
                            .entity(target_st.entity)
                            .insert(TangentOnCurve::new(curve_st.entity, tracker_st.entity));
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
            AnimationType::Transform { target } => AnimationType::Transform {
                target: *id_map.get(target)?,
            },
            AnimationType::ReplacementTransform { target } => AnimationType::ReplacementTransform {
                target: *id_map.get(target)?,
            },
            other => other.clone(),
        };
        Some(AnimationBuilder {
            target,
            anim_type,
            duration: anim.duration,
            delay: anim.delay,
            rate_func: anim.rate_func.clone(),
        })
    }

    fn spawn_one(
        builder: &mut SceneBuilder,
        spec: &ObjectSpec,
        id_map: &HashMap<ObjectId, ObjectId>,
        frame_bounds: Bounds3D,
        text_config: &gaanim_text::prelude::TextConfig,
    ) -> MobjectRef {
        match &spec.kind {
            SpawnKind::Circle(r) => {
                let b = builder.circle(*r);
                let mr = Self::finish_spawn_builder(b, spec);
                Self::apply_layout(builder, mr.id, spec, id_map, frame_bounds);
                mr
            }
            SpawnKind::Rect(w, h) => {
                let b = builder.rectangle(*w, *h);
                let mr = Self::finish_spawn_builder(b, spec);
                Self::apply_layout(builder, mr.id, spec, id_map, frame_bounds);
                mr
            }
            SpawnKind::RoundedRect(w, h, r) => {
                let b = builder.rounded_rect(*w, *h, *r);
                let mr = Self::finish_spawn_builder(b, spec);
                Self::apply_layout(builder, mr.id, spec, id_map, frame_bounds);
                mr
            }
            SpawnKind::Square(sz) => {
                let b = builder.square(*sz);
                let mr = Self::finish_spawn_builder(b, spec);
                Self::apply_layout(builder, mr.id, spec, id_map, frame_bounds);
                mr
            }
            SpawnKind::Dot(r) => {
                let b = builder.dot(*r);
                let mr = Self::finish_spawn_builder(b, spec);
                Self::apply_layout(builder, mr.id, spec, id_map, frame_bounds);
                mr
            }
            SpawnKind::Ellipse(rx, ry) => {
                let b = builder.ellipse(*rx, *ry);
                let mr = Self::finish_spawn_builder(b, spec);
                Self::apply_layout(builder, mr.id, spec, id_map, frame_bounds);
                mr
            }
            SpawnKind::Line(x1, y1, x2, y2) => {
                let b = builder.line(Point::new(*x1, *y1), Point::new(*x2, *y2));
                let mr = Self::finish_spawn_builder(b, spec);
                Self::apply_layout(builder, mr.id, spec, id_map, frame_bounds);
                mr
            }
            SpawnKind::Arrow(x1, y1, x2, y2) => {
                let b = builder.arrow(Point::new(*x1, *y1), Point::new(*x2, *y2));
                let mr = Self::finish_spawn_builder(b, spec);
                Self::apply_layout(builder, mr.id, spec, id_map, frame_bounds);
                mr
            }
            SpawnKind::Arc {
                center,
                radius,
                start_angle,
                sweep_angle,
            } => {
                let b = builder.arc(
                    Point::new(center.0, center.1),
                    Vec2::new(*radius, *radius),
                    *start_angle,
                    *sweep_angle,
                    0.0,
                );
                let mr = Self::finish_spawn_builder(b, spec);
                Self::apply_layout(builder, mr.id, spec, id_map, frame_bounds);
                mr
            }
            SpawnKind::CurvedArrow(x1, y1, x2, y2, angle) => {
                let b = builder.curved_arrow(Point::new(*x1, *y1), Point::new(*x2, *y2), *angle);
                let mr = Self::finish_spawn_builder(b, spec);
                Self::apply_layout(builder, mr.id, spec, id_map, frame_bounds);
                mr
            }
            SpawnKind::CurvedArrowArc {
                center,
                radius,
                start_angle,
                sweep_angle,
            } => {
                let b = builder.curved_arrow_arc(
                    Point::new(center.0, center.1),
                    *radius,
                    *start_angle,
                    *sweep_angle,
                );
                let mr = Self::finish_spawn_builder(b, spec);
                Self::apply_layout(builder, mr.id, spec, id_map, frame_bounds);
                mr
            }
            SpawnKind::Dimension { start, end, offset } => {
                let dx = end.0 - start.0;
                let dy = end.1 - start.1;
                let length = dx.hypot(dy);
                if length <= f64::EPSILON {
                    let b = builder.line(Point::new(start.0, start.1), Point::new(end.0, end.1));
                    let mr = Self::finish_spawn_builder(b, spec);
                    Self::apply_layout(builder, mr.id, spec, id_map, frame_bounds);
                    mr
                } else {
                    let normal = (-dy / length, dx / length);
                    let dimension_start =
                        Point::new(start.0 + normal.0 * *offset, start.1 + normal.1 * *offset);
                    let dimension_end =
                        Point::new(end.0 + normal.0 * *offset, end.1 + normal.1 * *offset);
                    let color = PenikoColor::from_rgb8(0x80, 0x80, 0x80);
                    let extension_a = builder
                        .line(Point::new(start.0, start.1), dimension_start)
                        .no_fill()
                        .stroke(color, 2.0)
                        .spawn();
                    let extension_b = builder
                        .line(Point::new(end.0, end.1), dimension_end)
                        .no_fill()
                        .stroke(color, 2.0)
                        .spawn();
                    let measurement = builder
                        .double_arrow(dimension_start, dimension_end, Some(12.0), Some(10.0))
                        .fill(color)
                        .no_stroke()
                        .spawn();
                    let group = builder.group(&[extension_a, extension_b, measurement]);
                    Self::post_apply(builder, group.id, spec, id_map, frame_bounds);
                    group
                }
            }
            SpawnKind::Polyline(points) => {
                let points: Vec<Point> = points.iter().map(|&(x, y)| Point::new(x, y)).collect();
                let b = builder.open_path(&points);
                let mr = Self::finish_spawn_builder(b, spec);
                Self::apply_layout(builder, mr.id, spec, id_map, frame_bounds);
                mr
            }
            SpawnKind::Axes {
                x_range,
                y_range,
                config,
            } => {
                let axes = builder.axes(*x_range, *y_range, config.numbers, config.ticks);
                if let Some(axis_state) = builder.states.get_mut(axes.id) {
                    let stroke = StrokeBrush::new(config.axis_color, config.axis_width);
                    axis_state.stroke = stroke.clone();
                    builder.commands.entity(axis_state.entity).insert(stroke);
                }
                if config.grid {
                    let grid = Self::finish_spawn_builder(
                        builder
                            .number_plane(*x_range, *y_range, config.axis_width, config.grid_width)
                            .stroke(config.grid_color, config.grid_width),
                        spec,
                    );
                    let group = builder.group(&[grid, axes]);
                    Self::post_apply(builder, group.id, spec, id_map, frame_bounds);
                    group
                } else {
                    Self::post_apply(builder, axes.id, spec, id_map, frame_bounds);
                    axes
                }
            }
            SpawnKind::Text(t) => {
                let style = &text_config.roles[&gaanim_text::prelude::TextRole::Body];
                let mr = builder.text(t, &style.font_family, style.size);
                Self::post_apply(builder, mr.id, spec, id_map, frame_bounds);
                mr
            }
            SpawnKind::Title(t) => {
                let style = &text_config.roles[&gaanim_text::prelude::TextRole::Title];
                let mr = builder.text(t, &style.font_family, style.size);
                Self::post_apply(builder, mr.id, spec, id_map, frame_bounds);
                mr
            }
            SpawnKind::Subtitle(t) => {
                let style = &text_config.roles[&gaanim_text::prelude::TextRole::Subtitle];
                let mr = builder.text(t, &style.font_family, style.size);
                Self::post_apply(builder, mr.id, spec, id_map, frame_bounds);
                mr
            }
            SpawnKind::Equation(f) => {
                let mr = builder.equation(f);
                Self::post_apply(builder, mr.id, spec, id_map, frame_bounds);
                mr
            }
            SpawnKind::Image { image, view } => {
                let b = builder.image(image.clone(), *view);
                let mr = Self::finish_spawn_builder(b, spec);
                Self::apply_layout(builder, mr.id, spec, id_map, frame_bounds);
                mr
            }
            SpawnKind::Svg(document) => {
                let mr = builder.svg_group(&document.paths);
                Self::post_apply(builder, mr.id, spec, id_map, frame_bounds);
                mr
            }
            SpawnKind::Group(ids) => {
                let refs: Vec<MobjectRef> = ids
                    .iter()
                    .filter_map(|id| id_map.get(id).copied().map(|id| MobjectRef { id }))
                    .collect();
                let mr = builder.group(&refs);
                Self::post_apply(builder, mr.id, spec, id_map, frame_bounds);
                mr
            }
            SpawnKind::ValueTracker(initial) => {
                // Spawn a FloatSignal entity (no visual output).
                let new_id = builder.next_id();
                let entity = builder
                    .commands
                    .spawn((
                        gaanim_scene::MobjectId(new_id),
                        gaanim_animation::FloatSignal::new(*initial),
                    ))
                    .id();
                builder.tag_entity(entity);
                builder.states.insert(
                    new_id,
                    MobjectState {
                        path: std::sync::Arc::new(gaanim_core::kurbo::BezPath::new()),
                        bounds: Bounds3D::default(),
                        transform: SpatialTransform::default(),
                        opacity: 1.0,
                        fill: None,
                        stroke: StrokeBrush::default(),
                        entity,
                        child_spans: Vec::new(),
                        children: Vec::new(),
                        parent: None,
                    },
                );
                builder.float_signals.insert(new_id, *initial);
                MobjectRef { id: new_id }
            }
            SpawnKind::TracedPathLine => {
                // Spawn a minimal line (0,0)→(0,0). TracedPath will overwrite its Path2D.
                let b = builder.line(Point::new(0.0, 0.0), Point::new(0.0, 0.0));
                let mr = Self::finish_spawn_builder(b, spec);
                mr
            }
            SpawnKind::TrackingLine => {
                // Spawn a minimal line (0,0)→(0,0). TrackingLine will overwrite its Path2D.
                let b = builder.line(Point::new(0.0, 0.0), Point::new(0.0, 0.0));
                let mr = Self::finish_spawn_builder(b, spec);
                mr
            }
        }
    }

    fn finish_spawn_builder<'b, 'w, 's, 'a>(
        mut b: crate::builder::MobjectSpawnBuilder<'b, 'w, 's, 'a>,
        spec: &ObjectSpec,
    ) -> MobjectRef {
        if spec.stroke_overridden {
            if let Some((c, w)) = spec.stroke {
                b = b.stroke(c, w);
            } else {
                b = b.no_stroke();
            }
        }
        if spec.fill_overridden {
            if let Some(ref f) = spec.fill {
                b = b.fill_brush(f.clone());
            } else {
                b = b.no_fill();
            }
        }
        b = b.opacity(spec.opacity).z_index(spec.z_index);
        b.spawn()
    }

    fn post_apply(
        builder: &mut SceneBuilder,
        id: ObjectId,
        spec: &ObjectSpec,
        id_map: &HashMap<ObjectId, ObjectId>,
        frame_bounds: Bounds3D,
    ) {
        let mut child_spans = Vec::new();
        if let Some(st) = builder.states.get_mut(id) {
            child_spans = st.child_spans.clone();
            let is_textual_hierarchy = !child_spans.is_empty();
            if spec.stroke_overridden {
                if let Some((c, w)) = spec.stroke {
                    let sb = StrokeBrush::new(c, w);
                    st.stroke = sb.clone();
                    if !is_textual_hierarchy {
                        builder.commands.entity(st.entity).insert(sb);
                    }
                } else {
                    st.stroke = StrokeBrush::transparent();
                    if !is_textual_hierarchy {
                        builder
                            .commands
                            .entity(st.entity)
                            .insert(StrokeBrush::transparent());
                    }
                }
            }
            if spec.fill_overridden {
                if let Some(ref f) = spec.fill {
                    st.fill = Some(f.clone());
                    if !is_textual_hierarchy {
                        builder
                            .commands
                            .entity(st.entity)
                            .insert(FillBrush(Some(f.clone())));
                    }
                } else {
                    st.fill = None;
                    if !is_textual_hierarchy {
                        builder
                            .commands
                            .entity(st.entity)
                            .insert(FillBrush::transparent());
                    }
                }
            }
            if spec.opacity != 1.0 {
                st.opacity = spec.opacity;
                builder
                    .commands
                    .entity(st.entity)
                    .insert(Opacity(spec.opacity));
            }
            if spec.z_index != 0 {
                builder.commands.entity(st.entity).insert(RenderOrder {
                    z_index: spec.z_index,
                    ..Default::default()
                });
            }
        }
        if spec.fill_overridden {
            for child in &child_spans {
                if let Some(child_state) = builder.states.get_mut(child.id) {
                    child_state.fill = spec.fill.clone();
                }
                builder
                    .commands
                    .entity(child.entity)
                    .insert(if let Some(ref f) = spec.fill {
                        FillBrush(Some(f.clone()))
                    } else {
                        FillBrush::transparent()
                    });
            }
        }
        if spec.opacity != 1.0 {
            for child in &child_spans {
                if let Some(child_state) = builder.states.get_mut(child.id) {
                    child_state.opacity = spec.opacity;
                }
                builder
                    .commands
                    .entity(child.entity)
                    .insert(Opacity(spec.opacity));
            }
        }
        if spec.stroke_overridden {
            for child in &child_spans {
                let sb = if let Some((c, w)) = spec.stroke {
                    StrokeBrush::new(c, w)
                } else {
                    StrokeBrush::transparent()
                };
                if let Some(child_state) = builder.states.get_mut(child.id) {
                    child_state.stroke = sb.clone();
                }
                builder.commands.entity(child.entity).insert(sb);
            }
        }
        Self::apply_layout(builder, id, spec, id_map, frame_bounds);
    }

    fn apply_layout(
        builder: &mut SceneBuilder,
        id: ObjectId,
        spec: &ObjectSpec,
        id_map: &HashMap<ObjectId, ObjectId>,
        frame_bounds: Bounds3D,
    ) {
        if spec.layout_ops.is_empty() {
            return;
        }

        let Some(state) = builder.states.get(id) else {
            return;
        };
        let bounds = state.bounds;
        let original_transform = state.transform;
        let entity = state.entity;
        let mut transform = original_transform;
        let mut pivot_in_scene = None;

        for op in &spec.layout_ops {
            match op {
                LayoutOp::SetTranslation(translation) => {
                    transform.translation = *translation;
                }
                LayoutOp::SetScale(factor) => {
                    transform.scale = original_transform.scale * *factor;
                }
                LayoutOp::SetRotation(radians) => {
                    transform.rotation = gaanim_core::glam::DQuat::from_rotation_z(*radians);
                }
                LayoutOp::SetPivot(pivot) => {
                    pivot_in_scene = Some(*pivot);
                }
                LayoutOp::MoveAnchorTo { target, anchor } => {
                    transform =
                        gaanim_layout::compute_move_to(bounds, &transform, *target, *anchor);
                }
                LayoutOp::NextTo {
                    reference,
                    direction,
                    spacing,
                    aligned_edge,
                } => {
                    let Some(reference_id) = id_map.get(reference).copied() else {
                        bevy::prelude::warn!(
                            "Canvas layout skipped: reference object {:?} was not spawned before {:?}",
                            reference,
                            spec.id
                        );
                        continue;
                    };
                    let Some(reference_state) = builder.states.get(reference_id) else {
                        bevy::prelude::warn!(
                            "Canvas layout skipped: missing state for reference object {:?}",
                            reference_id
                        );
                        continue;
                    };
                    let reference_transform = builder.get_world_transform(reference_id);
                    let shift = gaanim_layout::compute_next_to_new(
                        bounds,
                        &transform,
                        reference_state.bounds,
                        &reference_transform,
                        *direction,
                        *spacing,
                        *aligned_edge,
                    );
                    transform = transform.shift_3d(shift);
                }
                LayoutOp::AlignTo {
                    reference,
                    target_anchor,
                    reference_anchor,
                } => {
                    let Some(reference_id) = id_map.get(reference).copied() else {
                        bevy::prelude::warn!(
                            "Canvas layout skipped: reference object {:?} was not spawned before {:?}",
                            reference,
                            spec.id
                        );
                        continue;
                    };
                    let Some(reference_state) = builder.states.get(reference_id) else {
                        bevy::prelude::warn!(
                            "Canvas layout skipped: missing state for reference object {:?}",
                            reference_id
                        );
                        continue;
                    };
                    let reference_transform = builder.get_world_transform(reference_id);
                    let shift = gaanim_layout::compute_align_to_new(
                        bounds,
                        &transform,
                        reference_state.bounds,
                        &reference_transform,
                        *target_anchor,
                        *reference_anchor,
                    );
                    transform = transform.shift_3d(shift);
                }
                LayoutOp::ToEdge { direction, buff } => {
                    transform = gaanim_layout::compute_to_edge(
                        bounds,
                        &transform,
                        *direction,
                        *buff,
                        frame_bounds,
                    );
                }
                LayoutOp::ToCorner { corner, buff } => {
                    transform = gaanim_layout::compute_to_corner(
                        bounds,
                        &transform,
                        *corner,
                        *buff,
                        frame_bounds,
                    );
                }
            }
        }

        if let Some(pivot) = pivot_in_scene {
            // SpatialTransform stores anchors in local coordinates, while the
            // public API accepts the stable scene-space point users see.
            transform.anchor = pivot - transform.translation;
        }

        if transform != original_transform {
            if let Some(state) = builder.states.get_mut(id) {
                state.transform = transform;
            }
            builder.commands.entity(entity).insert(transform);
        }
    }
}
