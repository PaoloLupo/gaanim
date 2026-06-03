use bevy::prelude::*;
use bevy_egui::{EguiContexts, EguiPlugin, EguiPrimaryContextPass, egui, input::EguiWantsInput};
use gaanim_core::id::ObjectId;
use gaanim_core::peniko;
use gaanim_math::Camera;
use gaanim_scene::{FillBrush, MobjectId, ObjectTag, Opacity, RenderOrder, StrokeBrush, Visible, WorldBounds};
use gaanim_timeline::timeline::Timeline;
use std::collections::HashMap;

mod fps_overlay;
mod timeline_widget;

pub struct GaanimEditorPlugin;

impl Plugin for GaanimEditorPlugin {
    fn build(&self, app: &mut App) {
        // Multipass is enabled by default in bevy_egui 0.39, but it causes
        // the play button's `.clicked()` to fire multiple times per frame
        // (once per pass), toggling `is_playing` true→false→true within a
        // single frame. The deprecation says "use EguiPlugin::default()",
        // but that still defaults to multipass=true; we have to keep this
        // explicit override until bevy_egui provides a non-deprecated
        // way to opt out (tracked upstream).
        #[allow(deprecated)]
        app.add_plugins(EguiPlugin {
            enable_multipass_for_primary_context: false,
            ..default()
        })
        .init_resource::<EditorState>()
        .init_resource::<fps_overlay::FpsOverlay>()
        .add_systems(
            Update,
            (editor_picking_system, fps_overlay::fps_overlay_system),
        )
        .add_systems(EguiPrimaryContextPass, editor_ui_system);
    }
}

#[derive(Resource)]
pub struct EditorState {
    pub selected: Option<Entity>,
    pub show_hierarchy: bool,
    pub show_inspector: bool,
    pub timeline_widget: timeline_widget::TimelineWidget,
}

impl Default for EditorState {
    fn default() -> Self {
        Self {
            selected: None,
            show_hierarchy: false,
            show_inspector: false,
            timeline_widget: timeline_widget::TimelineWidget::new(),
        }
    }
}

fn editor_ui_system(
    mut contexts: EguiContexts,
    mut state: ResMut<EditorState>,
    mut timeline: ResMut<Timeline>,
    camera: Res<Camera>,
    fps_overlay: Res<fps_overlay::FpsOverlay>,
    entity_query: Query<(Entity, Option<&MobjectId>, Option<&ObjectTag>)>,
    transform_query: Query<&gaanim_math::SpatialTransform>,
    fill_query: Query<&FillBrush>,
    stroke_query: Query<&StrokeBrush>,
    opacity_query: Query<&Opacity>,
    render_query: Query<&RenderOrder>,
    visible_query: Query<&Visible>,
    bounds_query: Query<&WorldBounds>,
) {
    let Ok(ctx) = contexts.ctx_mut() else {
        return;
    };

    let mut property_values: HashMap<ObjectId, timeline_widget::PropertyValues> = HashMap::new();
    for (entity, mobj_id, _) in &entity_query {
        let Some(oid) = mobj_id else { continue; };

        let pos = if let Ok(t) = transform_query.get(entity) { t.translation } else { glam::DVec3::ZERO };
        let scale = if let Ok(t) = transform_query.get(entity) { t.scale } else { glam::DVec3::ONE };
        let rotation_deg = if let Ok(t) = transform_query.get(entity) {
            2.0 * f64::atan2(t.rotation.z, t.rotation.w).to_degrees()
        } else {
            0.0
        };

        let fill_label = if let Ok(fb) = fill_query.get(entity) {
            brush_string(&fb.0)
        } else {
            "none".into()
        };

        let stroke_label = if let Ok(sb) = stroke_query.get(entity) {
            brush_string(&sb.brush)
        } else {
            "none".into()
        };

        let stroke_width = if let Ok(sb) = stroke_query.get(entity) {
            sb.style.width
        } else {
            0.0
        };

        let opacity = if let Ok(o) = opacity_query.get(entity) { o.0 } else { 1.0 };

        property_values.insert(
            oid.0,
            timeline_widget::PropertyValues {
                pos_x: pos.x,
                pos_y: pos.y,
                pos_z: pos.z,
                scale_x: scale.x,
                scale_y: scale.y,
                scale_z: scale.z,
                rotation_deg,
                fill_label,
                stroke_label,
                stroke_width,
                opacity,
            },
        );
    }

    egui::TopBottomPanel::top("menu_bar").show(ctx, |ui| {
        ui.horizontal(|ui| {
            ui.menu_button("View", |ui| {
                ui.checkbox(&mut state.show_hierarchy, "Hierarchy");
                ui.checkbox(&mut state.show_inspector, "Inspector");
            });
            ui.separator();
            if let Some(selected) = state.selected {
                let name = entity_query
                    .get(selected)
                    .ok()
                    .and_then(|(_, _, tag)| tag.map(|t| t.0.as_str()))
                    .unwrap_or("???");
                ui.label(format!("Selected: {name}"));
            } else {
                ui.label("Gaanim Editor");
            }
        });
    });

    if state.show_hierarchy {
        egui::SidePanel::left("hierarchy")
            .resizable(true)
            .default_width(220.0)
            .show(ctx, |ui| {
                ui.heading("Scene");
                ui.separator();

                let mut items: Vec<(Entity, String)> = entity_query
                    .iter()
                    .map(|(entity, _, tag)| {
                        let name = tag
                            .map(|t| t.0.clone())
                            .unwrap_or_else(|| format!("Entity({entity:?})"));
                        (entity, name)
                    })
                    .collect();

                items.sort_by(|(_, a), (_, b)| a.cmp(b));

                for (entity, name) in &items {
                    let selected = state.selected == Some(*entity);
                    if ui.selectable_label(selected, name).clicked() {
                        state.selected = Some(*entity);
                    }
                }

                if items.is_empty() {
                    ui.label("No objects in scene.");
                }
            });
    }

    if state.show_inspector {
        egui::SidePanel::right("inspector")
            .resizable(true)
            .default_width(280.0)
            .show(ctx, |ui| {
                ui.heading("Inspector");
                ui.separator();

                if let Some(selected) = state.selected {
                    if let Ok((_, mobject_id, tag)) = entity_query.get(selected) {
                        ui.collapsing("Entity", |ui| {
                            ui.label(format!("Entity: {selected:?}"));
                            if let Some(id) = mobject_id {
                                ui.label(format!("MobjectId: {:?}", id.0));
                            }
                            if let Some(t) = tag {
                                ui.label(format!("Tag: {}", t.0));
                            }
                            ui.label(format!("Visible: {}", visible_query.contains(selected)));
                        });
                    }

                    if let Ok(t) = transform_query.get(selected) {
                        ui.collapsing("Transform", |ui| {
                            ui.label(format!(
                                "Pos: ({:.1}, {:.1}, {:.1})",
                                t.translation.x, t.translation.y, t.translation.z
                            ));
                            ui.label(format!(
                                "Scale: ({:.2}, {:.2}, {:.2})",
                                t.scale.x, t.scale.y, t.scale.z
                            ));
                            let angle = 2.0 * f64::atan2(t.rotation.z, t.rotation.w);
                            ui.label(format!("Rotation: {:.2}°", angle.to_degrees()));
                            ui.label(format!(
                                "Anchor: ({:.1}, {:.1}, {:.1})",
                                t.anchor.x, t.anchor.y, t.anchor.z
                            ));
                        });
                    }

                    ui.collapsing("Appearance", |ui| {
                        if let Ok(fb) = fill_query.get(selected) {
                            match &fb.0 {
                                Some(brush) => {
                                    ui.label(format!("Fill: {}", brush_label(brush)));
                                }
                                None => {
                                    ui.label("Fill: none");
                                }
                            }
                        }
                        if let Ok(sb) = stroke_query.get(selected) {
                            match &sb.brush {
                                Some(brush) => {
                                    ui.label(format!("Stroke: {}", brush_label(brush)));
                                }
                                None => {
                                    ui.label("Stroke: none");
                                }
                            }
                            ui.label(format!("Stroke Width: {:.2}", sb.style.width));
                        }
                        if let Ok(o) = opacity_query.get(selected) {
                            ui.label(format!("Opacity: {:.2}", o.0));
                        }
                    });

                    if let Ok(ro) = render_query.get(selected) {
                        ui.collapsing("Render", |ui| {
                            ui.label(format!("Z-Index: {}", ro.z_index));
                            ui.label(format!("Creation Order: {}", ro.creation_order));
                        });
                    }

                    if let Ok(b) = bounds_query.get(selected) {
                        ui.collapsing("Bounds", |ui| {
                            ui.label(format!(
                                "Min: ({:.1}, {:.1}, {:.1})",
                                b.0.min.x, b.0.min.y, b.0.min.z
                            ));
                            ui.label(format!(
                                "Max: ({:.1}, {:.1}, {:.1})",
                                b.0.max.x, b.0.max.y, b.0.max.z
                            ));
                        });
                    }
                } else {
                    ui.label("No entity selected.");
                    ui.separator();
                    ui.label("Click an object to inspect it.");
                }
            });
    }

    egui::TopBottomPanel::bottom("timeline")
        .resizable(true)
        .default_height(200.0)
        .min_height(100.0)
        .show(ctx, |ui| {
            state.timeline_widget.show(ui, &mut timeline, &property_values);
        });

    // Track sidebar click → select the corresponding entity
    if let Some(track_id) = state.timeline_widget.selected_track {
        if let Some(track) = timeline.tracks.get(track_id)
            && let Some(obj_id) = track.object_id {
                for (entity, mobj_id, _) in &entity_query {
                    if let Some(mid) = mobj_id
                        && mid.0 == obj_id {
                            state.selected = Some(entity);
                            break;
                        }
                }
            }
        state.timeline_widget.selected_track = None;
    }

    // ── Viewport selection indicator ──────────────────────────────────
    if let Some(selected) = state.selected
        && let Ok(bounds) = bounds_query.get(selected) {
            let corners = [
                glam::DVec3::new(bounds.0.min.x, bounds.0.min.y, 0.0),
                glam::DVec3::new(bounds.0.max.x, bounds.0.min.y, 0.0),
                glam::DVec3::new(bounds.0.max.x, bounds.0.max.y, 0.0),
                glam::DVec3::new(bounds.0.min.x, bounds.0.max.y, 0.0),
            ];

            let screen: Vec<egui::Pos2> = corners
                .iter()
                .map(|c| {
                    let s = camera.world_to_screen(*c);
                    egui::Pos2::new(s.x as f32, s.y as f32)
                })
                .collect();

            let vp = ctx.viewport_rect();
            let color = egui::Color32::from_rgba_premultiplied(68, 160, 255, 180);
            let stroke = egui::Stroke::new(2.0, color);
            egui::Area::new("viewport_selection".into())
                .fixed_pos(egui::pos2(0.0, 0.0))
                .interactable(false)
                .show(ctx, |ui| {
                    let _ = ui.allocate_space(vp.size());
                    let p = ui.painter();
                    for i in 0..4 {
                        p.line_segment([screen[i], screen[(i + 1) % 4]], stroke);
                    }
                    let cs = 6.0;
                    for &c in &screen {
                        p.line_segment(
                            [egui::Pos2::new(c.x - cs, c.y), egui::Pos2::new(c.x + cs, c.y)],
                            stroke,
                        );
                        p.line_segment(
                            [egui::Pos2::new(c.x, c.y - cs), egui::Pos2::new(c.x, c.y + cs)],
                            stroke,
                        );
                    }
                });
        }

    fps_overlay.render(ctx);
}

fn brush_string(brush: &Option<peniko::Brush>) -> String {
    match brush {
        Some(peniko::Brush::Solid(color)) => {
            let rgba = color.to_rgba8();
            format!("#{:02X}{:02X}{:02X}{:02X}", rgba.r, rgba.g, rgba.b, rgba.a)
        }
        Some(peniko::Brush::Gradient(_)) => "<gradient>".into(),
        Some(peniko::Brush::Image(_)) => "<image>".into(),
        None => "none".into(),
    }
}

fn brush_label(brush: &gaanim_core::peniko::Brush) -> String {
    match brush {
        gaanim_core::peniko::Brush::Solid(color) => {
            let rgba = color.to_rgba8();
            format!("#{:02X}{:02X}{:02X}{:02X}", rgba.r, rgba.g, rgba.b, rgba.a)
        }
        gaanim_core::peniko::Brush::Gradient(_) => "<gradient>".into(),
        gaanim_core::peniko::Brush::Image(_) => "<image>".into(),
    }
}

/// Picking system that runs in `Update` and uses `EguiWantsInput` (which
/// includes `is_popup_open`) to skip clicks consumed by egui.
///
/// We use Bevy's mouse input directly (not egui) so we have a one-frame
/// safety margin: a click that closed a popup in the previous frame is
/// still flagged by `EguiWantsInput::wants_any_pointer_input()` because
/// egui's resource is updated in PostUpdate, after the popup close.
fn editor_picking_system(
    egui_wants: Res<EguiWantsInput>,
    camera: Res<Camera>,
    mouse: Res<ButtonInput<MouseButton>>,
    windows: Query<&Window, With<bevy::window::PrimaryWindow>>,
    entities: Query<(Entity, &WorldBounds, Option<&RenderOrder>)>,
    mut state: ResMut<EditorState>,
) {
    // If egui wants any pointer input (pointer over area, popup open,
    // dragging, etc.) from the previous frame, skip picking. This
    // handles dropdown/popup clicks without the race where the popup
    // closes before the picking system runs.
    if egui_wants.wants_any_pointer_input() {
        return;
    }

    if !mouse.just_pressed(MouseButton::Left) {
        return;
    }

    let Ok(window) = windows.single() else {
        return;
    };
    let Some(cursor_pos) = window.cursor_position() else {
        return;
    };

    let world_pos =
        camera.screen_to_world(glam::DVec2::new(cursor_pos.x as f64, cursor_pos.y as f64));

    let mut best_z = i32::MIN;
    let mut best_entity: Option<Entity> = None;

    for (entity, bounds, render_order) in &entities {
        if bounds
            .0
            .contains(glam::DVec3::new(world_pos.x, world_pos.y, 0.0))
        {
            let z = render_order.map(|ro| ro.z_index).unwrap_or(0);
            if z >= best_z {
                best_z = z;
                best_entity = Some(entity);
            }
        }
    }

    state.selected = best_entity;
}
