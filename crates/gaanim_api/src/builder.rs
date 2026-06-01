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
}

/// A lightweight reference handle to a spawned Mobject in the Scene.
#[derive(Clone, Copy, Debug)]
pub struct MobjectRef {
    pub id: ObjectId,
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
        );

        let state = MobjectState {
            bounds,
            transform: SpatialTransform::default(),
            opacity: 1.0,
            fill: Some(gaanim_core::peniko::Brush::Solid(gaanim_core::peniko::Color::BLACK)),
            stroke: gaanim_scene::StrokeBrush::transparent(),
            entity,
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

        let (entity, bounds) = compile_text_to_hierarchy(
            self.commands,
            self.font_registry,
            content,
            font_family,
            font_size,
            fill,
            stroke,
            parent_id,
            next_id_fn,
        );

        let state = MobjectState {
            bounds,
            transform: SpatialTransform::default(),
            opacity: 1.0,
            fill: Some(gaanim_core::peniko::Brush::Solid(gaanim_core::peniko::Color::WHITE)),
            stroke: gaanim_scene::StrokeBrush::transparent(),
            entity,
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

        let (entity, bounds) = compile_text_to_hierarchy(
            self.commands,
            self.font_registry,
            content,
            &style.font_family,
            style.size,
            fill,
            stroke,
            parent_id,
            next_id_fn,
        );

        let state = MobjectState {
            bounds,
            transform: SpatialTransform::default(),
            opacity: 1.0,
            fill: Some(gaanim_core::peniko::Brush::Solid(style.fill_color)),
            stroke: gaanim_scene::StrokeBrush::transparent(),
            entity,
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
                font_family: "NewCMMath".to_string(),
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

        let (entity, bounds) = compile_typst_to_hierarchy(
            self.commands,
            self.font_registry,
            formula,
            true, // is_math
            None,
            Some(&style.font_family),
            None,
            Some(style.size),
            fill,
            stroke,
            parent_id,
            next_id_fn,
        );

        let state = MobjectState {
            bounds,
            transform: SpatialTransform::default(),
            opacity: 1.0,
            fill: Some(gaanim_core::peniko::Brush::Solid(style.fill_color)),
            stroke: gaanim_scene::StrokeBrush::transparent(),
            entity,
        };
        self.states.insert(parent_id, state);

        MobjectRef { id: parent_id }
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
        };
        self.builder.states.insert(self.id, state);

        MobjectRef { id: self.id }
    }
}
