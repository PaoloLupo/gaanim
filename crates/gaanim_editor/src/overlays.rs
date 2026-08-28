use bevy::prelude::*;
use bevy_egui::{EguiContexts, egui};
use gaanim_math::{Camera, ResolvedCamera};
use gaanim_renderer::pipeline::CanvasBackground;

use crate::{PresentationMode, PreviewInteractive};

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
    /// Mostrar grilla dentro del canvas.
    pub show_grid: bool,
}

impl Default for EditorOverlays {
    fn default() -> Self {
        Self {
            enabled: false, // oculto al inicio, como modo interactivo
            show_bounds: true,
            show_coords: true,
            show_grid: false,
        }
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

    let raw_step = TARGET_SPACING_PX / pixels_per_unit;
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

fn world_to_egui(cam: &ResolvedCamera, window: &Window, world: glam::DVec3) -> egui::Pos2 {
    if matches!(cam.projection, gaanim_math::Projection::Perspective { .. }) {
        let s = cam.world_to_screen(world);
        return egui::pos2(s.x as f32, s.y as f32);
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

fn egui_to_world(cam: &ResolvedCamera, window: &Window, screen: egui::Pos2) -> glam::DVec3 {
    if matches!(cam.projection, gaanim_math::Projection::Perspective { .. }) {
        return cam.screen_to_world(glam::DVec2::new(screen.x as f64, screen.y as f64));
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

/// Panel flotante con toggles de overlays.
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

    // Barra superior flotante (no panel) - solo en modo overlays
    egui::Area::new("overlays_top_bar".into())
        .anchor(egui::Align2::CENTER_TOP, egui::vec2(0.0, 6.0))
        .order(egui::Order::Foreground)
        .interactable(true)
        .show(ctx, |ui| {
            egui::Frame::new()
                .fill(egui::Color32::from_rgba_premultiplied(18, 18, 24, 220))
                .corner_radius(8.0)
                .inner_margin(egui::Margin::symmetric(10, 4))
                .stroke(egui::Stroke::new(
                    1.0,
                    egui::Color32::from_rgba_premultiplied(65, 65, 80, 140),
                ))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(
                            egui::RichText::new("Vista:")
                                .size(11.0)
                                .color(egui::Color32::from_rgb(160, 160, 175)),
                        );
                        let interactive_btn = egui::Button::new(
                            egui::RichText::new(if interactive.enabled {
                                "Interactivo: ON"
                            } else {
                                "Interactivo: OFF"
                            })
                            .size(11.0)
                            .color(if interactive.enabled {
                                egui::Color32::from_rgb(120, 235, 150)
                            } else {
                                egui::Color32::from_rgb(150, 150, 165)
                            }),
                        )
                        .min_size(egui::vec2(104.0, 18.0))
                        .corner_radius(3.0)
                        .fill(egui::Color32::from_rgba_premultiplied(30, 30, 45, 140));
                        if ui.add(interactive_btn).clicked() {
                            interactive.toggle(authored_camera.as_deref().copied());
                        }
                        ui.separator();
                        let bounds_btn = egui::Button::new(
                            egui::RichText::new(if overlays.show_bounds {
                                "⬜ Límites ✓"
                            } else {
                                "⬜ Límites"
                            })
                            .size(11.0)
                            .color(if overlays.show_bounds {
                                egui::Color32::from_rgb(255, 200, 80)
                            } else {
                                egui::Color32::from_rgb(150, 150, 165)
                            }),
                        )
                        .min_size(egui::vec2(78.0, 18.0))
                        .corner_radius(3.0)
                        .fill(egui::Color32::from_rgba_premultiplied(30, 30, 45, 140));
                        if ui
                            .add(bounds_btn)
                            .on_hover_text("B: alternar límites del canvas")
                            .clicked()
                        {
                            overlays.show_bounds = !overlays.show_bounds;
                        }
                        let coords_btn = egui::Button::new(
                            egui::RichText::new(if overlays.show_coords {
                                "⊕ Coords ✓"
                            } else {
                                "⊕ Coords"
                            })
                            .size(11.0)
                            .color(if overlays.show_coords {
                                egui::Color32::from_rgb(90, 220, 120)
                            } else {
                                egui::Color32::from_rgb(150, 150, 165)
                            }),
                        )
                        .min_size(egui::vec2(78.0, 18.0))
                        .corner_radius(3.0)
                        .fill(egui::Color32::from_rgba_premultiplied(30, 30, 45, 140));
                        if ui
                            .add(coords_btn)
                            .on_hover_text("C: alternar ejes y coordenadas del cursor")
                            .clicked()
                        {
                            overlays.show_coords = !overlays.show_coords;
                        }
                        let grid_btn = egui::Button::new(
                            egui::RichText::new(if overlays.show_grid {
                                "▦ Grilla ✓"
                            } else {
                                "▦ Grilla"
                            })
                            .size(11.0)
                            .color(if overlays.show_grid {
                                egui::Color32::from_rgb(120, 180, 255)
                            } else {
                                egui::Color32::from_rgb(150, 150, 165)
                            }),
                        )
                        .min_size(egui::vec2(70.0, 18.0))
                        .corner_radius(3.0)
                        .fill(egui::Color32::from_rgba_premultiplied(30, 30, 45, 140));
                        if ui
                            .add(grid_btn)
                            .on_hover_text("G: alternar grilla en unidades lógicas")
                            .clicked()
                        {
                            overlays.show_grid = !overlays.show_grid;
                        }
                    });
                });
        });
}

/// Sistema que dibuja límites del canvas y coordenadas sobre el viewport.
/// Solo cuando el modo overlays está activo (`enabled=true`, tecla `O`).
pub fn scene_overlays_system(
    mut contexts: EguiContexts,
    overlays: Res<EditorOverlays>,
    camera: Option<Res<ResolvedCamera>>,
    canvas_bg: Option<Res<CanvasBackground>>,
    windows: Query<&Window, With<bevy::window::PrimaryWindow>>,
    presentation: Res<PresentationMode>,
) {
    if presentation.active || !overlays.enabled {
        return;
    }
    if !overlays.show_bounds && !overlays.show_coords && !overlays.show_grid {
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
    let (bmin, bmax, label) = if let Some(bg) = canvas_bg.as_ref() {
        let b = bg.bounds;
        let label = format!(
            "{} × {} unidades lógicas",
            cam.frame_width, cam.frame_height
        );
        (
            glam::DVec2::new(b.min.x, b.min.y),
            glam::DVec2::new(b.max.x, b.max.y),
            label,
        )
    } else {
        let hw = cam.frame_width * 0.5;
        let hh = cam.frame_height * 0.5;
        (
            glam::DVec2::new(-hw, -hh),
            glam::DVec2::new(hw, hh),
            format!(
                "{} × {} unidades lógicas",
                cam.frame_width, cam.frame_height
            ),
        )
    };

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
        let corners_world = [
            glam::DVec3::new(bmin.x, bmin.y, 0.0),
            glam::DVec3::new(bmax.x, bmin.y, 0.0),
            glam::DVec3::new(bmax.x, bmax.y, 0.0),
            glam::DVec3::new(bmin.x, bmax.y, 0.0),
        ];
        corners_world
            .iter()
            .map(|w| world_to_egui(&cam, window, *w))
            .collect()
    };

    // Área de dibujo full-screen no interactiva
    egui::Area::new("scene_overlays".into())
        .fixed_pos(egui::pos2(0.0, 0.0))
        .interactable(false)
        .show(ctx, |ui| {
            let vp = ctx.viewport_rect();
            let _ = ui.allocate_space(vp.size());
            let painter = ui.painter();

            // --- Límites del área real (canvas) ---
            if overlays.show_bounds {
                let stroke = egui::Stroke::new(1.5, egui::Color32::from_rgb(255, 200, 80));
                // Dibujar borde (sólido con esquinas y dash sutil)
                for i in 0..4 {
                    painter.line_segment([corners_screen[i], corners_screen[(i + 1) % 4]], stroke);
                }
                // Esquinas en L para mayor visibilidad
                let corner_len = 10.0;
                for &c in &corners_screen {
                    // buscar dos vecinos para orientar L
                    // simplificado: dibujar pequeña cruz en cada esquina
                    painter.line_segment(
                        [
                            egui::pos2(c.x - corner_len, c.y),
                            egui::pos2(c.x + 4.0, c.y),
                        ],
                        stroke,
                    );
                    painter.line_segment(
                        [
                            egui::pos2(c.x, c.y - corner_len),
                            egui::pos2(c.x, c.y + 4.0),
                        ],
                        stroke,
                    );
                }
                // Etiqueta dimensiones en borde superior
                let top_center = egui::pos2(
                    (corners_screen[0].x + corners_screen[2].x) * 0.5,
                    corners_screen[0]
                        .y
                        .min(corners_screen[1].y)
                        .min(corners_screen[2].y)
                        .min(corners_screen[3].y),
                );
                let galley = painter.layout_no_wrap(
                    label.clone(),
                    egui::FontId::proportional(10.0),
                    egui::Color32::from_rgb(220, 200, 140),
                );
                let text_pos = egui::pos2(
                    top_center.x - galley.size().x * 0.5,
                    top_center.y - galley.size().y - 4.0,
                );
                let bg_rect = egui::Rect::from_min_size(
                    text_pos - egui::vec2(4.0, 1.0),
                    galley.size() + egui::vec2(8.0, 2.0),
                );
                painter.rect_filled(
                    bg_rect,
                    3.0,
                    egui::Color32::from_rgba_premultiplied(18, 18, 22, 200),
                );
                painter.galley(text_pos, galley, egui::Color32::from_rgb(220, 200, 140));
            }

            // --- Coordenadas y grilla ---
            // En perspectiva la grilla 2D no tiene sentido (el mundo es 3D); solo mostrar en ortho
            if overlays.show_grid && !is_perspective {
                let grid_color = egui::Color32::from_rgba_premultiplied(100, 100, 110, 45);
                let grid_stroke = egui::Stroke::new(0.7, grid_color);
                let step = logical_grid_step(effective_zoom(&cam));
                // Líneas verticales
                let mut x = (bmin.x / step).ceil() * step;
                while x <= bmax.x + 1e-6 {
                    if x.abs() < 1e-6 {
                        x += step;
                        continue;
                    }
                    let a = world_to_egui(&cam, window, glam::DVec3::new(x, bmin.y, 0.0));
                    let b = world_to_egui(&cam, window, glam::DVec3::new(x, bmax.y, 0.0));
                    painter.line_segment([a, b], grid_stroke);
                    x += step;
                }
                // Líneas horizontales
                let mut y = (bmin.y / step).ceil() * step;
                while y <= bmax.y + 1e-6 {
                    if y.abs() < 1e-6 {
                        y += step;
                        continue;
                    }
                    let a = world_to_egui(&cam, window, glam::DVec3::new(bmin.x, y, 0.0));
                    let b = world_to_egui(&cam, window, glam::DVec3::new(bmax.x, y, 0.0));
                    painter.line_segment([a, b], grid_stroke);
                    y += step;
                }
            }

            if overlays.show_coords && !is_perspective {
                // Ejes X/Y si el origen está dentro del canvas (solo ortho; en perspectiva los ejes 3D ya existen)
                let origin_visible =
                    bmin.x <= 0.0 && bmax.x >= 0.0 && bmin.y <= 0.0 && bmax.y >= 0.0;
                if origin_visible {
                    let x_start = world_to_egui(&cam, window, glam::DVec3::new(bmin.x, 0.0, 0.0));
                    let x_end = world_to_egui(&cam, window, glam::DVec3::new(bmax.x, 0.0, 0.0));
                    let y_start = world_to_egui(&cam, window, glam::DVec3::new(0.0, bmin.y, 0.0));
                    let y_end = world_to_egui(&cam, window, glam::DVec3::new(0.0, bmax.y, 0.0));
                    let x_stroke = egui::Stroke::new(1.4, egui::Color32::from_rgb(255, 90, 90));
                    let y_stroke = egui::Stroke::new(1.4, egui::Color32::from_rgb(90, 220, 120));
                    painter.line_segment([x_start, x_end], x_stroke);
                    painter.line_segment([y_start, y_end], y_stroke);
                    // Origen
                    let origin = world_to_egui(&cam, window, glam::DVec3::ZERO);
                    painter.circle_filled(origin, 3.0, egui::Color32::WHITE);
                    painter.circle_stroke(
                        origin,
                        3.0,
                        egui::Stroke::new(1.0, egui::Color32::from_rgb(60, 60, 70)),
                    );
                    // Flechas simples en extremos positivos
                    let arrow_len = 8.0;
                    // Calcular dirección en screen space para flecha
                    // X positivo hacia derecha en world -> en screen depende de rotación/zoom
                    let dir_x = (x_end - x_start).normalized() * arrow_len;
                    let dir_y = (y_end - y_start).normalized() * arrow_len;
                    // Punta X
                    painter.line_segment(
                        [
                            x_end,
                            x_end - egui::vec2(dir_x.x - dir_x.y * 0.4, dir_x.y + dir_x.x * 0.4),
                        ],
                        x_stroke,
                    );
                    painter.line_segment(
                        [
                            x_end,
                            x_end - egui::vec2(dir_x.x + dir_x.y * 0.4, dir_x.y - dir_x.x * 0.4),
                        ],
                        x_stroke,
                    );
                    // Punta Y
                    painter.line_segment(
                        [
                            y_end,
                            y_end - egui::vec2(dir_y.x - dir_y.y * 0.4, dir_y.y + dir_y.x * 0.4),
                        ],
                        y_stroke,
                    );
                    painter.line_segment(
                        [
                            y_end,
                            y_end - egui::vec2(dir_y.x + dir_y.y * 0.4, dir_y.y - dir_y.x * 0.4),
                        ],
                        y_stroke,
                    );
                    // Etiquetas ejes
                    painter.text(
                        x_end + egui::vec2(6.0, -8.0),
                        egui::Align2::LEFT_CENTER,
                        "X",
                        egui::FontId::proportional(11.0),
                        egui::Color32::from_rgb(255, 90, 90),
                    );
                    painter.text(
                        y_end + egui::vec2(6.0, -6.0),
                        egui::Align2::LEFT_CENTER,
                        "Y",
                        egui::FontId::proportional(11.0),
                        egui::Color32::from_rgb(90, 220, 120),
                    );
                    // Etiquetas numéricas en el mismo paso lógico adaptativo que la grilla.
                    let tick_len = 4.0;
                    let step = logical_grid_step(effective_zoom(&cam));
                    let text_color = egui::Color32::from_rgba_premultiplied(180, 180, 190, 190);
                    let font = egui::FontId::proportional(9.0);
                    let mut x = (bmin.x / step).ceil() * step;
                    while x <= bmax.x + 1e-6 {
                        if x.abs() > 1e-6 {
                            let p = world_to_egui(&cam, window, glam::DVec3::new(x, 0.0, 0.0));
                            // tick perpendicular a X (vertical)
                            let perp = egui::vec2(0.0, tick_len);
                            painter.line_segment(
                                [p - perp, p + perp],
                                egui::Stroke::new(0.8, egui::Color32::from_rgb(255, 90, 90)),
                            );
                            painter.text(
                                p + egui::vec2(0.0, 9.0),
                                egui::Align2::CENTER_TOP,
                                format_logical_value(x, step),
                                font.clone(),
                                text_color,
                            );
                        }
                        x += step;
                    }
                    let mut y = (bmin.y / step).ceil() * step;
                    while y <= bmax.y + 1e-6 {
                        if y.abs() > 1e-6 {
                            let p = world_to_egui(&cam, window, glam::DVec3::new(0.0, y, 0.0));
                            let perp = egui::vec2(tick_len, 0.0);
                            painter.line_segment(
                                [p - perp, p + perp],
                                egui::Stroke::new(0.8, egui::Color32::from_rgb(90, 220, 120)),
                            );
                            painter.text(
                                p + egui::vec2(6.0, 0.0),
                                egui::Align2::LEFT_CENTER,
                                format_logical_value(y, step),
                                font.clone(),
                                text_color,
                            );
                        }
                        y += step;
                    }
                }
            }
        });

    // --- Coordenadas del cursor (fuera del Area para que use viewport_rect) ---
    if overlays.show_coords
        && let Some(cursor) = window.cursor_position()
    {
        let world = egui_to_world(&cam, window, egui::pos2(cursor.x, cursor.y));
        let text = format!("({:.2}, {:.2})", world.x, world.y);
        let Ok(ctx) = contexts.ctx_mut() else {
            return;
        };
        let label_position =
            cursor_label_position(egui::pos2(cursor.x, cursor.y), ctx.viewport_rect());
        egui::Area::new("mouse_coords".into())
            .fixed_pos(label_position)
            .order(egui::Order::Tooltip)
            .interactable(false)
            .show(ctx, |ui| {
                egui::Frame::new()
                    .fill(egui::Color32::from_rgba_premultiplied(18, 18, 24, 210))
                    .corner_radius(6.0)
                    .inner_margin(egui::Margin::symmetric(8, 4))
                    .stroke(egui::Stroke::new(
                        1.0,
                        egui::Color32::from_rgba_premultiplied(70, 70, 85, 160),
                    ))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(
                                egui::RichText::new("◉")
                                    .color(egui::Color32::from_rgb(90, 220, 120))
                                    .size(10.0),
                            );
                            ui.monospace(
                                egui::RichText::new(text)
                                    .color(egui::Color32::from_rgb(220, 220, 230))
                                    .size(11.0),
                            );
                            ui.label(
                                egui::RichText::new(" u")
                                    .color(egui::Color32::from_rgb(150, 150, 165))
                                    .size(10.0),
                            );
                        });
                    });
            });
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
    if keys.just_pressed(KeyCode::KeyC) {
        overlays.show_coords = !overlays.show_coords;
    }
    if keys.just_pressed(KeyCode::KeyG) {
        overlays.show_grid = !overlays.show_grid;
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
