//! Kiosk mode: a presentation that runs by itself (`--present --auto 8s
//! --loop`), for a stand, a hallway or a room's screen.
//!
//! Every rest (a stop, its ambient loop, a pause) advances after the
//! `--auto` wait, and with `--loop` the end starts the talk over. A key or a
//! click starts the wait again, so a visitor keeps control while they press
//! and the talk carries on once they leave. A kiosk opens no Presenter View,
//! shows no playback dock and hides the cursor.

use std::time::Duration;

use bevy::prelude::*;
use bevy::window::{CursorOptions, PrimaryWindow};
use gaanim_timeline::timeline::Timeline;

use crate::PresentationMode;

/// How a presentation runs by itself (`--auto`, `--loop`).
#[derive(Resource, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Kiosk {
    /// Wait at each rest before advancing (`--auto`).
    pub auto: Option<Duration>,
    /// Start over at the end (`--loop`), after the `auto` wait if any.
    pub looping: bool,
}

impl Kiosk {
    /// Whether the presentation runs by itself.
    pub fn active(&self) -> bool {
        self.auto.is_some() || self.looping
    }
}

/// Where a presentation rests, for the kiosk's wait.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Rest {
    /// Paused at this time, or looping the ambient animation of the stop
    /// there.
    At(f64),
    /// Paused at the end.
    End,
}

/// What the kiosk asks of the presentation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum KioskAction {
    /// Go on to the next step.
    Advance,
    /// Play the talk again from its start.
    Restart,
}

/// The wait at the current rest.
#[derive(Resource, Debug, Default)]
pub(crate) struct KioskClock {
    rest: Option<Rest>,
    waited: f64,
    /// The wait at this rest is over and its action was taken.
    done: bool,
}

impl KioskClock {
    /// Count `delta` seconds at `rest` (none while playing). A new rest or
    /// `input` starts the wait again; the action comes once per rest.
    pub(crate) fn step(
        &mut self,
        kiosk: &Kiosk,
        rest: Option<Rest>,
        input: bool,
        delta: f64,
    ) -> Option<KioskAction> {
        if rest != self.rest || input {
            self.rest = rest;
            self.waited = 0.0;
            self.done = false;
            return None;
        }
        let rest = rest?;
        if self.done {
            return None;
        }
        self.waited += delta.max(0.0);
        let (wait, action) = match rest {
            Rest::End if kiosk.looping => (kiosk.auto.unwrap_or_default(), KioskAction::Restart),
            Rest::End => return None,
            Rest::At(_) => (kiosk.auto?, KioskAction::Advance),
        };
        if self.waited + 1e-9 < wait.as_secs_f64() {
            return None;
        }
        self.done = true;
        Some(action)
    }
}

/// Where `timeline` rests now, if it does.
pub(crate) fn rest_of(timeline: &Timeline) -> Option<Rest> {
    if timeline.seek_request.is_some() {
        return None;
    }
    if let Some((start, _)) = timeline.ambient_loop_at(timeline.current_time) {
        return Some(Rest::At(start));
    }
    if timeline.is_playing {
        return None;
    }
    Some(if timeline.current_time >= timeline.playback_end() - 1e-6 {
        Rest::End
    } else {
        Rest::At(timeline.current_time)
    })
}

/// A wait for `--auto`: seconds alone (`8`), or a length with units (`8s`,
/// `1m30s`) or a clock (`0:45`).
pub fn parse_auto_wait(text: &str) -> Result<Duration, String> {
    let expected = || format!("--auto takes a wait such as 8s, 8 or 1m30s, not `{text}`");
    let wait = match text.trim().parse::<f64>() {
        Ok(seconds) if seconds.is_finite() && seconds > 0.0 => Duration::from_secs_f64(seconds),
        Ok(_) => return Err(expected()),
        Err(_) => super::pace::parse_talk_duration(text).map_err(|_| expected())?,
    };
    Ok(wait)
}

/// Advance at each rest after the wait and start over at the end.
pub(crate) fn kiosk_system(
    kiosk: Res<Kiosk>,
    presentation: Res<PresentationMode>,
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    time: Res<Time<bevy::time::Real>>,
    timeline: Option<ResMut<Timeline>>,
    mut clock: ResMut<KioskClock>,
) {
    let Some(mut timeline) = timeline else {
        return;
    };
    if !kiosk.active() || !presentation.active {
        clock.rest = None;
        return;
    }
    // The kiosk decides what the end does: a script's preview loops over
    // the whole timeline, which would wrap around without the wait.
    if timeline.loop_range.is_some() {
        timeline.loop_range = None;
    }
    let input =
        keys.get_just_pressed().next().is_some() || mouse.get_just_pressed().next().is_some();
    match clock.step(&kiosk, rest_of(&timeline), input, time.delta_secs_f64()) {
        Some(KioskAction::Advance) => timeline.advance(),
        Some(KioskAction::Restart) => {
            timeline.seek_request = Some(timeline.playback_start());
            timeline.is_playing = true;
        }
        None => {}
    }
}

/// Hide the cursor over a kiosk's audience screen; show it again when the
/// presentation ends.
pub(crate) fn kiosk_cursor_system(
    kiosk: Res<Kiosk>,
    presentation: Res<PresentationMode>,
    mut cursors: Query<&mut CursorOptions, With<PrimaryWindow>>,
) {
    let visible = !(kiosk.active() && presentation.active);
    for mut cursor in &mut cursors {
        if cursor.visible != visible {
            cursor.visible = visible;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const AUTO: Kiosk = Kiosk {
        auto: Some(Duration::from_secs(8)),
        looping: false,
    };

    #[test]
    fn a_rest_advances_once_after_the_wait() {
        let mut clock = KioskClock::default();
        let rest = Some(Rest::At(2.0));
        assert_eq!(clock.step(&AUTO, rest, false, 0.0), None);
        assert_eq!(clock.step(&AUTO, rest, false, 7.5), None);
        assert_eq!(
            clock.step(&AUTO, rest, false, 0.5),
            Some(KioskAction::Advance)
        );
        assert_eq!(clock.step(&AUTO, rest, false, 10.0), None);
    }

    #[test]
    fn input_and_a_new_rest_start_the_wait_again() {
        let mut clock = KioskClock::default();
        clock.step(&AUTO, Some(Rest::At(2.0)), false, 0.0);
        clock.step(&AUTO, Some(Rest::At(2.0)), false, 7.0);
        // A key press: the visitor gets the whole wait again.
        assert_eq!(clock.step(&AUTO, Some(Rest::At(2.0)), true, 0.0), None);
        assert_eq!(clock.step(&AUTO, Some(Rest::At(2.0)), false, 7.0), None);
        // `←` lands on the previous stop: a new rest, a new wait.
        assert_eq!(clock.step(&AUTO, Some(Rest::At(1.0)), false, 0.0), None);
        assert_eq!(clock.step(&AUTO, Some(Rest::At(1.0)), false, 7.9), None);
        assert_eq!(
            clock.step(&AUTO, Some(Rest::At(1.0)), false, 0.1),
            Some(KioskAction::Advance)
        );
    }

    #[test]
    fn playback_does_not_count() {
        let mut clock = KioskClock::default();
        clock.step(&AUTO, Some(Rest::At(2.0)), false, 0.0);
        clock.step(&AUTO, Some(Rest::At(2.0)), false, 6.0);
        assert_eq!(clock.step(&AUTO, None, false, 6.0), None);
        assert_eq!(clock.step(&AUTO, Some(Rest::At(2.0)), false, 6.0), None);
    }

    #[test]
    fn the_end_starts_over_only_with_loop() {
        let mut clock = KioskClock::default();
        clock.step(&AUTO, Some(Rest::End), false, 0.0);
        assert_eq!(clock.step(&AUTO, Some(Rest::End), false, 60.0), None);

        let looping = Kiosk {
            looping: true,
            ..AUTO
        };
        let mut clock = KioskClock::default();
        clock.step(&looping, Some(Rest::End), false, 0.0);
        assert_eq!(clock.step(&looping, Some(Rest::End), false, 7.0), None);
        assert_eq!(
            clock.step(&looping, Some(Rest::End), false, 1.0),
            Some(KioskAction::Restart)
        );
    }

    #[test]
    fn loop_alone_keeps_stops_and_starts_over_at_once() {
        let looping = Kiosk {
            auto: None,
            looping: true,
        };
        let mut clock = KioskClock::default();
        clock.step(&looping, Some(Rest::At(2.0)), false, 0.0);
        assert_eq!(
            clock.step(&looping, Some(Rest::At(2.0)), false, 600.0),
            None
        );
        clock.step(&looping, Some(Rest::End), false, 0.0);
        assert_eq!(
            clock.step(&looping, Some(Rest::End), false, 0.0),
            Some(KioskAction::Restart)
        );
    }

    #[test]
    fn rests_follow_the_timeline() {
        let mut timeline = Timeline::default();
        timeline.cached_duration = 10.0;
        timeline.current_time = 4.0;
        timeline.is_playing = true;
        assert_eq!(rest_of(&timeline), None);
        timeline.is_playing = false;
        assert_eq!(rest_of(&timeline), Some(Rest::At(4.0)));
        timeline.seek_request = Some(0.0);
        assert_eq!(rest_of(&timeline), None);
        timeline.seek_request = None;
        timeline.current_time = 10.0;
        assert_eq!(rest_of(&timeline), Some(Rest::End));
    }

    #[test]
    fn auto_waits_are_seconds_unless_given_units() {
        assert_eq!(parse_auto_wait("8"), Ok(Duration::from_secs(8)));
        assert_eq!(parse_auto_wait("8s"), Ok(Duration::from_secs(8)));
        assert_eq!(parse_auto_wait("1m30s"), Ok(Duration::from_secs(90)));
        assert_eq!(parse_auto_wait("0:45"), Ok(Duration::from_secs(45)));
        assert!(parse_auto_wait("0").is_err());
        assert!(parse_auto_wait("soon").is_err());
    }
}
