use std::collections::HashMap;
use bevy::prelude::{Commands, Entity, BuildChildrenTransformExt};
use gaanim_core::ObjectId;
use gaanim_math::{Bounds3D, SpatialTransform};
use gaanim_scene::{FillBrush, Opacity, StrokeBrush};
use gaanim_objects::prelude::MobjectBundle;
use gaanim_timeline::{
    timeline::Timeline,
    clip::{ClipPayload, AnimationSpec, PropertyLensSpec, TrackId},
};
use gaanim_layout::{LayoutAnchor, LayoutDirection};
use gaanim_core::peniko::{Brush, Color};
use gaanim_core::kurbo;
use gaanim_text::font::FontRegistry;
use gaanim_text::shaper::compile_text_to_hierarchy;
use gaanim_text::typst_compiler::compile_typst_to_hierarchy;

use crate::anim::{AnimationBuilder, AnimationType};

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
    pub fn set_fill(&mut self, color: Color) -> &mut Self {
        for child_id in &self.child_ids {
            if let Some(state) = self.builder.states.get_mut(child_id) {
                state.fill = Some(Brush::Solid(color));
                self.builder.commands.entity(state.entity)
                    .insert(FillBrush(Some(Brush::Solid(color))));
            }
        }
        self
    }

    /// Instantly colors the outline stroke of all selected symbols.
    pub fn set_stroke(&mut self, color: Color, width: f64) -> &mut Self {
        for child_id in &self.child_ids {
            if let Some(state) = self.builder.states.get_mut(child_id) {
                state.stroke = StrokeBrush::new(color, width);
                self.builder.commands.entity(state.entity)
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
                    .rate_func(self.rate_func.clone())
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
                    .rate_func(self.rate_func.clone())
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
                    .rate_func(self.rate_func.clone())
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
                    .rate_func(self.rate_func.clone())
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
    pub states: HashMap<ObjectId, MobjectState>,
    pub default_track: TrackId,
}

impl<'w, 's, 'a> SceneBuilder<'w, 's, 'a> {
    /// Creates a new `SceneBuilder` wrapping the Bevy `Commands` context, `Timeline` resource, and `FontRegistry`.
    pub fn new(
        commands: &'a mut Commands<'w, 's>,
        timeline: &'a mut Timeline,
        font_registry: &'a FontRegistry,
        text_config: &'a gaanim_text::prelude::TextConfig,
    ) -> Self {
        // Ensure a default track exists on the timeline
        let default_track = if let Some(track_id) = timeline.tracks.keys().next() {
            track_id.clone()
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
            states: HashMap::new(),
            default_track,
        }
    }

    /// Allocates a new, stable, auto-incremented ObjectId.
    pub fn next_id(&mut self) -> ObjectId {
        self.id_counter += 1;
        ObjectId::from_parts(self.id_counter, 1)
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
        // The Write animation expands into N staggered sub-clips, so it
        // needs its own branch that can clone child ids and access the
        // timeline multiple times. All other variants collapse to a
        // single clip below.
        if matches!(anim.anim_type, AnimationType::Write { .. }) {
            self.play_write_internal(anim);
            return;
        }

        let state = match self.states.get_mut(&anim.target) {
            Some(s) => s,
            None => {
                bevy::prelude::warn!("Attempted to animate unregistered Mobject: {:?}", anim.target);
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
            AnimationType::Write { .. } => unreachable!("Write is dispatched in the early branch above"),
        };

        // Add the resolved clip to the Timeline resource
        self.timeline.add_clip(
            self.default_track,
            self.current_time,
            anim.duration,
            ClipPayload::Animation(AnimationSpec {
                target: anim.target,
                lens: lens_spec,
                rate_func: anim.rate_func,
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
    /// the progressive path draw followed by the fill cross-fade.
    ///
    /// Stagger math (ported from `crabanim::engine::animation::drawing`):
    /// `item_duration = duration / (1 + (n - 1) * lag_ratio)`,
    /// `lag_step = item_duration * lag_ratio` with `lag_ratio = 0.25`.
    /// So the next child starts when the previous one is 25% drawn.
    fn play_write_internal(&mut self, anim: AnimationBuilder) {
        let stroke_width = match anim.anim_type {
            AnimationType::Write { stroke_width } => stroke_width,
            _ => unreachable!(),
        };

        // Collect target ids: the target's own id plus all child spans.
        // We snapshot the ids because the inner loop needs `self.timeline`
        // (which immutably borrows `self`).
        let items: Vec<ObjectId> = {
            let state = match self.states.get(&anim.target) {
                Some(s) => s,
                None => {
                    bevy::prelude::warn!(
                        "Attempted to Write unregistered Mobject: {:?}",
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

        // (A) Auto-stroke: entities that have no stroke brush yet
        // (the common case for text glyphs, which default to
        // `StrokeBrush::transparent()`) get a stroke synthesized from
        // the fill color and the user-supplied `stroke_width`
        // (or 1.0). The fill color is extracted from:
        //   1. `Brush::Solid(c)` — direct color.
        //   2. `Brush::Gradient(g)` — first color stop of the gradient.
        //   3. `Brush::Image(_)` — falls back to white (no meaningful
        //      single color).
        // Without an outline brush the renderer's
        // `if let Some(stroke_brush)` branch is skipped, so the
        // progressive `PathCompletion` trim would be invisible — the
        // user would only ever see the fill.
        for item_id in &items {
            if let Some(state) = self.states.get_mut(item_id) {
                if state.stroke.brush.is_none() {
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
        }

        // (B) Hide the fill on every target entity right now. The next
        // four clips (reset, hold, PathCompletion, then FillDrawProgress
        // fade) animate it back to fully drawn, so the user only sees
        // the path being drawn and then the fill cross-fading in. The
        // insert is a best-effort initial value; the "reset" clip
        // below is the authoritative boundary reset.
        for item_id in &items {
            if let Some(state) = self.states.get(item_id) {
                self.commands
                    .entity(state.entity)
                    .insert(gaanim_animation::FillDrawProgress(0.0));
            }
        }

        /// Lag ratio between consecutive items in a multi-character
        /// Write. With 0.25, the next character starts its own outline
        /// draw when the previous one is 25% through its total duration,
        /// producing a tighter, faster cascade than the Manim default
        /// 0.5 (which leaves a more visible per-character pause).
        const LAG_RATIO: f64 = 0.25;
        /// Fraction of `item_duration` spent drawing the outline before
        /// the fill cross-fade starts. Matches Manim's default split
        /// (~70% draw, ~30% fade-in).
        const DRAW_RATIO: f64 = 0.7;

        let item_duration = anim.duration / (1.0 + (n as f64 - 1.0) * LAG_RATIO);
        let lag_step = item_duration * LAG_RATIO;
        let min_step = 1e-6_f64.max(item_duration * 0.01);

        let draw_duration = (item_duration * DRAW_RATIO).max(min_step);
        let fade_duration = (item_duration * (1.0 - DRAW_RATIO)).max(min_step);

        // (C) Global fill reset: a 0-duration clip scheduled at
        // `self.current_time` (the start of the entire Write, *not*
        // per-character) that forces `FillDrawProgress` back to 0.0
        // for every target entity at once. This is what prevents the
        // "blinking" effect in back-to-back playbacks: a per-character
        // hold would only hide the fill for one character at a time
        // (because of the LAG_RATIO stagger), so a character whose
        // `item_start` is still in the future would render fully filled
        // for that interval. The reset clip runs for every character
        // at the same instant, so the fill is hidden from the very
        // first frame of the Write and stays hidden until the fade
        // clip kicks in.
        for item_id in &items {
            self.timeline.add_clip(
                self.default_track,
                self.current_time,
                min_step,
                ClipPayload::Animation(AnimationSpec {
                    target: *item_id,
                    lens: PropertyLensSpec::FillDrawProgress { from: 0.0, to: 0.0 },
                    rate_func: anim.rate_func.clone(),
                }),
            );
        }

        for (i, item_id) in items.iter().enumerate() {
            let item_delay = (i as f64 * lag_step).max(0.0);
            let item_start = self.current_time + item_delay;
            let fade_start = item_start + draw_duration;

            // (1) Fill hold: clamps `FillDrawProgress` to 0.0 for the
            // entire draw phase. Belt-and-suspenders alongside the
            // global reset clip: it keeps the fill hidden even if a
            // stray clip from a different animation tries to mutate
            // the component mid-draw.
            self.timeline.add_clip(
                self.default_track,
                item_start,
                draw_duration,
                ClipPayload::Animation(AnimationSpec {
                    target: *item_id,
                    lens: PropertyLensSpec::FillDrawProgress { from: 0.0, to: 0.0 },
                    rate_func: anim.rate_func.clone(),
                }),
            );

            // (2) Outline draw: PathCompletion 0.0 -> 1.0 over draw_duration.
            // The lens application reads the cached `PathSource` and
            // trims the visible `Path2D` directly. With the auto-stroke
            // fix above, the trimmed path is now actually visible in the
            // renderer's stroke pass.
            self.timeline.add_clip(
                self.default_track,
                item_start,
                draw_duration,
                ClipPayload::Animation(AnimationSpec {
                    target: *item_id,
                    lens: PropertyLensSpec::PathCompletion { from: 0.0, to: 1.0 },
                    rate_func: anim.rate_func.clone(),
                }),
            );

            // (3) Fill cross-fade: FillDrawProgress 0.0 -> 1.0 over
            // fade_duration, starting right after the path draw completes.
            // The lens inserts/updates a `FillDrawProgress` component on
            // the entity, which the renderer reads to modulate the fill
            // brush's color alpha. The preceding hold clip ends exactly
            // at `fade_start`, so the fade clip's t=0 value (0.0) takes
            // over seamlessly.
            self.timeline.add_clip(
                self.default_track,
                fade_start,
                fade_duration,
                ClipPayload::Animation(AnimationSpec {
                    target: *item_id,
                    lens: PropertyLensSpec::FillDrawProgress { from: 0.0, to: 1.0 },
                    rate_func: anim.rate_func.clone(),
                }),
            );

            // (3) Optional stroke width override during the draw phase.
            // We schedule a 0-duration `StrokeWidthTo` clip at item_start
            // so the outline thickness is set to the user-requested value
            // before any pixels of the draw are visible. We don't restore
            // the original — the user can chain a later StrokeWidthTo if
            // they want to revert.
            if let Some(width) = stroke_width {
                self.timeline.add_clip(
                    self.default_track,
                    item_start,
                    min_step,
                    ClipPayload::Animation(AnimationSpec {
                        target: *item_id,
                        lens: PropertyLensSpec::StrokeWidth {
                            from: width,
                            to: width,
                        },
                        rate_func: anim.rate_func.clone(),
                    }),
                );
            }
        }
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
    pub fn rounded_rect(&mut self, width: f64, height: f64, corner_radius: f64) -> MobjectSpawnBuilder<'_, 'w, 's, 'a> {
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
    pub fn line(&mut self, start: kurbo::Point, end: kurbo::Point) -> MobjectSpawnBuilder<'_, 'w, 's, 'a> {
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

    /// Spawns a symmetric star primitive.
    pub fn star(&mut self, n_points: u32, outer_radius: f64, inner_radius: f64) -> MobjectSpawnBuilder<'_, 'w, 's, 'a> {
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
    pub fn arrow(&mut self, start: kurbo::Point, end: kurbo::Point) -> MobjectSpawnBuilder<'_, 'w, 's, 'a> {
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
    pub fn regular_polygon(&mut self, n_sides: u32, radius: f64) -> MobjectSpawnBuilder<'_, 'w, 's, 'a> {
        let id = self.next_id();
        let bundle = gaanim_objects::primitives::regular_polygon(id, n_sides, radius);
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
        let fill = Some(gaanim_core::peniko::Brush::Solid(gaanim_core::peniko::Color::BLACK));
        let stroke = gaanim_scene::StrokeBrush::transparent();

        // Extract mutable counter separately to avoid borrow conflict with `self.commands`.
        let id_counter = &mut self.id_counter;
        let next_id_fn = move || {
            *id_counter += 1;
            gaanim_core::ObjectId::from_parts(*id_counter, 1)
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
            let child_state = MobjectState {
                bounds: Bounds3D::default(),
                transform: SpatialTransform::default(),
                opacity: 1.0,
                fill: Some(gaanim_core::peniko::Brush::Solid(gaanim_core::peniko::Color::BLACK)),
                stroke: gaanim_scene::StrokeBrush::transparent(),
                entity: *child_entity,
                child_spans: Vec::new(),
            };
            self.states.insert(*child_id, child_state);
        }

        let state = MobjectState {
            bounds,
            transform: SpatialTransform::default(),
            opacity: 1.0,
            fill: Some(gaanim_core::peniko::Brush::Solid(gaanim_core::peniko::Color::BLACK)),
            stroke: gaanim_scene::StrokeBrush::transparent(),
            entity,
            child_spans,
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
        self.typst(source, is_math, Some(text_font), Some(math_font), None, None)
    }

    /// Compiles a plain text string into a hierarchy of vector character Mobjects.
    ///
    /// Shapes the text using HarfBuzz (`rustybuzz`) and extracts outlines via `ttf-parser`.
    /// `font_family` is the font name (e.g. "Arial", "sans-serif").
    /// `font_size` is the text size in pixels/points.
    ///
    /// Returns a reference to the parent container of the text.
    pub fn text(
        &mut self,
        content: &str,
        font_family: &str,
        font_size: f64,
    ) -> MobjectRef {
        let parent_id = self.next_id();
        let fill = Some(gaanim_core::peniko::Brush::Solid(gaanim_core::peniko::Color::WHITE));
        let stroke = gaanim_scene::StrokeBrush::transparent();

        // Extract mutable counter separately to avoid borrow conflict with `self.commands`.
        let id_counter = &mut self.id_counter;
        let next_id_fn = move || {
            *id_counter += 1;
            gaanim_core::ObjectId::from_parts(*id_counter, 1)
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
            let child_state = MobjectState {
                bounds: Bounds3D::default(),
                transform: SpatialTransform::default(),
                opacity: 1.0,
                fill: fill.clone(),
                stroke: stroke.clone(),
                entity: *child_entity,
                child_spans: Vec::new(),
            };
            self.states.insert(*child_id, child_state);
        }

        let state = MobjectState {
            bounds,
            transform: SpatialTransform::default(),
            opacity: 1.0,
            fill: Some(gaanim_core::peniko::Brush::Solid(gaanim_core::peniko::Color::WHITE)),
            stroke: gaanim_scene::StrokeBrush::transparent(),
            entity,
            child_spans,
        };
        self.states.insert(parent_id, state);

        MobjectRef { id: parent_id }
    }

    /// Spawns a vector text Mobject using the default styling of the requested `TextRole`.
    pub fn spawn_text(&mut self, content: &str, role: gaanim_text::prelude::TextRole) -> MobjectRef {
        let style = self.text_config.roles.get(&role)
            .cloned()
            .unwrap_or_else(|| gaanim_text::prelude::RoleStyle {
                font_family: "Arial".to_string(),
                size: 32.0,
                fill_color: gaanim_core::peniko::Color::WHITE,
            });

        let parent_id = self.next_id();
        let fill = Some(gaanim_core::peniko::Brush::Solid(style.fill_color.clone()));
        let stroke = gaanim_scene::StrokeBrush::transparent();

        let id_counter = &mut self.id_counter;
        let next_id_fn = move || {
            *id_counter += 1;
            gaanim_core::ObjectId::from_parts(*id_counter, 1)
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
            let child_state = MobjectState {
                bounds: Bounds3D::default(),
                transform: SpatialTransform::default(),
                opacity: 1.0,
                fill: fill.clone(),
                stroke: stroke.clone(),
                entity: *child_entity,
                child_spans: Vec::new(),
            };
            self.states.insert(*child_id, child_state);
        }

        let state = MobjectState {
            bounds,
            transform: SpatialTransform::default(),
            opacity: 1.0,
            fill: Some(gaanim_core::peniko::Brush::Solid(style.fill_color)),
            stroke: gaanim_scene::StrokeBrush::transparent(),
            entity,
            child_spans,
        };
        self.states.insert(parent_id, state);

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
        let style = self.text_config.roles.get(&gaanim_text::prelude::TextRole::Math)
            .cloned()
            .unwrap_or_else(|| gaanim_text::prelude::RoleStyle {
                font_family: "New Computer Modern Math".to_string(),
                size: 48.0,
                fill_color: gaanim_core::peniko::Color::WHITE,
            });

        let parent_id = self.next_id();
        let fill = Some(gaanim_core::peniko::Brush::Solid(style.fill_color.clone()));
        let stroke = gaanim_scene::StrokeBrush::transparent();

        let id_counter = &mut self.id_counter;
        let next_id_fn = move || {
            *id_counter += 1;
            gaanim_core::ObjectId::from_parts(*id_counter, 1)
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
            let child_state = MobjectState {
                bounds: Bounds3D::default(),
                transform: SpatialTransform::default(),
                opacity: 1.0,
                fill: fill.clone(),
                stroke: stroke.clone(),
                entity: *child_entity,
                child_spans: Vec::new(),
            };
            self.states.insert(*child_id, child_state);
        }

        let state = MobjectState {
            bounds,
            transform: SpatialTransform::default(),
            opacity: 1.0,
            fill: Some(gaanim_core::peniko::Brush::Solid(style.fill_color)),
            stroke: gaanim_scene::StrokeBrush::transparent(),
            entity,
            child_spans,
        };
        self.states.insert(parent_id, state);

        MobjectRef { id: parent_id }
    }

    /// Selects a subset of characters/shapes in a text or equation Mobject by exact substring.
    /// This implementation is robust: it is case-insensitive, whitespace-insensitive,
    /// format-insensitive (ignores ^, _, $), and correctly maps UTF-8 byte offsets back to glyph IDs.
    pub fn select<'q>(&'q mut self, target: MobjectRef, substring: &str) -> MobjectSelection<'q, 'w, 's, 'a> {
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
        if let Some(state) = self.states.get(&target.id) {
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
            if !normalized_query.is_empty() {
                if let Some(start_byte_idx) = normalized_text.find(&normalized_query) {
                    let end_byte_idx = start_byte_idx + normalized_query.len();
                    
                    // We gather unique child_spans indices that fall within the matched byte range
                    let mut matched_span_indices = Vec::new();
                    for byte_idx in start_byte_idx..end_byte_idx {
                        if let Some(&span_idx) = index_mapping.get(byte_idx) {
                            if !matched_span_indices.contains(&span_idx) {
                                matched_span_indices.push(span_idx);
                            }
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
        }

        MobjectSelection {
            builder: self,
            parent_id: target.id,
            child_ids,
        }
    }

    /// Selects a subset of characters/shapes in a text or equation Mobject by a custom closure predicate.
    pub fn select_by<'q, F>(&'q mut self, target: MobjectRef, predicate: F) -> MobjectSelection<'q, 'w, 's, 'a>
    where
        F: Fn(&gaanim_scene::components::TextSpan) -> bool,
    {
        let mut child_ids = Vec::new();
        if let Some(state) = self.states.get(&target.id) {
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
    pub fn select_range<'q>(&'q mut self, target: MobjectRef, range: std::ops::Range<usize>) -> MobjectSelection<'q, 'w, 's, 'a> {
        let mut child_ids = Vec::new();
        if let Some(state) = self.states.get(&target.id) {
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
    pub fn next_to(mut self, reference: MobjectRef, direction: LayoutDirection, spacing: f64) -> Self {
        if let Some(ref_state) = self.builder.states.get(&reference.id) {
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
    pub fn align_to(mut self, reference: MobjectRef, target_anchor: LayoutAnchor, ref_anchor: LayoutAnchor) -> Self {
        if let Some(ref_state) = self.builder.states.get(&reference.id) {
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
        if let Some(parent_state) = self.builder.states.get(&parent.id) {
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

        let state = MobjectState {
            bounds: self.bundle.bounds.0,
            transform: self.bundle.transform,
            opacity: self.bundle.opacity.0,
            fill: self.bundle.fill.0.clone(),
            stroke: self.bundle.stroke.clone(),
            entity,
            child_spans: Vec::new(),
        };
        self.builder.states.insert(self.id, state);

        MobjectRef { id: self.id }
    }
}
