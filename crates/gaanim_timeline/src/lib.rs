use bevy::prelude::*;

pub mod clip;
pub mod prelude;
pub mod scene;
pub mod snapshot;
pub mod timeline;
pub mod transition;

use gaanim_scene::hierarchy::SceneSet;
use timeline::Timeline;

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
                    timeline_seek_system.in_set(SceneSet::Animation),
                ),
            );
    }
}

/// System: Exclusive system to capture the initial world state at t=0.0 as a keyframe,
/// running in the PostStartup stage so all startup commands have completed.
///
/// Also triggers an initial seek to t=0 so that scene visibility is applied
/// immediately (hiding entities that belong to non-active scenes).
pub fn capture_initial_keyframe_system(world: &mut World) {
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
) {
    if timeline.is_playing {
        let delta = dt.dt * timeline.playback_rate;
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
    keyboard: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    mut timeline: ResMut<Timeline>,
) {
    if timeline.breakpoints.is_empty() || timeline.ignore_input {
        return;
    }

    let mut should_advance = false;
    if keyboard.just_pressed(KeyCode::Space)
        || keyboard.just_pressed(KeyCode::Enter)
        || keyboard.just_pressed(KeyCode::ArrowRight)
        || mouse.just_pressed(MouseButton::Left)
    {
        should_advance = true;
    }

    let mut should_go_back = false;
    if keyboard.just_pressed(KeyCode::ArrowLeft) || keyboard.just_pressed(KeyCode::Backspace) {
        should_go_back = true;
    }

    if should_advance {
        if !timeline.is_playing {
            timeline.is_playing = true;
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
        let current = timeline.current_time;
        let mut prev_bp = None;
        for &bp in timeline.breakpoints.iter().rev() {
            if bp < current - 0.2 {
                prev_bp = Some(bp);
                break;
            }
        }
        let target = prev_bp.unwrap_or(0.0);
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
