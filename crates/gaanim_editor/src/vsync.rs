use bevy::prelude::*;
use bevy::window::PresentMode;

#[derive(Resource)]
pub struct VsyncState {
    pub enabled: bool,
}

impl Default for VsyncState {
    fn default() -> Self {
        Self { enabled: true }
    }
}

pub fn vsync_toggle_system(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut windows: Query<&mut Window, With<bevy::window::PrimaryWindow>>,
    mut state: ResMut<VsyncState>,
) {
    if keyboard.just_pressed(KeyCode::F11) {
        state.enabled = !state.enabled;
        if let Ok(mut window) = windows.single_mut() {
            window.present_mode = if state.enabled {
                PresentMode::AutoVsync
            } else {
                PresentMode::AutoNoVsync
            };
        }
    }
}
