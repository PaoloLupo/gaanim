//! Lenses and components that recordings and caches write with serde read
//! back what they wrote, and older records keep their meaning.
#![cfg(feature = "serde")]

use gaanim_core::glam::DVec3;
use gaanim_timeline::clip::PropertyLensSpec;

/// An orbit recorded before orbits had a dolly keeps its distance.
#[test]
fn an_orbit_recorded_without_a_dolly_keeps_its_distance() {
    let recorded = r#"{"CameraOrbit": {
        "from_position": [0.0, 2.0, 12.0],
        "target": [0.0, 0.5, 0.0],
        "up": [0.0, 1.0, 0.0],
        "delta_yaw": 0.6,
        "delta_pitch": 0.1
    }}"#;
    let lens: PropertyLensSpec = serde_json::from_str(recorded).unwrap();
    let PropertyLensSpec::CameraOrbit { dolly, target, .. } = lens else {
        panic!("expected an orbit, got {lens:?}");
    };
    assert_eq!(dolly, 1.0);
    assert_eq!(target, DVec3::new(0.0, 0.5, 0.0));
}

/// A text span keeps its source range as `(start, end)`.
#[test]
fn a_text_span_keeps_its_source_range() {
    let span = gaanim_scene::components::TextSpan {
        character: 'E',
        char_index: 3,
        source_range: core::range::Range { start: 2, end: 5 },
    };
    let written = serde_json::to_string(&span).unwrap();
    assert!(written.contains("[2,5]"), "{written}");
    let read: gaanim_scene::components::TextSpan = serde_json::from_str(&written).unwrap();
    assert_eq!(read, span);
}

/// A morph table is a cache: it is written empty and read back as a new one.
#[test]
fn a_morph_table_is_written_empty() {
    let table = gaanim_animation::tween::MorphTable::default();
    let written = serde_json::to_string(&table).unwrap();
    assert_eq!(written, "null");
    let _: gaanim_animation::tween::MorphTable = serde_json::from_str(&written).unwrap();
}
