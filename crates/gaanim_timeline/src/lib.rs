use bevy::prelude::*;

pub mod clip;
pub mod snapshot;
pub mod timeline;
pub mod prelude;

use timeline::Timeline;
use gaanim_scene::hierarchy::SceneSet;

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
                    timeline_seek_system.in_set(SceneSet::Animation),
                ),
            );
    }
}

/// System: Exclusive system to capture the initial world state at t=0.0 as a keyframe,
/// running in the PostStartup stage so all startup commands have completed.
pub fn capture_initial_keyframe_system(world: &mut World) {
    let timeline = world.remove_resource::<Timeline>();
    if let Some(mut tl) = timeline {
        if !tl.keyframes.contains_key(&ordered_float::OrderedFloat(0.0)) {
            let snapshot = snapshot::WorldSnapshot::capture(world);
            tl.add_keyframe(0.0, snapshot);
        }
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

        if let Some((start, end)) = timeline.loop_range {
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
