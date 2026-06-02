use bevy::prelude::*;
use bevy_egui::{egui, EguiContexts, EguiPlugin, EguiPrimaryContextPass};
use gaanim_math::Camera;
use gaanim_scene::{
    MobjectId, ObjectTag, Opacity, RenderOrder, Visible, WorldBounds,
};
use gaanim_timeline::timeline::Timeline;

pub struct GaanimEditorPlugin;

impl Plugin for GaanimEditorPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(EguiPlugin::default())
            .init_resource::<EditorState>()
            .add_systems(EguiPrimaryContextPass, editor_ui_system)
            .add_systems(Update, picking_system);
    }
}

#[derive(Resource)]
pub struct EditorState {
    pub selected: Option<Entity>,
    pub playing: bool,
    pub show_hierarchy: bool,
    pub show_inspector: bool,
}

impl Default for EditorState {
    fn default() -> Self {
        Self {
            selected: None,
            playing: false,
            show_hierarchy: true,
            show_inspector: true,
        }
    }
}

fn editor_ui_system(
    mut contexts: EguiContexts,
    mut state: ResMut<EditorState>,
    mut timeline: ResMut<Timeline>,
    entity_query: Query<(Entity, Option<&MobjectId>, Option<&ObjectTag>)>,
    transform_query: Query<&gaanim_math::SpatialTransform>,
    fill_query: Query<&gaanim_scene::FillBrush>,
    stroke_query: Query<&gaanim_scene::StrokeBrush>,
    opacity_query: Query<&Opacity>,
    render_query: Query<&RenderOrder>,
    visible_query: Query<&Visible>,
    bounds_query: Query<&WorldBounds>,
) {
    let Ok(ctx) = contexts.ctx_mut() else {
        return;
    };

    // --- Top menu bar ---
    egui::TopBottomPanel::top("menu_bar").show(ctx, |ui| {
        ui.horizontal(|ui| {
            ui.menu_button("View", |ui| {
                ui.checkbox(&mut state.show_hierarchy, "Hierarchy");
                ui.checkbox(&mut state.show_inspector, "Inspector");
            });
            ui.separator();
            if let Some(selected) = state.selected {
                let name = if let Ok((_, _, tag)) = entity_query.get(selected) {
                    tag.map(|t| t.0.as_str()).unwrap_or("???")
                } else {
                    "???"
                };
                ui.label(format!("Selected: {name}"));
            } else {
                ui.label("Gaanim Editor");
            }
        });
    });

    // --- Hierarchy panel (left) ---
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

    // --- Inspector panel (right) ---
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

    // --- Bottom bar: playback controls ---
    egui::TopBottomPanel::bottom("playback").show(ctx, |ui| {
        ui.horizontal(|ui| {
            if ui
                .add(egui::Button::new(if timeline.is_playing { "⏸" } else { "▶" }))
                .clicked()
            {
                timeline.is_playing = !timeline.is_playing;
                state.playing = timeline.is_playing;
            }

            if ui.button("⏮").clicked() {
                timeline.is_playing = false;
                state.playing = false;
                timeline.seek_request = Some(0.0);
            }

            ui.separator();

            let cached = timeline.cached_duration;
            let mut current = timeline.current_time;
            ui.label(format!("t: {current:.2}s / {cached:.2}s"));

            if cached > 0.0 {
                let slider = egui::Slider::new(&mut current, 0.0..=cached)
                    .text("")
                    .step_by(0.05);
                if ui.add(slider).changed() {
                    timeline.is_playing = false;
                    state.playing = false;
                    timeline.seek_request = Some(current);
                }
            }

            ui.separator();

            if ui.button("⏭").clicked() {
                timeline.is_playing = false;
                state.playing = false;
                timeline.seek_request = Some(cached);
            }
        });
    });
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

fn picking_system(
    windows: Query<&Window, With<bevy::window::PrimaryWindow>>,
    camera: Res<Camera>,
    mouse_button_input: Res<ButtonInput<MouseButton>>,
    entities: Query<(Entity, &WorldBounds, Option<&RenderOrder>)>,
    mut state: ResMut<EditorState>,
) {
    let Ok(window) = windows.single() else {
        return;
    };

    if mouse_button_input.just_pressed(MouseButton::Left) {
        let Some(cursor) = window.cursor_position() else {
            return;
        };

        let world_pos =
            camera.screen_to_world(glam::DVec2::new(cursor.x as f64, cursor.y as f64));

        let mut best_z = i32::MIN;
        let mut best_entity: Option<Entity> = None;

        for (entity, bounds, render_order) in &entities {
            if bounds.0.contains(world_pos) {
                let z = render_order.map(|ro| ro.z_index).unwrap_or(0);
                if z >= best_z {
                    best_z = z;
                    best_entity = Some(entity);
                }
            }
        }

        state.selected = best_entity;
    }
}
