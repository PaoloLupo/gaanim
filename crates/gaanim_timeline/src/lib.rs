use bevy::prelude::*;

pub mod clip;
pub mod notes;
pub mod prelude;
pub mod scene;
pub mod selection;
pub mod snapshot;
pub mod sound;
pub mod timeline;
pub mod transition;
pub mod transition_mask;

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
        let started = std::time::Instant::now();
        capture_reload_keyframe(world);
        if std::env::var_os("GAANIM_RELOAD_PROFILE").is_some_and(|value| value != "0") {
            gaanim_core::console::info(
                "profile",
                format!(
                    "t=0 keyframe {:.1} ms",
                    started.elapsed().as_secs_f64() * 1000.0
                ),
            );
        }
    }
}

/// Capture the t=0 keyframe of a freshly replayed scene without seeking.
pub fn capture_reload_keyframe(world: &mut World) {
    let Some(mut tl) = world.remove_resource::<Timeline>() else {
        return;
    };
    if !tl.keyframes.contains_key(&ordered_float::OrderedFloat(0.0)) {
        tl.add_keyframe(0.0, snapshot::WorldSnapshot::capture(world));
    }
    world.insert_resource(tl);
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
        playback_state.updaters_current = false;
    }
    if timeline.is_playing && timeline.seek_request.is_none() {
        let delta = scaled_dt;
        let next_time = timeline.current_time + delta;

        // Segment boundaries are continuous. Only explicitly authored stops
        // pause real-time playback.
        let current = timeline.current_time;
        let respect = *stop_policy == PlaybackStopPolicy::Respect;
        let ambient = respect.then(|| timeline.ambient_loop_at(current)).flatten();
        let hit_stop = (respect && ambient.is_none())
            .then(|| timeline.next_playback_stop(current, next_time))
            .flatten();

        if let Some((start, end)) = ambient {
            // A stop's ambient loop keeps playing while the presenter talks.
            timeline.seek_request = Some(if next_time >= end {
                start + (next_time - end) % (end - start)
            } else {
                next_time
            });
        } else if let Some(stop) = hit_stop {
            timeline.rest_at(stop);
        } else if let Some((start, end)) = timeline.loop_range {
            if next_time >= end {
                // Loop back around
                let loop_duration = end - start;
                let excess = next_time - end;
                timeline.seek_request = Some(start + excess % loop_duration);
            } else {
                timeline.seek_request = Some(next_time);
            }
        } else if !timeline.play_ranges.is_empty() {
            let (target, keep_playing) = timeline.selected_playback_step(current, next_time);
            timeline.seek_request = Some(target);
            timeline.is_playing = keep_playing;
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
    let ignore_pointer = timeline.ignore_pointer;
    let mouse_pressed = |button| {
        !ignore_pointer
            && mouse
                .as_ref()
                .is_some_and(|mouse| mouse.just_pressed(button))
    };

    if key_pressed(KeyCode::Home) {
        timeline.seek_request = Some(timeline.playback_start());
        timeline.is_playing = false;
        return;
    }
    if key_pressed(KeyCode::End) {
        timeline.seek_request = Some(timeline.playback_end());
        timeline.is_playing = false;
        return;
    }

    // Space only plays or pauses; it never skips to the next stop, so a
    // speaker can hold an animation mid-way without losing their place.
    if key_pressed(KeyCode::Space) {
        timeline.is_playing = !timeline.is_playing;
        return;
    }

    if key_pressed(KeyCode::Enter)
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
        timeline.advance();
    } else if should_go_back {
        timeline.go_back();
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
        .map_or(0.0, |timeline| timeline.evaluation_time());
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
    // A segment leaving through a transition keeps the camera it ended
    // with, through the constraints active just before its end; the camera
    // phase keeps it when it differs from the one at the playhead.
    let Some(rest) = world
        .get_resource::<gaanim_scene::SceneTransitionFrame>()
        .and_then(|frame| frame.outgoing_rest)
    else {
        return;
    };
    let (camera, end) = rest;
    let outgoing = gaanim_animation::bind_camera(world, end - OUTGOING_REST, camera);
    let mut frame = world.resource_mut::<gaanim_scene::SceneTransitionFrame>();
    if frame.outgoing_camera != Some(outgoing) {
        frame.outgoing_camera = Some(outgoing);
    }
}

/// How long before its end a segment leaving through a transition is seen
/// for its camera: what it showed last, before the next segment's changes.
const OUTGOING_REST: f64 = 1e-9;

/// Clips with the camera lenses that [`camera_rig_system`] and seeks resolve
/// on every frame, cached until clips are added or removed. A presentation
/// has thousands of clips and only a few of them drive the camera.
#[derive(Resource, Default)]
struct CameraClipIndex {
    key: Option<(u64, usize)>,
    ids: Vec<clip::ClipId>,
}

/// The camera clips of `timeline`, in arena order like `clips.values()`.
pub(crate) fn camera_clip_ids(timeline: &Timeline, world: &mut World) -> Vec<clip::ClipId> {
    let key = (timeline.property_revision(), timeline.clips.len());
    let mut index = world.get_resource_or_insert_with(CameraClipIndex::default);
    if index.key != Some(key) {
        index.ids = timeline
            .clips
            .iter()
            .filter(|(_, clip)| {
                matches!(
                    &clip.payload,
                    clip::ClipPayload::Animation(anim) if matches!(
                        anim.lens,
                        clip::PropertyLensSpec::CameraFollow { .. }
                            | clip::PropertyLensSpec::CameraFollowEndpoint { .. }
                            | clip::PropertyLensSpec::CameraPositionSource { .. }
                            | clip::PropertyLensSpec::CameraFrameDynamic { .. }
                            | clip::PropertyLensSpec::CameraShake { .. }
                            | clip::PropertyLensSpec::CameraPosition { .. }
                            | clip::PropertyLensSpec::CameraPathFollow { .. }
                            | clip::PropertyLensSpec::CameraDollyZoom { .. }
                            | clip::PropertyLensSpec::CameraPanZoom { .. }
                            | clip::PropertyLensSpec::CameraState { .. }
                            | clip::PropertyLensSpec::CameraLookAtSource { .. }
                    )
                )
            })
            .map(|(id, _)| id)
            .collect();
        index.key = Some(key);
    }
    index.ids.clone()
}

/// Where a camera constraint puts the camera this frame.
enum Followed {
    /// The camera's x and y, as a pan on the scene plane sets them.
    At(gaanim_core::glam::DVec3),
    /// The fraction `amount` of the way toward `point`, from `from` for a
    /// flat camera; a camera at an angle moves its framing instead, or with
    /// `turn` turns toward it where it stands.
    Toward {
        point: gaanim_core::glam::DVec3,
        from: gaanim_core::glam::DVec3,
        amount: f64,
        turn: bool,
    },
    /// The eye at `position`, turned toward `target`.
    Aimed {
        position: gaanim_core::glam::DVec3,
        target: gaanim_core::glam::DVec3,
        up: gaanim_core::glam::DVec3,
    },
}

/// Resolves temporary camera constraints and additive modifiers after reactive
/// updaters/layout, immediately before the presentation camera is copied.
pub fn camera_rig_system(world: &mut World) {
    if !world.contains_resource::<Timeline>() {
        return;
    }
    let camera_clips =
        world.resource_scope(|world, timeline: Mut<Timeline>| camera_clip_ids(&timeline, world));
    let timeline = world.resource::<Timeline>();
    let current_time = timeline.evaluation_time();
    let follow = camera_clips
        .iter()
        .filter_map(|id| timeline.clips.get(*id))
        .filter(|clip| clip.start <= current_time && current_time < clip.end())
        .filter_map(|clip| {
            let clip::ClipPayload::Animation(anim) = &clip.payload else {
                return None;
            };
            // A pan eases toward where its endpoint is drawn now, with the
            // clip's easing at its progress, and a look-at whose endpoints
            // move turns toward where they are drawn: fixed ones the
            // timeline already aimed.
            let constrains = match &anim.lens {
                clip::PropertyLensSpec::CameraFollow { .. }
                | clip::PropertyLensSpec::CameraFollowEndpoint { .. }
                | clip::PropertyLensSpec::CameraPositionSource { .. } => true,
                clip::PropertyLensSpec::CameraLookAtSource { eye, target, .. } => {
                    !matches!(eye, gaanim_animation::TrackingEndpoint::Static(_))
                        || !matches!(target, gaanim_animation::TrackingEndpoint::Static(_))
                }
                _ => false,
            };
            if !constrains {
                return None;
            }
            let progress = if clip.duration > 0.0 {
                ((current_time - clip.start) / clip.duration).clamp(0.0, 1.0)
            } else {
                1.0
            };
            let eased = anim.rate_func.evaluate(progress);
            Some((clip.start, eased, anim.lens.clone()))
        })
        .max_by(|(left, ..), (right, ..)| left.total_cmp(right));
    let dynamic_frame = camera_clips
        .iter()
        .filter_map(|id| timeline.clips.get(*id))
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
    let mut shake_roll = 0.0;
    for clip in camera_clips
        .iter()
        .filter_map(|id| timeline.clips.get(*id))
        .filter(|clip| clip.start <= current_time && current_time < clip.end())
    {
        let clip::ClipPayload::Animation(anim) = &clip.payload else {
            continue;
        };
        let clip::PropertyLensSpec::CameraShake {
            origin: _,
            amplitude,
            frequency,
            trauma,
        } = &anim.lens
        else {
            continue;
        };
        if let Some(trauma) = trauma {
            // Trauma decays in timeline seconds; easing does not reshape it.
            let (offset, roll) = trauma.sample(current_time - clip.start, clip.duration);
            shake_offset += offset.extend(0.0);
            shake_roll += roll;
            continue;
        }
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
            (phase * std::f64::consts::GOLDEN_RATIO).sin() * *amplitude * 0.6 * envelope,
            0.0,
        );
    }

    let follow_position = follow.and_then(|(start, eased, lens)| match lens {
        clip::PropertyLensSpec::CameraFollow { target } => {
            let mut query =
                world.query::<(&gaanim_scene::MobjectId, &gaanim_math::SpatialTransform)>();
            query
                .iter(world)
                .find_map(|(id, transform)| (id.0 == target).then_some(transform.translation))
                .map(|position| (start, Followed::At(position)))
        }
        clip::PropertyLensSpec::CameraFollowEndpoint {
            target,
            from,
            offset,
            offset_space,
            lag,
            turn,
            ..
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
            Some((
                start,
                Followed::Toward {
                    point: desired,
                    from,
                    amount: influence,
                    turn,
                },
            ))
        }
        clip::PropertyLensSpec::CameraPositionSource { from, to, .. } => {
            let target = gaanim_animation::resolve_tracking_endpoint(&to, world)?;
            let destination = gaanim_core::glam::DVec3::new(target.x, target.y, from.z);
            Some((start, Followed::At(from.lerp(destination, eased))))
        }
        clip::PropertyLensSpec::CameraLookAtSource {
            from_position,
            from_target,
            eye,
            target,
            up,
            from_up,
            ..
        } => {
            let eye = gaanim_animation::resolve_tracking_endpoint(&eye, world)?;
            let target = gaanim_animation::resolve_tracking_endpoint(&target, world)?;
            let position = from_position.lerp(eye, eased);
            let target = from_target.lerp(target, eased);
            Some((
                start,
                Followed::Aimed {
                    position,
                    target,
                    up: gaanim_math::Camera::turning_up(
                        from_up.unwrap_or(up),
                        up,
                        eased,
                        position,
                        target,
                    ),
                },
            ))
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
            interpolation,
            ..
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
            from_position.lerp(
                framed.center(),
                interpolation.pan_weight(from_zoom, desired_zoom, influence),
            ),
            interpolation.zoom(from_zoom, desired_zoom, influence),
        ))
    });

    let Some(mut camera) = world.get_resource_mut::<gaanim_math::CameraRigCamera>() else {
        return;
    };
    let selected = match (follow_position, frame_result) {
        (Some((follow_start, follow)), Some((frame_start, frame, zoom))) => {
            if frame_start >= follow_start {
                camera.0.projection = gaanim_math::Projection::Orthographic { zoom };
                Some(Followed::At(frame))
            } else {
                Some(follow)
            }
        }
        (Some((_, follow)), None) => Some(follow),
        (None, Some((_, frame, zoom))) => {
            camera.0.projection = gaanim_math::Projection::Orthographic { zoom };
            Some(Followed::At(frame))
        }
        (None, None) => None,
    };
    let selected_position = match selected {
        Some(Followed::Toward {
            point,
            from,
            amount,
            ..
        }) if camera.0.is_flat() => Some(from.lerp(point, amount)),
        // A camera that looks at the scene from an angle, or through a
        // perspective, keeps its framing: it moves with the point, or turns
        // toward it where it stands.
        Some(Followed::Toward {
            point,
            amount,
            turn: true,
            ..
        }) => {
            let target = camera.0.target.lerp(point, amount);
            let (position, up) = (camera.0.position, camera.0.up);
            if gaanim_math::Camera::validate_look_at(position, target, up).is_ok() {
                camera
                    .0
                    .look_at(position, target, up)
                    .expect("validated turned pose");
            }
            None
        }
        Some(Followed::Toward { point, amount, .. }) => {
            camera.0 = camera.0.moved_toward(point, amount);
            None
        }
        Some(Followed::At(position)) => Some(position),
        Some(Followed::Aimed {
            position,
            target,
            up,
        }) => {
            if gaanim_math::Camera::validate_look_at(position, target, up).is_ok() {
                camera
                    .0
                    .look_at(position, target, up)
                    .expect("validated aimed pose");
            }
            None
        }
        None => None,
    };
    if let Some(position) = selected_position {
        camera.0.position.x = position.x;
        camera.0.position.y = position.y;
        if position.z != 0.0 {
            camera.0.position.z = position.z;
        }
    }
    camera.0.position += shake_offset;
    if shake_roll != 0.0 {
        camera.0.rotation *= gaanim_core::glam::DQuat::from_rotation_z(shake_roll);
    }
}

#[cfg(test)]
mod tests;
