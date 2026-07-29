//! Presenter view hosted in a second native window.

use bevy::{
    camera::RenderTarget,
    ecs::schedule::ScheduleLabel,
    prelude::*,
    window::{WindowClosed, WindowCreated, WindowRef, WindowResolution},
};
use bevy_egui::{EguiContext, EguiMultipassSchedule, egui};
use crossbeam_channel::{Receiver, TryRecvError, bounded};
use gaanim_export::prelude::{AspectRatioPreset, ExportConfig, capture_scene_direct};
use gaanim_timeline::timeline::Timeline;
use std::collections::HashMap;

use crate::{AudienceBlank, PresentationMode, export::StashedReplay};

/// Dedicated egui schedule for the presenter window.
#[derive(ScheduleLabel, Clone, Debug, PartialEq, Eq, Hash)]
pub struct PresenterEguiPass;

#[derive(Component)]
pub(crate) struct PresenterWindow;

#[derive(Component)]
pub(crate) struct PresenterCamera {
    window: Entity,
}

/// Ephemeral controls for navigating a semantic presentation by slide name.
#[derive(Default)]
pub(crate) struct PresenterOverviewState {
    open: bool,
    query: String,
    textures: HashMap<u32, egui::TextureHandle>,
}

#[derive(Debug)]
struct ThumbnailPixels {
    slide_id: u32,
    width: u32,
    height: u32,
    rgba: Vec<u8>,
}

type ThumbnailResult = Result<Vec<ThumbnailPixels>, String>;

/// Async thumbnail state. Captures use a fresh headless world, so generating
/// the overview never seeks or mutates the audience's live presentation.
#[derive(Resource, Default)]
pub(crate) struct PresenterThumbnailCache {
    requested_revision: u64,
    receiver: Option<Receiver<(u64, ThumbnailResult)>>,
    error: Option<String>,
}

impl PresenterThumbnailCache {
    fn request(&mut self, stash: &StashedReplay, timeline: &Timeline) {
        if stash.revision == 0
            || stash.revision == self.requested_revision
            || timeline.presentation.is_empty()
        {
            return;
        }
        let Some(canvas) = stash.canvas.clone() else {
            return;
        };

        let revision = stash.revision;
        let slide_ids = timeline
            .presentation
            .iter()
            .map(|slide| slide.id)
            .collect::<Vec<_>>();
        let times = timeline
            .presentation
            .iter()
            .map(|slide| representative_slide_time(slide.start_time, slide.end_time))
            .collect::<Vec<_>>();
        let (width, height) = thumbnail_dimensions(canvas.width, canvas.height);
        let (sender, receiver) = bounded(1);

        self.requested_revision = revision;
        self.receiver = Some(receiver);
        self.error = None;

        let spawn_result = std::thread::Builder::new()
            .name("gaanim-presenter-thumbnails".to_string())
            .spawn(move || {
                let mut config = ExportConfig::new("presenter-thumbnails.png");
                config.width = width;
                config.height = height;
                config.aspect_ratio = AspectRatioPreset::Custom;
                config.headless = true;
                let result = capture_scene_direct(config, &times, move |world| {
                    gaanim_api::runtime::replay_canvas_into(world, canvas)
                })
                .map(|frames| {
                    slide_ids
                        .into_iter()
                        .zip(frames)
                        .map(|(slide_id, frame)| ThumbnailPixels {
                            slide_id,
                            width: frame.width,
                            height: frame.height,
                            rgba: frame.rgba,
                        })
                        .collect()
                })
                .map_err(|error| error.to_string());
                let _ = sender.send((revision, result));
            });

        if let Err(error) = spawn_result {
            self.receiver = None;
            self.error = Some(format!("could not start thumbnail renderer: {error}"));
        }
    }

    fn receive(&mut self) -> Option<(u64, ThumbnailResult)> {
        let result = match self.receiver.as_ref()?.try_recv() {
            Ok(result) => Some(result),
            Err(TryRecvError::Empty) => None,
            Err(TryRecvError::Disconnected) => {
                self.error = Some("thumbnail renderer stopped unexpectedly".to_string());
                self.receiver = None;
                None
            }
        };
        if result.is_some() {
            self.receiver = None;
        }
        result
    }

    fn is_loading(&self) -> bool {
        self.receiver.is_some()
    }
}

fn thumbnail_dimensions(canvas_width: u32, canvas_height: u32) -> (u32, u32) {
    const MAX_EDGE: f64 = 320.0;
    let width = canvas_width.max(1) as f64;
    let height = canvas_height.max(1) as f64;
    let scale = MAX_EDGE / width.max(height);
    (
        (width * scale).round().max(1.0) as u32,
        (height * scale).round().max(1.0) as u32,
    )
}

fn representative_slide_time(start_time: f64, end_time: f64) -> f64 {
    if end_time > start_time + 1e-4 {
        (end_time - 1e-4).max(start_time)
    } else {
        start_time
    }
}

fn apply_presenter_style(ctx: &egui::Context) {
    use egui::{Color32, FontFamily, FontId, TextStyle};

    let mut style = (*ctx.style()).clone();
    style.spacing.item_spacing = egui::vec2(12.0, 10.0);
    style.spacing.button_padding = egui::vec2(16.0, 10.0);
    style.spacing.window_margin = egui::Margin::same(18);
    style.visuals = egui::Visuals::dark();
    style.visuals.panel_fill = Color32::from_rgb(7, 11, 22);
    style.visuals.window_fill = Color32::from_rgb(13, 20, 36);
    style.visuals.extreme_bg_color = Color32::from_rgb(5, 8, 16);
    style.visuals.faint_bg_color = Color32::from_rgb(17, 26, 46);
    style.visuals.widgets.inactive.bg_fill = Color32::from_rgb(22, 33, 57);
    style.visuals.widgets.hovered.bg_fill = Color32::from_rgb(39, 58, 92);
    style.visuals.widgets.active.bg_fill = Color32::from_rgb(56, 83, 132);
    style.visuals.selection.bg_fill = Color32::from_rgb(69, 105, 174);
    style.visuals.hyperlink_color = Color32::from_rgb(110, 168, 254);
    style.text_styles.insert(
        TextStyle::Heading,
        FontId::new(30.0, FontFamily::Proportional),
    );
    style
        .text_styles
        .insert(TextStyle::Body, FontId::new(18.0, FontFamily::Proportional));
    style.text_styles.insert(
        TextStyle::Button,
        FontId::new(16.0, FontFamily::Proportional),
    );
    style.text_styles.insert(
        TextStyle::Small,
        FontId::new(14.0, FontFamily::Proportional),
    );
    style.text_styles.insert(
        TextStyle::Monospace,
        FontId::new(17.0, FontFamily::Monospace),
    );
    ctx.set_style(style);
}

/// Create the speaker-facing window only for `gaanim --present`.
pub(crate) fn spawn_presenter_window_system(
    presentation_mode: Res<PresentationMode>,
    mut commands: Commands,
) {
    if !presentation_mode.active {
        return;
    }

    spawn_presenter_window(&mut commands);
}

/// Spawn the speaker-facing window from either startup or an editor command.
pub(crate) fn spawn_presenter_window(commands: &mut Commands) {
    commands.spawn((
        Window {
            title: "Gaanim — Presenter View".to_string(),
            resolution: WindowResolution::new(960, 640),
            resizable: true,
            ..default()
        },
        PresenterWindow,
    ));
}

/// Attach/detach the egui camera in lockstep with the native winit window.
///
/// Waiting for `WindowCreated` prevents bevy_egui from observing a render
/// target before winit can resolve it.
pub(crate) fn sync_presenter_camera_system(
    mut created: MessageReader<WindowCreated>,
    mut closed: MessageReader<WindowClosed>,
    presenter_windows: Query<(), With<PresenterWindow>>,
    presenter_cameras: Query<(Entity, &PresenterCamera)>,
    mut commands: Commands,
) {
    for event in created.read() {
        if presenter_windows.get(event.window).is_err()
            || presenter_cameras
                .iter()
                .any(|(_, camera)| camera.window == event.window)
        {
            continue;
        }
        commands.spawn((
            Camera3d::default(),
            Camera::default(),
            bevy::core_pipeline::tonemapping::Tonemapping::None,
            RenderTarget::Window(WindowRef::Entity(event.window)),
            EguiMultipassSchedule::new(PresenterEguiPass),
            PresenterCamera {
                window: event.window,
            },
        ));
    }

    for event in closed.read() {
        for (entity, camera) in &presenter_cameras {
            if camera.window == event.window {
                commands.entity(entity).despawn();
            }
        }
    }
}

/// Speaker-facing controls and semantic presentation information.
pub(crate) fn presenter_view_system(
    mut contexts: Query<&mut EguiContext, With<PresenterCamera>>,
    mut timeline: ResMut<Timeline>,
    mut audience_blank: ResMut<AudienceBlank>,
    replay_stash: Res<StashedReplay>,
    mut thumbnail_cache: ResMut<PresenterThumbnailCache>,
    mut overview: Local<PresenterOverviewState>,
) {
    let Ok(mut context) = contexts.single_mut() else {
        return;
    };
    let ctx = context.get_mut();
    apply_presenter_style(ctx);
    if overview.open {
        thumbnail_cache.request(&replay_stash, &timeline);
    }
    if let Some((revision, result)) = thumbnail_cache.receive()
        && revision == replay_stash.revision
    {
        overview.textures.clear();
        match result {
            Ok(frames) => {
                for frame in frames {
                    let image = egui::ColorImage::from_rgba_unmultiplied(
                        [frame.width as usize, frame.height as usize],
                        &frame.rgba,
                    );
                    let texture = ctx.load_texture(
                        format!("presenter-slide-{}-{revision}", frame.slide_id),
                        image,
                        egui::TextureOptions::LINEAR,
                    );
                    overview.textures.insert(frame.slide_id, texture);
                }
                thumbnail_cache.error = None;
            }
            Err(error) => thumbnail_cache.error = Some(error),
        }
    }
    let current_time = timeline.current_time;
    let current_position = timeline.presentation_position_at(current_time);
    let current = current_position.and_then(|position| {
        timeline
            .presentation
            .iter()
            .position(|slide| slide.id == position.slide_id)
            .map(|index| (index, position))
    });

    let format_time = |seconds: f64| {
        let seconds = seconds.max(0.0).round() as u64;
        format!("{:02}:{:02}", seconds / 60, seconds % 60)
    };

    if !overview.open && !ctx.wants_keyboard_input() {
        let previous_pressed = ctx.input(|input| input.key_pressed(egui::Key::ArrowLeft));
        let next_pressed = ctx.input(|input| input.key_pressed(egui::Key::ArrowRight));
        let toggle_playback = ctx.input(|input| input.key_pressed(egui::Key::Space));
        let open_overview = ctx.input(|input| input.key_pressed(egui::Key::O));
        let black_screen = ctx.input(|input| input.key_pressed(egui::Key::B));
        let white_screen = ctx.input(|input| input.key_pressed(egui::Key::W));

        if previous_pressed {
            timeline.is_playing = false;
            timeline.seek_request = Some(
                timeline
                    .previous_presentation_stop(current_time)
                    .unwrap_or(0.0),
            );
        }
        if next_pressed {
            timeline.is_playing = true;
        }
        if toggle_playback {
            timeline.is_playing = !timeline.is_playing;
        }
        if open_overview {
            overview.open = true;
        }
        if black_screen {
            *audience_blank = if *audience_blank == AudienceBlank::Black {
                AudienceBlank::None
            } else {
                AudienceBlank::Black
            };
        }
        if white_screen {
            *audience_blank = if *audience_blank == AudienceBlank::White {
                AudienceBlank::None
            } else {
                AudienceBlank::White
            };
        }
    }

    let mut requested_seek = None;

    egui::CentralPanel::default()
        .frame(
            egui::Frame::new()
                .fill(egui::Color32::from_rgb(7, 11, 22))
                .inner_margin(egui::Margin::same(24)),
        )
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new("GAANIM")
                        .strong()
                        .size(16.0)
                        .color(egui::Color32::from_rgb(255, 209, 102)),
                );
                ui.separator();
                ui.label(
                    egui::RichText::new("PRESENTER VIEW")
                        .strong()
                        .size(16.0)
                        .color(egui::Color32::from_rgb(158, 172, 195)),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.monospace(format!(
                        "{}  /  {}",
                        format_time(current_time),
                        format_time(timeline.cached_duration)
                    ));
                });
            });
            let progress = if timeline.cached_duration > 0.0 {
                (current_time / timeline.cached_duration) as f32
            } else {
                0.0
            };
            ui.add(
                egui::ProgressBar::new(progress.clamp(0.0, 1.0))
                    .desired_height(6.0)
                    .fill(egui::Color32::from_rgb(91, 143, 255)),
            );
            ui.add_space(10.0);
            match *audience_blank {
                AudienceBlank::Black => {
                    ui.colored_label(egui::Color32::YELLOW, "Audience screen is BLACK (B)");
                    ui.separator();
                }
                AudienceBlank::White => {
                    ui.colored_label(egui::Color32::YELLOW, "Audience screen is WHITE (W)");
                    ui.separator();
                }
                AudienceBlank::None => {}
            }

            if let Some((index, position)) = current {
                let slide = &timeline.presentation[index];
                let step_label = position
                    .step_index
                    .map(|step| format!(" · Step {}", step + 1))
                    .unwrap_or_default();
                ui.heading(format!(
                    "Current · {} / {} · {}{}",
                    index + 1,
                    timeline.presentation.len(),
                    slide.name,
                    step_label
                ));
                ui.label(format!(
                    "Slide time {}",
                    format_time(current_time - slide.start_time)
                ));
                ui.add_space(8.0);
                ui.label(egui::RichText::new("Notes").strong());
                ui.label(slide.notes.as_deref().unwrap_or("No notes for this slide."));

                ui.add_space(16.0);
                ui.label(egui::RichText::new("Next").strong());
                if let Some(next_time) = timeline.next_presentation_stop(current_time) {
                    if let Some(next_position) = timeline.presentation_position_at(next_time)
                        && let Some(next_index) = timeline
                            .presentation
                            .iter()
                            .position(|slide| slide.id == next_position.slide_id)
                    {
                        let next = &timeline.presentation[next_index];
                        let next_step = next_position
                            .step_index
                            .map(|step| format!(" · Step {}", step + 1))
                            .unwrap_or_default();
                        ui.label(format!("{}{}", next.name, next_step));
                    }
                } else {
                    ui.label("End of presentation");
                }
            } else {
                ui.label("No semantic slides are defined in this scene.");
            }

            ui.with_layout(egui::Layout::bottom_up(egui::Align::Center), |ui| {
                ui.separator();
                ui.horizontal(|ui| {
                    if ui.button("◀ Previous").clicked() {
                        timeline.is_playing = false;
                        timeline.seek_request = Some(
                            timeline
                                .previous_presentation_stop(current_time)
                                .unwrap_or(0.0),
                        );
                    }
                    let label = if timeline.is_playing { "Pause" } else { "Play" };
                    if ui.button(label).clicked() {
                        timeline.is_playing = !timeline.is_playing;
                    }
                    if ui.button("Overview").clicked() {
                        overview.open = true;
                    }
                    if ui.button("Black (B)").clicked() {
                        *audience_blank = if *audience_blank == AudienceBlank::Black {
                            AudienceBlank::None
                        } else {
                            AudienceBlank::Black
                        };
                    }
                    if ui.button("White (W)").clicked() {
                        *audience_blank = if *audience_blank == AudienceBlank::White {
                            AudienceBlank::None
                        } else {
                            AudienceBlank::White
                        };
                    }
                    if ui.button("Next ▶").clicked() {
                        timeline.is_playing = true;
                    }
                });
            });
        });

    if overview.open {
        let mut is_open = overview.open;
        egui::Window::new("Slide overview")
            .open(&mut is_open)
            .resizable(true)
            .default_width(620.0)
            .show(ctx, |ui| {
                thumbnail_cache.request(&replay_stash, &timeline);
                ui.label("Jump to a slide by name");
                ui.text_edit_singleline(&mut overview.query);
                ui.add_space(8.0);

                if thumbnail_cache.is_loading() {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label("Rendering slide previews...");
                    });
                    ui.add_space(8.0);
                } else if let Some(error) = &thumbnail_cache.error {
                    ui.colored_label(
                        egui::Color32::from_rgb(255, 140, 140),
                        format!("Could not render previews: {error}"),
                    );
                    ui.add_space(8.0);
                }

                let query = overview.query.trim().to_lowercase();
                egui::ScrollArea::vertical().show(ui, |ui| {
                    for (index, slide) in timeline.presentation.iter().enumerate() {
                        if !query.is_empty() && !slide.name.to_lowercase().contains(&query) {
                            continue;
                        }

                        let active =
                            current_position.is_some_and(|position| position.slide_id == slide.id);
                        let step_count = slide.steps.len();
                        let mut clicked = false;
                        egui::Frame::group(ui.style())
                            .fill(if active {
                                egui::Color32::from_rgb(31, 50, 84)
                            } else {
                                egui::Color32::from_rgb(13, 20, 36)
                            })
                            .inner_margin(egui::Margin::same(10))
                            .show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    if let Some(texture) = overview.textures.get(&slide.id) {
                                        let texture_size = texture.size_vec2();
                                        let scale =
                                            (190.0 / texture_size.x).min(140.0 / texture_size.y);
                                        clicked |= ui
                                            .add(
                                                egui::Image::new(texture)
                                                    .fit_to_exact_size(texture_size * scale)
                                                    .sense(egui::Sense::click()),
                                            )
                                            .clicked();
                                    } else {
                                        let (rect, response) = ui.allocate_exact_size(
                                            egui::vec2(190.0, 107.0),
                                            egui::Sense::click(),
                                        );
                                        ui.painter().rect_filled(
                                            rect,
                                            6.0,
                                            egui::Color32::from_rgb(5, 8, 16),
                                        );
                                        clicked |= response.clicked();
                                    }
                                    ui.vertical(|ui| {
                                        ui.label(
                                            egui::RichText::new(format!(
                                                "{:02}  {}",
                                                index + 1,
                                                slide.name
                                            ))
                                            .strong()
                                            .size(18.0),
                                        );
                                        ui.label(format!(
                                            "{} step{}",
                                            step_count,
                                            if step_count == 1 { "" } else { "s" }
                                        ));
                                        clicked |= ui.button("Go to slide").clicked();
                                        if !slide.steps.is_empty() {
                                            ui.horizontal_wrapped(|ui| {
                                                for (step_index, step) in
                                                    slide.steps.iter().enumerate()
                                                {
                                                    let label = step
                                                        .name
                                                        .as_deref()
                                                        .map(str::to_owned)
                                                        .unwrap_or_else(|| {
                                                            format!("Step {}", step_index + 1)
                                                        });
                                                    if ui.small_button(label).clicked() {
                                                        requested_seek = timeline
                                                            .presentation_time_indexed(
                                                                &slide.name,
                                                                Some(step_index),
                                                            );
                                                    }
                                                }
                                            });
                                        }
                                    });
                                });
                            });
                        ui.add_space(8.0);
                        if clicked {
                            requested_seek = timeline.presentation_time_named(&slide.name);
                        }
                    }
                });
            });
        overview.open = is_open && requested_seek.is_none();
    }

    if let Some(time) = requested_seek {
        timeline.is_playing = false;
        timeline.seek_request = Some(time);
    }
}

#[cfg(test)]
mod tests {
    use super::{representative_slide_time, thumbnail_dimensions};

    #[test]
    fn thumbnail_dimensions_preserve_common_aspect_ratios() {
        assert_eq!(thumbnail_dimensions(1920, 1080), (320, 180));
        assert_eq!(thumbnail_dimensions(1080, 1920), (180, 320));
        assert_eq!(thumbnail_dimensions(0, 0), (320, 320));
    }

    #[test]
    fn representative_time_stays_inside_the_slide() {
        assert_eq!(representative_slide_time(2.0, 2.0), 2.0);
        let time = representative_slide_time(2.0, 5.0);
        assert!(time >= 2.0 && time < 5.0);
    }
}
