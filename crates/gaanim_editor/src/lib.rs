use bevy::prelude::*;
use bevy_egui::{EguiPlugin, EguiPrimaryContextPass, egui, input::EguiWantsInput};
use gaanim_core::id::ObjectId;
use gaanim_core::peniko;
use gaanim_math::Camera;
use gaanim_scene::{
    FillBrush, GroupMarker, MobjectId, ObjectTag, Opacity, RenderOrder, StrokeBrush, WorldBounds,
};
use gaanim_timeline::timeline::Timeline;
use std::collections::HashMap;

mod export;
mod fps_overlay;
mod timeline_widget;
mod vsync;

pub struct GaanimEditorPlugin;

impl Plugin for GaanimEditorPlugin {
    fn build(&self, app: &mut App) {
        #[allow(deprecated)]
        app.add_plugins(EguiPlugin {
            enable_multipass_for_primary_context: false,
            ..default()
        })
        .init_resource::<EditorState>()
        .init_resource::<export::ExportState>()
        .init_non_send_resource::<export::ExportRuntime>()
        .init_resource::<fps_overlay::FpsOverlay>()
        .init_resource::<vsync::VsyncState>()
        .add_systems(
            Update,
            (
                editor_picking_system,
                fps_overlay::fps_overlay_system,
                vsync::vsync_toggle_system,
            ),
        )
        .add_systems(Update, export::export_per_frame_system)
        .add_systems(
            EguiPrimaryContextPass,
            (editor_ui_system, export::export_dialog_system),
        );
    }
}

#[derive(Resource)]
pub struct EditorState {
    pub selected: Option<Entity>,
    pub timeline_widget: timeline_widget::TimelineWidget,
}

impl Default for EditorState {
    fn default() -> Self {
        Self {
            selected: None,
            timeline_widget: timeline_widget::TimelineWidget::new(),
        }
    }
}

fn editor_ui_system(
    mut ctx: bevy_egui::EguiContexts,
    mut state: ResMut<EditorState>,
    mut export_state: ResMut<export::ExportState>,
    mut timeline: ResMut<Timeline>,
    camera: Res<Camera>,
    fps_overlay: Res<fps_overlay::FpsOverlay>,
    entity_query: Query<(Entity, Option<&MobjectId>, Option<&ObjectTag>)>,
    children_query: Query<&Children>,
    group_query: Query<&GroupMarker>,
    transform_query: Query<&gaanim_math::SpatialTransform>,
    fill_query: Query<&FillBrush>,
    stroke_query: Query<&StrokeBrush>,
    opacity_query: Query<&Opacity>,
    bounds_query: Query<&WorldBounds>,
) {
    let Ok(ctx) = ctx.ctx_mut() else {
        return;
    };

    let is_exporting = export_state.active;

    let mut property_values: HashMap<ObjectId, timeline_widget::PropertyValues> = HashMap::new();
    for (entity, mobj_id, _) in &entity_query {
        let Some(oid) = mobj_id else {
            continue;
        };

        let pos = if let Ok(t) = transform_query.get(entity) {
            t.translation
        } else {
            glam::DVec3::ZERO
        };
        let scale = if let Ok(t) = transform_query.get(entity) {
            t.scale
        } else {
            glam::DVec3::ONE
        };
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

        let opacity = if let Ok(o) = opacity_query.get(entity) {
            o.0
        } else {
            1.0
        };

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

    let mobject_to_track: HashMap<gaanim_core::id::ObjectId, gaanim_timeline::clip::TrackId> =
        timeline
            .tracks
            .iter()
            .filter_map(|(tid, t)| t.object_id.map(|oid| (oid, tid)))
            .collect();
    let mut group_children: HashMap<
        gaanim_timeline::clip::TrackId,
        Vec<gaanim_timeline::clip::TrackId>,
    > = HashMap::new();
    for (entity, mobj_id, _) in &entity_query {
        if !group_query.contains(entity) {
            continue;
        }
        let Some(group_oid) = mobj_id else { continue };
        let Some(&group_tid) = mobject_to_track.get(&group_oid.0) else {
            continue;
        };
        if let Ok(children) = children_query.get(entity) {
            let child_tids: Vec<gaanim_timeline::clip::TrackId> = children
                .iter()
                .filter_map(|child| {
                    entity_query
                        .get(child)
                        .ok()
                        .and_then(|(_, mid, _)| mid)
                        .and_then(|oid| mobject_to_track.get(&oid.0))
                        .copied()
                })
                .collect();
            if !child_tids.is_empty() {
                group_children.insert(group_tid, child_tids);
            }
        }
    }

    egui::TopBottomPanel::top("menu_bar").show(ctx, |ui| {
        egui::MenuBar::new().ui(ui, |ui| {
            ui.menu_button("File", |ui| {
                if ui.button("Export...").clicked() {
                    export_state.dialog_open = true;
                    ui.close();
                }
            });

            if is_exporting {
                ui.add(
                    egui::ProgressBar::new(export_state.progress)
                        .desired_width(140.0)
                        .text(format!("{:.0}%", export_state.progress * 100.0)),
                );
                ui.label(format!(
                    "Frame {}/{}",
                    export_state.current_frame, export_state.total_frames
                ));
            } else if let Some(selected) = state.selected {
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

    egui::TopBottomPanel::bottom("timeline")
        .resizable(true)
        .default_height(200.0)
        .min_height(100.0)
        .show(ctx, |ui| {
            state
                .timeline_widget
                .show(ui, &mut timeline, &property_values, &group_children);
        });

    if let Some(track_id) = state.timeline_widget.selected_track {
        if let Some(track) = timeline.tracks.get(track_id)
            && let Some(obj_id) = track.object_id
        {
            for (entity, mobj_id, _) in &entity_query {
                if let Some(mid) = mobj_id
                    && mid.0 == obj_id
                {
                    state.selected = Some(entity);
                    break;
                }
            }
        }
        state.timeline_widget.selected_track = None;
    }

    if let Some(selected) = state.selected
        && let Ok(bounds) = bounds_query.get(selected)
    {
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

        let color = egui::Color32::from_rgba_premultiplied(68, 160, 255, 180);
        let stroke = egui::Stroke::new(2.0, color);
        egui::Area::new("viewport_selection".into())
            .fixed_pos(egui::pos2(0.0, 0.0))
            .interactable(false)
            .show(ctx, |ui| {
                let vp = ctx.viewport_rect();
                let _ = ui.allocate_space(vp.size());
                let p = ui.painter();
                for i in 0..4 {
                    p.line_segment([screen[i], screen[(i + 1) % 4]], stroke);
                }
                let cs = 6.0;
                for &c in &screen {
                    p.line_segment(
                        [
                            egui::Pos2::new(c.x - cs, c.y),
                            egui::Pos2::new(c.x + cs, c.y),
                        ],
                        stroke,
                    );
                    p.line_segment(
                        [
                            egui::Pos2::new(c.x, c.y - cs),
                            egui::Pos2::new(c.x, c.y + cs),
                        ],
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

fn editor_picking_system(
    egui_wants: Res<EguiWantsInput>,
    camera: Res<Camera>,
    mouse: Res<ButtonInput<MouseButton>>,
    windows: Query<&Window, With<bevy::window::PrimaryWindow>>,
    entities: Query<(Entity, &WorldBounds, Option<&RenderOrder>)>,
    mut state: ResMut<EditorState>,
) {
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
