//! Measuring drawables while a scene is being authored.

use std::sync::{Arc, Mutex};

use bevy::ecs::world::CommandQueue;
use bevy::prelude::{Commands, World};
use gaanim_math::Bounds3D;
use gaanim_timeline::snapshot::WorldSnapshot;
use gaanim_timeline::timeline::Timeline;

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
    /// and animations that ended before the cursor all count. Geometry that
    /// reactive updaters rebuild every frame is measured as declared.
    ///
    /// A drawable declared since the scene last advanced (by `play`, `wait`
    /// and the like) that nothing else refers to yet has no animation, cut,
    /// group or layout acting on it: its box depends only on its own
    /// declaration and those of its members, so it is measured by compiling
    /// just them. Otherwise this compiles the scene authored so far.
    pub fn bounds_of(&self, handle: &DrawableHandle) -> Result<Bounds3D, BoundsError> {
        if !self.owns(handle) {
            return Err(BoundsError::ForeignScene);
        }
        if let Some(isolated) = self.isolated_declaration(handle.id) {
            return isolated.compiled_bounds(handle.id, 0.0);
        }
        self.compiled_bounds(handle.id, self.current_time())
    }

    /// Box of the object `id` in this scene compiled up to the authoring
    /// cursor and seeked to `time`.
    fn compiled_bounds(&self, id: ObjectId, time: f64) -> Result<Bounds3D, BoundsError> {
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
        let runtime = checkpoint
            .and_then(|checkpoint| checkpoint.cursor.runtime_id(id))
            .ok_or(BoundsError::Empty)?;
        timeline.add_keyframe(0.0, WorldSnapshot::capture(&mut world));
        timeline.seek(&mut world, time);

        let entity = world
            .query::<(bevy::prelude::Entity, &gaanim_scene::MobjectId)>()
            .iter(&world)
            .find_map(|(entity, id)| (id.0 == runtime).then_some(entity))
            .ok_or(BoundsError::Empty)?;
        let bounds = gaanim_animation::updaters::resolve_entity_bounds(entity, &world)
            .ok_or(BoundsError::Empty)?;
        if !(bounds.min.is_finite() && bounds.max.is_finite()) || bounds.min.x > bounds.max.x {
            return Err(BoundsError::Empty);
        }
        Ok(bounds)
    }
}

impl SceneModel {
    /// This scene reduced to the declarations of the object `id` and its
    /// members, when they are all that decides its box; see
    /// [`SceneModel::bounds_of`].
    fn isolated_declaration(&self, id: ObjectId) -> Option<SceneModel> {
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
        for op in &segment.ops[first..] {
            let Op::Spawn(spec) = op else {
                // An animation, cut, grouping, layout or binding may act on
                // the closure.
                return None;
            };
            let spec = spec.lock().expect("object spec poisoned");
            if closure.contains(&spec.id) {
                ops.push(op.clone());
            } else if spawn_members(&spec.kind)?
                .iter()
                .any(|member| closure.contains(member))
            {
                // A later group gathers the closure and may move it.
                return None;
            }
        }
        if ops.len() != closure.len() {
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
        isolated.state = Arc::new(Mutex::new(isolated_state));
        Some(isolated)
    }
}

/// The object `id` and every object its box depends on (group members, SVG
/// roots, layout references), when all of them are declared but not yet
/// frozen and none depends on anything else. `None` otherwise.
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
        // Frozen objects may carry cuts and animations; layout containers
        // are placed by their layout tree.
        if state.frozen_spawn_specs.contains_key(&id) || state.latest_layouts.contains_key(&id) {
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

/// Whether nothing outside the declaration of `spec` shapes it: no layout,
/// coordinate view, HUD placement or animation state.
fn self_contained(spec: &ObjectSpec) -> bool {
    spec.layout_owner.is_none()
        && spec.layout_background.is_none()
        && !spec.hud
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
        scene.compiled_bounds(self.inner.target, scene.current_time())
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
