//! The native window waits while nothing on screen can change.
//!
//! Bevy updates a focused window continuously: the whole schedule, egui and
//! the canvas composition ran every vsync for a paused preview or the project
//! hub. While nothing moves, the loop instead waits for input, a repaint egui
//! asks for, a worker's wake-up (see [`waking_receiver`]) or `IDLE_WAIT`.
//! Anything that moves on its own (playback, a presentation, an export, a
//! decoding video, a running job) keeps it continuous, and so does the
//! `ACTIVITY_GRACE` after it stops, for work that takes a few frames to
//! settle. The web player keeps Bevy's default.

use std::time::{Duration, Instant};

use bevy::audio::{AudioSink, AudioSinkPlayback};
use bevy::input::touch::Touches;
use bevy::prelude::*;
use bevy::window::WindowEvent;
use bevy::winit::{EventLoopProxyWrapper, UpdateMode, WinitSettings, WinitUserEvent};
use gaanim_animation::DeltaTime;
use gaanim_timeline::timeline::Timeline;

use crate::overlays::COPIED_FEEDBACK_SECS;

/// Longest wait of a resting window: the fallback for results nothing wakes
/// it for, such as an image asset or a shader pipeline that finishes loading
/// late. It matches `Time<Virtual>`'s largest step, so clocks read from
/// `Time` keep up while resting.
const IDLE_WAIT: Duration = Duration::from_millis(250);
/// How long the loop stays continuous after the last input or motion, so a
/// seek, a reload's replay or a stopped playback's full-resolution frame
/// settles before it rests.
const ACTIVITY_GRACE: Duration = Duration::from_millis(500);
/// Largest timeline step of the first update after a rest, in seconds:
/// playback started by that update's key press begins where it was, not a
/// whole wait later.
const WAKE_DT: f64 = 1.0 / 60.0;

pub(crate) struct IdlePlugin;

impl Plugin for IdlePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<IdleState>()
            .add_systems(
                Update,
                clamp_wake_delta_system
                    .in_set(gaanim_scene::hierarchy::SceneSet::Input)
                    .after(gaanim_animation::sync_delta_time_system)
                    .before(gaanim_timeline::timeline_playback_system),
            )
            .add_systems(
                Last,
                (
                    note_input_system,
                    note_playback_system,
                    note_editor_system,
                    note_work_system,
                    update_mode_system,
                )
                    .chain(),
            );
    }
}

#[derive(Resource)]
struct IdleState {
    /// When the user last acted or something last moved.
    last_activity: Instant,
    /// Something moved or the user acted during this update.
    active: bool,
    /// Playhead and duration of the previous update, as bits.
    previous_time: u64,
    previous_duration: u64,
    /// The loop rests (waits) before the next update.
    resting: bool,
    /// Frame profiling measures continuous frames.
    profiling: bool,
}

impl Default for IdleState {
    fn default() -> Self {
        Self {
            last_activity: Instant::now(),
            active: false,
            previous_time: 0,
            previous_duration: 0,
            resting: false,
            profiling: crate::frame_profile::enabled(),
        }
    }
}

/// The first update after a rest measured the whole wait: keep it from
/// advancing playback that this update starts.
fn clamp_wake_delta_system(state: Res<IdleState>, mut delta: ResMut<DeltaTime>) {
    if state.resting {
        delta.dt = delta.dt.min(WAKE_DT);
    }
}

/// Input this update, or a key, button or finger still held.
fn note_input_system(
    mut state: ResMut<IdleState>,
    mut events: MessageReader<WindowEvent>,
    keys: Option<Res<ButtonInput<KeyCode>>>,
    mouse: Option<Res<ButtonInput<MouseButton>>>,
    touches: Option<Res<Touches>>,
) {
    let mut window_input = false;
    for event in events.read() {
        // Raw mouse motion arrives from anywhere on the desktop, not only
        // over the window, so it does not count.
        window_input |= !matches!(event, WindowEvent::MouseMotion(_));
    }
    let held = keys.is_some_and(|keys| keys.get_pressed().next().is_some())
        || mouse.is_some_and(|mouse| mouse.get_pressed().next().is_some())
        || touches.is_some_and(|touches| touches.iter().next().is_some());
    state.active = window_input || held;
}

/// Playback, a seek, a playhead that moved, a video frame still decoding or
/// a sound playing.
fn note_playback_system(
    mut state: ResMut<IdleState>,
    timeline: Res<Timeline>,
    videos: Query<&gaanim_media::VideoPlayback>,
    sinks: Query<&AudioSink>,
) {
    let time = timeline.current_time;
    let moved = time.to_bits() != state.previous_time
        || timeline.cached_duration.to_bits() != state.previous_duration;
    state.previous_time = time.to_bits();
    state.previous_duration = timeline.cached_duration.to_bits();
    // A realtime video frame arrives from the decoder thread, polled once
    // per update until the playhead's frame is shown.
    let decoding = videos
        .iter()
        .any(|video| video.last_frame != Some(video.frame_index(time)));
    let sounding = sinks.iter().any(|sink| !sink.is_paused() && !sink.empty());
    state.active |=
        moved || timeline.is_playing || timeline.seek_request.is_some() || decoding || sounding;
}

/// A presentation, a drag, an animating playback bar, or a display that
/// measures or times itself.
#[allow(clippy::too_many_arguments)]
fn note_editor_system(
    mut state: ResMut<IdleState>,
    presentation: Res<crate::PresentationMode>,
    presenter_windows: Query<(), With<crate::presenter::PresenterWindow>>,
    editor: Res<crate::EditorState>,
    drag: Res<crate::PreviewDrag>,
    interactive: Res<crate::PreviewInteractive>,
    overlays: Res<crate::overlays::EditorOverlays>,
    fps: Res<crate::fps_overlay::FpsOverlay>,
    time: Res<Time>,
) {
    // Shader backgrounds move on the wall clock while a presentation rests,
    // and Presenter View shows its clock.
    let presenting = presentation.active || !presenter_windows.is_empty();
    let bar_sliding = editor.bar_visibility > 0.0 && editor.bar_visibility < 1.0;
    // The seek bar's hover card shows previews rendered on a worker.
    let seek_bar = editor.seek_bar_hover.is_some() || editor.seek_bar_drag_target.is_some();
    let now = time.elapsed_secs_f64();
    let copied = overlays
        .copied_at
        .is_some_and(|at| now - at < COPIED_FEEDBACK_SECS);
    let profiling = state.profiling;
    state.active |= presenting
        || bar_sliding
        || seek_bar
        || editor.touch_reveal
        || drag.active
        || interactive.needs_frame
        || copied
        || fps.visible
        || profiling;
}

/// Background work whose results the next updates take: an export, the
/// narration recorder and its jobs, a poll relay, the hub's workers.
fn note_work_system(
    mut state: ResMut<IdleState>,
    export: Res<crate::export::ExportState>,
    narration: Res<crate::narration::NarrationSession>,
    narration_panel: Res<crate::narration::NarrationPanel>,
    polls: Res<crate::polls::AudiencePolls>,
    hub: Option<Res<crate::project_hub::ProjectHubState>>,
) {
    state.active |= export.active
        || narration.capturing()
        || narration_panel.working()
        || polls.connected()
        || hub.is_some_and(|hub| hub.working());
}

/// Continuous while anything is active or within [`ACTIVITY_GRACE`] of it,
/// as Bevy's default does; otherwise rest, focused or not.
fn update_mode_system(mut state: ResMut<IdleState>, settings: Option<ResMut<WinitSettings>>) {
    let now = Instant::now();
    if state.active {
        state.last_activity = now;
    }
    let resting = now.duration_since(state.last_activity) >= ACTIVITY_GRACE;
    state.resting = resting;
    let Some(mut settings) = settings else {
        return;
    };
    let (focused_mode, unfocused_mode) = if resting {
        // Device events (raw mouse motion anywhere) do not wake it; window
        // input, egui's repaints and worker wake-ups do.
        let rest = UpdateMode::reactive_low_power(IDLE_WAIT);
        (rest, rest)
    } else {
        let default = WinitSettings::default();
        (default.focused_mode, default.unfocused_mode)
    };
    if settings.focused_mode != focused_mode || settings.unfocused_mode != unfocused_mode {
        settings.focused_mode = focused_mode;
        settings.unfocused_mode = unfocused_mode;
    }
}

/// Forward what a worker sends on `receiver` to the returned receiver,
/// waking the window's event loop after each message, so a resting window
/// takes the result at once instead of on its next input or timeout.
/// Without a winit event loop the receiver comes back unchanged.
pub fn waking_receiver<T: Send + 'static>(
    world: &World,
    receiver: crossbeam_channel::Receiver<T>,
    thread_name: &str,
) -> crossbeam_channel::Receiver<T> {
    let Some(proxy) = world.get_resource::<EventLoopProxyWrapper>() else {
        return receiver;
    };
    let proxy = (**proxy).clone();
    let unforwarded = receiver.clone();
    let (sender, forwarded) = crossbeam_channel::unbounded();
    let spawned = std::thread::Builder::new()
        .name(thread_name.to_string())
        .spawn(move || {
            for message in receiver {
                if sender.send(message).is_err() {
                    break;
                }
                let _ = proxy.send_event(WinitUserEvent::WakeUp);
            }
        });
    match spawned {
        Ok(_) => forwarded,
        Err(error) => {
            bevy::log::warn!("a resting window will take worker results late: {error}");
            unforwarded
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::ecs::system::RunSystemOnce;

    #[test]
    fn rests_after_the_grace_and_runs_continuously_while_active() {
        let mut world = World::new();
        world.insert_resource(WinitSettings::default());
        world.insert_resource(IdleState {
            last_activity: Instant::now() - ACTIVITY_GRACE,
            ..default()
        });
        world.run_system_once(update_mode_system).unwrap();
        let rest = UpdateMode::reactive_low_power(IDLE_WAIT);
        let settings = world.resource::<WinitSettings>();
        assert_eq!(
            (settings.focused_mode, settings.unfocused_mode),
            (rest, rest)
        );
        assert!(world.resource::<IdleState>().resting);

        world.resource_mut::<IdleState>().active = true;
        world.run_system_once(update_mode_system).unwrap();
        assert_eq!(
            world.resource::<WinitSettings>().focused_mode,
            UpdateMode::Continuous
        );
        assert!(!world.resource::<IdleState>().resting);
    }

    #[test]
    fn the_update_after_a_rest_advances_playback_one_frame_at_most() {
        let mut world = World::new();
        world.insert_resource(DeltaTime { dt: 0.25 });
        world.insert_resource(IdleState {
            resting: true,
            ..default()
        });
        world.run_system_once(clamp_wake_delta_system).unwrap();
        assert_eq!(world.resource::<DeltaTime>().dt, WAKE_DT);

        world.insert_resource(DeltaTime { dt: 0.25 });
        world.resource_mut::<IdleState>().resting = false;
        world.run_system_once(clamp_wake_delta_system).unwrap();
        assert_eq!(world.resource::<DeltaTime>().dt, 0.25);
    }

    #[test]
    fn without_an_event_loop_the_receiver_comes_back_unchanged() {
        let world = World::new();
        let (sender, receiver) = crossbeam_channel::unbounded();
        let receiver = waking_receiver(&world, receiver, "gaanim-test-wake");
        sender.send(7).unwrap();
        assert_eq!(receiver.try_recv(), Ok(7));
    }
}
