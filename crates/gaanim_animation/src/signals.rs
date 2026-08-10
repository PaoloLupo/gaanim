use bevy::prelude::{Changed, Commands, Component, Entity, Query, World};
use gaanim_core::glam::DVec3;
use gaanim_core::kurbo::{BezPath, PathEl, Point};
use gaanim_core::peniko::Color;
use gaanim_math::{Bounds3D, SpatialTransform};
use std::collections::HashMap;
use std::sync::Arc;

/// A generic, observable reactive signal component.
///
/// Wraps any data type `T` inside Bevy ECS. Downstream reactive systems
/// can leverage Bevy's built-in change detection (`Changed<Signal<T>>`)
/// to trigger immediate, multi-threaded re-evaluations.
#[derive(Component, Debug, Clone)]
pub struct Signal<T: Send + Sync + Clone + 'static> {
    /// The current value of the signal.
    pub value: T,
}

impl<T: Send + Sync + Clone + 'static> Signal<T> {
    /// Creates a new reactive signal with an initial value.
    pub fn new(value: T) -> Self {
        Self { value }
    }
}

/// Helper aliases for common signal types.
pub type FloatSignal = Signal<f64>;
pub type Vec3Signal = Signal<gaanim_core::glam::DVec3>;
pub type ColorSignal = Signal<Color>;

/// Component defining a binding constraint between a source signal and a target entity.
///
/// When the source signal changes, the `apply` closure is executed to propagate
/// the new value into the target entity's components.
#[derive(Component)]
pub struct SignalBinding {
    /// The source entity holding the `Signal<T>` component.
    pub source: Entity,
    /// Closure that applies changes to the target entity using Bevy Commands.
    ///
    /// The closure receives the target entity and a `Commands` buffer.
    /// If reading from the `World` is required, the closure should use `SystemState`
    /// internally or capture necessary data at construction time.
    pub apply: Arc<dyn Fn(Entity, &mut Commands) + Send + Sync>,
}

impl SignalBinding {
    /// Creates a new signal binding between a source signal and a target.
    pub fn new(
        source: Entity,
        apply: impl Fn(Entity, &mut Commands) + Send + Sync + 'static,
    ) -> Self {
        Self {
            source,
            apply: Arc::new(apply),
        }
    }
}

/// Evaluates binding updates for a specific signal type `T` in parallel.
///
/// Utilizes Bevy's highly optimized `Changed` filter to skip unchanged signals.
pub fn signal_binding_system<T: Send + Sync + Clone + 'static>(
    mut commands: Commands,
    query_bindings: Query<(Entity, &SignalBinding)>,
    query_signals: Query<&Signal<T>, Changed<Signal<T>>>,
) {
    for (target_entity, binding) in &query_bindings {
        if query_signals.get(binding.source).is_ok() {
            // The source signal of type T changed during this frame! Re-run the binding.
            (binding.apply)(target_entity, &mut commands);
        }
    }
}

/// Strongly typed values supported by dynamic Mobject builders.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum SpecValue {
    Float(f64),
    Vec3(gaanim_core::glam::DVec3),
    Color(Color),
    String(String),
    Bool(bool),
}

/// A serialized/FFI-friendly representation of a Mobject's creation parameters.
///
/// Enables Python scripting and UI layers to interact dynamically with Mobjects.
#[derive(Debug, Clone)]
pub struct MobjectSpec {
    /// The kind of Mobject (e.g. "circle", "rectangle", "typst").
    pub kind: String,
    /// The map of property names to their spec values.
    pub params: HashMap<String, SpecValue>,
}

/// Component instructing the engine to rebuild a Mobject's geometry
/// on any frame where its dependent signals are modified.
#[derive(Component)]
pub struct AlwaysRedraw {
    /// The list of entities containing the source signals that this Mobject depends on.
    pub signals: Vec<Entity>,
    /// Builder closure that reads the World state and returns the updated Mobject specifications.
    pub builder: Arc<dyn Fn(&World) -> MobjectSpec + Send + Sync>,
}

impl AlwaysRedraw {
    /// Creates a new AlwaysRedraw component with a list of signals and a builder function.
    pub fn new(
        signals: Vec<Entity>,
        builder: impl Fn(&World) -> MobjectSpec + Send + Sync + 'static,
    ) -> Self {
        Self {
            signals,
            builder: Arc::new(builder),
        }
    }
}

// ---------------------------------------------------------------------------
// PositionBinding — copy position axes from source entity to target each frame
// ---------------------------------------------------------------------------

/// Which axes to copy in a `PositionBinding`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AxisMask {
    pub x: bool,
    pub y: bool,
    pub z: bool,
}

impl AxisMask {
    pub const X: Self = Self {
        x: true,
        y: false,
        z: false,
    };
    pub const Y: Self = Self {
        x: false,
        y: true,
        z: false,
    };
    pub const Z: Self = Self {
        x: false,
        y: false,
        z: true,
    };
    pub const XY: Self = Self {
        x: true,
        y: true,
        z: false,
    };
    pub const XYZ: Self = Self {
        x: true,
        y: true,
        z: true,
    };

    pub fn contains(self, other: Self) -> bool {
        (!other.x || self.x) && (!other.y || self.y) && (!other.z || self.z)
    }
}

/// Component that copies specified position axes from a source entity each frame.
///
/// Runs in `SceneSet::Updaters` (after updater_system) so it sees the
/// source's updated position from updaters like orbit/bob.
#[derive(Component)]
pub struct PositionBinding {
    /// The entity whose position is read each frame.
    pub source: Entity,
    /// Which axes to copy (X, Y, Z, or any combination).
    pub axes: AxisMask,
    /// Offset applied after copying the selected source axes.
    pub offset: DVec3,
}

impl PositionBinding {
    pub fn new(source: Entity, axes: AxisMask) -> Self {
        Self {
            source,
            axes,
            offset: DVec3::ZERO,
        }
    }

    pub fn with_offset(source: Entity, axes: AxisMask, offset: DVec3) -> Self {
        Self {
            source,
            axes,
            offset,
        }
    }
}

/// System that applies `PositionBinding` — copies source position axes to target.
pub fn position_binding_system(world: &mut World) {
    let mut updates = Vec::new();

    // Collect all bindings and their source positions.
    let mut query = world.query::<(Entity, &PositionBinding)>();
    for (target, binding) in query.iter(world) {
        if let Some(src_transform) = world.get::<SpatialTransform>(binding.source) {
            updates.push((
                target,
                src_transform.translation,
                binding.axes,
                binding.offset,
            ));
        }
    }

    // Apply the axis copies.
    for (target, src_pos, axes, offset) in updates {
        if let Some(mut transform) = world.get_mut::<SpatialTransform>(target) {
            if axes.contains(AxisMask::X) {
                transform.translation.x = src_pos.x + offset.x;
            }
            if axes.contains(AxisMask::Y) {
                transform.translation.y = src_pos.y + offset.y;
            }
            if axes.contains(AxisMask::Z) {
                transform.translation.z = src_pos.z + offset.z;
            }
        }
    }
}

// ---------------------------------------------------------------------------
// PointOnCurve — place a drawable at a normalized arc-length along a polyline
// ---------------------------------------------------------------------------

/// Keeps an entity positioned on a sampled curve using a normalized `FloatSignal`.
///
/// The curve is read as a `Path2D` and only its `MoveTo`/`LineTo` elements are
/// considered. This deliberately keeps the binding native and suitable for
/// polylines, function graphs, and parametric curves without calling Python per
/// frame.
#[derive(Component, Debug, Clone, Copy)]
pub struct PointOnCurve {
    /// Entity that owns the source `Path2D`.
    pub curve: Entity,
    /// Entity that owns the source `FloatSignal`.
    pub tracker: Entity,
}

impl PointOnCurve {
    pub fn new(curve: Entity, tracker: Entity) -> Self {
        Self { curve, tracker }
    }
}

/// Keeps a line centered on a sampled curve and aligned with its tangent.
#[derive(Component, Debug, Clone, Copy)]
pub struct TangentOnCurve {
    pub curve: Entity,
    pub tracker: Entity,
}

impl TangentOnCurve {
    pub fn new(curve: Entity, tracker: Entity) -> Self {
        Self { curve, tracker }
    }
}

/// Keeps a line centered on a sampled curve and aligned with its normal.
#[derive(Component, Debug, Clone, Copy)]
pub struct NormalOnCurve {
    pub curve: Entity,
    pub tracker: Entity,
}

impl NormalOnCurve {
    pub fn new(curve: Entity, tracker: Entity) -> Self {
        Self { curve, tracker }
    }
}

/// Keeps a unit circle scaled to the local osculating circle of a sampled curve.
#[derive(Component, Debug, Clone, Copy)]
pub struct CurvatureOnCurve {
    pub curve: Entity,
    pub tracker: Entity,
    pub window: f64,
}

impl CurvatureOnCurve {
    pub fn new(curve: Entity, tracker: Entity, window: f64) -> Self {
        Self {
            curve,
            tracker,
            window,
        }
    }
}

/// Updates `PointOnCurve` bindings after reactive curve regenerators.
pub fn point_on_curve_system(world: &mut World) {
    let mut updates = Vec::new();
    let mut query = world.query::<(Entity, &PointOnCurve)>();
    for (target, binding) in query.iter(world) {
        let Some(signal) = world.get::<FloatSignal>(binding.tracker) else {
            continue;
        };
        let Some(path) = world.get::<gaanim_scene::Path2D>(binding.curve) else {
            continue;
        };
        let Some(point) = point_at_polyline_fraction(path.0.as_ref(), signal.value) else {
            continue;
        };
        let z = world
            .get::<SpatialTransform>(target)
            .map(|transform| transform.translation.z)
            .unwrap_or(0.0);
        updates.push((target, DVec3::new(point.x, point.y, z)));
    }

    for (target, translation) in updates {
        if let Some(mut transform) = world.get_mut::<SpatialTransform>(target) {
            transform.translation = translation;
        }
    }
}

/// Updates tangent bindings after the curve and tracker have been updated.
pub fn tangent_on_curve_system(world: &mut World) {
    let mut updates = Vec::new();
    let mut query = world.query::<(Entity, &TangentOnCurve)>();
    for (target, binding) in query.iter(world) {
        let Some(signal) = world.get::<FloatSignal>(binding.tracker) else {
            continue;
        };
        let Some(path) = world.get::<gaanim_scene::Path2D>(binding.curve) else {
            continue;
        };
        let Some((point, tangent)) = sample_polyline(path.0.as_ref(), signal.value) else {
            continue;
        };
        let z = world
            .get::<SpatialTransform>(target)
            .map(|transform| transform.translation.z)
            .unwrap_or(0.0);
        updates.push((
            target,
            DVec3::new(point.x, point.y, z),
            gaanim_core::glam::DQuat::from_rotation_z(tangent.y.atan2(tangent.x)),
        ));
    }

    for (target, translation, rotation) in updates {
        if let Some(mut transform) = world.get_mut::<SpatialTransform>(target) {
            transform.translation = translation;
            transform.rotation = rotation;
        }
    }
}

/// Updates normal bindings after their source curve and tracker have changed.
pub fn normal_on_curve_system(world: &mut World) {
    let mut updates = Vec::new();
    let mut query = world.query::<(Entity, &NormalOnCurve)>();
    for (target, binding) in query.iter(world) {
        let Some(signal) = world.get::<FloatSignal>(binding.tracker) else {
            continue;
        };
        let Some(path) = world.get::<gaanim_scene::Path2D>(binding.curve) else {
            continue;
        };
        let Some((point, tangent)) = sample_polyline(path.0.as_ref(), signal.value) else {
            continue;
        };
        let z = world
            .get::<SpatialTransform>(target)
            .map(|transform| transform.translation.z)
            .unwrap_or(0.0);
        updates.push((
            target,
            DVec3::new(point.x, point.y, z),
            gaanim_core::glam::DQuat::from_rotation_z(
                tangent.y.atan2(tangent.x) + std::f64::consts::FRAC_PI_2,
            ),
        ));
    }

    for (target, translation, rotation) in updates {
        if let Some(mut transform) = world.get_mut::<SpatialTransform>(target) {
            transform.translation = translation;
            transform.rotation = rotation;
        }
    }
}

/// Updates osculating-circle bindings from three nearby arc-length samples.
pub fn curvature_on_curve_system(world: &mut World) {
    let mut updates = Vec::new();
    let mut query = world.query::<(Entity, &CurvatureOnCurve)>();
    for (target, binding) in query.iter(world) {
        let Some(signal) = world.get::<FloatSignal>(binding.tracker) else {
            continue;
        };
        let Some(path) = world.get::<gaanim_scene::Path2D>(binding.curve) else {
            continue;
        };
        let Some((center, radius)) =
            osculating_circle(path.0.as_ref(), signal.value, binding.window)
        else {
            continue;
        };
        let z = world
            .get::<SpatialTransform>(target)
            .map(|transform| transform.translation.z)
            .unwrap_or(0.0);
        updates.push((target, DVec3::new(center.x, center.y, z), radius));
    }
    for (target, translation, radius) in updates {
        if let Some(mut transform) = world.get_mut::<SpatialTransform>(target) {
            transform.translation = translation;
            transform.scale = DVec3::splat(radius);
        }
    }
}

fn point_at_polyline_fraction(path: &BezPath, fraction: f64) -> Option<Point> {
    sample_polyline(path, fraction).map(|(point, _)| point)
}

fn sample_polyline(path: &BezPath, fraction: f64) -> Option<(Point, gaanim_core::kurbo::Vec2)> {
    let mut segments = Vec::new();
    let mut current = None;
    let mut subpath_start = None;
    for element in path.elements() {
        match *element {
            PathEl::MoveTo(point) => {
                current = Some(point);
                subpath_start = Some(point);
            }
            PathEl::LineTo(point) => {
                if let Some(start) = current {
                    push_line_segment(&mut segments, start, point);
                }
                current = Some(point);
            }
            PathEl::QuadTo(control, point) => {
                if let Some(start) = current {
                    let mut previous = start;
                    for index in 1..=24 {
                        let t = index as f64 / 24.0;
                        let inverse = 1.0 - t;
                        let next = Point::new(
                            inverse * inverse * start.x
                                + 2.0 * inverse * t * control.x
                                + t * t * point.x,
                            inverse * inverse * start.y
                                + 2.0 * inverse * t * control.y
                                + t * t * point.y,
                        );
                        push_line_segment(&mut segments, previous, next);
                        previous = next;
                    }
                }
                current = Some(point);
            }
            PathEl::CurveTo(control1, control2, point) => {
                if let Some(start) = current {
                    let mut previous = start;
                    for index in 1..=32 {
                        let t = index as f64 / 32.0;
                        let inverse = 1.0 - t;
                        let next = Point::new(
                            inverse.powi(3) * start.x
                                + 3.0 * inverse * inverse * t * control1.x
                                + 3.0 * inverse * t * t * control2.x
                                + t.powi(3) * point.x,
                            inverse.powi(3) * start.y
                                + 3.0 * inverse * inverse * t * control1.y
                                + 3.0 * inverse * t * t * control2.y
                                + t.powi(3) * point.y,
                        );
                        push_line_segment(&mut segments, previous, next);
                        previous = next;
                    }
                }
                current = Some(point);
            }
            PathEl::ClosePath => {
                if let (Some(start), Some(end)) = (current, subpath_start) {
                    push_line_segment(&mut segments, start, end);
                    current = Some(end);
                }
            }
        }
    }

    let total_length: f64 = segments.iter().map(|(_, _, length)| length).sum();
    if total_length <= f64::EPSILON {
        return None;
    }
    let distance = fraction.clamp(0.0, 1.0) * total_length;
    let mut traversed = 0.0;
    for (start, end, length) in &segments {
        if distance <= traversed + length {
            return Some((
                start.lerp(*end, (distance - traversed) / length),
                *end - *start,
            ));
        }
        traversed += length;
    }
    segments.last().map(|(start, end, _)| (*end, *end - *start))
}

fn push_line_segment(segments: &mut Vec<(Point, Point, f64)>, start: Point, end: Point) {
    let length = (end - start).hypot();
    if length > f64::EPSILON {
        segments.push((start, end, length));
    }
}

fn osculating_circle(path: &BezPath, fraction: f64, window: f64) -> Option<(Point, f64)> {
    let fraction = fraction.clamp(0.0, 1.0);
    let window = window.clamp(1e-4, 0.5);
    let left = (fraction - window).max(0.0);
    let right = (fraction + window).min(1.0);
    if fraction - left <= f64::EPSILON || right - fraction <= f64::EPSILON {
        return None;
    }
    let a = point_at_polyline_fraction(path, left)?;
    let b = point_at_polyline_fraction(path, fraction)?;
    let c = point_at_polyline_fraction(path, right)?;
    let d = 2.0 * (a.x * (b.y - c.y) + b.x * (c.y - a.y) + c.x * (a.y - b.y));
    if d.abs() <= 1e-9 {
        return None;
    }
    let a2 = a.x * a.x + a.y * a.y;
    let b2 = b.x * b.x + b.y * b.y;
    let c2 = c.x * c.x + c.y * c.y;
    let center = Point::new(
        (a2 * (b.y - c.y) + b2 * (c.y - a.y) + c2 * (a.y - b.y)) / d,
        (a2 * (c.x - b.x) + b2 * (a.x - c.x) + c2 * (b.x - a.x)) / d,
    );
    let radius = (a - center).hypot();
    (radius.is_finite() && radius > f64::EPSILON).then_some((center, radius))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn point_on_curve_uses_normalized_arc_length_and_clamps_tracker() {
        let mut path = BezPath::new();
        path.move_to(Point::new(0.0, 0.0));
        path.line_to(Point::new(100.0, 0.0));
        path.line_to(Point::new(100.0, 300.0));

        let mut world = World::new();
        let curve = world.spawn(gaanim_scene::Path2D(Arc::new(path))).id();
        let tracker = world.spawn(FloatSignal::new(0.5)).id();
        let target = world
            .spawn((
                SpatialTransform::default(),
                PointOnCurve::new(curve, tracker),
            ))
            .id();

        point_on_curve_system(&mut world);
        assert_eq!(
            world.get::<SpatialTransform>(target).unwrap().translation,
            DVec3::new(100.0, 100.0, 0.0),
        );

        world.get_mut::<FloatSignal>(tracker).unwrap().value = 0.5;
        let tangent = world
            .spawn((
                SpatialTransform::default(),
                TangentOnCurve::new(curve, tracker),
            ))
            .id();
        tangent_on_curve_system(&mut world);
        let transform = world.get::<SpatialTransform>(tangent).unwrap();
        assert_eq!(transform.translation, DVec3::new(100.0, 100.0, 0.0));
        let direction = transform.rotation * DVec3::X;
        assert!(direction.x.abs() < 1e-9 && (direction.y - 1.0).abs() < 1e-9);

        let normal = world
            .spawn((
                SpatialTransform::default(),
                NormalOnCurve::new(curve, tracker),
            ))
            .id();
        normal_on_curve_system(&mut world);
        let direction = world.get::<SpatialTransform>(normal).unwrap().rotation * DVec3::X;
        assert!((direction.x + 1.0).abs() < 1e-9 && direction.y.abs() < 1e-9);

        let mut cubic = BezPath::new();
        cubic.move_to(Point::new(0.0, 0.0));
        cubic.curve_to(
            Point::new(0.0, 120.0),
            Point::new(120.0, 120.0),
            Point::new(120.0, 0.0),
        );
        assert_eq!(
            point_at_polyline_fraction(&cubic, 0.0),
            Some(Point::new(0.0, 0.0))
        );
        let end = point_at_polyline_fraction(&cubic, 1.0).expect("cubic endpoint");
        assert!((end.x - 120.0).abs() < 1e-9 && end.y.abs() < 1e-9);

        world.get_mut::<FloatSignal>(tracker).unwrap().value = 2.0;
        point_on_curve_system(&mut world);
        assert_eq!(
            world.get::<SpatialTransform>(target).unwrap().translation,
            DVec3::new(100.0, 300.0, 0.0),
        );
    }
}

// ---------------------------------------------------------------------------
// AlwaysRedrawRegen — rebuild Path2D each frame via a closure
// ---------------------------------------------------------------------------

/// Component that regenerates an entity's `Path2D` every frame by calling a closure
/// with `&mut World` access. Unlike `AlwaysRedraw` (which returns a spec),
/// this directly writes the new path.
#[derive(Component)]
pub struct AlwaysRedrawRegen {
    /// Closure that reads world state and returns a new BezPath.
    pub regen: Arc<dyn Fn(&World) -> gaanim_core::kurbo::BezPath + Send + Sync>,
}

impl AlwaysRedrawRegen {
    pub fn new(
        regen: impl Fn(&World) -> gaanim_core::kurbo::BezPath + Send + Sync + 'static,
    ) -> Self {
        Self {
            regen: Arc::new(regen),
        }
    }
}

/// Exclusive system that executes `AlwaysRedrawRegen` closures and updates Path2D.
pub fn always_redraw_regen_system(world: &mut World) {
    let mut updates = Vec::new();

    let mut query = world.query::<(Entity, &AlwaysRedrawRegen)>();
    for (entity, regen) in query.iter(world) {
        let path = (regen.regen)(world);
        let bounds = if path.elements().is_empty() {
            Bounds3D::default()
        } else {
            let rect = gaanim_core::kurbo::Shape::bounding_box(&path);
            Bounds3D::new_2d(
                rect.x0 - 12.0,
                rect.y0 - 12.0,
                rect.x1 + 12.0,
                rect.y1 + 12.0,
            )
        };
        // Respect any active PathReveal (draw) progress. Without this,
        // a reactive ExpressionPlot would overwrite the trimmed Path2D
        // produced by its Write/Create animation and appear fully from
        // frame 0.
        let reveal = world
            .get::<crate::writing::PathReveal>(entity)
            .map(|r| r.0)
            .unwrap_or(1.0)
            .clamp(0.0, 1.0);
        updates.push((entity, path, bounds, reveal));
    }

    for (entity, path, bounds, reveal) in updates {
        let trimmed = if (reveal - 1.0).abs() < 1e-9 {
            path.clone()
        } else {
            gaanim_math::get_subpath(&path, reveal)
        };
        if let Some(mut path_comp) = world.get_mut::<gaanim_scene::Path2D>(entity) {
            path_comp.0 = std::sync::Arc::new(trimmed);
        }
        if let Some(mut path_source) = world.get_mut::<gaanim_scene::PathSource>(entity) {
            path_source.0 = std::sync::Arc::new(path);
        }
        if let Some(mut local_bounds) = world.get_mut::<gaanim_scene::LocalBounds>(entity) {
            local_bounds.0 = bounds;
        }
        // Keep the PathReveal component alive so snapshot restore
        // can see the correct reveal factor.
        if world.get::<crate::writing::PathReveal>(entity).is_none() && (reveal - 1.0).abs() > 1e-9 {
            if let Ok(mut em) = world.get_entity_mut(entity) {
                em.insert(crate::writing::PathReveal(reveal));
            }
        }
    }
}
