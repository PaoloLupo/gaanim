use bevy::prelude::*;
use std::collections::HashMap;

use gaanim_api::anim::AnimationBuilder;
use gaanim_api::prelude::{LayoutDirection, MobjectRef, MobjectSpawnBuilder, SceneBuilder};
use gaanim_core::peniko;
use gaanim_core::ObjectId;
use gaanim_math::{Bounds3D, Camera};
use gaanim_renderer::prelude::VelloView;
use gaanim_scene::{FillBrush, Opacity, RenderOrder, StrokeBrush};
use gaanim_text::font::FontRegistry;
use gaanim_text::prelude::TextRole;
use gaanim_timeline::timeline::Timeline;

use crate::mobject::{MobjectSpec, PythonGroupLayoutOp, PythonPositioningOp, TextRoleKind};
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

/// Build a Bevy `App` with the editor plugin, replay the deferred op queue,
/// and run the interactive editor window.
pub fn run_editor(
    ops: Vec<DeferredOp>,
    width: u32,
    height: u32,
    title: String,
    background: Option<peniko::Color>,
) {
    let mut app = App::new();
    app.add_plugins(DefaultPlugins.set(WindowPlugin {
        primary_window: Some(Window {
            title: format!(
                "{} [Editor]",
                if title.trim().is_empty() || title == "Gaanim Scene" {
                    "Gaanim".to_string()
                } else {
                    title
                }
            ),
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
    .add_plugins(gaanim_editor::GaanimEditorPlugin);
    let mut ops = Some(ops);
    app.add_systems(Startup, move |world: &mut World| {
        let ops_taken = ops.take().expect("Startup called twice");
        let ops_clone = ops_taken.clone();
        let replay_fn: std::sync::Arc<dyn Fn(&mut World) + Send + Sync> =
            std::sync::Arc::new(move |w: &mut World| {
                replay_into(w, ops_clone.clone(), width, height, background);
            });
        world.insert_resource(gaanim_editor::export::StashedReplay(Some(replay_fn)));
        replay_into(world, ops_taken, width, height, background);
        // Start paused in editor mode so the user can inspect first.
        let mut timeline = world.resource_mut::<Timeline>();
        timeline.is_playing = false;
    });

    app.run();
}

pub(crate) fn replay_into(
    world: &mut World,
    ops: Vec<DeferredOp>,
    width: u32,
    height: u32,
    background: Option<peniko::Color>,
) {
    // Take ownership of the resources SceneBuilder needs, then reinsert after.
    // This avoids Bevy's per-borrow restrictions.
    let mut timeline = match world.remove_resource::<Timeline>() {
        Some(res) => res,
        None => {
            bevy::prelude::error!("Timeline resource missing");
            return;
        }
    };
    let font_registry = match world.remove_resource::<FontRegistry>() {
        Some(res) => res,
        None => {
            bevy::prelude::error!("FontRegistry resource missing");
            return;
        }
    };
    let text_config = match world.remove_resource::<gaanim_text::prelude::TextConfig>() {
        Some(res) => res,
        None => {
            bevy::prelude::error!("TextConfig resource missing");
            return;
        }
    };

    // Scope the mut borrow on `world` so we can reinsert resources at the end.
    let result = {
        let mut commands = world.commands();

        // Spawn the orthographic camera + Vello view BEFORE building the scene,
        // so the renderer has a target to draw to (matches crates/gaanim_api/examples/math_demo.rs).
        commands.insert_resource(Camera::ortho_2d(width, height));
        commands.spawn((Camera2d, VelloView));

        let mut scene =
            SceneBuilder::new(&mut commands, &mut timeline, &font_registry, &text_config);
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

fn run_replay(scene: &mut SceneBuilder<'_, '_, '_>, ops: Vec<DeferredOp>) -> ReplayResult {
    let mut py_to_bevy: HashMap<ObjectId, ObjectId> = HashMap::new();
    let mut selection_map: HashMap<ObjectId, Vec<ObjectId>> = HashMap::new();
    // Track SceneIds by their replay order index for connection processing.
    let mut scene_ids: Vec<gaanim_timeline::clip::SceneId> = Vec::new();

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
                let spec_value = match spec.lock() {
                    Ok(guard) => guard.clone(),
                    Err(_) => {
                        bevy::prelude::error!("Failed to lock MobjectSpec during replay");
                        continue;
                    }
                };
                if let Some(bevy_id) = spawn_mobject(scene, spec_value.clone(), &py_to_bevy) {
                    let z_index = spec_value.z_index();
                    if let Some(state) = scene.states.get(bevy_id) {
                        let order = RenderOrder {
                            z_index,
                            creation_order,
                        };
                        scene.commands.entity(state.entity).insert(order);

                        for (_, child_entity, _) in &state.child_spans {
                            scene.commands.entity(*child_entity).insert(order);
                        }
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
            DeferredOp::Ungroup { group } => {
                if let Some(&bevy_group) = py_to_bevy.get(&group) {
                    scene.ungroup(MobjectRef { id: bevy_group });
                }
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
                        if let Some(state) = scene.states.get_mut(*child_id) {
                            state.fill = Some(peniko::Brush::Solid(color));
                            scene
                                .commands
                                .entity(state.entity)
                                .insert(FillBrush(Some(peniko::Brush::Solid(color))));
                            // If the entity already has a stroke
                            // (typically the auto-stroke synthesized
                            // by a prior `Write` animation), retint
                            // it to match the new fill so the
                            // progressive outline uses the selection
                            // color. The global PathCompletion reset
                            // in `play_write_internal` is what keeps
                            // the outline hidden until the draw phase
                            // starts, so re-inserting a stroke here
                            // does not produce the "visible from
                            // start" regression.
                            if state.stroke.brush.is_some() {
                                let width = state.stroke.style.width;
                                let new_stroke = StrokeBrush::new(color, width);
                                state.stroke = new_stroke.clone();
                                scene.commands.entity(state.entity).insert(new_stroke);
                            }
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
                        if let Some(state) = scene.states.get_mut(*child_id) {
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
            DeferredOp::SceneBegin { ref name } => {
                let id = scene.begin_scene(name);
                scene_ids.push(id);
            }
            DeferredOp::SceneEnd => {
                scene.end_scene();
            }
            DeferredOp::SceneConnect {
                from_index,
                to_index,
                transition,
            } => {
                if from_index < scene_ids.len() && to_index < scene_ids.len() {
                    scene
                        .timeline
                        .connect(scene_ids[from_index], scene_ids[to_index], transition);
                }
            }
            DeferredOp::SpawnValueTracker { id, initial } => {
                let tracker = scene.value_tracker(initial);
                py_to_bevy.insert(id, tracker.id);
            }
            DeferredOp::AddUpdater {
                target,
                updater_type,
                params,
                follow_target,
            } => {
                if let Some(&bevy_target) = py_to_bevy.get(&target) {
                    if let Some(state) = scene.states.get(bevy_target) {
                        let updater = match updater_type.as_str() {
                            "bob" => {
                                let amp = params.get(0).copied().unwrap_or(20.0);
                                let freq = params.get(1).copied().unwrap_or(1.0);
                                gaanim_animation::updaters::bob_updater(amp, freq)
                            }
                            "rotate" => {
                                let speed = params.get(0).copied().unwrap_or(1.0);
                                gaanim_animation::updaters::rotate_updater(speed)
                            }
                            "orbit" => {
                                let cx = params.get(0).copied().unwrap_or(0.0);
                                let cy = params.get(1).copied().unwrap_or(0.0);
                                let radius = params.get(2).copied().unwrap_or(100.0);
                                let speed = params.get(3).copied().unwrap_or(1.0);
                                gaanim_animation::updaters::orbit_updater(
                                    gaanim_core::glam::DVec3::new(cx, cy, 0.0),
                                    radius,
                                    speed,
                                )
                            }
                            "pulse" => {
                                let min = params.get(0).copied().unwrap_or(0.8);
                                let max = params.get(1).copied().unwrap_or(1.2);
                                let freq = params.get(2).copied().unwrap_or(1.0);
                                gaanim_animation::updaters::pulse_updater(min, max, freq)
                            }
                            "follow" => {
                                let ox = params.get(0).copied().unwrap_or(0.0);
                                let oy = params.get(1).copied().unwrap_or(0.0);
                                let smoothing = params.get(2).copied().unwrap_or(0.0);
                                let f_target =
                                    follow_target.and_then(|t| py_to_bevy.get(&t).copied());
                                if let Some(bevy_ft) = f_target {
                                    if let Some(ft_state) = scene.states.get(bevy_ft) {
                                        gaanim_animation::updaters::follow_updater(
                                            ft_state.entity,
                                            gaanim_core::glam::DVec3::new(ox, oy, 0.0),
                                            smoothing,
                                        )
                                    } else {
                                        continue;
                                    }
                                } else {
                                    continue;
                                }
                            }
                            _ => continue,
                        };
                        scene.commands.entity(state.entity).insert(updater);
                    }
                }
            }
            DeferredOp::RemoveUpdater { target } => {
                if let Some(&bevy_target) = py_to_bevy.get(&target) {
                    if let Some(state) = scene.states.get(bevy_target) {
                        scene
                            .commands
                            .entity(state.entity)
                            .remove::<gaanim_animation::Updater>();
                    }
                }
            }
            DeferredOp::SpawnTracedPath {
                id,
                source,
                color,
                width,
                min_distance,
                max_points,
            } => {
                if let Some(&bevy_source) = py_to_bevy.get(&source) {
                    let source_ent = scene.states.get(bevy_source).map(|s| s.entity);
                    if let Some(source_entity) = source_ent {
                        let trace_id = scene.next_id();
                        let bundle = gaanim_objects::primitives::line(
                            trace_id,
                            gaanim_core::kurbo::Point::new(0.0, 0.0),
                            gaanim_core::kurbo::Point::new(0.0, 0.0),
                        );
                        let trace_entity = scene
                            .commands
                            .spawn((
                                bundle,
                                gaanim_animation::TracedPath::new(
                                    source_entity,
                                    min_distance,
                                    max_points,
                                ),
                            ))
                            .id();
                        scene.tag_entity(trace_entity);

                        scene.commands.entity(trace_entity).insert((
                            gaanim_scene::StrokeBrush::new(color, width),
                            gaanim_scene::FillBrush(None),
                        ));

                        let state = gaanim_api::builder::MobjectState {
                            bounds: gaanim_math::Bounds3D::default(),
                            transform: gaanim_math::SpatialTransform::default(),
                            opacity: 1.0,
                            fill: None,
                            stroke: gaanim_scene::StrokeBrush::new(color, width),
                            entity: trace_entity,
                            child_spans: Vec::new(),
                            children: Vec::new(),
                            parent: None,
                        };
                        scene.states.insert(trace_id, state);
                        py_to_bevy.insert(id, trace_id);
                    }
                }
            }
        }
    }

    scene.timeline.cached_duration = scene.timeline.cached_duration.max(scene.current_time);
    let loop_range: Option<(f64, f64)> = Some((0.0, scene.timeline.cached_duration));
    ReplayResult { loop_range }
}

/// Spawn one mobject and return its bevy-side `ObjectId`.
fn spawn_mobject(
    scene: &mut SceneBuilder<'_, '_, '_>,
    spec: MobjectSpec,
    py_to_bevy: &HashMap<ObjectId, ObjectId>,
) -> Option<ObjectId> {
    let positioning_ops = spec.common().positioning_ops.clone();
    let layout_op = if let MobjectSpec::Group { layout_op, .. } = &spec {
        layout_op.clone()
    } else {
        None
    };
    // Pre-resolve the next_to hint: (bevy_ref_id, dir, spacing) if the
    // referenced mobject is already known to the scene. We do this check
    // up front (releasing the borrow before mutating the scene) to avoid
    // closure-capture conflicts with `scene.circle(...)` etc.
    let resolved_next_to: Option<(ObjectId, LayoutDirection, f64)> = match spec.next_to() {
        Some((py_ref_id, dir, spacing)) => py_to_bevy
            .get(&py_ref_id)
            .copied()
            .filter(|bevy_id| scene.states.contains_key(*bevy_id))
            .map(|bevy_id| (bevy_id, dir, spacing)),
        None => None,
    };

    let id = match spec {
        MobjectSpec::Circle { common, radius } => {
            let b = scene.circle(radius);
            let b = apply_visual(b, common.fill, common.stroke);
            let b = b.transform(common.transform).opacity(common.opacity);
            apply_next_to(b, resolved_next_to).spawn().id
        }
        MobjectSpec::Rectangle {
            common,
            width,
            height,
        } => {
            let b = scene.rectangle(width, height);
            let b = apply_visual(b, common.fill, common.stroke);
            let b = b.transform(common.transform).opacity(common.opacity);
            apply_next_to(b, resolved_next_to).spawn().id
        }
        MobjectSpec::RoundedRect {
            common,
            width,
            height,
            radius,
        } => {
            let b = scene.rounded_rect(width, height, radius);
            let b = apply_visual(b, common.fill, common.stroke);
            let b = b.transform(common.transform).opacity(common.opacity);
            apply_next_to(b, resolved_next_to).spawn().id
        }
        MobjectSpec::Line { common, start, end } => {
            let b = scene.line(
                gaanim_core::kurbo::Point::new(start.0, start.1),
                gaanim_core::kurbo::Point::new(end.0, end.1),
            );
            let b = apply_visual(b, common.fill, common.stroke);
            let b = b.transform(common.transform).opacity(common.opacity);
            apply_next_to(b, resolved_next_to).spawn().id
        }
        MobjectSpec::Polygon { common, points } => {
            let pts: Vec<gaanim_core::kurbo::Point> = points
                .iter()
                .map(|(x, y)| gaanim_core::kurbo::Point::new(*x, *y))
                .collect();
            let b = scene.polygon(&pts);
            let b = apply_visual(b, common.fill, common.stroke);
            let b = b.transform(common.transform).opacity(common.opacity);
            apply_next_to(b, resolved_next_to).spawn().id
        }
        MobjectSpec::Star {
            common,
            n_points,
            outer_radius,
            inner_radius,
        } => {
            let b = scene.star(n_points, outer_radius, inner_radius);
            let b = apply_visual(b, common.fill, common.stroke);
            let b = b.transform(common.transform).opacity(common.opacity);
            apply_next_to(b, resolved_next_to).spawn().id
        }
        MobjectSpec::Ellipse { common, rx, ry } => {
            let b = scene.ellipse(rx, ry);
            let b = apply_visual(b, common.fill, common.stroke);
            let b = b.transform(common.transform).opacity(common.opacity);
            apply_next_to(b, resolved_next_to).spawn().id
        }
        MobjectSpec::Dot { common, radius } => {
            let b = scene.dot(radius);
            let b = apply_visual(b, common.fill, common.stroke);
            let b = b.transform(common.transform).opacity(common.opacity);
            apply_next_to(b, resolved_next_to).spawn().id
        }
        MobjectSpec::Square { common, side } => {
            let b = scene.square(side);
            let b = apply_visual(b, common.fill, common.stroke);
            let b = b.transform(common.transform).opacity(common.opacity);
            apply_next_to(b, resolved_next_to).spawn().id
        }
        MobjectSpec::Checkmark { common, size } => {
            let b = scene.checkmark(size);
            let b = apply_visual(b, common.fill, common.stroke);
            let b = b.transform(common.transform).opacity(common.opacity);
            apply_next_to(b, resolved_next_to).spawn().id
        }
        MobjectSpec::Arrow { common, start, end } => {
            let b = scene.arrow(
                gaanim_core::kurbo::Point::new(start.0, start.1),
                gaanim_core::kurbo::Point::new(end.0, end.1),
            );
            let b = apply_visual(b, common.fill, common.stroke);
            let b = b.transform(common.transform).opacity(common.opacity);
            apply_next_to(b, resolved_next_to).spawn().id
        }
        MobjectSpec::RegularPolygon {
            common,
            n_sides,
            radius,
        } => {
            let b = scene.regular_polygon(n_sides, radius);
            let b = apply_visual(b, common.fill, common.stroke);
            let b = b.transform(common.transform).opacity(common.opacity);
            apply_next_to(b, resolved_next_to).spawn().id
        }
        MobjectSpec::DashedLine {
            common,
            start,
            end,
            dash_length,
            gap_length,
        } => {
            let b = scene.dashed_line(
                gaanim_core::kurbo::Point::new(start.0, start.1),
                gaanim_core::kurbo::Point::new(end.0, end.1),
                dash_length,
                gap_length,
            );
            let b = apply_visual(b, common.fill, common.stroke);
            let b = b.transform(common.transform).opacity(common.opacity);
            apply_next_to(b, resolved_next_to).spawn().id
        }
        MobjectSpec::Arc {
            common,
            center,
            rx,
            ry,
            start_angle,
            sweep_angle,
        } => {
            let b = scene.arc(
                gaanim_core::kurbo::Point::new(center.0, center.1),
                gaanim_core::kurbo::Vec2::new(rx, ry),
                start_angle,
                sweep_angle,
                0.0,
            );
            let b = apply_visual(b, common.fill, common.stroke);
            let b = b.transform(common.transform).opacity(common.opacity);
            apply_next_to(b, resolved_next_to).spawn().id
        }
        MobjectSpec::ArcBetweenPoints {
            common,
            start,
            end,
            angle,
        } => {
            let b = scene.arc_between_points(
                gaanim_core::kurbo::Point::new(start.0, start.1),
                gaanim_core::kurbo::Point::new(end.0, end.1),
                angle,
            );
            let b = apply_visual(b, common.fill, common.stroke);
            let b = b.transform(common.transform).opacity(common.opacity);
            apply_next_to(b, resolved_next_to).spawn().id
        }
        MobjectSpec::DoubleArrow {
            common,
            start,
            end,
            head_len,
            head_width,
        } => {
            let b = scene.double_arrow(
                gaanim_core::kurbo::Point::new(start.0, start.1),
                gaanim_core::kurbo::Point::new(end.0, end.1),
                head_len,
                head_width,
            );
            let b = apply_visual(b, common.fill, common.stroke);
            let b = b.transform(common.transform).opacity(common.opacity);
            apply_next_to(b, resolved_next_to).spawn().id
        }
        MobjectSpec::Sector {
            common,
            center,
            radius,
            start_angle,
            sweep_angle,
        } => {
            let b = scene.sector(
                gaanim_core::kurbo::Point::new(center.0, center.1),
                radius,
                start_angle,
                sweep_angle,
            );
            let b = apply_visual(b, common.fill, common.stroke);
            let b = b.transform(common.transform).opacity(common.opacity);
            apply_next_to(b, resolved_next_to).spawn().id
        }
        MobjectSpec::Annulus {
            common,
            outer_radius,
            inner_radius,
        } => {
            let b = scene.annulus(outer_radius, inner_radius);
            let b = apply_visual(b, common.fill, common.stroke);
            let b = b.transform(common.transform).opacity(common.opacity);
            apply_next_to(b, resolved_next_to).spawn().id
        }
        MobjectSpec::SurroundingRectangle {
            common,
            width,
            height,
            corner_radius,
        } => {
            let b = scene.surrounding_rectangle(width, height, corner_radius);
            let b = apply_visual(b, common.fill, common.stroke);
            let b = b.transform(common.transform).opacity(common.opacity);
            apply_next_to(b, resolved_next_to).spawn().id
        }
        MobjectSpec::BackgroundRectangle {
            common,
            width,
            height,
        } => {
            let b = scene.background_rectangle(width, height);
            let b = apply_visual(b, common.fill, common.stroke);
            let b = b.transform(common.transform).opacity(common.opacity);
            apply_next_to(b, resolved_next_to).spawn().id
        }
        MobjectSpec::Cross { common, size } => {
            let b = scene.cross(size);
            let b = apply_visual(b, common.fill, common.stroke);
            let b = b.transform(common.transform).opacity(common.opacity);
            apply_next_to(b, resolved_next_to).spawn().id
        }
        MobjectSpec::RightAngle { common, arm_length } => {
            let b = scene.right_angle(arm_length);
            let b = apply_visual(b, common.fill, common.stroke);
            let b = b.transform(common.transform).opacity(common.opacity);
            apply_next_to(b, resolved_next_to).spawn().id
        }
        MobjectSpec::TangentLine {
            common,
            curve,
            t,
            length,
        } => {
            let pts: Vec<gaanim_core::kurbo::Point> = curve
                .iter()
                .map(|(x, y)| gaanim_core::kurbo::Point::new(*x, *y))
                .collect();
            let b = scene.tangent_line(&pts, t, length);
            let b = apply_visual(b, common.fill, common.stroke);
            let b = b.transform(common.transform).opacity(common.opacity);
            apply_next_to(b, resolved_next_to).spawn().id
        }
        MobjectSpec::NumberPlane {
            common,
            x_range,
            y_range,
            axis_stroke,
            grid_stroke,
        } => {
            let b = scene.number_plane(x_range, y_range, axis_stroke, grid_stroke);
            let b = apply_visual(b, common.fill, common.stroke);
            let b = b.transform(common.transform).opacity(common.opacity);
            apply_next_to(b, resolved_next_to).spawn().id
        }
        MobjectSpec::BooleanResult { common, contours } => {
            let rings: Vec<Vec<gaanim_core::kurbo::Point>> = contours
                .iter()
                .map(|c| {
                    c.iter()
                        .map(|p| gaanim_core::kurbo::Point::new(p[0], p[1]))
                        .collect()
                })
                .collect();
            let b = scene.polylines(rings);
            let b = apply_visual(b, common.fill, common.stroke);
            let b = b.transform(common.transform).opacity(common.opacity);
            apply_next_to(b, resolved_next_to).spawn().id
        }
        MobjectSpec::Text {
            common,
            content,
            role,
        } => {
            let tr = to_text_role(role);
            let mut mref = scene.spawn_text(&content, tr);
            if let Some(c) = common.fill {
                mref = paint_fill(scene, mref, c);
            }
            mref = apply_2d_transform(scene, mref, common.transform);
            mref = apply_opacity(scene, mref, common.opacity);
            mref.id
        }
        MobjectSpec::DecimalNumber {
            common,
            signal_id,
            num_decimals,
            prefix,
            suffix,
            font_family,
            font_size,
        } => {
            let resolved_id = py_to_bevy.get(&signal_id).copied().unwrap_or(signal_id);
            let tracker_ref = gaanim_api::anim::ValueTrackerRef { id: resolved_id };
            let mut mref = scene.decimal_number(
                tracker_ref,
                num_decimals,
                &prefix,
                &suffix,
                &font_family,
                font_size,
            );
            if let Some(c) = common.fill {
                mref = paint_fill(scene, mref, c);
            }
            mref = apply_2d_transform(scene, mref, common.transform);
            mref = apply_opacity(scene, mref, common.opacity);
            mref.id
        }
        MobjectSpec::Equation { common, formula } => {
            let mut mref = scene.equation(&formula);
            if let Some(c) = common.fill {
                mref = paint_fill(scene, mref, c);
            }
            mref = apply_2d_transform(scene, mref, common.transform);
            mref = apply_opacity(scene, mref, common.opacity);
            mref.id
        }
        MobjectSpec::Group {
            common,
            children,
            layout_op: _,
        } => {
            // Translate python-side child IDs to Bevy-side ObjectIds
            let bevy_children: Vec<MobjectRef> = children
                .iter()
                .filter_map(|(py_id, _, _)| {
                    py_to_bevy.get(py_id).map(|&bid| MobjectRef { id: bid })
                })
                .collect();

            // Spawn the group natively in SceneBuilder
            let group_ref = scene.group(&bevy_children);

            if let Some(op) = &layout_op {
                match op {
                    PythonGroupLayoutOp::Arrange { direction, spacing } => {
                        scene.arrange(group_ref, *direction, *spacing);
                    }
                    PythonGroupLayoutOp::ArrangeInGrid {
                        rows,
                        cols,
                        h_spacing,
                        v_spacing,
                    } => {
                        scene.arrange_in_grid(group_ref, *rows, *cols, *h_spacing, *v_spacing);
                    }
                    PythonGroupLayoutOp::VStack { spacing } => {
                        scene.arrange(group_ref, gaanim_layout::Direction::Down, *spacing);
                    }
                    PythonGroupLayoutOp::HStack { spacing } => {
                        scene.arrange(group_ref, gaanim_layout::Direction::Right, *spacing);
                    }
                }
            }

            // Apply group-level visual styling/transform components:
            if let Some(state) = scene.states.get_mut(group_ref.id) {
                state.transform = common.transform;
                state.opacity = common.opacity;
                state.fill = common.fill.map(peniko::Brush::Solid);
                if let Some(stroke) = common.stroke {
                    state.stroke = StrokeBrush::new(stroke.0, stroke.1);
                }

                scene
                    .commands
                    .entity(state.entity)
                    .insert(common.transform)
                    .insert(Opacity(common.opacity))
                    .insert(FillBrush(common.fill.map(peniko::Brush::Solid)));

                if let Some(stroke) = common.stroke {
                    scene
                        .commands
                        .entity(state.entity)
                        .insert(StrokeBrush::new(stroke.0, stroke.1));
                }
            }

            // Resolve next_to positioning hint
            if let Some((bevy_ref_id, dir, spacing)) = resolved_next_to {
                if let Some(ref_state) = scene.states.get(bevy_ref_id) {
                    let ref_bounds = ref_state.bounds;
                    let ref_transform = ref_state.transform;

                    if let Some(group_state) = scene.states.get_mut(group_ref.id) {
                        let shift = gaanim_layout::compute_next_to(
                            group_state.bounds,
                            &group_state.transform,
                            ref_bounds,
                            &ref_transform,
                            dir,
                            spacing,
                        );
                        let new_transform = group_state.transform.shift_3d(shift);
                        group_state.transform = new_transform;
                        scene
                            .commands
                            .entity(group_state.entity)
                            .insert(new_transform);
                    }
                }
            }

            group_ref.id
        }
    };

    // Replay new positioning operations!
    for op in positioning_ops {
        if let Some(state) = scene.states.get(id) {
            let mut new_transform = state.transform;
            match op {
                PythonPositioningOp::At { target, anchor } => {
                    new_transform = gaanim_layout::compute_move_to(
                        state.bounds,
                        &new_transform,
                        target,
                        anchor,
                    );
                }
                PythonPositioningOp::ToEdge { direction, buff } => {
                    let frame_bounds = Bounds3D::new(
                        gaanim_core::glam::DVec3::new(-640.0, -360.0, 0.0),
                        gaanim_core::glam::DVec3::new(640.0, 360.0, 0.0),
                    );
                    new_transform = gaanim_layout::compute_to_edge(
                        state.bounds,
                        &new_transform,
                        direction,
                        buff,
                        frame_bounds,
                    );
                }
                PythonPositioningOp::ToCorner { corner, buff } => {
                    let frame_bounds = Bounds3D::new(
                        gaanim_core::glam::DVec3::new(-640.0, -360.0, 0.0),
                        gaanim_core::glam::DVec3::new(640.0, 360.0, 0.0),
                    );
                    new_transform = gaanim_layout::compute_to_corner(
                        state.bounds,
                        &new_transform,
                        corner,
                        buff,
                        frame_bounds,
                    );
                }
                PythonPositioningOp::AlignTo {
                    reference,
                    target_anchor,
                    ref_anchor,
                } => {
                    if let Some(bevy_ref_id) = py_to_bevy.get(&reference).copied() {
                        if let Some(ref_state) = scene.states.get(bevy_ref_id) {
                            let shift = gaanim_layout::compute_align_to_new(
                                state.bounds,
                                &new_transform,
                                ref_state.bounds,
                                &ref_state.transform,
                                target_anchor,
                                ref_anchor,
                            );
                            new_transform = new_transform.shift_3d(shift);
                        }
                    }
                }
                PythonPositioningOp::NextTo {
                    reference,
                    direction,
                    spacing,
                    aligned_edge,
                } => {
                    if let Some(bevy_ref_id) = py_to_bevy.get(&reference).copied() {
                        if let Some(ref_state) = scene.states.get(bevy_ref_id) {
                            let shift = gaanim_layout::compute_next_to_new(
                                state.bounds,
                                &new_transform,
                                ref_state.bounds,
                                &ref_state.transform,
                                direction,
                                spacing,
                                aligned_edge,
                            );
                            new_transform = new_transform.shift_3d(shift);
                        }
                    }
                }
            }
            if let Some(state_mut) = scene.states.get_mut(id) {
                state_mut.transform = new_transform;
                scene
                    .commands
                    .entity(state_mut.entity)
                    .insert(new_transform);
            }
        }
    }

    Some(id)
}

fn apply_visual<'b, 'w, 's, 'a>(
    mut b: MobjectSpawnBuilder<'b, 'w, 's, 'a>,
    fill: Option<peniko::Color>,
    stroke: Option<(peniko::Color, f64)>,
) -> MobjectSpawnBuilder<'b, 'w, 's, 'a> {
    if let Some(c) = fill {
        b = b.fill(c);
    } else {
        b = b.no_fill();
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
    if let Some(state) = scene.states.get_mut(mref.id) {
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
    if let Some(state) = scene.states.get_mut(mref.id) {
        state.opacity = opacity;
        scene.commands.entity(state.entity).insert(Opacity(opacity));
    }
    mref
}

fn apply_2d_transform(
    scene: &mut SceneBuilder<'_, '_, '_>,
    mref: MobjectRef,
    transform: gaanim_math::SpatialTransform,
) -> MobjectRef {
    if let Some(state) = scene.states.get_mut(mref.id) {
        state.transform = transform;
        scene.commands.entity(state.entity).insert(transform);
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
