//! Deterministic scalar and vector callables used by language bindings.
//!
//! This module deliberately contains no expression tree.  A binding supplies
//! an opaque pure callback plus an ordered list of explicit inputs.  Runtime
//! evaluation reads those inputs from the ECS world at the requested timeline
//! time, which makes direct seeks independent of the path used to reach them.

use std::collections::BTreeMap;
use std::fmt;
use std::sync::{Arc, Mutex};

use bevy::prelude::{Entity, World};
use gaanim_core::ObjectId;

use crate::signals::FloatSignal;
use crate::updaters::PlaybackState;

/// One explicitly declared input to a reactive Python callable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ReactiveInput {
    /// Current value of an animatable scalar identified in authoring space.
    Signal(ObjectId),
    /// Absolute timeline time in seconds.
    Time,
}

/// Stable failure reported by an opaque reactive callable.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ReactiveError {
    #[error("reactive callable expected {expected} coordinate values, received {actual}")]
    CoordinateArity { expected: usize, actual: usize },
    #[error("reactive callable expected {expected} outputs, received {actual}")]
    OutputArity { expected: usize, actual: usize },
    #[error("reactive input {0:?} could not be resolved")]
    MissingInput(ReactiveInput),
    #[error("reactive callable received or returned a non-finite value")]
    NonFinite,
    #[error("reactive callable failed: {0}")]
    Callback(String),
}

type ReactiveCallback = Arc<dyn Fn(&[f64]) -> Result<Vec<f64>, String> + Send + Sync + 'static>;

/// Opaque deterministic mapping used when a scalar needs a native coordinate
/// transform without introducing an expression AST or a visualization-crate
/// dependency into the animation layer.
#[derive(Clone)]
pub struct ScalarMap(Arc<dyn Fn(f64) -> Option<f64> + Send + Sync + 'static>);

impl fmt::Debug for ScalarMap {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.debug_tuple("ScalarMap").finish_non_exhaustive()
    }
}

impl ScalarMap {
    #[doc(hidden)]
    pub fn new(map: impl Fn(f64) -> Option<f64> + Send + Sync + 'static) -> Self {
        Self(Arc::new(map))
    }

    pub fn evaluate(&self, value: f64) -> Option<f64> {
        value.is_finite().then(|| (self.0)(value)).flatten()
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
struct FunctionCache {
    input_snapshot: Vec<u64>,
    coordinates: BTreeMap<Vec<u64>, Vec<f64>>,
}

/// Opaque pure function with fixed coordinate/output arity and explicit inputs.
#[derive(Clone)]
pub struct ReactiveFunction {
    coordinate_arity: usize,
    output_arity: usize,
    inputs: Arc<[ReactiveInput]>,
    scene_owners: Arc<[u64]>,
    callback: ReactiveCallback,
    cache: Arc<Mutex<FunctionCache>>,
}

impl fmt::Debug for ReactiveFunction {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ReactiveFunction")
            .field("coordinate_arity", &self.coordinate_arity)
            .field("output_arity", &self.output_arity)
            .field("inputs", &self.inputs)
            .finish_non_exhaustive()
    }
}

impl ReactiveFunction {
    /// Construct an opaque function. Language bindings are responsible for
    /// adapting their callable into the numeric callback used here.
    #[doc(hidden)]
    pub fn new(
        coordinate_arity: usize,
        output_arity: usize,
        inputs: Vec<ReactiveInput>,
        callback: impl Fn(&[f64]) -> Result<Vec<f64>, String> + Send + Sync + 'static,
    ) -> Self {
        Self {
            coordinate_arity,
            output_arity,
            inputs: inputs.into(),
            scene_owners: Arc::from([]),
            callback: Arc::new(callback),
            cache: Arc::new(Mutex::new(FunctionCache::default())),
        }
    }

    pub fn coordinate_arity(&self) -> usize {
        self.coordinate_arity
    }

    /// Scene identities attached by an authoring facade, retained transitively.
    pub fn scene_owners(&self) -> &[u64] {
        &self.scene_owners
    }

    pub fn with_scene_owner(mut self, owner: u64) -> Self {
        let mut owners = self.scene_owners.to_vec();
        if !owners.contains(&owner) {
            owners.push(owner);
        }
        self.scene_owners = owners.into();
        self
    }

    /// Compose scalar sources while retaining a single snapshot of their leaf inputs.
    ///
    /// Sources may share other functions. Their existing caches remain shared, so a
    /// diamond of derived values evaluates its common dependencies only once for
    /// each input snapshot. The callback receives coordinates followed by source
    /// values in declaration order, independently of leaf deduplication.
    pub fn from_sources(
        coordinate_arity: usize,
        output_arity: usize,
        sources: Vec<ScalarSource>,
        callback: impl Fn(&[f64]) -> Result<Vec<f64>, String> + Send + Sync + 'static,
    ) -> Self {
        let mut owners = Vec::new();
        let mut leaves = Vec::new();
        for source in &sources {
            for owner in source.scene_owners() {
                if !owners.contains(owner) {
                    owners.push(*owner);
                }
            }
            let inputs = match source {
                ScalarSource::Constant(_) => Vec::new(),
                ScalarSource::Signal(id) => vec![ReactiveInput::Signal(*id)],
                ScalarSource::Time => vec![ReactiveInput::Time],
                ScalarSource::Function(function) => function.inputs().to_vec(),
            };
            for input in inputs {
                if !leaves.contains(&input) {
                    leaves.push(input);
                }
            }
        }
        let snapshot_inputs = leaves.clone();
        let mut composed = Self::new(coordinate_arity, output_arity, leaves, move |arguments| {
            let snapshot = &arguments[coordinate_arity..];
            let time = snapshot_inputs
                .iter()
                .position(|input| *input == ReactiveInput::Time)
                .map_or(0.0, |index| snapshot[index]);
            let mut values = arguments[..coordinate_arity].to_vec();
            for source in &sources {
                values.push(
                    source
                        .evaluate(time, |id| {
                            snapshot_inputs
                                .iter()
                                .position(|input| *input == ReactiveInput::Signal(id))
                                .map(|index| snapshot[index])
                        })
                        .map_err(|error| error.to_string())?,
                );
            }
            callback(&values)
        });
        composed.scene_owners = owners.into();
        composed
    }

    pub fn output_arity(&self) -> usize {
        self.output_arity
    }

    pub fn inputs(&self) -> &[ReactiveInput] {
        &self.inputs
    }

    pub fn parameter_ids(&self) -> Vec<ObjectId> {
        self.inputs
            .iter()
            .filter_map(|input| match input {
                ReactiveInput::Signal(id) => Some(*id),
                ReactiveInput::Time => None,
            })
            .collect()
    }

    pub fn depends_on_time(&self) -> bool {
        self.inputs.contains(&ReactiveInput::Time)
    }

    /// Transform the single output of a zero-coordinate callable while
    /// preserving its explicit input list.
    pub fn map_scalar(
        &self,
        map: impl Fn(f64) -> f64 + Send + Sync + 'static,
    ) -> Result<Self, ReactiveError> {
        if self.coordinate_arity != 0 {
            return Err(ReactiveError::CoordinateArity {
                expected: 0,
                actual: self.coordinate_arity,
            });
        }
        if self.output_arity != 1 {
            return Err(ReactiveError::OutputArity {
                expected: 1,
                actual: self.output_arity,
            });
        }
        Ok(Self::from_sources(
            0,
            1,
            vec![ScalarSource::Function(self.clone())],
            move |arguments| Ok(vec![map(arguments[0])]),
        ))
    }

    /// Evaluate using a caller-provided logical signal resolver.
    pub fn evaluate(
        &self,
        coordinates: &[f64],
        time: f64,
        mut signal: impl FnMut(ObjectId) -> Option<f64>,
    ) -> Result<Vec<f64>, ReactiveError> {
        if coordinates.len() != self.coordinate_arity {
            return Err(ReactiveError::CoordinateArity {
                expected: self.coordinate_arity,
                actual: coordinates.len(),
            });
        }
        if !coordinates.iter().all(|value| value.is_finite()) || !time.is_finite() {
            return Err(ReactiveError::NonFinite);
        }

        let mut arguments = Vec::with_capacity(coordinates.len() + self.inputs.len());
        arguments.extend_from_slice(coordinates);
        for input in self.inputs.iter().copied() {
            let value = match input {
                ReactiveInput::Signal(id) => {
                    signal(id).ok_or(ReactiveError::MissingInput(input))?
                }
                ReactiveInput::Time => time,
            };
            if !value.is_finite() {
                return Err(ReactiveError::NonFinite);
            }
            arguments.push(value);
        }

        let coordinate_key = coordinates
            .iter()
            .map(|value| value.to_bits())
            .collect::<Vec<_>>();
        let input_snapshot = arguments[self.coordinate_arity..]
            .iter()
            .map(|value| value.to_bits())
            .collect::<Vec<_>>();
        {
            let mut cache = self.cache.lock().expect("reactive cache poisoned");
            if cache.input_snapshot != input_snapshot {
                cache.input_snapshot = input_snapshot.clone();
                cache.coordinates.clear();
            }
            if let Some(value) = cache.coordinates.get(&coordinate_key) {
                return Ok(value.clone());
            }
        }

        let value = (self.callback)(&arguments).map_err(ReactiveError::Callback)?;
        if value.len() != self.output_arity {
            return Err(ReactiveError::OutputArity {
                expected: self.output_arity,
                actual: value.len(),
            });
        }
        if !value.iter().all(|value| value.is_finite()) {
            return Err(ReactiveError::NonFinite);
        }
        let mut cache = self.cache.lock().expect("reactive cache poisoned");
        if cache.input_snapshot != input_snapshot {
            cache.input_snapshot = input_snapshot;
            cache.coordinates.clear();
        }
        cache.coordinates.insert(coordinate_key, value.clone());
        Ok(value)
    }
}

/// Scalar source used by endpoints, camera bindings, reveal cursors and readouts.
#[derive(Debug, Clone)]
pub enum ScalarSource {
    Constant(f64),
    Signal(ObjectId),
    Time,
    Function(ReactiveFunction),
}

impl ScalarSource {
    pub fn scene_owners(&self) -> &[u64] {
        match self {
            Self::Function(function) => function.scene_owners(),
            _ => &[],
        }
    }
    pub fn constant(value: f64) -> Self {
        Self::Constant(value)
    }

    pub fn signal(id: ObjectId) -> Self {
        Self::Signal(id)
    }

    pub fn function(function: ReactiveFunction) -> Result<Self, ReactiveError> {
        if function.coordinate_arity() != 0 {
            return Err(ReactiveError::CoordinateArity {
                expected: 0,
                actual: function.coordinate_arity(),
            });
        }
        if function.output_arity() != 1 {
            return Err(ReactiveError::OutputArity {
                expected: 1,
                actual: function.output_arity(),
            });
        }
        Ok(Self::Function(function))
    }

    pub fn parameter_ids(&self) -> Vec<ObjectId> {
        match self {
            Self::Signal(id) => vec![*id],
            Self::Function(function) => function.parameter_ids(),
            Self::Constant(_) | Self::Time => Vec::new(),
        }
    }

    pub fn depends_on_time(&self) -> bool {
        matches!(self, Self::Time)
            || matches!(self, Self::Function(function) if function.depends_on_time())
    }

    pub fn constant_value(&self) -> Option<f64> {
        match self {
            Self::Constant(value) => Some(*value),
            _ => None,
        }
    }

    pub fn scaled(&self, factor: f64) -> Self {
        match self {
            Self::Constant(value) => Self::Constant(*value * factor),
            Self::Signal(id) => {
                let id = *id;
                Self::Function(ReactiveFunction::new(
                    0,
                    1,
                    vec![ReactiveInput::Signal(id)],
                    move |arguments| Ok(vec![arguments[0] * factor]),
                ))
            }
            Self::Time => Self::Function(ReactiveFunction::new(
                0,
                1,
                vec![ReactiveInput::Time],
                move |arguments| Ok(vec![arguments[0] * factor]),
            )),
            Self::Function(function) => Self::Function(
                function
                    .map_scalar(move |value| value * factor)
                    .expect("ScalarSource functions always have scalar arity"),
            ),
        }
    }

    pub fn evaluate(
        &self,
        time: f64,
        mut signal: impl FnMut(ObjectId) -> Option<f64>,
    ) -> Result<f64, ReactiveError> {
        let value = match self {
            Self::Constant(value) => *value,
            Self::Signal(id) => {
                signal(*id).ok_or(ReactiveError::MissingInput(ReactiveInput::Signal(*id)))?
            }
            Self::Time => time,
            Self::Function(function) => function.evaluate(&[], time, signal)?[0],
        };
        value
            .is_finite()
            .then_some(value)
            .ok_or(ReactiveError::NonFinite)
    }
}

impl From<f64> for ScalarSource {
    fn from(value: f64) -> Self {
        Self::Constant(value)
    }
}

impl From<f32> for ScalarSource {
    fn from(value: f32) -> Self {
        Self::Constant(f64::from(value))
    }
}

/// Runtime scalar with logical signal ids resolved to ECS entities.
#[derive(Debug, Clone)]
pub struct ResolvedScalarSource {
    pub source: ScalarSource,
    pub parameters: Vec<(ObjectId, Entity)>,
}

impl ResolvedScalarSource {
    pub fn evaluate(&self, world: &World) -> Option<f64> {
        let time = world
            .get_resource::<PlaybackState>()
            .map_or(0.0, |state| state.current_time);
        self.source
            .evaluate(time, |logical| {
                self.parameters
                    .iter()
                    .find_map(|(id, entity)| (*id == logical).then_some(*entity))
                    .and_then(|entity| world.get::<FloatSignal>(entity))
                    .map(|signal| signal.value)
            })
            .ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn composed_sources_share_diamond_cache_and_preserve_argument_order() {
        let id = ObjectId::from_raw(42);
        let calls = Arc::new(Mutex::new(0));
        let observed = calls.clone();
        let common = ScalarSource::function(ReactiveFunction::from_sources(
            0,
            1,
            vec![ScalarSource::Signal(id)],
            move |values| {
                *observed.lock().unwrap() += 1;
                Ok(vec![values[0] * values[0]])
            },
        ))
        .unwrap();
        let left = ScalarSource::function(ReactiveFunction::from_sources(
            0,
            1,
            vec![common.clone()],
            |values| Ok(vec![values[0] + 1.0]),
        ))
        .unwrap();
        let right = ScalarSource::function(ReactiveFunction::from_sources(
            0,
            1,
            vec![common],
            |values| Ok(vec![values[0] * 2.0]),
        ))
        .unwrap();
        let result =
            ReactiveFunction::from_sources(1, 1, vec![right, ScalarSource::Time, left], |values| {
                Ok(vec![values[0] + values[1] * 10.0 + values[2] + values[3]])
            });
        assert_eq!(result.parameter_ids(), vec![id]);
        assert!(result.depends_on_time());
        assert_eq!(result.evaluate(&[3.0], 2.0, |_| Some(2.0)), Ok(vec![90.0]));
        assert_eq!(result.evaluate(&[3.0], 2.0, |_| Some(2.0)), Ok(vec![90.0]));
        assert_eq!(result.evaluate(&[4.0], 3.0, |_| Some(2.0)), Ok(vec![92.0]));
        assert_eq!(*calls.lock().unwrap(), 1);
        assert_eq!(result.evaluate(&[3.0], 2.0, |_| Some(3.0)), Ok(vec![195.0]));
        assert_eq!(*calls.lock().unwrap(), 2);
    }

    #[test]
    fn composed_sources_propagate_failures_and_seek_exactly() {
        let inner = ScalarSource::function(ReactiveFunction::from_sources(
            0,
            1,
            vec![ScalarSource::Time],
            |values| {
                if values[0] == 1.0 {
                    Ok(vec![f64::NAN])
                } else {
                    Ok(vec![values[0] * 2.0])
                }
            },
        ))
        .unwrap();
        let outer =
            ReactiveFunction::from_sources(0, 1, vec![inner], |values| Ok(vec![values[0] + 1.0]));
        for time in [0.0, 2.0, 0.5, 2.0] {
            assert_eq!(
                outer.evaluate(&[], time, |_| None),
                Ok(vec![time * 2.0 + 1.0])
            );
        }
        assert!(outer.evaluate(&[], 1.0, |_| None).is_err());
        assert_eq!(outer.evaluate(&[], 2.0, |_| None), Ok(vec![5.0]));
    }

    #[test]
    fn explicit_inputs_keep_declared_order_and_cache_identical_snapshots() {
        let first = ObjectId::from_raw(1);
        let second = ObjectId::from_raw(2);
        let calls = Arc::new(Mutex::new(0usize));
        let observed = calls.clone();
        let function = ReactiveFunction::new(
            1,
            1,
            vec![ReactiveInput::Signal(second), ReactiveInput::Signal(first)],
            move |values| {
                *observed.lock().unwrap() += 1;
                Ok(vec![values[0] + 10.0 * values[1] + 100.0 * values[2]])
            },
        );
        let resolve = |id| match id {
            value if value == first => Some(3.0),
            value if value == second => Some(2.0),
            _ => None,
        };
        assert_eq!(function.evaluate(&[1.0], 0.0, resolve), Ok(vec![321.0]));
        assert_eq!(function.evaluate(&[2.0], 0.0, resolve), Ok(vec![322.0]));
        assert_eq!(function.evaluate(&[1.0], 0.0, resolve), Ok(vec![321.0]));
        assert_eq!(*calls.lock().unwrap(), 2);
    }

    #[test]
    fn time_is_an_explicit_deterministic_input() {
        let function = ReactiveFunction::new(0, 1, vec![ReactiveInput::Time], |values| {
            Ok(vec![values[0] * 2.0])
        });
        assert_eq!(function.evaluate(&[], 1.25, |_| None), Ok(vec![2.5]));
        assert_eq!(function.evaluate(&[], 0.5, |_| None), Ok(vec![1.0]));
        assert_eq!(function.evaluate(&[], 1.25, |_| None), Ok(vec![2.5]));
    }

    #[test]
    fn failures_never_reuse_a_previous_valid_value() {
        let signal = ObjectId::from_raw(7);
        let function =
            ReactiveFunction::new(
                0,
                1,
                vec![ReactiveInput::Signal(signal)],
                |values| match values[0] {
                    value if value < 0.0 => Err("negative input".to_owned()),
                    value if value == 0.0 => Ok(vec![f64::NAN]),
                    value => Ok(vec![value * 2.0]),
                },
            );

        assert_eq!(function.evaluate(&[], 0.0, |_| Some(2.0)), Ok(vec![4.0]));
        assert!(matches!(
            function.evaluate(&[], 0.0, |_| Some(-1.0)),
            Err(ReactiveError::Callback(message)) if message == "negative input"
        ));
        assert_eq!(
            function.evaluate(&[], 0.0, |_| Some(0.0)),
            Err(ReactiveError::NonFinite)
        );
    }

    #[test]
    fn output_shape_is_validated() {
        let function = ReactiveFunction::new(1, 2, vec![], |_| Ok(vec![1.0]));
        assert_eq!(
            function.evaluate(&[0.0], 0.0, |_| None),
            Err(ReactiveError::OutputArity {
                expected: 2,
                actual: 1,
            })
        );
    }
}
