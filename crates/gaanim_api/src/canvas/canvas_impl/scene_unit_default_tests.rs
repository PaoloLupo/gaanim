//! Defaults once authored in pixels must stay proportional to the 16 x 9
//! logical frame (pixel values / 100, like the migrated text roles).
use super::*;
use crate::canvas::ops::Op;
use crate::canvas::visualization::{ArrowFieldOptions, FlowParticleOptions, StreamLinesStyle};
use gaanim_core::glam::DVec3;

fn stroke_width(handle: &DrawableHandle) -> f64 {
    handle
        .spec
        .lock()
        .expect("object spec poisoned")
        .stroke
        .as_ref()
        .map(|(_, width)| *width)
        .expect("stroke")
}

fn fixed(x: f64, y: f64) -> CanvasEndpoint {
    CanvasEndpoint::Static(DVec3::new(x, y, 0.0))
}

#[test]
fn point_on_curve_marker_is_a_small_dot() {
    let mut canvas = SceneModel::new(320, 180);
    let curve = canvas.line(-2.0, 0.0, 2.0, 0.0);
    let tracker = canvas.value_tracker(0.0);
    let marker = canvas.point_on_curve(&curve, ScalarSource::signal(tracker.id));
    let spec = marker.spec.lock().expect("object spec poisoned");
    assert!(matches!(spec.kind, SpawnKind::Dot(radius) if radius == 0.08));
}

#[test]
fn mechanics_annotations_use_scene_unit_strokes_and_heads() {
    let mut canvas = SceneModel::new(320, 180);
    let vector = canvas
        .vector_between_with_parts(
            fixed(-1.0, 0.0),
            fixed(1.0, 0.5),
            None,
            false,
            ".1f".to_owned(),
            None,
            1.0,
            0.14,
            None,
            None,
        )
        .expect("vector");
    assert_eq!(stroke_width(&vector.shaft), 0.04);
    let state = canvas.state.lock().expect("canvas state poisoned");
    let head = state.active().ops.iter().find_map(|op| match op {
        Op::AttachTrackingVectorHead { length, width, .. } => Some((*length, *width)),
        _ => None,
    });
    assert_eq!(head, Some((0.16, 0.12)));
    drop(state);

    let angle = canvas
        .angle_between_with_options(
            fixed(0.0, 0.0),
            CanvasRay::Direction(DVec3::X),
            CanvasRay::Direction(DVec3::Y),
            0.64,
            AngleDimensionOptions::default(),
        )
        .expect("angle");
    assert_eq!(stroke_width(&angle.arc), 0.03);
    assert_eq!(stroke_width(&angle.extensions), 0.02);

    let rack = canvas.rack(2.75, 18, None);
    assert!(stroke_width(&rack) <= 0.05, "{}", stroke_width(&rack));
}

#[test]
fn option_defaults_are_in_scene_units() {
    assert_eq!(AngleDimensionOptions::default().label_gap, 0.12);
    let dimension = DimensionOptions::default();
    assert_eq!(
        (
            dimension.label_gap,
            dimension.line_width,
            dimension.dash_length,
            dimension.gap_length
        ),
        (0.10, 0.03, 0.12, 0.08)
    );
    assert_eq!(ArrowFieldOptions::default().width, 0.02);
    assert_eq!(ArrowFieldOptions::default().max_length, 0.28);
    assert_eq!(StreamLinesStyle::default().width, 0.02);
    assert_eq!(FlowParticleOptions::default().radius, 0.05);
}
