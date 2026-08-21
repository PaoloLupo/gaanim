//! Renderer health and frame diagnostics exposed to Gaanim hosts.
//!
//! Bevy 0.19 reports device loss and uncaptured wgpu errors through a
//! [`bevy::render::error_handler::RenderErrorHandler`].  The default handler
//! terminates the app.  The editor instead keeps the authored scene alive,
//! records a user-visible failure, and recreates the render device after a
//! device-loss event.

use bevy::diagnostic::DiagnosticsStore;
use bevy::prelude::*;
use bevy::render::error_handler::{ErrorType, RenderError, RenderErrorHandler, RenderErrorPolicy};
use bevy::render::settings::RenderCreation;

use bevy_vello::render::diagnostics::{
    CLIPS_COUNT, OPEN_CLIPS_COUNT, PATH_COUNT, PATH_SEGMENTS_COUNT, UI_SCENE_COUNT,
    WORLD_SCENE_COUNT,
};

/// The kind of a renderer failure reported by Bevy/wgpu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenderFailureKind {
    DeviceLost,
    OutOfMemory,
    Validation,
    Internal,
}

impl From<ErrorType> for RenderFailureKind {
    fn from(value: ErrorType) -> Self {
        match value {
            ErrorType::DeviceLost => Self::DeviceLost,
            ErrorType::OutOfMemory => Self::OutOfMemory,
            ErrorType::Validation => Self::Validation,
            ErrorType::Internal => Self::Internal,
        }
    }
}

impl RenderFailureKind {
    /// A concise label suitable for a host UI.
    pub const fn label(self) -> &'static str {
        match self {
            Self::DeviceLost => "GPU device lost",
            Self::OutOfMemory => "GPU out of memory",
            Self::Validation => "GPU validation error",
            Self::Internal => "internal GPU error",
        }
    }
}

/// The most recent renderer failure and recovery state.
///
/// This resource is intentionally engine-facing: hosts such as the editor can
/// render it in their own UI without coupling script authors to Bevy ECS.
#[derive(Resource, Debug, Clone, Default)]
pub struct RenderHealth {
    /// Most recently reported failure, if rendering is degraded or recovering.
    pub last_failure: Option<RenderFailure>,
    /// Number of device-loss recoveries initiated for this process.
    pub recovery_count: u32,
    retry_requested: bool,
}

/// A captured renderer failure that leaves the application world intact.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderFailure {
    pub kind: RenderFailureKind,
    pub description: String,
}

impl RenderHealth {
    /// Ask the handler to create a fresh Bevy render device on its next poll.
    ///
    /// This is deliberately opt-in for out-of-memory, validation, and internal
    /// errors: retrying those automatically can produce a flashing loop.
    pub fn request_retry(&mut self) {
        self.retry_requested = true;
    }

    /// Clear a displayed failure after the host has acknowledged it.
    ///
    /// This does not restart rendering; use [`Self::request_retry`] for that.
    pub fn clear_failure(&mut self) {
        self.last_failure = None;
    }

    fn record(&mut self, error: &RenderError) {
        let failure = RenderFailure {
            kind: error.ty.into(),
            description: error.description.clone(),
        };
        if self.last_failure.as_ref() != Some(&failure) {
            self.last_failure = Some(failure);
        }
    }
}

/// Per-frame Vello scene complexity measured by `bevy_vello`.
///
/// Values are from the most recently completed render extraction and are thus
/// normally one frame behind the editor UI. `None` means no rendered frame has
/// published that measurement yet.
#[derive(Resource, Debug, Clone, Default, PartialEq)]
pub struct VelloDiagnostics {
    pub world_scenes: Option<u32>,
    pub ui_scenes: Option<u32>,
    pub paths: Option<u32>,
    pub path_segments: Option<u32>,
    pub clips: Option<u32>,
    pub open_clips: Option<u32>,
}

fn latest(diagnostics: &DiagnosticsStore, path: &bevy::diagnostic::DiagnosticPath) -> Option<u32> {
    diagnostics
        .get_measurement(path)
        .and_then(|measurement| (measurement.value >= 0.0).then_some(measurement.value as u32))
}

/// Copy Vello's published diagnostics into a compact, host-facing resource.
pub fn collect_vello_diagnostics_system(
    diagnostics: Option<Res<DiagnosticsStore>>,
    mut vello: ResMut<VelloDiagnostics>,
) {
    let Some(diagnostics) = diagnostics else {
        return;
    };
    vello.world_scenes = latest(&diagnostics, &WORLD_SCENE_COUNT);
    vello.ui_scenes = latest(&diagnostics, &UI_SCENE_COUNT);
    vello.paths = latest(&diagnostics, &PATH_COUNT);
    vello.path_segments = latest(&diagnostics, &PATH_SEGMENTS_COUNT);
    vello.clips = latest(&diagnostics, &CLIPS_COUNT);
    vello.open_clips = latest(&diagnostics, &OPEN_CLIPS_COUNT);
}

/// Install Gaanim's non-destructive Bevy render-error policy.
pub fn install_render_error_handler(app: &mut App) {
    app.insert_resource(RenderErrorHandler(handle_render_error));
}

fn handle_render_error(
    error: &RenderError,
    main_world: &mut World,
    _render_world: &mut World,
) -> RenderErrorPolicy {
    let health = main_world.resource_mut::<RenderHealth>().into_inner();
    health.record(error);

    // A lost device has no useful resources left, and Bevy 0.19 can recreate
    // them without discarding the main ECS world. Recover it automatically.
    if matches!(error.ty, ErrorType::DeviceLost) {
        health.recovery_count = health.recovery_count.saturating_add(1);
        health.retry_requested = false;
        health.last_failure = None;
        return RenderErrorPolicy::Recover(RenderCreation::default());
    }

    // Retrying OOM/validation/internal failures without a user decision can
    // enter a rapid error/render loop. Keep the authored session alive and
    // wait for the host's explicit retry request instead.
    if health.retry_requested {
        health.retry_requested = false;
        health.recovery_count = health.recovery_count.saturating_add(1);
        health.last_failure = None;
        RenderErrorPolicy::Recover(RenderCreation::default())
    } else {
        RenderErrorPolicy::StopRendering
    }
}

#[cfg(test)]
mod tests {
    use super::{RenderFailure, RenderFailureKind, RenderHealth, handle_render_error};
    use bevy::prelude::World;
    use bevy::render::error_handler::{ErrorType, RenderError, RenderErrorPolicy};

    #[test]
    fn retry_request_is_explicit_and_clear_does_not_request_a_retry() {
        let mut health = RenderHealth {
            last_failure: Some(RenderFailure {
                kind: RenderFailureKind::OutOfMemory,
                description: "allocation failed".into(),
            }),
            ..Default::default()
        };
        health.clear_failure();
        assert!(health.last_failure.is_none());
        assert!(!health.retry_requested);

        health.request_retry();
        assert!(health.retry_requested);
    }

    #[test]
    fn device_loss_recovers_automatically_without_leaving_a_stale_failure() {
        let mut main_world = World::new();
        main_world.init_resource::<RenderHealth>();
        let mut render_world = World::new();
        let policy = handle_render_error(
            &RenderError {
                ty: ErrorType::DeviceLost,
                description: "driver reset".into(),
                source: None,
            },
            &mut main_world,
            &mut render_world,
        );

        assert!(matches!(policy, RenderErrorPolicy::Recover(_)));
        let health = main_world.resource::<RenderHealth>();
        assert_eq!(health.recovery_count, 1);
        assert!(health.last_failure.is_none());
    }

    #[test]
    fn stopped_renderer_recovers_after_an_explicit_retry() {
        let mut main_world = World::new();
        main_world.init_resource::<RenderHealth>();
        let mut render_world = World::new();
        let error = RenderError {
            ty: ErrorType::OutOfMemory,
            description: "allocation failed".into(),
            source: None,
        };

        assert!(matches!(
            handle_render_error(&error, &mut main_world, &mut render_world),
            RenderErrorPolicy::StopRendering
        ));
        main_world.resource_mut::<RenderHealth>().request_retry();
        assert!(matches!(
            handle_render_error(&error, &mut main_world, &mut render_world),
            RenderErrorPolicy::Recover(_)
        ));
        let health = main_world.resource::<RenderHealth>();
        assert_eq!(health.recovery_count, 1);
        assert!(health.last_failure.is_none());
    }
}
