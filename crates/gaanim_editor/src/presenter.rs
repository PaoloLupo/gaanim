//! Presenter view hosted in a second native window.

use bevy::{
    camera::RenderTarget,
    ecs::schedule::ScheduleLabel,
    prelude::*,
    window::{WindowClosed, WindowCreated, WindowRef, WindowResolution},
};
use bevy_egui::{EguiContext, EguiMultipassSchedule, egui};
use gaanim_timeline::timeline::Timeline;

use crate::{AudienceBlank, PresentationMode};

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
    mut overview: Local<PresenterOverviewState>,
) {
    let Ok(mut context) = contexts.single_mut() else {
        return;
    };
    let ctx = context.get_mut();
    apply_presenter_style(ctx);
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
                ui.label("Jump to a slide by name");
                ui.text_edit_singleline(&mut overview.query);
                ui.add_space(8.0);

                let query = overview.query.trim().to_lowercase();
                egui::ScrollArea::vertical().show(ui, |ui| {
                    for (index, slide) in timeline.presentation.iter().enumerate() {
                        if !query.is_empty() && !slide.name.to_lowercase().contains(&query) {
                            continue;
                        }

                        let active =
                            current_position.is_some_and(|position| position.slide_id == slide.id);
                        let step_count = slide.steps.len();
                        let label = format!(
                            "{:02}  {}  · {} step{}",
                            index + 1,
                            slide.name,
                            step_count,
                            if step_count == 1 { "" } else { "s" }
                        );
                        if ui
                            .add_sized(
                                [560.0, 64.0],
                                egui::Button::new(label).selected(active).wrap(),
                            )
                            .clicked()
                        {
                            requested_seek = Some(slide.start_time);
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
