//! Native egui visual-diff viewer.

use std::path::{Path, PathBuf};

use eframe::egui::{self, Color32, TextureHandle, TextureOptions};

use crate::{CompareOptions, DiffReport, FrameStatus, compare_directories};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ViewMode {
    Baseline,
    Current,
    Diff,
}

impl ViewMode {
    fn label(self) -> &'static str {
        match self {
            Self::Baseline => "Baseline",
            Self::Current => "Actual",
            Self::Diff => "Diff",
        }
    }
}

#[derive(Default)]
struct FrameTextures {
    baseline: Option<TextureHandle>,
    current: Option<TextureHandle>,
    diff: Option<TextureHandle>,
}

impl FrameTextures {
    fn for_mode(&self, mode: ViewMode) -> Option<&TextureHandle> {
        match mode {
            ViewMode::Baseline => self.baseline.as_ref(),
            ViewMode::Current => self.current.as_ref(),
            ViewMode::Diff => self.diff.as_ref(),
        }
    }
}

pub fn run(
    report: DiffReport,
    baseline_dir: PathBuf,
    current_dir: PathBuf,
    report_dir: PathBuf,
    options: CompareOptions,
) -> eframe::Result<()> {
    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("gaanim diff")
            .with_inner_size([1440.0, 900.0])
            .with_min_inner_size([900.0, 600.0]),
        ..Default::default()
    };

    eframe::run_native(
        "gaanim diff",
        native_options,
        Box::new(move |creation_context| {
            Ok(Box::new(DiffViewer::new(
                creation_context,
                report,
                baseline_dir,
                current_dir,
                report_dir,
                options,
            )))
        }),
    )
}

struct DiffViewer {
    report: DiffReport,
    baseline_dir: PathBuf,
    current_dir: PathBuf,
    report_dir: PathBuf,
    textures: Vec<FrameTextures>,
    selected: usize,
    mode: ViewMode,
    only_failures: bool,
    fit_to_view: bool,
    zoom: f32,
    pixel_threshold: u8,
    max_changed_ratio: f64,
    generation: u64,
    error: Option<String>,
}

impl DiffViewer {
    fn new(
        creation_context: &eframe::CreationContext<'_>,
        report: DiffReport,
        baseline_dir: PathBuf,
        current_dir: PathBuf,
        report_dir: PathBuf,
        options: CompareOptions,
    ) -> Self {
        configure_style(&creation_context.egui_ctx);
        let textures = load_textures(&creation_context.egui_ctx, &report_dir, &report, 0);
        Self {
            report,
            baseline_dir,
            current_dir,
            report_dir,
            textures,
            selected: 0,
            mode: ViewMode::Diff,
            only_failures: false,
            fit_to_view: true,
            zoom: 1.0,
            pixel_threshold: options.pixel_threshold,
            max_changed_ratio: options.max_changed_ratio,
            generation: 0,
            error: None,
        }
    }

    fn visible_indices(&self) -> Vec<usize> {
        self.report
            .frames
            .iter()
            .enumerate()
            .filter_map(|(index, frame)| {
                (!self.only_failures || frame.status != FrameStatus::Unchanged).then_some(index)
            })
            .collect()
    }

    fn select_relative(&mut self, offset: isize) {
        let visible = self.visible_indices();
        if visible.is_empty() {
            return;
        }
        let position = visible
            .iter()
            .position(|index| *index == self.selected)
            .unwrap_or(0) as isize;
        let next = (position + offset).clamp(0, visible.len() as isize - 1) as usize;
        self.selected = visible[next];
    }

    fn recompute(&mut self, context: &egui::Context) {
        let options = CompareOptions {
            pixel_threshold: self.pixel_threshold,
            max_changed_ratio: self.max_changed_ratio,
        };
        match compare_directories(
            &self.baseline_dir,
            &self.current_dir,
            &self.report_dir,
            options,
        ) {
            Ok(report) => {
                self.generation += 1;
                self.textures = load_textures(context, &self.report_dir, &report, self.generation);
                self.report = report;
                self.selected = self
                    .selected
                    .min(self.report.frames.len().saturating_sub(1));
                self.error = None;
            }
            Err(error) => self.error = Some(error.to_string()),
        }
    }

    fn keyboard_shortcuts(&mut self, context: &egui::Context) {
        let (left, right, one, two, three) = context.input(|input| {
            let text = input.events.iter().filter_map(|event| match event {
                egui::Event::Text(text) => Some(text.as_str()),
                _ => None,
            });
            let text: Vec<_> = text.collect();
            (
                input.key_pressed(egui::Key::ArrowLeft),
                input.key_pressed(egui::Key::ArrowRight),
                text.contains(&"1"),
                text.contains(&"2"),
                text.contains(&"3"),
            )
        });
        if left {
            self.select_relative(-1);
        }
        if right {
            self.select_relative(1);
        }
        if one {
            self.mode = ViewMode::Baseline;
        }
        if two {
            self.mode = ViewMode::Current;
        }
        if three {
            self.mode = ViewMode::Diff;
        }
    }
}

impl eframe::App for DiffViewer {
    fn update(&mut self, context: &egui::Context, _frame: &mut eframe::Frame) {
        self.keyboard_shortcuts(context);

        egui::TopBottomPanel::top("toolbar").show(context, |ui| {
            ui.add_space(5.0);
            ui.horizontal_wrapped(|ui| {
                ui.heading("gaanim diff");
                let (status, color) = if self.report.passed {
                    ("PASS", Color32::from_rgb(55, 214, 122))
                } else {
                    ("FAIL", Color32::from_rgb(255, 93, 115))
                };
                ui.colored_label(color, status);
                ui.separator();
                ui.label(format!(
                    "{} comparados · {} cambiados · {} ausentes",
                    self.report.compared, self.report.changed, self.report.missing
                ));
                ui.separator();
                for mode in [ViewMode::Baseline, ViewMode::Current, ViewMode::Diff] {
                    if ui
                        .selectable_label(self.mode == mode, mode.label())
                        .clicked()
                    {
                        self.mode = mode;
                    }
                }
                ui.separator();
                if ui.button("←").on_hover_text("Seek anterior").clicked() {
                    self.select_relative(-1);
                }
                if ui.button("→").on_hover_text("Seek siguiente").clicked() {
                    self.select_relative(1);
                }
                ui.checkbox(&mut self.only_failures, "Sólo fallos");
                ui.checkbox(&mut self.fit_to_view, "Ajustar");
                if !self.fit_to_view {
                    ui.add(egui::Slider::new(&mut self.zoom, 0.1..=8.0).text("Zoom"));
                }
            });
            ui.horizontal_wrapped(|ui| {
                ui.label("Umbral/canal");
                ui.add(egui::DragValue::new(&mut self.pixel_threshold).range(0..=255));
                ui.label("Ratio permitido");
                ui.add(
                    egui::DragValue::new(&mut self.max_changed_ratio)
                        .range(0.0..=1.0)
                        .speed(0.00001)
                        .max_decimals(6),
                );
                if ui.button("Recalcular").clicked() {
                    self.recompute(context);
                }
                ui.weak("Atajos: 1 baseline · 2 actual · 3 diff · ←/→ navegar");
            });
            if let Some(error) = &self.error {
                ui.colored_label(Color32::from_rgb(255, 93, 115), error);
            }
            ui.add_space(4.0);
        });

        egui::SidePanel::left("seeks")
            .resizable(true)
            .default_width(285.0)
            .show(context, |ui| {
                ui.heading("Seeks");
                ui.separator();
                let visible = self.visible_indices();
                egui::ScrollArea::vertical().show(ui, |ui| {
                    for index in visible {
                        let frame = &self.report.frames[index];
                        let color = status_color(frame.status);
                        ui.horizontal(|ui| {
                            ui.colored_label(color, "●");
                            let time = frame
                                .time_seconds
                                .map(|time| format!("  {time:.6}s"))
                                .unwrap_or_default();
                            if ui
                                .selectable_label(
                                    self.selected == index,
                                    format!("{}{}", frame.id, time),
                                )
                                .clicked()
                            {
                                self.selected = index;
                            }
                        });
                    }
                });
            });

        egui::CentralPanel::default().show(context, |ui| {
            let Some(frame) = self.report.frames.get(self.selected) else {
                ui.centered_and_justified(|ui| ui.weak("No hay seeks para mostrar"));
                return;
            };
            ui.horizontal_wrapped(|ui| {
                ui.heading(&frame.id);
                ui.colored_label(status_color(frame.status), format!("{:?}", frame.status));
                if let Some(time) = frame.time_seconds {
                    ui.monospace(format!("t = {time:.6}s"));
                }
            });
            ui.horizontal_wrapped(|ui| {
                ui.monospace(format!(
                    "{} / {} px · {:.6}% · MAE {:.6} · max Δ {}",
                    frame.changed_pixels,
                    frame.total_pixels,
                    frame.changed_ratio * 100.0,
                    frame.mean_absolute_error,
                    frame.max_channel_delta
                ));
                if let Some([x0, y0, x1, y1]) = frame.change_bounds {
                    ui.monospace(format!("bounds [{x0}, {y0}]–[{x1}, {y1}]"));
                }
            });
            ui.separator();

            let texture = self
                .textures
                .get(self.selected)
                .and_then(|textures| textures.for_mode(self.mode));
            match texture {
                Some(texture) => {
                    egui::ScrollArea::both()
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            let native = texture.size_vec2();
                            let available = ui.available_size();
                            let scale = if self.fit_to_view {
                                (available.x / native.x)
                                    .min(available.y / native.y)
                                    .min(1.0)
                            } else {
                                self.zoom
                            };
                            ui.add(
                                egui::Image::new(texture)
                                    .fit_to_exact_size((native * scale).max(egui::vec2(1.0, 1.0))),
                            );
                        });
                }
                None => {
                    ui.centered_and_justified(|ui| {
                        ui.weak(format!("No existe imagen para {}", self.mode.label()))
                    });
                }
            }
        });
    }
}

fn load_textures(
    context: &egui::Context,
    report_dir: &Path,
    report: &DiffReport,
    generation: u64,
) -> Vec<FrameTextures> {
    report
        .frames
        .iter()
        .enumerate()
        .map(|(index, frame)| FrameTextures {
            baseline: load_texture(
                context,
                report_dir,
                frame.baseline_file.as_deref(),
                format!("baseline-{generation}-{index}"),
            ),
            current: load_texture(
                context,
                report_dir,
                frame.current_file.as_deref(),
                format!("current-{generation}-{index}"),
            ),
            diff: load_texture(
                context,
                report_dir,
                frame.diff_file.as_deref(),
                format!("diff-{generation}-{index}"),
            ),
        })
        .collect()
}

fn load_texture(
    context: &egui::Context,
    report_dir: &Path,
    relative_path: Option<&str>,
    name: String,
) -> Option<TextureHandle> {
    let path = report_dir.join(relative_path?);
    let image = image::open(path).ok()?.to_rgba8();
    let size = [image.width() as usize, image.height() as usize];
    let color_image = egui::ColorImage::from_rgba_unmultiplied(size, image.as_raw());
    Some(context.load_texture(name, color_image, TextureOptions::LINEAR))
}

fn configure_style(context: &egui::Context) {
    let mut visuals = egui::Visuals::dark();
    visuals.panel_fill = Color32::from_rgb(14, 17, 23);
    visuals.window_fill = Color32::from_rgb(20, 24, 33);
    visuals.extreme_bg_color = Color32::from_rgb(9, 11, 16);
    visuals.selection.bg_fill = Color32::from_rgb(65, 76, 125);
    context.set_visuals(visuals);
}

fn status_color(status: FrameStatus) -> Color32 {
    match status {
        FrameStatus::Unchanged => Color32::from_rgb(55, 214, 122),
        FrameStatus::Changed | FrameStatus::DimensionMismatch => Color32::from_rgb(255, 93, 115),
        FrameStatus::MissingBaseline | FrameStatus::MissingCurrent => {
            Color32::from_rgb(255, 190, 92)
        }
    }
}
