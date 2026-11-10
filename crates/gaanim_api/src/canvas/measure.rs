//! Measuring drawables while a scene is being authored.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use bevy::ecs::world::CommandQueue;
use bevy::prelude::{Commands, World};
use gaanim_math::Bounds3D;
use gaanim_timeline::snapshot::WorldSnapshot;
use gaanim_timeline::timeline::Timeline;

use super::authored::Authored;
use super::ops::{CanvasState, Op, Segment};
use super::profile::Measured;
use super::types::{LayoutOp, ObjectSpec, SpawnKind};
use super::{DrawableHandle, SceneModel};
use gaanim_core::ObjectId;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum BoundsError {
    #[error("the drawable belongs to another Scene")]
    ForeignScene,
    #[error("the drawable's Scene is no longer alive")]
    SceneDropped,
    #[error("the drawable has no geometry at the current cursor")]
    Empty,
}

impl SceneModel {
    /// Share this scene so its drawables can measure themselves with
    /// [`DrawableHandle::bounds`].
    pub fn into_shared(self) -> Arc<Mutex<SceneModel>> {
        let state = self.state.clone();
        let shared = Arc::new(Mutex::new(self));
        state.lock().expect("canvas state poisoned").owner = Some(Arc::downgrade(&shared));
        shared
    }

    /// Scene-space box of `handle` and its descendants at the authoring
    /// cursor, as they would render there: layout, transforms, text shaping,
    /// animations that ended by the cursor and setters applied at it all
    /// count, and so do the boxes' backgrounds. Geometry that reactive
    /// updaters rebuild every frame is measured as declared.
    ///
    /// A drawable declared since the scene last advanced (by `play`, `wait`
    /// and the like) that nothing else refers to yet has no animation, cut or
    /// group acting on it: its box depends only on its own declaration, those
    /// of its members and, inside a box, those of its box tree when that was
    /// declared since then too and placed without animation. It is measured
    /// by compiling just them. Otherwise this compiles the scene authored so
    /// far. Measurements with nothing authored in between share one
    /// compilation, and a later one resumes it from the segment where the
    /// scene changed, as an editor reload does. The next run of the script
    /// resumes the last one, so a reload recompiles to measure only from the
    /// segment being edited.
    pub fn bounds_of(&self, handle: &DrawableHandle) -> Result<Bounds3D, BoundsError> {
        if !self.owns(handle) {
            return Err(BoundsError::ForeignScene);
        }
        if let Some((isolated, closure)) = self.isolated_declaration(handle.id) {
            return self.measure(handle.id, MeasureScope::Isolated(closure), |previous| {
                // Free the previous world before compiling the next one.
                drop(previous);
                (isolated.compile_measure(Some(0.0)), Measured::Isolated)
            });
        }
        self.measured_bounds(handle.id)
    }

    /// Box of the object `id` in this scene compiled up to the authoring
    /// cursor.
    fn measured_bounds(&self, id: ObjectId) -> Result<Bounds3D, BoundsError> {
        let time = self.current_time();
        self.measure(id, MeasureScope::Cursor(time), |previous| {
            self.measure_at_cursor(previous)
        })
    }

    /// The scene authored so far, compiled into the world of `previous` or
    /// of the last run's measurements, resuming what that world compiled,
    /// and seeked to the end of what it compiled.
    fn measure_at_cursor(&self, previous: Option<CompiledMeasure>) -> (CompiledMeasure, Measured) {
        let mut world = previous
            .and_then(CompiledMeasure::into_resumable)
            .or_else(|| LAST_MEASURE_WORLD.lock().ok()?.take())
            .unwrap_or_else(measure_world);
        let kind = crate::runtime::replay_for_measuring(&mut world, self.clone(), |world| {
            crate::runtime::drop_elsewhere(std::mem::replace(world, measure_world()));
        });
        let how = match kind {
            crate::runtime::ReplayKind::Full => Measured::Scene,
            crate::runtime::ReplayKind::Incremental { .. } => Measured::Resumed,
            // Still seeked to the end it measured at.
            crate::runtime::ReplayKind::Unchanged { .. } => {
                return (CompiledMeasure::resumable(world), Measured::Reused);
            }
        };
        let Some(end) = world
            .get_resource::<crate::runtime::RetainedReplay>()
            .map(crate::runtime::RetainedReplay::end_time)
        else {
            // Nothing was retained to resume or to map ids with.
            return (self.compile_measure(None), Measured::Scene);
        };
        gaanim_timeline::capture_reload_keyframe(&mut world);
        if let Some(mut timeline) = world.remove_resource::<Timeline>() {
            timeline.seek(&mut world, end);
            world.insert_resource(timeline);
        }
        // Boxes and their backgrounds take their layout box every frame.
        gaanim_animation::updaters::resolve_layout_boxes(&mut world, end);
        (CompiledMeasure::resumable(world), how)
    }

    /// Box of the object `id` in the compilation `scope` names, compiled
    /// with `compile` unless the last one is still current. Measurements
    /// between two changes to the scene share one compilation: nothing it
    /// reads changed while the authored state's revision and the scene-wide
    /// settings stayed the same.
    fn measure(
        &self,
        id: ObjectId,
        scope: MeasureScope,
        compile: impl FnOnce(Option<CompiledMeasure>) -> (CompiledMeasure, Measured),
    ) -> Result<Bounds3D, BoundsError> {
        let started = std::time::Instant::now();
        // Busy when a callback measures from inside a measurement.
        let (Ok(mut cache), Some(scene_wide)) = (
            self.measured.0.try_lock(),
            self.scene_wide_fingerprint().finish(),
        ) else {
            let (mut compiled, how) = compile(None);
            let bounds = compiled.bounds(id);
            self.profile.measured(how, started.elapsed());
            return bounds;
        };
        let slot = match scope {
            MeasureScope::Cursor(_) => &mut cache.cursor,
            MeasureScope::Isolated(_) => &mut cache.isolated,
        };
        let key = MeasureKey {
            revision: self.state.revision(),
            scene_wide,
            scope,
        };
        let how = if slot.as_ref().is_none_or(|(current, _)| *current != key) {
            let previous = slot.take().map(|(_, compiled)| compiled);
            let (compiled, how) = compile(previous);
            // Compiling records diagnostics in the authored state, so the
            // revision it compiled is the one after it.
            let key = MeasureKey {
                revision: self.state.revision(),
                ..key
            };
            *slot = Some((key, compiled));
            how
        } else {
            Measured::Reused
        };
        let (_, compiled) = slot.as_mut().expect("measurement compiled above");
        let bounds = compiled.bounds(id);
        drop(cache);
        self.profile.measured(how, started.elapsed());
        bounds
    }

    /// Box of the object `id` in this scene compiled up to the authoring
    /// cursor, without reusing a compilation.
    #[cfg(test)]
    fn compiled_bounds(&self, id: ObjectId) -> Result<Bounds3D, BoundsError> {
        self.compile_measure(None).bounds(id)
    }

    /// The scene compiled up to the authoring cursor and seeked to `time`,
    /// or to that cursor. The cursor is taken on the compiled clock: it sums
    /// the durations in another order than [`Self::current_time`], so the two
    /// can differ by a few ULPs, and what starts at the cursor, such as a cut
    /// made by a setter, starts exactly at the compiled one.
    fn compile_measure(&self, time: Option<f64>) -> CompiledMeasure {
        let mut fonts = gaanim_text::font::FontRegistry::without_system_fonts();
        self.register_theme_fonts(&mut fonts);
        let config = self.themed_text_config();
        let mut world = World::new();
        let mut queue = CommandQueue::default();
        let mut timeline = Timeline::new();
        let compiled = {
            let mut commands = Commands::new(&mut queue, &world);
            self.compile_resumable(
                &mut commands,
                &mut timeline,
                &fonts,
                &config,
                None,
                None,
                Vec::new(),
            )
        };
        let time = time.unwrap_or(compiled.end_time);
        queue.apply(&mut world);
        timeline.add_keyframe(0.0, WorldSnapshot::capture(&mut world));
        timeline.seek(&mut world, time);
        // Boxes and their backgrounds take their layout box every frame.
        gaanim_animation::updaters::resolve_layout_boxes(&mut world, time);
        CompiledMeasure {
            ids: Some(compiled.ids),
            world,
            #[cfg(test)]
            generation: next_generation(),
        }
    }
}

/// The world the last run of the script measured its scene in, which the
/// next run's first measurement resumes: a reload recompiles the same
/// segments up to the one being edited.
static LAST_MEASURE_WORLD: Mutex<Option<World>> = Mutex::new(None);

/// An empty world to compile a scene into for measuring it.
fn measure_world() -> World {
    let mut world = World::new();
    world.insert_resource(Timeline::new());
    world.insert_resource(gaanim_text::font::FontRegistry::without_system_fonts());
    world.insert_resource(gaanim_text::prelude::TextConfig::default());
    world
}

/// A number for each compilation measurements make, telling them apart.
#[cfg(test)]
fn next_generation() -> u64 {
    static GENERATION: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    GENERATION.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}

/// What a measurement compiled.
#[derive(Debug, Clone, PartialEq)]
enum MeasureScope {
    /// The scene up to the authoring cursor at this time.
    Cursor(f64),
    /// These declarations on their own; see [`SceneModel::bounds_of`].
    Isolated(Vec<ObjectId>),
}

/// The inputs a compiled measurement is current for.
#[derive(Debug, Clone, PartialEq)]
struct MeasureKey {
    /// Revision of the authored state.
    revision: u64,
    /// Fingerprint of the scene-wide settings, which live outside that state.
    scene_wide: u64,
    scope: MeasureScope,
}

/// A scene compiled to measure its drawables.
pub(crate) struct CompiledMeasure {
    /// The compiled object of each authored one; `None` when the world's
    /// retained replay holds them, and a later measurement can resume it.
    ids: Option<HashMap<ObjectId, ObjectId>>,
    world: World,
    /// Which compilation this is.
    #[cfg(test)]
    generation: u64,
}

impl CompiledMeasure {
    /// A measurement in `world`, which a replay for measuring compiled.
    fn resumable(world: World) -> Self {
        Self {
            ids: None,
            world,
            #[cfg(test)]
            generation: next_generation(),
        }
    }

    /// The world, when a later replay for measuring can resume it.
    fn into_resumable(self) -> Option<World> {
        self.ids.is_none().then_some(self.world)
    }

    /// Box of the object `id` in this compilation.
    fn bounds(&mut self, id: ObjectId) -> Result<Bounds3D, BoundsError> {
        let runtime = match &self.ids {
            Some(ids) => ids.get(&id).copied(),
            None => self
                .world
                .get_resource::<crate::runtime::RetainedReplay>()
                .and_then(|replay| replay.runtime_id(id)),
        }
        .ok_or(BoundsError::Empty)?;
        let world = &mut self.world;
        let entity = world
            .query::<(bevy::prelude::Entity, &gaanim_scene::MobjectId)>()
            .iter(world)
            .find_map(|(entity, id)| (id.0 == runtime).then_some(entity))
            .ok_or(BoundsError::Empty)?;
        let bounds = gaanim_animation::updaters::resolve_entity_bounds(entity, world)
            .ok_or(BoundsError::Empty)?;
        if !(bounds.min.is_finite() && bounds.max.is_finite()) || bounds.min.x > bounds.max.x {
            return Err(BoundsError::Empty);
        }
        Ok(bounds)
    }
}

/// The compilations a scene's measurements share while the scene stays
/// unchanged. Copies of a scene share them, since drawables measure in a
/// copy of their scene.
#[derive(Clone, Default)]
pub(crate) struct MeasureCache(Arc<Mutex<Measurements>>);

#[derive(Default)]
struct Measurements {
    /// The scene compiled up to the authoring cursor.
    cursor: Option<(MeasureKey, CompiledMeasure)>,
    /// The last declarations measured on their own, such as the members of
    /// one box tree.
    isolated: Option<(MeasureKey, CompiledMeasure)>,
}

impl MeasureCache {
    /// Forget the compilations, e.g. once authoring is over. The next run
    /// of the script resumes the one at the cursor.
    pub(crate) fn clear(&self) {
        let Ok(mut measurements) = self.0.lock() else {
            return;
        };
        let cursor = std::mem::take(&mut *measurements).cursor;
        drop(measurements);
        if let Some(world) = cursor.and_then(|(_, compiled)| compiled.into_resumable())
            && let Ok(mut last) = LAST_MEASURE_WORLD.lock()
        {
            *last = Some(world);
        }
    }
}

impl std::fmt::Debug for MeasureCache {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("MeasureCache")
    }
}

impl SceneModel {
    /// This scene reduced to the declarations of the object `id` and its
    /// members, when they are all that decides its box, and those objects
    /// in order; see [`SceneModel::bounds_of`].
    fn isolated_declaration(&self, id: ObjectId) -> Option<(SceneModel, Vec<ObjectId>)> {
        let state = self.state.lock().expect("canvas state poisoned");
        let closure = independent_closure(&state, id)?;
        let segment = state.segments.get(state.active_idx)?;
        let spawned = |op: &Op| match op {
            Op::Spawn(spec) => Some(spec.lock().expect("object spec poisoned").id),
            _ => None,
        };
        let first = segment
            .ops
            .iter()
            .position(|op| spawned(op).is_some_and(|spawned| closure.contains(&spawned)))?;
        let mut ops = Vec::new();
        let mut spawned_closure = 0;
        for op in &segment.ops[first..] {
            match op {
                Op::Spawn(spec) => {
                    let spec = spec.lock().expect("object spec poisoned");
                    if closure.contains(&spec.id) {
                        ops.push(op.clone());
                        spawned_closure += 1;
                    } else if spawn_members(&spec.kind)?
                        .iter()
                        .any(|member| closure.contains(member))
                    {
                        // A later group gathers the closure and may move it.
                        return None;
                    }
                }
                // Boxes of the closure placed at once; an animated reflow
                // moves them over time.
                Op::LayoutTransition { to, duration, .. } => {
                    let ours = closure.contains(&to.container);
                    if !ours && !to.members.iter().any(|member| closure.contains(&member.id)) {
                        continue;
                    }
                    if !ours || duration.is_some() {
                        return None;
                    }
                    ops.push(op.clone());
                }
                Op::AttachLayoutBackground {
                    target, container, ..
                } => match (closure.contains(target), closure.contains(container)) {
                    (true, true) => ops.push(op.clone()),
                    (false, false) => {}
                    _ => return None,
                },
                // Zones only compute rectangles.
                Op::RecordLayoutZones { .. } => {}
                // An animation, cut, grouping, constraint or binding may act
                // on the closure.
                _ => return None,
            }
        }
        if spawned_closure != closure.len() {
            return None;
        }

        let mut isolated_state = CanvasState::new();
        isolated_state.next_id = state.next_id;
        isolated_state.all_drawables = closure.iter().copied().collect();
        isolated_state.object_specs = closure
            .iter()
            .filter_map(|id| Some((*id, state.object_specs.get(id)?.clone())))
            .collect();
        isolated_state.segments = vec![Segment {
            ops,
            mobject_ids: closure.iter().copied().collect(),
            ..Segment::implicit()
        }];
        drop(state);
        let mut isolated = self.clone();
        isolated.state = Arc::new(Authored::new(isolated_state));
        isolated.measured = MeasureCache::default();
        let mut closure: Vec<_> = closure.into_iter().collect();
        closure.sort_unstable();
        Some((isolated, closure))
    }
}

/// The object `id` and every object its box depends on (group members, SVG
/// roots, layout references, the whole box tree it belongs to), when all of
/// them are declared but not yet frozen and none depends on anything else.
/// `None` otherwise.
fn independent_closure(
    state: &CanvasState,
    id: ObjectId,
) -> Option<std::collections::HashSet<ObjectId>> {
    let mut closure = std::collections::HashSet::new();
    let mut pending = vec![id];
    while let Some(id) = pending.pop() {
        if !closure.insert(id) {
            continue;
        }
        // Frozen objects may carry cuts and animations.
        if state.frozen_spawn_specs.contains_key(&id) {
            return None;
        }
        let spec = state
            .object_specs
            .get(&id)?
            .lock()
            .expect("object spec poisoned");
        if !self_contained(&spec) {
            return None;
        }
        pending.extend_from_slice(spawn_members(&spec.kind)?);
        pending.extend(spec.svg_owner);
        // A box places its members and is placed by the box that holds it,
        // so every box of a tree depends on the whole tree.
        pending.extend(spec.layout_owner);
        pending.extend(spec.layout_background);
        if let Some(snapshot) = state.latest_layouts.get(&id) {
            pending.extend(snapshot.members.iter().map(|member| member.id));
        }
        for op in &spec.layout_ops {
            match op {
                LayoutOp::NextTo { reference, .. } | LayoutOp::AlignTo { reference, .. } => {
                    pending.push(*reference);
                }
                LayoutOp::MoveToAnchorPoint { .. } => return None,
                _ => {}
            }
        }
    }
    Some(closure)
}

/// Whether nothing outside the declaration of `spec` and its box tree shapes
/// it: no coordinate view, HUD or parallax placement, path to follow or animation
/// state.
fn self_contained(spec: &ObjectSpec) -> bool {
    !spec.hud
        && spec.parallax_depth.is_none()
        && spec.text_path.is_none()
        && spec.extrusion.is_none()
        && !spec.defer_visibility_until_play
        && spec.coordinate_view_role.is_none()
        && spec.coordinate_label_offset.is_none()
        && spec.axis_label_gap.is_none()
        && spec.axis_box_part.is_none()
        && spec.coordinate_tick_level.is_none()
        && spec.coordinate_view_cursor.is_none()
        && spec.reactive_readout_layout.is_none()
        && spec.typed_text.is_none()
        && spec.media_frame.is_none()
        && spec.material_animation_cursor.is_none()
        && spec.fill_level_cursor.is_none()
        && !spec.manual_position_animation
}

/// Objects a spawned kind draws from: its members for a group, none for
/// geometry defined by its own parameters, `None` for every other kind.
fn spawn_members(kind: &SpawnKind) -> Option<&[ObjectId]> {
    match kind {
        SpawnKind::Group(members) | SpawnKind::GroupNoCenter(members) => Some(members),
        SpawnKind::Circle(..)
        | SpawnKind::Rect(..)
        | SpawnKind::RoundedRect(..)
        | SpawnKind::Square(..)
        | SpawnKind::Dot(..)
        | SpawnKind::Ellipse(..)
        | SpawnKind::Line(..)
        | SpawnKind::Arrow(..)
        | SpawnKind::SizedArrow { .. }
        | SpawnKind::DashedLine { .. }
        | SpawnKind::DoubleArrow { .. }
        | SpawnKind::Polygon(..)
        | SpawnKind::Metaballs { .. }
        | SpawnKind::Points { .. }
        | SpawnKind::Star { .. }
        | SpawnKind::RegularPolygon { .. }
        | SpawnKind::Sector { .. }
        | SpawnKind::Annulus { .. }
        | SpawnKind::Brace { .. }
        | SpawnKind::Checkmark(..)
        | SpawnKind::Cross(..)
        | SpawnKind::RightAngle(..)
        | SpawnKind::Arc { .. }
        | SpawnKind::CurvedArrow { .. }
        | SpawnKind::CurvedArrowArc { .. }
        | SpawnKind::Dimension { .. }
        | SpawnKind::Polyline { .. }
        | SpawnKind::ReactivePolyline { .. }
        | SpawnKind::Bezier { .. }
        | SpawnKind::Curve(..)
        | SpawnKind::Text(..)
        | SpawnKind::Typst { .. }
        | SpawnKind::Image { .. }
        | SpawnKind::SvgPath(..) => Some(&[]),
        _ => None,
    }
}

impl super::Anim {
    /// Scene-space box of the animated drawable at the authoring cursor,
    /// before this animation runs.
    pub fn target_bounds(&self) -> Result<Bounds3D, BoundsError> {
        let owner = self
            .owner
            .as_ref()
            .ok_or(BoundsError::SceneDropped)?
            .lock()
            .expect("canvas state poisoned")
            .owner
            .clone()
            .ok_or(BoundsError::SceneDropped)?;
        let scene = owner.upgrade().ok_or(BoundsError::SceneDropped)?;
        let scene = scene.lock().expect("scene poisoned").clone();
        scene.measured_bounds(self.inner.target)
    }
}

impl DrawableHandle {
    /// The shared scene this drawable belongs to, while it exists.
    pub fn scene(&self) -> Option<Arc<Mutex<SceneModel>>> {
        self.state
            .lock()
            .expect("canvas state poisoned")
            .owner
            .clone()
            .and_then(|owner| owner.upgrade())
    }

    /// Scene-space box of this drawable at its scene's authoring cursor; see
    /// [`SceneModel::bounds_of`]. The scene must have been shared with
    /// [`SceneModel::into_shared`].
    pub fn bounds(&self) -> Result<Bounds3D, BoundsError> {
        let owner = self
            .state
            .lock()
            .expect("canvas state poisoned")
            .owner
            .clone()
            .ok_or(BoundsError::SceneDropped)?;
        let scene = owner.upgrade().ok_or(BoundsError::SceneDropped)?;
        let scene = scene.lock().expect("scene poisoned").clone();
        scene.bounds_of(self)
    }
}

/// Why [`scatter`] could not lay its drawables out.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum ScatterLayoutError {
    #[error("item {index} cannot be measured: {source}")]
    Item { index: usize, source: BoundsError },
    #[error("avoided shape {index} cannot be measured: {source}")]
    Avoid { index: usize, source: BoundsError },
    #[error(transparent)]
    Layout(#[from] gaanim_layout::ScatterError),
}

/// Move `items` to seeded positions inside `region` where none overlaps
/// another or the box of an `avoid` drawable or rectangle, with at least
/// `gap` between them. The same items, region and seed give the same
/// layout. Nothing moves unless every item fits.
pub fn scatter(
    items: &[DrawableHandle],
    region: Bounds3D,
    avoid: &[Bounds3D],
    gap: f64,
    seed: u64,
) -> Result<(), ScatterLayoutError> {
    let boxes = items
        .iter()
        .enumerate()
        .map(|(index, item)| {
            item.bounds()
                .map_err(|source| ScatterLayoutError::Item { index, source })
        })
        .collect::<Result<Vec<_>, _>>()?;
    let sizes: Vec<_> = boxes
        .iter()
        .map(|bounds| gaanim_core::glam::DVec2::new(bounds.width(), bounds.height()))
        .collect();
    let centers = gaanim_layout::scatter(&sizes, region, avoid, gap, seed)?;
    for (item, center) in items.iter().zip(centers) {
        item.clone()
            .at_anchor(center.x, center.y, gaanim_layout::Anchor::Center);
    }
    Ok(())
}

/// The boxes of `drawables`, for the `avoid` list of [`scatter`].
pub fn avoid_boxes(drawables: &[DrawableHandle]) -> Result<Vec<Bounds3D>, ScatterLayoutError> {
    drawables
        .iter()
        .enumerate()
        .map(|(index, drawable)| {
            drawable
                .bounds()
                .map_err(|source| ScatterLayoutError::Avoid { index, source })
        })
        .collect()
}

#[cfg(test)]
mod tests;
