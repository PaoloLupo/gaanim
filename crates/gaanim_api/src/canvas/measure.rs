//! Measuring drawables while a scene is being authored.

use std::sync::{Arc, Mutex};

use bevy::ecs::world::CommandQueue;
use bevy::prelude::{Commands, World};
use gaanim_math::Bounds3D;
use gaanim_timeline::snapshot::WorldSnapshot;
use gaanim_timeline::timeline::Timeline;

use super::authored::Authored;
use super::compile::CompileCursor;
use super::ops::{CanvasState, Op, Segment};
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
    /// cursor, as they would render there: layout, transforms, text shaping
    /// and animations that ended before the cursor all count, and so do the
    /// boxes' backgrounds. Geometry that reactive updaters rebuild every
    /// frame is measured as declared.
    ///
    /// A drawable declared since the scene last advanced (by `play`, `wait`
    /// and the like) that nothing else refers to yet has no animation, cut or
    /// group acting on it: its box depends only on its own declaration, those
    /// of its members and, inside a box, those of its box tree when that was
    /// declared since then too and placed without animation. It is measured
    /// by compiling just them. Otherwise this compiles the scene authored so
    /// far. Measurements with nothing authored in between share one
    /// compilation.
    pub fn bounds_of(&self, handle: &DrawableHandle) -> Result<Bounds3D, BoundsError> {
        if !self.owns(handle) {
            return Err(BoundsError::ForeignScene);
        }
        if let Some((isolated, closure)) = self.isolated_declaration(handle.id) {
            return self.measure(handle.id, MeasureScope::Isolated(closure), || {
                isolated.compile_measure(0.0)
            });
        }
        self.measured_bounds(handle.id)
    }

    /// Box of the object `id` in this scene compiled up to the authoring
    /// cursor.
    fn measured_bounds(&self, id: ObjectId) -> Result<Bounds3D, BoundsError> {
        let time = self.current_time();
        self.measure(id, MeasureScope::Cursor(time), || {
            self.compile_measure(time)
        })
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
        compile: impl FnOnce() -> CompiledMeasure,
    ) -> Result<Bounds3D, BoundsError> {
        // Busy when a callback measures from inside a measurement.
        let (Ok(mut cache), Some(scene_wide)) = (
            self.measured.0.try_lock(),
            self.scene_wide_fingerprint().finish(),
        ) else {
            return compile().bounds(id);
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
        if slot.as_ref().is_none_or(|(current, _)| *current != key) {
            // Free the previous world before compiling the next one.
            *slot = None;
            let compiled = compile();
            // Compiling records diagnostics in the authored state, so the
            // revision it compiled is the one after it.
            let key = MeasureKey {
                revision: self.state.revision(),
                ..key
            };
            *slot = Some((key, compiled));
        }
        let (_, compiled) = slot.as_mut().expect("measurement compiled above");
        compiled.bounds(id)
    }

    /// Box of the object `id` in this scene compiled up to the authoring
    /// cursor and seeked to `time`.
    #[cfg(test)]
    fn compiled_bounds(&self, id: ObjectId, time: f64) -> Result<Bounds3D, BoundsError> {
        self.compile_measure(time).bounds(id)
    }

    /// The scene compiled up to the authoring cursor and seeked to `time`.
    fn compile_measure(&self, time: f64) -> CompiledMeasure {
        let segments = self
            .state
            .lock()
            .expect("canvas state poisoned")
            .segments
            .len();

        let mut fonts = gaanim_text::font::FontRegistry::without_system_fonts();
        self.register_theme_fonts(&mut fonts);
        let config = self.themed_text_config();
        let mut world = World::new();
        let mut queue = CommandQueue::default();
        let mut timeline = Timeline::new();
        let checkpoint = {
            let mut commands = Commands::new(&mut queue, &world);
            self.compile_resumable(
                &mut commands,
                &mut timeline,
                &fonts,
                &config,
                None,
                Some(segments),
                Vec::new(),
            )
        };
        queue.apply(&mut world);
        timeline.add_keyframe(0.0, WorldSnapshot::capture(&mut world));
        timeline.seek(&mut world, time);
        // Boxes and their backgrounds take their layout box every frame.
        gaanim_animation::updaters::resolve_layout_boxes(&mut world, time);
        CompiledMeasure {
            cursor: checkpoint.map(|checkpoint| checkpoint.cursor),
            world,
        }
    }
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
    cursor: Option<CompileCursor>,
    world: World,
}

impl CompiledMeasure {
    /// Box of the object `id` in this compilation.
    fn bounds(&mut self, id: ObjectId) -> Result<Bounds3D, BoundsError> {
        let runtime = self
            .cursor
            .as_ref()
            .and_then(|cursor| cursor.runtime_id(id))
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
    /// Forget the compilations, e.g. once authoring is over.
    pub(crate) fn clear(&self) {
        if let Ok(mut measurements) = self.0.lock() {
            *measurements = Measurements::default();
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
/// it: no coordinate view, HUD placement or animation state.
fn self_contained(spec: &ObjectSpec) -> bool {
    !spec.hud
        && !spec.defer_visibility_until_play
        && spec.coordinate_view_role.is_none()
        && spec.coordinate_label_offset.is_none()
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
        | SpawnKind::Polyline(..)
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
mod tests {
    use super::*;
    use crate::canvas::{Anchor, Direction};
    use gaanim_core::peniko::Color;

    fn assert_same(scene: &SceneModel, handle: &DrawableHandle) {
        assert!(
            scene.isolated_declaration(handle.id).is_some(),
            "the fast path applies"
        );
        let fast = scene.bounds_of(handle).unwrap();
        let full = scene
            .compiled_bounds(handle.id, scene.current_time())
            .unwrap();
        for (a, b) in [
            (fast.min.x, full.min.x),
            (fast.min.y, full.min.y),
            (fast.max.x, full.max.x),
            (fast.max.y, full.max.y),
        ] {
            assert!((a - b).abs() < 1e-9, "{fast:?} != {full:?}");
        }
    }

    /// A scene with earlier, already animated content.
    fn busy_scene() -> SceneModel {
        let mut scene = SceneModel::new(16.0, 9.0);
        let title = scene.text("Título").move_to(0.0, 3.0);
        let dot = scene.circle(0.4).fill(Color::WHITE);
        scene.play(vec![
            title.animate().write().duration(0.5),
            dot.animate().shift_by(2.0, 0.0).duration(0.5),
        ]);
        scene.segment("dos", None).unwrap();
        let other = scene.rect(1.0, 1.0).move_to(-3.0, 0.0);
        scene.play(vec![other.animate().rotate_by(0.5).duration(0.3)]);
        scene
    }

    #[test]
    fn a_fresh_declaration_measures_like_the_compiled_scene() {
        let mut scene = busy_scene();
        let label = scene
            .text("Datos de diseño")
            .at_anchor(-6.0, -2.7, Anchor::Left)
            .scale_by(1.3)
            .rotate_by(0.2);
        assert_same(&scene, &label);

        // A pill: a text on a rounded rect, gathered in a group.
        let word = scene.text("Carga vertical").move_to(1.0, -1.0);
        let pill = scene
            .rounded_rect(3.0, 0.6, 0.3)
            .stroke(Color::WHITE, 0.02)
            .move_to(1.0, -1.0);
        let group = scene.group(&[&pill, &word]).move_to(2.0, -2.0);
        assert_same(&scene, &group);

        // Placed against another fresh declaration.
        let next = scene
            .text("Sismo moderado")
            .next_to(&label, Direction::Right, 0.18);
        assert_same(&scene, &next);
    }

    /// A decorated column holding a text and a sized spacer, inside a row
    /// on the safe area beside another box, like a slide's cards.
    fn card_row(scene: &mut SceneModel) -> (DrawableHandle, DrawableHandle, DrawableHandle) {
        use crate::canvas::{LayoutMemberSpec, LayoutSpec, LayoutWithin};
        use gaanim_core::glam::DVec2;
        use gaanim_layout::{Align, Insets, LayoutNodeKind, LayoutStyle, SizeRule};

        fn place(
            scene: &mut SceneModel,
            members: &[&DrawableHandle],
            decorated: bool,
            item: gaanim_layout::LayoutItemStyle,
            spec: LayoutSpec,
        ) -> DrawableHandle {
            let root = scene.group(members);
            for member in members {
                member.claim_layout(&root).unwrap();
            }
            if decorated {
                scene
                    .decorate_layout(&root, Some(Color::WHITE.into()), None, 0.0)
                    .unwrap();
            }
            root.set_layout_item(item);
            let snapshots = members
                .iter()
                .map(|member| LayoutMemberSpec {
                    id: member.id,
                    style: member.layout_item(),
                })
                .collect();
            scene.reflow_layout(&root, snapshots, spec, 1, None, None, None);
            root
        }

        let label = scene.text("Densidad de muros");
        let slot = scene.rect(0.1, 0.1).no_fill().no_stroke();
        slot.set_layout_item(gaanim_layout::LayoutItemStyle {
            shrink: 0.0,
            ..Default::default()
        });
        let card = place(
            scene,
            &[&slot, &label],
            true,
            gaanim_layout::LayoutItemStyle {
                grow: 1.0,
                ..Default::default()
            },
            LayoutSpec {
                kind: LayoutNodeKind::Column { wrap: false },
                style: LayoutStyle {
                    padding: Insets::all(0.25),
                    gap: DVec2::splat(0.15),
                    height: SizeRule::Fill(1.0),
                    ..Default::default()
                },
                within: LayoutWithin::Intrinsic,
            },
        );
        let other = scene.circle(0.6);
        let row = place(
            scene,
            &[&card, &other],
            false,
            Default::default(),
            LayoutSpec {
                kind: LayoutNodeKind::Row { wrap: false },
                style: LayoutStyle {
                    width: SizeRule::Fill(1.0),
                    height: SizeRule::Fill(1.0),
                    padding: Insets::all(0.4),
                    gap: DVec2::splat(0.3),
                    align: Align::Stretch,
                    ..Default::default()
                },
                within: LayoutWithin::Safe,
            },
        );
        (row, card, slot)
    }

    #[test]
    fn a_fresh_box_tree_measures_like_the_compiled_scene() {
        let mut scene = busy_scene();
        let (row, card, slot) = card_row(&mut scene);
        assert_same(&scene, &slot);
        assert_same(&scene, &card);
        assert_same(&scene, &row);

        // A root box placed by one of its anchors beside the first tree.
        let (moved, _, inner) = card_row(&mut scene);
        let moved = moved.at_anchor(-6.0, 2.0, Anchor::Left);
        assert_same(&scene, &inner);
        assert_same(&scene, &moved);
        assert_same(&scene, &slot);
    }

    /// A decorated 0.4 x 0.2 card, smaller than its background's 1 x 1
    /// placeholder, beside a circle of diameter 1.2: a 1.6 x 1.2 row.
    fn decorated_row(scene: &mut SceneModel) -> (DrawableHandle, DrawableHandle) {
        use crate::canvas::{LayoutMemberSpec, LayoutSpec, LayoutWithin};
        use gaanim_layout::LayoutNodeKind;

        fn place(
            scene: &mut SceneModel,
            members: &[&DrawableHandle],
            decorated: bool,
        ) -> DrawableHandle {
            let root = scene.group(members);
            for member in members {
                member.claim_layout(&root).unwrap();
            }
            if decorated {
                scene
                    .decorate_layout(&root, Some(Color::WHITE.into()), None, 0.1)
                    .unwrap();
            }
            let snapshots = members
                .iter()
                .map(|member| LayoutMemberSpec {
                    id: member.id,
                    style: member.layout_item(),
                })
                .collect();
            let spec = LayoutSpec {
                kind: LayoutNodeKind::Row { wrap: false },
                style: Default::default(),
                within: LayoutWithin::Intrinsic,
            };
            scene.reflow_layout(&root, snapshots, spec, 1, None, None, None);
            root
        }

        let slot = scene.rect(0.4, 0.2);
        let card = place(scene, &[&slot], true);
        let dot = scene.circle(0.6);
        let row = place(scene, &[&card, &dot], false);
        (row, card)
    }

    fn assert_size(bounds: Bounds3D, width: f64, height: f64) {
        assert!(
            (bounds.width() - width).abs() < 1e-6 && (bounds.height() - height).abs() < 1e-6,
            "{bounds:?} is not {width} x {height}"
        );
    }

    /// A decorated box measures its layout box, not its background's
    /// placeholder (#291).
    #[test]
    fn a_decorated_box_measures_its_layout_box() {
        let mut scene = SceneModel::new(16.0, 9.0);
        let (row, card) = decorated_row(&mut scene);
        assert_size(scene.bounds_of(&card).unwrap(), 0.4, 0.2);
        assert_size(scene.bounds_of(&row).unwrap(), 1.6, 1.2);
    }

    /// A frame around a box, alone or inside another box, follows the box
    /// (#292).
    #[test]
    fn surrounding_rects_frame_boxes() {
        use bevy::prelude::With;

        let mut scene = SceneModel::new(16.0, 9.0);
        let (row, card) = decorated_row(&mut scene);
        let expected = [
            scene.bounds_of(&card).unwrap(),
            scene.bounds_of(&row).unwrap(),
        ];
        for target in [&card, &row] {
            scene
                .surrounding_rect(vec![target.bounds_target()], [0.0; 4], 0.0)
                .unwrap();
        }
        scene.wait(0.5);

        let mut world = World::new();
        world.insert_resource(Timeline::new());
        world.insert_resource(gaanim_text::font::FontRegistry::new());
        world.insert_resource(gaanim_text::prelude::TextConfig::default());
        scene.compile(&mut world);
        world.flush();
        let mut timeline = world.remove_resource::<Timeline>().unwrap();
        timeline.add_keyframe(0.0, WorldSnapshot::capture(&mut world));
        timeline.seek(&mut world, 0.5);
        gaanim_animation::tracking_line_system(&mut world);
        gaanim_animation::surrounding_rect_system(&mut world);
        let mut drawn: Vec<Bounds3D> = world
            .query_filtered::<&gaanim_scene::LocalBounds, With<gaanim_animation::SurroundingRect>>()
            .iter(&world)
            .map(|bounds| bounds.0)
            .collect();
        drawn.sort_by(|a, b| a.width().total_cmp(&b.width()));
        assert_eq!(drawn.len(), 2);
        for (drawn, expected) in drawn.iter().zip(expected) {
            assert_size(*drawn, expected.width(), expected.height());
            assert!(
                (drawn.center() - expected.center()).length() < 1e-6,
                "{drawn:?} != {expected:?}"
            );
        }
    }

    /// A readout's row is laid out when the scene compiles, centered on the
    /// group's origin as it is every frame, so a layout centers it in its
    /// cell instead of placing its unlaid parts (#294).
    #[test]
    fn a_readout_measures_its_row_centered_on_its_origin() {
        let mut scene = SceneModel::new(16.0, 9.0);
        let number = scene.reactive_readout(
            gaanim_animation::ScalarSource::constant(42.0),
            ".0f",
            "",
            "%",
            "-",
            Some(0.5),
        );
        let label = scene.text_spec(
            gaanim_text::prelude::TextSpec::new(
                vec!["R".into()],
                None,
                gaanim_text::prelude::TextStyle {
                    size: Some(0.5),
                    ..Default::default()
                },
                gaanim_text::prelude::TextFlow::default(),
            )
            .unwrap(),
        );
        let widths = [
            scene.bounds_of(&label).unwrap().width(),
            scene.bounds_of(&number).unwrap().width(),
        ];
        let readout = scene.reactive_readout_group(Some(&label), None, &number, None, 0.1);
        let bounds = scene.bounds_of(&readout).unwrap();
        assert!(bounds.center().x.abs() < 1e-6, "{bounds:?}");
        assert!(
            (bounds.width() - (widths[0] + 0.1 + widths[1])).abs() < 1e-6,
            "{bounds:?} from {widths:?}"
        );
    }

    #[test]
    fn box_trees_with_history_compile_the_scene() {
        let mut scene = busy_scene();
        let (row, _, slot) = card_row(&mut scene);
        scene.play(vec![row.animate().shift_by(1.0, 0.0).duration(0.4)]);
        assert!(scene.isolated_declaration(slot.id).is_none());
        let bounds = scene.bounds_of(&slot).unwrap();
        assert!(bounds.min.x.is_finite(), "{bounds:?}");

        // A fresh tree whose box then reflows over time.
        let (row, card, fresh) = card_row(&mut scene);
        let members = [&card]
            .iter()
            .map(|member| crate::canvas::LayoutMemberSpec {
                id: member.id,
                style: member.layout_item(),
            })
            .collect();
        scene.reflow_layout(&row, members, Default::default(), 2, Some(0.5), None, None);
        assert!(scene.isolated_declaration(fresh.id).is_none());
    }

    /// The world the scene's measurements at the cursor currently share.
    fn measured_world(scene: &SceneModel) -> Option<bevy::ecs::world::WorldId> {
        let measurements = scene.measured.0.lock().unwrap();
        let (_, compiled) = measurements.cursor.as_ref()?;
        Some(compiled.world.id())
    }

    #[test]
    fn measurements_share_a_compilation_until_the_scene_changes() {
        let mut scene = busy_scene();
        let label = scene.text("medido").move_to(1.0, 1.0);
        let dot = scene.circle(0.3).move_to(-1.0, 0.0);
        scene.play(vec![label.animate().shift_by(1.0, 0.0).duration(0.4)]);
        assert!(scene.isolated_declaration(label.id).is_none());

        let first = scene.bounds_of(&label).unwrap();
        let world = measured_world(&scene).expect("a compiled measurement");
        assert_eq!(scene.bounds_of(&label).unwrap(), first);
        scene.bounds_of(&dot).unwrap();
        assert_eq!(measured_world(&scene), Some(world), "nothing changed");

        // A cut on an animated drawable moves it from the cursor on.
        let label = label.move_to(-2.0, 1.0);
        scene.wait(0.1);
        let moved = scene.bounds_of(&label).unwrap();
        assert!((moved.center().x + 2.0).abs() < 1e-6, "{moved:?}");
        let world = measured_world(&scene).unwrap();

        // A declaration changes what the scene compiles.
        let fresh = scene.rect(1.0, 1.0);
        assert_eq!(scene.bounds_of(&label).unwrap(), moved);
        assert_ne!(measured_world(&scene), Some(world));
        let world = measured_world(&scene).unwrap();
        fresh.fill(Color::BLACK);
        scene.bounds_of(&label).unwrap();
        assert_ne!(measured_world(&scene), Some(world), "a spec changed");

        // So does a scene-wide setting, which lives outside the authored state.
        let world = measured_world(&scene).unwrap();
        scene.background = Some(Color::BLACK);
        scene.bounds_of(&label).unwrap();
        assert_ne!(measured_world(&scene), Some(world));

        // Copies measure in the same compilation; a submitted scene drops it.
        let world = measured_world(&scene).unwrap();
        assert_eq!(scene.clone().bounds_of(&label).unwrap(), moved);
        assert_eq!(measured_world(&scene), Some(world));
        scene.render();
        assert_eq!(measured_world(&scene), None);
    }

    #[test]
    fn a_box_tree_compiles_once_for_all_its_members() {
        let mut scene = busy_scene();
        let (row, card, slot) = card_row(&mut scene);
        let isolated_world = |scene: &SceneModel| {
            let measurements = scene.measured.0.lock().unwrap();
            let (_, compiled) = measurements.isolated.as_ref()?;
            Some(compiled.world.id())
        };
        let boxes = [&slot, &card, &row].map(|member| {
            assert!(scene.isolated_declaration(member.id).is_some());
            scene.bounds_of(member).unwrap()
        });
        let world = isolated_world(&scene).expect("an isolated measurement");
        assert_eq!(scene.bounds_of(&slot).unwrap(), boxes[0]);
        assert_eq!(isolated_world(&scene), Some(world), "one box tree");

        let fresh = scene.text("suelto");
        scene.bounds_of(&fresh).unwrap();
        assert_ne!(isolated_world(&scene), Some(world));
        let world = isolated_world(&scene).unwrap();
        // Another closure, measured after a declaration: compiled again.
        assert_eq!(scene.bounds_of(&card).unwrap(), boxes[1]);
        assert_ne!(isolated_world(&scene), Some(world));
        assert_same(&scene, &row);
    }

    #[test]
    fn declarations_with_history_compile_the_scene() {
        let mut scene = busy_scene();
        let label = scene.text("medido").move_to(1.0, 1.0);
        scene.play(vec![label.animate().shift_by(1.0, 0.0).duration(0.4)]);
        // Frozen and animated: only the compiled scene knows where it is.
        assert!(scene.isolated_declaration(label.id).is_none());
        let bounds = scene.bounds_of(&label).unwrap();
        assert!((bounds.center().x - 2.0).abs() < 1e-6, "{bounds:?}");

        // A later group may move it.
        let fresh = scene.text("suelto");
        let _group = scene.group(&[&fresh]);
        assert!(scene.isolated_declaration(fresh.id).is_none());
        // Placed against something with history.
        let placed = scene.text("junto").next_to(&label, Direction::Down, 0.2);
        assert!(scene.isolated_declaration(placed.id).is_none());
    }
}
