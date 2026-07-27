//! Compile — replay Canvas ops into SceneBuilder/Bevy.

use std::collections::HashMap;

use bevy::prelude::*;
use gaanim_core::ObjectId;
use gaanim_core::glam::DVec3;
use gaanim_core::kurbo::{Point, Shape, Vec2};
use gaanim_core::peniko::Color as PenikoColor;
use gaanim_math::Bounds3D;
use gaanim_scene::{FillBrush, Opacity, RenderOrder, StrokeBrush, Visible};
use gaanim_timeline::clip::SceneId;
use gaanim_timeline::timeline::{PresentationSlide, PresentationStep, Timeline};

use crate::anim::{AnimationBuilder, AnimationType};
use crate::builder::{MobjectRef, MobjectState, SceneBuilder};
use crate::canvas::canvas_impl::Canvas;
use crate::canvas::ops::{CanvasEndpoint, FragmentRevealStyle, Op, Segment};
use crate::canvas::types::{
    LayoutKind, LayoutOp, ObjectSpec, ParagraphOptions, ParagraphOverflow, SpawnKind, TextAlign,
};

use gaanim_animation::{
    CurvatureOnCurve, NormalOnCurve, PointOnCurve, PositionBinding, TangentOnCurve, TracedPath,
    TrackingEndpoint, TrackingLine, Updater,
};
use gaanim_math::{RateFunc, SpatialTransform};

fn escape_typst_string(text: &str) -> String {
    text.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "")
}

fn typst_foreground_for_background(background: gaanim_core::peniko::Color) -> &'static str {
    let rgba = background.to_rgba8();
    let luminance =
        (0.2126 * f64::from(rgba.r) + 0.7152 * f64::from(rgba.g) + 0.0722 * f64::from(rgba.b))
            / 255.0;
    if luminance > 0.5 { "000000" } else { "ffffff" }
}

fn paragraph_typst_source(text: &str, options: &ParagraphOptions, font_size: f64) -> String {
    let width = options.width.max(1.0);
    let leading = font_size * (options.line_spacing.max(1.0) - 1.0);
    let (alignment, justify) = match options.align {
        TextAlign::Left => ("left", false),
        TextAlign::Center => ("center", false),
        TextAlign::Right => ("right", false),
        TextAlign::Justify => ("left", true),
    };
    let content = format!(
        "#align({alignment})[#text(\"{}\")]",
        escape_typst_string(text)
    );
    let content = if let Some(max_lines) = options.max_lines.filter(|lines| *lines > 0) {
        let height = font_size * options.line_spacing.max(1.0) * max_lines as f64;
        let clip = matches!(options.overflow, ParagraphOverflow::Clip);
        format!("#block(width: 100%, height: {height}pt, clip: {clip})[{content}]")
    } else {
        content
    };
    format!(
        "#set page(width: {width}pt, height: auto, margin: 0pt)\n\
         #set par(justify: {justify}, leading: {leading}pt)\n\
         {content}",
    )
}

impl Canvas {
    pub fn compile_into<'w, 's>(
        &self,
        commands: &mut Commands<'w, 's>,
        timeline: &mut Timeline,
        font_registry: &gaanim_text::font::FontRegistry,
        text_config: &gaanim_text::prelude::TextConfig,
    ) {
        // Finalize the open slide's metadata. Breakpoints are already stored
        // as deferred operations, so compilation remains timeline-driven.
        let presentation = self.presentation_manifest();
        timeline.set_presentation(
            presentation
                .slides
                .into_iter()
                .map(|slide| PresentationSlide {
                    id: slide.id.raw(),
                    name: slide.name,
                    notes: slide.notes,
                    start_time: slide.start_time,
                    end_time: slide.end_time.unwrap_or_default(),
                    steps: slide
                        .steps
                        .into_iter()
                        .map(|step| PresentationStep {
                            name: step.name,
                            time: step.time,
                        })
                        .collect(),
                })
                .collect(),
        );
        let segments = self
            .state
            .lock()
            .expect("canvas state poisoned")
            .segments
            .clone();
        let mut builder = SceneBuilder::new(commands, timeline, font_registry, text_config);
        let mut scene_ids: Vec<SceneId> = Vec::new();
        let mut id_map: HashMap<ObjectId, ObjectId> = HashMap::new();
        let mut camera_position = DVec3::ZERO;
        let mut camera_zoom = 1.0;
        let mut camera_rotation = gaanim_core::glam::DQuat::IDENTITY;
        let mut cancellation_marks: HashMap<ObjectId, Vec<ObjectId>> = HashMap::new();
        let mut canceled_term_children: HashMap<ObjectId, Vec<ObjectId>> = HashMap::new();
        // Raw bounds for the canvas background (visual, no margin).
        let raw_bounds = self.units.frame_bounds(self.width, self.height);
        // Inset bounds for layout operations (to_edge, to_corner respect margin).
        let m = &self.margin;
        let frame_bounds = Bounds3D::new_2d(
            raw_bounds.min.x + m.left,
            raw_bounds.min.y + m.bottom,
            raw_bounds.max.x - m.right,
            raw_bounds.max.y - m.top,
        );
        let bg_color = self.background.unwrap_or(gaanim_core::peniko::Color::WHITE);

        for seg in &segments {
            scene_ids.push(builder.begin_scene(&seg.name));
            Self::replay_seg(
                &mut builder,
                seg,
                &mut id_map,
                frame_bounds,
                text_config,
                bg_color,
                &mut camera_position,
                &mut camera_zoom,
                &mut camera_rotation,
                &mut cancellation_marks,
                &mut canceled_term_children,
            );
            builder.end_scene();
        }

        for (i, seg) in segments.iter().enumerate() {
            if let Some(prev) = seg.prev_segment
                && prev < i
                && i < scene_ids.len()
                && prev < scene_ids.len()
                && let Some(tr) = &seg.transition
            {
                builder
                    .timeline
                    .connect(scene_ids[prev], scene_ids[i], tr.clone());
            }
        }

        // Insert canvas background resource so the renderer draws a visible
        // canvas boundary, distinguishing the canvas area from the window.
        // Uses raw_bounds (no margin) — the visual background covers the full canvas.
        builder
            .commands
            .insert_resource(gaanim_renderer::pipeline::CanvasBackground {
                color: bg_color,
                bounds: raw_bounds,
            });

        // Clear with the canvas color as well. The drawable background is
        // world-space geometry and can be rotated by the camera; using the
        // same clear color prevents Bevy's window clear color from showing
        // through at the viewport edges during that rotation.
        let rgba = bg_color.to_rgba8();
        builder
            .commands
            .insert_resource(ClearColor(Color::srgba_u8(rgba.r, rgba.g, rgba.b, rgba.a)));
    }

    pub fn compile(&self, world: &mut World) {
        let mut timeline = world
            .remove_resource::<Timeline>()
            .expect("Timeline missing");
        let font_registry = world
            .remove_resource::<gaanim_text::font::FontRegistry>()
            .expect("FontRegistry missing");
        let mut text_config = world
            .remove_resource::<gaanim_text::prelude::TextConfig>()
            .expect("TextConfig missing");
        if self.theme.is_some() {
            text_config = self.themed_text_config();
        }
        let mut commands = world.commands();
        self.compile_into(&mut commands, &mut timeline, &font_registry, &text_config);
        world.insert_resource(timeline);
        world.insert_resource(font_registry);
        world.insert_resource(text_config);
    }

    fn replay_seg(
        builder: &mut SceneBuilder,
        seg: &Segment,
        id_map: &mut HashMap<ObjectId, ObjectId>,
        frame_bounds: Bounds3D,
        text_config: &gaanim_text::prelude::TextConfig,
        scene_background: gaanim_core::peniko::Color,
        camera_position: &mut DVec3,
        camera_zoom: &mut f64,
        camera_rotation: &mut gaanim_core::glam::DQuat,
        cancellation_marks: &mut HashMap<ObjectId, Vec<ObjectId>>,
        canceled_term_children: &mut HashMap<ObjectId, Vec<ObjectId>>,
    ) {
        for op in &seg.ops {
            match op {
                Op::Spawn(spec) => {
                    let spec = spec.lock().expect("object spec poisoned").clone();
                    let actual = Self::spawn_one(
                        builder,
                        &spec,
                        id_map,
                        frame_bounds,
                        text_config,
                        scene_background,
                    );
                    id_map.insert(spec.id, actual.id);
                }
                Op::Animate { anim, active } => {
                    if *active {
                        if let Some(anim) = Self::remap_anim(anim, id_map) {
                            builder.play(anim);
                        }
                    }
                }
                Op::Play(anims) => {
                    let remapped: Vec<AnimationBuilder> = anims
                        .iter()
                        .filter_map(|anim| Self::remap_anim(anim, id_map))
                        .collect();
                    builder.play_parallel(remapped);
                }
                Op::FragmentFill {
                    target,
                    fragment,
                    occurrence,
                    color,
                } => {
                    if let Some(&target) = id_map.get(target) {
                        builder
                            .select_occurrence(MobjectRef { id: target }, fragment, *occurrence)
                            .set_fill(*color);
                    }
                }
                Op::FragmentIndicate {
                    target,
                    fragment,
                    occurrence,
                    color,
                    duration,
                } => {
                    let children =
                        Self::fragment_child_ids(builder, id_map, *target, fragment, *occurrence);
                    canceled_term_children
                        .entry(*target)
                        .or_default()
                        .extend(children.iter().copied());
                    let anims = children
                        .into_iter()
                        .map(|target| AnimationBuilder {
                            target,
                            anim_type: AnimationType::Indicate {
                                color: *color,
                                scale_factor: 1.3,
                            },
                            duration: *duration,
                            rate_func: RateFunc::ThereAndBack,
                            delay: 0.0,
                        })
                        .collect();
                    builder.play_parallel(anims);
                }
                Op::FragmentReveal {
                    target,
                    fragment,
                    occurrence,
                    style,
                    duration,
                } => {
                    let children =
                        Self::fragment_child_ids(builder, id_map, *target, fragment, *occurrence);
                    let anims = children
                        .into_iter()
                        .map(|target| AnimationBuilder {
                            target,
                            anim_type: match style {
                                FragmentRevealStyle::Fade => AnimationType::FadeIn,
                                FragmentRevealStyle::Wipe => AnimationType::Write {
                                    config: Default::default(),
                                },
                                FragmentRevealStyle::FromBelow => AnimationType::FadeInFrom {
                                    offset: DVec3::new(0.0, -24.0, 0.0),
                                },
                            },
                            duration: *duration,
                            rate_func: RateFunc::Smooth,
                            delay: 0.0,
                        })
                        .collect();
                    builder.play_parallel(anims);
                }
                Op::CancelFragment {
                    target,
                    fragment,
                    occurrence,
                    duration,
                } => {
                    let children =
                        Self::fragment_child_ids(builder, id_map, *target, fragment, *occurrence);
                    let parent_transform = id_map
                        .get(target)
                        .and_then(|parent| builder.states.get(*parent))
                        .map(|state| state.transform);
                    let strike_color = children
                        .iter()
                        .find_map(|child| {
                            builder
                                .states
                                .get(*child)
                                .and_then(|state| match &state.fill {
                                    Some(gaanim_core::peniko::Brush::Solid(color)) => Some(*color),
                                    _ => None,
                                })
                        })
                        .unwrap_or(PenikoColor::WHITE);
                    let bounds = children
                        .iter()
                        .filter_map(|child| {
                            let state = builder.states.get(*child)?;
                            // Textual child bounds have already been centered
                            // into their parent's local coordinate system by
                            // the shaper. Applying the child's transform here
                            // would subtract that center a second time and put
                            // the strike near the canvas corner.
                            let bounds = state.bounds;
                            Some(match parent_transform {
                                Some(parent) => bounds.transform_2d(&parent.to_affine_2d()),
                                None => bounds,
                            })
                        })
                        .reduce(|bounds, next| bounds.union(&next));
                    if let Some(bounds) = bounds {
                        let pad = (bounds.width() * 0.08).max(3.0);
                        let strike = builder
                            .line(
                                Point::new(bounds.min.x - pad, bounds.min.y - pad * 0.25),
                                Point::new(bounds.max.x + pad, bounds.max.y + pad * 0.25),
                            )
                            .no_fill()
                            .stroke(strike_color, 3.0)
                            .spawn();
                        cancellation_marks
                            .entry(*target)
                            .or_default()
                            .push(strike.id);
                        builder.play(AnimationBuilder {
                            target: strike.id,
                            anim_type: AnimationType::Create {
                                config: Default::default(),
                            },
                            duration: *duration,
                            rate_func: RateFunc::Smooth,
                            delay: 0.0,
                        });
                    } else {
                        builder.wait(*duration);
                    }
                }
                Op::BraceLabel {
                    target,
                    fragment,
                    occurrence,
                    label,
                    above,
                    duration,
                } => {
                    let children =
                        Self::fragment_child_ids(builder, id_map, *target, fragment, *occurrence);
                    let parent = id_map
                        .get(target)
                        .and_then(|id| builder.states.get(*id))
                        .map(|s| s.transform);
                    let bounds = children
                        .iter()
                        .filter_map(|id| builder.states.get(*id).map(|s| s.bounds))
                        .map(|b| {
                            parent
                                .map(|p| b.transform_2d(&p.to_affine_2d()))
                                .unwrap_or(b)
                        })
                        .reduce(|a, b| a.union(&b));
                    if let Some(bounds) = bounds {
                        let color = children
                            .iter()
                            .find_map(|id| {
                                builder.states.get(*id).and_then(|s| match &s.fill {
                                    Some(gaanim_core::peniko::Brush::Solid(c)) => Some(*c),
                                    _ => None,
                                })
                            })
                            .unwrap_or(PenikoColor::WHITE);
                        let side = if *above { 1.0 } else { -1.0 };
                        let y = if *above {
                            bounds.max.y + 12.0
                        } else {
                            bounds.min.y - 12.0
                        };
                        let brace = builder
                            .brace(
                                Point::new(bounds.min.x, y),
                                Point::new(bounds.max.x, y),
                                -side * 10.0,
                            )
                            .no_fill()
                            .stroke(color, 2.0)
                            .spawn();
                        let style = &text_config.roles[&gaanim_text::prelude::TextRole::Body];
                        let label_ref = builder.text(label, &style.font_family, style.size);
                        if let Some(state) = builder.states.get_mut(label_ref.id) {
                            state.transform.translation =
                                DVec3::new(bounds.center().x, y + side * 25.0, 0.0);
                            builder
                                .commands
                                .entity(state.entity)
                                .insert(state.transform);
                        }
                        builder.play_parallel(vec![
                            AnimationBuilder {
                                target: brace.id,
                                anim_type: AnimationType::Create {
                                    config: Default::default(),
                                },
                                duration: *duration,
                                rate_func: RateFunc::Smooth,
                                delay: 0.0,
                            },
                            AnimationBuilder {
                                target: label_ref.id,
                                anim_type: AnimationType::FadeIn,
                                duration: *duration,
                                rate_func: RateFunc::Smooth,
                                delay: 0.0,
                            },
                        ]);
                    }
                }
                Op::AnnotateFragment {
                    target,
                    fragment,
                    occurrence,
                    label,
                    offset,
                    duration,
                } => {
                    let children =
                        Self::fragment_child_ids(builder, id_map, *target, fragment, *occurrence);
                    let parent = id_map
                        .get(target)
                        .and_then(|id| builder.states.get(*id))
                        .map(|s| s.transform);
                    let bounds = children
                        .iter()
                        .filter_map(|id| builder.states.get(*id).map(|s| s.bounds))
                        .map(|b| {
                            parent
                                .map(|p| b.transform_2d(&p.to_affine_2d()))
                                .unwrap_or(b)
                        })
                        .reduce(|a, b| a.union(&b));
                    if let Some(bounds) = bounds {
                        let position = bounds.center() + *offset;
                        let style = &text_config.roles[&gaanim_text::prelude::TextRole::Body];
                        let label_ref = builder.text(label, &style.font_family, style.size);
                        let label_size = builder
                            .states
                            .get(label_ref.id)
                            .map(|state| state.bounds.size())
                            .unwrap_or(DVec3::new(80.0, 24.0, 0.0));
                        let toward_label_x = if offset.x >= 0.0 { -1.0 } else { 1.0 };
                        let toward_label_y = if offset.y >= 0.0 { -1.0 } else { 1.0 };
                        // Attach at the label corner nearest the term, so the
                        // leader line never crosses the annotation text.
                        let label_anchor = position
                            + DVec3::new(
                                toward_label_x * label_size.x * 0.46,
                                toward_label_y * label_size.y * 0.42,
                                0.0,
                            );
                        if let Some(state) = builder.states.get_mut(label_ref.id) {
                            state.transform.translation = position;
                            builder
                                .commands
                                .entity(state.entity)
                                .insert(state.transform);
                        }
                        let line = builder
                            .line(
                                Point::new(bounds.center().x, bounds.center().y),
                                Point::new(label_anchor.x, label_anchor.y),
                            )
                            .no_fill()
                            .stroke(PenikoColor::WHITE, 2.0)
                            .spawn();
                        // Text glyph transforms are local to their equation.
                        // Use an invisible scene-space proxy so the leader
                        // starts at the tag rather than a canvas corner.
                        let proxy = builder.dot(0.01).no_fill().no_stroke().spawn();
                        if let Some(proxy_state) = builder.states.get_mut(proxy.id) {
                            proxy_state.transform.translation = bounds.center();
                            builder
                                .commands
                                .entity(proxy_state.entity)
                                .insert(proxy_state.transform);
                        }
                        if let (Some(line_state), Some(proxy_state)) =
                            (builder.states.get(line.id), builder.states.get(proxy.id))
                        {
                            if let Some(parent_id) = id_map.get(target).copied()
                                && let Some(parent_state) = builder.states.get(parent_id)
                            {
                                builder.commands.entity(proxy_state.entity).insert(
                                    PositionBinding::with_offset(
                                        parent_state.entity,
                                        gaanim_animation::AxisMask::XY,
                                        bounds.center() - parent_state.transform.translation,
                                    ),
                                );
                            }
                            builder
                                .commands
                                .entity(line_state.entity)
                                .insert(TrackingLine::new(
                                    TrackingEndpoint::Entity(proxy_state.entity),
                                    TrackingEndpoint::Static(label_anchor),
                                ));
                        }
                        builder.play_parallel(vec![
                            AnimationBuilder {
                                target: line.id,
                                // TrackingLine regenerates its path each
                                // frame, so a path-draw clip cannot hide it
                                // before its scheduled start. FadeIn keeps it
                                // invisible until the annotation begins.
                                anim_type: AnimationType::FadeIn,
                                duration: *duration,
                                rate_func: RateFunc::Smooth,
                                delay: 0.0,
                            },
                            AnimationBuilder {
                                target: label_ref.id,
                                anim_type: AnimationType::FadeIn,
                                duration: *duration,
                                rate_func: RateFunc::Smooth,
                                delay: 0.0,
                            },
                        ]);
                    }
                }
                Op::WriteTerms {
                    target,
                    terms,
                    duration,
                } => {
                    let term_duration = *duration / terms.len() as f64;
                    for (fragment, occurrence) in terms {
                        let anims = Self::fragment_child_ids(
                            builder,
                            id_map,
                            *target,
                            fragment,
                            *occurrence,
                        )
                        .into_iter()
                        .map(|target| AnimationBuilder {
                            target,
                            anim_type: AnimationType::Write {
                                config: Default::default(),
                            },
                            duration: term_duration,
                            rate_func: RateFunc::Smooth,
                            delay: 0.0,
                        })
                        .collect();
                        builder.play_parallel(anims);
                    }
                }
                Op::FocusEquation {
                    target,
                    terms,
                    dim_opacity,
                    duration,
                } => {
                    let focused = terms
                        .iter()
                        .flat_map(|(fragment, occurrence)| {
                            Self::fragment_child_ids(
                                builder,
                                id_map,
                                *target,
                                fragment,
                                *occurrence,
                            )
                        })
                        .collect::<std::collections::HashSet<_>>();
                    let all = id_map
                        .get(target)
                        .and_then(|target| builder.states.get(*target))
                        .map(|state| state.children.clone())
                        .unwrap_or_default();
                    let mut anims = Vec::with_capacity(all.len() + focused.len());
                    for child in all {
                        if focused.contains(&child) {
                            anims.push(AnimationBuilder {
                                target: child,
                                anim_type: AnimationType::Indicate {
                                    color: None,
                                    scale_factor: 1.12,
                                },
                                duration: *duration,
                                rate_func: RateFunc::ThereAndBack,
                                delay: 0.0,
                            });
                        } else {
                            anims.push(AnimationBuilder {
                                target: child,
                                anim_type: AnimationType::FadeTo { to: *dim_opacity },
                                duration: *duration,
                                rate_func: RateFunc::Smooth,
                                delay: 0.0,
                            });
                        }
                    }
                    builder.play_parallel(anims);
                }
                Op::FragmentFillTo {
                    target,
                    fragment,
                    occurrence,
                    color,
                    duration,
                } => {
                    let children =
                        Self::fragment_child_ids(builder, id_map, *target, fragment, *occurrence);
                    let anims = children
                        .into_iter()
                        .map(|target| AnimationBuilder {
                            target,
                            anim_type: AnimationType::FillColorTo { to: *color },
                            duration: *duration,
                            rate_func: RateFunc::Smooth,
                            delay: 0.0,
                        })
                        .collect();
                    builder.play_parallel(anims);
                }
                Op::FragmentTransform {
                    source,
                    source_fragment,
                    source_occurrence,
                    target,
                    target_fragment,
                    target_occurrence,
                    duration,
                } => {
                    let sources = Self::fragment_child_ids(
                        builder,
                        id_map,
                        *source,
                        source_fragment,
                        *source_occurrence,
                    );
                    let targets = Self::fragment_child_ids(
                        builder,
                        id_map,
                        *target,
                        target_fragment,
                        *target_occurrence,
                    );
                    let source_parent = id_map.get(source).copied();
                    let target_parent = id_map.get(target).copied();
                    let mut transformed = false;
                    if let (Some(source_parent), Some(target_parent)) =
                        (source_parent, target_parent)
                    {
                        for (source, target) in sources.into_iter().zip(targets) {
                            builder.play_fragment_transform(
                                source,
                                target,
                                source_parent,
                                target_parent,
                                *duration,
                            );
                            transformed = true;
                        }
                    }
                    if transformed {
                        builder.current_time += *duration;
                    }
                }
                Op::TaggedTransform {
                    source,
                    target,
                    pairs,
                    duration,
                } => {
                    let source_parent = id_map.get(source).copied();
                    let target_parent = id_map.get(target).copied();
                    let mut transformed = false;
                    for (source_fragment, source_occurrence, target_fragment, target_occurrence) in
                        pairs
                    {
                        let sources = Self::fragment_child_ids(
                            builder,
                            id_map,
                            *source,
                            source_fragment,
                            *source_occurrence,
                        );
                        let targets = Self::fragment_child_ids(
                            builder,
                            id_map,
                            *target,
                            target_fragment,
                            *target_occurrence,
                        );
                        if let (Some(source_parent), Some(target_parent)) =
                            (source_parent, target_parent)
                        {
                            for (source, target) in sources.into_iter().zip(targets) {
                                builder.play_fragment_transform(
                                    source,
                                    target,
                                    source_parent,
                                    target_parent,
                                    *duration,
                                );
                                transformed = true;
                            }
                        }
                    }
                    if transformed {
                        builder.current_time += *duration;
                    }
                }
                Op::ExpandEquation {
                    source,
                    target,
                    source_fragment,
                    source_occurrence,
                    target_fragment,
                    target_occurrence,
                    duration,
                } => {
                    Self::fade_cancellation_marks(builder, cancellation_marks, *source, *duration);
                    Self::fade_canceled_term_children(
                        builder,
                        canceled_term_children,
                        *source,
                        *duration,
                    );
                    let sources = Self::fragment_child_ids(
                        builder,
                        id_map,
                        *source,
                        source_fragment,
                        *source_occurrence,
                    );
                    let targets = Self::fragment_child_ids(
                        builder,
                        id_map,
                        *target,
                        target_fragment,
                        *target_occurrence,
                    );
                    if let (Some(&source_parent), Some(&target_parent)) =
                        (id_map.get(source), id_map.get(target))
                    {
                        if !sources.is_empty() && !targets.is_empty() {
                            builder.play_equation_expansion(
                                source_parent,
                                target_parent,
                                *duration,
                            );
                        }
                    }
                }
                Op::StepEquation {
                    source,
                    target,
                    duration,
                } => {
                    Self::fade_cancellation_marks(builder, cancellation_marks, *source, *duration);
                    Self::fade_canceled_term_children(
                        builder,
                        canceled_term_children,
                        *source,
                        *duration,
                    );
                    if let (Some(&source_parent), Some(&target_parent)) =
                        (id_map.get(source), id_map.get(target))
                    {
                        builder.play_equation_expansion(source_parent, target_parent, *duration);
                    }
                }
                Op::LayoutReflow {
                    container,
                    members,
                    kind,
                    gap,
                    duration,
                    entering,
                    leaving,
                    max_width,
                    max_height,
                    shrink_to_fit,
                    wrap,
                    justify,
                } => {
                    let Some(container) = id_map.get(container).copied() else {
                        continue;
                    };
                    let members: Vec<ObjectId> = members
                        .iter()
                        .filter_map(|member| id_map.get(member).copied())
                        .filter(|member| builder.states.get(*member).is_some())
                        .collect();
                    if members.is_empty() {
                        continue;
                    }

                    // A layout group may gain children after it was first
                    // declared. Reparent them before arranging: this keeps a
                    // nested layout's transform attached to its visual tree,
                    // rather than merely moving its invisible group root.
                    for member in &members {
                        let parent = builder.states.get(*member).and_then(|state| state.parent);
                        if parent != Some(container) {
                            builder.add_to_group(
                                MobjectRef { id: container },
                                MobjectRef { id: *member },
                            );
                        }
                    }
                    if let Some(state) = builder.states.get_mut(container) {
                        state.children = members.clone();
                    }
                    let before: HashMap<ObjectId, SpatialTransform> = members
                        .iter()
                        .filter_map(|member| {
                            builder
                                .states
                                .get(*member)
                                .map(|state| (*member, state.transform))
                        })
                        .collect();
                    match kind {
                        LayoutKind::Row if *wrap => builder.arrange_wrapped(
                            MobjectRef { id: container },
                            max_width.unwrap_or(f64::INFINITY),
                            *gap,
                        ),
                        LayoutKind::Row if justify != "center" => builder.arrange_justified(
                            MobjectRef { id: container },
                            max_width.unwrap_or(f64::INFINITY),
                            *gap,
                            justify,
                        ),
                        LayoutKind::Row => builder.arrange_aligned(
                            MobjectRef { id: container },
                            gaanim_layout::Direction::Right,
                            *gap,
                            gaanim_layout::Anchor::Center,
                        ),
                        LayoutKind::Column => builder.arrange_aligned(
                            MobjectRef { id: container },
                            gaanim_layout::Direction::Down,
                            *gap,
                            gaanim_layout::Anchor::Left,
                        ),
                        LayoutKind::Grid { columns } => builder.arrange_in_grid(
                            MobjectRef { id: container },
                            None,
                            Some((*columns).max(1)),
                            *gap,
                            *gap,
                        ),
                    }

                    let targets: Vec<(ObjectId, DVec3)> = members
                        .iter()
                        .filter_map(|member| {
                            builder
                                .states
                                .get(*member)
                                .map(|state| (*member, state.transform.translation))
                        })
                        .collect();
                    let scale_to = if *shrink_to_fit {
                        builder.states.get(container).map(|state| {
                            let bounds = state.bounds;
                            let width_scale = max_width
                                .map(|max| max / bounds.width().max(1.0))
                                .unwrap_or(1.0);
                            let height_scale = max_height
                                .map(|max| max / bounds.height().max(1.0))
                                .unwrap_or(1.0);
                            width_scale.min(height_scale).min(1.0)
                        })
                    } else {
                        None
                    };
                    let Some(duration) = duration else {
                        if let Some(scale) = scale_to
                            && let Some(state) = builder.states.get_mut(container)
                        {
                            state.transform.scale = DVec3::splat(scale);
                            builder
                                .commands
                                .entity(state.entity)
                                .insert(state.transform);
                        }
                        continue;
                    };

                    // Arrangement writes the final transforms. Restore the
                    // layout visible at the current timeline cursor, then let
                    // the regular animation machinery interpolate to the new
                    // arrangement and advance its cursor.
                    for (member, transform) in before {
                        if let Some(state) = builder.states.get_mut(member) {
                            state.transform = transform;
                            builder.commands.entity(state.entity).insert(transform);
                        }
                    }
                    let mut animations: Vec<AnimationBuilder> = targets
                        .into_iter()
                        .map(|(target, to)| AnimationBuilder {
                            target,
                            anim_type: AnimationType::TranslateTo { to },
                            duration: *duration,
                            rate_func: RateFunc::Smooth,
                            delay: 0.0,
                        })
                        .collect();
                    if let Some(entering) = entering
                        .as_ref()
                        .and_then(|member| id_map.get(member))
                        .copied()
                    {
                        animations.push(AnimationBuilder {
                            target: entering,
                            anim_type: AnimationType::FadeIn,
                            duration: *duration,
                            rate_func: RateFunc::Smooth,
                            delay: 0.0,
                        });
                    }
                    if let Some(leaving) = leaving
                        .as_ref()
                        .and_then(|member| id_map.get(member))
                        .copied()
                    {
                        animations.push(AnimationBuilder {
                            target: leaving,
                            anim_type: AnimationType::FadeOut,
                            duration: *duration,
                            rate_func: RateFunc::Smooth,
                            delay: 0.0,
                        });
                    }
                    if let Some(scale) = scale_to {
                        animations.push(AnimationBuilder {
                            target: container,
                            anim_type: AnimationType::ScaleTo {
                                to: DVec3::splat(scale),
                            },
                            duration: *duration,
                            rate_func: RateFunc::Smooth,
                            delay: 0.0,
                        });
                    }
                    builder.play_parallel(animations);
                }
                Op::Wait(d) => builder.wait(*d),
                Op::CameraPosition { to, duration, .. } => {
                    builder.timeline.add_clip(
                        builder.default_track,
                        builder.current_time,
                        *duration,
                        gaanim_timeline::clip::ClipPayload::Animation(
                            gaanim_timeline::clip::AnimationSpec {
                                target: gaanim_core::ObjectId::from_parts(0, 1),
                                lens: gaanim_timeline::clip::PropertyLensSpec::CameraPosition {
                                    from: *camera_position,
                                    to: *to,
                                },
                                rate_func: gaanim_math::RateFunc::Smooth,
                                delay: 0.0,
                                label: None,
                            },
                        ),
                    );
                    *camera_position = *to;
                    builder.wait(*duration);
                }
                Op::CameraZoom { to, duration, .. } => {
                    builder.timeline.add_clip(
                        builder.default_track,
                        builder.current_time,
                        *duration,
                        gaanim_timeline::clip::ClipPayload::Animation(
                            gaanim_timeline::clip::AnimationSpec {
                                target: gaanim_core::ObjectId::from_parts(0, 1),
                                lens: gaanim_timeline::clip::PropertyLensSpec::CameraZoom {
                                    from: *camera_zoom,
                                    to: *to,
                                },
                                rate_func: gaanim_math::RateFunc::Smooth,
                                delay: 0.0,
                                label: None,
                            },
                        ),
                    );
                    *camera_zoom = *to;
                    builder.wait(*duration);
                }
                Op::CameraRotation { to, duration } => {
                    builder.timeline.add_clip(
                        builder.default_track,
                        builder.current_time,
                        *duration,
                        gaanim_timeline::clip::ClipPayload::Animation(
                            gaanim_timeline::clip::AnimationSpec {
                                target: gaanim_core::ObjectId::from_parts(0, 1),
                                lens: gaanim_timeline::clip::PropertyLensSpec::CameraRotation {
                                    from: *camera_rotation,
                                    to: *to,
                                },
                                rate_func: gaanim_math::RateFunc::Smooth,
                                delay: 0.0,
                                label: None,
                            },
                        ),
                    );
                    *camera_rotation = *to;
                    builder.wait(*duration);
                }
                Op::CameraFrame {
                    target,
                    margin,
                    duration,
                } => {
                    let Some(actual) = id_map.get(target).copied() else {
                        continue;
                    };
                    let Some(state) = builder.states.get(actual) else {
                        continue;
                    };
                    let bounds = state
                        .bounds
                        .transform_2d(&builder.get_world_transform(actual).to_affine_2d());
                    let width = (bounds.width() + margin * 2.0).max(1.0);
                    let height = (bounds.height() + margin * 2.0).max(1.0);
                    let zoom = (frame_bounds.width() / width)
                        .min(frame_bounds.height() / height)
                        .max(0.01);
                    let center = bounds.center();
                    for lens in [
                        gaanim_timeline::clip::PropertyLensSpec::CameraPosition {
                            from: *camera_position,
                            to: center,
                        },
                        gaanim_timeline::clip::PropertyLensSpec::CameraZoom {
                            from: *camera_zoom,
                            to: zoom,
                        },
                    ] {
                        builder.timeline.add_clip(
                            builder.default_track,
                            builder.current_time,
                            *duration,
                            gaanim_timeline::clip::ClipPayload::Animation(
                                gaanim_timeline::clip::AnimationSpec {
                                    target: gaanim_core::ObjectId::from_parts(0, 1),
                                    lens,
                                    rate_func: gaanim_math::RateFunc::Smooth,
                                    delay: 0.0,
                                    label: None,
                                },
                            ),
                        );
                    }
                    *camera_position = center;
                    *camera_zoom = zoom;
                    builder.wait(*duration);
                }
                Op::CameraFollow { target, duration } => {
                    let Some(actual) = id_map.get(target).copied() else {
                        continue;
                    };
                    builder.timeline.add_clip(
                        builder.default_track,
                        builder.current_time,
                        *duration,
                        gaanim_timeline::clip::ClipPayload::Animation(
                            gaanim_timeline::clip::AnimationSpec {
                                target: gaanim_core::ObjectId::from_parts(0, 1),
                                lens: gaanim_timeline::clip::PropertyLensSpec::CameraFollow {
                                    target: actual,
                                },
                                rate_func: gaanim_math::RateFunc::Linear,
                                delay: 0.0,
                                label: None,
                            },
                        ),
                    );
                    if let Some(state) = builder.states.get(actual) {
                        camera_position.x = state.transform.translation.x;
                        camera_position.y = state.transform.translation.y;
                    }
                    builder.wait(*duration);
                }
                Op::CameraShake {
                    amplitude,
                    frequency,
                    duration,
                } => {
                    builder.timeline.add_clip(
                        builder.default_track,
                        builder.current_time,
                        *duration,
                        gaanim_timeline::clip::ClipPayload::Animation(
                            gaanim_timeline::clip::AnimationSpec {
                                target: gaanim_core::ObjectId::from_parts(0, 1),
                                lens: gaanim_timeline::clip::PropertyLensSpec::CameraShake {
                                    origin: *camera_position,
                                    amplitude: *amplitude,
                                    frequency: *frequency,
                                },
                                rate_func: gaanim_math::RateFunc::Linear,
                                delay: 0.0,
                                label: None,
                            },
                        ),
                    );
                    builder.wait(*duration);
                }
                Op::Slide => builder.slide(),
                Op::Show(id) => {
                    if let Some(id) = id_map.get(id).copied()
                        && let Some(st) = builder.states.get_mut(id)
                    {
                        builder.commands.entity(st.entity).insert(Visible);
                    }
                }
                Op::Hide(id) => {
                    if let Some(id) = id_map.get(id).copied()
                        && let Some(st) = builder.states.get_mut(id)
                    {
                        builder.commands.entity(st.entity).remove::<Visible>();
                    }
                }
                Op::Remove(id) => {
                    if let Some(id) = id_map.get(id).copied()
                        && let Some(st) = builder.states.get(id)
                    {
                        builder.commands.entity(st.entity).despawn();
                    }
                }

                // -- Reactive ops --
                Op::AttachUpdater { target, preset } => {
                    if let Some(target_id) = id_map.get(target).copied()
                        && let Some(st) = builder.states.get(target_id)
                    {
                        let updater: Updater = preset.clone().into_updater();
                        builder.commands.entity(st.entity).insert(updater);
                    }
                }

                Op::RemoveUpdater(target) => {
                    if let Some(target_id) = id_map.get(target).copied() {
                        builder.schedule_remove_updater(target_id);
                    }
                }

                Op::AttachTracedPath {
                    target,
                    source,
                    min_distance,
                    max_points,
                } => {
                    if let Some(target_id) = id_map.get(target).copied()
                        && let Some(source_id) = id_map.get(source).copied()
                        && let Some(target_st) = builder.states.get(target_id)
                        && let Some(source_st) = builder.states.get(source_id)
                    {
                        let traced = TracedPath::new(source_st.entity, *min_distance, *max_points);
                        builder.commands.entity(target_st.entity).insert(traced);
                    }
                }

                Op::AttachPositionBinding {
                    target,
                    source,
                    axes,
                } => {
                    if let Some(target_id) = id_map.get(target).copied()
                        && let Some(source_id) = id_map.get(source).copied()
                        && let Some(target_st) = builder.states.get(target_id)
                        && let Some(source_st) = builder.states.get(source_id)
                    {
                        let binding = PositionBinding::new(source_st.entity, *axes);
                        builder.commands.entity(target_st.entity).insert(binding);
                    }
                }

                Op::AttachPositionFollow {
                    target,
                    source,
                    offset,
                } => {
                    if let Some(target_id) = id_map.get(target).copied()
                        && let Some(source_id) = id_map.get(source).copied()
                        && let Some(target_st) = builder.states.get(target_id)
                        && let Some(source_st) = builder.states.get(source_id)
                    {
                        builder.commands.entity(target_st.entity).insert(
                            PositionBinding::with_offset(
                                source_st.entity,
                                gaanim_animation::AxisMask::XY,
                                *offset,
                            ),
                        );
                    }
                }

                Op::AttachTrackingLine { target, from, to } => {
                    if let Some(target_id) = id_map.get(target).copied()
                        && let Some(st) = builder.states.get(target_id)
                    {
                        let resolve_endpoint = |ep: &CanvasEndpoint| -> TrackingEndpoint {
                            match ep {
                                CanvasEndpoint::Static(pos) => TrackingEndpoint::Static(*pos),
                                CanvasEndpoint::Entity(oid) => {
                                    if let Some(rid) = id_map.get(oid).copied() {
                                        if let Some(s) = builder.states.get(rid) {
                                            TrackingEndpoint::Entity(s.entity)
                                        } else {
                                            TrackingEndpoint::Static(DVec3::ZERO)
                                        }
                                    } else {
                                        TrackingEndpoint::Static(DVec3::ZERO)
                                    }
                                }
                            }
                        };
                        let line = TrackingLine::new(resolve_endpoint(from), resolve_endpoint(to));
                        builder.commands.entity(st.entity).insert(line);
                    }
                }

                Op::AttachTrackingSpring {
                    target,
                    from,
                    to,
                    coils,
                    amplitude,
                } => {
                    if let Some(target_id) = id_map.get(target).copied()
                        && let Some(st) = builder.states.get(target_id)
                    {
                        let resolve_endpoint = |ep: &CanvasEndpoint| -> TrackingEndpoint {
                            match ep {
                                CanvasEndpoint::Static(pos) => TrackingEndpoint::Static(*pos),
                                CanvasEndpoint::Entity(oid) => id_map
                                    .get(oid)
                                    .and_then(|rid| builder.states.get(*rid))
                                    .map(|state| TrackingEndpoint::Entity(state.entity))
                                    .unwrap_or(TrackingEndpoint::Static(DVec3::ZERO)),
                            }
                        };
                        let from = resolve_endpoint(from);
                        let to = resolve_endpoint(to);
                        let coils = *coils;
                        let amplitude = *amplitude;
                        let redraw = gaanim_animation::AlwaysRedrawRegen::new(move |world| {
                            let endpoint_position = |endpoint: &TrackingEndpoint| match endpoint {
                                TrackingEndpoint::Static(position) => *position,
                                TrackingEndpoint::Entity(entity) => world
                                    .get::<SpatialTransform>(*entity)
                                    .map(|transform| transform.translation)
                                    .unwrap_or(DVec3::ZERO),
                            };
                            let from = endpoint_position(&from);
                            let to = endpoint_position(&to);
                            gaanim_objects::primitives::spring_path(
                                Point::new(from.x, from.y),
                                Point::new(to.x, to.y),
                                coils,
                                amplitude,
                            )
                        });
                        builder.commands.entity(st.entity).insert(redraw);
                    }
                }

                Op::AttachTrackingDimension {
                    target,
                    from,
                    to,
                    offset,
                } => {
                    if let Some(target_id) = id_map.get(target).copied()
                        && let Some(st) = builder.states.get(target_id)
                    {
                        let resolve_endpoint = |ep: &CanvasEndpoint| -> TrackingEndpoint {
                            match ep {
                                CanvasEndpoint::Static(pos) => TrackingEndpoint::Static(*pos),
                                CanvasEndpoint::Entity(oid) => id_map
                                    .get(oid)
                                    .and_then(|rid| builder.states.get(*rid))
                                    .map(|state| TrackingEndpoint::Entity(state.entity))
                                    .unwrap_or(TrackingEndpoint::Static(DVec3::ZERO)),
                            }
                        };
                        let from = resolve_endpoint(from);
                        let to = resolve_endpoint(to);
                        let offset = *offset;
                        let redraw = gaanim_animation::AlwaysRedrawRegen::new(move |world| {
                            let endpoint_position = |endpoint: &TrackingEndpoint| match endpoint {
                                TrackingEndpoint::Static(position) => *position,
                                TrackingEndpoint::Entity(entity) => world
                                    .get::<SpatialTransform>(*entity)
                                    .map(|transform| transform.translation)
                                    .unwrap_or(DVec3::ZERO),
                            };
                            let from = endpoint_position(&from);
                            let to = endpoint_position(&to);
                            gaanim_objects::primitives::dimension_path(
                                Point::new(from.x, from.y),
                                Point::new(to.x, to.y),
                                offset,
                            )
                        });
                        builder.commands.entity(st.entity).insert(redraw);
                    }
                }

                Op::AttachTrackerArc {
                    target,
                    tracker,
                    center,
                    radius,
                    start_angle,
                    sweep_scale,
                    sweep_offset,
                } => {
                    if let Some(target_id) = id_map.get(target).copied()
                        && let Some(tracker_id) = id_map.get(tracker).copied()
                        && let Some(target_st) = builder.states.get(target_id)
                        && let Some(tracker_st) = builder.states.get(tracker_id)
                    {
                        let tracker_entity = tracker_st.entity;
                        let center = Point::new(center.0, center.1);
                        let radius = *radius;
                        let start_angle = *start_angle;
                        let sweep_scale = *sweep_scale;
                        let sweep_offset = *sweep_offset;
                        let redraw = gaanim_animation::AlwaysRedrawRegen::new(move |world| {
                            let value = world
                                .get::<gaanim_animation::FloatSignal>(tracker_entity)
                                .map(|signal| signal.value)
                                .unwrap_or(0.0);
                            gaanim_objects::primitives::curved_arrow_arc(
                                gaanim_core::ObjectId::from_raw(0),
                                center,
                                radius,
                                start_angle,
                                value * sweep_scale + sweep_offset,
                            )
                            .path
                            .0
                            .as_ref()
                            .clone()
                        });
                        builder.commands.entity(target_st.entity).insert(redraw);
                    }
                }
                Op::AttachPointOnCurve {
                    target,
                    curve,
                    tracker,
                } => {
                    if let Some(target_id) = id_map.get(target).copied()
                        && let Some(curve_id) = id_map.get(curve).copied()
                        && let Some(tracker_id) = id_map.get(tracker).copied()
                        && let Some(target_st) = builder.states.get(target_id)
                        && let Some(curve_st) = builder.states.get(curve_id)
                        && let Some(tracker_st) = builder.states.get(tracker_id)
                    {
                        builder
                            .commands
                            .entity(target_st.entity)
                            .insert(PointOnCurve::new(curve_st.entity, tracker_st.entity));
                    }
                }
                Op::AttachTangentOnCurve {
                    target,
                    curve,
                    tracker,
                } => {
                    if let Some(target_id) = id_map.get(target).copied()
                        && let Some(curve_id) = id_map.get(curve).copied()
                        && let Some(tracker_id) = id_map.get(tracker).copied()
                        && let Some(target_st) = builder.states.get(target_id)
                        && let Some(curve_st) = builder.states.get(curve_id)
                        && let Some(tracker_st) = builder.states.get(tracker_id)
                    {
                        builder
                            .commands
                            .entity(target_st.entity)
                            .insert(TangentOnCurve::new(curve_st.entity, tracker_st.entity));
                    }
                }
                Op::AttachNormalOnCurve {
                    target,
                    curve,
                    tracker,
                } => {
                    if let Some(target_id) = id_map.get(target).copied()
                        && let Some(curve_id) = id_map.get(curve).copied()
                        && let Some(tracker_id) = id_map.get(tracker).copied()
                        && let Some(target_st) = builder.states.get(target_id)
                        && let Some(curve_st) = builder.states.get(curve_id)
                        && let Some(tracker_st) = builder.states.get(tracker_id)
                    {
                        builder
                            .commands
                            .entity(target_st.entity)
                            .insert(NormalOnCurve::new(curve_st.entity, tracker_st.entity));
                    }
                }
                Op::AttachCurvatureOnCurve {
                    target,
                    curve,
                    tracker,
                    window,
                } => {
                    if let Some(target_id) = id_map.get(target).copied()
                        && let Some(curve_id) = id_map.get(curve).copied()
                        && let Some(tracker_id) = id_map.get(tracker).copied()
                        && let Some(target_st) = builder.states.get(target_id)
                        && let Some(curve_st) = builder.states.get(curve_id)
                        && let Some(tracker_st) = builder.states.get(tracker_id)
                    {
                        builder
                            .commands
                            .entity(target_st.entity)
                            .insert(CurvatureOnCurve::new(
                                curve_st.entity,
                                tracker_st.entity,
                                *window,
                            ));
                    }
                }
            }
        }
    }

    fn remap_anim(
        anim: &AnimationBuilder,
        id_map: &HashMap<ObjectId, ObjectId>,
    ) -> Option<AnimationBuilder> {
        let target = *id_map.get(&anim.target)?;
        let anim_type = match &anim.anim_type {
            AnimationType::FadeTransform { target } => AnimationType::FadeTransform {
                target: *id_map.get(target)?,
            },
            AnimationType::Transform { target } => AnimationType::Transform {
                target: *id_map.get(target)?,
            },
            AnimationType::ReplacementTransform { target } => AnimationType::ReplacementTransform {
                target: *id_map.get(target)?,
            },
            other => other.clone(),
        };
        Some(AnimationBuilder {
            target,
            anim_type,
            duration: anim.duration,
            delay: anim.delay,
            rate_func: anim.rate_func.clone(),
        })
    }

    fn fragment_child_ids(
        builder: &mut SceneBuilder,
        id_map: &HashMap<ObjectId, ObjectId>,
        target: ObjectId,
        fragment: &str,
        occurrence: Option<usize>,
    ) -> Vec<ObjectId> {
        let Some(&target) = id_map.get(&target) else {
            return Vec::new();
        };
        builder
            .select_occurrence(MobjectRef { id: target }, fragment, occurrence)
            .child_ids
    }

    fn fade_cancellation_marks(
        builder: &mut SceneBuilder,
        cancellation_marks: &mut HashMap<ObjectId, Vec<ObjectId>>,
        source: ObjectId,
        transition_duration: f64,
    ) {
        let Some(marks) = cancellation_marks.remove(&source) else {
            return;
        };
        let duration = (transition_duration * 0.25).clamp(0.12, 0.3);
        for target in marks {
            builder.play_at_current_time(AnimationBuilder {
                target,
                anim_type: AnimationType::FadeOut,
                duration,
                rate_func: RateFunc::Smooth,
                delay: 0.0,
            });
        }
    }

    fn fade_canceled_term_children(
        builder: &mut SceneBuilder,
        canceled_term_children: &mut HashMap<ObjectId, Vec<ObjectId>>,
        source: ObjectId,
        transition_duration: f64,
    ) {
        let Some(children) = canceled_term_children.remove(&source) else {
            return;
        };
        let duration = (transition_duration * 0.25).clamp(0.12, 0.3);
        for target in children {
            builder.play_at_current_time(AnimationBuilder {
                target,
                anim_type: AnimationType::FadeOut,
                duration,
                rate_func: RateFunc::Smooth,
                delay: 0.0,
            });
        }
    }

    fn spawn_one(
        builder: &mut SceneBuilder,
        spec: &ObjectSpec,
        id_map: &HashMap<ObjectId, ObjectId>,
        frame_bounds: Bounds3D,
        text_config: &gaanim_text::prelude::TextConfig,
        scene_background: gaanim_core::peniko::Color,
    ) -> MobjectRef {
        match &spec.kind {
            SpawnKind::Circle(r) => {
                let b = builder.circle(*r);
                let mr = Self::finish_spawn_builder(b, spec);
                Self::apply_layout(builder, mr.id, spec, id_map, frame_bounds);
                mr
            }
            SpawnKind::Rect(w, h) => {
                let b = builder.rectangle(*w, *h);
                let mr = Self::finish_spawn_builder(b, spec);
                Self::apply_layout(builder, mr.id, spec, id_map, frame_bounds);
                mr
            }
            SpawnKind::RoundedRect(w, h, r) => {
                let b = builder.rounded_rect(*w, *h, *r);
                let mr = Self::finish_spawn_builder(b, spec);
                Self::apply_layout(builder, mr.id, spec, id_map, frame_bounds);
                mr
            }
            SpawnKind::Square(sz) => {
                let b = builder.square(*sz);
                let mr = Self::finish_spawn_builder(b, spec);
                Self::apply_layout(builder, mr.id, spec, id_map, frame_bounds);
                mr
            }
            SpawnKind::Dot(r) => {
                let b = builder.dot(*r);
                let mr = Self::finish_spawn_builder(b, spec);
                Self::apply_layout(builder, mr.id, spec, id_map, frame_bounds);
                mr
            }
            SpawnKind::Ellipse(rx, ry) => {
                let b = builder.ellipse(*rx, *ry);
                let mr = Self::finish_spawn_builder(b, spec);
                Self::apply_layout(builder, mr.id, spec, id_map, frame_bounds);
                mr
            }
            SpawnKind::Line(x1, y1, x2, y2) => {
                let b = builder.line(Point::new(*x1, *y1), Point::new(*x2, *y2));
                let mr = Self::finish_spawn_builder(b, spec);
                Self::apply_layout(builder, mr.id, spec, id_map, frame_bounds);
                mr
            }
            SpawnKind::Arrow(x1, y1, x2, y2) => {
                let b = builder.arrow(Point::new(*x1, *y1), Point::new(*x2, *y2));
                let mr = Self::finish_spawn_builder(b, spec);
                Self::apply_layout(builder, mr.id, spec, id_map, frame_bounds);
                mr
            }
            SpawnKind::DashedLine {
                start,
                end,
                dash_length,
                gap_length,
            } => {
                let b = builder.dashed_line(
                    Point::new(start.0, start.1),
                    Point::new(end.0, end.1),
                    *dash_length,
                    *gap_length,
                );
                let mr = Self::finish_spawn_builder(b, spec);
                Self::apply_layout(builder, mr.id, spec, id_map, frame_bounds);
                mr
            }
            SpawnKind::DoubleArrow {
                start,
                end,
                head_length,
                head_width,
            } => {
                let b = builder.double_arrow(
                    Point::new(start.0, start.1),
                    Point::new(end.0, end.1),
                    *head_length,
                    *head_width,
                );
                let mr = Self::finish_spawn_builder(b, spec);
                Self::apply_layout(builder, mr.id, spec, id_map, frame_bounds);
                mr
            }
            SpawnKind::Polygon(points) => {
                let points: Vec<Point> = points.iter().map(|&(x, y)| Point::new(x, y)).collect();
                let b = builder.polygon(&points);
                let mr = Self::finish_spawn_builder(b, spec);
                Self::apply_layout(builder, mr.id, spec, id_map, frame_bounds);
                mr
            }
            SpawnKind::Star {
                points,
                outer_radius,
                inner_radius,
            } => {
                let b = builder.star(*points, *outer_radius, *inner_radius);
                let mr = Self::finish_spawn_builder(b, spec);
                Self::apply_layout(builder, mr.id, spec, id_map, frame_bounds);
                mr
            }
            SpawnKind::RegularPolygon { sides, radius } => {
                let b = builder.regular_polygon(*sides, *radius);
                let mr = Self::finish_spawn_builder(b, spec);
                Self::apply_layout(builder, mr.id, spec, id_map, frame_bounds);
                mr
            }
            SpawnKind::Sector {
                center,
                radius,
                start_angle,
                sweep_angle,
            } => {
                let b = builder.sector(
                    Point::new(center.0, center.1),
                    *radius,
                    *start_angle,
                    *sweep_angle,
                );
                let mr = Self::finish_spawn_builder(b, spec);
                Self::apply_layout(builder, mr.id, spec, id_map, frame_bounds);
                mr
            }
            SpawnKind::Annulus {
                outer_radius,
                inner_radius,
            } => {
                let b = builder.annulus(*outer_radius, *inner_radius);
                let mr = Self::finish_spawn_builder(b, spec);
                Self::apply_layout(builder, mr.id, spec, id_map, frame_bounds);
                mr
            }
            SpawnKind::Brace { start, end, height } => {
                let b = builder.brace(
                    Point::new(start.0, start.1),
                    Point::new(end.0, end.1),
                    *height,
                );
                let mr = Self::finish_spawn_builder(b, spec);
                Self::apply_layout(builder, mr.id, spec, id_map, frame_bounds);
                mr
            }
            SpawnKind::Checkmark(size) => {
                let b = builder.checkmark(*size);
                let mr = Self::finish_spawn_builder(b, spec);
                Self::apply_layout(builder, mr.id, spec, id_map, frame_bounds);
                mr
            }
            SpawnKind::Cross(size) => {
                let b = builder.cross(*size);
                let mr = Self::finish_spawn_builder(b, spec);
                Self::apply_layout(builder, mr.id, spec, id_map, frame_bounds);
                mr
            }
            SpawnKind::RightAngle(arm_length) => {
                let b = builder.right_angle(*arm_length);
                let mr = Self::finish_spawn_builder(b, spec);
                Self::apply_layout(builder, mr.id, spec, id_map, frame_bounds);
                mr
            }
            SpawnKind::Arc {
                center,
                radius,
                start_angle,
                sweep_angle,
            } => {
                let b = builder.arc(
                    Point::new(center.0, center.1),
                    Vec2::new(*radius, *radius),
                    *start_angle,
                    *sweep_angle,
                    0.0,
                );
                let mr = Self::finish_spawn_builder(b, spec);
                Self::apply_layout(builder, mr.id, spec, id_map, frame_bounds);
                mr
            }
            SpawnKind::CurvedArrow(x1, y1, x2, y2, angle) => {
                let b = builder.curved_arrow(Point::new(*x1, *y1), Point::new(*x2, *y2), *angle);
                let mr = Self::finish_spawn_builder(b, spec);
                Self::apply_layout(builder, mr.id, spec, id_map, frame_bounds);
                mr
            }
            SpawnKind::CurvedArrowArc {
                center,
                radius,
                start_angle,
                sweep_angle,
            } => {
                let b = builder.curved_arrow_arc(
                    Point::new(center.0, center.1),
                    *radius,
                    *start_angle,
                    *sweep_angle,
                );
                let mr = Self::finish_spawn_builder(b, spec);
                Self::apply_layout(builder, mr.id, spec, id_map, frame_bounds);
                mr
            }
            SpawnKind::Dimension { start, end, offset } => {
                let dx = end.0 - start.0;
                let dy = end.1 - start.1;
                let length = dx.hypot(dy);
                if length <= f64::EPSILON {
                    let b = builder.line(Point::new(start.0, start.1), Point::new(end.0, end.1));
                    let mr = Self::finish_spawn_builder(b, spec);
                    Self::apply_layout(builder, mr.id, spec, id_map, frame_bounds);
                    mr
                } else {
                    let normal = (-dy / length, dx / length);
                    let dimension_start =
                        Point::new(start.0 + normal.0 * *offset, start.1 + normal.1 * *offset);
                    let dimension_end =
                        Point::new(end.0 + normal.0 * *offset, end.1 + normal.1 * *offset);
                    let color = PenikoColor::from_rgb8(0x80, 0x80, 0x80);
                    let extension_a = builder
                        .line(Point::new(start.0, start.1), dimension_start)
                        .no_fill()
                        .stroke(color, 2.0)
                        .spawn();
                    let extension_b = builder
                        .line(Point::new(end.0, end.1), dimension_end)
                        .no_fill()
                        .stroke(color, 2.0)
                        .spawn();
                    let measurement = builder
                        .double_arrow(dimension_start, dimension_end, Some(12.0), Some(10.0))
                        .fill(color)
                        .no_stroke()
                        .spawn();
                    let group = builder.group(&[extension_a, extension_b, measurement]);
                    Self::post_apply(builder, group.id, spec, id_map, frame_bounds);
                    group
                }
            }
            SpawnKind::Polyline(points) => {
                let points: Vec<Point> = points.iter().map(|&(x, y)| Point::new(x, y)).collect();
                let b = builder.open_path(&points);
                let mr = Self::finish_spawn_builder(b, spec);
                Self::apply_layout(builder, mr.id, spec, id_map, frame_bounds);
                mr
            }
            SpawnKind::Bezier {
                start,
                controls,
                end,
            } => {
                let mut path = gaanim_core::kurbo::BezPath::new();
                path.move_to(Point::new(start.0, start.1));
                match controls.as_slice() {
                    [control] => {
                        path.quad_to(Point::new(control.0, control.1), Point::new(end.0, end.1))
                    }
                    [control1, control2] => path.curve_to(
                        Point::new(control1.0, control1.1),
                        Point::new(control2.0, control2.1),
                        Point::new(end.0, end.1),
                    ),
                    _ => path.line_to(Point::new(end.0, end.1)),
                }
                let rect = path.bounding_box();
                let svg_path = gaanim_objects::prelude::SvgPath {
                    id: "Bezier".to_string(),
                    path,
                    bounds: Bounds3D::new_2d(rect.x0, rect.y0, rect.x1, rect.y1),
                    fill: None,
                    stroke: StrokeBrush::transparent(),
                };
                let b = builder.svg_path(&svg_path);
                let mr = Self::finish_spawn_builder(b, spec);
                Self::apply_layout(builder, mr.id, spec, id_map, frame_bounds);
                mr
            }
            SpawnKind::Curve(elements) => {
                let mut path = gaanim_core::kurbo::BezPath::new();
                let mut cursor = Point::ORIGIN;
                let mut subpath_start = Point::ORIGIN;
                let mut last_quad = None;
                let mut last_cubic = None;
                let resolve = |point: (f64, f64), relative: bool, cursor: Point| {
                    if relative {
                        Point::new(cursor.x + point.0, cursor.y + point.1)
                    } else {
                        Point::new(point.0, point.1)
                    }
                };
                for element in elements {
                    match element {
                        crate::canvas::CurveElement::Move { to, relative } => {
                            cursor = resolve(*to, *relative, cursor);
                            subpath_start = cursor;
                            path.move_to(cursor);
                            last_quad = None;
                            last_cubic = None;
                        }
                        crate::canvas::CurveElement::Line { to, relative } => {
                            cursor = resolve(*to, *relative, cursor);
                            path.line_to(cursor);
                            last_quad = None;
                            last_cubic = None;
                        }
                        crate::canvas::CurveElement::Quad {
                            control,
                            to,
                            relative,
                        } => {
                            let end = resolve(*to, *relative, cursor);
                            let control = match control {
                                crate::canvas::CurveControl::None => end,
                                crate::canvas::CurveControl::Auto => last_quad
                                    .map(|p: Point| {
                                        Point::new(2.0 * cursor.x - p.x, 2.0 * cursor.y - p.y)
                                    })
                                    .unwrap_or(cursor),
                                crate::canvas::CurveControl::Point(point) => {
                                    resolve(*point, *relative, cursor)
                                }
                            };
                            path.quad_to(control, end);
                            cursor = end;
                            last_quad = Some(control);
                            last_cubic = None;
                        }
                        crate::canvas::CurveElement::Cubic {
                            control_start,
                            control_end,
                            to,
                            relative,
                        } => {
                            let end = resolve(*to, *relative, cursor);
                            let start = match control_start {
                                crate::canvas::CurveControl::None => cursor,
                                crate::canvas::CurveControl::Auto => last_cubic
                                    .map(|p: Point| {
                                        Point::new(2.0 * cursor.x - p.x, 2.0 * cursor.y - p.y)
                                    })
                                    .unwrap_or(cursor),
                                crate::canvas::CurveControl::Point(point) => {
                                    resolve(*point, *relative, cursor)
                                }
                            };
                            let finish = match control_end {
                                crate::canvas::CurveControl::None => end,
                                crate::canvas::CurveControl::Auto => end,
                                crate::canvas::CurveControl::Point(point) => {
                                    resolve(*point, *relative, cursor)
                                }
                            };
                            path.curve_to(start, finish, end);
                            cursor = end;
                            last_cubic = Some(finish);
                            last_quad = None;
                        }
                        crate::canvas::CurveElement::Close { smooth } => {
                            if *smooth {
                                let control = last_cubic
                                    .or(last_quad)
                                    .map(|p: Point| {
                                        Point::new(2.0 * cursor.x - p.x, 2.0 * cursor.y - p.y)
                                    })
                                    .unwrap_or(cursor);
                                path.curve_to(control, subpath_start, subpath_start);
                            } else {
                                path.close_path();
                            }
                            cursor = subpath_start;
                            last_quad = None;
                            last_cubic = None;
                        }
                    }
                }
                let rect = path.bounding_box();
                let svg_path = gaanim_objects::prelude::SvgPath {
                    id: "Curve".to_string(),
                    path,
                    bounds: Bounds3D::new_2d(rect.x0, rect.y0, rect.x1, rect.y1),
                    fill: None,
                    stroke: StrokeBrush::transparent(),
                };
                let b = builder.svg_path(&svg_path);
                let mr = Self::finish_spawn_builder(b, spec);
                Self::apply_layout(builder, mr.id, spec, id_map, frame_bounds);
                mr
            }
            SpawnKind::Axes {
                x_range,
                y_range,
                config,
            } => {
                let axes = builder.axes(*x_range, *y_range, config.numbers, config.ticks);
                if let Some(axis_state) = builder.states.get_mut(axes.id) {
                    let stroke = StrokeBrush::new(config.axis_color, config.axis_width);
                    axis_state.stroke = stroke.clone();
                    builder.commands.entity(axis_state.entity).insert(stroke);
                }
                if config.grid {
                    let grid = Self::finish_spawn_builder(
                        builder
                            .number_plane(*x_range, *y_range, config.axis_width, config.grid_width)
                            .stroke(config.grid_color, config.grid_width),
                        spec,
                    );
                    let group = builder.group(&[grid, axes]);
                    Self::post_apply(builder, group.id, spec, id_map, frame_bounds);
                    group
                } else {
                    Self::post_apply(builder, axes.id, spec, id_map, frame_bounds);
                    axes
                }
            }
            SpawnKind::Text(t) => {
                let role = gaanim_text::prelude::TextRole::Body;
                let mr = builder.spawn_text(t, role);
                let styled_spec =
                    Self::with_default_text_fill(spec, text_config.roles[&role].fill_color);
                Self::post_apply(builder, mr.id, &styled_spec, id_map, frame_bounds);
                Self::apply_fragment_fills(builder, mr, &styled_spec);
                mr
            }
            SpawnKind::Paragraph { text, options } => {
                let body = &text_config.roles[&gaanim_text::prelude::TextRole::Body];
                let font_size = options.font_size.unwrap_or(body.size).max(1.0);
                let font_family = options.font_family.as_deref().unwrap_or(&body.font_family);
                let source = paragraph_typst_source(text, options, font_size);
                let mr = builder.typst(
                    &source,
                    false,
                    Some(font_family),
                    None,
                    Some(font_size),
                    None,
                );
                let mut paragraph_spec = spec.clone();
                if !paragraph_spec.fill_overridden {
                    paragraph_spec.fill = Some(gaanim_core::peniko::Brush::Solid(body.fill_color));
                    paragraph_spec.fill_overridden = true;
                }
                Self::post_apply(builder, mr.id, &paragraph_spec, id_map, frame_bounds);
                Self::apply_fragment_fills(builder, mr, &paragraph_spec);
                mr
            }
            SpawnKind::Title(t) => {
                let role = gaanim_text::prelude::TextRole::Title;
                let mr = builder.spawn_text(t, role);
                let styled_spec =
                    Self::with_default_text_fill(spec, text_config.roles[&role].fill_color);
                Self::post_apply(builder, mr.id, &styled_spec, id_map, frame_bounds);
                Self::apply_fragment_fills(builder, mr, &styled_spec);
                mr
            }
            SpawnKind::Subtitle(t) => {
                let role = gaanim_text::prelude::TextRole::Subtitle;
                let mr = builder.spawn_text(t, role);
                let styled_spec =
                    Self::with_default_text_fill(spec, text_config.roles[&role].fill_color);
                Self::post_apply(builder, mr.id, &styled_spec, id_map, frame_bounds);
                Self::apply_fragment_fills(builder, mr, &styled_spec);
                mr
            }
            SpawnKind::Equation(f) => {
                let mr = builder.equation(f);
                let math = &text_config.roles[&gaanim_text::prelude::TextRole::Math];
                let styled_spec = Self::with_default_text_fill(spec, math.fill_color);
                Self::post_apply(builder, mr.id, &styled_spec, id_map, frame_bounds);
                Self::apply_fragment_fills(builder, mr, &styled_spec);
                mr
            }
            SpawnKind::Typst(source) => {
                let body = &text_config.roles[&gaanim_text::prelude::TextRole::Body];
                // Let Typst retain every explicit fill from document markup
                // (including package-provided shapes). Only unstyled text gets
                // a foreground that contrasts with the scene background.
                let foreground = typst_foreground_for_background(scene_background);
                let source = format!("#set text(fill: rgb(\"{foreground}\"))\n{source}");
                let mr = builder.typst(
                    &source,
                    false,
                    Some(&body.font_family),
                    None,
                    Some(body.size),
                    None,
                );
                Self::post_apply(builder, mr.id, spec, id_map, frame_bounds);
                Self::apply_fragment_fills(builder, mr, spec);
                mr
            }
            SpawnKind::Image { image, view } => {
                let b = builder.image(image.clone(), *view);
                let mr = Self::finish_spawn_builder(b, spec);
                Self::apply_layout(builder, mr.id, spec, id_map, frame_bounds);
                mr
            }
            SpawnKind::SvgPath(path) => {
                let b = builder.svg_path(path);
                let mr = Self::finish_spawn_builder(b, spec);
                Self::apply_layout(builder, mr.id, spec, id_map, frame_bounds);
                mr
            }
            SpawnKind::Group(ids) => {
                let refs: Vec<MobjectRef> = ids
                    .iter()
                    .filter_map(|id| id_map.get(id).copied().map(|id| MobjectRef { id }))
                    .collect();
                let mr = builder.group(&refs);
                Self::apply_group_arrangement(builder, mr.id, spec);
                Self::post_apply(builder, mr.id, spec, id_map, frame_bounds);
                mr
            }
            SpawnKind::ValueTracker(initial) => {
                // Spawn a FloatSignal entity (no visual output).
                let new_id = builder.next_id();
                let entity = builder
                    .commands
                    .spawn((
                        gaanim_scene::MobjectId(new_id),
                        gaanim_animation::FloatSignal::new(*initial),
                    ))
                    .id();
                builder.tag_entity(entity);
                builder.states.insert(
                    new_id,
                    MobjectState {
                        path: std::sync::Arc::new(gaanim_core::kurbo::BezPath::new()),
                        bounds: Bounds3D::default(),
                        transform: SpatialTransform::default(),
                        opacity: 1.0,
                        fill: None,
                        stroke: StrokeBrush::default(),
                        entity,
                        child_spans: Vec::new(),
                        children: Vec::new(),
                        parent: None,
                    },
                );
                builder.float_signals.insert(new_id, *initial);
                MobjectRef { id: new_id }
            }
            SpawnKind::TracedPathLine => {
                // Spawn a minimal line (0,0)→(0,0). TracedPath will overwrite its Path2D.
                let b = builder.line(Point::new(0.0, 0.0), Point::new(0.0, 0.0));
                let mr = Self::finish_spawn_builder(b, spec);
                mr
            }
            SpawnKind::TrackingLine => {
                // Spawn a minimal line (0,0)→(0,0). TrackingLine will overwrite its Path2D.
                let b = builder.line(Point::new(0.0, 0.0), Point::new(0.0, 0.0));
                let mr = Self::finish_spawn_builder(b, spec);
                mr
            }
        }
    }

    fn finish_spawn_builder<'b, 'w, 's, 'a>(
        mut b: crate::builder::MobjectSpawnBuilder<'b, 'w, 's, 'a>,
        spec: &ObjectSpec,
    ) -> MobjectRef {
        if spec.stroke_overridden {
            if let Some((c, w)) = spec.stroke {
                b = b.stroke(c, w);
            } else {
                b = b.no_stroke();
            }
        }
        if spec.fill_overridden {
            if let Some(ref f) = spec.fill {
                b = b.fill_brush(f.clone());
            } else {
                b = b.no_fill();
            }
        }
        b = b.opacity(spec.opacity).z_index(spec.z_index);
        b.spawn()
    }

    /// Applies deferred glyph-level color overrides after the normal object
    /// style has been propagated to the compiled text hierarchy.
    fn apply_fragment_fills(builder: &mut SceneBuilder, target: MobjectRef, spec: &ObjectSpec) {
        for (fragment, color) in &spec.fragment_fills {
            builder.select(target, fragment).set_fill(*color);
        }
    }

    fn with_default_text_fill(spec: &ObjectSpec, color: PenikoColor) -> ObjectSpec {
        let mut styled = spec.clone();
        if !styled.fill_overridden {
            styled.fill = Some(gaanim_core::peniko::Brush::Solid(color));
            styled.fill_overridden = true;
        }
        styled
    }

    fn post_apply(
        builder: &mut SceneBuilder,
        id: ObjectId,
        spec: &ObjectSpec,
        id_map: &HashMap<ObjectId, ObjectId>,
        frame_bounds: Bounds3D,
    ) {
        let mut child_spans = Vec::new();
        if let Some(st) = builder.states.get_mut(id) {
            child_spans = st.child_spans.clone();
            let is_textual_hierarchy = !child_spans.is_empty();
            if spec.stroke_overridden {
                if let Some((c, w)) = spec.stroke {
                    let sb = StrokeBrush::new(c, w);
                    st.stroke = sb.clone();
                    if !is_textual_hierarchy {
                        builder.commands.entity(st.entity).insert(sb);
                    }
                } else {
                    st.stroke = StrokeBrush::transparent();
                    if !is_textual_hierarchy {
                        builder
                            .commands
                            .entity(st.entity)
                            .insert(StrokeBrush::transparent());
                    }
                }
            }
            if spec.fill_overridden {
                if let Some(ref f) = spec.fill {
                    st.fill = Some(f.clone());
                    if !is_textual_hierarchy {
                        builder
                            .commands
                            .entity(st.entity)
                            .insert(FillBrush(Some(f.clone())));
                    }
                } else {
                    st.fill = None;
                    if !is_textual_hierarchy {
                        builder
                            .commands
                            .entity(st.entity)
                            .insert(FillBrush::transparent());
                    }
                }
            }
            if spec.opacity != 1.0 {
                st.opacity = spec.opacity;
                builder
                    .commands
                    .entity(st.entity)
                    .insert(Opacity(spec.opacity));
            }
            if spec.z_index != 0 {
                builder.commands.entity(st.entity).insert(RenderOrder {
                    z_index: spec.z_index,
                    ..Default::default()
                });
            }
        }
        if spec.fill_overridden {
            for child in &child_spans {
                if let Some(child_state) = builder.states.get_mut(child.id) {
                    child_state.fill = spec.fill.clone();
                }
                builder
                    .commands
                    .entity(child.entity)
                    .insert(if let Some(ref f) = spec.fill {
                        FillBrush(Some(f.clone()))
                    } else {
                        FillBrush::transparent()
                    });
            }
        }
        if spec.opacity != 1.0 {
            for child in &child_spans {
                if let Some(child_state) = builder.states.get_mut(child.id) {
                    child_state.opacity = spec.opacity;
                }
                builder
                    .commands
                    .entity(child.entity)
                    .insert(Opacity(spec.opacity));
            }
        }
        if spec.stroke_overridden {
            for child in &child_spans {
                let sb = if let Some((c, w)) = spec.stroke {
                    StrokeBrush::new(c, w)
                } else {
                    StrokeBrush::transparent()
                };
                if let Some(child_state) = builder.states.get_mut(child.id) {
                    child_state.stroke = sb.clone();
                }
                builder.commands.entity(child.entity).insert(sb);
            }
        }
        Self::apply_layout(builder, id, spec, id_map, frame_bounds);
    }

    fn apply_layout(
        builder: &mut SceneBuilder,
        id: ObjectId,
        spec: &ObjectSpec,
        id_map: &HashMap<ObjectId, ObjectId>,
        frame_bounds: Bounds3D,
    ) {
        if spec.layout_ops.is_empty() {
            return;
        }

        let Some(state) = builder.states.get(id) else {
            return;
        };
        let bounds = state.bounds;
        let original_transform = state.transform;
        let entity = state.entity;
        let mut transform = original_transform;
        let mut pivot_in_scene = None;

        for op in &spec.layout_ops {
            match op {
                LayoutOp::SetTranslation(translation) => {
                    transform.translation = *translation;
                }
                LayoutOp::SetScale(factor) => {
                    transform.scale = original_transform.scale * *factor;
                }
                LayoutOp::SetRotation(radians) => {
                    transform.rotation = gaanim_core::glam::DQuat::from_rotation_z(*radians);
                }
                LayoutOp::SetPivot(pivot) => {
                    pivot_in_scene = Some(*pivot);
                }
                LayoutOp::MoveAnchorTo { target, anchor } => {
                    transform =
                        gaanim_layout::compute_move_to(bounds, &transform, *target, *anchor);
                }
                LayoutOp::NextTo {
                    reference,
                    direction,
                    spacing,
                    aligned_edge,
                } => {
                    let Some(reference_id) = id_map.get(reference).copied() else {
                        bevy::prelude::warn!(
                            "Canvas layout skipped: reference object {:?} was not spawned before {:?}",
                            reference,
                            spec.id
                        );
                        continue;
                    };
                    let Some(reference_state) = builder.states.get(reference_id) else {
                        bevy::prelude::warn!(
                            "Canvas layout skipped: missing state for reference object {:?}",
                            reference_id
                        );
                        continue;
                    };
                    let reference_transform = builder.get_world_transform(reference_id);
                    let shift = gaanim_layout::compute_next_to_new(
                        bounds,
                        &transform,
                        reference_state.bounds,
                        &reference_transform,
                        *direction,
                        *spacing,
                        *aligned_edge,
                    );
                    transform = transform.shift_3d(shift);
                }
                LayoutOp::AlignTo {
                    reference,
                    target_anchor,
                    reference_anchor,
                } => {
                    let Some(reference_id) = id_map.get(reference).copied() else {
                        bevy::prelude::warn!(
                            "Canvas layout skipped: reference object {:?} was not spawned before {:?}",
                            reference,
                            spec.id
                        );
                        continue;
                    };
                    let Some(reference_state) = builder.states.get(reference_id) else {
                        bevy::prelude::warn!(
                            "Canvas layout skipped: missing state for reference object {:?}",
                            reference_id
                        );
                        continue;
                    };
                    let reference_transform = builder.get_world_transform(reference_id);
                    let shift = gaanim_layout::compute_align_to_new(
                        bounds,
                        &transform,
                        reference_state.bounds,
                        &reference_transform,
                        *target_anchor,
                        *reference_anchor,
                    );
                    transform = transform.shift_3d(shift);
                }
                LayoutOp::ToEdge { direction, buff } => {
                    transform = gaanim_layout::compute_to_edge(
                        bounds,
                        &transform,
                        *direction,
                        *buff,
                        frame_bounds,
                    );
                }
                LayoutOp::ToCorner { corner, buff } => {
                    transform = gaanim_layout::compute_to_corner(
                        bounds,
                        &transform,
                        *corner,
                        *buff,
                        frame_bounds,
                    );
                }
                LayoutOp::Arrange { .. } => {
                    // Group child placement is resolved before this group's own
                    // bounds are positioned by the other layout operations.
                }
            }
        }

        if let Some(pivot) = pivot_in_scene {
            // SpatialTransform stores anchors in local coordinates, while the
            // public API accepts the stable scene-space point users see.
            transform.anchor = pivot - transform.translation;
        }

        if transform != original_transform {
            if let Some(state) = builder.states.get_mut(id) {
                state.transform = transform;
            }
            builder.commands.entity(entity).insert(transform);
        }
    }

    fn apply_group_arrangement(builder: &mut SceneBuilder, id: ObjectId, spec: &ObjectSpec) {
        for op in &spec.layout_ops {
            if let LayoutOp::Arrange {
                direction,
                spacing,
                aligned_edge,
            } = op
            {
                builder.arrange_aligned(MobjectRef { id }, *direction, *spacing, *aligned_edge);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::ecs::world::CommandQueue;
    use gaanim_layout::Anchor;
    use gaanim_scene::LocalBounds;

    #[test]
    fn justified_paragraph_compiles_to_vector_glyphs() {
        let mut canvas = Canvas::new(640, 360);
        canvas.paragraph(
            "Este párrafo debe ocupar varias líneas y conservar glifos vectoriales.",
            ParagraphOptions {
                width: 180.0,
                align: TextAlign::Justify,
                line_spacing: 1.25,
                font_size: Some(28.0),
                font_family: None,
                max_lines: None,
                overflow: ParagraphOverflow::Clip,
            },
        );

        let world = World::new();
        let mut queue = CommandQueue::default();
        let mut commands = Commands::new(&mut queue, &world);
        let mut timeline = Timeline::new();
        let fonts = gaanim_text::font::FontRegistry::new();
        let text_config = gaanim_text::prelude::TextConfig::default();
        canvas.compile_into(&mut commands, &mut timeline, &fonts, &text_config);
        drop(commands);

        let mut world = world;
        queue.apply(&mut world);
        let mut query = world.query::<&LocalBounds>();
        let visible_bounds = query
            .iter(&world)
            .filter(|bounds| bounds.0.width() > 0.0 && bounds.0.height() > 0.0)
            .count();
        assert!(visible_bounds > 5, "paragraph should produce vector glyphs");
    }

    #[test]
    fn paragraph_max_lines_emits_a_clipped_text_box() {
        let source = paragraph_typst_source(
            "A bounded paragraph",
            &ParagraphOptions {
                width: 240.0,
                align: TextAlign::Left,
                line_spacing: 1.2,
                font_size: Some(30.0),
                font_family: None,
                max_lines: Some(2),
                overflow: ParagraphOverflow::Clip,
            },
            30.0,
        );

        assert!(source.contains("height: 72pt"));
        assert!(source.contains("clip: true"));
    }

    #[test]
    fn equation_fragment_fill_overrides_matching_vector_glyphs() {
        let highlight = gaanim_core::peniko::Color::from_rgb8(255, 180, 0);
        let mut canvas = Canvas::new(640, 360);
        canvas.equation("E = m c^2").color_by("m", highlight);

        let world = World::new();
        let mut queue = CommandQueue::default();
        let mut commands = Commands::new(&mut queue, &world);
        let mut timeline = Timeline::new();
        let fonts = gaanim_text::font::FontRegistry::new();
        let text_config = gaanim_text::prelude::TextConfig::default();
        canvas.compile_into(&mut commands, &mut timeline, &fonts, &text_config);
        drop(commands);

        let mut world = world;
        queue.apply(&mut world);
        let mut query = world.query::<&gaanim_scene::FillBrush>();
        assert!(query.iter(&world).any(|fill| {
            matches!(
                &fill.0,
                Some(gaanim_core::peniko::Brush::Solid(color)) if *color == highlight
            )
        }));
    }

    #[test]
    fn text_fragment_fill_overrides_matching_vector_glyphs() {
        let highlight = gaanim_core::peniko::Color::from_rgb8(64, 180, 255);
        let mut canvas = Canvas::new(640, 360);
        canvas
            .text("Energy depends on mass")
            .color_by("mass", highlight);

        let world = World::new();
        let mut queue = CommandQueue::default();
        let mut commands = Commands::new(&mut queue, &world);
        let mut timeline = Timeline::new();
        let fonts = gaanim_text::font::FontRegistry::new();
        let text_config = gaanim_text::prelude::TextConfig::default();
        canvas.compile_into(&mut commands, &mut timeline, &fonts, &text_config);
        drop(commands);

        let mut world = world;
        queue.apply(&mut world);
        let mut query = world.query::<&gaanim_scene::FillBrush>();
        assert!(query.iter(&world).any(|fill| {
            matches!(
                &fill.0,
                Some(gaanim_core::peniko::Brush::Solid(color)) if *color == highlight
            )
        }));
    }

    #[test]
    fn paper_theme_applies_role_fills_to_text_glyphs() {
        let mut canvas = Canvas::new(640, 360);
        canvas
            .set_theme("paper")
            .expect("paper is a built-in theme");
        canvas.title("Heading");
        canvas.subtitle("Subheading");
        canvas.text("Body copy");
        canvas.equation("x = y");

        let world = World::new();
        let mut queue = CommandQueue::default();
        let mut commands = Commands::new(&mut queue, &world);
        let mut timeline = Timeline::new();
        let fonts = gaanim_text::font::FontRegistry::new();
        let text_config = canvas.themed_text_config();
        canvas.compile_into(&mut commands, &mut timeline, &fonts, &text_config);
        drop(commands);

        let mut world = world;
        queue.apply(&mut world);
        let fills: Vec<_> = world
            .query::<&gaanim_scene::FillBrush>()
            .iter(&world)
            .filter_map(|fill| match fill.0 {
                Some(gaanim_core::peniko::Brush::Solid(color)) => Some(color),
                _ => None,
            })
            .collect();

        assert!(fills.contains(&PenikoColor::BLACK));
        assert!(!fills.contains(&PenikoColor::WHITE));
    }

    #[test]
    fn fragment_transform_moves_selected_vector_glyphs() {
        let mut canvas = Canvas::new(640, 360);
        let source = canvas.equation("E = m c^2");
        let target = canvas.equation("p = m v");
        source.select("m").transform_to(&target.select("m"), 0.8);

        let world = World::new();
        let mut queue = CommandQueue::default();
        let mut commands = Commands::new(&mut queue, &world);
        let mut timeline = Timeline::new();
        let fonts = gaanim_text::font::FontRegistry::new();
        let text_config = gaanim_text::prelude::TextConfig::default();
        canvas.compile_into(&mut commands, &mut timeline, &fonts, &text_config);

        assert!(timeline.clips.values().any(|clip| {
            matches!(
                &clip.payload,
                gaanim_timeline::clip::ClipPayload::Animation(
                    gaanim_timeline::clip::AnimationSpec {
                        lens: gaanim_timeline::clip::PropertyLensSpec::Translation { .. },
                        ..
                    }
                )
            )
        }));
    }

    #[test]
    fn named_fragment_tag_resolves_to_a_vector_selection() {
        let highlight = gaanim_core::peniko::Color::from_rgb8(64, 180, 255);
        let mut canvas = Canvas::new(640, 360);
        let formula = canvas.equation("E = m c^2").define_tag("mass", "m", None);
        formula
            .tag("mass")
            .expect("registered tag should resolve")
            .fill(highlight);

        let world = World::new();
        let mut queue = CommandQueue::default();
        let mut commands = Commands::new(&mut queue, &world);
        let mut timeline = Timeline::new();
        let fonts = gaanim_text::font::FontRegistry::new();
        let text_config = gaanim_text::prelude::TextConfig::default();
        canvas.compile_into(&mut commands, &mut timeline, &fonts, &text_config);
        drop(commands);

        let mut world = world;
        queue.apply(&mut world);
        let mut query = world.query::<&gaanim_scene::FillBrush>();
        assert!(query.iter(&world).any(|fill| {
            matches!(
                &fill.0,
                Some(gaanim_core::peniko::Brush::Solid(color)) if *color == highlight
            )
        }));
    }

    #[test]
    fn cancel_term_places_a_diagonal_strike_over_the_selected_glyphs() {
        let strike_color = PenikoColor::WHITE;
        let mut canvas = Canvas::new(640, 360);
        let formula = canvas
            .equation("x + 3 = 7")
            .define_tag("constant", "3", None);
        formula
            .tag("constant")
            .expect("registered tag should resolve")
            .cancel(0.6);

        let world = World::new();
        let mut queue = CommandQueue::default();
        let mut commands = Commands::new(&mut queue, &world);
        let mut timeline = Timeline::new();
        let fonts = gaanim_text::font::FontRegistry::new();
        let text_config = gaanim_text::prelude::TextConfig::default();
        canvas.compile_into(&mut commands, &mut timeline, &fonts, &text_config);
        drop(commands);

        let mut world = world;
        queue.apply(&mut world);
        let mut query = world.query::<(&gaanim_scene::StrokeBrush, &LocalBounds)>();
        let bounds = query
            .iter(&world)
            .find_map(|(stroke, bounds)| {
                matches!(&stroke.brush, Some(gaanim_core::peniko::Brush::Solid(color)) if *color == strike_color)
                    .then_some(bounds.0)
            })
            .expect("cancel should spawn a coral strikethrough");
        assert!(bounds.center().x.abs() < 100.0);
        assert!(bounds.width() > 0.0 && bounds.height() > 0.0);
    }

    #[test]
    fn tagged_equation_transform_moves_shared_tags() {
        let mut canvas = Canvas::new(640, 360);
        let source = canvas
            .equation("E = m c^2")
            .define_tag("mass", "m", None)
            .at(0.0, 70.0);
        let target = canvas
            .equation("p = m v")
            .define_tag("mass", "m", None)
            .at(0.0, -90.0);
        canvas.transform_equation_tags(&source, &target, None, 0.8);

        let world = World::new();
        let mut queue = CommandQueue::default();
        let mut commands = Commands::new(&mut queue, &world);
        let mut timeline = Timeline::new();
        let fonts = gaanim_text::font::FontRegistry::new();
        let text_config = gaanim_text::prelude::TextConfig::default();
        canvas.compile_into(&mut commands, &mut timeline, &fonts, &text_config);

        assert!(timeline.clips.values().any(|clip| {
            matches!(
                &clip.payload,
                gaanim_timeline::clip::ClipPayload::Animation(
                    gaanim_timeline::clip::AnimationSpec {
                        lens: gaanim_timeline::clip::PropertyLensSpec::Translation { from, to },
                        ..
                    }
                ) if from.y > to.y
            )
        }));
    }

    #[test]
    fn equation_expansion_cross_fades_equations_around_tag() {
        let mut canvas = Canvas::new(640, 360);
        let source = canvas.equation("E = m c^2").define_tag("mass", "m", None);
        let target = canvas
            .equation("E = (m_1 + m_2) c^2")
            .define_tag("mass", "m", None);
        canvas.expand_equation_tag(&source, &target, "mass", 0.8);

        let world = World::new();
        let mut queue = CommandQueue::default();
        let mut commands = Commands::new(&mut queue, &world);
        let mut timeline = Timeline::new();
        let fonts = gaanim_text::font::FontRegistry::new();
        let text_config = gaanim_text::prelude::TextConfig::default();
        canvas.compile_into(&mut commands, &mut timeline, &fonts, &text_config);

        let opacity_clips = timeline
            .clips
            .values()
            .filter(|clip| {
                matches!(
                    &clip.payload,
                    gaanim_timeline::clip::ClipPayload::Animation(
                        gaanim_timeline::clip::AnimationSpec {
                            lens: gaanim_timeline::clip::PropertyLensSpec::Opacity { .. },
                            ..
                        }
                    )
                )
            })
            .count();
        assert!(opacity_clips > 2, "every equation glyph should cross-fade");
        assert!(timeline.clips.values().any(|clip| {
            matches!(
                &clip.payload,
                gaanim_timeline::clip::ClipPayload::Animation(
                    gaanim_timeline::clip::AnimationSpec {
                        lens: gaanim_timeline::clip::PropertyLensSpec::Translation { .. },
                        ..
                    }
                )
            )
        }));
    }

    #[test]
    fn equation_step_matches_common_glyphs() {
        let mut canvas = Canvas::new(640, 360);
        let source = canvas.equation("x + 3 = 7");
        let target = canvas.equation("x = 4");
        canvas.step_equation(&source, &target, 0.8);

        let world = World::new();
        let mut queue = CommandQueue::default();
        let mut commands = Commands::new(&mut queue, &world);
        let mut timeline = Timeline::new();
        let fonts = gaanim_text::font::FontRegistry::new();
        let text_config = gaanim_text::prelude::TextConfig::default();
        canvas.compile_into(&mut commands, &mut timeline, &fonts, &text_config);

        assert!(timeline.clips.values().any(|clip| {
            matches!(
                &clip.payload,
                gaanim_timeline::clip::ClipPayload::Animation(
                    gaanim_timeline::clip::AnimationSpec {
                        lens: gaanim_timeline::clip::PropertyLensSpec::Translation { .. },
                        ..
                    }
                )
            )
        }));
    }

    #[test]
    fn fade_in_from_down_schedules_opacity_and_upward_translation() {
        let mut canvas = Canvas::new(640, 360);
        let label = canvas.text("Aparece desde abajo");
        label.fade_in_from(crate::canvas::Direction::Down, 72.0, 0.8);

        let world = World::new();
        let mut queue = CommandQueue::default();
        let mut commands = Commands::new(&mut queue, &world);
        let mut timeline = Timeline::new();
        let fonts = gaanim_text::font::FontRegistry::new();
        let text_config = gaanim_text::prelude::TextConfig::default();
        canvas.compile_into(&mut commands, &mut timeline, &fonts, &text_config);

        assert!(timeline.clips.values().any(|clip| {
            matches!(
                &clip.payload,
                gaanim_timeline::clip::ClipPayload::Animation(
                    gaanim_timeline::clip::AnimationSpec {
                        lens: gaanim_timeline::clip::PropertyLensSpec::Opacity { from, to },
                        ..
                    }
                ) if *from == 0.0 && *to == 1.0
            )
        }));
        assert!(timeline.clips.values().any(|clip| {
            matches!(
                &clip.payload,
                gaanim_timeline::clip::ClipPayload::Animation(
                    gaanim_timeline::clip::AnimationSpec {
                        lens: gaanim_timeline::clip::PropertyLensSpec::Translation { from, to },
                        ..
                    }
                ) if from.y < to.y
            )
        }));
    }

    #[test]
    fn vstack_updates_group_bounds_before_region_placement() {
        let mut canvas = Canvas::new(640, 360);
        let first = canvas.rect(80.0, 30.0);
        let second = canvas.rect(80.0, 30.0);
        let stack = canvas.group(&[&first, &second]).vstack(20.0, Anchor::Left);
        let layout = canvas.layout(0.0, 0.0, 0.0);
        let _ = layout.content.place(stack, Anchor::TopLeft);

        let world = World::new();
        let mut queue = CommandQueue::default();
        let mut commands = Commands::new(&mut queue, &world);
        let mut timeline = Timeline::new();
        let fonts = gaanim_text::font::FontRegistry::new();
        let text_config = gaanim_text::prelude::TextConfig::default();
        canvas.compile_into(&mut commands, &mut timeline, &fonts, &text_config);
        drop(commands);

        let mut world = world;
        queue.apply(&mut world);
        let mut query = world.query::<&LocalBounds>();
        assert!(query.iter(&world).any(|bounds| {
            (bounds.0.width() - 80.0).abs() < 1e-6 && (bounds.0.height() - 80.0).abs() < 1e-6
        }));
    }

    #[test]
    fn layout_reflow_animates_displaced_members_and_fades_the_insertion() {
        let mut canvas = Canvas::new(640, 360);
        let first = canvas.rect(80.0, 30.0);
        let second = canvas.rect(80.0, 30.0);
        let container = canvas.group(&[&first]);
        canvas.reflow_layout(
            &container,
            &[&first],
            LayoutKind::Column,
            20.0,
            None,
            None,
            None,
            None,
            None,
            false,
            false,
            "center",
        );
        canvas.set_group_members(&container, &[&first, &second]);
        canvas.reflow_layout(
            &container,
            &[&first, &second],
            LayoutKind::Column,
            20.0,
            Some(0.5),
            Some(&second),
            None,
            None,
            None,
            false,
            false,
            "center",
        );

        let world = World::new();
        let mut queue = CommandQueue::default();
        let mut commands = Commands::new(&mut queue, &world);
        let mut timeline = Timeline::new();
        let fonts = gaanim_text::font::FontRegistry::new();
        let text_config = gaanim_text::prelude::TextConfig::default();
        canvas.compile_into(&mut commands, &mut timeline, &fonts, &text_config);

        assert!(timeline.clips.values().any(|clip| matches!(
            &clip.payload,
            gaanim_timeline::clip::ClipPayload::Animation(gaanim_timeline::clip::AnimationSpec {
                lens: gaanim_timeline::clip::PropertyLensSpec::Translation { .. },
                ..
            })
        )));
        assert!(timeline.clips.values().any(|clip| matches!(
            &clip.payload,
            gaanim_timeline::clip::ClipPayload::Animation(
                gaanim_timeline::clip::AnimationSpec {
                    lens: gaanim_timeline::clip::PropertyLensSpec::Opacity { from, to },
                    ..
                }
            ) if *from == 0.0 && *to == 1.0
        )));
    }
}
