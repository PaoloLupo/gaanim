//! Saving the frame the preview shows as a PNG: `Ctrl+Shift+S` (`Cmd` on
//! macOS) or the overlays bar. The frame renders on a worker thread from the
//! scene or bundle on screen, at the size an export would use, so the preview
//! keeps playing; editor overlays never appear in it.

use std::ops::ControlFlow;
use std::path::{Path, PathBuf};

use bevy::prelude::*;
use bevy_egui::{EguiContexts, egui, input::EguiWantsInput};
use crossbeam_channel::{Receiver, TryRecvError};
use gaanim_export::prelude::{
    AspectRatioPreset, CapturedFrame, ExportConfig, capture_bundle_streaming,
    capture_scene_direct_streaming, write_png_frame,
};
use gaanim_timeline::timeline::Timeline;

use crate::PresentationMode;
use crate::export::{ProjectPaths, StashedReplay};
use crate::ui_kit::{self, Icon, palette};

/// Seconds the result stays on screen.
const NOTICE_SECONDS: f64 = 4.0;

/// A capture asked for, running, or just finished.
#[derive(Resource, Default)]
pub struct FrameCapture {
    /// Set by the shortcut or the overlays bar; the next frame starts it.
    pub requested: bool,
    job: Option<Receiver<Result<PathBuf, String>>>,
    /// The last result and when it arrived (seconds since startup).
    notice: Option<(Result<PathBuf, String>, f64)>,
}

impl FrameCapture {
    pub fn busy(&self) -> bool {
        self.job.is_some()
    }
}

/// Whether this build can capture: the web player has no threads or files.
pub const AVAILABLE: bool = !crate::WEB;

/// `Ctrl+Shift+S` (`Cmd+Shift+S`) asks for a capture.
pub fn frame_capture_keys_system(
    egui_wants: Res<EguiWantsInput>,
    keys: Res<ButtonInput<KeyCode>>,
    presentation: Res<PresentationMode>,
    mut capture: ResMut<FrameCapture>,
) {
    if !AVAILABLE || presentation.active || egui_wants.wants_keyboard_input() {
        return;
    }
    let command = keys.any_pressed([
        KeyCode::ControlLeft,
        KeyCode::ControlRight,
        KeyCode::SuperLeft,
        KeyCode::SuperRight,
    ]);
    let shift = keys.any_pressed([KeyCode::ShiftLeft, KeyCode::ShiftRight]);
    if command && shift && keys.just_pressed(KeyCode::KeyS) {
        capture.requested = true;
    }
}

/// Starts a requested capture and collects its result.
pub fn frame_capture_system(
    mut capture: ResMut<FrameCapture>,
    timeline: Res<Timeline>,
    stash: Res<StashedReplay>,
    project: Option<Res<ProjectPaths>>,
    time: Res<Time>,
) {
    if let Some(job) = &capture.job {
        match job.try_recv() {
            Ok(result) => {
                match &result {
                    Ok(path) => gaanim_core::console::success(
                        "capture",
                        format!("Saved {}", gaanim_core::console::display_path(path)),
                    ),
                    Err(error) => gaanim_core::console::warn("capture", error),
                }
                capture.notice = Some((result, time.elapsed_secs_f64()));
                capture.job = None;
            }
            Err(TryRecvError::Empty) => {}
            Err(TryRecvError::Disconnected) => {
                capture.notice = Some((
                    Err("the capture stopped unexpectedly".to_owned()),
                    time.elapsed_secs_f64(),
                ));
                capture.job = None;
            }
        }
    }
    if !std::mem::take(&mut capture.requested) || capture.busy() || !AVAILABLE {
        return;
    }
    let source = match (&stash.bundle, &stash.canvas) {
        (Some(bundle), _) => Source::Bundle(bundle.path.clone(), bundle.size),
        (None, Some(canvas)) => Source::Scene(Box::new(canvas.clone())),
        (None, None) => {
            capture.notice = Some((
                Err("no scene to capture yet".to_owned()),
                time.elapsed_secs_f64(),
            ));
            return;
        }
    };
    let (folder, stem) = match project.as_deref() {
        Some(paths) => (
            paths.output_dir.clone(),
            paths
                .script_path
                .file_stem()
                .map(|stem| stem.to_string_lossy().into_owned()),
        ),
        None => (
            std::env::current_dir().unwrap_or_default(),
            stash
                .bundle
                .as_ref()
                .and_then(|bundle| bundle.path.file_stem())
                .map(|stem| stem.to_string_lossy().into_owned()),
        ),
    };
    let path = capture_path(
        &folder.join("captures"),
        stem.as_deref().unwrap_or("frame"),
        timeline.current_time,
    );
    let at = timeline.current_time;
    let (sender, receiver) = crossbeam_channel::bounded(1);
    let spawned = std::thread::Builder::new()
        .name("gaanim-frame-capture".to_owned())
        .spawn(move || {
            let _ = sender.send(render_frame(source, at, &path).map(|()| path));
        });
    match spawned {
        Ok(_) => capture.job = Some(receiver),
        Err(error) => {
            capture.notice = Some((
                Err(format!("could not start the capture: {error}")),
                time.elapsed_secs_f64(),
            ));
        }
    }
}

/// What a capture renders from.
enum Source {
    Scene(Box<gaanim_api::canvas::SceneModel>),
    /// A playback bundle and its recorded size.
    Bundle(PathBuf, (u32, u32)),
}

/// Render `source` at `at` seconds into a PNG at `path`.
fn render_frame(source: Source, at: f64, path: &Path) -> Result<(), String> {
    if let Some(folder) = path.parent() {
        std::fs::create_dir_all(folder)
            .map_err(|error| format!("could not create {}: {error}", folder.display()))?;
    }
    let mut written = None;
    let on_frame = |frame: CapturedFrame| {
        written = Some(
            write_png_frame(path, frame.rgba, frame.width, frame.height, false)
                .map_err(|error| error.to_string()),
        );
        ControlFlow::Break(())
    };
    let times = [at];
    match source {
        Source::Scene(canvas) => {
            let (width, height) = canvas.frame.preview_pixel_size();
            let mut config = ExportConfig::new(&path.to_string_lossy());
            config.width = width;
            config.height = height;
            config.aspect_ratio = AspectRatioPreset::Custom;
            config.headless = true;
            capture_scene_direct_streaming(
                config,
                &times,
                move |world| gaanim_api::runtime::replay_canvas_into(world, *canvas),
                on_frame,
            )
        }
        Source::Bundle(bundle, (width, height)) => {
            capture_bundle_streaming(&bundle, width, height, &times, on_frame)
        }
    }
    .map_err(|error| error.to_string())?;
    written.unwrap_or_else(|| Err("the renderer produced no frame".to_owned()))
}

/// `folder/stem_2.35s.png`, numbered when that file already exists so a
/// capture never replaces an earlier one.
fn capture_path(folder: &Path, stem: &str, at: f64) -> PathBuf {
    let base = format!("{stem}_{:.2}s", at.max(0.0));
    let mut path = folder.join(format!("{base}.png"));
    let mut copy = 2;
    while path.exists() {
        path = folder.join(format!("{base}_{copy}.png"));
        copy += 1;
    }
    path
}

/// The result of the last capture, for a few seconds; a click opens the file.
pub fn frame_capture_notice_system(
    mut contexts: EguiContexts,
    mut capture: ResMut<FrameCapture>,
    presentation: Res<PresentationMode>,
    time: Res<Time>,
) {
    if presentation.active {
        return;
    }
    let now = time.elapsed_secs_f64();
    let busy = capture.busy();
    let shown = capture
        .notice
        .as_ref()
        .filter(|(_, at)| now - at < NOTICE_SECONDS)
        .map(|(result, _)| result.clone());
    if shown.is_none() && !busy {
        capture.notice = None;
        return;
    }
    let Ok(ctx) = contexts.ctx_mut() else {
        return;
    };
    let (text, color, file) = match (&shown, busy) {
        (_, true) => (
            "Guardando el fotograma…".to_owned(),
            palette::TEXT_MUTED,
            None,
        ),
        (Some(Ok(path)), false) => (
            format!(
                "Fotograma guardado · {}",
                path.file_name()
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_default()
            ),
            palette::LOOP,
            Some(path.clone()),
        ),
        (Some(Err(error)), false) => (
            format!("No se pudo guardar el fotograma: {error}"),
            palette::DANGER,
            None,
        ),
        (None, false) => return,
    };
    egui::Area::new("frame_capture_notice".into())
        // Below the overlays bar, clear of the inspector and the reload badge.
        .anchor(egui::Align2::CENTER_TOP, egui::vec2(0.0, 60.0))
        .order(egui::Order::Foreground)
        .show(ctx, |ui| {
            let response = egui::Frame::new()
                .fill(palette::PANEL)
                .inner_margin(egui::Margin::symmetric(10, 6))
                .stroke(egui::Stroke::new(1.0, color.gamma_multiply(0.5)))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        let (rect, _) =
                            ui.allocate_exact_size(egui::vec2(14.0, 14.0), egui::Sense::hover());
                        ui_kit::paint_icon(ui.painter(), rect, Icon::Capture, color);
                        ui.label(egui::RichText::new(text).size(12.5).color(palette::TEXT));
                    });
                })
                .response;
            if let Some(file) = file {
                let response = response
                    .interact(egui::Sense::click())
                    .on_hover_cursor(egui::CursorIcon::PointingHand)
                    .on_hover_text(format!("{}\nClic: abrir", file.display()));
                if response.clicked()
                    && let Err(error) = crate::platform::open_detached(&file)
                {
                    gaanim_core::console::warn(
                        "capture",
                        format!("could not open {} · {error}", file.display()),
                    );
                }
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn captures_are_named_by_time_and_never_replace_each_other() {
        let folder = std::env::temp_dir().join(format!("gaanim-captures-{}", std::process::id()));
        std::fs::create_dir_all(&folder).unwrap();
        let first = capture_path(&folder, "main", 2.345);
        assert_eq!(first, folder.join("main_2.35s.png"));
        std::fs::write(&first, b"").unwrap();
        assert_eq!(
            capture_path(&folder, "main", 2.345),
            folder.join("main_2.35s_2.png")
        );
        assert_eq!(
            capture_path(&folder, "main", -1.0),
            folder.join("main_0.00s.png")
        );
        let _ = std::fs::remove_dir_all(&folder);
    }
}
