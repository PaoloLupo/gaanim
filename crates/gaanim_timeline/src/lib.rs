use bevy::prelude::*;

pub mod clip;
pub mod prelude;
pub mod scene;
pub mod snapshot;
pub mod timeline;
pub mod transition;

use gaanim_scene::hierarchy::SceneSet;
use timeline::{PlaybackStopPolicy, Timeline};

/// Marker resource: inserted by `reload_with` to signal that the t=0 keyframe
/// snapshot should be captured on the next frame — after deferred Commands from
/// `replay_into` have been flushed.
#[derive(Resource)]
pub struct NeedsKeyframeCapture;

/// The Bevy plugin that registers the `Timeline` resource and its scheduling systems.
pub struct GaanimTimelinePlugin;

impl Plugin for GaanimTimelinePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Timeline>()
            .init_resource::<PlaybackStopPolicy>()
            .init_resource::<snapshot::CapturedCameraStates>()
            .add_systems(PostStartup, capture_initial_keyframe_system)
            .add_systems(
                Update,
                (
                    timeline_playback_system.in_set(SceneSet::Input),
                    interactive_stop_input_system
                        .in_set(SceneSet::Input)
                        .after(timeline_playback_system),
                    deferred_keyframe_capture_system
                        .in_set(SceneSet::Animation)
                        .before(timeline_seek_system),
                    timeline_seek_system.in_set(SceneSet::Animation),
                    camera_binding_system
                        .in_set(SceneSet::Camera)
                        .before(camera_rig_system),
                    camera_rig_system
                        .in_set(SceneSet::Camera)
                        .before(gaanim_scene::systems::resolve_camera_system),
                ),
            );
    }
}

/// System: PostStartup wrapper that captures the t=0 keyframe once, after all
/// startup commands have run.
pub fn capture_initial_keyframe_system(world: &mut World) {
    capture_initial_keyframe(world);
}

/// System: If [`NeedsKeyframeCapture`] was inserted by a hot-reload, capture
/// the t=0 keyframe *now* so that deferred Commands have been flushed and the
/// snapshot reflects the real entity state.
///
/// Unlike [`capture_initial_keyframe`], this does NOT seek to t=0 — doing so
/// would clobber the playback position that `reload_listener_system` restored.
fn deferred_keyframe_capture_system(world: &mut World) {
    if world.remove_resource::<NeedsKeyframeCapture>().is_some() {
        let timeline = world.remove_resource::<Timeline>();
        if let Some(mut tl) = timeline {
            if !tl.keyframes.contains_key(&ordered_float::OrderedFloat(0.0)) {
                let snapshot = snapshot::WorldSnapshot::capture(world);
                tl.add_keyframe(0.0, snapshot);
            }
            world.insert_resource(tl);
        }
    }
}

/// Captures the current world state at t=0.0 as a keyframe (if not already
/// present), restores scene visibility at t=0, and re-inserts the `Timeline`.
///
/// Re-running the initial-seek is required after a hot-reload replays the
/// scene: the freshly spawned entities need to be registered as the t=0
/// keyframe so that subsequent timeline seeks and play/pause toggles work.
///
/// Factored out of the `PostStartup` system so the host can call it again
/// after rebuilding the world in place.
pub fn capture_initial_keyframe(world: &mut World) {
    let timeline = world.remove_resource::<Timeline>();
    if let Some(mut tl) = timeline {
        if !tl.keyframes.contains_key(&ordered_float::OrderedFloat(0.0)) {
            let snapshot = snapshot::WorldSnapshot::capture(world);
            tl.add_keyframe(0.0, snapshot);
        }
        // Apply scene visibility at t=0 so entities from non-active scenes are hidden
        // before the first render frame.
        tl.seek(world, 0.0);
        world.insert_resource(tl);
    }
}

/// System: Advances virtual timeline playback head time based on frame delta time.
///
/// Ticks progress by setting seek_request every frame, ensuring real-time playback
/// and random-access seek share the exact same evaluation logic.
pub fn timeline_playback_system(
    mut timeline: ResMut<Timeline>,
    stop_policy: Res<PlaybackStopPolicy>,
    dt: Res<gaanim_animation::DeltaTime>,
    playback_state: Option<ResMut<gaanim_animation::PlaybackState>>,
) {
    let scaled_dt = if timeline.is_playing && timeline.seek_request.is_none() {
        dt.dt * timeline.playback_rate
    } else {
        0.0
    };
    if let Some(mut playback_state) = playback_state {
        playback_state.is_playing = timeline.is_playing;
        playback_state.scaled_dt = scaled_dt;
    }
    if timeline.is_playing && timeline.seek_request.is_none() {
        let delta = scaled_dt;
        let next_time = timeline.current_time + delta;

        // Segment boundaries are continuous. Only explicitly authored stops
        // pause real-time playback.
        let current = timeline.current_time;
        let hit_stop = (*stop_policy == PlaybackStopPolicy::Respect)
            .then(|| timeline.next_playback_stop(current, next_time))
            .flatten();

        if let Some(stop) = hit_stop {
            timeline.seek_request = Some(stop);
            timeline.is_playing = false;
        } else if let Some((start, end)) = timeline.loop_range {
            if next_time >= end {
                // Loop back around
                let loop_duration = end - start;
                let excess = next_time - end;
                timeline.seek_request = Some(start + excess % loop_duration);
            } else {
                timeline.seek_request = Some(next_time);
            }
        } else {
            if next_time >= timeline.cached_duration {
                timeline.seek_request = Some(timeline.cached_duration);
                timeline.is_playing = false; // Pause playback at the end
            } else {
                timeline.seek_request = Some(next_time);
            }
        }
    }
}

/// System: Handles user input (keyboard, mouse) to navigate explicit stops.
pub fn interactive_stop_input_system(
    keyboard: Option<Res<ButtonInput<KeyCode>>>,
    mouse: Option<Res<ButtonInput<MouseButton>>>,
    mut timeline: ResMut<Timeline>,
) {
    if timeline
        .segments
        .iter()
        .all(|segment| segment.stops.is_empty())
        || timeline.ignore_input
    {
        return;
    }

    let mut should_advance = false;
    let key_pressed = |key| {
        keyboard
            .as_ref()
            .is_some_and(|keyboard| keyboard.just_pressed(key))
    };
    let mouse_pressed = |button| {
        mouse
            .as_ref()
            .is_some_and(|mouse| mouse.just_pressed(button))
    };

    if key_pressed(KeyCode::Home) {
        timeline.seek_request = Some(0.0);
        timeline.is_playing = false;
        return;
    }
    if key_pressed(KeyCode::End) {
        timeline.seek_request = Some(timeline.cached_duration);
        timeline.is_playing = false;
        return;
    }

    if key_pressed(KeyCode::Space)
        || key_pressed(KeyCode::Enter)
        || key_pressed(KeyCode::ArrowRight)
        || mouse_pressed(MouseButton::Left)
    {
        should_advance = true;
    }

    let mut should_go_back = false;
    if key_pressed(KeyCode::ArrowLeft) || key_pressed(KeyCode::Backspace) {
        should_go_back = true;
    }

    if should_advance {
        if !timeline.is_playing {
            timeline.is_playing = true;
        } else {
            if let Some(stop) = timeline.next_stop(timeline.current_time) {
                timeline.seek_request = Some(stop);
                timeline.is_playing = false;
            } else {
                timeline.seek_request = Some(timeline.cached_duration);
                timeline.is_playing = false;
            }
        }
    } else if should_go_back {
        let target = timeline.previous_stop(timeline.current_time).unwrap_or(0.0);
        timeline.seek_request = Some(target);
        timeline.is_playing = false;
    }
}

/// System: Exclusive system executing pending timeline seek requests.
///
/// Since restoring snapshots requires direct access to all components,
/// this system runs exclusively with `&mut World` access.
pub fn timeline_seek_system(world: &mut World) {
    let seek_time = if let Some(timeline) = world.get_resource::<Timeline>() {
        timeline.seek_request
    } else {
        None
    };

    if let Some(target) = seek_time {
        // Temporarily extract the Timeline resource to satisfy Rust's exclusive access checks
        if let Some(mut timeline) = world.remove_resource::<Timeline>() {
            timeline.seek_request = None;
            timeline.seek(world, target);
            // Re-insert the Timeline resource back into the world
            world.insert_resource(timeline);
        }
    }
}

/// Evaluate persistent native camera bindings against the fully updated world.
pub fn camera_binding_system(world: &mut World) {
    let time = world
        .get_resource::<Timeline>()
        .map_or(0.0, |timeline| timeline.current_time);
    let Some(authored) = world.get_resource::<gaanim_math::Camera>().copied() else {
        return;
    };
    gaanim_animation::apply_camera_bindings(world, time);
    let evaluated = world
        .get_resource::<gaanim_math::Camera>()
        .copied()
        .unwrap_or(authored);
    world.insert_resource(authored);
    world.insert_resource(gaanim_math::CameraRigCamera(evaluated));
}

/// Resolves temporary camera constraints and additive modifiers after reactive
/// updaters/layout, immediately before the presentation camera is copied.
pub fn camera_rig_system(world: &mut World) {
    let Some(timeline) = world.get_resource::<Timeline>() else {
        return;
    };
    let current_time = timeline.current_time;
    let follow = timeline
        .clips
        .values()
        .filter(|clip| clip.start <= current_time && current_time < clip.end())
        .filter_map(|clip| match &clip.payload {
            clip::ClipPayload::Animation(anim) => match &anim.lens {
                clip::PropertyLensSpec::CameraFollow { .. }
                | clip::PropertyLensSpec::CameraFollowEndpoint { .. } => {
                    Some((clip.start, clip.duration, anim.lens.clone()))
                }
                _ => None,
            },
            _ => None,
        })
        .max_by(|(left, ..), (right, ..)| left.total_cmp(right));
    let dynamic_frame = timeline
        .clips
        .values()
        .filter(|clip| clip.start <= current_time && current_time < clip.end())
        .filter_map(|clip| match &clip.payload {
            clip::ClipPayload::Animation(anim) => match &anim.lens {
                clip::PropertyLensSpec::CameraFrameDynamic { .. } => Some((
                    clip.start,
                    clip.duration,
                    anim.rate_func.clone(),
                    anim.lens.clone(),
                )),
                _ => None,
            },
            _ => None,
        })
        .max_by(|(left, ..), (right, ..)| left.total_cmp(right));
    let mut shake_offset = gaanim_core::glam::DVec3::ZERO;
    for clip in timeline
        .clips
        .values()
        .filter(|clip| clip.start <= current_time && current_time < clip.end())
    {
        let clip::ClipPayload::Animation(anim) = &clip.payload else {
            continue;
        };
        let clip::PropertyLensSpec::CameraShake {
            origin: _,
            amplitude,
            frequency,
        } = &anim.lens
        else {
            continue;
        };
        let progress = if clip.duration > 0.0 {
            ((current_time - clip.start) / clip.duration).clamp(0.0, 1.0)
        } else {
            1.0
        };
        let t = anim.rate_func.evaluate(progress);
        let phase = t * *frequency * std::f64::consts::TAU;
        let envelope = (1.0 - t).max(0.0);
        shake_offset += gaanim_core::glam::DVec3::new(
            phase.sin() * *amplitude * envelope,
            (phase * 1.618_033_988_75).sin() * *amplitude * 0.6 * envelope,
            0.0,
        );
    }

    let follow_position = follow.and_then(|(start, _duration, lens)| match lens {
        clip::PropertyLensSpec::CameraFollow { target } => {
            let mut query =
                world.query::<(&gaanim_scene::MobjectId, &gaanim_math::SpatialTransform)>();
            query
                .iter(world)
                .find_map(|(id, transform)| (id.0 == target).then_some(transform.translation))
                .map(|position| (start, position))
        }
        clip::PropertyLensSpec::CameraFollowEndpoint {
            target,
            from,
            offset,
            offset_space,
            lag,
        } => {
            let desired = gaanim_animation::resolve_tracking_endpoint_with_offset(
                &target,
                offset,
                offset_space,
                world,
            )?;
            let influence = if lag <= f64::EPSILON {
                1.0
            } else {
                1.0 - (-(current_time - start).max(0.0) / lag).exp()
            };
            Some((start, from.lerp(desired, influence)))
        }
        _ => None,
    });

    let frame_result = dynamic_frame.and_then(|(start, duration, rate_func, lens)| {
        let clip::PropertyLensSpec::CameraFrameDynamic {
            targets,
            from_position,
            from_zoom,
            margins,
            frame_width,
            frame_height,
        } = lens
        else {
            return None;
        };
        let bounds = targets
            .iter()
            .filter_map(|entity| gaanim_animation::resolve_entity_bounds(*entity, world))
            .reduce(|left, right| left.union(&right))?;
        let [top, right, bottom, left] = margins;
        let framed = gaanim_math::Bounds3D::new_2d(
            bounds.min.x - left,
            bounds.min.y - bottom,
            bounds.max.x + right,
            bounds.max.y + top,
        );
        let desired_zoom =
            (frame_width / framed.width().max(1.0)).min(frame_height / framed.height().max(1.0));
        let progress = if duration > 0.0 {
            ((current_time - start) / duration).clamp(0.0, 1.0)
        } else {
            1.0
        };
        let influence = rate_func.evaluate(progress);
        Some((
            start,
            from_position.lerp(framed.center(), influence),
            from_zoom + (desired_zoom - from_zoom) * influence,
        ))
    });

    let Some(mut camera) = world.get_resource_mut::<gaanim_math::CameraRigCamera>() else {
        return;
    };
    let selected_position = match (follow_position, frame_result) {
        (Some((follow_start, follow)), Some((frame_start, frame, zoom))) => {
            if frame_start >= follow_start {
                camera.0.projection = gaanim_math::Projection::Orthographic { zoom };
                Some(frame)
            } else {
                Some(follow)
            }
        }
        (Some((_, follow)), None) => Some(follow),
        (None, Some((_, frame, zoom))) => {
            camera.0.projection = gaanim_math::Projection::Orthographic { zoom };
            Some(frame)
        }
        (None, None) => None,
    };
    if let Some(position) = selected_position {
        camera.0.position.x = position.x;
        camera.0.position.y = position.y;
        if position.z != 0.0 {
            camera.0.position.z = position.z;
        }
    }
    camera.0.position += shake_offset;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scalar(value: f64) -> gaanim_animation::TrackingScalar {
        gaanim_animation::TrackingScalar {
            source: gaanim_animation::ScalarSource::constant(value),
            parameters: Vec::new(),
        }
    }

    #[test]
    fn shake_is_additive_after_bindings_without_mutating_authored_camera() {
        let mut world = World::new();
        let mut authored = gaanim_math::Camera::ortho_2d(1280, 720);
        authored.position.x = 10.0;
        world.insert_resource(authored);
        world.spawn(gaanim_animation::CameraBinding {
            order: 0,
            kind: gaanim_animation::CameraBindingKind::TwoD {
                center: Some(gaanim_animation::TrackingEndpoint::Static(
                    gaanim_core::glam::DVec3::new(20.0, 0.0, 0.0),
                )),
                zoom: None,
                rotation: None,
            },
            influence: scalar(1.0),
            windows: vec![gaanim_animation::CameraBindingWindow {
                start: 0.0,
                end: None,
            }],
        });
        let mut timeline = Timeline::new();
        let track = timeline.add_track("Camera", 0);
        timeline.add_clip(
            track,
            0.0,
            1.0,
            clip::ClipPayload::Animation(clip::AnimationSpec {
                target: gaanim_core::ObjectId::from_raw(0),
                lens: clip::PropertyLensSpec::CameraShake {
                    origin: authored.position,
                    amplitude: 12.0,
                    frequency: 1.0,
                },
                rate_func: gaanim_math::RateFunc::Linear,
                delay: 0.0,
                label: Some("Camera".into()),
            }),
        );
        timeline.current_time = 0.25;
        world.insert_resource(timeline);

        camera_binding_system(&mut world);
        camera_rig_system(&mut world);

        assert_eq!(*world.resource::<gaanim_math::Camera>(), authored);
        let rig = world.resource::<gaanim_math::CameraRigCamera>().0;
        assert!((rig.position.x - 29.0).abs() < 1e-9);
        assert!((rig.position.y - 3.049_028_386_654_035_7).abs() < 1e-9);
    }

    #[test]
    fn scene_camera_phase_publishes_bound_camera_to_resolved_resource() {
        let mut app = App::new();
        app.add_plugins((
            gaanim_scene::hierarchy::GaanimScenePlugin,
            GaanimTimelinePlugin,
        ));
        app.insert_resource(gaanim_math::Camera::ortho_2d(1280, 720));
        app.insert_resource(gaanim_animation::DeltaTime { dt: 0.0 });
        app.world_mut().spawn(gaanim_animation::CameraBinding {
            order: 0,
            kind: gaanim_animation::CameraBindingKind::TwoD {
                center: Some(gaanim_animation::TrackingEndpoint::Static(
                    gaanim_core::glam::DVec3::new(125.0, -30.0, 0.0),
                )),
                zoom: Some(scalar(1.5)),
                rotation: None,
            },
            influence: scalar(1.0),
            windows: vec![gaanim_animation::CameraBindingWindow {
                start: 0.0,
                end: None,
            }],
        });

        app.update();

        let resolved = app.world().resource::<gaanim_math::ResolvedCamera>();
        assert_eq!(resolved.position.x, 125.0);
        assert_eq!(resolved.position.y, -30.0);
        assert!(matches!(
            resolved.projection,
            gaanim_math::Projection::Orthographic { zoom: 1.5 }
        ));
    }

    #[test]
    fn orbit_seek_publishes_the_atomic_pose_to_the_resolved_camera() {
        let mut app = App::new();
        app.add_plugins((
            gaanim_scene::hierarchy::GaanimScenePlugin,
            GaanimTimelinePlugin,
        ));
        app.insert_resource(gaanim_animation::DeltaTime { dt: 0.0 });

        let mut start = gaanim_math::Camera::perspective_3d(1280, 720, std::f64::consts::FRAC_PI_4);
        start.position = gaanim_core::glam::DVec3::new(0.0, 2.0, 12.0);
        start.target = gaanim_core::glam::DVec3::new(0.0, 1.0, 0.0);
        start
            .look_at(start.position, start.target, gaanim_core::glam::DVec3::Y)
            .expect("test camera pose is valid");
        app.insert_resource(start);
        app.world_mut()
            .spawn(gaanim_scene::MobjectId(gaanim_core::ObjectId::from_raw(0)));

        let mut timeline = app.world_mut().remove_resource::<Timeline>().unwrap();
        timeline.add_keyframe(0.0, snapshot::WorldSnapshot::capture(app.world_mut()));
        let track = timeline.add_track("Camera", 0);
        timeline.add_clip(
            track,
            0.0,
            2.0,
            clip::ClipPayload::Animation(clip::AnimationSpec {
                target: gaanim_core::ObjectId::from_raw(0),
                lens: clip::PropertyLensSpec::CameraOrbit {
                    from_position: start.position,
                    target: start.target,
                    up: start.up,
                    delta_yaw: std::f64::consts::FRAC_PI_2,
                    delta_pitch: -std::f64::consts::FRAC_PI_6,
                },
                rate_func: gaanim_math::RateFunc::Linear,
                delay: 0.0,
                label: Some("CameraOrbit".into()),
            }),
        );
        app.insert_resource(timeline);

        // Let startup capture settle, then perform the exact seek that an exporter
        // or snapshot capture would request before the camera publication phase.
        app.update();
        let mut timeline = app.world_mut().remove_resource::<Timeline>().unwrap();
        timeline.seek(app.world_mut(), 1.0);
        app.insert_resource(timeline);
        app.update();

        let mut expected = start;
        expected
            .orbit_around_target(std::f64::consts::FRAC_PI_4, -std::f64::consts::PI / 12.0)
            .expect("test orbit is valid");
        let authored = *app.world().resource::<gaanim_math::Camera>();
        let rig = app.world().resource::<gaanim_math::CameraRigCamera>().0;
        let resolved = app.world().resource::<gaanim_math::ResolvedCamera>();
        for (label, camera) in [
            ("authored", authored),
            ("rig", rig),
            ("resolved", **resolved),
        ] {
            assert!(
                (camera.position - expected.position).length() < 1e-9,
                "{label} position {:?}, expected {:?}",
                camera.position,
                expected.position
            );
            assert!((camera.target - start.target).length() < 1e-9);
            assert!(camera.rotation.dot(expected.rotation).abs() > 1.0 - 1e-12);
        }
    }

    #[test]
    fn follow_lag_is_bitwise_identical_for_direct_incremental_and_rewind_evaluation() {
        let mut world = World::new();
        world.insert_resource(gaanim_math::Camera::ortho_2d(1280, 720));
        let mut timeline = Timeline::new();
        let track = timeline.add_track("Camera", 0);
        timeline.add_clip(
            track,
            0.0,
            10.0,
            clip::ClipPayload::Animation(clip::AnimationSpec {
                target: gaanim_core::ObjectId::from_raw(0),
                lens: clip::PropertyLensSpec::CameraFollowEndpoint {
                    target: gaanim_animation::TrackingEndpoint::Static(
                        gaanim_core::glam::DVec3::new(100.0, -40.0, 0.0),
                    ),
                    from: gaanim_core::glam::DVec3::ZERO,
                    offset: gaanim_core::glam::DVec3::ZERO,
                    offset_space: gaanim_animation::FollowOffsetSpace::World,
                    lag: 0.5,
                },
                rate_func: gaanim_math::RateFunc::Linear,
                delay: 0.0,
                label: Some("Camera".into()),
            }),
        );
        world.insert_resource(timeline);

        let evaluate = |world: &mut World, time: f64| {
            world.resource_mut::<Timeline>().current_time = time;
            camera_binding_system(world);
            camera_rig_system(world);
            world.resource::<gaanim_math::CameraRigCamera>().0.position
        };

        let direct = evaluate(&mut world, 2.0);
        let _ = evaluate(&mut world, 0.25);
        let _ = evaluate(&mut world, 0.75);
        let incremental = evaluate(&mut world, 2.0);
        let _ = evaluate(&mut world, 0.1);
        let rewind = evaluate(&mut world, 2.0);

        assert_eq!(direct.x.to_bits(), incremental.x.to_bits());
        assert_eq!(direct.y.to_bits(), incremental.y.to_bits());
        assert_eq!(direct.x.to_bits(), rewind.x.to_bits());
        assert_eq!(direct.y.to_bits(), rewind.y.to_bits());
    }

    #[test]
    fn interactive_stop_controls_are_optional_in_headless_apps() {
        let mut app = App::new();
        app.init_resource::<Timeline>()
            .add_systems(Update, interactive_stop_input_system);

        assert!(!app.world().contains_resource::<ButtonInput<KeyCode>>());
        assert!(!app.world().contains_resource::<ButtonInput<MouseButton>>());
        app.update();
    }
    #[test]
    fn playback_crosses_segment_boundaries_and_pauses_only_at_explicit_stops() {
        let mut timeline = Timeline::new();
        timeline.current_time = 0.4;
        timeline.cached_duration = 2.0;
        timeline.is_playing = true;
        timeline.set_segments(vec![
            timeline::SegmentMetadata {
                id: 1,
                name: "first".to_owned(),
                notes: None,
                start_time: 0.0,
                end_time: 1.0,
                stops: Vec::new(),
            },
            timeline::SegmentMetadata {
                id: 2,
                name: "second".to_owned(),
                notes: None,
                start_time: 1.0,
                end_time: 2.0,
                stops: vec![timeline::SegmentStop {
                    name: Some("pause".to_owned()),
                    time: 1.2,
                }],
            },
        ]);

        let mut app = App::new();
        app.insert_resource(timeline)
            .insert_resource(PlaybackStopPolicy::Respect)
            .insert_resource(gaanim_animation::DeltaTime { dt: 1.0 })
            .add_systems(Update, timeline_playback_system);
        app.update();

        let timeline = app.world().resource::<Timeline>();
        assert_eq!(timeline.seek_request, Some(1.2));
        assert!(!timeline.is_playing);
    }

    #[test]
    fn continuous_playback_ignores_authored_stops() {
        let mut timeline = Timeline::new();
        timeline.current_time = 0.4;
        timeline.cached_duration = 2.0;
        timeline.is_playing = true;
        timeline.set_segments(vec![timeline::SegmentMetadata {
            id: 1,
            name: "segment".to_owned(),
            notes: None,
            start_time: 0.0,
            end_time: 2.0,
            stops: vec![timeline::SegmentStop {
                name: Some("pause".to_owned()),
                time: 1.2,
            }],
        }]);

        let mut app = App::new();
        app.insert_resource(timeline)
            .insert_resource(PlaybackStopPolicy::Ignore)
            .insert_resource(gaanim_animation::DeltaTime { dt: 1.0 })
            .add_systems(Update, timeline_playback_system);
        app.update();

        let timeline = app.world().resource::<Timeline>();
        assert_eq!(timeline.seek_request, Some(1.4));
        assert!(timeline.is_playing);
    }

    #[test]
    fn continuous_playback_still_applies_rate_and_looping() {
        let mut timeline = Timeline::new();
        timeline.current_time = 1.4;
        timeline.cached_duration = 3.0;
        timeline.is_playing = true;
        timeline.playback_rate = 2.0;
        timeline.loop_range = Some((1.0, 2.0));
        timeline.set_segments(vec![timeline::SegmentMetadata {
            id: 1,
            name: "segment".to_owned(),
            notes: None,
            start_time: 0.0,
            end_time: 3.0,
            stops: vec![timeline::SegmentStop {
                name: None,
                time: 1.5,
            }],
        }]);

        let mut app = App::new();
        app.insert_resource(timeline)
            .insert_resource(PlaybackStopPolicy::Ignore)
            .insert_resource(gaanim_animation::DeltaTime { dt: 0.4 })
            .add_systems(Update, timeline_playback_system);
        app.update();

        let timeline = app.world().resource::<Timeline>();
        assert!((timeline.seek_request.unwrap() - 1.2).abs() < 1e-9);
        assert!(timeline.is_playing);
    }
}
