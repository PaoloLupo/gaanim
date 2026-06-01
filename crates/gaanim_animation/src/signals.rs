use bevy::prelude::{Changed, Commands, Component, Entity, Query, World};
use gaanim_core::peniko::Color;
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
