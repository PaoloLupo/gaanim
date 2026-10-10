//! The inspector: what the drawable selected in the preview is, the script
//! line that created it, and its properties at the current time. It shows
//! while the overlays are on (`O`) and something is selected.

use bevy::prelude::*;
use bevy_egui::{EguiContexts, egui};
use gaanim_core::peniko::Brush;
use gaanim_math::{GlobalSpatialTransform, SpatialTransform};
use gaanim_scene::{
    AuthoredObject, AuthoredObjects, DepthOfField, FillBrush, GlobalOpacity, HudOverlay, MobjectId,
    Opacity, ParallaxLayer, RenderOrder, StrokeBrush, Visible, ZLayer, components::TextSpan,
};

use crate::overlays::EditorOverlays;
use crate::ui_kit::{self, ButtonTone, Icon, caption, icon_button_sized, palette};
use crate::{EditorState, PresentationMode};

/// Whether the inspector shows the selection.
#[derive(Resource, Debug, Clone)]
pub struct InspectorPanel {
    pub enabled: bool,
}

impl Default for InspectorPanel {
    fn default() -> Self {
        Self { enabled: true }
    }
}

/// The entity of the nearest authored drawable at or above `entity`: the
/// one whose [`MobjectId`] the scene script created, rather than a glyph or
/// another part compiled inside it.
pub(crate) fn authored_entity(
    entity: Entity,
    authored: &AuthoredObjects,
    ids: &Query<&MobjectId>,
    parents: &Query<&ChildOf>,
) -> Option<Entity> {
    let mut current = entity;
    // A hierarchy deeper than this is a cycle, which Bevy rules out.
    for _ in 0..256 {
        if ids.get(current).is_ok_and(|id| authored.contains(&id.0)) {
            return Some(current);
        }
        current = parents.get(current).ok()?.parent();
    }
    None
}

/// What the inspector reads from the selected entity.
type InspectedData = (
    Option<&'static MobjectId>,
    Option<&'static SpatialTransform>,
    Option<&'static GlobalSpatialTransform>,
    Option<&'static FillBrush>,
    Option<&'static StrokeBrush>,
    Option<&'static Opacity>,
    Option<&'static GlobalOpacity>,
    Option<&'static RenderOrder>,
    Option<&'static ZLayer>,
    Option<&'static ParallaxLayer>,
    Has<HudOverlay>,
    Has<Visible>,
);

/// The inspector panel, docked to the right of the preview.
#[allow(clippy::too_many_arguments)]
pub fn inspector_panel_system(
    mut contexts: EguiContexts,
    overlays: Res<EditorOverlays>,
    mut panel: ResMut<InspectorPanel>,
    presentation: Res<PresentationMode>,
    mut state: ResMut<EditorState>,
    authored: Option<Res<AuthoredObjects>>,
    inspected: Query<InspectedData>,
    pickable: Query<crate::PickBoundsQueryData>,
    ids: Query<&MobjectId>,
    parents: Query<&ChildOf>,
    children: Query<&Children>,
    spans: Query<&TextSpan>,
    camera: Option<Res<gaanim_math::Camera>>,
    depth_of_field: Option<Res<DepthOfField>>,
) {
    if presentation.active || !overlays.enabled || !panel.enabled {
        return;
    }
    let Some(selected) = state.selected else {
        // Nothing selected: the scene camera at the current time.
        if let Some(camera) = camera
            && let Ok(ctx) = contexts.ctx_mut()
            && camera_panel(ctx, &camera, depth_of_field.as_deref())
        {
            panel.enabled = false;
        }
        return;
    };
    let Ok(data) = inspected.get(selected) else {
        // The selection was despawned by a reload.
        state.selected = None;
        return;
    };
    let Ok(ctx) = contexts.ctx_mut() else {
        return;
    };
    let authored = authored.as_deref();
    let object = authored.and_then(|authored| {
        data.0
            .and_then(|id| authored.get(&id.0))
            .map(|object| (authored, object))
    });
    let parent = authored.and_then(|authored| {
        let parent = parents.get(selected).ok()?.parent();
        let entity = authored_entity(parent, authored, &ids, &parents)?;
        let object = authored.get(&ids.get(entity).ok()?.0)?;
        Some((entity, object))
    });
    let child_count = children.get(selected).map_or(0, |children| children.len());
    let text = text_of(selected, &children, &spans);
    let bounds = crate::selection_bounds(selected, &pickable, &children);

    let mut close = false;
    let mut select = None;
    inspector_window().show(ctx, |ui| {
        ui.set_width(280.0);
        ui.spacing_mut().item_spacing.y = 6.0;
        ui.horizontal(|ui| {
            ui.label(caption("INSPECTOR"));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if icon_button_sized(ui, Icon::Close, ButtonTone::Ghost, true, 24.0)
                    .on_hover_text("Quitar la selección")
                    .clicked()
                {
                    close = true;
                }
            });
        });
        header(ui, object.map(|(_, object)| object), data.0);
        if let Some((_, object)) = object {
            location_row(ui, object);
        } else if authored.is_some() {
            muted(ui, "Una parte interna, sin línea propia en el script.");
        }
        if let Some(text) = &text {
            field(ui, "Texto", text);
        }

        section(ui, "TRANSFORMACIÓN");
        if let Some(transform) = data.1 {
            field(ui, "Posición", &format_vec(transform.translation));
            field(ui, "Rotación", &format_rotation(transform.rotation));
            field(ui, "Escala", &format_scale(transform.scale));
        }
        if let Some(bounds) = bounds {
            let size = bounds.max - bounds.min;
            field(
                ui,
                "Tamaño",
                &format!("{} × {}", number(size.x), number(size.y)),
            );
            field(ui, "Centro", &format_vec((bounds.min + bounds.max) * 0.5));
        } else if let Some(global) = data.2 {
            field(
                ui,
                "En la escena",
                &format_vec(global.mat4.w_axis.truncate()),
            );
        }

        section(ui, "ESTILO");
        match data.3 {
            Some(fill) => brush_field(ui, "Relleno", fill.0.as_ref()),
            None => field(ui, "Relleno", "—"),
        }
        match data.4 {
            Some(stroke) if stroke.brush.is_some() && stroke.style.width > 0.0 => {
                brush_field(ui, "Trazo", stroke.brush.as_ref());
                field(ui, "Grosor", &number(stroke.style.width));
            }
            _ => field(ui, "Trazo", "—"),
        }
        let opacity = data.5.map_or(1.0, |opacity| opacity.0);
        let global = data.6.map_or(opacity, |opacity| opacity.0);
        field(
            ui,
            "Opacidad",
            &if (global - opacity).abs() > 1e-3 {
                format!("{} (en pantalla {})", percent(opacity), percent(global))
            } else {
                percent(opacity)
            },
        );

        section(ui, "ORDEN");
        if let Some(order) = data.7 {
            field(ui, "z_index", &order.z_index.to_string());
        }
        if let Some(layer) = data.8 {
            field(ui, "Capa", &format!("rango {}", layer.0));
        }
        if let Some(parallax) = data.9 {
            field(
                ui,
                "Parallax",
                &format!("profundidad {}", number(parallax.depth)),
            );
        }
        if data.10 {
            field(ui, "HUD", "fijo en pantalla");
        }
        field(ui, "Visible", if data.11 { "sí" } else { "no" });
        if child_count > 0 {
            field(ui, "Hijos", &child_count.to_string());
        }
        if let Some((entity, parent)) = parent {
            ui.add_space(4.0);
            let label = format!("Dentro de {}", object_title(parent));
            if ui_kit::small_button(ui, &label, true)
                .on_hover_text("Seleccionar el grupo que lo contiene")
                .clicked()
            {
                select = Some(entity);
            }
        }
    });
    if close {
        state.selected = None;
    } else if let Some(entity) = select {
        state.selected = Some(entity);
    }
}

/// The inspector's window, docked to the right of the preview.
fn inspector_window() -> egui::Window<'static> {
    egui::Window::new("Inspector")
        .id(egui::Id::new("editor_inspector"))
        .title_bar(false)
        .anchor(egui::Align2::RIGHT_TOP, egui::vec2(-12.0, 62.0))
        .order(egui::Order::Foreground)
        .resizable(false)
        .collapsible(false)
        .default_width(280.0)
        .frame(ui_kit::card_frame().inner_margin(egui::Margin::same(14)))
}

/// The scene camera, shown while nothing is selected. Returns whether the
/// inspector was closed.
fn camera_panel(
    ctx: &egui::Context,
    camera: &gaanim_math::Camera,
    depth_of_field: Option<&DepthOfField>,
) -> bool {
    let mut close = false;
    inspector_window().show(ctx, |ui| {
        ui.set_width(280.0);
        ui.spacing_mut().item_spacing.y = 6.0;
        ui.horizontal(|ui| {
            ui.label(caption("INSPECTOR"));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if icon_button_sized(ui, Icon::Close, ButtonTone::Ghost, true, 24.0)
                    .on_hover_text("Ocultar el inspector")
                    .clicked()
                {
                    close = true;
                }
            });
        });
        ui.add(
            egui::Label::new(
                egui::RichText::new("Cámara de la escena")
                    .size(16.0)
                    .strong()
                    .color(palette::TEXT),
            )
            .wrap(),
        );
        muted(ui, "Un clic en un objeto lo inspecciona.");
        for (name, value) in camera_fields(camera) {
            field(ui, name, &value);
        }
        if let Some(depth_of_field) = depth_of_field.filter(|dof| dof.aperture > 0.0) {
            section(ui, "PROFUNDIDAD DE CAMPO");
            field(ui, "Enfoque", &number(depth_of_field.focus));
            field(ui, "Apertura", &number(depth_of_field.aperture));
            field(ui, "Desenfoque máx.", &number(depth_of_field.max_blur));
        }
    });
    close
}

/// The camera's pose and lens, as a script would set them.
fn camera_fields(camera: &gaanim_math::Camera) -> Vec<(&'static str, String)> {
    let mut fields = vec![
        ("Posición", format_vec(camera.position)),
        ("Rotación", format_rotation(camera.rotation)),
    ];
    match camera.projection {
        gaanim_math::Projection::Orthographic { zoom } => fields.push(("Zoom", number(zoom))),
        gaanim_math::Projection::Perspective { fov_y, near, far } => {
            fields.push((
                "FOV",
                format!(
                    "{} rad ({}°)",
                    number(fov_y),
                    number_with(fov_y.to_degrees(), 1)
                ),
            ));
            fields.push(("Recorte", format!("{} – {}", number(near), number(far))));
            fields.push(("Objetivo", format_vec(camera.target)));
        }
    }
    fields.push((
        "Encuadre",
        format!(
            "{} × {}",
            number(camera.frame_width),
            number(camera.frame_height)
        ),
    ));
    fields
}

fn header(ui: &mut egui::Ui, object: Option<&AuthoredObject>, id: Option<&MobjectId>) {
    let title = match (object, id) {
        (Some(object), _) => object_title(object),
        (None, Some(id)) => format!("objeto {}", id.0.index()),
        (None, None) => "entidad".to_owned(),
    };
    ui.add(
        egui::Label::new(
            egui::RichText::new(title)
                .size(16.0)
                .strong()
                .color(palette::TEXT),
        )
        .wrap(),
    );
}

/// `circle`, or `circle "logo"` when the script named it.
pub(crate) fn object_title(object: &AuthoredObject) -> String {
    match &object.name {
        Some(name) => format!("{} \"{name}\"", object.kind),
        None => object.kind.clone(),
    }
}

fn location_row(ui: &mut egui::Ui, object: &AuthoredObject) {
    let Some(location) = &object.location else {
        muted(ui, "Creado sin una línea de script conocida.");
        return;
    };
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new(crate::feedback::location_label(location))
                .size(12.5)
                .color(palette::ACCENT),
        )
        .on_hover_text(location.to_string());
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if crate::source_link::AVAILABLE
                && ui_kit::small_button(ui, "Abrir", true)
                    .on_hover_text(format!(
                        "Abrir esta línea en el editor de código\n{}",
                        crate::source_link::hint()
                    ))
                    .clicked()
            {
                crate::source_link::open_or_report(location);
            }
            if ui_kit::small_button(ui, "Copiar", true)
                .on_hover_text("Copiar archivo:línea")
                .clicked()
            {
                ui.ctx().copy_text(location.to_string());
            }
        });
    });
    // A drawable a script's own function created: the lines that called it.
    for caller in &object.callers {
        let text = format!("llamado desde {}", crate::feedback::location_label(caller));
        let response = ui
            .add(
                egui::Label::new(
                    egui::RichText::new(text)
                        .size(12.0)
                        .color(palette::TEXT_MUTED),
                )
                .sense(egui::Sense::click()),
            )
            .on_hover_cursor(egui::CursorIcon::PointingHand);
        let response = if crate::source_link::AVAILABLE {
            response.on_hover_text(format!("{caller}\nClic: abrir en el editor de código"))
        } else {
            response.on_hover_text(format!("{caller}\nClic: copiar"))
        };
        if response.clicked() {
            if crate::source_link::AVAILABLE {
                crate::source_link::open_or_report(caller);
            } else {
                ui.ctx().copy_text(caller.to_string());
            }
        }
    }
}

fn section(ui: &mut egui::Ui, title: &str) {
    ui.add_space(6.0);
    ui.label(caption(title));
}

fn muted(ui: &mut egui::Ui, text: &str) {
    ui.add(
        egui::Label::new(
            egui::RichText::new(text)
                .size(12.0)
                .color(palette::TEXT_FAINT),
        )
        .wrap(),
    );
}

fn field(ui: &mut egui::Ui, name: &str, value: &str) {
    ui.horizontal(|ui| {
        field_name(ui, name);
        ui.add(egui::Label::new(egui::RichText::new(value).size(12.5).color(palette::TEXT)).wrap());
    });
}

/// The name column of a field, left-aligned at a fixed width.
fn field_name(ui: &mut egui::Ui, name: &str) {
    ui.allocate_ui_with_layout(
        egui::vec2(86.0, 18.0),
        egui::Layout::left_to_right(egui::Align::Center),
        |ui| {
            ui.set_min_width(86.0);
            ui.label(
                egui::RichText::new(name)
                    .size(12.5)
                    .color(palette::TEXT_MUTED),
            );
        },
    );
}

fn brush_field(ui: &mut egui::Ui, name: &str, brush: Option<&Brush>) {
    let Some(Brush::Solid(color)) = brush else {
        field(ui, name, &brush_label(brush));
        return;
    };
    let rgba = color.to_rgba8();
    ui.horizontal(|ui| {
        field_name(ui, name);
        let (rect, _) = ui.allocate_exact_size(egui::vec2(14.0, 14.0), egui::Sense::hover());
        ui.painter().rect_filled(
            rect,
            0.0,
            egui::Color32::from_rgba_unmultiplied(rgba.r, rgba.g, rgba.b, rgba.a),
        );
        ui.painter().rect_stroke(
            rect,
            0.0,
            egui::Stroke::new(1.0, palette::TEXT_FAINT),
            egui::StrokeKind::Outside,
        );
        ui.label(
            egui::RichText::new(brush_label(brush))
                .size(12.5)
                .monospace()
                .color(palette::TEXT),
        );
    });
}

/// `#RRGGBB` (with alpha when not opaque), or what kind of paint it is.
fn brush_label(brush: Option<&Brush>) -> String {
    match brush {
        None => "—".to_owned(),
        Some(Brush::Solid(color)) => {
            let rgba = color.to_rgba8();
            if rgba.a == 255 {
                format!("#{:02X}{:02X}{:02X}", rgba.r, rgba.g, rgba.b)
            } else {
                format!("#{:02X}{:02X}{:02X}{:02X}", rgba.r, rgba.g, rgba.b, rgba.a)
            }
        }
        Some(Brush::Gradient(_)) => "degradado".to_owned(),
        Some(Brush::Image(_)) => "imagen".to_owned(),
    }
}

/// The characters of the text an entity draws, in order, from the glyphs
/// below it; `None` when it draws no text.
fn text_of(
    entity: Entity,
    children: &Query<&Children>,
    spans: &Query<&TextSpan>,
) -> Option<String> {
    /// Glyphs read at most, so a long text costs little per frame.
    const MAX_GLYPHS: usize = 400;
    let mut glyphs = Vec::new();
    let mut pending = vec![entity];
    while let Some(current) = pending.pop() {
        if glyphs.len() >= MAX_GLYPHS {
            break;
        }
        if let Ok(span) = spans.get(current) {
            glyphs.push((span.char_index, span.character));
        }
        if let Ok(list) = children.get(current) {
            pending.extend(list.iter());
        }
    }
    if glyphs.is_empty() {
        return None;
    }
    glyphs.sort_unstable_by_key(|(index, _)| *index);
    glyphs.dedup_by_key(|(index, _)| *index);
    let text: String = glyphs.into_iter().map(|(_, character)| character).collect();
    Some(shorten(&text, 80))
}

fn shorten(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_owned();
    }
    let mut short: String = text.chars().take(max_chars - 1).collect();
    short.push('…');
    short
}

/// The rotation in radians, as scripts give it (`rotate_to`), with the
/// angle about z also in degrees; a 3D rotation lists its Euler angles.
fn format_rotation(rotation: glam::DQuat) -> String {
    let (x, y, z) = rotation.to_euler(glam::EulerRot::XYZ);
    if x.abs() < 1e-9 && y.abs() < 1e-9 {
        format!("{} rad ({}°)", number(z), number_with(z.to_degrees(), 1))
    } else {
        format!("({}, {}, {}) rad", number(x), number(y), number(z))
    }
}

/// A number with up to three decimals and no trailing zeros.
fn number(value: f64) -> String {
    number_with(value, 3)
}

fn number_with(value: f64, decimals: usize) -> String {
    let text = format!("{value:.decimals$}");
    let text = if text.contains('.') {
        text.trim_end_matches('0').trim_end_matches('.')
    } else {
        &text
    };
    match text {
        "-0" | "" => "0".to_owned(),
        text => text.to_owned(),
    }
}

fn format_vec(value: glam::DVec3) -> String {
    if value.z.abs() < 1e-9 {
        format!("({}, {})", number(value.x), number(value.y))
    } else {
        format!(
            "({}, {}, {})",
            number(value.x),
            number(value.y),
            number(value.z)
        )
    }
}

fn format_scale(scale: glam::DVec3) -> String {
    let flat = (scale.z - 1.0).abs() < 1e-9;
    if flat && (scale.x - scale.y).abs() < 1e-9 {
        number(scale.x)
    } else if flat {
        format!("({}, {})", number(scale.x), number(scale.y))
    } else {
        format_vec(scale)
    }
}

fn percent(value: f32) -> String {
    format!("{:.0} %", value * 100.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::ecs::system::RunSystemOnce;
    use gaanim_core::peniko::Color;

    #[test]
    fn numbers_read_like_a_script_would_write_them() {
        assert_eq!(number(1.5), "1.5");
        assert_eq!(number(2.0), "2");
        assert_eq!(number(-0.0001), "0");
        assert_eq!(number(0.333333), "0.333");
        assert_eq!(format_vec(glam::DVec3::new(1.0, -2.5, 0.0)), "(1, -2.5)");
        assert_eq!(format_vec(glam::DVec3::new(1.0, 0.0, 3.0)), "(1, 0, 3)");
        assert_eq!(format_scale(glam::DVec3::new(2.0, 2.0, 1.0)), "2");
        assert_eq!(format_scale(glam::DVec3::new(2.0, 1.0, 1.0)), "(2, 1)");
    }

    #[test]
    fn rotations_are_shown_in_radians_like_scripts_write_them() {
        let quarter = glam::DQuat::from_rotation_z(std::f64::consts::FRAC_PI_2);
        assert_eq!(format_rotation(quarter), "1.571 rad (90°)");
        assert_eq!(format_rotation(glam::DQuat::IDENTITY), "0 rad (0°)");
        let tilted = glam::DQuat::from_euler(glam::EulerRot::XYZ, 0.5, 0.0, 0.25);
        assert_eq!(format_rotation(tilted), "(0.5, 0, 0.25) rad");
    }

    #[test]
    fn the_camera_lists_its_lens_like_a_script_sets_it() {
        let mut camera = gaanim_math::Camera::ortho_2d_frame(16.0, 9.0, 1920, 1080);
        camera.position = glam::DVec3::new(2.0, -1.5, 0.0);
        camera.projection = gaanim_math::Projection::Orthographic { zoom: 1.6 };
        let fields = camera_fields(&camera);
        assert!(fields.contains(&("Posición", "(2, -1.5)".to_owned())));
        assert!(fields.contains(&("Zoom", "1.6".to_owned())));
        assert!(fields.contains(&("Encuadre", "16 × 9".to_owned())));
        camera.projection = gaanim_math::Projection::Perspective {
            fov_y: 0.5,
            near: 0.1,
            far: 100.0,
        };
        let fields = camera_fields(&camera);
        assert!(fields.contains(&("FOV", "0.5 rad (28.6°)".to_owned())));
        assert!(fields.contains(&("Recorte", "0.1 – 100".to_owned())));
        assert!(!fields.iter().any(|(name, _)| *name == "Zoom"));
    }

    #[test]
    fn solid_brushes_are_named_by_their_hex_color() {
        let red = Brush::Solid(Color::from_rgba8(255, 0, 0, 255));
        assert_eq!(brush_label(Some(&red)), "#FF0000");
        let faded = Brush::Solid(Color::from_rgba8(0, 128, 255, 128));
        assert_eq!(brush_label(Some(&faded)), "#0080FF80");
        assert_eq!(brush_label(None), "—");
    }

    #[test]
    fn long_texts_are_shortened() {
        assert_eq!(shorten("hola", 80), "hola");
        assert_eq!(shorten("abcdef", 4), "abc…");
    }

    #[test]
    fn a_part_resolves_to_the_drawable_the_script_authored() {
        let mut world = World::new();
        let id = gaanim_core::ObjectId::from_raw(5);
        let root = world.spawn(MobjectId(id)).id();
        let glyph = world
            .spawn((
                MobjectId(gaanim_core::ObjectId::from_raw(99)),
                ChildOf(root),
            ))
            .id();
        let stray = world.spawn_empty().id();
        let authored = AuthoredObjects::new(gaanim_scene::AuthoredIndex {
            objects: std::collections::HashMap::from([(
                id,
                AuthoredObject {
                    kind: "text".to_owned(),
                    name: None,
                    location: None,
                    callers: Vec::new(),
                },
            )]),
            plays: Default::default(),
        });
        world.insert_resource(authored);
        let found = world
            .run_system_once(
                move |authored: Res<AuthoredObjects>,
                      ids: Query<&MobjectId>,
                      parents: Query<&ChildOf>| {
                    (
                        authored_entity(glyph, &authored, &ids, &parents),
                        authored_entity(stray, &authored, &ids, &parents),
                    )
                },
            )
            .unwrap();
        assert_eq!(found, (Some(root), None));
    }
}
