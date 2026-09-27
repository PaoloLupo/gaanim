use bevy::prelude::*;
use bevy_egui::{EguiContexts, egui};
use gaanim_math::{Camera, ResolvedCamera};
use gaanim_renderer::pipeline::CanvasBackground;
use serde::{Deserialize, Serialize};

use crate::ui_kit::{
    ButtonTone, Icon, caption, divider, icon_button_sized, paint_icon, palette, pill_toggle,
};
use crate::{EditorState, PresentationMode, PreviewInteractive, PreviewView};

/// Configuración de overlays del editor.
///
/// Funciona al igual que el modo interactivo: oculto al inicio (`enabled=false`)
/// y se activa al entrar en su modo correspondiente (tecla `O`).
#[derive(Resource, Debug, Clone)]
pub struct EditorOverlays {
    /// Modo overlays activo (muestra barra y dibujos).
    pub enabled: bool,
    /// Mostrar el rectángulo del área real de la escena (canvas).
    pub show_bounds: bool,
    /// Mostrar ejes y coordenadas.
    pub show_coords: bool,
    /// Mostrar grilla dentro del canvas (en 3D, sobre el plano XZ).
    pub show_grid: bool,
    /// Mostrar márgenes seguros y tercios.
    pub show_guides: bool,
    /// Momento (segundos de la app) en que se copiaron las coordenadas del cursor.
    pub copied_at: Option<f64>,
}

impl Default for EditorOverlays {
    fn default() -> Self {
        Self {
            enabled: false, // oculto al inicio, como modo interactivo
            show_bounds: true,
            show_coords: true,
            show_grid: false,
            show_guides: false,
            copied_at: None,
        }
    }
}

const PREFERENCES_FILE: &str = "overlays.json";

/// Overlays que el usuario dejó activados, recordados entre sesiones.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct OverlayPreferences {
    #[serde(default = "enabled")]
    pub show_bounds: bool,
    #[serde(default = "enabled")]
    pub show_coords: bool,
    #[serde(default)]
    pub show_grid: bool,
    #[serde(default)]
    pub show_guides: bool,
}

fn enabled() -> bool {
    true
}

impl Default for OverlayPreferences {
    fn default() -> Self {
        EditorOverlays::default().preferences()
    }
}

impl OverlayPreferences {
    fn path() -> Option<std::path::PathBuf> {
        gaanim_project::user_data_dir().map(|dir| dir.join(PREFERENCES_FILE))
    }

    /// Las preferencias guardadas, o las de fábrica si no hay o no se leen.
    pub fn load() -> Self {
        Self::path()
            .and_then(|path| std::fs::read_to_string(path).ok())
            .and_then(|source| serde_json::from_str(&source).ok())
            .unwrap_or_default()
    }

    fn save(&self) -> Result<(), String> {
        let path = Self::path().ok_or("no se encontró la carpeta de datos de Gaanim")?;
        let source = serde_json::to_string_pretty(self).map_err(|error| error.to_string())?;
        gaanim_media::narration::write_atomically(&path, source.as_bytes())
            .map_err(|error| error.to_string())
    }
}

impl EditorOverlays {
    /// Overlays ocultos, con los interruptores de `preferences`.
    pub fn with_preferences(preferences: OverlayPreferences) -> Self {
        Self {
            show_bounds: preferences.show_bounds,
            show_coords: preferences.show_coords,
            show_grid: preferences.show_grid,
            show_guides: preferences.show_guides,
            ..Self::default()
        }
    }

    pub fn preferences(&self) -> OverlayPreferences {
        OverlayPreferences {
            show_bounds: self.show_bounds,
            show_coords: self.show_coords,
            show_grid: self.show_grid,
            show_guides: self.show_guides,
        }
    }
}

/// Guarda los interruptores de overlays cuando cambian.
pub fn save_overlay_preferences_system(
    overlays: Res<EditorOverlays>,
    mut saved: Local<Option<OverlayPreferences>>,
) {
    let current = overlays.preferences();
    let Some(previous) = *saved else {
        // The first run records what was loaded; nothing changed yet.
        *saved = Some(current);
        return;
    };
    if previous == current {
        return;
    }
    *saved = Some(current);
    if let Err(error) = current.save() {
        warn!("no se pudieron guardar las preferencias de overlays: {error}");
    }
}

fn effective_zoom(cam: &ResolvedCamera) -> f64 {
    match cam.projection {
        gaanim_math::Projection::Orthographic { zoom } => {
            (cam.pixels_per_unit() * zoom * cam.viewport.scale).max(0.01)
        }
        _ => 1.0,
    }
}

fn logical_grid_step(pixels_per_unit: f64) -> f64 {
    const TARGET_SPACING_PX: f64 = 64.0;
    if !pixels_per_unit.is_finite() || pixels_per_unit <= 0.0 {
        return 1.0;
    }
    nice_step(TARGET_SPACING_PX / pixels_per_unit)
}

/// The 1, 2 or 5 times a power of ten at or above `raw_step`.
fn nice_step(raw_step: f64) -> f64 {
    if !raw_step.is_finite() || raw_step <= 0.0 {
        return 1.0;
    }
    let magnitude = 10.0_f64.powf(raw_step.log10().floor());
    let normalized = raw_step / magnitude;
    let nice = if normalized <= 1.0 {
        1.0
    } else if normalized <= 2.0 {
        2.0
    } else if normalized <= 5.0 {
        5.0
    } else {
        10.0
    };
    nice * magnitude
}

fn format_logical_value(value: f64, step: f64) -> String {
    let decimals = if step >= 1.0 {
        0
    } else {
        (-step.log10().floor()) as usize
    };
    format!("{value:.decimals$}")
}

fn cursor_label_position(cursor: egui::Pos2, viewport: egui::Rect) -> egui::Pos2 {
    const OFFSET: f32 = 14.0;
    // An upper bound keeps the non-interactive label inside the window without
    // requiring a layout pass before positioning it.
    const LABEL_SIZE: egui::Vec2 = egui::vec2(132.0, 32.0);

    let mut position = cursor + egui::vec2(OFFSET, OFFSET);
    if position.x + LABEL_SIZE.x > viewport.max.x {
        position.x = cursor.x - LABEL_SIZE.x - OFFSET;
    }
    if position.y + LABEL_SIZE.y > viewport.max.y {
        position.y = cursor.y - LABEL_SIZE.y - OFFSET;
    }
    egui::pos2(
        position.x.clamp(
            viewport.min.x,
            (viewport.max.x - LABEL_SIZE.x).max(viewport.min.x),
        ),
        position.y.clamp(
            viewport.min.y,
            (viewport.max.y - LABEL_SIZE.y).max(viewport.min.y),
        ),
    )
}

/// Window pixel of the output frame's center, where the camera looks.
fn window_center(cam: &ResolvedCamera, window: &Window) -> glam::DVec2 {
    glam::DVec2::new(
        window.width() as f64 * 0.5,
        window.height() as f64 * 0.5 + cam.viewport.offset_y,
    )
}

/// Output-frame pixel (what `Camera::world_to_screen` returns) to window pixel.
fn output_to_window(cam: &ResolvedCamera, window: &Window, output: glam::DVec2) -> egui::Pos2 {
    let center = glam::DVec2::new(
        cam.viewport_width as f64 * 0.5,
        cam.viewport_height as f64 * 0.5,
    );
    let point = window_center(cam, window) + (output - center) * cam.viewport.scale;
    egui::pos2(point.x as f32, point.y as f32)
}

/// Clip-space `w` below which a point counts as behind a perspective camera.
const NEAR_W: f64 = 1e-3;

/// Window pixel of a clip-space point in front of the camera.
fn clip_to_window(cam: &ResolvedCamera, window: &Window, clip: glam::DVec4) -> egui::Pos2 {
    let ndc = clip.truncate() / clip.w;
    let output = glam::DVec2::new(
        (ndc.x + 1.0) * 0.5 * cam.viewport_width as f64,
        (1.0 - ndc.y) * 0.5 * cam.viewport_height as f64,
    );
    output_to_window(cam, window, output)
}

/// Window segment of `a`–`b`, cut where it passes behind the camera.
fn project_segment(
    cam: &ResolvedCamera,
    window: &Window,
    a: glam::DVec3,
    b: glam::DVec3,
) -> Option<[egui::Pos2; 2]> {
    let view_projection = cam.projection_matrix() * cam.view_matrix();
    let mut clip_a = view_projection * a.extend(1.0);
    let mut clip_b = view_projection * b.extend(1.0);
    if clip_a.w < NEAR_W && clip_b.w < NEAR_W {
        return None;
    }
    let cut = |front: glam::DVec4, behind: glam::DVec4| {
        front + (behind - front) * ((front.w - NEAR_W) / (front.w - behind.w))
    };
    if clip_a.w < NEAR_W {
        clip_a = cut(clip_b, clip_a);
    } else if clip_b.w < NEAR_W {
        clip_b = cut(clip_a, clip_b);
    }
    Some([
        clip_to_window(cam, window, clip_a),
        clip_to_window(cam, window, clip_b),
    ])
}

pub(crate) fn world_to_egui(
    cam: &ResolvedCamera,
    window: &Window,
    world: glam::DVec3,
) -> egui::Pos2 {
    if matches!(cam.projection, gaanim_math::Projection::Perspective { .. }) {
        let clip = cam.projection_matrix() * cam.view_matrix() * world.extend(1.0);
        if clip.w < NEAR_W {
            return egui::pos2(f32::NAN, f32::NAN);
        }
        return clip_to_window(cam, window, clip);
    }
    let eff = effective_zoom(cam);
    let hw = window.width() as f64 * 0.5;
    let hh = window.height() as f64 * 0.5 + cam.viewport.offset_y;
    let angle = cam.z_angle();
    let cos = (-angle).cos();
    let sin = (-angle).sin();
    let dx = world.x - cam.position.x;
    let dy = world.y - cam.position.y;
    let rx = dx * cos - dy * sin;
    let ry = dx * sin + dy * cos;
    egui::pos2((hw + rx * eff) as f32, (hh - ry * eff) as f32)
}

pub(crate) fn egui_to_world(
    cam: &ResolvedCamera,
    window: &Window,
    screen: egui::Pos2,
) -> glam::DVec3 {
    if matches!(cam.projection, gaanim_math::Projection::Perspective { .. }) {
        let offset =
            glam::DVec2::new(screen.x as f64, screen.y as f64) - window_center(cam, window);
        let output = glam::DVec2::new(
            cam.viewport_width as f64 * 0.5,
            cam.viewport_height as f64 * 0.5,
        ) + offset / cam.viewport.scale.max(1e-9);
        return cam.screen_to_world(output);
    }
    let eff = effective_zoom(cam);
    let hw = window.width() as f64 * 0.5;
    let hh = window.height() as f64 * 0.5 + cam.viewport.offset_y;
    let angle = cam.z_angle();
    // screen -> rotated
    let rx = (screen.x as f64 - hw) / eff;
    let ry = (hh - screen.y as f64) / eff;
    let cos = angle.cos();
    let sin = angle.sin();
    let dx = rx * cos - ry * sin;
    let dy = rx * sin + ry * cos;
    glam::DVec3::new(cam.position.x + dx, cam.position.y + dy, 0.0)
}

/// Colores de los overlays, tomados de la paleta del editor.
mod colors {
    use super::{egui::Color32, palette};

    pub const BOUNDS: Color32 = palette::STOP;
    pub const AXIS_X: Color32 = palette::DANGER;
    pub const AXIS_Y: Color32 = palette::LOOP;
    pub const AXIS_Z: Color32 = palette::ACCENT;
    pub const SELECTION: Color32 = palette::ACCENT;
    pub const GRID: Color32 = Color32::from_rgba_premultiplied(22, 22, 24, 24);
    pub const TICK_LABEL: Color32 = palette::TEXT_MUTED;
}

/// How long the coordinate label confirms a copy (seconds).
const COPIED_FEEDBACK_SECS: f64 = 1.2;

/// Barra flotante con los modos de vista y los overlays.
/// Solo visible cuando `enabled=true` (activado con `O`, como el modo interactivo con `I`).
pub fn overlays_settings_ui_system(
    mut contexts: EguiContexts,
    mut overlays: ResMut<EditorOverlays>,
    mut interactive: ResMut<PreviewInteractive>,
    authored_camera: Option<Res<Camera>>,
    presentation: Res<PresentationMode>,
) {
    if presentation.active || !overlays.enabled {
        return;
    }
    let Ok(ctx) = contexts.ctx_mut() else {
        return;
    };

    egui::Area::new("overlays_top_bar".into())
        .anchor(egui::Align2::CENTER_TOP, egui::vec2(0.0, 10.0))
        .order(egui::Order::Foreground)
        .interactable(true)
        .show(ctx, |ui| {
            egui::Frame::new()
                .fill(palette::PANEL)
                .inner_margin(egui::Margin::symmetric(8, 6))
                .stroke(egui::Stroke::new(1.0, palette::PANEL_STROKE))
                .shadow(egui::Shadow {
                    offset: [0, 8],
                    blur: 28,
                    spread: 0,
                    color: egui::Color32::from_black_alpha(110),
                })
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 6.0;
                        const HEIGHT: f32 = 30.0;
                        if pill_toggle(
                            ui,
                            Icon::Move,
                            Some("Interactivo"),
                            interactive.enabled,
                            palette::ACCENT,
                            HEIGHT,
                        )
                        .on_hover_text(
                            "Mover la vista sin tocar la cámara de la escena · I\n\
                             Arrastrar desplaza, la rueda acerca hacia el cursor y un clic selecciona.",
                        )
                        .clicked()
                        {
                            interactive.toggle(authored_camera.as_deref().copied());
                        }
                        divider(ui);
                        if pill_toggle(
                            ui,
                            Icon::Fullscreen,
                            Some("Límites"),
                            overlays.show_bounds,
                            colors::BOUNDS,
                            HEIGHT,
                        )
                        .on_hover_text("Límites del marco de salida · B")
                        .clicked()
                        {
                            overlays.show_bounds = !overlays.show_bounds;
                        }
                        if pill_toggle(
                            ui,
                            Icon::Crosshair,
                            Some("Coordenadas"),
                            overlays.show_coords,
                            colors::AXIS_Y,
                            HEIGHT,
                        )
                        .on_hover_text(
                            "Ejes y coordenadas del cursor · C\n\
                             Ctrl+C copia el punto bajo el cursor; Mayús lo ajusta a la grilla.",
                        )
                        .clicked()
                        {
                            overlays.show_coords = !overlays.show_coords;
                        }
                        if pill_toggle(
                            ui,
                            Icon::Grid,
                            Some("Grilla"),
                            overlays.show_grid,
                            palette::ACCENT,
                            HEIGHT,
                        )
                        .on_hover_text(
                            "Grilla en unidades lógicas · G\nEn 3D, sobre el plano XZ.",
                        )
                        .clicked()
                        {
                            overlays.show_grid = !overlays.show_grid;
                        }
                        if pill_toggle(
                            ui,
                            Icon::Thirds,
                            Some("Guías"),
                            overlays.show_guides,
                            palette::TEXT,
                            HEIGHT,
                        )
                        .on_hover_text("Márgenes seguros (90 % y 80 %) y tercios · M")
                        .clicked()
                        {
                            overlays.show_guides = !overlays.show_guides;
                        }
                        if interactive.enabled {
                            divider(ui);
                            let free_3d = interactive.view == PreviewView::Free3D;
                            let moved = free_3d
                                || interactive.pan != glam::DVec2::ZERO
                                || (interactive.user_zoom - 1.0).abs() > 1e-9;
                            let zoom = if free_3d {
                                "3D libre".to_owned()
                            } else {
                                format!("{:.0} %", interactive.user_zoom * 100.0)
                            };
                            ui.add(
                                egui::Label::new(
                                    egui::RichText::new(zoom).size(12.5).color(if moved {
                                        palette::ACCENT
                                    } else {
                                        palette::TEXT_MUTED
                                    }),
                                )
                                .selectable(false),
                            )
                            .on_hover_text(if moved {
                                "La vista no coincide con la cámara de la escena"
                            } else {
                                "La vista coincide con la cámara de la escena"
                            });
                            if icon_button_sized(ui, Icon::Reset, ButtonTone::Ghost, moved, HEIGHT)
                                .on_hover_text("Restablecer la vista · R")
                                .clicked()
                            {
                                interactive.reset();
                            }
                        }
                        divider(ui);
                        let keyboard =
                            icon_button_sized(ui, Icon::Keyboard, ButtonTone::Ghost, true, HEIGHT)
                                .on_hover_text("Atajos de teclado");
                        egui::Popup::menu(&keyboard).show(show_shortcuts);
                        if icon_button_sized(ui, Icon::Close, ButtonTone::Ghost, true, HEIGHT)
                            .on_hover_text("Ocultar overlays · O o Esc")
                            .clicked()
                        {
                            overlays.enabled = false;
                        }
                    });
                });
        });
}

/// Atajos del modo overlays y de la inspección.
const SHORTCUTS: &[(&str, &str)] = &[
    ("O · Esc", "Mostrar u ocultar los overlays"),
    ("I", "Inspección: mover la vista sin tocar la cámara"),
    ("B · C · G · M", "Límites, coordenadas, grilla y guías"),
    (
        "Arrastrar",
        "Desplazar la vista (en 3D, orbitar con el derecho)",
    ),
    ("Clic", "Seleccionar el objeto bajo el cursor"),
    ("Rueda", "Acercar o alejar hacia el cursor"),
    ("W A S D", "Mover la cámara"),
    ("R · F", "Restablecer la vista · encuadrar en 3D"),
    ("Mayús", "Ajustar el punto a la grilla"),
    ("Ctrl+C", "Copiar el punto bajo el cursor"),
];

fn show_shortcuts(ui: &mut egui::Ui) {
    ui.set_min_width(360.0);
    ui.label(caption("ATAJOS DE TECLADO"));
    ui.add_space(6.0);
    egui::Grid::new("overlay-shortcuts")
        .num_columns(2)
        .spacing([16.0, 7.0])
        .show(ui, |ui| {
            for (keys, action) in SHORTCUTS {
                egui::Frame::new()
                    .fill(palette::FIELD)
                    .inner_margin(egui::Margin::symmetric(7, 2))
                    .show(ui, |ui| {
                        ui.label(
                            egui::RichText::new(*keys)
                                .monospace()
                                .size(12.0)
                                .color(palette::TEXT),
                        );
                    });
                ui.label(
                    egui::RichText::new(*action)
                        .size(13.0)
                        .color(palette::TEXT_MUTED),
                );
                ui.end_row();
            }
        });
}

/// Small panel-colored tag with one line of text, placed by `align` at `pos`.
fn paint_tag(
    painter: &egui::Painter,
    pos: egui::Pos2,
    align: egui::Align2,
    text: String,
    color: egui::Color32,
) -> egui::Rect {
    let galley = painter.layout_no_wrap(text, egui::FontId::proportional(11.0), color);
    let padding = egui::vec2(6.0, 3.0);
    let rect = align.anchor_size(pos, galley.size() + padding * 2.0);
    painter.rect_filled(rect, 0.0, palette::PANEL);
    painter.rect_stroke(
        rect,
        0.0,
        egui::Stroke::new(1.0, palette::PANEL_STROKE),
        egui::StrokeKind::Inside,
    );
    painter.galley(rect.min + padding, galley, color);
    rect
}

/// Nearest multiple of `step` to `value`, avoiding `-0`.
fn snap_to_step(value: f64, step: f64) -> f64 {
    if !(step.is_finite() && step > 0.0) {
        return value;
    }
    let snapped = (value / step).round() * step;
    if snapped == 0.0 { 0.0 } else { snapped }
}

/// Text copied for a point: a Python tuple, e.g. `(1.25, -0.5)`.
fn point_text(point: glam::DVec2, step: Option<f64>) -> String {
    match step {
        Some(step) => format!(
            "({}, {})",
            format_logical_value(point.x, step),
            format_logical_value(point.y, step)
        ),
        None => format!("({:.2}, {:.2})", point.x, point.y),
    }
}

/// Screen rectangle covering every corner of `bounds`, or `None` when a
/// corner does not project (e.g. behind a perspective camera).
fn projected_rect(
    cam: &ResolvedCamera,
    window: &Window,
    bounds: &gaanim_math::Bounds3D,
) -> Option<egui::Rect> {
    let mut rect = egui::Rect::NOTHING;
    for i in 0..8 {
        let corner = glam::DVec3::new(
            if i & 1 == 0 {
                bounds.min.x
            } else {
                bounds.max.x
            },
            if i & 2 == 0 {
                bounds.min.y
            } else {
                bounds.max.y
            },
            if i & 4 == 0 {
                bounds.min.z
            } else {
                bounds.max.z
            },
        );
        let point = world_to_egui(cam, window, corner);
        if !(point.x.is_finite() && point.y.is_finite()) {
            return None;
        }
        rect.extend_with(point);
    }
    rect.is_positive().then_some(rect).or_else(|| {
        // A flat shape (a line) still deserves a visible outline.
        rect.is_finite().then(|| rect.expand(1.0))
    })
}

/// Sistema que dibuja límites del canvas, grilla, ejes, la selección y las
/// coordenadas del cursor sobre el viewport.
/// Solo cuando el modo overlays está activo (`enabled=true`, tecla `O`).
#[allow(clippy::too_many_arguments)]
pub fn scene_overlays_system(
    mut contexts: EguiContexts,
    mut overlays: ResMut<EditorOverlays>,
    camera: Option<Res<ResolvedCamera>>,
    canvas_bg: Option<Res<CanvasBackground>>,
    windows: Query<&Window, With<bevy::window::PrimaryWindow>>,
    presentation: Res<PresentationMode>,
    state: Res<EditorState>,
    interactive: Res<PreviewInteractive>,
    pickable: Query<crate::PickBoundsQueryData>,
    keys: Res<ButtonInput<KeyCode>>,
    time: Res<Time>,
) {
    if presentation.active || !overlays.enabled {
        return;
    }
    let Some(cam) = camera else {
        return;
    };
    let Ok(window) = windows.single() else {
        return;
    };
    let Ok(ctx) = contexts.ctx_mut() else {
        return;
    };

    let is_perspective = matches!(cam.projection, gaanim_math::Projection::Perspective { .. });

    // Determinar bounds del canvas real
    let (bmin, bmax) = if let Some(bg) = canvas_bg.as_ref() {
        let b = bg.bounds;
        (
            glam::DVec2::new(b.min.x, b.min.y),
            glam::DVec2::new(b.max.x, b.max.y),
        )
    } else {
        let hw = cam.frame_width * 0.5;
        let hh = cam.frame_height * 0.5;
        (glam::DVec2::new(-hw, -hh), glam::DVec2::new(hw, hh))
    };
    let label = format!(
        "{} × {} unidades lógicas",
        cam.frame_width, cam.frame_height
    );

    let corners_screen: Vec<egui::Pos2> = if is_perspective {
        // En perspectiva el canvas es screen-space (fijo a la cámara), no world-space.
        // Dibujar un rectángulo fijo centrado en la ventana usando viewport scale/offset.
        let w = window.width();
        let h = window.height();
        let vp_w = cam.viewport_width as f32 * cam.viewport.scale as f32;
        let vp_h = cam.viewport_height as f32 * cam.viewport.scale as f32;
        let cx = w * 0.5;
        let cy = h * 0.5 + cam.viewport.offset_y as f32;
        vec![
            egui::pos2(cx - vp_w * 0.5, cy - vp_h * 0.5),
            egui::pos2(cx + vp_w * 0.5, cy - vp_h * 0.5),
            egui::pos2(cx + vp_w * 0.5, cy + vp_h * 0.5),
            egui::pos2(cx - vp_w * 0.5, cy + vp_h * 0.5),
        ]
    } else {
        [
            glam::DVec3::new(bmin.x, bmin.y, 0.0),
            glam::DVec3::new(bmax.x, bmin.y, 0.0),
            glam::DVec3::new(bmax.x, bmax.y, 0.0),
            glam::DVec3::new(bmin.x, bmax.y, 0.0),
        ]
        .iter()
        .map(|w| world_to_egui(&cam, window, *w))
        .collect()
    };
    let step = logical_grid_step(effective_zoom(&cam));
    // While inspecting, the preview shows more than the output frame, so the
    // grid and the axes cover the whole visible area.
    let (lo, hi) = if interactive.enabled && !is_perspective {
        visible_world_range(&cam, window)
    } else {
        (bmin, bmax)
    };
    let selected = state
        .selected
        .and_then(|entity| pickable.get(entity).ok())
        .and_then(crate::pick_bounds);

    // Área de dibujo full-screen no interactiva
    egui::Area::new("scene_overlays".into())
        .fixed_pos(egui::pos2(0.0, 0.0))
        .interactable(false)
        .show(ctx, |ui| {
            let vp = ctx.viewport_rect();
            let _ = ui.allocate_space(vp.size());
            let painter = ui.painter();

            // --- Grilla (debajo de todo lo demás) ---
            if overlays.show_grid && is_perspective {
                paint_ground_grid(painter, &cam, window);
            } else if overlays.show_grid {
                let grid_stroke = egui::Stroke::new(1.0, colors::GRID);
                let mut x = (lo.x / step).ceil() * step;
                while x <= hi.x + 1e-6 {
                    if x.abs() > 1e-6 || !overlays.show_coords {
                        let a = world_to_egui(&cam, window, glam::DVec3::new(x, lo.y, 0.0));
                        let b = world_to_egui(&cam, window, glam::DVec3::new(x, hi.y, 0.0));
                        painter.line_segment([a, b], grid_stroke);
                    }
                    x += step;
                }
                let mut y = (lo.y / step).ceil() * step;
                while y <= hi.y + 1e-6 {
                    if y.abs() > 1e-6 || !overlays.show_coords {
                        let a = world_to_egui(&cam, window, glam::DVec3::new(lo.x, y, 0.0));
                        let b = world_to_egui(&cam, window, glam::DVec3::new(hi.x, y, 0.0));
                        painter.line_segment([a, b], grid_stroke);
                    }
                    y += step;
                }
            }

            // --- Guías de composición ---
            if overlays.show_guides {
                paint_guides(painter, &corners_screen);
            }

            // --- Límites del área real (canvas) ---
            if overlays.show_bounds {
                let stroke = egui::Stroke::new(1.0, colors::BOUNDS.gamma_multiply(0.7));
                for i in 0..4 {
                    painter.line_segment([corners_screen[i], corners_screen[(i + 1) % 4]], stroke);
                }
                // Esquinas en L, orientadas hacia los dos lados que se unen en cada esquina.
                let corner_stroke = egui::Stroke::new(2.0, colors::BOUNDS);
                for i in 0..4 {
                    let corner = corners_screen[i];
                    let arm = |to: egui::Pos2| {
                        let dir = (to - corner).normalized() * 12.0;
                        corner + dir
                    };
                    painter.line(
                        vec![
                            arm(corners_screen[(i + 3) % 4]),
                            corner,
                            arm(corners_screen[(i + 1) % 4]),
                        ],
                        corner_stroke,
                    );
                }
                let top = corners_screen
                    .iter()
                    .map(|c| c.y)
                    .fold(f32::INFINITY, f32::min);
                let center_x = (corners_screen[0].x + corners_screen[2].x) * 0.5;
                paint_tag(
                    painter,
                    egui::pos2(center_x, top - 6.0),
                    egui::Align2::CENTER_BOTTOM,
                    label.clone(),
                    colors::BOUNDS,
                );
            }

            // --- Ejes X/Y (solo ortho; en perspectiva los ejes 3D ya existen) ---
            let origin_visible = lo.x <= 0.0 && hi.x >= 0.0 && lo.y <= 0.0 && hi.y >= 0.0;
            if overlays.show_coords && !is_perspective && origin_visible {
                paint_axes(painter, &cam, window, lo, hi, step);
            }

            // --- Selección ---
            if let Some(bounds) = selected
                && let Some(rect) = projected_rect(&cam, window, &bounds)
            {
                painter.rect_stroke(
                    rect,
                    0.0,
                    egui::Stroke::new(1.5, colors::SELECTION),
                    egui::StrokeKind::Outside,
                );
                for corner in [
                    rect.left_top(),
                    rect.right_top(),
                    rect.right_bottom(),
                    rect.left_bottom(),
                ] {
                    painter.rect_filled(
                        egui::Rect::from_center_size(corner, egui::Vec2::splat(6.0)),
                        0.0,
                        colors::SELECTION,
                    );
                }
                let center = bounds.center();
                let size = bounds.size();
                let text = if size.z.abs() > 1e-6 {
                    format!(
                        "({:.2}, {:.2}, {:.2}) · {:.2} × {:.2} × {:.2}",
                        center.x, center.y, center.z, size.x, size.y, size.z
                    )
                } else {
                    format!(
                        "({:.2}, {:.2}) · {:.2} × {:.2}",
                        center.x, center.y, size.x, size.y
                    )
                };
                paint_tag(
                    painter,
                    rect.center_bottom() + egui::vec2(0.0, 8.0),
                    egui::Align2::CENTER_TOP,
                    text,
                    palette::TEXT,
                );
            }
        });

    // --- Coordenadas del cursor ---
    let Some(cursor) = window.cursor_position() else {
        return;
    };
    // Sobre la barra o los paneles del editor las coordenadas no significan nada.
    if !overlays.show_coords || is_perspective || ctx.is_pointer_over_egui() {
        return;
    }
    let cursor = egui::pos2(cursor.x, cursor.y);
    let world = egui_to_world(&cam, window, cursor);
    let snap = keys.any_pressed([KeyCode::ShiftLeft, KeyCode::ShiftRight]);
    let point = if snap {
        glam::DVec2::new(snap_to_step(world.x, step), snap_to_step(world.y, step))
    } else {
        world.truncate()
    };
    let text = point_text(point, snap.then_some(step));

    let now = time.elapsed_secs_f64();
    let command = keys.any_pressed([
        KeyCode::ControlLeft,
        KeyCode::ControlRight,
        KeyCode::SuperLeft,
        KeyCode::SuperRight,
    ]);
    if command && keys.just_pressed(KeyCode::KeyC) && !ctx.egui_wants_keyboard_input() {
        ctx.copy_text(text.clone());
        overlays.copied_at = Some(now);
    }
    let copied = overlays
        .copied_at
        .is_some_and(|at| now - at < COPIED_FEEDBACK_SECS);

    if snap {
        let marker = world_to_egui(&cam, window, point.extend(0.0));
        ctx.layer_painter(egui::LayerId::new(
            egui::Order::Foreground,
            egui::Id::new("overlay_snap_marker"),
        ))
        .circle_stroke(marker, 4.0, egui::Stroke::new(1.5, palette::ACCENT));
    }

    let label_position = cursor_label_position(cursor, ctx.viewport_rect());
    egui::Area::new("mouse_coords".into())
        .fixed_pos(label_position)
        .order(egui::Order::Tooltip)
        .interactable(false)
        .show(ctx, |ui| {
            egui::Frame::new()
                .fill(palette::PANEL)
                .inner_margin(egui::Margin::symmetric(8, 5))
                .stroke(egui::Stroke::new(1.0, palette::PANEL_STROKE))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 6.0;
                        let (icon, tint) = if copied {
                            (Icon::Check, palette::LOOP)
                        } else {
                            (Icon::Crosshair, colors::AXIS_Y)
                        };
                        let (rect, _) =
                            ui.allocate_exact_size(egui::Vec2::splat(12.0), egui::Sense::hover());
                        paint_icon(ui.painter(), rect, icon, tint);
                        let value = if copied { "Copiado".to_owned() } else { text };
                        ui.add(
                            egui::Label::new(
                                egui::RichText::new(value)
                                    .monospace()
                                    .size(11.5)
                                    .color(if snap { palette::ACCENT } else { palette::TEXT }),
                            )
                            .selectable(false),
                        );
                    });
                });
        });
}

/// World box covering the whole window in an orthographic view.
fn visible_world_range(cam: &ResolvedCamera, window: &Window) -> (glam::DVec2, glam::DVec2) {
    let (width, height) = (window.width(), window.height());
    let corners = [
        egui::pos2(0.0, 0.0),
        egui::pos2(width, 0.0),
        egui::pos2(width, height),
        egui::pos2(0.0, height),
    ]
    .map(|corner| egui_to_world(cam, window, corner).truncate());
    let lo = corners
        .iter()
        .copied()
        .reduce(glam::DVec2::min)
        .unwrap_or_default();
    let hi = corners
        .iter()
        .copied()
        .reduce(glam::DVec2::max)
        .unwrap_or_default();
    (lo, hi)
}

/// Point at fractions `(u, v)` of the frame quad (`corners` clockwise from
/// the top left), which may be rotated.
fn frame_point(corners: &[egui::Pos2], u: f32, v: f32) -> egui::Pos2 {
    corners[0] + (corners[1] - corners[0]) * u + (corners[3] - corners[0]) * v
}

/// Márgenes seguros de acción (90 %) y de títulos (80 %), y tercios.
fn paint_guides(painter: &egui::Painter, corners: &[egui::Pos2]) {
    let color = palette::TEXT_MUTED;
    for (inset, alpha) in [(0.05, 0.55), (0.10, 0.4)] {
        let (a, b) = (inset, 1.0 - inset);
        let mut outline = vec![
            frame_point(corners, a, a),
            frame_point(corners, b, a),
            frame_point(corners, b, b),
            frame_point(corners, a, b),
        ];
        outline.push(outline[0]);
        painter.extend(egui::Shape::dashed_line(
            &outline,
            egui::Stroke::new(1.0, color.gamma_multiply(alpha)),
            6.0,
            4.0,
        ));
    }
    let thirds = egui::Stroke::new(1.0, color.gamma_multiply(0.3));
    for t in [1.0 / 3.0, 2.0 / 3.0] {
        painter.line_segment(
            [frame_point(corners, t, 0.0), frame_point(corners, t, 1.0)],
            thirds,
        );
        painter.line_segment(
            [frame_point(corners, 0.0, t), frame_point(corners, 1.0, t)],
            thirds,
        );
    }
}

/// Lines drawn on each side of the ground grid's center.
const GROUND_GRID_HALF_LINES: i32 = 10;

/// Grilla del plano XZ (Y = 0) alrededor del punto que mira la cámara, con
/// el eje X en rojo y el Z en azul.
fn paint_ground_grid(painter: &egui::Painter, cam: &ResolvedCamera, window: &Window) {
    let distance = (cam.position - cam.target).length();
    let step = nice_step(distance / 8.0);
    let center_x = snap_to_step(cam.target.x, step);
    let center_z = snap_to_step(cam.target.z, step);
    let reach = step * GROUND_GRID_HALF_LINES as f64;
    let grid = egui::Stroke::new(1.0, egui::Color32::from_white_alpha(30));
    for k in -GROUND_GRID_HALF_LINES..=GROUND_GRID_HALF_LINES {
        let offset = step * k as f64;
        // A line of constant z runs along X; z = 0 is the X axis.
        let z = center_z + offset;
        let stroke = if z.abs() < step * 1e-6 {
            egui::Stroke::new(1.4, colors::AXIS_X.gamma_multiply(0.85))
        } else {
            grid
        };
        if let Some(segment) = project_segment(
            cam,
            window,
            glam::DVec3::new(center_x - reach, 0.0, z),
            glam::DVec3::new(center_x + reach, 0.0, z),
        ) {
            painter.line_segment(segment, stroke);
        }
        let x = center_x + offset;
        let stroke = if x.abs() < step * 1e-6 {
            egui::Stroke::new(1.4, colors::AXIS_Z.gamma_multiply(0.85))
        } else {
            grid
        };
        if let Some(segment) = project_segment(
            cam,
            window,
            glam::DVec3::new(x, 0.0, center_z - reach),
            glam::DVec3::new(x, 0.0, center_z + reach),
        ) {
            painter.line_segment(segment, stroke);
        }
    }
}

/// Ejes X/Y con marcas y valores en el mismo paso lógico adaptativo que la grilla.
fn paint_axes(
    painter: &egui::Painter,
    cam: &ResolvedCamera,
    window: &Window,
    bmin: glam::DVec2,
    bmax: glam::DVec2,
    step: f64,
) {
    let x_start = world_to_egui(cam, window, glam::DVec3::new(bmin.x, 0.0, 0.0));
    let x_end = world_to_egui(cam, window, glam::DVec3::new(bmax.x, 0.0, 0.0));
    let y_start = world_to_egui(cam, window, glam::DVec3::new(0.0, bmin.y, 0.0));
    let y_end = world_to_egui(cam, window, glam::DVec3::new(0.0, bmax.y, 0.0));
    let x_stroke = egui::Stroke::new(1.2, colors::AXIS_X.gamma_multiply(0.85));
    let y_stroke = egui::Stroke::new(1.2, colors::AXIS_Y.gamma_multiply(0.85));
    painter.line_segment([x_start, x_end], x_stroke);
    painter.line_segment([y_start, y_end], y_stroke);

    // Puntas de flecha en los extremos positivos, orientadas con la cámara.
    for (tip, from, stroke) in [(x_end, x_start, x_stroke), (y_end, y_start, y_stroke)] {
        let dir = (tip - from).normalized() * 8.0;
        let normal = egui::vec2(-dir.y, dir.x) * 0.45;
        painter.line(vec![tip - dir + normal, tip, tip - dir - normal], stroke);
    }
    let axis_font = egui::FontId::proportional(11.0);
    painter.text(
        x_end + egui::vec2(6.0, -8.0),
        egui::Align2::LEFT_CENTER,
        "X",
        axis_font.clone(),
        colors::AXIS_X,
    );
    painter.text(
        y_end + egui::vec2(6.0, -6.0),
        egui::Align2::LEFT_CENTER,
        "Y",
        axis_font,
        colors::AXIS_Y,
    );

    // Origen
    let origin = world_to_egui(cam, window, glam::DVec3::ZERO);
    let marker = egui::Rect::from_center_size(origin, egui::Vec2::splat(6.0));
    painter.rect_filled(marker, 0.0, palette::TEXT);
    painter.rect_stroke(
        marker,
        0.0,
        egui::Stroke::new(1.0, palette::INK),
        egui::StrokeKind::Outside,
    );

    let tick_len = 4.0;
    let font = egui::FontId::proportional(9.5);
    let mut x = (bmin.x / step).ceil() * step;
    while x <= bmax.x + 1e-6 {
        if x.abs() > 1e-6 {
            let p = world_to_egui(cam, window, glam::DVec3::new(x, 0.0, 0.0));
            let perp = egui::vec2(0.0, tick_len);
            painter.line_segment([p - perp, p + perp], x_stroke);
            painter.text(
                p + egui::vec2(0.0, 8.0),
                egui::Align2::CENTER_TOP,
                format_logical_value(x, step),
                font.clone(),
                colors::TICK_LABEL,
            );
        }
        x += step;
    }
    let mut y = (bmin.y / step).ceil() * step;
    while y <= bmax.y + 1e-6 {
        if y.abs() > 1e-6 {
            let p = world_to_egui(cam, window, glam::DVec3::new(0.0, y, 0.0));
            let perp = egui::vec2(tick_len, 0.0);
            painter.line_segment([p - perp, p + perp], y_stroke);
            painter.text(
                p + egui::vec2(7.0, 0.0),
                egui::Align2::LEFT_CENTER,
                format_logical_value(y, step),
                font.clone(),
                colors::TICK_LABEL,
            );
        }
        y += step;
    }
}

/// Atajos de teclado para overlays: O = modo overlays (como I para interactivo), B/C/G dentro del modo.
pub fn overlays_toggle_keys_system(
    egui_wants: Res<bevy_egui::input::EguiWantsInput>,
    keys: Res<ButtonInput<KeyCode>>,
    mut overlays: ResMut<EditorOverlays>,
    presentation: Res<PresentationMode>,
) {
    if presentation.active {
        return;
    }
    if egui_wants.wants_keyboard_input() {
        return;
    }
    // O: alternar modo overlays (oculto al inicio, como modo interactivo)
    if keys.just_pressed(KeyCode::KeyO) {
        overlays.enabled = !overlays.enabled;
        return;
    }
    // Esc: salir del modo overlays si está activo
    if overlays.enabled && keys.just_pressed(KeyCode::Escape) {
        overlays.enabled = false;
        return;
    }
    if !overlays.enabled {
        return;
    }
    if keys.just_pressed(KeyCode::KeyB) {
        overlays.show_bounds = !overlays.show_bounds;
    }
    // Ctrl+C (Cmd+C) copia las coordenadas del cursor; no alterna los ejes.
    let command = keys.any_pressed([
        KeyCode::ControlLeft,
        KeyCode::ControlRight,
        KeyCode::SuperLeft,
        KeyCode::SuperRight,
    ]);
    if keys.just_pressed(KeyCode::KeyC) && !command {
        overlays.show_coords = !overlays.show_coords;
    }
    if keys.just_pressed(KeyCode::KeyG) {
        overlays.show_grid = !overlays.show_grid;
    }
    if keys.just_pressed(KeyCode::KeyM) {
        overlays.show_guides = !overlays.show_guides;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grid_step_uses_readable_logical_units_at_common_editor_scales() {
        assert_eq!(logical_grid_step(80.0), 1.0);
        assert_eq!(logical_grid_step(40.0), 2.0);
        assert_eq!(logical_grid_step(160.0), 0.5);
    }

    #[test]
    fn logical_tick_labels_preserve_fractional_steps() {
        assert_eq!(format_logical_value(2.0, 1.0), "2");
        assert_eq!(format_logical_value(0.5, 0.5), "0.5");
        assert_eq!(format_logical_value(-0.05, 0.05), "-0.05");
    }

    #[test]
    fn preferences_default_to_bounds_and_coordinates_and_round_trip() {
        let stored: OverlayPreferences = serde_json::from_str("{}").unwrap();
        assert_eq!(stored, OverlayPreferences::default());
        assert!(stored.show_bounds && stored.show_coords);
        assert!(!stored.show_grid && !stored.show_guides);

        let chosen = OverlayPreferences {
            show_bounds: false,
            show_coords: true,
            show_grid: true,
            show_guides: true,
        };
        let overlays = EditorOverlays::with_preferences(chosen);
        assert!(!overlays.enabled, "the mode itself always starts hidden");
        assert_eq!(overlays.preferences(), chosen);
    }

    #[test]
    fn nice_steps_are_one_two_or_five_times_a_power_of_ten() {
        assert_eq!(nice_step(0.8), 1.0);
        assert_eq!(nice_step(1.3), 2.0);
        assert_eq!(nice_step(30.0), 50.0);
        assert!((nice_step(0.004) - 0.005).abs() < 1e-12);
        assert_eq!(nice_step(0.0), 1.0);
    }

    fn perspective_view() -> (ResolvedCamera, Window) {
        let mut window = Window::default();
        window.resolution.set(1280.0, 720.0);
        let mut camera = Camera::perspective_3d(1920, 1080, 0.8);
        camera
            .look_at(
                glam::DVec3::new(0.0, 4.0, 10.0),
                glam::DVec3::ZERO,
                glam::DVec3::Y,
            )
            .unwrap();
        let viewport = gaanim_math::CameraViewport {
            scale: 0.5,
            offset_y: -30.0,
        };
        (ResolvedCamera::new(camera, viewport), window)
    }

    #[test]
    fn perspective_points_map_to_the_window_frame_and_back() {
        let (camera, window) = perspective_view();
        // The look-at target sits at the center of the fitted frame.
        let target = world_to_egui(&camera, &window, glam::DVec3::ZERO);
        assert!((target.x - 640.0).abs() < 1e-3 && (target.y - 330.0).abs() < 1e-3);

        let point = glam::DVec3::new(1.5, 0.0, -2.0);
        let screen = world_to_egui(&camera, &window, point);
        // `screen_to_world` lands on the z = 0 plane, so compare along that ray.
        let back = egui_to_world(&camera, &window, screen);
        let again = world_to_egui(&camera, &window, back);
        assert!((again - screen).length() < 1e-3, "{screen:?} -> {again:?}");

        let behind = world_to_egui(&camera, &window, glam::DVec3::new(0.0, 4.0, 20.0));
        assert!(behind.x.is_nan(), "points behind the camera do not project");
    }

    #[test]
    fn segments_behind_the_camera_are_cut_or_skipped() {
        let (camera, window) = perspective_view();
        let ahead = glam::DVec3::new(0.0, 0.0, 0.0);
        let behind = glam::DVec3::new(0.0, 0.0, 30.0);
        let [a, b] = project_segment(&camera, &window, ahead, behind).expect("half visible");
        assert!(a.x.is_finite() && a.y.is_finite() && b.x.is_finite() && b.y.is_finite());
        assert_eq!(a, world_to_egui(&camera, &window, ahead));
        assert!(
            project_segment(&camera, &window, behind, behind + glam::DVec3::X).is_none(),
            "a segment fully behind the camera is not drawn"
        );
    }

    #[test]
    fn inspection_grid_covers_the_visible_window() {
        let mut window = Window::default();
        window.resolution.set(1280.0, 720.0);
        let mut camera = ResolvedCamera::new(
            Camera::ortho_2d_frame(16.0, 9.0, 1920, 1080),
            gaanim_math::CameraViewport {
                scale: 1280.0 / 1920.0 * 0.5,
                offset_y: 0.0,
            },
        );
        camera.camera.position = glam::DVec3::new(3.0, 1.0, 0.0);
        // Zoomed out to 50 %, the window shows twice the frame around (3, 1).
        let (lo, hi) = visible_world_range(&camera, &window);
        assert!((lo - glam::DVec2::new(-13.0, -8.0)).length() < 1e-9, "{lo}");
        assert!((hi - glam::DVec2::new(19.0, 10.0)).length() < 1e-9, "{hi}");
    }

    #[test]
    fn guides_follow_the_frame_quad() {
        let corners = [
            egui::pos2(100.0, 50.0),
            egui::pos2(400.0, 50.0),
            egui::pos2(400.0, 250.0),
            egui::pos2(100.0, 250.0),
        ];
        assert_eq!(frame_point(&corners, 0.5, 0.5), egui::pos2(250.0, 150.0));
        assert_eq!(frame_point(&corners, 0.1, 0.9), egui::pos2(130.0, 230.0));
    }

    #[test]
    fn snapped_points_copy_as_python_tuples_at_the_grid_precision() {
        assert_eq!(snap_to_step(1.26, 0.5), 1.5);
        assert_eq!(snap_to_step(-0.2, 0.5), 0.0);
        assert_eq!(snap_to_step(0.37, 0.0), 0.37);
        assert_eq!(
            point_text(glam::DVec2::new(1.5, -2.0), Some(0.5)),
            "(1.5, -2.0)"
        );
        assert_eq!(point_text(glam::DVec2::new(3.0, 1.0), Some(1.0)), "(3, 1)");
        assert_eq!(
            point_text(glam::DVec2::new(1.234, -0.5), None),
            "(1.23, -0.50)"
        );
    }

    #[test]
    fn cursor_coordinates_follow_pointer_and_flip_inside_viewport_edges() {
        let viewport = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1280.0, 720.0));
        assert_eq!(
            cursor_label_position(egui::pos2(100.0, 80.0), viewport),
            egui::pos2(114.0, 94.0)
        );

        let near_edge = cursor_label_position(egui::pos2(1275.0, 715.0), viewport);
        assert!(near_edge.x < 1275.0);
        assert!(near_edge.y < 715.0);
        assert!(near_edge.x + 132.0 <= viewport.max.x);
        assert!(near_edge.y + 32.0 <= viewport.max.y);
    }

    #[test]
    fn overlay_coordinates_are_resolution_independent_logical_units() {
        for (width, height) in [(1280, 720), (1920, 1080), (3840, 2160)] {
            let mut window = Window::default();
            window.resolution.set(width as f32, height as f32);
            let camera = ResolvedCamera::new(
                Camera::ortho_2d_frame(16.0, 9.0, width, height),
                gaanim_math::CameraViewport::default(),
            );

            let top_left = egui_to_world(&camera, &window, egui::pos2(0.0, 0.0));
            let bottom_right =
                egui_to_world(&camera, &window, egui::pos2(width as f32, height as f32));
            assert!((top_left.x + 8.0).abs() < 1e-9);
            assert!((top_left.y - 4.5).abs() < 1e-9);
            assert!((bottom_right.x - 8.0).abs() < 1e-9);
            assert!((bottom_right.y + 4.5).abs() < 1e-9);

            let authored = glam::DVec3::new(2.25, -1.75, 0.0);
            let screen = world_to_egui(&camera, &window, authored);
            let restored = egui_to_world(&camera, &window, screen);
            assert!((restored - authored).length() < 1e-6);
        }
    }
}
