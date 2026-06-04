use bevy::prelude::{BuildChildrenTransformExt, Commands, Entity};
use gaanim_core::ObjectId;
use gaanim_core::kurbo;
use gaanim_core::peniko::{Brush, Color};
use gaanim_layout::{LayoutAnchor, LayoutDirection};
use gaanim_math::{Bounds3D, SpatialTransform};
use gaanim_objects::prelude::MobjectBundle;
use gaanim_scene::{FillBrush, GroupMarker, LocalBounds, MobjectId, Opacity, StrokeBrush, Visible, WorldBounds};
use gaanim_text::font::FontRegistry;
use gaanim_text::shaper::compile_text_to_hierarchy;
use gaanim_text::typst_compiler::compile_typst_to_hierarchy;
use gaanim_timeline::{
    clip::{AnimationSpec, ClipPayload, PropertyLensSpec, SceneId, TrackId},
    scene::SceneMember,
    timeline::Timeline,
    transition::TransitionType,
};
use crate::anim::{AnimationBuilder, AnimationType, ValueTrackerRef};
use std::collections::HashMap;

/// Extracts a representative `Color` from a `peniko::Brush` for use as a
/// stroke color in the `Write` animation's auto-stroke fallback.
///
/// - `Brush::Solid(c)` → `Some(c)`
/// - `Brush::Gradient(g)` → first color stop, if any
/// - `Brush::Image(_)` → `None` (no meaningful single color)
fn extract_brush_color(brush: &Brush) -> Option<Color> {
    use gaanim_core::peniko::color::Srgb;
    match brush {
        Brush::Solid(c) => Some(*c),
        Brush::Gradient(g) => g.stops.first().map(|s| s.color.to_alpha_color::<Srgb>()),
        _ => None,
    }
}

/// Tracks the active hot state of an Mobject during scene construction.
/// This enables subsequent layouts and animations to automatically calculate
/// their offsets and "from" properties without manual user input.
#[derive(Debug, Clone)]
pub struct MobjectState {
    pub bounds: Bounds3D,
    pub transform: SpatialTransform,
    pub opacity: f32,
    pub fill: Option<Brush>,
    pub stroke: StrokeBrush,
    pub entity: Entity,
    pub child_spans: Vec<(ObjectId, Entity, gaanim_scene::components::TextSpan)>,
    pub children: Vec<ObjectId>,
    pub parent: Option<ObjectId>,
}

/// A `Vec`-backed map from `ObjectId` to `MobjectState`.
///
/// IDs tend to be allocated sequentially by `SceneBuilder::next_id()`, but
/// the parent state is often inserted *after* its children (higher indices),
/// so gaps are handled gracefully with `Option`.
#[derive(Debug, Clone)]
pub struct MobjectStateMap {
    v: Vec<Option<MobjectState>>,
}

impl Default for MobjectStateMap {
    fn default() -> Self {
        Self::new()
    }
}

impl MobjectStateMap {
    pub fn new() -> Self {
        Self { v: Vec::new() }
    }

    pub fn insert(&mut self, id: ObjectId, state: MobjectState) {
        let idx = id.index() as usize;
        if idx >= self.v.len() {
            self.v.resize_with(idx + 1, || None);
        }
        self.v[idx] = Some(state);
    }

    pub fn get(&self, id: ObjectId) -> Option<&MobjectState> {
        self.v.get(id.index() as usize).and_then(|v| v.as_ref())
    }

    pub fn get_mut(&mut self, id: ObjectId) -> Option<&mut MobjectState> {
        self.v.get_mut(id.index() as usize).and_then(|v| v.as_mut())
    }

    pub fn contains_key(&self, id: ObjectId) -> bool {
        self.v
            .get(id.index() as usize)
            .and_then(|v| v.as_ref())
            .is_some()
    }

    pub fn remove(&mut self, id: ObjectId) {
        let idx = id.index() as usize;
        if idx < self.v.len() {
            self.v[idx] = None;
        }
    }
}

/// A lightweight reference handle to a spawned Mobject in the Scene.
#[derive(Clone, Copy, Debug)]
pub struct MobjectRef {
    pub id: ObjectId,
}

/// A selection of multiple child Mobjects (usually characters or shapes in a text/equation)
/// that can be styled or animated as a single coordinated group.
pub struct MobjectSelection<'a, 'w, 's, 'b> {
    pub builder: &'a mut SceneBuilder<'w, 's, 'b>,
    pub parent_id: ObjectId,
    pub child_ids: Vec<ObjectId>,
}

impl<'a, 'w, 's, 'b> MobjectSelection<'a, 'w, 's, 'b> {
    /// Instantly colors the fill of all selected symbols.
    ///
    /// If the entity already carries a stroke brush (for example the
    /// auto-stroke synthesized by a `Write` animation), the stroke is
    /// retinted to match the new fill so the progressive outline stays
    /// color-coordinated with the selection. The accompanying
    /// `PathCompletion` global reset in `play_write_internal` is what
    /// prevents the outline from being visible at frame 0, so updating
    /// the stroke here does not reintroduce that regression.
    pub fn set_fill(&mut self, color: Color) -> &mut Self {
        for child_id in &self.child_ids {
            if let Some(state) = self.builder.states.get_mut(*child_id) {
                state.fill = Some(Brush::Solid(color));
                self.builder
                    .commands
                    .entity(state.entity)
                    .insert(FillBrush(Some(Brush::Solid(color))));
                if state.stroke.brush.is_some() {
                    let width = state.stroke.style.width;
                    let new_stroke = StrokeBrush::new(color, width);
                    state.stroke = new_stroke.clone();
                    self.builder
                        .commands
                        .entity(state.entity)
                        .insert(new_stroke);
                }
            }
        }
        self
    }

    /// Instantly colors the outline stroke of all selected symbols.
    pub fn set_stroke(&mut self, color: Color, width: f64) -> &mut Self {
        for child_id in &self.child_ids {
            if let Some(state) = self.builder.states.get_mut(*child_id) {
                state.stroke = StrokeBrush::new(color, width);
                self.builder
                    .commands
                    .entity(state.entity)
                    .insert(StrokeBrush::new(color, width));
            }
        }
        self
    }

    /// Prepares a parallel coordinated animation sequence for all selected entities.
    pub fn animate(&mut self) -> CoordinatedAnimationBuilder<'_, 'w, 's, 'b> {
        CoordinatedAnimationBuilder {
            builder: self.builder,
            child_ids: self.child_ids.clone(),
            duration: 1.0,
            rate_func: gaanim_math::prelude::RateFunc::Smooth,
        }
    }
}

/// Fluent builder to configure and play parallel animations across a selection of Mobjects.
pub struct CoordinatedAnimationBuilder<'a, 'w, 's, 'b> {
    builder: &'a mut SceneBuilder<'w, 's, 'b>,
    child_ids: Vec<ObjectId>,
    duration: f64,
    rate_func: gaanim_math::prelude::RateFunc,
}

impl<'a, 'w, 's, 'b> CoordinatedAnimationBuilder<'a, 'w, 's, 'b> {
    pub fn duration(mut self, d: f64) -> Self {
        self.duration = d;
        self
    }

    pub fn rate_func(mut self, r: gaanim_math::prelude::RateFunc) -> Self {
        self.rate_func = r;
        self
    }

    pub fn smooth(mut self) -> Self {
        self.rate_func = gaanim_math::prelude::RateFunc::Smooth;
        self
    }

    pub fn linear(mut self) -> Self {
        self.rate_func = gaanim_math::prelude::RateFunc::Linear;
        self
    }

    pub fn spring(mut self) -> Self {
        self.rate_func = gaanim_math::prelude::RateFunc::Spring {
            stiffness: 90.0,
            damping: 12.0,
        };
        self
    }

    /// Play a shift/translation animation on all selected sub-elements in parallel.
    pub fn shift_2d(self, x: f64, y: f64) {
        let mut anims = Vec::new();
        for id in self.child_ids {
            anims.push(
                MobjectRef { id }
                    .shift_2d(x, y)
                    .duration(self.duration)
                    .rate_func(self.rate_func.clone()),
            );
        }
        self.builder.play_parallel(anims);
    }

    /// Play a fade out animation on all selected sub-elements in parallel.
    pub fn fade_out(self) {
        let mut anims = Vec::new();
        for id in self.child_ids {
            anims.push(
                MobjectRef { id }
                    .fade_out()
                    .duration(self.duration)
                    .rate_func(self.rate_func.clone()),
            );
        }
        self.builder.play_parallel(anims);
    }

    /// Play a scale animation on all selected sub-elements in parallel.
    pub fn scale_uniform(self, factor: f64) {
        let mut anims = Vec::new();
        for id in self.child_ids {
            anims.push(
                MobjectRef { id }
                    .scale_uniform(factor)
                    .duration(self.duration)
                    .rate_func(self.rate_func.clone()),
            );
        }
        self.builder.play_parallel(anims);
    }

    /// Play a fill color interpolation on all selected sub-elements in parallel.
    pub fn fill_color_to(self, color: Color) {
        let mut anims = Vec::new();
        for id in self.child_ids {
            anims.push(
                MobjectRef { id }
                    .fill_color_to(color)
                    .duration(self.duration)
                    .rate_func(self.rate_func.clone()),
            );
        }
        self.builder.play_parallel(anims);
    }
}

/// The high-level fluent API builder for constructing gaanim scenes.
///
/// Manages auto-incrementing ObjectId generation, relative layouts, active states,
/// and sequential/parallel animation clip registration on the Timeline clock.
pub struct SceneBuilder<'w, 's, 'a> {
    pub commands: &'a mut Commands<'w, 's>,
    pub timeline: &'a mut Timeline,
    pub font_registry: &'a FontRegistry,
    pub text_config: &'a gaanim_text::prelude::TextConfig,
    pub id_counter: u32,
    pub current_time: f64,
    pub states: MobjectStateMap,
    pub default_track: TrackId,
    mobject_tracks: HashMap<ObjectId, TrackId>,
    mobject_names: HashMap<ObjectId, String>,
    next_track: u32,
    current_label: Option<String>,
    /// The currently active scene (None when outside any scene scope).
    pub current_scene: Option<SceneId>,
    /// Tracks the current value of each float signal / value tracker
    pub float_signals: HashMap<ObjectId, f64>,
}

impl<'w, 's, 'a> SceneBuilder<'w, 's, 'a> {
    /// Computes the true world-space transform of a Mobject by walking up the parent chain.
    /// This is necessary during group creation to calculate accurate world bounds for nested children.
    pub fn get_world_transform(&self, id: ObjectId) -> SpatialTransform {
        let mut current_id = Some(id);
        let mut world_affine = gaanim_core::kurbo::Affine::IDENTITY;

        while let Some(obj_id) = current_id {
            if let Some(state) = self.states.get(obj_id) {
                world_affine = state.transform.to_affine_2d() * world_affine;
                current_id = state.parent;
            } else {
                break;
            }
        }

        SpatialTransform::from_affine_2d(&world_affine)
    }

    /// Creates a new `SceneBuilder` wrapping the Bevy `Commands` context, `Timeline` resource, and `FontRegistry`.
    pub fn new(
        commands: &'a mut Commands<'w, 's>,
        timeline: &'a mut Timeline,
        font_registry: &'a FontRegistry,
        text_config: &'a gaanim_text::prelude::TextConfig,
    ) -> Self {
        // Ensure a default track exists on the timeline
        let default_track = if let Some(track_id) = timeline.tracks.keys().next() {
            track_id
        } else {
            timeline.add_track("Main Graphics", 0)
        };

        Self {
            commands,
            timeline,
            font_registry,
            text_config,
            id_counter: 0,
            current_time: 0.0,
            states: MobjectStateMap::new(),
            default_track,
            mobject_tracks: HashMap::new(),
            mobject_names: HashMap::new(),
            next_track: 0,
            current_label: None,
            current_scene: None,
            float_signals: HashMap::new(),
        }
    }

    /// Returns the per-mobject track for the given target, creating a new
    /// numbered track if this is the first time we see this ObjectId.
    fn ensure_track(&mut self, target: ObjectId) -> TrackId {
        let current_scene = self.current_scene;
        *self.mobject_tracks.entry(target).or_insert_with(|| {
            self.next_track += 1;
            let name = self
                .mobject_names
                .get(&target)
                .cloned()
                .unwrap_or_else(|| format!("Object {}", self.next_track));
            let track_id = self.timeline.add_track(&name, self.next_track as i32);
            if let Some(track) = self.timeline.tracks.get_mut(track_id) {
                track.object_id = Some(target);
                track.scene = current_scene;
            }
            track_id
        })
    }

    /// Begins a new scene scope. All mobjects spawned and animations scheduled
    /// after this call will belong to the scene until `end_scene()` is called.
    pub fn begin_scene(&mut self, name: &str) -> SceneId {
        let scene_id = self.timeline.add_scene(name);
        self.current_scene = Some(scene_id);
        // Insert SceneStart marker clip at the current time
        self.timeline.add_clip(
            self.default_track,
            self.current_time,
            0.0,
            ClipPayload::SceneStart(scene_id),
        );
        // Index the scene start time for O(log n) lookup in scene_at().
        self.timeline.index_scene(scene_id, self.current_time);
        scene_id
    }

    /// Ends the current scene scope.
    pub fn end_scene(&mut self) {
        if let Some(scene_id) = self.current_scene {
            self.timeline.add_clip(
                self.default_track,
                self.current_time,
                0.0,
                ClipPayload::SceneEnd(scene_id),
            );
            self.current_scene = None;
        }
    }

    /// Executes a closure within a scene scope, automatically handling begin/end.
    pub fn scene_scope<F>(&mut self, name: &str, f: F) -> SceneId
    where
        F: FnOnce(&mut Self),
    {
        let id = self.begin_scene(name);
        f(self);
        self.end_scene();
        id
    }

    /// Tags an entity with the current scene's `SceneMember` component,
    /// if currently inside a scene scope. Call this from ALL entity
    /// spawning paths (not just `MobjectSpawnBuilder::spawn()`).
    pub fn tag_entity(&mut self, entity: Entity) {
        if let Some(scene_id) = self.current_scene {
            self.commands.entity(entity).insert(SceneMember(scene_id));
        }
    }

    /// Records a transition from the current scene to a target scene.
    pub fn transition_to(&mut self, target: SceneId, transition: TransitionType) {
        let current = self.current_scene.expect("Must be inside a scene to call transition_to");
        self.timeline.connect(current, target, transition);
    }

    fn anim_label(ty: &AnimationType) -> &'static str {
        match ty {
            AnimationType::Write { .. } => "Write",
            AnimationType::Create { .. } => "Create",
            AnimationType::Unwrite { .. } => "Unwrite",
            AnimationType::Uncreate { .. } => "Uncreate",
            AnimationType::TranslateTo { .. } | AnimationType::TranslateBy { .. } => "Move",
            AnimationType::RotateTo { .. } | AnimationType::RotateBy { .. } => "Rotate",
            AnimationType::ScaleTo { .. } | AnimationType::ScaleUniform { .. } => "Scale",
            AnimationType::FadeTo { .. } | AnimationType::FadeIn | AnimationType::FadeOut => "Fade",
            AnimationType::FillColorTo { .. } => "Fill",
            AnimationType::StrokeColorTo { .. } => "Stroke",
            AnimationType::StrokeWidthTo { .. } => "StrokeW",
            AnimationType::GrowFromCenter => "Grow",
            AnimationType::ShrinkToCenter => "Shrink",
            AnimationType::SpinInFromNothing => "SpinIn",
            AnimationType::Indicate { .. } => "Indicate",
            AnimationType::FadeTransform { .. } => "Morph",
            AnimationType::Wiggle => "Wiggle",
            AnimationType::GrowFromPoint { .. } | AnimationType::GrowFromEdge { .. } => "Grow",
            AnimationType::DrawBorderThenFill => "DrawFill",
            AnimationType::Flash { .. } => "Flash",
            AnimationType::Circumscribe { .. } => "Circum",
            AnimationType::MoveAlongPath { .. } => "Follow",
            AnimationType::GrowArrow => "Arrow",
            AnimationType::SignalFloat { .. } => "Signal",
            AnimationType::ShowPassingFlash { .. } => "ShowPassingFlash",
        }
    }

    /// Allocates a new, stable, auto-incremented ObjectId starting at index 0.
    pub fn next_id(&mut self) -> ObjectId {
        let id = ObjectId::from_parts(self.id_counter, 1);
        self.id_counter += 1;
        id
    }

    /// Advances the internal timeline playhead by the specified duration.
    pub fn wait(&mut self, duration: f64) {
        self.current_time += duration;
    }

    /// Registers an interactive breakpoint (slide transition) at the current timeline playhead.
    pub fn slide(&mut self) {
        self.timeline.add_clip(
            self.default_track,
            self.current_time,
            0.0,
            ClipPayload::Breakpoint,
        );
        self.timeline.breakpoints.push(self.current_time);
    }

    /// Sequences a single animation clip on the timeline and advances the playhead.
    pub fn play(&mut self, anim: AnimationBuilder) {
        let duration = anim.duration;
        self.play_internal(anim);
        self.current_time += duration;
    }

    /// Plays multiple animation clips starting at the same time,
    /// advancing the playhead by the maximum duration among them.
    pub fn play_parallel(&mut self, anims: Vec<AnimationBuilder>) {
        let mut max_duration = 0.0;
        for anim in anims {
            if anim.duration > max_duration {
                max_duration = anim.duration;
            }
            self.play_internal(anim);
        }
        self.current_time += max_duration;
    }

    /// Internal method to resolve and schedule a single animation clip.
    fn play_internal(&mut self, anim: AnimationBuilder) {
        self.current_label = Some(Self::anim_label(&anim.anim_type).to_string());
        let track = self.ensure_track(anim.target);

        // The Write/Create/Uncreate/Unwrite/SpinIn/Indicate animations expand into
        // multiple staggered or parallel sub-clips, so they have their own branches
        // that access the timeline multiple times. All other variants collapse to a
        // single clip below.
        if matches!(anim.anim_type, AnimationType::Write { .. }) {
            self.play_draw_erase_internal(anim, false, true, track);
            return;
        }
        if matches!(anim.anim_type, AnimationType::Create { .. }) {
            self.play_draw_erase_internal(anim, false, false, track);
            return;
        }
        if matches!(anim.anim_type, AnimationType::Unwrite { .. }) {
            self.play_draw_erase_internal(anim, true, true, track);
            return;
        }
        if matches!(anim.anim_type, AnimationType::Uncreate { .. }) {
            self.play_draw_erase_internal(anim, true, false, track);
            return;
        }
        if matches!(anim.anim_type, AnimationType::SpinInFromNothing) {
            self.play_spin_in_from_nothing_internal(anim, track);
            return;
        }
        if matches!(anim.anim_type, AnimationType::Indicate { .. }) {
            self.play_indicate_internal(anim, track);
            return;
        }
        if matches!(anim.anim_type, AnimationType::FadeTransform { .. }) {
            self.play_fade_transform_internal(anim, track);
            return;
        }
        if matches!(anim.anim_type, AnimationType::Wiggle) {
            self.play_wiggle_internal(anim, track);
            return;
        }
        if matches!(anim.anim_type, AnimationType::GrowFromPoint { .. }) {
            self.play_grow_from_point_internal(anim, track);
            return;
        }
        if matches!(anim.anim_type, AnimationType::GrowFromEdge { .. }) {
            self.play_grow_from_edge_internal(anim, track);
            return;
        }
        if matches!(anim.anim_type, AnimationType::DrawBorderThenFill) {
            self.play_draw_border_then_fill_internal(anim, track);
            return;
        }
        if matches!(anim.anim_type, AnimationType::Flash { .. }) {
            self.play_flash_internal(anim, track);
            return;
        }
        if matches!(anim.anim_type, AnimationType::Circumscribe { .. }) {
            self.play_circumscribe_internal(anim, track);
            return;
        }
        if matches!(anim.anim_type, AnimationType::MoveAlongPath { .. }) {
            self.play_move_along_path_internal(anim, track);
            return;
        }
        if matches!(anim.anim_type, AnimationType::GrowArrow) {
            self.play_grow_arrow_internal(anim, track);
            return;
        }

        let state = match self.states.get_mut(anim.target) {
            Some(s) => s,
            None => {
                bevy::prelude::warn!(
                    "Attempted to animate unregistered Mobject: {:?}",
                    anim.target
                );
                return;
            }
        };

        // Resolve lens and update our tracked local hot state
        let lens_spec = match anim.anim_type {
            AnimationType::TranslateTo { to } => {
                let from = state.transform.translation;
                state.transform.translation = to;
                PropertyLensSpec::Translation { from, to }
            }
            AnimationType::TranslateBy { delta } => {
                let from = state.transform.translation;
                let to = from + delta;
                state.transform.translation = to;
                PropertyLensSpec::Translation { from, to }
            }
            AnimationType::RotateTo { to } => {
                let from = state.transform.rotation;
                state.transform.rotation = to;
                PropertyLensSpec::Rotation { from, to }
            }
            AnimationType::RotateBy { angle_radians } => {
                let from = state.transform.rotation;
                let to = from * gaanim_core::glam::DQuat::from_rotation_z(angle_radians);
                state.transform.rotation = to;
                PropertyLensSpec::Rotation { from, to }
            }
            AnimationType::ScaleTo { to } => {
                let from = state.transform.scale;
                state.transform.scale = to;
                PropertyLensSpec::Scale { from, to }
            }
            AnimationType::ScaleUniform { factor } => {
                let from = state.transform.scale;
                let to = from * factor;
                state.transform.scale = to;
                PropertyLensSpec::Scale { from, to }
            }
            AnimationType::FadeTo { to } => {
                let from = state.opacity;
                state.opacity = to;
                PropertyLensSpec::Opacity { from, to }
            }
            AnimationType::FadeIn => {
                let from = 0.0;
                let to = 1.0;
                state.opacity = 1.0;
                PropertyLensSpec::Opacity { from, to }
            }
            AnimationType::FadeOut => {
                let from = state.opacity;
                let to = 0.0;
                state.opacity = 0.0;
                PropertyLensSpec::Opacity { from, to }
            }
            AnimationType::FillColorTo { to } => {
                let from = match &state.fill {
                    Some(Brush::Solid(c)) => *c,
                    _ => Color::WHITE,
                };
                state.fill = Some(Brush::Solid(to));
                PropertyLensSpec::FillColor { from, to }
            }
            AnimationType::StrokeColorTo { to } => {
                let from = match &state.stroke.brush {
                    Some(Brush::Solid(c)) => *c,
                    _ => Color::WHITE,
                };
                state.stroke.brush = Some(Brush::Solid(to));
                PropertyLensSpec::StrokeColor { from, to }
            }
            AnimationType::StrokeWidthTo { to } => {
                let from = state.stroke.style.width;
                state.stroke.style.width = to;
                PropertyLensSpec::StrokeWidth { from, to }
            }
            AnimationType::GrowFromCenter => {
                let to = state.transform.scale;
                let from = gaanim_core::glam::DVec3::ZERO;
                // Pre-set the scale to 0.0 right now via deferred commands to avoid flickers
                let mut temp_transform = state.transform;
                temp_transform.scale = from;
                self.commands.entity(state.entity).insert(temp_transform);
                PropertyLensSpec::Scale { from, to }
            }
            AnimationType::ShrinkToCenter => {
                let from = state.transform.scale;
                let to = gaanim_core::glam::DVec3::ZERO;
                state.transform.scale = to;
                PropertyLensSpec::Scale { from, to }
            }
            AnimationType::Write { .. }
            | AnimationType::Create { .. }
            | AnimationType::Unwrite { .. }
            | AnimationType::Uncreate { .. }
            | AnimationType::SpinInFromNothing
            | AnimationType::Indicate { .. }
            | AnimationType::FadeTransform { .. }
            | AnimationType::Wiggle
            | AnimationType::GrowFromPoint { .. }
            | AnimationType::GrowFromEdge { .. }
            | AnimationType::DrawBorderThenFill
            | AnimationType::Flash { .. }
            | AnimationType::Circumscribe { .. }
            | AnimationType::MoveAlongPath { .. }
            | AnimationType::GrowArrow => {
                unreachable!("Expansion is dispatched in the early branch above")
            }
            AnimationType::SignalFloat { to } => {
                let from = *self.float_signals.get(&anim.target).unwrap_or(&0.0);
                self.float_signals.insert(anim.target, to);
                PropertyLensSpec::SignalFloat { from, to }
            }
            AnimationType::ShowPassingFlash { time_width } => {
                PropertyLensSpec::PathRange {
                    from: 0.0,
                    to: 1.0 + time_width,
                    time_width,
                }
            }
        };

        // Add the resolved clip to the Timeline resource
        self.timeline.add_clip(
            track,
            self.current_time,
            anim.duration,
            ClipPayload::Animation(AnimationSpec {
                target: anim.target,
                lens: lens_spec,
                rate_func: anim.rate_func,
                label: self.current_label.clone(),
            }),
        );
    }

    /// Internal: materialize a `Write` animation as one or more staggered
    /// sub-clip pairs. If the target has children (text/equation glyphs,
    /// group members), each child draws in sequence. If the target is a
    /// leaf (e.g. a single `circle`/`line`), one pair is scheduled on the
    /// target itself.
    ///
    /// Each item receives two clips:
    /// 1. `PathCompletion 0.0 -> 1.0` over the first `DRAW_RATIO` of
    ///    `item_duration` — progressively reveals the outline.
    /// 2. `FillDrawProgress 0.0 -> 1.0` over the remaining
    ///    `1 - DRAW_RATIO` of `item_duration`, starting right after the
    ///    path draw completes — cross-fades the fill in once the outline
    ///    is fully drawn.
    ///
    /// To prevent the "object fully visible from the start" bug, we also
    /// `insert(FillDrawProgress(0.0))` on every target entity right here
    /// (via the deferred command queue). By the time the first render
    /// frame runs, the fill alpha multiplier is `0.0` and the renderer
    /// will render an empty/invisible fill, so the user only ever sees
    /// Internal: generalize a draw/erase animation (Write, Create, Unwrite, Uncreate)
    /// as one or more staggered or parallel sub-clip sequences.
    fn play_draw_erase_internal(
        &mut self,
        anim: AnimationBuilder,
        is_erase: bool,
        staggered: bool,
        parent_track: TrackId,
    ) {
        let stroke_width = match anim.anim_type {
            AnimationType::Write { stroke_width } => stroke_width,
            AnimationType::Create { stroke_width } => stroke_width,
            AnimationType::Unwrite { stroke_width } => stroke_width,
            AnimationType::Uncreate { stroke_width } => stroke_width,
            _ => None,
        };

        // Collect target ids: the target's own id plus all child spans.
        let mut items: Vec<ObjectId> = {
            let state = match self.states.get(anim.target) {
                Some(s) => s,
                None => {
                    bevy::prelude::warn!(
                        "Attempted to animate unregistered Mobject: {:?}",
                        anim.target
                    );
                    return;
                }
            };
            if state.child_spans.is_empty() {
                vec![anim.target]
            } else {
                state.child_spans.iter().map(|(id, _, _)| *id).collect()
            }
        };

        let n = items.len();
        if n == 0 {
            return;
        }

        // If staggered and is_erase, we reverse the items so sequential erasure happens in reverse (right-to-left)
        if staggered && is_erase {
            items.reverse();
        }

        // (A) Auto-stroke: entities that have no stroke brush yet get a stroke synthesized
        // from the fill color and the user-supplied stroke_width (or 1.0) so the outline
        // is visible during drawing/erasing.
        for item_id in &items {
            if let Some(state) = self.states.get_mut(*item_id)
                && state.stroke.brush.is_none() {
                    let color = state
                        .fill
                        .as_ref()
                        .and_then(extract_brush_color)
                        .unwrap_or(Color::WHITE);
                    let width = stroke_width.unwrap_or(1.0);
                    let new_stroke = StrokeBrush::new(color, width);
                    state.stroke = new_stroke.clone();
                    self.commands.entity(state.entity).insert(new_stroke);
                }
        }

        // (B) Set initial value via deferred commands to avoid flicker
        for item_id in &items {
            if let Some(state) = self.states.get(*item_id) {
                let initial_val = if is_erase { 1.0 } else { 0.0 };
                self.commands
                    .entity(state.entity)
                    .insert(gaanim_animation::FillDrawProgress(initial_val));

                // If drawing, immediately insert an empty Path2D to guarantee no first-frame flash!
                if !is_erase {
                    self.commands
                        .entity(state.entity)
                        .insert(gaanim_scene::components::Path2D(
                            std::sync::Arc::new(gaanim_core::kurbo::BezPath::new()),
                        ));
                }
            }
        }

        /// Lag ratio between consecutive items.
        const LAG_RATIO: f64 = 0.25;
        /// Fraction of item_duration spent drawing/erasing the outline vs fill fade.
        const DRAW_RATIO: f64 = 0.7;

        let item_duration = if staggered {
            anim.duration / (1.0 + (n as f64 - 1.0) * LAG_RATIO)
        } else {
            anim.duration
        };
        let lag_step = if staggered {
            item_duration * LAG_RATIO
        } else {
            0.0
        };
        let min_step = 1e-6_f64.max(item_duration * 0.01);

        let draw_duration = (item_duration * DRAW_RATIO).max(min_step);
        let fade_duration = (item_duration * (1.0 - DRAW_RATIO)).max(min_step);

        // (C) Global resets at self.current_time to ensure determinism during seek/rewind.
        let reset_fill_val = if is_erase { 1.0 } else { 0.0 };
        let reset_path_val = if is_erase { 1.0 } else { 0.0 };

        for item_id in &items {
            self.timeline.add_clip(
                parent_track,
                self.current_time,
                min_step,
                ClipPayload::Animation(AnimationSpec {
                    target: *item_id,
                    lens: PropertyLensSpec::FillDrawProgress {
                        from: reset_fill_val,
                        to: reset_fill_val,
                    },
                    rate_func: anim.rate_func.clone(),
                    label: self.current_label.clone(),
                }),
            );
            self.timeline.add_clip(
                parent_track,
                self.current_time,
                min_step,
                ClipPayload::Animation(AnimationSpec {
                    target: *item_id,
                    lens: PropertyLensSpec::PathCompletion {
                        from: reset_path_val,
                        to: reset_path_val,
                    },
                    rate_func: anim.rate_func.clone(),
                    label: self.current_label.clone(),
                }),
            );
        }

        // (D) Schedule per-item sequence
        for (i, item_id) in items.iter().enumerate() {
            let item_delay = (i as f64 * lag_step).max(0.0);
            let item_start = self.current_time + item_delay;

            if !is_erase {
                // DRAW FLOW: outline draws first, then fill fades in
                let fade_start = item_start + draw_duration;

                // 1. Fill hold at 0.0 during draw phase
                self.timeline.add_clip(
                    parent_track,
                    item_start,
                    draw_duration,
                    ClipPayload::Animation(AnimationSpec {
                        target: *item_id,
                        lens: PropertyLensSpec::FillDrawProgress { from: 0.0, to: 0.0 },
                        rate_func: anim.rate_func.clone(),
                        label: self.current_label.clone(),
                    }),
                );

                // 2. Outline draw: PathCompletion 0.0 -> 1.0
                self.timeline.add_clip(
                    parent_track,
                    item_start,
                    draw_duration,
                    ClipPayload::Animation(AnimationSpec {
                        target: *item_id,
                        lens: PropertyLensSpec::PathCompletion { from: 0.0, to: 1.0 },
                        rate_func: anim.rate_func.clone(),
                        label: self.current_label.clone(),
                    }),
                );

                // 3. Fill fade-in: FillDrawProgress 0.0 -> 1.0
                self.timeline.add_clip(
                    parent_track,
                    fade_start,
                    fade_duration,
                    ClipPayload::Animation(AnimationSpec {
                        target: *item_id,
                        lens: PropertyLensSpec::FillDrawProgress { from: 0.0, to: 1.0 },
                        rate_func: anim.rate_func.clone(),
                        label: self.current_label.clone(),
                    }),
                );
            } else {
                // ERASE FLOW: fill fades out first, then outline erases
                let draw_start = item_start + fade_duration;

                // 1. Fill fade-out: FillDrawProgress 1.0 -> 0.0
                self.timeline.add_clip(
                    parent_track,
                    item_start,
                    fade_duration,
                    ClipPayload::Animation(AnimationSpec {
                        target: *item_id,
                        lens: PropertyLensSpec::FillDrawProgress { from: 1.0, to: 0.0 },
                        rate_func: anim.rate_func.clone(),
                        label: self.current_label.clone(),
                    }),
                );

                // 2. Outline hold at 1.0 during fade phase
                self.timeline.add_clip(
                    parent_track,
                    item_start,
                    fade_duration,
                    ClipPayload::Animation(AnimationSpec {
                        target: *item_id,
                        lens: PropertyLensSpec::PathCompletion { from: 1.0, to: 1.0 },
                        rate_func: anim.rate_func.clone(),
                        label: self.current_label.clone(),
                    }),
                );

                // 3. Outline erase: PathCompletion 1.0 -> 0.0
                self.timeline.add_clip(
                    parent_track,
                    draw_start,
                    draw_duration,
                    ClipPayload::Animation(AnimationSpec {
                        target: *item_id,
                        lens: PropertyLensSpec::PathCompletion { from: 1.0, to: 0.0 },
                        rate_func: anim.rate_func.clone(),
                        label: self.current_label.clone(),
                    }),
                );

                // 4. Fill hold at 0.0 during erase phase
                self.timeline.add_clip(
                    parent_track,
                    draw_start,
                    draw_duration,
                    ClipPayload::Animation(AnimationSpec {
                        target: *item_id,
                        lens: PropertyLensSpec::FillDrawProgress { from: 0.0, to: 0.0 },
                        rate_func: anim.rate_func.clone(),
                        label: self.current_label.clone(),
                    }),
                );
            }

            // Stroke width override if requested
            if let Some(width) = stroke_width {
                self.timeline.add_clip(
                    parent_track,
                    item_start,
                    min_step,
                    ClipPayload::Animation(AnimationSpec {
                        target: *item_id,
                        lens: PropertyLensSpec::StrokeWidth {
                            from: width,
                            to: width,
                        },
                        rate_func: anim.rate_func.clone(),
                        label: self.current_label.clone(),
                    }),
                );
            }
        }
    }

    /// Internal: materialize `SpinInFromNothing` as a simultaneous scale-up and 360-degree rotation.
    fn play_spin_in_from_nothing_internal(&mut self, anim: AnimationBuilder, parent_track: TrackId) {
        let state = match self.states.get_mut(anim.target) {
            Some(s) => s,
            None => {
                bevy::prelude::warn!(
                    "Attempted to SpinInFromNothing unregistered Mobject: {:?}",
                    anim.target
                );
                return;
            }
        };

        let initial_scale = state.transform.scale;
        let initial_rotation = state.transform.rotation;

        // To avoid quaternion SLERP shortest-path 0-rotation logic issues for 360 degrees,
        // we split the rotation into two consecutive 180-degree clips (PI radians each).
        let mid_rotation =
            initial_rotation * gaanim_core::glam::DQuat::from_rotation_z(std::f64::consts::PI);
        let end_rotation = initial_rotation
            * gaanim_core::glam::DQuat::from_rotation_z(2.0 * std::f64::consts::PI);

        // Pre-set the scale to 0.0 right now via deferred commands to avoid first-frame flickers
        let mut temp_transform = state.transform;
        temp_transform.scale = gaanim_core::glam::DVec3::ZERO;
        self.commands.entity(state.entity).insert(temp_transform);

        // Update the final expected state at the end of scheduling
        state.transform.rotation = end_rotation;

        // 1. Unified scale clip (0.0 -> target_scale) over the full duration
        self.timeline.add_clip(
            parent_track,
            self.current_time,
            anim.duration,
            ClipPayload::Animation(AnimationSpec {
                target: anim.target,
                lens: PropertyLensSpec::Scale {
                    from: gaanim_core::glam::DVec3::ZERO,
                    to: initial_scale,
                },
                rate_func: anim.rate_func.clone(),
                label: self.current_label.clone(),
            }),
        );

        // 2. Rotation part 1 (0 -> 180 deg) over first half
        let half_duration = anim.duration * 0.5;
        self.timeline.add_clip(
            parent_track,
            self.current_time,
            half_duration,
            ClipPayload::Animation(AnimationSpec {
                target: anim.target,
                lens: PropertyLensSpec::Rotation {
                    from: initial_rotation,
                    to: mid_rotation,
                },
                rate_func: anim.rate_func.clone(),
                label: self.current_label.clone(),
            }),
        );

        // 3. Rotation part 2 (180 -> 360 deg) over second half
        self.timeline.add_clip(
            parent_track,
            self.current_time + half_duration,
            half_duration,
            ClipPayload::Animation(AnimationSpec {
                target: anim.target,
                lens: PropertyLensSpec::Rotation {
                    from: mid_rotation,
                    to: end_rotation,
                },
                rate_func: anim.rate_func.clone(),
                label: self.current_label.clone(),
            }),
        );
    }

    /// Internal: materialize `Indicate` as a temporary scale-up and color highlight.
    ///
    /// Indicate is symmetric: the object grows to `scale_factor`, highlights, then
    /// shrinks back to its original scale/color over the second half of the duration.
    /// We split it into two consecutive clips so the final state matches the initial
    /// state and subsequent animations start from the correct baseline.
    fn play_indicate_internal(&mut self, anim: AnimationBuilder, parent_track: TrackId) {
        let (highlight_color, scale_factor) = match anim.anim_type {
            AnimationType::Indicate {
                color,
                scale_factor,
            } => (color, scale_factor),
            _ => unreachable!(),
        };

        // Collect target ids: root target plus all child spans for coloring.
        let items: Vec<ObjectId> = {
            let state = match self.states.get(anim.target) {
                Some(s) => s,
                None => {
                    bevy::prelude::warn!(
                        "Attempted to Indicate unregistered Mobject: {:?}",
                        anim.target
                    );
                    return;
                }
            };
            if state.child_spans.is_empty() {
                vec![anim.target]
            } else {
                state.child_spans.iter().map(|(id, _, _)| *id).collect()
            }
        };

        let half = anim.duration * 0.5;

        // 1. Scale up on root target (first half), then scale back down (second half)
        let root_state = match self.states.get(anim.target) {
            Some(s) => s,
            None => return,
        };
        let scale_from = root_state.transform.scale;
        let scale_to = scale_from * scale_factor;

        self.timeline.add_clip(
            parent_track,
            self.current_time,
            half,
            ClipPayload::Animation(AnimationSpec {
                target: anim.target,
                lens: PropertyLensSpec::Scale {
                    from: scale_from,
                    to: scale_to,
                },
                rate_func: gaanim_math::RateFunc::Linear,
                label: self.current_label.clone(),
            }),
        );
        self.timeline.add_clip(
            parent_track,
            self.current_time + half,
            half,
            ClipPayload::Animation(AnimationSpec {
                target: anim.target,
                lens: PropertyLensSpec::Scale {
                    from: scale_to,
                    to: scale_from,
                },
                rate_func: gaanim_math::RateFunc::Linear,
                label: self.current_label.clone(),
            }),
        );

        // 2. Color highlight on children (if requested) — highlight then revert
        if let Some(color) = highlight_color {
            for item_id in &items {
                if let Some(state) = self.states.get(*item_id) {
                    if let Some(Brush::Solid(c)) = &state.fill {
                        self.timeline.add_clip(
                            parent_track,
                            self.current_time,
                            half,
                            ClipPayload::Animation(AnimationSpec {
                                target: *item_id,
                                lens: PropertyLensSpec::FillColor {
                                    from: *c,
                                    to: color,
                                },
                                rate_func: gaanim_math::RateFunc::Linear,
                                label: self.current_label.clone(),
                            }),
                        );
                        self.timeline.add_clip(
                            parent_track,
                            self.current_time + half,
                            half,
                            ClipPayload::Animation(AnimationSpec {
                                target: *item_id,
                                lens: PropertyLensSpec::FillColor {
                                    from: color,
                                    to: *c,
                                },
                                rate_func: gaanim_math::RateFunc::Linear,
                                label: self.current_label.clone(),
                            }),
                        );
                    }
                    if let Some(Brush::Solid(c)) = &state.stroke.brush {
                        self.timeline.add_clip(
                            parent_track,
                            self.current_time,
                            half,
                            ClipPayload::Animation(AnimationSpec {
                                target: *item_id,
                                lens: PropertyLensSpec::StrokeColor {
                                    from: *c,
                                    to: color,
                                },
                                rate_func: gaanim_math::RateFunc::Linear,
                                label: self.current_label.clone(),
                            }),
                        );
                        self.timeline.add_clip(
                            parent_track,
                            self.current_time + half,
                            half,
                            ClipPayload::Animation(AnimationSpec {
                                target: *item_id,
                                lens: PropertyLensSpec::StrokeColor {
                                    from: color,
                                    to: *c,
                                },
                                rate_func: gaanim_math::RateFunc::Linear,
                                label: self.current_label.clone(),
                            }),
                        );
                    }
                }
            }
        }
    }

    fn play_fade_transform_internal(&mut self, anim: AnimationBuilder, parent_track: TrackId) {
        let target = match &anim.anim_type {
            AnimationType::FadeTransform { target } => *target,
            _ => return,
        };

        {
            let source_state = match self.states.get(anim.target) {
                Some(s) => s,
                None => return,
            };
            let from = source_state.opacity;
            self.timeline.add_clip(
                parent_track,
                self.current_time,
                anim.duration,
                ClipPayload::Animation(AnimationSpec {
                    target: anim.target,
                    lens: PropertyLensSpec::Opacity { from, to: 0.0 },
                    rate_func: anim.rate_func.clone(),
                    label: self.current_label.clone(),
                }),
            );
            if let Some(source_state) = self.states.get_mut(anim.target) {
                source_state.opacity = 0.0;
            }
        }

        {
            let target_state = match self.states.get(target) {
                Some(s) => s,
                None => return,
            };
            let target_opacity = target_state.opacity;
            let target_entity = target_state.entity;
            self.timeline.add_clip(
                parent_track,
                self.current_time,
                anim.duration,
                ClipPayload::Animation(AnimationSpec {
                    target,
                    lens: PropertyLensSpec::Opacity { from: 0.0, to: target_opacity },
                    rate_func: anim.rate_func.clone(),
                    label: self.current_label.clone(),
                }),
            );
            if let Some(target_state) = self.states.get_mut(target) {
                target_state.opacity = target_opacity;
            }
            self.commands
                .entity(target_entity)
                .insert(Opacity(0.0));
        }
    }

    fn play_wiggle_internal(&mut self, anim: AnimationBuilder, parent_track: TrackId) {
        let state = match self.states.get(anim.target) {
            Some(s) => s,
            None => return,
        };
        let origin = state.transform.translation;
        let num_wiggles = 6;
        let step = anim.duration / num_wiggles as f64;
        let amplitude = 5.0;

        for i in 0..num_wiggles {
            let dir = if i % 2 == 0 { 1.0_f64 } else { -1.0_f64 };
            let offset_x = if i == num_wiggles - 1 { 0.0 } else { dir * amplitude };
            let from_x = if i == 0 { origin.x } else { origin.x - dir * amplitude };
            let to_x = origin.x + offset_x;

            self.timeline.add_clip(
                parent_track,
                self.current_time + i as f64 * step,
                step,
                ClipPayload::Animation(AnimationSpec {
                    target: anim.target,
                    lens: PropertyLensSpec::Translation {
                        from: gaanim_core::glam::DVec3::new(from_x, origin.y, origin.z),
                        to: gaanim_core::glam::DVec3::new(to_x, origin.y, origin.z),
                    },
                    rate_func: gaanim_math::RateFunc::Linear,
                    label: self.current_label.clone(),
                }),
            );
        }
    }

    fn play_grow_from_point_internal(&mut self, anim: AnimationBuilder, parent_track: TrackId) {
        let (px, py) = match &anim.anim_type {
            AnimationType::GrowFromPoint { px, py } => (*px, *py),
            _ => return,
        };

        let state = match self.states.get_mut(anim.target) {
            Some(s) => s,
            None => return,
        };

        let target_scale = state.transform.scale;
        let target_pos = state.transform.translation;

        let from = gaanim_core::glam::DVec3::ZERO;
        state.transform.scale = from;
        let mut temp_transform = state.transform;
        temp_transform.scale = from;
        temp_transform.translation = gaanim_core::glam::DVec3::new(px, py, 0.0);
        self.commands.entity(state.entity).insert(temp_transform);

        self.timeline.add_clip(
            parent_track,
            self.current_time,
            anim.duration,
            ClipPayload::Animation(AnimationSpec {
                target: anim.target,
                lens: PropertyLensSpec::Scale { from, to: target_scale },
                rate_func: anim.rate_func.clone(),
                label: self.current_label.clone(),
            }),
        );

        self.timeline.add_clip(
            parent_track,
            self.current_time,
            anim.duration,
            ClipPayload::Animation(AnimationSpec {
                target: anim.target,
                lens: PropertyLensSpec::Translation {
                    from: gaanim_core::glam::DVec3::new(px, py, 0.0),
                    to: target_pos,
                },
                rate_func: anim.rate_func.clone(),
                label: self.current_label.clone(),
            }),
        );
    }

    fn play_grow_from_edge_internal(&mut self, anim: AnimationBuilder, parent_track: TrackId) {
        let direction = match &anim.anim_type {
            AnimationType::GrowFromEdge { direction } => direction.clone(),
            _ => return,
        };

        let state = match self.states.get_mut(anim.target) {
            Some(s) => s,
            None => return,
        };

        let target_scale = state.transform.scale;
        let target_pos = state.transform.translation;
        let bounds = state.bounds;

        let (edge_lx, edge_ly) = match direction.as_str() {
            "up" | "top" => (0.0, bounds.max.y),
            "down" | "bottom" => (0.0, bounds.min.y),
            "left" => (bounds.min.x, 0.0),
            "right" => (bounds.max.x, 0.0),
            _ => (0.0, 0.0),
        };
        let edge_world = target_pos
            + gaanim_core::glam::DVec3::new(edge_lx, edge_ly, 0.0);

        let from = gaanim_core::glam::DVec3::ZERO;
        state.transform.scale = from;
        let mut temp_transform = state.transform;
        temp_transform.scale = from;
        temp_transform.translation = edge_world;
        self.commands.entity(state.entity).insert(temp_transform);

        self.timeline.add_clip(
            parent_track,
            self.current_time,
            anim.duration,
            ClipPayload::Animation(AnimationSpec {
                target: anim.target,
                lens: PropertyLensSpec::Scale { from, to: target_scale },
                rate_func: anim.rate_func.clone(),
                label: self.current_label.clone(),
            }),
        );

        self.timeline.add_clip(
            parent_track,
            self.current_time,
            anim.duration,
            ClipPayload::Animation(AnimationSpec {
                target: anim.target,
                lens: PropertyLensSpec::Translation {
                    from: edge_world,
                    to: target_pos,
                },
                rate_func: anim.rate_func.clone(),
                label: self.current_label.clone(),
            }),
        );
    }

    fn play_draw_border_then_fill_internal(&mut self, anim: AnimationBuilder, parent_track: TrackId) {
        let state = match self.states.get(anim.target) {
            Some(s) => s,
            None => return,
        };
        let entity = state.entity;

        let fill_color = state
            .fill
            .as_ref()
            .and_then(extract_brush_color)
            .unwrap_or(Color::WHITE);
        let stroke_width = 2.0;

        if state.stroke.brush.is_none() {
            let new_stroke = StrokeBrush::new(fill_color, stroke_width);
            self.commands
                .entity(entity)
                .insert(new_stroke.clone());
            if let Some(s) = self.states.get_mut(anim.target) {
                s.stroke = new_stroke;
            }
        }

        let draw_duration = anim.duration * 0.6;
        let fill_duration = anim.duration * 0.4;
        let min_step = 1e-6_f64;

        self.timeline.add_clip(
            parent_track,
            self.current_time,
            min_step,
            ClipPayload::Animation(AnimationSpec {
                target: anim.target,
                lens: PropertyLensSpec::FillDrawProgress { from: 0.0, to: 0.0 },
                rate_func: anim.rate_func.clone(),
                label: self.current_label.clone(),
            }),
        );
        self.timeline.add_clip(
            parent_track,
            self.current_time,
            min_step,
            ClipPayload::Animation(AnimationSpec {
                target: anim.target,
                lens: PropertyLensSpec::PathCompletion { from: 0.0, to: 0.0 },
                rate_func: anim.rate_func.clone(),
                label: self.current_label.clone(),
            }),
        );

        self.commands
            .entity(entity)
            .insert(gaanim_animation::FillDrawProgress(0.0));
        self.commands.entity(entity).insert(gaanim_scene::components::Path2D(
            std::sync::Arc::new(gaanim_core::kurbo::BezPath::new()),
        ));

        self.timeline.add_clip(
            parent_track,
            self.current_time,
            draw_duration,
            ClipPayload::Animation(AnimationSpec {
                target: anim.target,
                lens: PropertyLensSpec::PathCompletion { from: 0.0, to: 1.0 },
                rate_func: anim.rate_func.clone(),
                label: self.current_label.clone(),
            }),
        );

        let fill_start = self.current_time + draw_duration;
        self.timeline.add_clip(
            parent_track,
            fill_start,
            fill_duration,
            ClipPayload::Animation(AnimationSpec {
                target: anim.target,
                lens: PropertyLensSpec::FillDrawProgress { from: 0.0, to: 1.0 },
                rate_func: anim.rate_func.clone(),
                label: self.current_label.clone(),
            }),
        );
    }

    fn play_flash_internal(&mut self, anim: AnimationBuilder, parent_track: TrackId) {
        let state = match self.states.get(anim.target) {
            Some(s) => s,
            None => return,
        };
        let original_opacity = state.opacity;
        let original_scale = state.transform.scale;

        let n_lines = match &anim.anim_type {
            AnimationType::Flash { n_lines, .. } => *n_lines,
            _ => 12,
        };
        let radius = match &anim.anim_type {
            AnimationType::Flash { radius, .. } => *radius,
            _ => 100.0,
        };
        let half = anim.duration * 0.5;

        // Fade out then back in (1.0 -> 0.0 -> 1.0)
        self.timeline.add_clip(
            parent_track,
            self.current_time,
            half,
            ClipPayload::Animation(AnimationSpec {
                target: anim.target,
                lens: PropertyLensSpec::Opacity { from: original_opacity, to: 0.0 },
                rate_func: gaanim_math::RateFunc::Smooth,
                label: self.current_label.clone(),
            }),
        );
        self.timeline.add_clip(
            parent_track,
            self.current_time + half,
            half,
            ClipPayload::Animation(AnimationSpec {
                target: anim.target,
                lens: PropertyLensSpec::Opacity { from: 0.0, to: original_opacity },
                rate_func: gaanim_math::RateFunc::Smooth,
                label: self.current_label.clone(),
            }),
        );

        // Quick scale pulse to amplify the "flash" effect.
        // Pulse up to scale_factor proportional to the number of lines for visual feedback.
        let scale_factor = 1.0 + (n_lines as f64 / 12.0) * 0.25;
        let scale_to = original_scale * scale_factor;
        self.timeline.add_clip(
            parent_track,
            self.current_time,
            half,
            ClipPayload::Animation(AnimationSpec {
                target: anim.target,
                lens: PropertyLensSpec::Scale { from: original_scale, to: scale_to },
                rate_func: gaanim_math::RateFunc::Smooth,
                label: self.current_label.clone(),
            }),
        );
        self.timeline.add_clip(
            parent_track,
            self.current_time + half,
            half,
            ClipPayload::Animation(AnimationSpec {
                target: anim.target,
                lens: PropertyLensSpec::Scale { from: scale_to, to: original_scale },
                rate_func: gaanim_math::RateFunc::Smooth,
                label: self.current_label.clone(),
            }),
        );

        if let Some(s) = self.states.get_mut(anim.target) {
            s.opacity = original_opacity;
            s.transform.scale = original_scale;
        }
        let _ = radius; // reserved for future "radial line" geometry
    }

    fn play_circumscribe_internal(&mut self, anim: AnimationBuilder, parent_track: TrackId) {
        let color = match &anim.anim_type {
            AnimationType::Circumscribe { color } => *color,
            _ => None,
        };

        let state = match self.states.get(anim.target) {
            Some(s) => s,
            None => return,
        };
        let original_opacity = state.opacity;
        let original_scale = state.transform.scale;

        let half = anim.duration * 0.5;

        // Scale up then back down (1.0 -> 1.1 -> 1.0)
        let scale_to = original_scale * 1.1;
        self.timeline.add_clip(
            parent_track,
            self.current_time,
            half,
            ClipPayload::Animation(AnimationSpec {
                target: anim.target,
                lens: PropertyLensSpec::Scale { from: original_scale, to: scale_to },
                rate_func: gaanim_math::RateFunc::Smooth,
                label: self.current_label.clone(),
            }),
        );
        self.timeline.add_clip(
            parent_track,
            self.current_time + half,
            half,
            ClipPayload::Animation(AnimationSpec {
                target: anim.target,
                lens: PropertyLensSpec::Scale { from: scale_to, to: original_scale },
                rate_func: gaanim_math::RateFunc::Smooth,
                label: self.current_label.clone(),
            }),
        );

        // Optional fill color highlight (like Indicate but without scale_factor difference).
        if let Some(c) = color
            && let Some(Brush::Solid(current)) = &state.fill {
                self.timeline.add_clip(
                    parent_track,
                    self.current_time,
                    half,
                    ClipPayload::Animation(AnimationSpec {
                        target: anim.target,
                        lens: PropertyLensSpec::FillColor { from: *current, to: c },
                        rate_func: gaanim_math::RateFunc::Linear,
                        label: self.current_label.clone(),
                    }),
                );
                self.timeline.add_clip(
                    parent_track,
                    self.current_time + half,
                    half,
                    ClipPayload::Animation(AnimationSpec {
                        target: anim.target,
                        lens: PropertyLensSpec::FillColor { from: c, to: *current },
                        rate_func: gaanim_math::RateFunc::Linear,
                        label: self.current_label.clone(),
                    }),
                );
            }

        let _ = original_opacity; // reserved if we want to fade in/out later

        if let Some(s) = self.states.get_mut(anim.target) {
            s.transform.scale = original_scale;
        }
    }

    /// Internal: schedule a `MoveAlongPath` animation. The target's
    /// translation is sampled from the Bézier path at the eased `t`
    /// (parametric, not arc-length uniform). Updates the tracked state
    /// so the final translation equals `path(1.0)`.
    fn play_move_along_path_internal(&mut self, anim: AnimationBuilder, parent_track: TrackId) {
        let path = match &anim.anim_type {
            AnimationType::MoveAlongPath { path } => path.clone(),
            _ => unreachable!(),
        };

        // Resolve and persist the final translation so subsequent
        // animations build on top of the new position.
        let end_point = gaanim_math::get_point_at_alpha(&path, 1.0);
        let end_translation = gaanim_core::glam::DVec3::new(end_point.x, end_point.y, 0.0);

        if let Some(state) = self.states.get_mut(anim.target) {
            state.transform.translation = end_translation;
        }

        self.timeline.add_clip(
            parent_track,
            self.current_time,
            anim.duration,
            ClipPayload::Animation(AnimationSpec {
                target: anim.target,
                lens: PropertyLensSpec::PathFollow { path },
                rate_func: anim.rate_func.clone(),
                label: self.current_label.clone(),
            }),
        );
    }

    /// Internal: schedule a `GrowArrow` animation as a Create-style
    /// outline draw followed by a brief scale "punch" that emphasizes
    /// the arrowhead's arrival at the end of the trajectory.
    fn play_grow_arrow_internal(&mut self, anim: AnimationBuilder, parent_track: TrackId) {
        if self.states.get(anim.target).is_none() {
            bevy::prelude::warn!(
                "Attempted to GrowArrow unregistered Mobject: {:?}",
                anim.target
            );
            return;
        }

        // (B) Set initial value via deferred commands to avoid first-frame
        // flash: insert FillDrawProgress(0.0) AND an empty Path2D so the
        // renderer sees "no fill, no path" before the timeline runs.
        if let Some(state) = self.states.get(anim.target) {
            self.commands
                .entity(state.entity)
                .insert(gaanim_animation::FillDrawProgress(0.0));
            self.commands
                .entity(state.entity)
                .insert(gaanim_scene::components::Path2D(
                    std::sync::Arc::new(gaanim_core::kurbo::BezPath::new()),
                ));
        }

        // Phase 1: 70% of duration draws the outline (PathCompletion
        // 0 -> 1). The fill is held hidden during the draw, then
        // cross-fades in over the last 30% to give the arrowhead
        // emphasis.
        let draw_duration = anim.duration * 0.7;
        let fill_duration = anim.duration * 0.3;

        // Hold the fill at 0 during the draw phase so the outline-only
        // stage is visible.
        self.timeline.add_clip(
            parent_track,
            self.current_time,
            draw_duration,
            ClipPayload::Animation(AnimationSpec {
                target: anim.target,
                lens: PropertyLensSpec::FillDrawProgress {
                    from: 0.0,
                    to: 0.0,
                },
                rate_func: gaanim_math::RateFunc::Linear,
                label: self.current_label.clone(),
            }),
        );

        // Trace the outline.
        self.timeline.add_clip(
            parent_track,
            self.current_time,
            draw_duration,
            ClipPayload::Animation(AnimationSpec {
                target: anim.target,
                lens: PropertyLensSpec::PathCompletion {
                    from: 0.0,
                    to: 1.0,
                },
                rate_func: anim.rate_func.clone(),
                label: self.current_label.clone(),
            }),
        );

        // Cross-fade the fill in over the last segment, then a brief
        // scale punch (1.0 -> 1.15 -> 1.0) to highlight the arrowhead.
        self.timeline.add_clip(
            parent_track,
            self.current_time + draw_duration,
            fill_duration,
            ClipPayload::Animation(AnimationSpec {
                target: anim.target,
                lens: PropertyLensSpec::FillDrawProgress {
                    from: 0.0,
                    to: 1.0,
                },
                rate_func: gaanim_math::RateFunc::Smooth,
                label: self.current_label.clone(),
            }),
        );

        // Brief scale punch on the arrowhead (25% of the fill phase
        // each way). Yields a quick "pop" as the fill reveals.
        let punch_half = fill_duration * 0.5;
        let original_scale = self
            .states
            .get(anim.target)
            .map(|s| s.transform.scale)
            .unwrap_or(gaanim_core::glam::DVec3::ONE);
        let scale_to = original_scale * 1.15;
        self.timeline.add_clip(
            parent_track,
            self.current_time + draw_duration,
            punch_half,
            ClipPayload::Animation(AnimationSpec {
                target: anim.target,
                lens: PropertyLensSpec::Scale {
                    from: original_scale,
                    to: scale_to,
                },
                rate_func: gaanim_math::RateFunc::Smooth,
                label: self.current_label.clone(),
            }),
        );
        self.timeline.add_clip(
            parent_track,
            self.current_time + draw_duration + punch_half,
            punch_half,
            ClipPayload::Animation(AnimationSpec {
                target: anim.target,
                lens: PropertyLensSpec::Scale {
                    from: scale_to,
                    to: original_scale,
                },
                rate_func: gaanim_math::RateFunc::Smooth,
                label: self.current_label.clone(),
            }),
        );
    }

    /// Creates a hierarchical group of Mobjects.
    ///
    /// The group is a Mobject itself (with a GroupMarker component).
    /// The children are reparented under the group using Bevy's hierarchy,
    /// and their local transforms are adjusted relative to the group's center
    /// to avoid any spatial offset jumps.
    pub fn group(&mut self, children: &[MobjectRef]) -> MobjectRef {
        debug_assert!(!children.is_empty(), "Cannot create an empty group");
        let id = self.next_id();
        
        // 1. Calculate the collective bounds of the children in world space
        let mut union_bounds = Bounds3D::default();
        let mut has_bounds = false;
        
        for child in children {
            if let Some(state) = self.states.get(child.id) {
                // child world bounds = child local bounds transformed by its TRUE world transform
                let world_transform = self.get_world_transform(child.id);
                let world_bounds = state.bounds.transform_2d(&world_transform.to_affine_2d());
                if !has_bounds {
                    union_bounds = world_bounds;
                    has_bounds = true;
                } else {
                    union_bounds = union_bounds.union(&world_bounds);
                }
            }
        }
        
        // 2. Set the group's transform translation to the center of the union bounds
        let center = union_bounds.center();
        let group_transform = SpatialTransform::new_2d(center.x, center.y);
        
        // 3. Spawn the group entity with GroupMarker, Opacity, WorldBounds etc.
        let group_entity = self.commands.spawn((
            GroupMarker,
            MobjectId(id),
            group_transform,
            gaanim_math::GlobalSpatialTransform::from_local(&group_transform),
            Opacity(1.0),
            gaanim_scene::GlobalOpacity(1.0),
            LocalBounds(Bounds3D::new_2d(
                union_bounds.min.x - center.x,
                union_bounds.min.y - center.y,
                union_bounds.max.x - center.x,
                union_bounds.max.y - center.y,
            )),
            WorldBounds(union_bounds),
            gaanim_scene::RenderOrder::default(),
            Visible,
            FillBrush::transparent(),
            StrokeBrush::transparent(),
        )).id();

        self.tag_entity(group_entity);

        // 4. Reparent children and adjust their local transforms
        let inv_group_affine = group_transform.to_affine_2d().inverse();
        let mut child_ids = Vec::new();
        
        for child in children {
            child_ids.push(child.id);
            let child_world = self.get_world_transform(child.id);
            if let Some(state) = self.states.get_mut(child.id) {
                // child_local = group_inv * child_world
                let child_local_affine = inv_group_affine * child_world.to_affine_2d();
                let child_local = SpatialTransform::from_affine_2d(&child_local_affine);

                state.transform = child_local;
                state.parent = Some(id); // Track parent for world transform calculation

                self.commands.entity(state.entity)
                    .set_parent_in_place(group_entity)
                    .insert(child_local);
            }
        }
        
        // 5. Ensure tracks exist for group and children so the timeline can
        //    display the group hierarchy (group → children).
        self.ensure_track(id);
        for &child_id in &child_ids {
            self.ensure_track(child_id);
        }

        // 6. Store group state
        let group_state = MobjectState {
            bounds: Bounds3D::new_2d(
                union_bounds.min.x - center.x,
                union_bounds.min.y - center.y,
                union_bounds.max.x - center.x,
                union_bounds.max.y - center.y,
            ),
            transform: group_transform,
            opacity: 1.0,
            fill: None,
            stroke: gaanim_scene::StrokeBrush::transparent(),
            entity: group_entity,
            child_spans: Vec::new(),
            children: child_ids,
            parent: None,
        };
        self.states.insert(id, group_state);
        self.mobject_names.insert(id, format!("Group ({} children)", children.len()));
        
        MobjectRef { id }
    }

    /// Discharges all children from the group and despawns the group container entity.
    ///
    /// The children's local transforms are adjusted to world space so they remain in their
    /// absolute positions without any jumps.
    pub fn ungroup(&mut self, group: MobjectRef) {
        let (children_ids, group_transform, group_parent) = if let Some(state) = self.states.get(group.id) {
            (state.children.clone(), state.transform, state.parent)
        } else {
            return;
        };

        // Capture group fill/stroke for MobjectState propagation to children
        let group_fill_clone = self.states.get(group.id).map(|s| s.fill.clone());
        let group_stroke_clone = self.states.get(group.id).map(|s| s.stroke.clone());

        let group_affine = group_transform.to_affine_2d();

        for child_id in children_ids.clone() {
            let child_world = if let Some(child_state) = self.states.get(child_id) {
                let child_local = child_state.transform;
                let child_world_affine = group_affine * child_local.to_affine_2d();
                SpatialTransform::from_affine_2d(&child_world_affine)
            } else {
                continue;
            };

            if let Some(state) = self.states.get_mut(child_id) {
                // Propagate fill/stroke to MobjectState for subsequent construction ops
                if let Some(ref f) = group_fill_clone {
                    state.fill = f.clone();
                }
                if let Some(ref s) = group_stroke_clone {
                    state.stroke = s.clone();
                }
                state.transform = child_world;
                state.parent = group_parent;
            }
        }

        // No ECS commands here — the timeline Ungroup clip handles all hierarchy
        // mutations during playback. This keeps parent-child relationships intact
        // in ECS so that group-level animations (shift, rotate, etc.) affect all
        // children, and style propagation works before the ungroup time.

        // Pre-compute each child's world-space transform so the runtime ungroup
        // clip can re-apply them on subsequent seek frames (after the group entity
        // has been despawned and animation clips would otherwise overwrite the
        // correct world positions with stale local-space values).
        let children_world_transforms: Vec<(ObjectId, SpatialTransform)> = children_ids
            .iter()
            .filter_map(|&child_id| {
                self.states
                    .get(child_id)
                    .map(|state| (child_id, state.transform))
            })
            .collect();

        self.timeline.add_clip(
            self.default_track,
            self.current_time,
            0.0,
            ClipPayload::Ungroup {
                group: group.id,
                children: children_ids,
                group_parent,
                group_transform,
                children_world_transforms,
            },
        );

        self.states.remove(group.id);
        self.mobject_names.remove(&group.id);
    }

    /// Spawns a ValueTracker (FloatSignal) with the given initial value.
    pub fn value_tracker(&mut self, initial: f64) -> ValueTrackerRef {
        let id = self.next_id();
        let entity = self.commands.spawn((
            gaanim_scene::MobjectId(id),
            gaanim_animation::signals::FloatSignal::new(initial),
        )).id();
        self.tag_entity(entity);

        let state = MobjectState {
            bounds: Bounds3D::default(),
            transform: SpatialTransform::default(),
            opacity: 1.0,
            fill: None,
            stroke: StrokeBrush::default(),
            entity,
            child_spans: Vec::new(),
            children: Vec::new(),
            parent: None,
        };
        self.states.insert(id, state);
        self.float_signals.insert(id, initial);

        ValueTrackerRef { id }
    }

    /// Spawns a circle primitive.
    pub fn circle(&mut self, radius: f64) -> MobjectSpawnBuilder<'_, 'w, 's, 'a> {
        let id = self.next_id();
        let bundle = gaanim_objects::primitives::circle(id, radius);
        MobjectSpawnBuilder {
            builder: self,
            id,
            bundle,
            parent_entity: None,
        }
    }

    /// Spawns a rectangle primitive.
    pub fn rectangle(&mut self, width: f64, height: f64) -> MobjectSpawnBuilder<'_, 'w, 's, 'a> {
        let id = self.next_id();
        let bundle = gaanim_objects::primitives::rectangle(id, width, height);
        MobjectSpawnBuilder {
            builder: self,
            id,
            bundle,
            parent_entity: None,
        }
    }

    /// Spawns a rounded rectangle primitive.
    pub fn rounded_rect(
        &mut self,
        width: f64,
        height: f64,
        corner_radius: f64,
    ) -> MobjectSpawnBuilder<'_, 'w, 's, 'a> {
        let id = self.next_id();
        let bundle = gaanim_objects::primitives::rounded_rect(id, width, height, corner_radius);
        MobjectSpawnBuilder {
            builder: self,
            id,
            bundle,
            parent_entity: None,
        }
    }

    /// Spawns a line segment primitive.
    pub fn line(
        &mut self,
        start: kurbo::Point,
        end: kurbo::Point,
    ) -> MobjectSpawnBuilder<'_, 'w, 's, 'a> {
        let id = self.next_id();
        let bundle = gaanim_objects::primitives::line(id, start, end);
        MobjectSpawnBuilder {
            builder: self,
            id,
            bundle,
            parent_entity: None,
        }
    }

    /// Spawns a custom closed polygon primitive.
    pub fn polygon(&mut self, points: &[kurbo::Point]) -> MobjectSpawnBuilder<'_, 'w, 's, 'a> {
        let id = self.next_id();
        let bundle = gaanim_objects::primitives::polygon(id, points);
        MobjectSpawnBuilder {
            builder: self,
            id,
            bundle,
            parent_entity: None,
        }
    }

    /// Spawns a line tangent to a polyline `curve` at fractional
    /// position `t` in `[0.0, 1.0]`. The line has half-length `length`
    /// on either side of the tangent point. Falls back to a line of
    /// zero length (origin) if the curve is degenerate.
    pub fn tangent_line(
        &mut self,
        curve: &[kurbo::Point],
        t: f64,
        length: f64,
    ) -> MobjectSpawnBuilder<'_, 'w, 's, 'a> {
        let id = self.next_id();
        let bundle = gaanim_objects::primitives::tangent_line(id, curve, t, length)
            .unwrap_or_else(|| gaanim_objects::primitives::line(id, kurbo::Point::ORIGIN, kurbo::Point::ORIGIN));
        MobjectSpawnBuilder {
            builder: self,
            id,
            bundle,
            parent_entity: None,
        }
    }

    /// Spawns a Cartesian `NumberPlane` with grid + axes.
    /// `x_range`, `y_range` are `(min, max, step)` tuples.
    pub fn number_plane(
        &mut self,
        x_range: (f64, f64, f64),
        y_range: (f64, f64, f64),
        axis_stroke: f64,
        grid_stroke: f64,
    ) -> MobjectSpawnBuilder<'_, 'w, 's, 'a> {
        let id = self.next_id();
        let bundle =
            gaanim_objects::primitives::number_plane(id, x_range, y_range, axis_stroke, grid_stroke);
        MobjectSpawnBuilder {
            builder: self,
            id,
            bundle,
            parent_entity: None,
        }
    }

    /// Spawns a symmetric star primitive.
    pub fn star(
        &mut self,
        n_points: u32,
        outer_radius: f64,
        inner_radius: f64,
    ) -> MobjectSpawnBuilder<'_, 'w, 's, 'a> {
        let id = self.next_id();
        let bundle = gaanim_objects::primitives::star(id, n_points, outer_radius, inner_radius);
        MobjectSpawnBuilder {
            builder: self,
            id,
            bundle,
            parent_entity: None,
        }
    }

    /// Spawns an ellipse primitive.
    pub fn ellipse(&mut self, rx: f64, ry: f64) -> MobjectSpawnBuilder<'_, 'w, 's, 'a> {
        let id = self.next_id();
        let bundle = gaanim_objects::primitives::ellipse(id, rx, ry);
        MobjectSpawnBuilder {
            builder: self,
            id,
            bundle,
            parent_entity: None,
        }
    }

    /// Spawns a tiny dot primitive.
    pub fn dot(&mut self, radius: f64) -> MobjectSpawnBuilder<'_, 'w, 's, 'a> {
        let id = self.next_id();
        let bundle = gaanim_objects::primitives::dot(id, radius);
        MobjectSpawnBuilder {
            builder: self,
            id,
            bundle,
            parent_entity: None,
        }
    }

    /// Spawns a square primitive.
    pub fn square(&mut self, side_length: f64) -> MobjectSpawnBuilder<'_, 'w, 's, 'a> {
        let id = self.next_id();
        let bundle = gaanim_objects::primitives::square(id, side_length);
        MobjectSpawnBuilder {
            builder: self,
            id,
            bundle,
            parent_entity: None,
        }
    }

    /// Spawns a checkmark primitive.
    pub fn checkmark(&mut self, size: f64) -> MobjectSpawnBuilder<'_, 'w, 's, 'a> {
        let id = self.next_id();
        let bundle = gaanim_objects::primitives::checkmark(id, size);
        MobjectSpawnBuilder {
            builder: self,
            id,
            bundle,
            parent_entity: None,
        }
    }

    /// Spawns a directional arrow primitive.
    pub fn arrow(
        &mut self,
        start: kurbo::Point,
        end: kurbo::Point,
    ) -> MobjectSpawnBuilder<'_, 'w, 's, 'a> {
        let id = self.next_id();
        let bundle = gaanim_objects::primitives::arrow(id, start, end);
        MobjectSpawnBuilder {
            builder: self,
            id,
            bundle,
            parent_entity: None,
        }
    }

    /// Spawns a regular polygon primitive.
    pub fn regular_polygon(
        &mut self,
        n_sides: u32,
        radius: f64,
    ) -> MobjectSpawnBuilder<'_, 'w, 's, 'a> {
        let id = self.next_id();
        let bundle = gaanim_objects::primitives::regular_polygon(id, n_sides, radius);
        MobjectSpawnBuilder {
            builder: self,
            id,
            bundle,
            parent_entity: None,
        }
    }

    /// Spawns a dashed line primitive.
    pub fn dashed_line(
        &mut self,
        start: kurbo::Point,
        end: kurbo::Point,
        dash_length: f64,
        gap_length: f64,
    ) -> MobjectSpawnBuilder<'_, 'w, 's, 'a> {
        let id = self.next_id();
        let bundle =
            gaanim_objects::primitives::dashed_line(id, start, end, dash_length, gap_length);
        MobjectSpawnBuilder {
            builder: self,
            id,
            bundle,
            parent_entity: None,
        }
    }

    /// Spawns a circular/elliptical arc segment primitive.
    pub fn arc(
        &mut self,
        center: kurbo::Point,
        radii: kurbo::Vec2,
        start_angle: f64,
        sweep_angle: f64,
        x_rotation: f64,
    ) -> MobjectSpawnBuilder<'_, 'w, 's, 'a> {
        let id = self.next_id();
        let bundle = gaanim_objects::primitives::arc(id, center, radii, start_angle, sweep_angle, x_rotation);
        MobjectSpawnBuilder {
            builder: self,
            id,
            bundle,
            parent_entity: None,
        }
    }

    /// Spawns a smooth arc between two points with a given deflection angle.
    pub fn arc_between_points(
        &mut self,
        start: kurbo::Point,
        end: kurbo::Point,
        angle: f64,
    ) -> MobjectSpawnBuilder<'_, 'w, 's, 'a> {
        let id = self.next_id();
        let bundle = gaanim_objects::primitives::arc_between_points(id, start, end, angle);
        MobjectSpawnBuilder {
            builder: self,
            id,
            bundle,
            parent_entity: None,
        }
    }

    /// Spawns a double-headed arrow primitive.
    pub fn double_arrow(
        &mut self,
        start: kurbo::Point,
        end: kurbo::Point,
        head_len: Option<f64>,
        head_width: Option<f64>,
    ) -> MobjectSpawnBuilder<'_, 'w, 's, 'a> {
        let id = self.next_id();
        let bundle = gaanim_objects::primitives::double_arrow(id, start, end, head_len, head_width);
        MobjectSpawnBuilder {
            builder: self,
            id,
            bundle,
            parent_entity: None,
        }
    }

    /// Spawns a circular sector (pie slice) primitive.
    pub fn sector(
        &mut self,
        center: kurbo::Point,
        radius: f64,
        start_angle: f64,
        sweep_angle: f64,
    ) -> MobjectSpawnBuilder<'_, 'w, 's, 'a> {
        let id = self.next_id();
        let bundle = gaanim_objects::primitives::sector(id, center, radius, start_angle, sweep_angle);
        MobjectSpawnBuilder {
            builder: self,
            id,
            bundle,
            parent_entity: None,
        }
    }

    /// Spawns an annulus (ring/donut) primitive.
    pub fn annulus(
        &mut self,
        outer_radius: f64,
        inner_radius: f64,
    ) -> MobjectSpawnBuilder<'_, 'w, 's, 'a> {
        let id = self.next_id();
        let bundle = gaanim_objects::primitives::annulus(id, outer_radius, inner_radius);
        MobjectSpawnBuilder {
            builder: self,
            id,
            bundle,
            parent_entity: None,
        }
    }

    /// Spawns a surrounding rectangle (no fill, stroke outline).
    pub fn surrounding_rectangle(
        &mut self,
        width: f64,
        height: f64,
        corner_radius: f64,
    ) -> MobjectSpawnBuilder<'_, 'w, 's, 'a> {
        let id = self.next_id();
        let bundle =
            gaanim_objects::primitives::surrounding_rectangle(id, width, height, corner_radius);
        MobjectSpawnBuilder {
            builder: self,
            id,
            bundle,
            parent_entity: None,
        }
    }

    /// Spawns a background rectangle (filled, low z-index).
    pub fn background_rectangle(
        &mut self,
        width: f64,
        height: f64,
    ) -> MobjectSpawnBuilder<'_, 'w, 's, 'a> {
        let id = self.next_id();
        let bundle = gaanim_objects::primitives::background_rectangle(id, width, height);
        MobjectSpawnBuilder {
            builder: self,
            id,
            bundle,
            parent_entity: None,
        }
    }

    /// Spawns a cross (X) symbol primitive.
    pub fn cross(&mut self, size: f64) -> MobjectSpawnBuilder<'_, 'w, 's, 'a> {
        let id = self.next_id();
        let bundle = gaanim_objects::primitives::cross(id, size);
        MobjectSpawnBuilder {
            builder: self,
            id,
            bundle,
            parent_entity: None,
        }
    }

    /// Spawns a right-angle indicator primitive.
    pub fn right_angle(
        &mut self,
        arm_length: f64,
    ) -> MobjectSpawnBuilder<'_, 'w, 's, 'a> {
        let id = self.next_id();
        let bundle = gaanim_objects::primitives::right_angle(id, arm_length);
        MobjectSpawnBuilder {
            builder: self,
            id,
            bundle,
            parent_entity: None,
        }
    }

    /// Spawns a mobject from a list of pre-computed closed polylines.
    /// Used by boolean-operation replay to materialize the result geometry.
    pub fn polylines(
        &mut self,
        rings: Vec<Vec<kurbo::Point>>,
    ) -> MobjectSpawnBuilder<'_, 'w, 's, 'a> {
        let id = self.next_id();
        let bundle = gaanim_objects::primitives::polylines(id, &rings);
        MobjectSpawnBuilder {
            builder: self,
            id,
            bundle,
            parent_entity: None,
        }
    }

    /// Compiles a Typst markup or math formula into a hierarchy of vector Mobjects.
    ///
    /// `text_font` and `math_font` are optional font family names. When `None`,
    /// Typst uses its bundled defaults (LibertinusSerif for text, NewCMMath for math).
    ///
    /// `text_size` and `math_size` are optional sizes in **pt**. When `None`, Typst
    /// uses its default (11pt). For a comfortable canvas size, 24pt–32pt is recommended.
    ///
    /// Returns a reference to the parent container of the compiled formula.
    pub fn typst(
        &mut self,
        source: &str,
        is_math: bool,
        text_font: Option<&str>,
        math_font: Option<&str>,
        text_size: Option<f64>,
        math_size: Option<f64>,
    ) -> MobjectRef {
        let parent_id = self.next_id();
        let fill = Some(gaanim_core::peniko::Brush::Solid(
            gaanim_core::peniko::Color::BLACK,
        ));
        let stroke = gaanim_scene::StrokeBrush::transparent();

        // Extract mutable counter separately to avoid borrow conflict with `self.commands`.
        let id_counter = &mut self.id_counter;
        let next_id_fn = move || {
            let id = gaanim_core::ObjectId::from_parts(*id_counter, 1);
            *id_counter += 1;
            id
        };

        let mut child_spans = Vec::new();
        let (entity, bounds) = compile_typst_to_hierarchy(
            self.commands,
            self.font_registry,
            source,
            is_math,
            text_font,
            math_font,
            text_size,
            math_size,
            fill,
            stroke,
            parent_id,
            next_id_fn,
            &mut child_spans,
        );

        // Register each child in self.states so they can be styled and animated
        for (child_id, child_entity, _) in &child_spans {
            self.tag_entity(*child_entity);
            let child_state = MobjectState {
                bounds: Bounds3D::default(),
                transform: SpatialTransform::default(),
                opacity: 1.0,
                fill: Some(gaanim_core::peniko::Brush::Solid(
                    gaanim_core::peniko::Color::BLACK,
                )),
                stroke: gaanim_scene::StrokeBrush::transparent(),
                entity: *child_entity,
                child_spans: Vec::new(),
                children: Vec::new(),
                parent: None,
            };
            self.states.insert(*child_id, child_state);
        }

        self.tag_entity(entity);
        let state = MobjectState {
            bounds,
            transform: SpatialTransform::default(),
            opacity: 1.0,
            fill: Some(gaanim_core::peniko::Brush::Solid(
                gaanim_core::peniko::Color::BLACK,
            )),
            stroke: gaanim_scene::StrokeBrush::transparent(),
            entity,
            child_spans,
            children: Vec::new(),
            parent: None,
        };
        self.states.insert(parent_id, state);

        MobjectRef { id: parent_id }
    }

    /// Convenience wrapper for `typst` that uses explicit fonts for both text and math.
    pub fn typst_with_fonts(
        &mut self,
        source: &str,
        is_math: bool,
        text_font: &str,
        math_font: &str,
    ) -> MobjectRef {
        self.typst(
            source,
            is_math,
            Some(text_font),
            Some(math_font),
            None,
            None,
        )
    }

    /// Compiles a plain text string into a hierarchy of vector character Mobjects.
    ///
    /// Shapes the text using HarfBuzz (`rustybuzz`) and extracts outlines via `ttf-parser`.
    /// `font_family` is the font name (e.g. "Arial", "sans-serif").
    /// `font_size` is the text size in pixels/points.
    ///
    /// Returns a reference to the parent container of the text.
    pub fn text(&mut self, content: &str, font_family: &str, font_size: f64) -> MobjectRef {
        let parent_id = self.next_id();
        let fill = Some(gaanim_core::peniko::Brush::Solid(
            gaanim_core::peniko::Color::WHITE,
        ));
        let stroke = gaanim_scene::StrokeBrush::transparent();

        // Extract mutable counter separately to avoid borrow conflict with `self.commands`.
        let id_counter = &mut self.id_counter;
        let next_id_fn = move || {
            let id = gaanim_core::ObjectId::from_parts(*id_counter, 1);
            *id_counter += 1;
            id
        };

        let mut child_spans = Vec::new();
        let (entity, bounds) = match compile_text_to_hierarchy(
            self.commands,
            self.font_registry,
            content,
            font_family,
            font_size,
            fill.clone(),
            stroke.clone(),
            parent_id,
            next_id_fn,
            &mut child_spans,
        ) {
            Ok(res) => res,
            Err(e) => {
                bevy::prelude::error!("Text compilation failed: {}", e);
                let bounds = Bounds3D::default();
                let bundle = MobjectBundle::new(parent_id, kurbo::BezPath::new(), bounds);
                let entity = self.commands.spawn(bundle).id();
                (entity, bounds)
            }
        };

        // Register each child in self.states so they can be styled and animated
        for (child_id, child_entity, _) in &child_spans {
            self.tag_entity(*child_entity);
            let child_state = MobjectState {
                bounds: Bounds3D::default(),
                transform: SpatialTransform::default(),
                opacity: 1.0,
                fill: fill.clone(),
                stroke: stroke.clone(),
                entity: *child_entity,
                child_spans: Vec::new(),
                children: Vec::new(),
                parent: None,
            };
            self.states.insert(*child_id, child_state);
        }

        self.tag_entity(entity);
        let state = MobjectState {
            bounds,
            transform: SpatialTransform::default(),
            opacity: 1.0,
            fill: Some(gaanim_core::peniko::Brush::Solid(
                gaanim_core::peniko::Color::WHITE,
            )),
            stroke: gaanim_scene::StrokeBrush::transparent(),
            entity,
            child_spans,
            children: Vec::new(),
            parent: None,
        };
        self.states.insert(parent_id, state);

        MobjectRef { id: parent_id }
    }

    /// Spawns a vector text Mobject using the default styling of the requested `TextRole`.
    pub fn spawn_text(
        &mut self,
        content: &str,
        role: gaanim_text::prelude::TextRole,
    ) -> MobjectRef {
        let style = self
            .text_config
            .roles
            .get(&role)
            .cloned()
            .unwrap_or_else(|| gaanim_text::prelude::RoleStyle {
                font_family: "Arial".to_string(),
                size: 32.0,
                fill_color: gaanim_core::peniko::Color::WHITE,
            });

        let parent_id = self.next_id();
        let fill = Some(gaanim_core::peniko::Brush::Solid(style.fill_color));
        let stroke = gaanim_scene::StrokeBrush::transparent();

        let id_counter = &mut self.id_counter;
        let next_id_fn = move || {
            let id = gaanim_core::ObjectId::from_parts(*id_counter, 1);
            *id_counter += 1;
            id
        };

        let mut child_spans = Vec::new();
        let (entity, bounds) = match compile_text_to_hierarchy(
            self.commands,
            self.font_registry,
            content,
            &style.font_family,
            style.size,
            fill.clone(),
            stroke.clone(),
            parent_id,
            next_id_fn,
            &mut child_spans,
        ) {
            Ok(res) => res,
            Err(e) => {
                bevy::prelude::error!("Text compilation failed: {}", e);
                let bounds = Bounds3D::default();
                let bundle = MobjectBundle::new(parent_id, kurbo::BezPath::new(), bounds);
                let entity = self.commands.spawn(bundle).id();
                (entity, bounds)
            }
        };

        // Register each child in self.states so they can be styled and animated
        for (child_id, child_entity, _) in &child_spans {
            self.tag_entity(*child_entity);
            let child_state = MobjectState {
                bounds: Bounds3D::default(),
                transform: SpatialTransform::default(),
                opacity: 1.0,
                fill: fill.clone(),
                stroke: stroke.clone(),
                entity: *child_entity,
                child_spans: Vec::new(),
                children: Vec::new(),
                parent: None,
            };
            self.states.insert(*child_id, child_state);
        }

        self.tag_entity(entity);
        let state = MobjectState {
            bounds,
            transform: SpatialTransform::default(),
            opacity: 1.0,
            fill: Some(gaanim_core::peniko::Brush::Solid(style.fill_color)),
            stroke: gaanim_scene::StrokeBrush::transparent(),
            entity,
            child_spans,
            children: Vec::new(),
            parent: None,
        };
        self.states.insert(parent_id, state);
        self.mobject_names.insert(parent_id, format!("Text('{}')", content));

        MobjectRef { id: parent_id }
    }

    /// Spawns a reactive DecimalNumber Mobject that displays and updates according to a ValueTracker signal.
    pub fn decimal_number(
        &mut self,
        signal_ref: ValueTrackerRef,
        num_decimals: usize,
        prefix: &str,
        suffix: &str,
        font_family: &str,
        font_size: f64,
    ) -> MobjectRef {
        let parent_id = self.next_id();
        let fill = Some(gaanim_core::peniko::Brush::Solid(
            gaanim_core::peniko::Color::WHITE,
        ));
        let stroke = gaanim_scene::StrokeBrush::transparent();

        let signal_entity_bevy = match self.states.get(signal_ref.id) {
            Some(state) => state.entity,
            None => {
                bevy::prelude::warn!("ValueTrackerRef id {:?} not found", signal_ref.id);
                let bundle = MobjectBundle::new(parent_id, kurbo::BezPath::new(), Bounds3D::default());
                self.commands.spawn(bundle);
                return MobjectRef { id: parent_id };
            }
        };

        let initial_val = self.float_signals.get(&signal_ref.id).cloned().unwrap_or(0.0);
        let text = format!("{}{:.width$}{}", prefix, initial_val, suffix, width = num_decimals);

        let (path, bounds) = match gaanim_text::shaper::compile_text_to_path(
            self.font_registry,
            &text,
            font_family,
            font_size,
        ) {
            Ok(res) => res,
            Err(e) => {
                bevy::prelude::error!("DecimalNumber initial text compilation failed: {}", e);
                (kurbo::BezPath::new(), Bounds3D::default())
            }
        };

        let mut bundle = MobjectBundle::new(parent_id, path, bounds);
        bundle.fill = gaanim_scene::FillBrush(fill.clone());
        bundle.stroke = stroke.clone();
        bundle.tag = gaanim_scene::ObjectTag(format!("DecimalNumber({})", text));

        let entity = self.commands.spawn(bundle)
            .insert(crate::DecimalNumber {
                signal_entity: signal_entity_bevy,
                num_decimals,
                prefix: prefix.to_string(),
                suffix: suffix.to_string(),
                font_family: font_family.to_string(),
                font_size,
                last_value: Some(initial_val),
            })
            .id();

        self.tag_entity(entity);

        let state = MobjectState {
            bounds,
            transform: SpatialTransform::default(),
            opacity: 1.0,
            fill,
            stroke,
            entity,
            child_spans: Vec::new(),
            children: Vec::new(),
            parent: None,
        };
        self.states.insert(parent_id, state);
        self.mobject_names.insert(parent_id, format!("DecimalNumber('{}')", text));

        MobjectRef { id: parent_id }
    }

    /// Shorthand to spawn a Title text Mobject.
    pub fn title(&mut self, content: &str) -> MobjectRef {
        self.spawn_text(content, gaanim_text::prelude::TextRole::Title)
    }

    /// Shorthand to spawn a Subtitle text Mobject.
    pub fn subtitle(&mut self, content: &str) -> MobjectRef {
        self.spawn_text(content, gaanim_text::prelude::TextRole::Subtitle)
    }

    /// Shorthand to spawn a Body text Mobject.
    pub fn body(&mut self, content: &str) -> MobjectRef {
        self.spawn_text(content, gaanim_text::prelude::TextRole::Body)
    }

    /// Shorthand to spawn a Caption text Mobject.
    pub fn caption(&mut self, content: &str) -> MobjectRef {
        self.spawn_text(content, gaanim_text::prelude::TextRole::Caption)
    }

    /// Shorthand to spawn a mathematical equation Mobject (LaTeX style) with default Math styling.
    pub fn equation(&mut self, formula: &str) -> MobjectRef {
        let style = self
            .text_config
            .roles
            .get(&gaanim_text::prelude::TextRole::Math)
            .cloned()
            .unwrap_or_else(|| gaanim_text::prelude::RoleStyle {
                font_family: "New Computer Modern Math".to_string(),
                size: 48.0,
                fill_color: gaanim_core::peniko::Color::WHITE,
            });

        let parent_id = self.next_id();
        let fill = Some(gaanim_core::peniko::Brush::Solid(style.fill_color));
        let stroke = gaanim_scene::StrokeBrush::transparent();

        let id_counter = &mut self.id_counter;
        let next_id_fn = move || {
            let id = gaanim_core::ObjectId::from_parts(*id_counter, 1);
            *id_counter += 1;
            id
        };

        let mut child_spans = Vec::new();
        let (entity, bounds) = compile_typst_to_hierarchy(
            self.commands,
            self.font_registry,
            formula,
            true, // is_math
            None,
            Some(&style.font_family),
            None,
            Some(style.size),
            fill.clone(),
            stroke.clone(),
            parent_id,
            next_id_fn,
            &mut child_spans,
        );

        // Register each child in self.states so they can be styled and animated
        for (child_id, child_entity, _) in &child_spans {
            self.tag_entity(*child_entity);
            let child_state = MobjectState {
                bounds: Bounds3D::default(),
                transform: SpatialTransform::default(),
                opacity: 1.0,
                fill: fill.clone(),
                stroke: stroke.clone(),
                entity: *child_entity,
                child_spans: Vec::new(),
                children: Vec::new(),
                parent: None,
            };
            self.states.insert(*child_id, child_state);
        }

        self.tag_entity(entity);
        let state = MobjectState {
            bounds,
            transform: SpatialTransform::default(),
            opacity: 1.0,
            fill: Some(gaanim_core::peniko::Brush::Solid(style.fill_color)),
            stroke: gaanim_scene::StrokeBrush::transparent(),
            entity,
            child_spans,
            children: Vec::new(),
            parent: None,
        };
        self.states.insert(parent_id, state);
        self.mobject_names.insert(parent_id, format!("Typst('{}')", formula));

        MobjectRef { id: parent_id }
    }

    /// Selects a subset of characters/shapes in a text or equation Mobject by exact substring.
    /// This implementation is robust: it is case-insensitive, whitespace-insensitive,
    /// format-insensitive (ignores ^, _, $), and correctly maps UTF-8 byte offsets back to glyph IDs.
    pub fn select<'q>(
        &'q mut self,
        target: MobjectRef,
        substring: &str,
    ) -> MobjectSelection<'q, 'w, 's, 'a> {
        // Helper function to normalize mathematical italic/bold alphanumeric characters back to standard Latin/numeric equivalents.
        fn to_standard_char(c: char) -> char {
            let cp = c as u32;
            match cp {
                // Planck constant U+210E -> 'h'
                0x210E => 'h',
                // Mathematical Bold (Capitals & Smalls)
                0x1D400..=0x1D419 => char::from_u32(cp - 0x1D400 + 0x41).unwrap_or(c),
                0x1D41A..=0x1D433 => char::from_u32(cp - 0x1D41A + 0x61).unwrap_or(c),
                // Mathematical Italic (Capitals & Smalls)
                0x1D434..=0x1D44D => char::from_u32(cp - 0x1D434 + 0x41).unwrap_or(c),
                0x1D44E..=0x1D467 => char::from_u32(cp - 0x1D44E + 0x61).unwrap_or(c),
                // Mathematical Bold Italic (Capitals & Smalls)
                0x1D468..=0x1D481 => char::from_u32(cp - 0x1D468 + 0x41).unwrap_or(c),
                0x1D482..=0x1D49B => char::from_u32(cp - 0x1D482 + 0x61).unwrap_or(c),
                // Script (Capitals & Smalls)
                0x1D49C..=0x1D4B5 => char::from_u32(cp - 0x1D49C + 0x41).unwrap_or(c),
                0x1D4B6..=0x1D4CF => char::from_u32(cp - 0x1D4B6 + 0x61).unwrap_or(c),
                // Bold Script (Capitals & Smalls)
                0x1D4D0..=0x1D4E9 => char::from_u32(cp - 0x1D4D0 + 0x41).unwrap_or(c),
                0x1D4EA..=0x1D503 => char::from_u32(cp - 0x1D4EA + 0x61).unwrap_or(c),
                // Fraktur (Capitals & Smalls)
                0x1D504..=0x1D51D => char::from_u32(cp - 0x1D504 + 0x41).unwrap_or(c),
                0x1D51E..=0x1D537 => char::from_u32(cp - 0x1D51E + 0x61).unwrap_or(c),
                // Double-struck (Capitals & Smalls)
                0x1D538..=0x1D551 => char::from_u32(cp - 0x1D538 + 0x41).unwrap_or(c),
                0x1D552..=0x1D56B => char::from_u32(cp - 0x1D552 + 0x61).unwrap_or(c),
                // Bold Fraktur (Capitals & Smalls)
                0x1D56C..=0x1D585 => char::from_u32(cp - 0x1D56C + 0x41).unwrap_or(c),
                0x1D586..=0x1D59F => char::from_u32(cp - 0x1D586 + 0x61).unwrap_or(c),
                // Sans-serif (Capitals & Smalls)
                0x1D5A0..=0x1D5B9 => char::from_u32(cp - 0x1D5A0 + 0x41).unwrap_or(c),
                0x1D5BA..=0x1D5D3 => char::from_u32(cp - 0x1D5BA + 0x61).unwrap_or(c),
                // Sans-serif Bold (Capitals & Smalls)
                0x1D5D4..=0x1D5ED => char::from_u32(cp - 0x1D5D4 + 0x41).unwrap_or(c),
                0x1D5EE..=0x1D607 => char::from_u32(cp - 0x1D5EE + 0x61).unwrap_or(c),
                // Sans-serif Italic (Capitals & Smalls)
                0x1D608..=0x1D621 => char::from_u32(cp - 0x1D608 + 0x41).unwrap_or(c),
                0x1D622..=0x1D63B => char::from_u32(cp - 0x1D622 + 0x61).unwrap_or(c),
                // Sans-serif Bold Italic (Capitals & Smalls)
                0x1D63C..=0x1D655 => char::from_u32(cp - 0x1D63C + 0x41).unwrap_or(c),
                0x1D656..=0x1D66F => char::from_u32(cp - 0x1D656 + 0x61).unwrap_or(c),
                // Monospace (Capitals & Smalls)
                0x1D670..=0x1D689 => char::from_u32(cp - 0x1D670 + 0x41).unwrap_or(c),
                0x1D68A..=0x1D6A3 => char::from_u32(cp - 0x1D68A + 0x61).unwrap_or(c),
                // Mathematical Numbers (Bold, Double-struck, Sans-serif Bold, Sans-serif Italic, Monospace)
                0x1D7CE..=0x1D7FF => char::from_u32(0x30 + (cp - 0x1D7CE) % 10).unwrap_or(c),
                _ => c,
            }
        }

        let mut child_ids = Vec::new();
        if let Some(state) = self.states.get(target.id) {
            // 1. Build a normalized representation of flat_text and keep track of original child_spans indices
            let mut normalized_text = String::new();
            let mut index_mapping = Vec::new(); // maps each byte offset in normalized_text to child_spans index

            for (span_idx, (_, _, span)) in state.child_spans.iter().enumerate() {
                let raw_c = span.character;
                // Ignore spaces, subscripts, superscripts, and generic shape markers
                if raw_c.is_whitespace() || raw_c == '^' || raw_c == '_' {
                    continue;
                }

                // Map mathematical alphanumeric variants to standard equivalents
                let c = to_standard_char(raw_c);

                // Keep the lowercase version
                let lower_chars: Vec<char> = c.to_lowercase().collect();
                for lc in lower_chars {
                    let start_byte = normalized_text.len();
                    normalized_text.push(lc);
                    let end_byte = normalized_text.len();
                    // Map each byte of this character in normalized_text back to span_idx
                    for _ in start_byte..end_byte {
                        index_mapping.push(span_idx);
                    }
                }
            }

            // 2. Build the normalized query string
            let mut normalized_query = String::new();
            for raw_c in substring.chars() {
                if raw_c.is_whitespace() || raw_c == '^' || raw_c == '_' {
                    continue;
                }
                let c = to_standard_char(raw_c);
                for lc in c.to_lowercase() {
                    normalized_query.push(lc);
                }
            }

            // 3. Match normalized query against normalized text
            if !normalized_query.is_empty()
                && let Some(start_byte_idx) = normalized_text.find(&normalized_query) {
                    let end_byte_idx = start_byte_idx + normalized_query.len();

                    // We gather unique child_spans indices that fall within the matched byte range
                    let mut matched_span_indices = Vec::new();
                    for byte_idx in start_byte_idx..end_byte_idx {
                        if let Some(&span_idx) = index_mapping.get(byte_idx)
                            && !matched_span_indices.contains(&span_idx) {
                                matched_span_indices.push(span_idx);
                            }
                    }

                    // Add the child IDs
                    for span_idx in matched_span_indices {
                        if let Some((id, _, _)) = state.child_spans.get(span_idx) {
                            child_ids.push(*id);
                        }
                    }
                }
        }

        MobjectSelection {
            builder: self,
            parent_id: target.id,
            child_ids,
        }
    }

    /// Selects a subset of characters/shapes in a text or equation Mobject by a custom closure predicate.
    pub fn select_by<'q, F>(
        &'q mut self,
        target: MobjectRef,
        predicate: F,
    ) -> MobjectSelection<'q, 'w, 's, 'a>
    where
        F: Fn(&gaanim_scene::components::TextSpan) -> bool,
    {
        let mut child_ids = Vec::new();
        if let Some(state) = self.states.get(target.id) {
            for (id, _, span) in &state.child_spans {
                if predicate(span) {
                    child_ids.push(*id);
                }
            }
        }

        MobjectSelection {
            builder: self,
            parent_id: target.id,
            child_ids,
        }
    }

    /// Selects a subset of characters/shapes in a text or equation Mobject by a sequential character range.
    pub fn select_range<'q>(
        &'q mut self,
        target: MobjectRef,
        range: std::ops::Range<usize>,
    ) -> MobjectSelection<'q, 'w, 's, 'a> {
        let mut child_ids = Vec::new();
        if let Some(state) = self.states.get(target.id) {
            for (id, _, span) in &state.child_spans {
                if span.char_index >= range.start && span.char_index < range.end {
                    child_ids.push(*id);
                }
            }
        }

        MobjectSelection {
            builder: self,
            parent_id: target.id,
            child_ids,
        }
    }
}

/// Helper structure providing fluent configuration for Mobjects before spawning them.
pub struct MobjectSpawnBuilder<'b, 'w, 's, 'a> {
    pub builder: &'b mut SceneBuilder<'w, 's, 'a>,
    pub id: ObjectId,
    pub bundle: MobjectBundle,
    pub parent_entity: Option<Entity>,
}

impl<'b, 'w, 's, 'a> MobjectSpawnBuilder<'b, 'w, 's, 'a> {
    pub fn fill(mut self, color: Color) -> Self {
        self.bundle.fill = FillBrush(Some(Brush::Solid(color)));
        self
    }

    pub fn fill_brush(mut self, brush: Brush) -> Self {
        self.bundle.fill = FillBrush(Some(brush));
        self
    }

    pub fn no_fill(mut self) -> Self {
        self.bundle.fill = FillBrush(None);
        self
    }

    pub fn stroke(mut self, color: Color, width: f64) -> Self {
        self.bundle.stroke = StrokeBrush {
            brush: Some(Brush::Solid(color)),
            style: kurbo::Stroke::new(width),
        };
        self
    }

    pub fn stroke_brush(mut self, brush: Brush, width: f64) -> Self {
        self.bundle.stroke = StrokeBrush {
            brush: Some(brush),
            style: kurbo::Stroke::new(width),
        };
        self
    }

    pub fn no_stroke(mut self) -> Self {
        self.bundle.stroke = StrokeBrush::transparent();
        self
    }

    pub fn transform(mut self, transform: SpatialTransform) -> Self {
        self.bundle.transform = transform;
        self
    }

    pub fn translate(mut self, x: f64, y: f64) -> Self {
        self.bundle.transform = self.bundle.transform.shift_2d(x, y);
        self
    }

    pub fn scale(mut self, s: f64) -> Self {
        self.bundle.transform = self.bundle.transform.scale_uniform(s);
        self
    }

    pub fn rotate(mut self, radians: f64) -> Self {
        self.bundle.transform = self.bundle.transform.with_rotation_2d(radians);
        self
    }

    pub fn opacity(mut self, opacity: f32) -> Self {
        self.bundle.opacity = Opacity(opacity);
        self
    }

    pub fn z_index(mut self, z: i32) -> Self {
        self.bundle.render_order.z_index = z;
        self
    }

    /// Positions this object adjacent to a reference object in a specific layout direction.
    /// Centered along the orthogonal axis (like Manim).
    pub fn next_to(
        mut self,
        reference: MobjectRef,
        direction: LayoutDirection,
        spacing: f64,
    ) -> Self {
        if let Some(ref_state) = self.builder.states.get(reference.id) {
            let shift = gaanim_layout::compute_next_to(
                self.bundle.bounds.0,
                &self.bundle.transform,
                ref_state.bounds,
                &ref_state.transform,
                direction,
                spacing,
            );
            self.bundle.transform = self.bundle.transform.shift_3d(shift);
        }
        self
    }

    /// Aligns a target anchor point on this object with a reference anchor point on the reference object.
    pub fn align_to(
        mut self,
        reference: MobjectRef,
        target_anchor: LayoutAnchor,
        ref_anchor: LayoutAnchor,
    ) -> Self {
        if let Some(ref_state) = self.builder.states.get(reference.id) {
            let shift = gaanim_layout::compute_align_to(
                self.bundle.bounds.0,
                &self.bundle.transform,
                ref_state.bounds,
                &ref_state.transform,
                target_anchor,
                ref_anchor,
            );
            self.bundle.transform = self.bundle.transform.shift_3d(shift);
        }
        self
    }

    /// Establishes parent-child relationship via Bevy hierarchy systems.
    pub fn parent(mut self, parent: MobjectRef) -> Self {
        if let Some(parent_state) = self.builder.states.get(parent.id) {
            self.parent_entity = Some(parent_state.entity);
        }
        self
    }

    /// Finalizes the setup, spawning the Bevy ECS bundle and recording its tracked hot state in the SceneBuilder.
    pub fn spawn(self) -> MobjectRef {
        let mut entity_cmd = self.builder.commands.spawn(self.bundle.clone());
        let entity = entity_cmd.id();

        if let Some(parent) = self.parent_entity {
            entity_cmd.set_parent_in_place(parent);
        }

        // Tag entity with the current scene if inside a scene scope
        if let Some(scene_id) = self.builder.current_scene {
            self.builder.commands.entity(entity).insert(SceneMember(scene_id));
        }

        let state = MobjectState {
            bounds: self.bundle.bounds.0,
            transform: self.bundle.transform,
            opacity: self.bundle.opacity.0,
            fill: self.bundle.fill.0.clone(),
            stroke: self.bundle.stroke.clone(),
            entity,
            child_spans: Vec::new(),
            children: Vec::new(),
            parent: None,
        };
        self.builder.states.insert(self.id, state);
        self.builder.mobject_names.insert(self.id, self.bundle.tag.0.clone());

        MobjectRef { id: self.id }
    }
}
