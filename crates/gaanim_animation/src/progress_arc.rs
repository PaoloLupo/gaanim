//! Arcs whose sweep follows a scalar source, for progress rings and timers.

use std::sync::Arc;

use bevy::prelude::*;
use gaanim_core::ObjectId;
use gaanim_core::kurbo::{self, BezPath, Shape};
use gaanim_scene::{Path2D, PathSource};

use crate::reactive::ScalarSource;
use crate::signals::FloatSignal;
use crate::writing::{PathReveal, PathTrimWindow, visible_path};

/// Tolerance of the flattened arc, in scene units.
const ARC_TOLERANCE: f64 = 1e-3;

/// Component: the path is a circular arc from 12 o'clock that sweeps
/// clockwise through `source / maximum` of a full turn, clamped to `[0, 1]`.
#[derive(Component, Debug, Clone)]
pub struct ProgressArc {
    pub source: ScalarSource,
    /// Logical parameter ids of `source` and the entities holding their signals.
    pub parameters: Vec<(ObjectId, Entity)>,
    pub radius: f64,
    /// Source value that completes the ring.
    pub maximum: f64,
    last: Option<(f64, Arc<BezPath>)>,
}

impl ProgressArc {
    pub fn new(
        source: ScalarSource,
        parameters: Vec<(ObjectId, Entity)>,
        radius: f64,
        maximum: f64,
    ) -> Self {
        Self {
            source,
            parameters,
            radius,
            maximum,
            last: None,
        }
    }

    /// Completed fraction of the ring for a source `value`.
    pub fn fraction(&self, value: f64) -> f64 {
        let fraction = value / self.maximum;
        if fraction.is_finite() {
            fraction.clamp(0.0, 1.0)
        } else {
            0.0
        }
    }
}

/// The arc of a ring of `radius` completed to `fraction`, centered at the
/// origin, starting at the top and running clockwise (y points up).
pub fn progress_arc_path(radius: f64, fraction: f64) -> BezPath {
    if fraction <= 0.0 {
        return BezPath::new();
    }
    let arc = kurbo::Arc::new(
        kurbo::Point::ORIGIN,
        kurbo::Vec2::new(radius, radius),
        std::f64::consts::FRAC_PI_2,
        -fraction.min(1.0) * std::f64::consts::TAU,
        0.0,
    );
    arc.path_elements(ARC_TOLERANCE).collect()
}

/// Rebuilds progress arcs when their source changes, and after a snapshot
/// replay restores an older path.
pub fn progress_arc_system(
    playback: Option<Res<crate::updaters::PlaybackState>>,
    mut query: Query<(
        &mut ProgressArc,
        &mut Path2D,
        Option<&mut PathSource>,
        Option<&PathReveal>,
        Option<&PathTrimWindow>,
    )>,
    signals: Query<&FloatSignal>,
) {
    let time = playback.map_or(0.0, |state| state.current_time);
    for (mut arc, mut path, source, reveal, trim) in &mut query {
        let value = arc
            .source
            .evaluate(time, |logical| {
                arc.parameters
                    .iter()
                    .find_map(|(id, entity)| (*id == logical).then_some(*entity))
                    .and_then(|entity| signals.get(entity).ok())
                    .map(|signal| signal.value)
            })
            .unwrap_or(f64::NAN);
        let fraction = arc.fraction(value);
        let full = match &arc.last {
            Some((last, full)) if *last == fraction => full.clone(),
            _ => {
                let full = Arc::new(progress_arc_path(arc.radius, fraction));
                arc.last = Some((fraction, full.clone()));
                full
            }
        };
        if let Some(mut source) = source
            && !Arc::ptr_eq(&source.0, &full)
        {
            source.0 = full.clone();
        }
        // Draw-on progress and trim windows apply to the regenerated arc.
        let reveal = reveal.map_or(1.0, |reveal| reveal.0.clamp(0.0, 1.0));
        let shown = if trim.is_none() && reveal >= 1.0 {
            full
        } else {
            visible_path(&full, reveal, trim)
        };
        if !Arc::ptr_eq(&path.0, &shown) {
            path.0 = shown;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arcs_start_at_the_top_and_run_clockwise() {
        assert!(progress_arc_path(1.0, 0.0).elements().is_empty());
        let quarter = progress_arc_path(2.0, 0.25);
        let rect = quarter.bounding_box();
        assert!((rect.x0 - 0.0).abs() < 1e-6 && (rect.x1 - 2.0).abs() < 1e-6);
        assert!((rect.y0 - 0.0).abs() < 1e-6 && (rect.y1 - 2.0).abs() < 1e-6);
        let full = progress_arc_path(1.0, 3.0).bounding_box();
        assert!((full.width() - 2.0).abs() < 1e-6 && (full.height() - 2.0).abs() < 1e-6);
    }

    #[test]
    fn arcs_follow_their_parameter_signal() {
        let mut world = World::new();
        let id = ObjectId::from_raw(7);
        let signal = world.spawn(FloatSignal::new(5.0)).id();
        let arc = ProgressArc::new(ScalarSource::signal(id), vec![(id, signal)], 1.0, 10.0);
        assert_eq!(arc.fraction(f64::NAN), 0.0);
        assert_eq!(arc.fraction(-1.0), 0.0);
        let entity = world.spawn((arc, Path2D(Arc::new(BezPath::new())))).id();
        let mut system = IntoSystem::into_system(progress_arc_system);
        system.initialize(&mut world);
        system.run((), &mut world).unwrap();
        let half = world.get::<Path2D>(entity).unwrap().0.bounding_box();
        // Half a ring clockwise from the top covers the right side.
        assert!((half.x0 - 0.0).abs() < 1e-6 && (half.x1 - 1.0).abs() < 1e-6);
        assert!((half.height() - 2.0).abs() < 1e-6);

        world.get_mut::<FloatSignal>(signal).unwrap().value = 10.0;
        system.run((), &mut world).unwrap();
        let full = world.get::<Path2D>(entity).unwrap().0.bounding_box();
        assert!((full.width() - 2.0).abs() < 1e-6);
    }
}
