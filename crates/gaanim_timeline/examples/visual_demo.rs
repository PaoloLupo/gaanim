use std::f64::consts::PI;
use bevy::prelude::*;
use gaanim_core::ObjectId;
use gaanim_math::{Camera, GlobalSpatialTransform, RateFunc, SpatialTransform};
use gaanim_scene::{
    FillBrush, GlobalOpacity, GaanimScenePlugin, MobjectId, Opacity,
    RenderLayer, RenderOrder, StrokeBrush, Visible, SceneSet,
};
use gaanim_animation::{GaanimAnimationPlugin, DeltaTime};
use gaanim_timeline::{
    GaanimTimelinePlugin, timeline::Timeline, clip::ClipPayload, clip::AnimationSpec, clip::PropertyLensSpec,
};

fn main() {
    App::new()
        // Add Bevy's rendering and windowing defaults
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "Gaanim v2 — Real-time Visual ECS Demo".into(),
                resolution: (1280, 720).into(),
                ..default()
            }),
            ..default()
        }))
        // Add our programmatic animation engine plugins
        .add_plugins(GaanimScenePlugin)
        .add_plugins(GaanimAnimationPlugin)
        .add_plugins(GaanimTimelinePlugin)
        // Setup initial camera and scene graph
        .add_systems(Startup, setup_scene)
        // Draw Mobjects to Bevy screen using high-performance 2D Gizmos
        .add_systems(Update, draw_mobjects_gizmos.after(SceneSet::Propagation))
        // Simple timeline clock driver
        .add_systems(Update, drive_timeline_clock)
        .run();
}

/// Startup system: Spawns the camera, parent/child Mobjects, and schedules timeline clips
fn setup_scene(mut commands: Commands, mut timeline: ResMut<Timeline>) {
    // 1. Spawn a default Orthographic camera resource and entity
    commands.insert_resource(Camera::ortho_2d(1280, 720));
    commands.spawn(Camera2d);

    // Create unique IDs for our Mobjects (matching Bevy bitwise Entities)
    let parent_id = ObjectId::from_parts(1, 1);
    let child_id = ObjectId::from_parts(2, 1);

    // 2. Spawn Parent Mobject (represented as a dynamic rotating block)
    let parent_entity = commands
        .spawn((
            MobjectId(parent_id),
            SpatialTransform::new_2d(-200.0, 0.0), // Starts on the left
            GlobalSpatialTransform::default(),
            Opacity(1.0),
            GlobalOpacity(1.0),
            FillBrush(Some(gaanim_core::peniko::Brush::Solid(
                gaanim_core::peniko::Color::from_rgba8(0, 102, 204, 255), // Slick blue
            ))),
            StrokeBrush {
                brush: Some(gaanim_core::peniko::Brush::Solid(gaanim_core::peniko::Color::WHITE)),
                style: gaanim_core::kurbo::Stroke::new(3.0),
            },
            RenderOrder::default(),
            RenderLayer::Vello2D,
            Visible,
        ))
        .id();

    // 3. Spawn Child Mobject (orbiting circle) parented to the rotating block
    let child_entity = commands
        .spawn((
            MobjectId(child_id),
            SpatialTransform::new_2d(150.0, 0.0), // Local offset to the right
            GlobalSpatialTransform::default(),
            Opacity(1.0),
            GlobalOpacity(1.0),
            FillBrush(Some(gaanim_core::peniko::Brush::Solid(
                gaanim_core::peniko::Color::from_rgba8(235, 64, 120, 255), // Modern pink
            ))),
            StrokeBrush {
                brush: Some(gaanim_core::peniko::Brush::Solid(gaanim_core::peniko::Color::WHITE)),
                style: gaanim_core::kurbo::Stroke::new(2.0),
            },
            RenderOrder::default(),
            RenderLayer::Vello2D,
            Visible,
        ))
        .id();

    // Set parent-child relationship via Bevy's built-in command hierarchy
    commands.entity(child_entity).set_parent_in_place(parent_entity);

    // 4. Setup tracks in the Timeline
    let track_id = timeline.add_track("Main Graphics", 0);

    // Clip 1: Animate Parent translating left-to-right using a physical Spring solver!
    // Moves from x = -200.0 to x = 200.0
    timeline.add_clip(
        track_id,
        0.5, // starts at 0.5s
        2.5, // duration 2.5s
        ClipPayload::Animation(AnimationSpec {
            target: parent_id,
            lens: PropertyLensSpec::Translation {
                from: gaanim_core::glam::DVec3::new(-200.0, 0.0, 0.0),
                to: gaanim_core::glam::DVec3::new(200.0, 0.0, 0.0),
            },
            rate_func: RateFunc::Spring {
                stiffness: 90.0,
                damping: 12.0,
            },
        }),
    );

    // Clip 2: Animate Parent rotating 360 degrees concurrently!
    timeline.add_clip(
        track_id,
        1.0, // starts at 1.0s
        2.0, // duration 2.0s
        ClipPayload::Animation(AnimationSpec {
            target: parent_id,
            lens: PropertyLensSpec::Rotation {
                from: gaanim_core::glam::DQuat::IDENTITY,
                to: gaanim_core::glam::DQuat::from_rotation_z(2.0 * PI),
            },
            rate_func: RateFunc::DoubleSmooth,
        }),
    );

    // Clip 3: Animate Child Orbit local scaling up and down!
    timeline.add_clip(
        track_id,
        1.5,
        1.5,
        ClipPayload::Animation(AnimationSpec {
            target: child_id,
            lens: PropertyLensSpec::Scale {
                from: gaanim_core::glam::DVec3::ONE,
                to: gaanim_core::glam::DVec3::new(1.8, 1.8, 1.0),
            },
            rate_func: RateFunc::DoubleSmooth,
        }),
    );

    // Clip 4: Fades out the entire hierarchy!
    // Simply fades parent opacity, which multiplies down to children via propagation!
    timeline.add_clip(
        track_id,
        3.5,
        1.0,
        ClipPayload::Animation(AnimationSpec {
            target: parent_id,
            lens: PropertyLensSpec::Opacity { from: 1.0, to: 0.0 },
            rate_func: RateFunc::DoubleSmooth,
        }),
    );

    // Enable looping playback from 0.0s to 5.0s
    timeline.loop_range = Some((0.0, 5.0));
    timeline.is_playing = true;
}

/// System: Synchronizes simulation DeltaTime and ticks the timeline
fn drive_timeline_clock(
    time: Res<Time>,
    mut delta_time: ResMut<DeltaTime>,
) {
    delta_time.dt = time.delta_secs() as f64;
}

/// System: Queries the accumulated global properties and renders everything in real-time
fn draw_mobjects_gizmos(
    query: Query<(
        &MobjectId,
        &GlobalSpatialTransform,
        &GlobalOpacity,
        Option<&FillBrush>,
        Option<&StrokeBrush>,
    ), With<Visible>>,
    mut gizmos: Gizmos,
) {
    // Draw each Mobject
    for (mobj_id, global_transform, opacity, fill_opt, stroke_opt) in &query {
        // Extract 2D translation and scale from the propagated 4x4 matrix
        let (_, _, translation) = global_transform.mat4.to_scale_rotation_translation();
        let pos = Vec2::new(translation.x as f32, translation.y as f32);
        
        let alpha = opacity.0;

        // Render Parent Mobject (ID = index 1) as a beautiful blue circle
        if mobj_id.0.index() == 1 {
            if let Some(fill) = fill_opt
                && let Some(gaanim_core::peniko::Brush::Solid(color)) = &fill.0 {
                    let rgba = color.to_rgba8();
                    let bevy_color = Color::srgba(
                        rgba.r as f32 / 255.0,
                        rgba.g as f32 / 255.0,
                        rgba.b as f32 / 255.0,
                        rgba.a as f32 / 255.0 * alpha,
                    );
                    
                    gizmos.circle_2d(pos, 50.0, bevy_color);
                }

            // Draw border outline
            if let Some(stroke) = stroke_opt
                && let Some(gaanim_core::peniko::Brush::Solid(color)) = &stroke.brush {
                    let rgba = color.to_rgba8();
                    let bevy_color = Color::srgba(
                        rgba.r as f32 / 255.0,
                        rgba.g as f32 / 255.0,
                        rgba.b as f32 / 255.0,
                        rgba.a as f32 / 255.0 * alpha,
                    );
                    gizmos.circle_2d(pos, 52.0, bevy_color);
                }
        }
        // Render Child Mobject (ID = index 2) as an orbiting pink circle
        else if mobj_id.0.index() == 2 {
            if let Some(fill) = fill_opt
                && let Some(gaanim_core::peniko::Brush::Solid(color)) = &fill.0 {
                    let rgba = color.to_rgba8();
                    let bevy_color = Color::srgba(
                        rgba.r as f32 / 255.0,
                        rgba.g as f32 / 255.0,
                        rgba.b as f32 / 255.0,
                        rgba.a as f32 / 255.0 * alpha,
                    );
                    
                    // Extract local scaling multiplier
                    let (scale, _, _) = global_transform.mat4.to_scale_rotation_translation();
                    let radius = 25.0 * scale.x as f32;

                    gizmos.circle_2d(pos, radius, bevy_color);
                }

            // Draw orbit path border outline
            if let Some(stroke) = stroke_opt
                && let Some(gaanim_core::peniko::Brush::Solid(color)) = &stroke.brush {
                    let rgba = color.to_rgba8();
                    let bevy_color = Color::srgba(
                        rgba.r as f32 / 255.0,
                        rgba.g as f32 / 255.0,
                        rgba.b as f32 / 255.0,
                        rgba.a as f32 / 255.0 * alpha,
                    );
                    let (scale, _, _) = global_transform.mat4.to_scale_rotation_translation();
                    let radius = 25.0 * scale.x as f32;

                    gizmos.circle_2d(pos, radius + 2.0, bevy_color);
                }
        }
    }
}
