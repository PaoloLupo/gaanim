use bevy::prelude::*;

pub mod clip;
pub mod prelude;
pub mod scene;
pub mod snapshot;
pub mod timeline;
pub mod transition;

use gaanim_scene::hierarchy::SceneSet;
use timeline::Timeline;

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
            .add_systems(PostStartup, capture_initial_keyframe_system)
            .add_systems(
                Update,
                (
                    timeline_playback_system.in_set(SceneSet::Input),
                    presentation_input_system
                        .in_set(SceneSet::Input)
                        .after(timeline_playback_system),
                    deferred_keyframe_capture_system
                        .in_set(SceneSet::Animation)
                        .before(timeline_seek_system),
                    timeline_seek_system.in_set(SceneSet::Animation),
                    camera_follow_system.in_set(SceneSet::Layout),
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

        // Check if we crossed a slide breakpoint.
        let current = timeline.current_time;
        let mut hit_breakpoint = None;
        for &bp in &timeline.breakpoints {
            if bp > current + 1e-5 && bp <= next_time + 1e-5 {
                hit_breakpoint = Some(bp);
                break;
            }
        }

        if let Some(bp) = hit_breakpoint {
            timeline.seek_request = Some(bp);
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

/// System: Handles user input (keyboard, mouse) to navigate slide breakpoints in presentation mode.
pub fn presentation_input_system(
    keyboard: Option<Res<ButtonInput<KeyCode>>>,
    mouse: Option<Res<ButtonInput<MouseButton>>>,
    mut timeline: ResMut<Timeline>,
) {
    if (timeline.breakpoints.is_empty() && timeline.presentation.is_empty())
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
        } else if !timeline.presentation.is_empty() {
            if let Some(stop) = timeline.next_presentation_stop(timeline.current_time) {
                timeline.seek_request = Some(stop);
                timeline.is_playing = false;
            } else {
                timeline.seek_request = Some(timeline.cached_duration);
                timeline.is_playing = false;
            }
        } else {
            let current = timeline.current_time;
            let mut next_bp = None;
            for &bp in &timeline.breakpoints {
                if bp > current + 1e-5 {
                    next_bp = Some(bp);
                    break;
                }
            }
            if let Some(bp) = next_bp {
                timeline.seek_request = Some(bp);
                timeline.is_playing = false;
            } else {
                timeline.seek_request = Some(timeline.cached_duration);
                timeline.is_playing = false;
            }
        }
    } else if should_go_back {
        let target = if timeline.presentation.is_empty() {
            let current = timeline.current_time;
            timeline
                .breakpoints
                .iter()
                .rev()
                .find(|&&bp| bp < current - 0.2)
                .copied()
                .unwrap_or(0.0)
        } else {
            timeline
                .previous_presentation_stop(timeline.current_time)
                .unwrap_or(0.0)
        };
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

/// Updates active camera-follow clips after reactive updaters have moved their targets.
pub fn camera_follow_system(
    timeline: Res<Timeline>,
    targets: Query<(&gaanim_scene::MobjectId, &gaanim_math::SpatialTransform)>,
    camera: Option<ResMut<gaanim_math::Camera>>,
) {
    let Some(mut camera) = camera else {
        return;
    };
    let target_id = timeline
        .clips
        .values()
        .filter(|clip| clip.start <= timeline.current_time && timeline.current_time < clip.end())
        .filter_map(|clip| match &clip.payload {
            clip::ClipPayload::Animation(anim) => match anim.lens {
                clip::PropertyLensSpec::CameraFollow { target } => Some((clip.start, target)),
                _ => None,
            },
            _ => None,
        })
        .max_by(|(left, _), (right, _)| left.total_cmp(right))
        .map(|(_, target)| target);
    let Some(target_id) = target_id else {
        return;
    };
    if let Some((_, transform)) = targets.iter().find(|(id, _)| id.0 == target_id) {
        camera.position.x = transform.translation.x;
        camera.position.y = transform.translation.y;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presentation_controls_are_optional_in_headless_apps() {
        let mut app = App::new();
        app.init_resource::<Timeline>()
            .add_systems(Update, presentation_input_system);

        assert!(!app.world().contains_resource::<ButtonInput<KeyCode>>());
        assert!(!app.world().contains_resource::<ButtonInput<MouseButton>>());
        app.update();
    }
}
