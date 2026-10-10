//! How long opening a script took, phase by phase, printed with
//! `GAANIM_RELOAD_PROFILE=1` once its first scene is ready.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

static STARTED: OnceLock<Instant> = OnceLock::new();
static PHASES: Mutex<Vec<(&'static str, Duration)>> = Mutex::new(Vec::new());
static REPORTED: AtomicBool = AtomicBool::new(false);

/// Start the clock; the process calls it first.
pub fn begin() {
    STARTED.get_or_init(Instant::now);
}

/// Record that `phase` ended now.
pub fn mark(phase: &'static str) {
    let Some(started) = STARTED.get() else {
        return;
    };
    if let Ok(mut phases) = PHASES.lock() {
        phases.push((phase, started.elapsed()));
    }
}

/// Mark `phase` and print every phase, once per process, with the profile.
pub fn report(phase: &'static str) {
    if REPORTED.swap(true, Ordering::AcqRel) {
        return;
    }
    mark(phase);
    if !gaanim_api::canvas::reload_profile_enabled() {
        return;
    }
    let Ok(phases) = PHASES.lock() else {
        return;
    };
    gaanim_core::console::info("profile", format!("startup: {}", describe(&phases)));
}

/// Each phase with the time since the previous one.
fn describe(phases: &[(&str, Duration)]) -> String {
    let mut previous = Duration::ZERO;
    phases
        .iter()
        .map(|(phase, at)| {
            let took = at.saturating_sub(previous);
            previous = *at;
            format!("{phase} {:.0} ms", took.as_secs_f64() * 1000.0)
        })
        .collect::<Vec<_>>()
        .join(" · ")
}

/// System: mark the first frame, once the window and the GPU are up.
pub fn first_frame_system(mut seen: bevy::prelude::Local<bool>) {
    if !*seen {
        *seen = true;
        mark("first frame");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn phases_report_the_time_each_took() {
        let phases = [
            ("python", Duration::from_millis(120)),
            ("app", Duration::from_millis(500)),
            ("first frame", Duration::from_millis(1400)),
        ];
        assert_eq!(
            describe(&phases),
            "python 120 ms · app 380 ms · first frame 900 ms"
        );
    }
}
