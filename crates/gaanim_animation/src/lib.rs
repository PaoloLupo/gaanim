pub mod camera;
pub mod prelude;
pub mod signals;
pub mod tween;
pub mod updaters;
pub mod writing;

pub use camera::{CameraBinding, CameraBindingKind, CameraBindingWindow, apply_camera_bindings};
pub use signals::{
    AlwaysRedraw, AlwaysRedrawRegen, AxisMask, ColorSignal, CurvatureOnCurve, FloatSignal,
    MobjectSpec, NormalOnCurve, PointOnCurve, PositionBinding, ReactiveReadout,
    ReactiveReadoutLayout, Signal, SignalBinding, SpecValue, TangentOnCurve, Vec3Signal,
    always_redraw_regen_system, curvature_on_curve_system, format_reactive_number,
    normal_on_curve_system, point_on_curve_system, position_binding_system,
    reactive_readout_layout_system, reactive_readout_update_system, right_align_readout_path,
    right_aligned_readout_baseline, signal_binding_system, tangent_on_curve_system,
};
pub use tween::{
    AnimatableLens, CameraStateSource, DeltaTime, MorphTable, PropertyLens, Tween, TweenState,
    evaluate_custom_tweens_system, evaluate_line_path_ranges_system, evaluate_tweens_system,
    sync_delta_time_system,
};
pub use updaters::{
    AngleArrowheads, AngleLabelPlacement, AngleSweep, DimensionLabelOrientation,
    DimensionLabelPlacement, EndpointAngle, EndpointDistance, EndpointFollow, FollowOffsetSpace,
    InvalidFixedStep, InvalidSampledSeries, PlaybackState, RotationBinding,
    RotationTranslationBinding, SampledInterpolation, SampledProperty, SampledSeriesDriver,
    SurroundingRect, TracedPath, TracedPath3D, TrackingAngle, TrackingAnglePart, TrackingEndpoint,
    TrackingLine, TrackingRay, TrackingScalar, TrackingVectorHead, Updater, advance_updaters_by,
    advance_x_updater, angle_label_placement_system, bob_updater, dimension_label_placement_system,
    endpoint_angle_system, endpoint_distance_system, endpoint_follow_system, follow_updater,
    mechanism_binding_system, orbit_updater, pulse_updater, resolve_entity_bounds,
    resolve_tracking_endpoint, resolve_tracking_endpoint_with_offset, rotate_updater,
    sampled_series_system, seek_updaters, surrounding_rect_system, traced_path_3d_system,
    traced_path_system, tracking_angle_system, tracking_line_system, tracking_vector_head_system,
    tracking_world_to_local, updater_system,
};
pub use writing::{
    FillDrawProgress, PathReveal, PathSource, WriteTipGlow, path_source_seed_added_system,
};

use bevy::prelude::*;
use gaanim_scene::SceneSet;

/// Main plugin registering the tween evaluation and reactive signal systems in Bevy's deterministic schedule.
pub struct GaanimAnimationPlugin;

impl bevy::prelude::Plugin for GaanimAnimationPlugin {
    fn build(&self, app: &mut App) {
        // Register DeltaTime resource
        app.init_resource::<DeltaTime>();
        app.init_resource::<PlaybackState>();

        // Sync Bevy's Time -> DeltaTime before animation evaluation.
        app.add_systems(Update, sync_delta_time_system.in_set(SceneSet::Input));

        // PathSource seed runs in Input so that any entity that has just
        // received a `Path2D` (from spawn, snapshot restore, morph, etc.)
        // gets a `PathSource` mirror before the animation phase reads it.
        app.add_systems(
            Update,
            path_source_seed_added_system
                .in_set(SceneSet::Input)
                .after(sync_delta_time_system),
        );
        app.add_systems(
            Update,
            (
                reactive_readout_update_system,
                reactive_readout_layout_system.after(reactive_readout_update_system),
            )
                .in_set(SceneSet::Visualization),
        );

        // Register tween evaluation in the Animation Phase.
        // evaluate_tweens_system runs in parallel for built-in lenses,
        // followed by evaluate_custom_tweens_system for exclusive World access.
        // The `PathCompletion` lens writes the trimmed `Path2D` directly
        // from the cached `PathSource` (seeded once at spawn), so we no
        // longer need a separate application system for it.
        app.add_systems(
            Update,
            (
                evaluate_tweens_system,
                evaluate_line_path_ranges_system.after(evaluate_tweens_system),
                evaluate_custom_tweens_system.after(evaluate_line_path_ranges_system),
            )
                .in_set(SceneSet::Animation),
        );

        // Register standard signal binders and continuous updaters in the Updaters Phase.
        // Ordering: updaters run first (modify positions), then bindings copy positions,
        // then tracking lines and traced paths read the final positions.
        app.add_systems(
            Update,
            (
                updater_system,
                sampled_series_system.after(updater_system),
                position_binding_system.after(sampled_series_system),
                mechanism_binding_system.after(position_binding_system),
                endpoint_follow_system.after(mechanism_binding_system),
                always_redraw_regen_system.after(endpoint_follow_system),
                tracking_line_system.after(always_redraw_regen_system),
                tracking_angle_system.after(tracking_line_system),
                tracking_vector_head_system.after(tracking_angle_system),
                endpoint_distance_system.after(tracking_vector_head_system),
                endpoint_angle_system.after(endpoint_distance_system),
                dimension_label_placement_system.after(endpoint_angle_system),
                angle_label_placement_system.after(dimension_label_placement_system),
                traced_path_system.after(angle_label_placement_system),
                traced_path_3d_system.after(traced_path_system),
                point_on_curve_system.after(traced_path_3d_system),
                tangent_on_curve_system.after(point_on_curve_system),
                normal_on_curve_system.after(tangent_on_curve_system),
                curvature_on_curve_system.after(normal_on_curve_system),
            )
                .in_set(SceneSet::Updaters),
        );
        app.add_systems(
            Update,
            surrounding_rect_system.in_set(SceneSet::DerivedGeometry),
        );
        app.add_systems(
            Update,
            (
                signal_binding_system::<f64>,
                signal_binding_system::<gaanim_core::glam::DVec3>,
                signal_binding_system::<gaanim_core::peniko::Color>,
            )
                .in_set(SceneSet::Updaters),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gaanim_core::glam::DVec3;
    use gaanim_core::kurbo::{BezPath, PathEl, Point};
    use gaanim_math::{Bounds3D, SpatialTransform};
    use gaanim_scene::{LocalBounds, Path2D, PathSource};
    use std::sync::Arc;

    #[test]
    fn reactive_regeneration_observes_endpoint_follow_in_the_same_frame() {
        let mut app = App::new();
        app.insert_resource(Time::<()>::default())
            .insert_resource(gaanim_text::font::FontRegistry::new())
            .add_plugins((
                gaanim_scene::hierarchy::GaanimScenePlugin,
                GaanimAnimationPlugin,
            ));

        let expected = DVec3::new(40.0, 20.0, 0.0);
        let follower = app
            .world_mut()
            .spawn((
                SpatialTransform::default(),
                EndpointFollow {
                    endpoint: TrackingEndpoint::Static(expected),
                    offset: DVec3::ZERO,
                    offset_space: FollowOffsetSpace::World,
                },
            ))
            .id();

        let empty = Arc::new(BezPath::new());
        let reactive = app
            .world_mut()
            .spawn((
                SpatialTransform::default(),
                Path2D(empty.clone()),
                PathSource(empty),
                LocalBounds(Bounds3D::default()),
                AlwaysRedrawRegen::new(move |world| {
                    let position =
                        resolve_tracking_endpoint(&TrackingEndpoint::Entity(follower), world)
                            .expect("followed endpoint");
                    let mut path = BezPath::new();
                    path.move_to(Point::new(position.x, position.y));
                    path.line_to(Point::new(position.x + 1.0, position.y));
                    path
                }),
            ))
            .id();

        app.update();

        assert!(matches!(
            app.world()
                .get::<Path2D>(reactive)
                .expect("regenerated path")
                .0
                .elements()
                .first(),
            Some(PathEl::MoveTo(point)) if *point == Point::new(expected.x, expected.y)
        ));
    }
}
