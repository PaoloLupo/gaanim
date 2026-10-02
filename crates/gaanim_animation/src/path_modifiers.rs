//! Non-destructive path modifiers (zig zag, round corners, pucker & bloat,
//! twist, wiggle and offset) applied to a drawable's path right before it
//! is extracted, and undone at the start of the next frame, so seeks,
//! tweens and snapshots keep reading the authored path.

use std::sync::Arc;

use bevy::prelude::*;
use gaanim_core::kurbo::{BezPath, Shape};
use gaanim_math::{Bounds3D, path_modifiers};
use gaanim_objects::offset::{OffsetJoin, offset_copies};
use gaanim_scene::{LocalBounds, Path2D, PathSource};

use crate::signals::FloatSignal;

/// A modifier parameter: a fixed number, or the signal of a `Parameter`
/// that animations drive.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ModifierParam {
    Fixed(f64),
    Signal(Entity),
}

/// One operator of a stack and its fixed settings.
#[derive(Debug, Clone, PartialEq)]
pub enum ModifierKind {
    /// Parameters: size.
    ZigZag { ridges: u32, smooth: bool },
    /// Parameters: radius.
    RoundCorners,
    /// Parameters: amount.
    PuckerBloat,
    /// Parameters: angle in radians.
    Twist,
    /// Parameters: size, frequency.
    Wiggle { detail: u32, seed: u64 },
    /// Parameters: amount.
    Offset { join: OffsetJoin, copies: u32 },
}

#[derive(Debug, Clone, PartialEq)]
pub struct PathModifier {
    pub kind: ModifierKind,
    pub params: Vec<ModifierParam>,
    /// Scene time the modifier was declared at: it applies from then on,
    /// and a wiggle's time starts there.
    pub from: f64,
}

impl PathModifier {
    /// `path` modified with parameter `values`, `time` seconds into the scene.
    pub fn apply(&self, path: &BezPath, values: &[f64], time: f64) -> BezPath {
        let value = |index: usize| values.get(index).copied().unwrap_or(0.0);
        match self.kind {
            ModifierKind::ZigZag { ridges, smooth } => {
                path_modifiers::zigzag(path, value(0), ridges, smooth)
            }
            ModifierKind::RoundCorners => path_modifiers::round_corners(path, value(0)),
            ModifierKind::PuckerBloat => path_modifiers::pucker_bloat(path, value(0)),
            ModifierKind::Twist => path_modifiers::twist(path, value(0)),
            ModifierKind::Wiggle { detail, seed } => path_modifiers::wiggle(
                path,
                value(0),
                detail,
                value(1),
                seed,
                (time - self.from).max(0.0),
            ),
            ModifierKind::Offset { join, copies } => offset_copies(path, value(0), join, copies),
        }
    }

    fn live(&self) -> bool {
        matches!(self.kind, ModifierKind::Wiggle { .. })
    }
}

/// Inputs and outputs of the last modification.
#[derive(Debug, Clone)]
struct ModifiedCache {
    source: Arc<BezPath>,
    path: Arc<BezPath>,
    key: Vec<u64>,
    modified_source: Arc<BezPath>,
    modified_path: Arc<BezPath>,
    bounds: Option<Bounds3D>,
}

/// What the apply system replaced, restored in `First`.
#[derive(Debug, Clone)]
struct Applied {
    source: Option<Arc<BezPath>>,
    path: Arc<BezPath>,
    bounds: Option<Bounds3D>,
    modified_source: Option<Arc<BezPath>>,
    modified_path: Arc<BezPath>,
}

/// Component: a stack of path modifiers, applied in order.
#[derive(Component, Debug, Clone, Default)]
pub struct PathModifiers {
    pub stack: Vec<PathModifier>,
    cache: Option<ModifiedCache>,
    applied: Option<Applied>,
}

impl PathModifiers {
    pub fn new(stack: Vec<PathModifier>) -> Self {
        Self {
            stack,
            cache: None,
            applied: None,
        }
    }

    /// `path` through every modifier active at `time`, with `value` reading
    /// signal parameters.
    pub fn modify(
        &self,
        path: &BezPath,
        time: f64,
        value: impl Fn(ModifierParam) -> f64,
    ) -> BezPath {
        let mut path = path.clone();
        for modifier in self.active(time) {
            let values: Vec<f64> = modifier.params.iter().map(|param| value(*param)).collect();
            path = modifier.apply(&path, &values, time);
        }
        path
    }

    fn active(&self, time: f64) -> impl Iterator<Item = &PathModifier> {
        self.stack
            .iter()
            .filter(move |modifier| time + 1e-9 >= modifier.from)
    }
}

fn param_value(param: ModifierParam, signals: &Query<&FloatSignal>) -> f64 {
    match param {
        ModifierParam::Fixed(value) => value,
        ModifierParam::Signal(entity) => signals.get(entity).map_or(0.0, |signal| signal.value),
    }
}

/// Modifies paths right before extraction.
#[allow(clippy::type_complexity)]
pub fn apply_path_modifiers_system(
    playback: Option<Res<crate::updaters::PlaybackState>>,
    mut paths: Query<(
        &mut PathModifiers,
        &mut Path2D,
        Option<&mut PathSource>,
        Option<&mut LocalBounds>,
    )>,
    signals: Query<&FloatSignal>,
) {
    let time = playback.map_or(0.0, |state| state.current_time);
    for (mut modifiers, mut path, source, bounds) in &mut paths {
        if modifiers.active(time).next().is_none() {
            continue;
        }
        let authored_source = source.as_ref().map(|source| source.0.clone());
        let authored_path = path.0.clone();
        let base = authored_source
            .clone()
            .unwrap_or_else(|| authored_path.clone());
        // Parameter values (and the time, for a live wiggle) decide the result.
        let mut key = Vec::new();
        for modifier in modifiers.active(time) {
            key.push(modifier.stack_key());
            for param in &modifier.params {
                key.push(param_value(*param, &signals).to_bits());
            }
            if modifier.live() {
                key.push(time.to_bits());
            }
        }
        let cached = modifiers.cache.as_ref().filter(|cache| {
            Arc::ptr_eq(&cache.source, &base)
                && Arc::ptr_eq(&cache.path, &authored_path)
                && cache.key == key
        });
        let cache = match cached {
            Some(cache) => cache.clone(),
            None => {
                let modify = |path: &BezPath| {
                    modifiers.modify(path, time, |param| param_value(param, &signals))
                };
                let modified_source = Arc::new(modify(&base));
                // An untrimmed path is its source: they stay one path, so the
                // renderer still fills it.
                let modified_path = if Arc::ptr_eq(&authored_path, &base) || *authored_path == *base
                {
                    modified_source.clone()
                } else {
                    Arc::new(modify(&authored_path))
                };
                let box_ = modified_source.bounding_box();
                let cache = ModifiedCache {
                    source: base.clone(),
                    path: authored_path.clone(),
                    key,
                    modified_source,
                    modified_path,
                    bounds: (box_.width().is_finite() && !modified_source_is_empty(&box_))
                        .then(|| Bounds3D::new_2d(box_.x0, box_.y0, box_.x1, box_.y1)),
                };
                modifiers.cache = Some(cache.clone());
                cache
            }
        };
        path.0 = cache.modified_path.clone();
        let mut previous_bounds = None;
        if let Some(mut bounds) = bounds
            && let Some(modified) = cache.bounds
        {
            previous_bounds = Some(bounds.0);
            bounds.0 = modified;
        }
        let modified_source = source.map(|mut source| {
            source.0 = cache.modified_source.clone();
            cache.modified_source.clone()
        });
        modifiers.applied = Some(Applied {
            source: authored_source,
            path: authored_path,
            bounds: previous_bounds,
            modified_source,
            modified_path: cache.modified_path.clone(),
        });
    }
}

fn modified_source_is_empty(rect: &gaanim_core::kurbo::Rect) -> bool {
    !(rect.x0.is_finite() && rect.y0.is_finite() && rect.x1.is_finite() && rect.y1.is_finite())
}

impl PathModifier {
    /// Distinguishes the modifiers of a stack in the cache key.
    fn stack_key(&self) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        format!("{:?}", self.kind).hash(&mut hasher);
        self.from.to_bits().hash(&mut hasher);
        hasher.finish()
    }
}

/// Restores the authored paths at the start of the next frame, unless
/// something rewrote them in between.
pub fn restore_path_modifiers_system(
    mut paths: Query<(
        &mut PathModifiers,
        &mut Path2D,
        Option<&mut PathSource>,
        Option<&mut LocalBounds>,
    )>,
) {
    for (mut modifiers, mut path, source, bounds) in &mut paths {
        let Some(applied) = modifiers.applied.take() else {
            continue;
        };
        if Arc::ptr_eq(&path.0, &applied.modified_path) {
            path.0 = applied.path;
        }
        if let (Some(mut source), Some(authored), Some(modified)) =
            (source, applied.source, applied.modified_source)
            && Arc::ptr_eq(&source.0, &modified)
        {
            source.0 = authored;
        }
        if let (Some(mut bounds), Some(authored)) = (bounds, applied.bounds) {
            bounds.0 = authored;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::ecs::system::RunSystemOnce;
    use gaanim_core::kurbo;

    fn square() -> BezPath {
        kurbo::Rect::new(-1.0, -1.0, 1.0, 1.0).to_path(1e-3)
    }

    fn world() -> (World, Entity, Entity) {
        let mut world = World::new();
        world.insert_resource(crate::updaters::PlaybackState::default());
        let signal = world.spawn(FloatSignal::new(0.25)).id();
        let path = Arc::new(square());
        let entity = world
            .spawn((
                Path2D(path.clone()),
                PathSource(path),
                LocalBounds(Bounds3D::new_2d(-1.0, -1.0, 1.0, 1.0)),
                PathModifiers::new(vec![PathModifier {
                    kind: ModifierKind::Offset {
                        join: OffsetJoin::Miter,
                        copies: 1,
                    },
                    params: vec![ModifierParam::Signal(signal)],
                    from: 0.0,
                }]),
            ))
            .id();
        (world, entity, signal)
    }

    #[test]
    fn modifiers_apply_for_extraction_and_restore_the_authored_path() {
        let (mut world, entity, signal) = world();
        world.run_system_once(apply_path_modifiers_system).unwrap();
        let bounds = world.get::<LocalBounds>(entity).unwrap().0;
        assert!((bounds.max.x - 1.25).abs() < 1e-3, "{bounds:?}");
        let path = world.get::<Path2D>(entity).unwrap().0.clone();
        assert!(Arc::ptr_eq(
            &path,
            &world.get::<PathSource>(entity).unwrap().0
        ));
        world
            .run_system_once(restore_path_modifiers_system)
            .unwrap();
        assert_eq!(*world.get::<Path2D>(entity).unwrap().0, square());
        assert_eq!(*world.get::<PathSource>(entity).unwrap().0, square());
        assert_eq!(world.get::<LocalBounds>(entity).unwrap().0.max.x, 1.0);

        // The parameter's signal drives the next frame.
        world.get_mut::<FloatSignal>(signal).unwrap().value = 0.5;
        world.run_system_once(apply_path_modifiers_system).unwrap();
        let bounds = world.get::<LocalBounds>(entity).unwrap().0;
        assert!((bounds.max.x - 1.5).abs() < 1e-3, "{bounds:?}");
    }

    #[test]
    fn a_modifier_waits_for_the_time_it_was_declared() {
        let (mut world, entity, _) = world();
        world.get_mut::<PathModifiers>(entity).unwrap().stack[0].from = 2.0;
        world.run_system_once(apply_path_modifiers_system).unwrap();
        assert_eq!(*world.get::<Path2D>(entity).unwrap().0, square());
        world
            .resource_mut::<crate::updaters::PlaybackState>()
            .current_time = 2.0;
        world.run_system_once(apply_path_modifiers_system).unwrap();
        assert_ne!(*world.get::<Path2D>(entity).unwrap().0, square());
    }
}
