use bevy::prelude::*;
use std::collections::HashMap;

use gaanim_api::anim::AnimationBuilder;
use gaanim_api::prelude::{LayoutDirection, MobjectRef, MobjectSpawnBuilder, SceneBuilder};
use gaanim_core::peniko;
use gaanim_core::ObjectId;
use gaanim_math::Camera;
use gaanim_renderer::prelude::VelloView;
use gaanim_scene::{FillBrush, Opacity, RenderOrder, StrokeBrush};
use gaanim_text::font::FontRegistry;
use gaanim_text::prelude::TextRole;
use gaanim_timeline::timeline::Timeline;

use crate::mobject::{MobjectSpec, TextRoleKind};
use crate::scene::DeferredOp;

/// Build a Bevy `App`, replay the deferred op queue, and run the window.
pub fn run(
    ops: Vec<DeferredOp>,
    width: u32,
    height: u32,
    title: String,
    background: Option<peniko::Color>,
) {
    let mut app = App::new();
    app.add_plugins(DefaultPlugins.set(WindowPlugin {
        primary_window: Some(Window {
            title: if title.trim().is_empty() || title == "Gaanim Scene" {
                "Gaanim".to_string()
            } else {
                title
            },
            resolution: (width, height).into(),
            ..default()
        }),
        ..default()
    }))
    .add_plugins(gaanim_scene::GaanimScenePlugin)
    .add_plugins(gaanim_animation::GaanimAnimationPlugin)
    .add_plugins(gaanim_timeline::GaanimTimelinePlugin)
    .add_plugins(gaanim_text::GaanimTextPlugin)
    .add_plugins(gaanim_api::GaanimApiPlugin)
    .add_plugins(gaanim_renderer::GaanimRendererPlugin)
    .add_systems(Startup, move |world: &mut World| {
        replay_into(world, ops.clone(), width, height, background);
    })
    .add_systems(Update, drive_timeline_clock);

    app.run();
}

fn replay_into(
    world: &mut World,
    ops: Vec<DeferredOp>,
    width: u32,
    height: u32,
    background: Option<peniko::Color>,
) {
    // Take ownership of the resources SceneBuilder needs, then reinsert after.
    // This avoids Bevy's per-borrow restrictions.
    let mut timeline = world.remove_resource::<Timeline>().expect("Timeline missing");
    let font_registry = world.remove_resource::<FontRegistry>().expect("FontRegistry missing");
    let text_config = world
        .remove_resource::<gaanim_text::prelude::TextConfig>()
        .expect("TextConfig missing");

    // Scope the mut borrow on `world` so we can reinsert resources at the end.
    let result = {
        let mut commands = world.commands();

        // Spawn the orthographic camera + Vello view BEFORE building the scene,
        // so the renderer has a target to draw to (matches crates/gaanim_api/examples/math_demo.rs).
        commands.insert_resource(Camera::ortho_2d(width, height));
        commands.spawn((Camera2d, VelloView));

        let mut scene = SceneBuilder::new(
            &mut commands,
            &mut timeline,
            &font_registry,
            &text_config,
        );
        run_replay(&mut scene, ops)
    };

    // Reinsert resources.
    world.insert_resource(timeline);
    world.insert_resource(font_registry);
    world.insert_resource(text_config);

    if let Some(bg) = background {
        let rgba = bg.to_rgba8();
        let srgb = bevy::color::Color::srgb_u8(rgba.r, rgba.g, rgba.b);
        world.insert_resource(ClearColor(srgb));
    }

    if let Some((lo, hi)) = result.loop_range {
        let mut t = world.resource_mut::<Timeline>();
        t.loop_range = Some((lo, hi));
    }
}

struct ReplayResult {
    loop_range: Option<(f64, f64)>,
}

fn run_replay(
    scene: &mut SceneBuilder<'_, '_, '_>,
    ops: Vec<DeferredOp>,
) -> ReplayResult {
    let mut py_to_bevy: HashMap<ObjectId, ObjectId> = HashMap::new();
    let mut selection_map: HashMap<ObjectId, Vec<ObjectId>> = HashMap::new();
    let loop_range: Option<(f64, f64)>;

    for op in ops {
        match op {
            DeferredOp::Spawn {
                id,
                spec,
                creation_order,
            } => {
                // Lock and clone the spec at replay time so we see the
                // final state after all chain mutations (.fill().z_index()…)
                // have been applied.
                let spec_value = spec.lock().unwrap().clone();
                if let Some(bevy_id) = spawn_mobject(scene, spec_value, &py_to_bevy) {
                    if let Some(state) = scene.states.get(&bevy_id) {
                        scene
                            .commands
                            .entity(state.entity)
                            .insert(RenderOrder {
                                z_index: 0,
                                creation_order,
                            });
                    }
                    py_to_bevy.insert(id, bevy_id);
                }
            }
            DeferredOp::Play { specs } => {
                let mut builders: Vec<AnimationBuilder> = Vec::with_capacity(specs.len());
                for mut s in specs {
                    if let Some(&bevy_id) = py_to_bevy.get(&s.target) {
                        s.target = bevy_id;
                        builders.push(s);
                    }
                }
                if !builders.is_empty() {
                    scene.play_parallel(builders);
                }
            }
            DeferredOp::Wait { duration } => {
                scene.wait(duration);
            }
            DeferredOp::Select {
                parent,
                query,
                selection,
            } => {
                if let Some(&bevy_parent) = py_to_bevy.get(&parent) {
                    let sel = scene.select(MobjectRef { id: bevy_parent }, &query);
                    selection_map.insert(selection, sel.child_ids.clone());
                }
            }
            DeferredOp::SelectionFill { selection, color } => {
                if let Some(child_ids) = selection_map.get(&selection) {
                    for child_id in child_ids {
                        if let Some(state) = scene.states.get_mut(child_id) {
                            state.fill = Some(peniko::Brush::Solid(color));
                            scene
                                .commands
                                .entity(state.entity)
                                .insert(FillBrush(Some(peniko::Brush::Solid(color))));
                        }
                    }
                }
            }
            DeferredOp::SelectionStroke {
                selection,
                color,
                width,
            } => {
                if let Some(child_ids) = selection_map.get(&selection) {
                    for child_id in child_ids {
                        if let Some(state) = scene.states.get_mut(child_id) {
                            state.stroke = StrokeBrush::new(color, width);
                            scene
                                .commands
                                .entity(state.entity)
                                .insert(StrokeBrush::new(color, width));
                        }
                    }
                }
            }
            DeferredOp::SelectionShift {
                selection,
                dx,
                dy,
                duration,
                rate_func,
            } => {
                if let Some(child_ids) = selection_map.get(&selection).cloned() {
                    let builders: Vec<AnimationBuilder> = child_ids
                        .iter()
                        .map(|cid| {
                            MobjectRef { id: *cid }
                                .shift_2d(dx, dy)
                                .duration(duration)
                                .rate_func(rate_func.clone())
                        })
                        .collect();
                    if !builders.is_empty() {
                        scene.play_parallel(builders);
                    }
                }
            }
        }
    }

    loop_range = Some((0.0, scene.timeline.cached_duration + 0.5));
    ReplayResult { loop_range }
}

/// Spawn one mobject and return its bevy-side `ObjectId`.
fn spawn_mobject(
    scene: &mut SceneBuilder<'_, '_, '_>,
    spec: MobjectSpec,
    py_to_bevy: &HashMap<ObjectId, ObjectId>,
) -> Option<ObjectId> {
    // Pre-resolve the next_to hint: (bevy_ref_id, dir, spacing) if the
    // referenced mobject is already known to the scene. We do this check
    // up front (releasing the borrow before mutating the scene) to avoid
    // closure-capture conflicts with `scene.circle(...)` etc.
    let resolved_next_to: Option<(ObjectId, LayoutDirection, f64)> = match spec.next_to() {
        Some((py_ref_id, dir, spacing)) => py_to_bevy
            .get(&py_ref_id)
            .copied()
            .filter(|bevy_id| scene.states.contains_key(bevy_id))
            .map(|bevy_id| (bevy_id, dir, spacing)),
        None => None,
    };

    let id = match spec {
        MobjectSpec::Circle {
            radius,
            fill,
            stroke,
            opacity,
            transform,
            ..
        } => {
            let b = scene.circle(radius);
            let b = apply_visual(b, fill, stroke);
            let b = b.transform(transform).opacity(opacity);
            apply_next_to(b, resolved_next_to).spawn().id
        }
        MobjectSpec::Rectangle {
            width,
            height,
            fill,
            stroke,
            opacity,
            transform,
            ..
        } => {
            let b = scene.rectangle(width, height);
            let b = apply_visual(b, fill, stroke);
            let b = b.transform(transform).opacity(opacity);
            apply_next_to(b, resolved_next_to).spawn().id
        }
        MobjectSpec::RoundedRect {
            width,
            height,
            radius,
            fill,
            stroke,
            opacity,
            transform,
            ..
        } => {
            let b = scene.rounded_rect(width, height, radius);
            let b = apply_visual(b, fill, stroke);
            let b = b.transform(transform).opacity(opacity);
            apply_next_to(b, resolved_next_to).spawn().id
        }
        MobjectSpec::Line {
            start,
            end,
            stroke,
            opacity,
            transform,
            ..
        } => {
            let b = scene.line(
                gaanim_core::kurbo::Point::new(start.0, start.1),
                gaanim_core::kurbo::Point::new(end.0, end.1),
            );
            let b = apply_visual(b, None, stroke);
            let b = b.transform(transform).opacity(opacity);
            apply_next_to(b, resolved_next_to).spawn().id
        }
        MobjectSpec::Polygon {
            points,
            fill,
            stroke,
            opacity,
            transform,
            ..
        } => {
            let pts: Vec<gaanim_core::kurbo::Point> = points
                .iter()
                .map(|(x, y)| gaanim_core::kurbo::Point::new(*x, *y))
                .collect();
            let b = scene.polygon(&pts);
            let b = apply_visual(b, fill, stroke);
            let b = b.transform(transform).opacity(opacity);
            apply_next_to(b, resolved_next_to).spawn().id
        }
        MobjectSpec::Star {
            n_points,
            outer_radius,
            inner_radius,
            fill,
            stroke,
            opacity,
            transform,
            ..
        } => {
            let b = scene.star(n_points, outer_radius, inner_radius);
            let b = apply_visual(b, fill, stroke);
            let b = b.transform(transform).opacity(opacity);
            apply_next_to(b, resolved_next_to).spawn().id
        }
        MobjectSpec::Ellipse {
            rx,
            ry,
            fill,
            stroke,
            opacity,
            transform,
            ..
        } => {
            let b = scene.ellipse(rx, ry);
            let b = apply_visual(b, fill, stroke);
            let b = b.transform(transform).opacity(opacity);
            apply_next_to(b, resolved_next_to).spawn().id
        }
        MobjectSpec::Dot {
            radius,
            fill,
            stroke,
            opacity,
            transform,
            ..
        } => {
            let b = scene.dot(radius);
            let b = apply_visual(b, fill, stroke);
            let b = b.transform(transform).opacity(opacity);
            apply_next_to(b, resolved_next_to).spawn().id
        }
        MobjectSpec::Square {
            side,
            fill,
            stroke,
            opacity,
            transform,
            ..
        } => {
            let b = scene.square(side);
            let b = apply_visual(b, fill, stroke);
            let b = b.transform(transform).opacity(opacity);
            apply_next_to(b, resolved_next_to).spawn().id
        }
        MobjectSpec::Checkmark {
            size,
            stroke,
            opacity,
            transform,
            ..
        } => {
            let b = scene.checkmark(size);
            let b = apply_visual(b, None, stroke);
            let b = b.transform(transform).opacity(opacity);
            apply_next_to(b, resolved_next_to).spawn().id
        }
        MobjectSpec::Arrow {
            start,
            end,
            stroke,
            fill,
            opacity,
            transform,
            ..
        } => {
            let b = scene.arrow(
                gaanim_core::kurbo::Point::new(start.0, start.1),
                gaanim_core::kurbo::Point::new(end.0, end.1),
            );
            let b = apply_visual(b, fill, stroke);
            let b = b.transform(transform).opacity(opacity);
            apply_next_to(b, resolved_next_to).spawn().id
        }
        MobjectSpec::RegularPolygon {
            n_sides,
            radius,
            fill,
            stroke,
            opacity,
            transform,
            ..
        } => {
            let b = scene.regular_polygon(n_sides, radius);
            let b = apply_visual(b, fill, stroke);
            let b = b.transform(transform).opacity(opacity);
            apply_next_to(b, resolved_next_to).spawn().id
        }
        MobjectSpec::Text {
            content,
            role,
            fill,
            opacity,
            transform,
            ..
        } => {
            let tr = to_text_role(role);
            let mut mref = scene.spawn_text(&content, tr);
            if let Some(c) = fill {
                mref = paint_fill(scene, mref, c);
            }
            mref = apply_2d_transform(scene, mref, transform);
            mref = apply_opacity(scene, mref, opacity);
            mref.id
        }
        MobjectSpec::Equation {
            formula,
            fill,
            opacity,
            transform,
            ..
        } => {
            let mut mref = scene.equation(&formula);
            if let Some(c) = fill {
                mref = paint_fill(scene, mref, c);
            }
            mref = apply_2d_transform(scene, mref, transform);
            mref = apply_opacity(scene, mref, opacity);
            mref.id
        }
    };

    Some(id)
}

fn apply_visual<'b, 'w, 's, 'a>(
    mut b: MobjectSpawnBuilder<'b, 'w, 's, 'a>,
    fill: Option<peniko::Color>,
    stroke: Option<(peniko::Color, f64)>,
) -> MobjectSpawnBuilder<'b, 'w, 's, 'a> {
    if let Some(c) = fill {
        b = b.fill(c);
    }
    if let Some((c, w)) = stroke {
        b = b.stroke(c, w);
    }
    b
}

fn apply_next_to<'b, 'w, 's, 'a>(
    b: MobjectSpawnBuilder<'b, 'w, 's, 'a>,
    hint: Option<(ObjectId, LayoutDirection, f64)>,
) -> MobjectSpawnBuilder<'b, 'w, 's, 'a> {
    if let Some((bevy_id, dir, spacing)) = hint {
        b.next_to(MobjectRef { id: bevy_id }, dir, spacing)
    } else {
        b
    }
}

fn to_text_role(role: TextRoleKind) -> TextRole {
    match role {
        TextRoleKind::Title => TextRole::Title,
        TextRoleKind::Subtitle => TextRole::Subtitle,
        TextRoleKind::Body => TextRole::Body,
        TextRoleKind::Caption => TextRole::Caption,
        TextRoleKind::Code => TextRole::Code,
    }
}

fn paint_fill(
    scene: &mut SceneBuilder<'_, '_, '_>,
    mref: MobjectRef,
    color: peniko::Color,
) -> MobjectRef {
    if let Some(state) = scene.states.get_mut(&mref.id) {
        state.fill = Some(peniko::Brush::Solid(color));
        scene
            .commands
            .entity(state.entity)
            .insert(FillBrush(Some(peniko::Brush::Solid(color))));
    }
    mref
}

fn apply_opacity(
    scene: &mut SceneBuilder<'_, '_, '_>,
    mref: MobjectRef,
    opacity: f32,
) -> MobjectRef {
    if let Some(state) = scene.states.get_mut(&mref.id) {
        state.opacity = opacity;
        scene
            .commands
            .entity(state.entity)
            .insert(Opacity(opacity));
    }
    mref
}

fn apply_2d_transform(
    scene: &mut SceneBuilder<'_, '_, '_>,
    mref: MobjectRef,
    transform: gaanim_math::SpatialTransform,
) -> MobjectRef {
    if let Some(state) = scene.states.get_mut(&mref.id) {
        state.transform = transform;
        scene
            .commands
            .entity(state.entity)
            .insert(transform);
    }
    mref
}

fn drive_timeline_clock(
    mut timeline: ResMut<Timeline>,
    time: Res<Time>,
    mut dt: ResMut<gaanim_animation::DeltaTime>,
) {
    dt.dt = time.delta_secs_f64();
    timeline.is_playing = true;
}

