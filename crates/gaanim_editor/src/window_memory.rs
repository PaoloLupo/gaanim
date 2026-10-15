//! The main window opens where it was left: its size and position, saved in
//! the `window` section of the user's settings once a move or resize
//! settles. A saved position that no monitor shows any more (a screen that
//! was unplugged) is dropped and the window is centred instead.

use bevy::platform::time::Instant;
use bevy::prelude::*;
use bevy::window::{Monitor, PrimaryWindow, WindowMode, WindowPosition};
use serde::{Deserialize, Serialize};

use crate::PresentationMode;

/// A move or resize is written once the window rests this long.
const SETTLE_SECONDS: f32 = 1.0;

/// The smallest window restored, in logical pixels.
const MIN_SIZE: (f32, f32) = (640.0, 400.0);

/// The `window` section of the user's settings.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub(crate) struct WindowPreferences {
    /// Logical size.
    pub(crate) width: f32,
    pub(crate) height: f32,
    /// Top-left corner on the desktop, in physical pixels.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) position: Option<(i32, i32)>,
}

impl Default for WindowPreferences {
    fn default() -> Self {
        Self {
            width: 1280.0,
            height: 720.0,
            position: None,
        }
    }
}

impl WindowPreferences {
    const SECTION: &str = "window";

    pub(crate) fn load() -> Self {
        gaanim_project::settings::load::<Self>(Self::SECTION).usable()
    }

    /// At least [`MIN_SIZE`], so a window never opens too small to use.
    fn usable(mut self) -> Self {
        self.width = self.width.max(MIN_SIZE.0);
        self.height = self.height.max(MIN_SIZE.1);
        self
    }

    fn save(&self) -> Result<(), String> {
        gaanim_project::settings::save(Self::SECTION, self)
    }

    /// Where the window opens: the saved corner, or centred.
    pub(crate) fn window_position(&self) -> WindowPosition {
        match self.position {
            Some((x, y)) => WindowPosition::At(IVec2::new(x, y)),
            None => WindowPosition::Centered(bevy::window::MonitorSelection::Primary),
        }
    }
}

/// Whether some monitor shows the top of a window at `corner`: enough of its
/// title bar to grab it.
fn on_a_monitor(corner: IVec2, monitors: &[(IVec2, UVec2)]) -> bool {
    let grab = corner + IVec2::new(80, 16);
    monitors.iter().any(|(origin, size)| {
        let end = *origin + size.as_ivec2();
        grab.x >= origin.x && grab.y >= origin.y && grab.x < end.x && grab.y < end.y
    })
}

/// The last geometry seen, waiting to be written.
#[derive(Resource, Default)]
pub(crate) struct WindowMemory {
    pending: Option<(WindowPreferences, Instant)>,
    saved: Option<WindowPreferences>,
    /// The saved position was checked against the monitors.
    checked: bool,
}

pub(crate) fn window_memory_system(
    mut memory: ResMut<WindowMemory>,
    mut windows: Query<&mut Window, With<PrimaryWindow>>,
    monitors: Query<&Monitor>,
    presentation: Res<PresentationMode>,
) {
    if crate::WEB {
        return;
    }
    let Ok(mut window) = windows.single_mut() else {
        return;
    };
    if !memory.checked {
        let monitors: Vec<(IVec2, UVec2)> = monitors
            .iter()
            .map(|monitor| (monitor.physical_position, monitor.physical_size()))
            .collect();
        if monitors.is_empty() {
            return;
        }
        memory.checked = true;
        if let WindowPosition::At(corner) = window.position
            && !on_a_monitor(corner, &monitors)
        {
            window.position = WindowPosition::Centered(bevy::window::MonitorSelection::Primary);
        }
        memory.saved = Some(current(&window));
        return;
    }
    // Full screen and presentations are not the window to come back to.
    if presentation.active || !matches!(window.mode, WindowMode::Windowed) {
        memory.pending = None;
        return;
    }
    let now = current(&window);
    if memory.saved == Some(now) {
        memory.pending = None;
        return;
    }
    match memory.pending {
        Some((pending, since)) if pending == now => {
            if since.elapsed().as_secs_f32() >= SETTLE_SECONDS {
                if let Err(error) = now.save() {
                    gaanim_core::console::warn("window", format!("not saved: {error}"));
                }
                memory.saved = Some(now);
                memory.pending = None;
            }
        }
        _ => memory.pending = Some((now, Instant::now())),
    }
}

fn current(window: &Window) -> WindowPreferences {
    WindowPreferences {
        width: window.resolution.width(),
        height: window.resolution.height(),
        position: match window.position {
            WindowPosition::At(corner) => Some((corner.x, corner.y)),
            _ => None,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_window_needs_its_title_bar_on_a_monitor() {
        let monitors = [(IVec2::ZERO, UVec2::new(1920, 1080))];
        assert!(on_a_monitor(IVec2::new(100, 100), &monitors));
        assert!(!on_a_monitor(IVec2::new(100, -400), &monitors));
        assert!(!on_a_monitor(IVec2::new(2500, 100), &monitors));
        let two = [
            (IVec2::ZERO, UVec2::new(1920, 1080)),
            (IVec2::new(1920, 0), UVec2::new(1920, 1080)),
        ];
        assert!(on_a_monitor(IVec2::new(2500, 100), &two));
    }

    #[test]
    fn small_or_missing_sizes_open_at_a_usable_size() {
        let saved: WindowPreferences =
            serde_json::from_str(r#"{"width": 100, "height": 50}"#).unwrap();
        assert_eq!(saved.position, None);
        let usable = saved.usable();
        assert_eq!((usable.width, usable.height), MIN_SIZE);
        let empty: WindowPreferences = serde_json::from_str("{}").unwrap_or_default();
        assert_eq!(empty.width, 1280.0);
    }
}
