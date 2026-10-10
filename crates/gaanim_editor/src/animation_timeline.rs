//! The animation timeline: one row per drawable the script authored, one
//! block per `play` that animates it, along a ruler with the playhead. A
//! click on a block goes to its start and selects the drawable; a double
//! click opens the `play`'s line in the code editor. `T` shows or hides it.

use std::collections::HashMap;

use bevy::prelude::*;
use bevy_egui::{EguiContexts, egui, input::EguiWantsInput};
use gaanim_core::ObjectId;
use gaanim_core::console::ScriptLocation;
use gaanim_scene::{AuthoredObjects, MobjectId};
use gaanim_timeline::clip::ClipPayload;
use gaanim_timeline::timeline::Timeline;

use crate::overlays::EditorOverlays;
use crate::ui_kit::{self, ButtonTone, Icon, caption, icon_button_sized, palette};
use crate::{EditorState, PresentationMode};

/// Height of one row.
const ROW_HEIGHT: f32 = 22.0;
/// Rows shown before the list scrolls.
const VISIBLE_ROWS: usize = 7;
/// Width of the column of drawable names.
const LABEL_WIDTH: f32 = 168.0;
/// Height of the ruler.
const RULER_HEIGHT: f32 = 20.0;

/// The panel, and the rows it last built from the timeline.
#[derive(Resource, Default)]
pub struct AnimationTimeline {
    pub open: bool,
    /// Show the whole scene rather than the segment under the playhead.
    whole_scene: bool,
    /// Text a row's name or an animation's name must contain.
    query: String,
    model: Model,
    /// The timeline's clip revision the model was built from.
    built_from: Option<u64>,
    /// Height drawn last frame, so the console can sit above the panel.
    pub(crate) height: f32,
}

/// What a row stands for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
enum RowKey {
    Object(ObjectId),
    Camera,
}

#[derive(Debug, Clone, Default)]
struct Model {
    rows: Vec<Row>,
}

#[derive(Debug, Clone)]
struct Row {
    key: RowKey,
    title: String,
    /// The entity the inspector selects for this row.
    entity: Option<Entity>,
    /// Where the script created the drawable.
    location: Option<ScriptLocation>,
    blocks: Vec<Block>,
}

/// One `play` animating a row: every clip it made there, merged.
#[derive(Debug, Clone, PartialEq)]
struct Block {
    start: f64,
    end: f64,
    /// The animation's label (`Write`) or the property it moves (`opacity`).
    name: String,
    category: Category,
    /// The easing, as a script names it.
    rate: String,
    /// The `play` it came from (`Timeline::clip_origins`).
    origin: Option<u64>,
    /// Clips merged into it, such as one per glyph of a written text.
    clips: usize,
}

/// What kind of change an animation makes, which colours its block.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Category {
    Motion,
    Transform,
    Appearance,
    Style,
    Stroke,
    Camera,
    Other,
}

impl Category {
    fn of(name: &str) -> Self {
        let name = name.to_ascii_lowercase();
        let has = |words: &[&str]| words.iter().any(|word| name.contains(word));
        if name.starts_with("camera") {
            Self::Camera
        } else if has(&["translation", "position", "path_follow", "move", "shift"]) {
            Self::Motion
        } else if has(&["rotat", "scale", "transform", "skew", "morph", "matching"]) {
            Self::Transform
        } else if has(&["opacity", "fade", "presence", "visib", "appear"]) {
            Self::Appearance
        } else if has(&[
            "fill",
            "stroke_brush",
            "color",
            "colour",
            "glow",
            "brush",
            "material",
        ]) {
            Self::Style
        } else if has(&[
            "write",
            "draw",
            "create",
            "reveal",
            "completion",
            "trim",
            "path",
        ]) {
            Self::Stroke
        } else {
            Self::Other
        }
    }

    fn color(self) -> egui::Color32 {
        match self {
            Self::Motion => palette::ACCENT,
            Self::Transform => egui::Color32::from_rgb(167, 139, 250),
            Self::Appearance => palette::LOOP,
            Self::Style => palette::STOP,
            Self::Stroke => egui::Color32::from_rgb(244, 114, 182),
            Self::Camera => egui::Color32::from_rgb(45, 212, 191),
            Self::Other => palette::TEXT_MUTED,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Motion => "movimiento",
            Self::Transform => "transformación",
            Self::Appearance => "aparición",
            Self::Style => "estilo",
            Self::Stroke => "trazo",
            Self::Camera => "cámara",
            Self::Other => "otra",
        }
    }
}

/// One animation clip, as the model reads it.
#[derive(Debug, Clone)]
struct ClipView {
    start: f64,
    end: f64,
    target: ObjectId,
    name: String,
    rate: String,
    origin: Option<u64>,
    /// Whether it moves the camera rather than a drawable.
    camera: bool,
}

/// The rows `clips` make: `row_of` says which row a clip's target belongs
/// to. Blocks merge the clips of one `play` on one row; rows follow the
/// order their first animation starts in.
fn build_rows(
    clips: &[ClipView],
    row_of: impl Fn(&ClipView) -> RowKey,
) -> Vec<(RowKey, Vec<Block>)> {
    let mut rows: HashMap<RowKey, Vec<Block>> = HashMap::new();
    for clip in clips {
        let blocks = rows.entry(row_of(clip)).or_default();
        let same_play = |block: &&mut Block| {
            block.origin.is_some() && block.origin == clip.origin && block.name == clip.name
        };
        match blocks.iter_mut().find(|block| same_play(block)) {
            Some(block) => {
                block.start = block.start.min(clip.start);
                block.end = block.end.max(clip.end);
                block.clips += 1;
            }
            None => blocks.push(Block {
                start: clip.start,
                end: clip.end,
                category: if clip.camera {
                    Category::Camera
                } else {
                    Category::of(&clip.name)
                },
                name: clip.name.clone(),
                rate: clip.rate.clone(),
                origin: clip.origin,
                clips: 1,
            }),
        }
    }
    let mut rows: Vec<_> = rows.into_iter().collect();
    for (_, blocks) in &mut rows {
        blocks.sort_by(|a, b| a.start.total_cmp(&b.start));
    }
    rows.sort_by(|(key_a, a), (key_b, b)| {
        let first = |blocks: &Vec<Block>| blocks.first().map_or(f64::INFINITY, |block| block.start);
        first(a).total_cmp(&first(b)).then(key_a.cmp(key_b))
    });
    rows
}

/// The part of the timeline the panel shows: the segment under the playhead,
/// or the whole scene.
fn view_window(timeline: &Timeline, whole_scene: bool) -> (f64, f64) {
    let duration = timeline.cached_duration.max(0.01);
    if !whole_scene
        && let Some(segment) = timeline.segments.iter().find(|segment| {
            segment.start_time <= timeline.current_time && timeline.current_time < segment.end_time
        })
        && segment.end_time > segment.start_time
    {
        return (segment.start_time, segment.end_time);
    }
    (0.0, duration)
}

/// `T` shows or hides the panel.
pub fn animation_timeline_keys_system(
    egui_wants: Res<EguiWantsInput>,
    keys: Res<ButtonInput<KeyCode>>,
    presentation: Res<PresentationMode>,
    mut panel: ResMut<AnimationTimeline>,
) {
    if presentation.active || egui_wants.wants_keyboard_input() {
        return;
    }
    if keys.just_pressed(KeyCode::KeyT) {
        panel.open = !panel.open;
    }
}

/// Rebuild the rows when the clips change, while the panel is open.
pub fn animation_timeline_model_system(
    mut panel: ResMut<AnimationTimeline>,
    timeline: Res<Timeline>,
    authored: Option<Res<AuthoredObjects>>,
    entities: Query<(Entity, &MobjectId)>,
    ids: Query<&MobjectId>,
    parents: Query<&ChildOf>,
) {
    if !panel.open {
        return;
    }
    let revision = timeline.clip_revision();
    let authored_changed = authored
        .as_ref()
        .is_some_and(|authored| authored.is_changed());
    if panel.built_from == Some(revision) && !authored_changed {
        return;
    }
    panel.built_from = Some(revision);
    let authored = authored.as_deref();
    let clips: Vec<ClipView> = timeline
        .clips
        .iter()
        .filter_map(|(id, clip)| match &clip.payload {
            ClipPayload::Animation(animation) if clip.duration > 0.0 => {
                let lens = gaanim_core::names::variant_name(&animation.lens);
                Some(ClipView {
                    start: clip.start,
                    end: clip.end(),
                    target: animation.target,
                    camera: lens.starts_with("camera"),
                    name: animation.label.clone().unwrap_or(lens),
                    rate: gaanim_core::names::variant_name(&animation.rate_func),
                    origin: timeline.clip_origins.get(&id).copied(),
                })
            }
            _ => None,
        })
        .collect();
    let by_id: HashMap<ObjectId, Entity> =
        entities.iter().map(|(entity, id)| (id.0, entity)).collect();
    // Glyphs and other compiled parts fold into the drawable that the
    // script authored.
    let authored_of = |target: ObjectId| -> Option<(ObjectId, Entity)> {
        let authored = authored?;
        let entity = *by_id.get(&target)?;
        let found = crate::inspector::authored_entity(entity, authored, &ids, &parents)?;
        Some((ids.get(found).ok()?.0, found))
    };
    let rows = build_rows(&clips, |clip| {
        if clip.camera {
            return RowKey::Camera;
        }
        RowKey::Object(authored_of(clip.target).map_or(clip.target, |(id, _)| id))
    });
    panel.model = Model {
        rows: rows
            .into_iter()
            .map(|(key, blocks)| {
                let (title, entity, location) = match key {
                    RowKey::Camera => ("cámara".to_owned(), None, None),
                    RowKey::Object(id) => {
                        let object = authored.and_then(|authored| authored.get(&id));
                        (
                            object
                                .map(crate::inspector::object_title)
                                .unwrap_or_else(|| format!("objeto {}", id.index())),
                            by_id.get(&id).copied(),
                            object.and_then(|object| object.location.clone()),
                        )
                    }
                };
                Row {
                    key,
                    title,
                    entity,
                    location,
                    blocks,
                }
            })
            .collect(),
    };
}

/// What the person did in the panel this frame.
#[derive(Default)]
struct Actions {
    seek: Option<f64>,
    select: Option<Entity>,
    /// Stop playback, to look at the instant sought.
    pause: bool,
    open: Option<ScriptLocation>,
    close: bool,
}

/// The panel, above the playback bar.
#[allow(clippy::too_many_arguments)]
pub fn animation_timeline_panel_system(
    mut contexts: EguiContexts,
    mut panel: ResMut<AnimationTimeline>,
    mut timeline: ResMut<Timeline>,
    mut state: ResMut<EditorState>,
    mut overlays: ResMut<EditorOverlays>,
    authored: Option<Res<AuthoredObjects>>,
    presentation: Res<PresentationMode>,
) {
    if presentation.active || !panel.open {
        panel.height = 0.0;
        return;
    }
    let Ok(ctx) = contexts.ctx_mut() else {
        return;
    };
    let (from, to) = view_window(&timeline, panel.whole_scene);
    let now = timeline.current_time;
    let width = (ctx.viewport_rect().width() - 24.0).clamp(320.0, 1400.0);
    let mut actions = Actions::default();
    let panel = &mut *panel;
    let query = panel.query.trim().to_lowercase();
    let rows: Vec<&Row> = panel
        .model
        .rows
        .iter()
        .filter(|row| {
            row.blocks
                .iter()
                .any(|block| block.end > from && block.start < to)
        })
        .filter(|row| {
            query.is_empty()
                || row.title.to_lowercase().contains(&query)
                || row
                    .blocks
                    .iter()
                    .any(|block| block.name.to_lowercase().contains(&query))
        })
        .collect();
    let in_view = rows.len();

    let response = egui::Window::new("Animaciones")
        .id(egui::Id::new("editor_animation_timeline"))
        .title_bar(false)
        .anchor(egui::Align2::CENTER_BOTTOM, egui::vec2(0.0, -96.0))
        .order(egui::Order::Foreground)
        .resizable(false)
        .collapsible(false)
        .frame(ui_kit::card_frame().inner_margin(egui::Margin::same(12)))
        .show(ctx, |ui| {
            ui.set_width(width - 24.0);
            ui.spacing_mut().item_spacing.y = 4.0;
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 6.0;
                ui.label(caption("ANIMACIONES"));
                ui.add_space(6.0);
                if ui_kit::chip(ui, "Segmento", !panel.whole_scene)
                    .on_hover_text("Solo el segmento bajo el cabezal")
                    .clicked()
                {
                    panel.whole_scene = false;
                }
                if ui_kit::chip(ui, "Escena completa", panel.whole_scene).clicked() {
                    panel.whole_scene = true;
                }
                ui.add(
                    egui::TextEdit::singleline(&mut panel.query)
                        .hint_text("Buscar objeto o animación")
                        .desired_width(200.0),
                );
                ui.label(
                    egui::RichText::new(format!(
                        "{in_view} {} · {} – {} s",
                        if in_view == 1 { "objeto" } else { "objetos" },
                        seconds(from),
                        seconds(to)
                    ))
                    .size(12.0)
                    .color(palette::TEXT_FAINT),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if icon_button_sized(ui, Icon::Close, ButtonTone::Ghost, true, 24.0)
                        .on_hover_text("Ocultar las animaciones · T")
                        .clicked()
                    {
                        actions.close = true;
                    }
                });
            });

            ruler(ui, from, to, now, &mut actions);

            ui_kit::field_frame()
                .fill(palette::INK)
                .inner_margin(egui::Margin::ZERO)
                .show(ui, |ui| {
                    if rows.is_empty() {
                        ui.set_min_height(ROW_HEIGHT * 2.0);
                        ui.add_space(6.0);
                        ui.label(
                            egui::RichText::new(if panel.model.rows.is_empty() {
                                "La escena no tiene animaciones con duración."
                            } else if query.is_empty() {
                                "Nada se anima en este tramo."
                            } else {
                                "Ninguna animación coincide con la búsqueda."
                            })
                            .size(12.0)
                            .color(palette::TEXT_FAINT),
                        );
                        return;
                    }
                    ui.spacing_mut().item_spacing.y = 0.0;
                    egui::ScrollArea::vertical()
                        .max_height(ROW_HEIGHT * VISIBLE_ROWS as f32)
                        .auto_shrink([false, true])
                        .show_rows(ui, ROW_HEIGHT, rows.len(), |ui, range| {
                            for (index, row) in rows[range.clone()].iter().enumerate() {
                                row_view(
                                    ui,
                                    row,
                                    range.start + index,
                                    (from, to),
                                    now,
                                    state.selected,
                                    authored.as_deref(),
                                    &mut actions,
                                );
                            }
                        });
                });
        });
    panel.height = response.map_or(0.0, |response| response.response.rect.height());

    if let Some(time) = actions.seek {
        timeline.seek_request = Some(time);
    }
    if actions.pause {
        timeline.is_playing = false;
    }
    if let Some(entity) = actions.select {
        state.selected = Some(entity);
        // The inspector shows the selection while the overlays are on.
        overlays.enabled = true;
    }
    if let Some(location) = actions.open
        && crate::source_link::AVAILABLE
    {
        crate::source_link::open_or_report(&location);
    }
    if actions.close {
        panel.open = false;
    }
}

/// The ruler with second marks and the playhead; a click or a drag on it
/// moves the playhead.
fn ruler(ui: &mut egui::Ui, from: f64, to: f64, now: f64, actions: &mut Actions) {
    let (rect, response) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), RULER_HEIGHT),
        egui::Sense::click_and_drag(),
    );
    let track =
        egui::Rect::from_min_max(egui::pos2(rect.min.x + LABEL_WIDTH, rect.min.y), rect.max);
    let painter = ui.painter_at(rect);
    let span = (to - from).max(1e-6);
    let step = crate::overlays::nice_step(span * 80.0 / f64::from(track.width().max(1.0)));
    let mut tick = (from / step).ceil() * step;
    while tick <= to + 1e-9 {
        let x = x_of(tick, from, to, track);
        painter.line_segment(
            [egui::pos2(x, track.max.y - 5.0), egui::pos2(x, track.max.y)],
            egui::Stroke::new(1.0, palette::TEXT_FAINT),
        );
        painter.text(
            egui::pos2(x + 3.0, track.min.y + 2.0),
            egui::Align2::LEFT_TOP,
            format!("{} s", crate::overlays::format_logical_value(tick, step)),
            egui::FontId::proportional(10.5),
            palette::TEXT_FAINT,
        );
        tick += step;
    }
    painter.text(
        egui::pos2(rect.min.x + 4.0, rect.center().y),
        egui::Align2::LEFT_CENTER,
        format!("{} s", seconds(now)),
        egui::FontId::monospace(11.0),
        palette::TEXT_MUTED,
    );
    if (from..=to).contains(&now) {
        let x = x_of(now, from, to, track);
        painter.line_segment(
            [egui::pos2(x, track.min.y), egui::pos2(x, track.max.y)],
            egui::Stroke::new(2.0, palette::TEXT),
        );
    }
    if (response.clicked() || response.dragged())
        && let Some(pointer) = response.interact_pointer_pos()
        && pointer.x >= track.min.x
    {
        actions.seek = Some(time_of(pointer.x, from, to, track));
    }
    response.on_hover_text("Clic o arrastre: mover el cabezal");
}

/// One row: the drawable's name, then its blocks in the window.
#[allow(clippy::too_many_arguments)]
fn row_view(
    ui: &mut egui::Ui,
    row: &Row,
    index: usize,
    (from, to): (f64, f64),
    now: f64,
    selected: Option<Entity>,
    authored: Option<&AuthoredObjects>,
    actions: &mut Actions,
) {
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), ROW_HEIGHT),
        egui::Sense::hover(),
    );
    let painter = ui.painter_at(rect);
    let is_selected = row.entity.is_some() && row.entity == selected;
    if is_selected {
        painter.rect_filled(rect, 0.0, palette::SELECTED);
    } else if index % 2 == 1 {
        painter.rect_filled(rect, 0.0, egui::Color32::from_white_alpha(4));
    }

    let label_rect = egui::Rect::from_min_max(
        rect.min,
        egui::pos2(rect.min.x + LABEL_WIDTH - 8.0, rect.max.y),
    );
    let label = ui
        .interact(
            label_rect,
            ui.id().with(("timeline_row", row.key)),
            egui::Sense::click(),
        )
        .on_hover_cursor(egui::CursorIcon::PointingHand);
    painter.with_clip_rect(label_rect).text(
        egui::pos2(label_rect.min.x + 8.0, rect.center().y),
        egui::Align2::LEFT_CENTER,
        &row.title,
        egui::FontId::proportional(12.5),
        if is_selected {
            palette::TEXT
        } else {
            palette::TEXT_MUTED
        },
    );
    let label = label.on_hover_text(match &row.location {
        Some(location) => format!(
            "{}\nCreado en {}\nClic: seleccionar · doble clic: abrir la línea",
            row.title,
            crate::feedback::location_label(location)
        ),
        None => format!("{}\nClic: seleccionar", row.title),
    });
    if label.clicked()
        && let Some(entity) = row.entity
    {
        actions.select = Some(entity);
    }
    if label.double_clicked() {
        actions.open = row.location.clone();
    }

    let track =
        egui::Rect::from_min_max(egui::pos2(rect.min.x + LABEL_WIDTH, rect.min.y), rect.max);
    for (block_index, block) in row.blocks.iter().enumerate() {
        if block.end <= from || block.start >= to {
            continue;
        }
        let x0 = x_of(block.start.max(from), from, to, track);
        let x1 = x_of(block.end.min(to), from, to, track).max(x0 + 3.0);
        let block_rect = egui::Rect::from_min_max(
            egui::pos2(x0, rect.min.y + 3.0),
            egui::pos2(x1, rect.max.y - 3.0),
        );
        let color = block.category.color();
        let response = ui
            .interact(
                block_rect,
                ui.id().with(("timeline_block", row.key, block_index)),
                egui::Sense::click(),
            )
            .on_hover_cursor(egui::CursorIcon::PointingHand);
        let active = (block.start..block.end).contains(&now);
        painter.rect_filled(
            block_rect,
            2.0,
            color.gamma_multiply(if response.hovered() || active {
                0.75
            } else {
                0.45
            }),
        );
        painter.rect_stroke(
            block_rect,
            2.0,
            egui::Stroke::new(1.0, color),
            egui::StrokeKind::Inside,
        );
        let name = block.name.replace('_', " ");
        let text = if block.clips > 1 {
            format!("{name} ×{}", block.clips)
        } else {
            name
        };
        if block_rect.width() > 28.0 {
            painter.with_clip_rect(block_rect.shrink(2.0)).text(
                egui::pos2(block_rect.min.x + 5.0, block_rect.center().y),
                egui::Align2::LEFT_CENTER,
                &text,
                egui::FontId::proportional(11.0),
                palette::INK,
            );
        }
        let play = block
            .origin
            .and_then(|origin| authored.and_then(|authored| authored.play(origin)));
        let mut hover = format!(
            "{text} · {}\n{} – {} s ({} s) · {}\ncurva: {}",
            row.title,
            seconds(block.start),
            seconds(block.end),
            seconds(block.end - block.start),
            block.category.label(),
            block.rate
        );
        if let Some(play) = play {
            hover.push_str(&format!(
                "\nplay en {}",
                crate::feedback::location_label(&play.location)
            ));
            for caller in &play.callers {
                hover.push_str(&format!(
                    "\n  llamado desde {}",
                    crate::feedback::location_label(caller)
                ));
            }
            hover.push_str("\nClic: ir al inicio · doble clic: abrir la línea");
        } else {
            hover.push_str("\nClic: ir al inicio");
        }
        let response = response.on_hover_text(hover);
        if response.clicked() {
            actions.seek = Some(block.start);
            actions.pause = true;
            actions.select = row.entity;
        }
        if response.double_clicked() {
            actions.open = play.map(|play| play.location.clone());
        }
    }
    if (from..=to).contains(&now) {
        let x = x_of(now, from, to, track);
        painter.line_segment(
            [egui::pos2(x, rect.min.y), egui::pos2(x, rect.max.y)],
            egui::Stroke::new(1.0, palette::TEXT.gamma_multiply(0.6)),
        );
    }
}

fn x_of(time: f64, from: f64, to: f64, track: egui::Rect) -> f32 {
    let fraction = ((time - from) / (to - from).max(1e-9)).clamp(0.0, 1.0);
    track.min.x + track.width() * fraction as f32
}

fn time_of(x: f32, from: f64, to: f64, track: egui::Rect) -> f64 {
    let fraction = f64::from((x - track.min.x) / track.width().max(1.0)).clamp(0.0, 1.0);
    from + (to - from) * fraction
}

/// Seconds with up to two decimals.
fn seconds(value: f64) -> String {
    let text = format!("{value:.2}");
    let text = text.trim_end_matches('0').trim_end_matches('.');
    if text.is_empty() || text == "-0" {
        "0".to_owned()
    } else {
        text.to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn clip(start: f64, end: f64, target: u64, name: &str, origin: Option<u64>) -> ClipView {
        ClipView {
            start,
            end,
            target: ObjectId::from_raw(target),
            name: name.to_owned(),
            rate: "smooth".to_owned(),
            origin,
            camera: false,
        }
    }

    #[test]
    fn a_play_is_one_block_per_row_however_many_clips_it_made() {
        // A text written glyph by glyph, then moved, and a circle faded in.
        let clips = vec![
            clip(0.0, 0.4, 10, "Write", Some(1)),
            clip(0.1, 0.5, 11, "Write", Some(1)),
            clip(0.2, 0.6, 12, "Write", Some(1)),
            clip(1.0, 2.0, 10, "translation", Some(2)),
            clip(0.5, 1.5, 20, "opacity", Some(3)),
        ];
        // Glyphs 10-12 belong to the text, drawable 1.
        let rows = build_rows(&clips, |clip| {
            RowKey::Object(if clip.target.as_raw() < 20 {
                ObjectId::from_raw(1)
            } else {
                clip.target
            })
        });
        assert_eq!(rows.len(), 2);
        let (key, text) = &rows[0];
        assert_eq!(*key, RowKey::Object(ObjectId::from_raw(1)));
        assert_eq!(text.len(), 2);
        assert_eq!((text[0].start, text[0].end, text[0].clips), (0.0, 0.6, 3));
        assert_eq!(text[0].category, Category::Stroke);
        assert_eq!(text[1].category, Category::Motion);
        assert_eq!(rows[1].1[0].category, Category::Appearance);
    }

    #[test]
    fn clips_without_a_play_stay_apart() {
        let clips = vec![
            clip(0.0, 1.0, 5, "opacity", None),
            clip(2.0, 3.0, 5, "opacity", None),
        ];
        let rows = build_rows(&clips, |clip| RowKey::Object(clip.target));
        assert_eq!(rows[0].1.len(), 2);
    }

    #[test]
    fn categories_follow_what_the_animation_changes() {
        assert_eq!(Category::of("camera_path_follow"), Category::Camera);
        assert_eq!(Category::of("path_follow"), Category::Motion);
        assert_eq!(Category::of("rotation"), Category::Transform);
        assert_eq!(Category::of("Rotate"), Category::Transform);
        assert_eq!(Category::of("FadeIn"), Category::Appearance);
        assert_eq!(Category::of("fill_color"), Category::Style);
        assert_eq!(Category::of("Write"), Category::Stroke);
        assert_eq!(Category::of("signal_float"), Category::Other);
    }

    #[test]
    fn the_view_is_the_segment_under_the_playhead() {
        let mut timeline = Timeline::default();
        timeline.cached_duration = 10.0;
        timeline.segments = vec![
            gaanim_timeline::timeline::SegmentMetadata {
                id: 1,
                name: "intro".to_owned(),
                notes: None,
                start_time: 0.0,
                end_time: 4.0,
                stops: Vec::new(),
            },
            gaanim_timeline::timeline::SegmentMetadata {
                id: 2,
                name: "resto".to_owned(),
                notes: None,
                start_time: 4.0,
                end_time: 10.0,
                stops: Vec::new(),
            },
        ];
        timeline.current_time = 5.0;
        assert_eq!(view_window(&timeline, false), (4.0, 10.0));
        assert_eq!(view_window(&timeline, true), (0.0, 10.0));
        timeline.segments.clear();
        assert_eq!(view_window(&timeline, false), (0.0, 10.0));
    }

    #[test]
    fn times_and_positions_map_both_ways() {
        let track = egui::Rect::from_min_max(egui::pos2(100.0, 0.0), egui::pos2(300.0, 10.0));
        assert_eq!(x_of(2.0, 0.0, 4.0, track), 200.0);
        assert_eq!(x_of(9.0, 0.0, 4.0, track), 300.0);
        assert!((time_of(150.0, 0.0, 4.0, track) - 1.0).abs() < 1e-9);
        assert_eq!(seconds(1.5), "1.5");
        assert_eq!(seconds(2.0), "2");
    }
}
