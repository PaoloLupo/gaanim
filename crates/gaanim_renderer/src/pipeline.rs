use crate::background::{BackgroundPaint, ShaderBackgroundRequest};
use crate::background_gpu::ShaderBackgroundFrame;
use crate::effects::{
    BooleanBinding, ClipMask, DropShadow, FillLevelBinding, GaussianBlur, Glow,
    VectorOutlineBinding,
};
use crate::lottie::LottiePlayer;
use crate::stroke::{draw_stroke, view_stroke_transform};
use bevy::prelude::*;
use gaanim_animation::{FillDrawProgress, ReactiveReadout, WriteTipGlow};
use gaanim_core::ObjectId;
use gaanim_core::kurbo::Shape;
use gaanim_core::peniko;
use gaanim_math::GlobalSpatialTransform;
use gaanim_scene::{
    FillBrush, FillDirection, FillLevel, GlobalOpacity, LocalBounds, MobjectId, Path2D, PathSource,
    RasterImage, RenderLayer, RenderOrder, StrokeBrush, Visible, WorldBounds,
};
use std::collections::HashMap;
use std::sync::Arc;

// Explicit imports from bevy_vello instead of glob for clarity
use bevy_vello::integrations::scene::VelloScene2d;

/// Resource: stores the canvas background paint and logical frame bounds.
///
/// Inserted by `Canvas::compile_into` and consumed by the render systems to
/// draw a visible canvas boundary that distinguishes the canvas area from
/// the surrounding window background.
#[derive(Resource, Clone, Debug)]
pub struct CanvasBackground {
    /// Default background paint of the canvas.
    pub paint: BackgroundPaint,
    /// Optional paints selected by the active authored segment.
    pub segment_paints: Vec<SegmentBackgroundPaint>,
    /// Pixel dimensions used to rasterize shader backgrounds.
    pub pixel_size: (u32, u32),
    /// Frame bounds in world coordinates (Y-up, center-origin).
    pub bounds: gaanim_math::Bounds3D,
}

/// Background override and time range for one authored segment.
#[derive(Clone, Debug)]
pub struct SegmentBackgroundPaint {
    pub start_time: f64,
    pub end_time: f64,
    pub paint: Option<BackgroundPaint>,
    /// A terminal stop keeps the outgoing segment active at its shared boundary.
    pub hold_at_end: bool,
}

impl CanvasBackground {
    /// Resolve the segment paint at an exact timeline position.
    pub fn paint_at(&self, time_seconds: f64) -> &BackgroundPaint {
        active_segment(&self.segment_paints, time_seconds, |segment| {
            (segment.start_time, segment.end_time, segment.hold_at_end)
        })
        .and_then(|segment| segment.paint.as_ref())
        .unwrap_or(&self.paint)
    }
}

/// Authored segment active at an exact timeline position. `span` returns a
/// segment's start, end and whether a terminal stop holds it at its end.
pub(crate) fn active_segment<T>(
    segments: &[T],
    time_seconds: f64,
    span: impl Fn(&T) -> (f64, f64, bool),
) -> Option<&T> {
    const EPSILON: f64 = 1e-5;
    segments
        .iter()
        .find(|segment| {
            let (_, end, hold_at_end) = span(segment);
            hold_at_end && (end - time_seconds).abs() <= EPSILON
        })
        .or_else(|| {
            segments.iter().rev().find(|segment| {
                let (start, end, _) = span(segment);
                start <= time_seconds + EPSILON && time_seconds <= end + EPSILON
            })
        })
}

/// Resolve the background brush. With `gpu`, a shader paint draws a texture
/// rendered on the render device and records its request there; otherwise
/// the shader output is copied through the CPU.
fn resolve_canvas_background_brush(
    background: &CanvasBackground,
    rect: kurbo::Rect,
    pixel_size: (u32, u32),
    time_seconds: f64,
    gpu: Option<&mut Option<ShaderBackgroundRequest>>,
) -> (peniko::Brush, Option<kurbo::Affine>) {
    let paint = background.paint_at(time_seconds);
    let resolved = match (paint, gpu) {
        (BackgroundPaint::Shader(shader), Some(gpu)) => shader
            .gpu_request(pixel_size.0, pixel_size.1, time_seconds)
            .map(|request| {
                let brush = peniko::Brush::Image(peniko::ImageBrush::new(request.image().clone()));
                *gpu = Some(request);
                brush
            }),
        _ => paint.resolve_brush(pixel_size.0, pixel_size.1, time_seconds),
    };
    match resolved {
        Ok(brush) => {
            // Shader rows run top to bottom (uv (0, 0) is the top-left
            // corner) while the world is y-up, so row 0 maps to the top edge.
            let brush_transform = paint.is_shader().then(|| {
                kurbo::Affine::translate((rect.x0, rect.y1))
                    * kurbo::Affine::scale_non_uniform(
                        rect.width() / f64::from(pixel_size.0),
                        -rect.height() / f64::from(pixel_size.1),
                    )
            });
            (brush, brush_transform)
        }
        Err(_) => (peniko::Brush::Solid(paint.fallback_color()), None),
    }
}

/// Draw the canvas background into a scene in world coordinates.
fn fill_canvas_background(
    scene: &mut vello::Scene,
    background: &CanvasBackground,
    pixel_size: (u32, u32),
    time_seconds: f64,
    gpu: Option<&mut Option<ShaderBackgroundRequest>>,
) {
    let (rect, transform) = canvas_background_geometry(background);
    let (brush, brush_transform) =
        resolve_canvas_background_brush(background, rect, pixel_size, time_seconds, gpu);
    scene.fill(
        peniko::Fill::NonZero,
        transform,
        &brush,
        brush_transform,
        &rect,
    );
}

/// Keep Bevy's native clear pass aligned with the active segment background.
pub fn sync_canvas_background_clear_system(
    playback_state: Option<Res<gaanim_animation::PlaybackState>>,
    canvas_bg: Option<Res<CanvasBackground>>,
    clear_color: Option<ResMut<ClearColor>>,
) {
    let (Some(canvas_bg), Some(mut clear_color)) = (canvas_bg, clear_color) else {
        return;
    };
    let time_seconds = playback_state
        .as_ref()
        .map_or(0.0, |state| state.current_time);
    let rgba = canvas_bg.paint_at(time_seconds).fallback_color().to_rgba8();
    clear_color.0 = Color::srgba_u8(rgba.r, rgba.g, rgba.b, rgba.a);
}

fn interactive_background_pixel_size(
    background: &CanvasBackground,
    camera: Option<&gaanim_math::ResolvedCamera>,
) -> (u32, u32) {
    const MAX_SHADER_TEXTURE_DIMENSION: f64 = 8192.0;
    let scale = camera
        .map(|camera| camera.viewport.scale)
        .filter(|scale| scale.is_finite() && *scale > 0.0)
        .unwrap_or(1.0);
    let width = f64::from(background.pixel_size.0) * scale;
    let height = f64::from(background.pixel_size.1) * scale;
    let limit_scale = (MAX_SHADER_TEXTURE_DIMENSION / width.max(height)).min(1.0);
    let fit = |dimension: f64| (dimension * limit_scale).round().clamp(1.0, 8192.0) as u32;
    (fit(width), fit(height))
}

/// Marker component identifying the single global Vello compositing entity.
#[derive(Component, Debug, Clone, Copy, Default)]
pub struct MainVelloScene;

/// Full-target color clear performed before the fitted PBR viewport.
///
/// A camera only clears inside its viewport. Keeping this pass separate avoids
/// stale pixels around a fitted canvas and lets presentation hosts choose a
/// neutral letterbox color without changing the authored scene background.
#[doc(hidden)]
#[derive(Component, Debug, Clone, Copy, Default)]
pub struct GaanimFullWindowClearCamera;

/// The primary PBR camera owned by the shared Gaanim scene runtime.
#[doc(hidden)]
#[derive(Component, Debug, Clone, Copy, Default)]
pub struct GaanimPbrCamera;

/// Retained GPU cache of precompiled local Vello scenes for each Mobject.
///
/// Under the "Fragment Retain" system, local shapes, fills, and strokes are compiled into
/// standalone scene fragments and cached. They are only invalidated and rebuilt when
/// the Mobject's visual components (geometry or brushes) change.
#[derive(Resource, Default)]
pub struct GaanimRenderCache {
    pub fragment_cache: HashMap<ObjectId, Arc<vello::Scene>>,
    stroke_views: HashMap<ObjectId, kurbo::Affine>,
    fragment_inputs: HashMap<ObjectId, FragmentInputs>,
}

/// Values of the timeline-driven components a retained fragment was built from.
///
/// Every playback frame is an exact seek: it restores the t=0 keyframe and
/// replays each earlier clip, so these components are rewritten (and flagged as
/// changed by Bevy) even when their final value is identical to the previous
/// frame. Comparing the values keeps completed and not-yet-started objects
/// retained instead of re-encoding the whole scene on every frame.
#[derive(Clone)]
struct FragmentInputs {
    path: Option<Arc<kurbo::BezPath>>,
    path_source: Option<Arc<kurbo::BezPath>>,
    fill_level: Option<FillLevel>,
    fill: Option<FillBrush>,
    stroke: Option<StrokeBrush>,
    fill_progress: Option<FillDrawProgress>,
    tip_glow: Option<WriteTipGlow>,
}

impl FragmentInputs {
    fn capture(
        path: Option<&Path2D>,
        path_source: Option<&PathSource>,
        fill_level: Option<&FillLevel>,
        fill: Option<&FillBrush>,
        stroke: Option<&StrokeBrush>,
        fill_progress: Option<&FillDrawProgress>,
        tip_glow: Option<&WriteTipGlow>,
    ) -> Self {
        Self {
            path: path.map(|path| Arc::clone(&path.0)),
            path_source: path_source.map(|source| Arc::clone(&source.0)),
            fill_level: fill_level.copied(),
            fill: fill.cloned(),
            stroke: stroke.cloned(),
            fill_progress: fill_progress.copied(),
            tip_glow: tip_glow.cloned(),
        }
    }
}

fn same_path(left: &Option<Arc<kurbo::BezPath>>, right: &Option<Arc<kurbo::BezPath>>) -> bool {
    match (left, right) {
        (Some(left), Some(right)) => Arc::ptr_eq(left, right) || left == right,
        (None, None) => true,
        _ => false,
    }
}

impl PartialEq for FragmentInputs {
    fn eq(&self, other: &Self) -> bool {
        same_path(&self.path, &other.path)
            && same_path(&self.path_source, &other.path_source)
            && self.fill_level == other.fill_level
            && self.fill == other.fill
            && self.stroke == other.stroke
            && self.fill_progress == other.fill_progress
            && self.tip_glow == other.tip_glow
    }
}

pub struct ExtractedElement {
    transform: kurbo::Affine,
    opacity: f32,
    opacity_bounds: kurbo::Rect,
    opacity_group: Entity,
    render_order: RenderOrder,
    scene: Arc<vello::Scene>,
    clip_mask: Option<ClipMask>,
    /// Side of the active scene transition this element belongs to.
    transition_side: gaanim_scene::TransitionSide,
}

/// Stack order of a drawn element: its own `z_index` plus every ancestor's,
/// so a group's or text's `z_index` moves its whole subtree while leaves
/// inside unlayered groups keep their authored order. `creation_order`
/// still breaks ties.
fn stacked_render_order(
    own: RenderOrder,
    entity: Entity,
    mut parent_of: impl FnMut(Entity) -> Option<Entity>,
    mut z_index_of: impl FnMut(Entity) -> i32,
) -> RenderOrder {
    let mut z_index = own.z_index;
    let mut current = entity;
    while let Some(parent) = parent_of(current) {
        z_index = z_index.saturating_add(z_index_of(parent));
        current = parent;
    }
    RenderOrder { z_index, ..own }
}

fn opacity_layer_bounds(
    world_bounds: Option<&WorldBounds>,
    fallback: kurbo::Rect,
    shadow: Option<&DropShadow>,
    glow: Option<&Glow>,
    blur: Option<&GaussianBlur>,
) -> kurbo::Rect {
    let mut rect = world_bounds
        .map(|bounds| {
            kurbo::Rect::new(
                bounds.0.min.x,
                bounds.0.min.y,
                bounds.0.max.x,
                bounds.0.max.y,
            )
        })
        .filter(|rect| rect.width().is_finite() && rect.height().is_finite())
        .unwrap_or(fallback);

    let blur_padding = blur.map_or(0.0, |blur| blur.sigma.max(0.0) * 3.0);
    let glow_padding = glow.map_or(0.0, |glow| glow.radius.max(0.0));
    let shadow_padding = shadow.map_or(0.0, |shadow| {
        shadow.blur_radius.max(0.0) * 3.0 + shadow.offset.abs().max_element()
    });
    let padding = blur_padding.max(glow_padding).max(shadow_padding) + 1.0;
    rect = rect.inflate(padding, padding);
    rect
}

fn opacity_run_end(elements: &[ExtractedElement], start: usize) -> usize {
    let opacity = elements[start].opacity.to_bits();
    let mut end = start + 1;
    while let Some(element) = elements.get(end) {
        if element.clip_mask.is_some()
            || element.opacity_group != elements[start].opacity_group
            || !element.opacity.is_finite()
            || element.opacity <= 0.0
            || element.opacity >= 1.0
            || element.opacity.to_bits() != opacity
        {
            break;
        }
        end += 1;
    }
    end
}

/// End of the run of elements clipped by the same non-inverted mask sources.
/// Such a mask has the same world outline for every element, so the run can
/// share one clip layer instead of one per element.
fn shared_clip_run_end(elements: &[ExtractedElement], start: usize) -> usize {
    let Some(clip) = elements[start]
        .clip_mask
        .as_ref()
        .filter(|clip| !clip.invert && !clip.sources.is_empty())
    else {
        return start + 1;
    };
    let mut end = start + 1;
    while let Some(element) = elements.get(end) {
        let shares = element.clip_mask.as_ref().is_some_and(|other| {
            !other.invert && other.rule == clip.rule && other.sources == clip.sources
        });
        if !shares {
            break;
        }
        end += 1;
    }
    end
}

/// Composite elements, masking each run of transition scene members with its
/// side's reveal, then draw the transition overlays above everything.
fn append_extracted_elements(
    main_scene: &mut vello::Scene,
    elements: &[ExtractedElement],
    composition_bounds: kurbo::Rect,
    transition: Option<&gaanim_scene::SceneTransitionFrame>,
) {
    let mut index = 0;
    while index < elements.len() {
        let side = elements[index].transition_side;
        let mut end = index + 1;
        while elements
            .get(end)
            .is_some_and(|element| element.transition_side == side)
        {
            end += 1;
        }
        let mask = transition.and_then(|frame| frame.mask_for(side));
        if let Some(mask) = mask {
            push_transition_mask(main_scene, mask);
        }
        append_element_run(main_scene, &elements[index..end], composition_bounds);
        if let Some(mask) = mask {
            pop_transition_mask(main_scene, mask);
        }
        index = end;
    }
    if let Some(frame) = transition {
        for layer in &frame.overlays {
            main_scene.push_layer(
                peniko::Fill::NonZero,
                layer.blend,
                1.0,
                kurbo::Affine::IDENTITY,
                &layer.clip,
            );
            for (path, brush) in &layer.fills {
                main_scene.fill(
                    peniko::Fill::NonZero,
                    kurbo::Affine::IDENTITY,
                    brush,
                    None,
                    path,
                );
            }
            main_scene.pop_layer();
        }
    }
}

/// Open a world-space layer that keeps only a transition side's visible region.
///
/// Always an isolated `push_layer`, never `push_clip_layer`: the masked run
/// contains fragments with their own blend layers (closed strokes, e.g. text
/// glyphs, opacity groups, `ClipMask`s), and vello does not yet clip nested
/// blend layers inside a clip-only layer (linebender/vello#1198), so those
/// elements escaped the reveal.
fn push_transition_mask(scene: &mut vello::Scene, mask: &gaanim_scene::TransitionMask) {
    scene.push_layer(
        mask.rule,
        peniko::BlendMode::default(),
        1.0,
        kurbo::Affine::IDENTITY,
        &mask.path,
    );
}

/// Close a transition mask. A feathered mask first multiplies the layer by
/// its linear alpha ramp (a vector `DestIn` composite, no textures).
fn pop_transition_mask(scene: &mut vello::Scene, mask: &gaanim_scene::TransitionMask) {
    if let Some((opaque, clear)) = mask.fade {
        scene.push_layer(
            mask.rule,
            peniko::BlendMode::new(peniko::Mix::Normal, peniko::Compose::DestIn),
            1.0,
            kurbo::Affine::IDENTITY,
            &mask.path,
        );
        let ramp = peniko::Gradient::new_linear(opaque, clear)
            .with_stops([peniko::Color::BLACK, peniko::Color::TRANSPARENT]);
        scene.fill(
            mask.rule,
            kurbo::Affine::IDENTITY,
            &peniko::Brush::Gradient(ramp),
            None,
            &mask.path,
        );
        scene.pop_layer();
    }
    scene.pop_layer();
}

/// Timeline position whose segment background fills the whole frame. While a
/// transition splits the frame this is the outgoing segment.
fn transition_background_time(
    transition: Option<&gaanim_scene::SceneTransitionFrame>,
    time_seconds: f64,
) -> f64 {
    transition
        .and_then(|frame| frame.backgrounds)
        .map_or(time_seconds, |(outgoing, _)| outgoing)
}

/// Draw the incoming segment's background inside its reveal when it differs
/// from the outgoing one.
fn fill_transition_background(
    scene: &mut vello::Scene,
    background: &CanvasBackground,
    pixel_size: (u32, u32),
    transition: Option<&gaanim_scene::SceneTransitionFrame>,
) {
    let Some(frame) = transition else {
        return;
    };
    let (Some((outgoing, incoming)), Some(mask)) = (frame.backgrounds, &frame.incoming_mask) else {
        return;
    };
    if std::ptr::eq(background.paint_at(outgoing), background.paint_at(incoming)) {
        return;
    }
    push_transition_mask(scene, mask);
    fill_canvas_background(scene, background, pixel_size, incoming, None);
    pop_transition_mask(scene, mask);
}

fn append_element_run(
    main_scene: &mut vello::Scene,
    elements: &[ExtractedElement],
    composition_bounds: kurbo::Rect,
) {
    let mut index = 0;
    while let Some(elem) = elements.get(index) {
        if !elem.opacity.is_finite() || elem.opacity <= 0.0 {
            index += 1;
            continue;
        }

        if elem.clip_mask.is_none() && elem.opacity < 1.0 {
            let end = opacity_run_end(elements, index);
            main_scene.push_layer(
                peniko::Fill::NonZero,
                peniko::BlendMode::default(),
                elem.opacity.clamp(0.0, 1.0),
                kurbo::Affine::IDENTITY,
                &composition_bounds,
            );
            for grouped in &elements[index..end] {
                main_scene.append(&grouped.scene, Some(grouped.transform));
            }
            main_scene.pop_layer();
            index = end;
            continue;
        }

        let end = shared_clip_run_end(elements, index);
        if let Some(clip) = &elem.clip_mask {
            main_scene.push_layer(
                clip.rule,
                peniko::BlendMode::default(),
                1.0,
                elem.transform,
                &clip.path,
            );
        }
        for clipped in &elements[index..end] {
            if !clipped.opacity.is_finite() || clipped.opacity <= 0.0 {
                continue;
            }
            if clipped.opacity < 1.0 {
                main_scene.push_layer(
                    peniko::Fill::NonZero,
                    peniko::BlendMode::default(),
                    clipped.opacity.clamp(0.0, 1.0),
                    kurbo::Affine::IDENTITY,
                    &clipped.opacity_bounds,
                );
            }
            main_scene.append(&clipped.scene, Some(clipped.transform));
            if clipped.opacity < 1.0 {
                main_scene.pop_layer();
            }
        }
        if elem.clip_mask.is_some() {
            main_scene.pop_layer();
        }
        index = end;
    }
}

fn canvas_background_geometry(background: &CanvasBackground) -> (kurbo::Rect, kurbo::Affine) {
    let bounds = &background.bounds;
    (
        kurbo::Rect::new(bounds.min.x, bounds.min.y, bounds.max.x, bounds.max.y),
        kurbo::Affine::IDENTITY,
    )
}

fn stroke_clip_path<'a>(
    visible_path: &'a kurbo::BezPath,
    source_path: Option<&'a kurbo::BezPath>,
) -> Option<&'a kurbo::BezPath> {
    let clip_path = source_path.unwrap_or(visible_path);
    clip_path
        .elements()
        .contains(&kurbo::PathEl::ClosePath)
        .then_some(clip_path)
}

fn path_reveal_is_empty(tip: Option<&WriteTipGlow>) -> bool {
    tip.is_some_and(|tip| tip.completion <= f64::EPSILON)
}

fn opacity_is_empty(opacity: &GlobalOpacity) -> bool {
    opacity.0 <= f32::EPSILON
}

/// Resolve source paths for vector clipping after global transforms are known.
///
/// `ClipMask` stores source leaf entities rather than a compiler-time snapshot;
/// this deliberately keeps clipping correct when either the target or its mask
/// is translated, scaled, rotated, or morphed by the timeline.
pub fn resolve_dynamic_clip_masks_system(
    mut masks: Query<(&GlobalSpatialTransform, &LocalBounds, &mut ClipMask)>,
    sources: Query<(&Path2D, &GlobalSpatialTransform)>,
) {
    for (target_transform, target_bounds, mut mask) in &mut masks {
        if mask.sources.is_empty() {
            continue;
        }
        let mut world = kurbo::BezPath::new();
        for source in &mask.sources {
            let Ok((path, transform)) = sources.get(*source) else {
                continue;
            };
            let mut path = (*path.0).clone();
            path.apply_affine(transform.affine_2d);
            world.extend(path);
        }
        let mut local = world;
        local.apply_affine(target_transform.affine_2d.inverse());
        if mask.invert {
            // The renderer's layer clips to filled geometry. A stable enclosing
            // rectangle plus even-odd filling is the vector equivalent of an
            // inverse mask without requiring raster alpha composition.
            let b = target_bounds.0;
            let margin = b.width().max(b.height()).max(1.0) * 2.0;
            let mut inverse = kurbo::Rect::new(
                b.min.x - margin,
                b.min.y - margin,
                b.max.x + margin,
                b.max.y + margin,
            )
            .to_path(0.1);
            inverse.extend(local);
            local = inverse;
            mask.rule = peniko::Fill::EvenOdd;
        }
        if mask.path != local {
            mask.path = local;
        }
    }
}

/// Rebuild live booleans from their source paths after propagation. The output
/// remains a normal drawable, so renderer caching and bounds work unchanged.
pub fn resolve_dynamic_boolean_system(
    mut queries: ParamSet<(
        Query<(Entity, &BooleanBinding, &GlobalSpatialTransform)>,
        Query<(&Path2D, &GlobalSpatialTransform)>,
        Query<(&mut Path2D, &mut PathSource, &mut LocalBounds), With<BooleanBinding>>,
    )>,
) {
    let jobs = queries
        .p0()
        .iter()
        .map(|(entity, binding, transform)| (entity, binding.clone(), *transform))
        .collect::<Vec<_>>();
    let mut resolved_jobs = Vec::with_capacity(jobs.len());
    {
        let sources = queries.p1();
        for (entity, binding, target_transform) in jobs {
            let mut operands = binding.sources.iter().filter_map(|entity| {
                sources.get(*entity).ok().map(|(path, transform)| {
                    let mut world = (*path.0).clone();
                    world.apply_affine(transform.affine_2d);
                    world
                })
            });
            let mut output = operands.next().unwrap_or_default();
            for operand in operands {
                let resolved = gaanim_objects::boolean::apply_with_options(
                    &output,
                    &operand,
                    binding.op,
                    binding.tolerance,
                    binding.rule,
                );
                output =
                    resolved
                        .paths
                        .into_iter()
                        .fold(kurbo::BezPath::new(), |mut joined, part| {
                            joined.extend(part);
                            joined
                        });
            }
            output.apply_affine(target_transform.affine_2d.inverse());
            resolved_jobs.push((entity, output));
        }
    }

    let mut results = queries.p2();
    for (entity, output) in resolved_jobs {
        let Ok((mut path, mut source_path, mut bounds)) = results.get_mut(entity) else {
            continue;
        };
        if *path.0 != output {
            let rect = output.bounding_box();
            *bounds = LocalBounds(gaanim_math::Bounds3D::new_2d(
                rect.x0, rect.y0, rect.x1, rect.y1,
            ));
            let output = Arc::new(output);
            *path = Path2D(output.clone());
            *source_path = PathSource(output);
        }
    }
}

pub fn resolve_fill_level_system(
    mut queries: ParamSet<(
        Query<(
            Entity,
            &FillLevelBinding,
            &FillLevel,
            &GlobalSpatialTransform,
        )>,
        Query<(&Path2D, &GlobalSpatialTransform)>,
        Query<(&mut Path2D, &mut PathSource, &mut LocalBounds), With<FillLevelBinding>>,
    )>,
) {
    let jobs = queries
        .p0()
        .iter()
        .map(|(entity, binding, level, transform)| (entity, binding.clone(), *level, *transform))
        .collect::<Vec<_>>();
    let mut resolved_jobs = Vec::with_capacity(jobs.len());
    {
        let sources = queries.p1();
        for (entity, binding, level, target_transform) in jobs {
            let mut mask = kurbo::BezPath::new();
            for source_entity in &binding.sources {
                if let Ok((source, transform)) = sources.get(*source_entity) {
                    let mut world = (*source.0).clone();
                    world.apply_affine(transform.affine_2d);
                    mask.extend(world);
                }
            }
            let bbox = mask.bounding_box();
            let level = level.0.clamp(0.0, 1.0);
            let band = match binding.direction {
                FillDirection::Up => {
                    kurbo::Rect::new(bbox.x0, bbox.y0, bbox.x1, bbox.y0 + bbox.height() * level)
                }
                FillDirection::Down => {
                    kurbo::Rect::new(bbox.x0, bbox.y1 - bbox.height() * level, bbox.x1, bbox.y1)
                }
                FillDirection::Left => {
                    kurbo::Rect::new(bbox.x0, bbox.y0, bbox.x0 + bbox.width() * level, bbox.y1)
                }
                FillDirection::Right => {
                    kurbo::Rect::new(bbox.x1 - bbox.width() * level, bbox.y0, bbox.x1, bbox.y1)
                }
            }
            .to_path(0.1);
            let resolved = gaanim_objects::boolean::apply_with_options(
                &mask,
                &band,
                gaanim_objects::boolean::BooleanOp::Intersection,
                0.25,
                gaanim_objects::boolean::BooleanFillRule::NonZero,
            );
            let mut output =
                resolved
                    .paths
                    .into_iter()
                    .fold(kurbo::BezPath::new(), |mut out, part| {
                        out.extend(part);
                        out
                    });
            output.apply_affine(target_transform.affine_2d.inverse());
            resolved_jobs.push((entity, output));
        }
    }

    let mut results = queries.p2();
    for (entity, output) in resolved_jobs {
        let Ok((mut path, mut source_path, mut bounds)) = results.get_mut(entity) else {
            continue;
        };
        if *path.0 != output {
            let rect = output.bounding_box();
            *bounds = LocalBounds(gaanim_math::Bounds3D::new_2d(
                rect.x0, rect.y0, rect.x1, rect.y1,
            ));
            let output = Arc::new(output);
            *path = Path2D(output.clone());
            *source_path = PathSource(output);
        }
    }
}

pub fn resolve_vector_outline_system(
    mut queries: ParamSet<(
        Query<(Entity, &VectorOutlineBinding, &GlobalSpatialTransform)>,
        Query<(&Path2D, &GlobalSpatialTransform)>,
        Query<(&mut Path2D, &mut PathSource, &mut LocalBounds), With<VectorOutlineBinding>>,
    )>,
) {
    let jobs = queries
        .p0()
        .iter()
        .map(|(entity, binding, transform)| (entity, binding.clone(), *transform))
        .collect::<Vec<_>>();
    let mut resolved_jobs = Vec::with_capacity(jobs.len());
    {
        let sources = queries.p1();
        for (entity, binding, transform) in jobs {
            let mut output = kurbo::BezPath::new();
            for source_entity in &binding.sources {
                if let Ok((source, source_transform)) = sources.get(*source_entity) {
                    let mut world = (*source.0).clone();
                    world.apply_affine(source_transform.affine_2d);
                    output.extend(world);
                }
            }
            output.apply_affine(transform.affine_2d.inverse());
            resolved_jobs.push((entity, output));
        }
    }

    let mut outlines = queries.p2();
    for (entity, output) in resolved_jobs {
        let Ok((mut path, mut source_path, mut bounds)) = outlines.get_mut(entity) else {
            continue;
        };
        if *path.0 != output {
            let rect = output.bounding_box();
            *bounds = LocalBounds(gaanim_math::Bounds3D::new_2d(
                rect.x0, rect.y0, rect.x1, rect.y1,
            ));
            let output = Arc::new(output);
            *path = Path2D(output.clone());
            *source_path = PathSource(output);
        }
    }
}
const BLUR_KERNEL: [((f64, f64), f32); 13] = [
    ((0.0, 0.0), 0.20),
    ((0.65, 0.0), 0.10),
    ((-0.65, 0.0), 0.10),
    ((0.0, 0.65), 0.10),
    ((0.0, -0.65), 0.10),
    ((0.65, 0.65), 0.06),
    ((0.65, -0.65), 0.06),
    ((-0.65, 0.65), 0.06),
    ((-0.65, -0.65), 0.06),
    ((1.4, 0.0), 0.04),
    ((-1.4, 0.0), 0.04),
    ((0.0, 1.4), 0.04),
    ((0.0, -1.4), 0.04),
];

fn draw_soft_fill(
    scene: &mut vello::Scene,
    path: &kurbo::BezPath,
    brush: &peniko::Brush,
    sigma: f64,
    alpha: f32,
    origin: kurbo::Affine,
) {
    for ((x, y), weight) in BLUR_KERNEL {
        let sample_brush = brush.clone().multiply_alpha(weight * alpha);
        let transform = origin * kurbo::Affine::translate((x * sigma, y * sigma));
        scene.fill(peniko::Fill::NonZero, transform, &sample_brush, None, path);
    }
}

fn draw_soft_stroke(
    scene: &mut vello::Scene,
    path: &kurbo::BezPath,
    brush: &peniko::Brush,
    style: &kurbo::Stroke,
    sigma: f64,
    view: Option<kurbo::Affine>,
) {
    for ((x, y), weight) in BLUR_KERNEL {
        let sample_brush = brush.clone().multiply_alpha(weight);
        draw_stroke(
            scene,
            style,
            kurbo::Affine::translate((x * sigma, y * sigma)),
            &sample_brush,
            view,
            path,
        );
    }
}

fn draw_glow(
    scene: &mut vello::Scene,
    path: &kurbo::BezPath,
    glow: &Glow,
    view: Option<kurbo::Affine>,
) {
    if !glow.radius.is_finite()
        || glow.radius <= 0.0
        || !glow.intensity.is_finite()
        || glow.intensity <= 0.0
    {
        return;
    }
    let brush = peniko::Brush::Solid(glow.color);
    // Concentric strokes approximate the blur. Their count follows the
    // radius (one ring per 0.015 scene units, about 2 px at the default
    // 120 px per unit) so large glows stay smooth, and each ring's alpha is
    // the 7-ring reference alpha spread over the rings covering the same
    // distance, keeping the total opacity unchanged.
    let scale = view.map_or(1.0, |view| view.determinant().abs().sqrt());
    let steps = ((glow.radius * scale / 0.015).ceil() as u32).clamp(7, 96);
    let share = 7.0 / steps as f32;
    for step in (1..=steps).rev() {
        let fraction = step as f32 / steps as f32;
        let spread = glow.radius * f64::from(fraction);
        let falloff = 1.0 - (fraction - 1.0 / steps as f32);
        let reference = (glow.intensity * 0.18 * falloff * falloff).clamp(0.0, 1.0);
        let alpha = 1.0 - (1.0 - reference).powf(share);
        let sample = brush.clone().multiply_alpha(alpha);
        draw_stroke(
            scene,
            &kurbo::Stroke::new(spread * 2.0),
            kurbo::Affine::IDENTITY,
            &sample,
            view,
            path,
        );
    }
}

/// System: Synchronizes the `gaanim_math::Camera` resource to the active Bevy `Camera2d`.
///
/// This ensures that zoom, pan, and rotation configured on the gaanim camera are reflected
/// in the actual rendered output, since `bevy_vello` relies on the Bevy camera for projection.
pub fn sync_gaanim_camera_to_bevy_system(
    gaanim_camera: Option<Res<gaanim_math::ResolvedCamera>>,
    mut bevy_cameras: Query<(&mut Transform, &mut Projection), With<Camera2d>>,
) {
    let Some(cam) = gaanim_camera else { return };
    for (mut transform, mut projection) in &mut bevy_cameras {
        // Position — incorporate viewport_offset_y so the scene shifts up
        // when the timeline panel is visible (offset is in screen pixels).
        // In perspective mode the Vello (2D) camera must stay fixed at the
        // origin so that Vello world coordinates map to screen pixels via a
        // stable orthographic transform. 3D billboard labels are then
        // projected to screen via `world_to_screen` and placed in this fixed
        // Vello space. Following the 3D eye with the 2D camera would shift
        // HUD and 2D background incorrectly and break the projection.
        let is_perspective = matches!(cam.projection, gaanim_math::Projection::Perspective { .. });
        let effective_zoom = match cam.projection {
            gaanim_math::Projection::Orthographic { zoom } => {
                cam.pixels_per_unit() * zoom * cam.viewport.scale
            }
            _ => cam.pixels_per_unit() * cam.viewport.scale,
        };
        let offset_world_y = if effective_zoom > 0.0 {
            cam.viewport.offset_y / effective_zoom
        } else {
            0.0
        };
        if is_perspective {
            transform.translation.x = 0.0;
            transform.translation.y = (-offset_world_y) as f32;
            transform.rotation = Quat::IDENTITY;
            if let Projection::Orthographic(ortho) = projection.as_mut() {
                let effective = cam.pixels_per_unit() * cam.viewport.scale;
                let scale = if effective > 0.0 {
                    1.0 / effective as f32
                } else {
                    1.0
                };
                ortho.scale = scale;
            }
        } else {
            transform.translation.x = cam.position.x as f32;
            transform.translation.y = (cam.position.y - offset_world_y) as f32;
            // Rotation (2D Z-axis only) — compute directly from quaternion
            // to avoid the three-trig overhead of full Euler decomposition.
            let z_angle = 2.0 * f64::atan2(cam.rotation.z, cam.rotation.w);
            transform.rotation = Quat::from_rotation_z(-z_angle as f32);

            // Projection: only Orthographic is supported for 2D Vello
            if let gaanim_math::Projection::Orthographic { zoom } = cam.projection {
                // Apply viewport_scale so the scene maintains its aspect ratio
                // when the window dimensions differ from the scene's native resolution.
                let effective = cam.pixels_per_unit() * zoom * cam.viewport.scale;
                let scale = if effective > 0.0 {
                    1.0 / effective as f32
                } else {
                    1.0
                };
                if let Projection::Orthographic(ortho) = projection.as_mut() {
                    ortho.scale = scale;
                }
            }
        }
    }
}

/// System: Synchronizes the `gaanim_math::Camera` resource to any Bevy `Camera3d` (perspective).
///
/// Used for the hybrid 2D/3D pipeline where 3D meshes are rendered with Bevy's PBR
/// while Vello continues to handle 2D vector content. When the camera is orthographic
/// the 3D camera is still updated with the same position/rotation for consistency.
pub(crate) fn fitted_canvas_viewport(
    cam: &gaanim_math::Camera,
    viewport: gaanim_math::CameraViewport,
    window: &Window,
) -> Option<bevy::camera::Viewport> {
    let fit = viewport.scale;
    if !fit.is_finite() || fit <= 0.0 || cam.viewport_width == 0 || cam.viewport_height == 0 {
        return None;
    }

    let window_width = window.width() as f64;
    let window_height = window.height() as f64;
    if window_width <= 0.0 || window_height <= 0.0 {
        return None;
    }

    let viewport_width = (cam.viewport_width as f64 * fit).min(window_width);
    let viewport_height = (cam.viewport_height as f64 * fit).min(window_height);
    let left = ((window_width - viewport_width) * 0.5).max(0.0);
    let center_y = window_height * 0.5 + viewport.offset_y;
    let top =
        (center_y - viewport_height * 0.5).clamp(0.0, (window_height - viewport_height).max(0.0));
    let scale_factor = window.scale_factor() as f64;

    Some(bevy::camera::Viewport {
        physical_position: UVec2::new(
            (left * scale_factor).round() as u32,
            (top * scale_factor).round() as u32,
        ),
        physical_size: UVec2::new(
            (viewport_width * scale_factor).round().max(1.0) as u32,
            (viewport_height * scale_factor).round().max(1.0) as u32,
        ),
        depth: 0.0..1.0,
    })
}

pub fn sync_gaanim_camera_to_bevy_3d_system(
    gaanim_camera: Option<Res<gaanim_math::ResolvedCamera>>,
    authored_camera: Option<Res<gaanim_math::Camera>>,
    rig_camera: Option<Res<gaanim_math::CameraRigCamera>>,
    mut bevy_cameras: Query<
        (
            &mut Camera,
            &bevy::camera::RenderTarget,
            &mut Transform,
            &mut Projection,
            Option<&gaanim_scene::AuthoritativeCameraView>,
        ),
        With<Camera3d>,
    >,
    windows: Query<&Window>,
    primary_window: Query<Entity, With<bevy::window::PrimaryWindow>>,
) {
    let Some(resolved_camera) = gaanim_camera else {
        return;
    };
    let primary_window = primary_window.single().ok();
    for (mut bevy_camera, render_target, mut transform, mut projection, authoritative_view) in
        &mut bevy_cameras
    {
        let cam: &gaanim_math::Camera = if authoritative_view.is_some() {
            rig_camera
                .as_deref()
                .map(|rig| &rig.0)
                .or(authored_camera.as_deref())
                .unwrap_or(&resolved_camera.camera)
        } else {
            &resolved_camera.camera
        };
        transform.translation = Vec3::new(
            cam.position.x as f32,
            cam.position.y as f32,
            cam.position.z as f32,
        );
        // Convert DQuat -> Quat
        transform.rotation = Quat::from_xyzw(
            cam.rotation.x as f32,
            cam.rotation.y as f32,
            cam.rotation.z as f32,
            cam.rotation.w as f32,
        );
        match cam.projection {
            gaanim_math::Projection::Perspective { fov_y, near, far } => {
                if !matches!(projection.as_ref(), Projection::Perspective(_)) {
                    *projection = Projection::Perspective(Default::default());
                }
                let target_window = match render_target {
                    bevy::camera::RenderTarget::Window(bevy::window::WindowRef::Primary) => {
                        primary_window
                    }
                    bevy::camera::RenderTarget::Window(bevy::window::WindowRef::Entity(entity)) => {
                        Some(*entity)
                    }
                    _ => None,
                };
                if let Some(window) = target_window.and_then(|entity| windows.get(entity).ok()) {
                    bevy_camera.viewport =
                        fitted_canvas_viewport(cam, resolved_camera.viewport, window);
                }
                if let Projection::Perspective(persp) = projection.as_mut() {
                    persp.fov = fov_y as f32;
                    persp.near = near as f32;
                    persp.far = far as f32;
                    // Aspect is derived from viewport dimensions automatically by Bevy,
                    // but we set it explicitly to keep headless/export in sync.
                    persp.aspect_ratio = if cam.viewport_height > 0 {
                        cam.viewport_width as f32 / cam.viewport_height as f32
                    } else {
                        1.0
                    };
                }
            }
            gaanim_math::Projection::Orthographic { zoom } => {
                // Camera3d starts with a perspective projection. Assign the
                // orthographic variant explicitly when the authored rig resets
                // to 2D; merely updating an existing variant leaves the old
                // perspective frustum active and can magnify nearby meshes into
                // edge-wide colour bands.
                if !matches!(projection.as_ref(), Projection::Orthographic(_)) {
                    *projection = Projection::Orthographic(
                        bevy::camera::OrthographicProjection::default_3d(),
                    );
                }
                // Perspective uses a fitted canvas viewport. Orthographic 2D
                // rendering returns to the complete target, so retaining that
                // crop would expose stale pixels around the canvas.
                bevy_camera.viewport = None;
                if let Projection::Orthographic(ortho) = projection.as_mut() {
                    let effective =
                        resolved_camera.pixels_per_unit() * zoom * resolved_camera.viewport.scale;
                    ortho.scale = if effective > 0.0 {
                        1.0 / effective as f32
                    } else {
                        1.0
                    };
                }
            }
        }
    }
}

/// System: Cleans up stale fragment cache entries when Mobject entities are destroyed.
///
/// Run this system before `gaanim_render_system` so the cache stays consistent.
pub fn gaanim_render_cache_sweep_system(
    mut cache: ResMut<GaanimRenderCache>,
    mut removed: RemovedComponents<MobjectId>,
    query_mobj_ids: Query<&MobjectId>,
) {
    // Only sweep when entities with MobjectId were actually removed.
    // On static frames this is a no-op (zero cost).
    if removed.read().next().is_none() {
        return;
    }
    let active: std::collections::HashSet<ObjectId> = query_mobj_ids.iter().map(|m| m.0).collect();
    cache.fragment_cache.retain(|id, _| active.contains(id));
    cache.stroke_views.retain(|id, _| active.contains(id));
    cache.fragment_inputs.retain(|id, _| active.contains(id));
}

/// Standalone function: extracts all visible Vello2D mobjects from a Bevy World
/// and compiles them into a single composited `vello::Scene`.
///
/// This bypasses Bevy's render graph and does NOT require `VelloScene2d`, `Commands`,
/// or any window/render plugins. It rebuilds all fragments every call (no caching),
/// making it suitable for headless export where every frame is different.
///
/// # Coordinate System
/// The returned `vello::Scene` is in **gaanim world space** (Y-up, origin at
/// centre).  When rendering directly with `vello::Renderer::render_to_texture`,
/// apply a camera transform that maps world coordinates to the output viewport.
/// The export path in `gaanim_export` already handles this correctly.
pub fn compile_scene_from_world(
    world: &mut World,
    camera: Option<&gaanim_math::Camera>,
) -> vello::Scene {
    let background_time = world
        .get_resource::<gaanim_animation::PlaybackState>()
        .map_or(0.0, |state| state.current_time);
    let mut extracted = Vec::new();
    let mut culled_entities = std::collections::HashSet::new();
    let transition_frame = world
        .get_resource::<gaanim_scene::SceneTransitionFrame>()
        .filter(|frame| !frame.is_empty())
        .cloned();
    let opacity_fallback = world
        .get_resource::<CanvasBackground>()
        .map(|background| {
            kurbo::Rect::new(
                background.bounds.min.x,
                background.bounds.min.y,
                background.bounds.max.x,
                background.bounds.max.y,
            )
        })
        .unwrap_or_else(|| kurbo::Rect::new(-4096.0, -4096.0, 4096.0, 4096.0));

    let cam_bounds = camera.and_then(|cam| {
        if let gaanim_math::Projection::Orthographic { zoom } = cam.projection {
            let effective = zoom;
            let hw = cam.frame_width / (2.0 * effective);
            let hh = cam.frame_height / (2.0 * effective);
            let margin = cam.frame_width.max(cam.frame_height) * 0.08 / effective.max(0.1);
            Some(gaanim_math::Bounds3D::new_2d(
                cam.position.x - hw - margin,
                cam.position.y - hh - margin,
                cam.position.x + hw + margin,
                cam.position.y + hh + margin,
            ))
        } else {
            None
        }
    });

    let mut query_mobjects = world.query_filtered::<(
        Entity,
        &MobjectId,
        &GlobalSpatialTransform,
        &GlobalOpacity,
        &RenderOrder,
        &RenderLayer,
        Option<&Path2D>,
        Option<&PathSource>,
        Option<&FillBrush>,
        Option<&StrokeBrush>,
        Option<&RasterImage>,
        Option<&LottiePlayer>,
        Option<&gaanim_scene::ObjectTag>,
    ), With<Visible>>();

    let mut query_effects = world.query::<(
        Option<&DropShadow>,
        Option<&Glow>,
        Option<&GaussianBlur>,
        Option<&FillDrawProgress>,
        Option<&ClipMask>,
        Option<&WorldBounds>,
        Option<&gaanim_scene::GroupMarker>,
        Option<&WriteTipGlow>,
    )>();

    let mut child_query = world.query::<&ChildOf>();
    let mut order_query = world.query::<&RenderOrder>();

    for (
        entity,
        _mobj_id,
        transform,
        global_opacity,
        render_order,
        render_layer,
        path_opt,
        path_source_opt,
        fill_opt,
        stroke_opt,
        raster_image_opt,
        lottie_opt,
        _tag,
    ) in query_mobjects.iter(world)
    {
        if *render_layer != RenderLayer::Vello2D {
            continue;
        }
        if opacity_is_empty(global_opacity) {
            continue;
        }

        let Ok((
            shadow_opt,
            glow_opt,
            blur_opt,
            fill_progress_opt,
            clip_opt,
            world_bounds_opt,
            is_group_opt,
            tip_glow_opt,
        )) = query_effects.get(world, entity)
        else {
            continue;
        };

        if is_group_opt.is_some() {
            continue;
        }

        if let Some(ref bounds) = cam_bounds {
            let mut is_ancestor_culled = false;
            let mut current = entity;
            while let Ok(child_of) = child_query.get(world, current) {
                let parent = child_of.parent();
                if culled_entities.contains(&parent) {
                    is_ancestor_culled = true;
                    break;
                }
                current = parent;
            }

            if is_ancestor_culled {
                culled_entities.insert(entity);
                continue;
            }

            if let Some(w_bounds) = world_bounds_opt
                && !bounds.intersects(&w_bounds.0)
            {
                culled_entities.insert(entity);
                continue;
            }
        }

        let fill_alpha = fill_progress_opt
            .map(|f| f.0.clamp(0.0, 1.0))
            .unwrap_or(1.0);

        let mut scene = vello::Scene::new();
        if let Some(lottie) = lottie_opt {
            scene.append(lottie.scene(), None);
        }
        let empty_bez = kurbo::BezPath::new();
        let elem_path = if path_reveal_is_empty(tip_glow_opt) {
            &empty_bez
        } else {
            path_opt.map(|p| p.0.as_ref()).unwrap_or(&empty_bez)
        };
        let source_path = path_source_opt.map(|p| p.0.as_ref());
        let elem_fill = fill_opt.and_then(|f| f.0.as_ref());
        let elem_stroke = stroke_opt.and_then(|s| s.brush.as_ref());
        let elem_stroke_style = stroke_opt.map(|s| &s.style);
        let stroke_view = if elem_stroke.is_some() || glow_opt.is_some() {
            view_stroke_transform(entity, |ancestor| {
                Some((
                    *world.get::<gaanim_math::SpatialTransform>(ancestor)?,
                    world.get::<ChildOf>(ancestor).map(|parent| parent.parent()),
                    world
                        .get::<gaanim_scene::CoordinateViewRole>(ancestor)
                        .copied(),
                ))
            })
        } else {
            None
        };

        if let Some(shadow) = shadow_opt {
            let shadow_transform = kurbo::Affine::translate((shadow.offset.x, shadow.offset.y));
            let shadow_brush = peniko::Brush::Solid(shadow.color);
            if shadow.blur_radius > 0.0 {
                draw_soft_fill(
                    &mut scene,
                    elem_path,
                    &shadow_brush,
                    shadow.blur_radius,
                    1.0,
                    shadow_transform,
                );
            } else {
                scene.fill(
                    peniko::Fill::NonZero,
                    shadow_transform,
                    &shadow_brush,
                    None,
                    elem_path,
                );
            }
        }

        if let Some(glow) = glow_opt {
            draw_glow(&mut scene, elem_path, glow, stroke_view);
        }

        let is_trimmed_closed = source_path.is_some_and(|src| {
            src != elem_path && src.elements().contains(&kurbo::PathEl::ClosePath)
        });
        let blur_sigma = blur_opt
            .map(|blur| blur.sigma)
            .filter(|sigma| sigma.is_finite() && *sigma > 0.0);
        let blurred_vector = if let Some(sigma) = blur_sigma {
            if let Some(fill_brush) = elem_fill
                && !is_trimmed_closed
            {
                draw_soft_fill(
                    &mut scene,
                    elem_path,
                    fill_brush,
                    sigma,
                    fill_alpha,
                    kurbo::Affine::IDENTITY,
                );
            }
            if let (Some(stroke_brush), Some(style)) = (elem_stroke, elem_stroke_style) {
                draw_soft_stroke(
                    &mut scene,
                    elem_path,
                    stroke_brush,
                    style,
                    sigma,
                    stroke_view,
                );
            }
            if is_trimmed_closed {
                elem_stroke.is_some()
            } else {
                elem_fill.is_some() || elem_stroke.is_some()
            }
        } else {
            false
        };

        let completion_alpha = tip_glow_opt.map(|t| t.completion).unwrap_or(1.0);
        let anim_wave = if fill_alpha > 0.0 && fill_alpha < 1.0 {
            (fill_alpha as f64 * std::f64::consts::PI).sin()
        } else if completion_alpha > 0.0 && completion_alpha < 1.0 {
            (completion_alpha * std::f64::consts::PI).sin()
        } else {
            0.0
        };

        if !blurred_vector
            && let Some(raster_image) = raster_image_opt
            && let Some(image) = raster_image.image.as_ref()
        {
            scene.push_clip_layer(peniko::Fill::NonZero, kurbo::Affine::IDENTITY, elem_path);
            scene.draw_image(image.as_ref(), raster_image.local_transform);
            scene.pop_layer();
        } else if !blurred_vector && fill_alpha < 1.0 {
            if let Some(fill_brush) = elem_fill {
                if fill_alpha > 0.0 && !is_trimmed_closed {
                    // Push clip layer so ALL fill illumination is STRICTLY CLIPPED inside the character contour!
                    scene.push_clip_layer(
                        peniko::Fill::NonZero,
                        kurbo::Affine::IDENTITY,
                        elem_path,
                    );

                    // Keep the authored paint intact and reveal it only through
                    // alpha. Adding white illumination here made the fill flash
                    // as it entered instead of behaving like a true fade-in.
                    let modulated = modulate_brush_alpha(fill_brush, fill_alpha);
                    if let Some(ref brush) = modulated {
                        scene.fill(
                            peniko::Fill::NonZero,
                            kurbo::Affine::IDENTITY,
                            brush,
                            None,
                            elem_path,
                        );
                    }

                    scene.pop_layer();
                }
            }
        } else if !blurred_vector
            && let Some(fill_brush) = elem_fill
            && !is_trimmed_closed
        {
            scene.fill(
                peniko::Fill::NonZero,
                kurbo::Affine::IDENTITY,
                fill_brush,
                None,
                elem_path,
            );
        }

        if !blurred_vector
            && let Some(stroke_brush) = elem_stroke
            && let Some(style) = elem_stroke_style
        {
            let (effective_stroke_brush, effective_style) =
                animated_stroke_paint(stroke_brush, style, anim_wave);

            if let Some(clip_path) = stroke_clip_path(elem_path, source_path) {
                scene.push_layer(
                    peniko::Fill::NonZero,
                    peniko::BlendMode::default(),
                    1.0,
                    kurbo::Affine::IDENTITY,
                    clip_path,
                );
                draw_stroke(
                    &mut scene,
                    &effective_style,
                    kurbo::Affine::IDENTITY,
                    &effective_stroke_brush,
                    stroke_view,
                    elem_path,
                );
                scene.pop_layer();
            } else {
                draw_stroke(
                    &mut scene,
                    &effective_style,
                    kurbo::Affine::IDENTITY,
                    &effective_stroke_brush,
                    stroke_view,
                    elem_path,
                );
            }
        }

        let mut opacity_group = entity;
        while let Ok(child_of) = child_query.get(world, opacity_group) {
            opacity_group = child_of.parent();
        }
        extracted.push(ExtractedElement {
            transform: transform.affine_2d,
            opacity: global_opacity.0,
            opacity_bounds: opacity_layer_bounds(
                world_bounds_opt,
                opacity_fallback,
                shadow_opt,
                glow_opt,
                blur_opt,
            ),
            opacity_group,
            render_order: stacked_render_order(
                *render_order,
                entity,
                |e| child_query.get(world, e).ok().map(ChildOf::parent),
                |e| order_query.get(world, e).map_or(0, |order| order.z_index),
            ),
            scene: Arc::new(scene),
            clip_mask: clip_opt.cloned(),
            transition_side: transition_frame
                .as_ref()
                .map_or_else(Default::default, |frame| {
                    frame.side_of(entity, |e| {
                        child_query.get(world, e).ok().map(ChildOf::parent)
                    })
                }),
        });
    }

    extracted.sort_by(
        |a, b| match a.render_order.z_index.cmp(&b.render_order.z_index) {
            std::cmp::Ordering::Equal => a
                .render_order
                .creation_order
                .cmp(&b.render_order.creation_order),
            other => other,
        },
    );

    let mut main_scene = vello::Scene::new();

    // Draw canvas background as a filled rectangle at the frame bounds,
    // so the canvas area is visually distinct from the window background.
    // In perspective mode the 3D camera already clears to this color and the
    // Vello scene is rendered AFTER the 3D pass (order 1 vs 0) so that labels
    // appear on top of meshes. Skipping the opaque rect in that case prevents
    // the 2D background from occluding the 3D meshes behind it.
    let is_perspective = camera
        .is_some_and(|cam| matches!(cam.projection, gaanim_math::Projection::Perspective { .. }));
    if !is_perspective {
        if let Some(canvas_bg) = world.get_resource::<CanvasBackground>() {
            fill_canvas_background(
                &mut main_scene,
                canvas_bg,
                canvas_bg.pixel_size,
                transition_background_time(transition_frame.as_ref(), background_time),
                None,
            );
            fill_transition_background(
                &mut main_scene,
                canvas_bg,
                canvas_bg.pixel_size,
                transition_frame.as_ref(),
            );
        }
    }

    let composition_bounds = extracted.iter().fold(opacity_fallback, |bounds, elem| {
        bounds.union(elem.opacity_bounds)
    });
    append_extracted_elements(
        &mut main_scene,
        &extracted,
        composition_bounds,
        transition_frame.as_ref(),
    );

    main_scene
}

/// System: Extracts, composites, and renders all visible gaanim 2D Mobjects.
///
/// 1. Queries all active Mobjects marked for Vello2D rendering that are visible.
/// 2. Performs change detection to invalidate changed local vector fragments in the retained cache.
/// 3. Compiles/caches local geometry (including drop shadows, fills, and strokes).
/// 4. Sorts Mobjects deterministically based on Z-index + creation order to prevent visual jitter.
/// 5. Composites all fragments into a single main scene using global transforms and opacities.
/// 6. Assigns/updates the single global `VelloScene2d` entity marked with `MainVelloScene`.
///
/// # Coordinate System
/// gaanim uses a Y-up coordinate system (positive Y = up on screen).
/// Scene elements remain entirely in gaanim's Y-up world space. The single
/// global `MainVelloScene` entity has a negative Y scale that converts the
/// completed scene to Vello's Y-down pixel space without changing the meaning
/// of `.at(x, y)` or requiring per-object coordinate workarounds.
pub fn gaanim_render_system(
    mut commands: Commands,
    mut cache: ResMut<GaanimRenderCache>,
    gaanim_camera: Option<Res<gaanim_math::ResolvedCamera>>,
    playback_state: Option<Res<gaanim_animation::PlaybackState>>,
    canvas_bg: Option<Res<CanvasBackground>>,
    transition_frame: Option<Res<gaanim_scene::SceneTransitionFrame>>,
    child_query: Query<&ChildOf>,
    order_query: Query<&RenderOrder>,
    view_query: Query<(
        &gaanim_math::SpatialTransform,
        Option<&gaanim_scene::CoordinateViewRole>,
    )>,
    query_mobjects: Query<
        (
            Entity,
            Ref<MobjectId>,
            Ref<GlobalSpatialTransform>,
            Ref<GlobalOpacity>,
            Ref<RenderOrder>,
            Ref<RenderLayer>,
            Option<Ref<Path2D>>,
            Option<Ref<PathSource>>,
            Option<Ref<FillLevel>>,
            Option<Ref<FillBrush>>,
            Option<Ref<StrokeBrush>>,
            Option<Ref<RasterImage>>,
            Option<Ref<ReactiveReadout>>,
            Option<Ref<LottiePlayer>>,
            Option<&gaanim_scene::ObjectTag>,
        ),
        With<Visible>,
    >,
    query_effects: Query<(
        Option<Ref<DropShadow>>,
        Option<Ref<Glow>>,
        Option<Ref<GaussianBlur>>,
        Option<Ref<ClipMask>>,
        Option<Ref<FillDrawProgress>>,
        Option<&WorldBounds>,
        Option<&gaanim_scene::GroupMarker>,
        Option<Ref<WriteTipGlow>>,
        Option<Ref<Visible>>,
    )>,
    mut query_vello_scene: Query<(Entity, &mut VelloScene2d, &mut Transform), With<MainVelloScene>>,
    mut shader_frame: Option<ResMut<ShaderBackgroundFrame>>,
    mut local_extracted: Local<Vec<ExtractedElement>>,
    mut local_culled: Local<std::collections::HashSet<Entity>>,
) {
    local_extracted.clear();
    let mut scene_aabb_min = Vec3::splat(f32::INFINITY);
    let mut scene_aabb_max = Vec3::splat(f32::NEG_INFINITY);

    local_culled.clear();

    // 1. Calculate orthographic camera bounds for culling
    let cam_bounds = gaanim_camera.as_ref().and_then(|cam| {
        if let gaanim_math::Projection::Orthographic { zoom } = cam.projection {
            let effective = zoom * cam.viewport.scale;
            let hw = (cam.viewport_width as f64) / (2.0 * effective);
            let hh = (cam.viewport_height as f64) / (2.0 * effective);
            // Generous margin to avoid popping at boundaries
            let margin = 100.0 / effective.max(0.1);
            Some(gaanim_math::Bounds3D::new_2d(
                cam.position.x - hw - margin,
                cam.position.y - hh - margin,
                cam.position.x + hw + margin,
                cam.position.y + hh + margin,
            ))
        } else {
            None
        }
    });
    let opacity_fallback = canvas_bg
        .as_ref()
        .map(|background| {
            kurbo::Rect::new(
                background.bounds.min.x,
                background.bounds.min.y,
                background.bounds.max.x,
                background.bounds.max.y,
            )
        })
        .unwrap_or_else(|| kurbo::Rect::new(-4096.0, -4096.0, 4096.0, 4096.0));

    for (
        entity,
        mobj_id,
        transform,
        global_opacity,
        render_order,
        render_layer,
        path_ref,
        path_source_ref,
        fill_level_ref,
        fill_ref,
        stroke_ref,
        raster_image_ref,
        reactive_readout_ref,
        lottie_ref,
        _tag,
    ) in &query_mobjects
    {
        // Look up effects, bounds and group marker components on-demand to keep Query tuple size small
        let (
            shadow_ref,
            glow_ref,
            blur_ref,
            clip_ref,
            fill_progress_ref,
            world_bounds_opt,
            is_group_opt,
            tip_glow_ref,
            visible_ref,
        ) = query_effects
            .get(entity)
            .unwrap_or((None, None, None, None, None, None, None, None, None));

        // Invalidate before skipping hidden or culled objects. Their new geometry
        // may stop changing before they become visible again (e.g. a rewound Lottie).
        let stroke_view = if stroke_ref
            .as_ref()
            .is_some_and(|stroke| stroke.brush.is_some())
            || glow_ref.is_some()
        {
            view_stroke_transform(entity, |ancestor| {
                let (local, role) = view_query.get(ancestor).ok()?;
                Some((
                    *local,
                    child_query.get(ancestor).ok().map(|parent| parent.parent()),
                    role.copied(),
                ))
            })
        } else {
            None
        };
        let previous_view = match stroke_view {
            Some(view) => cache.stroke_views.insert(mobj_id.0, view),
            None => cache.stroke_views.remove(&mobj_id.0),
        };
        // Fill-level geometry is derived later in the frame. Track the source
        // value too, so a retained fragment can never outlive a rewind or a
        // segment replay that changes only this component.
        // This system only visits `Visible` entities, so the change ticks of
        // components rewritten while an entity was hidden (e.g. rewound
        // before its write/create clip) are already stale when it reappears.
        // Compare its inputs again whenever it becomes visible.
        let revealed = visible_ref.as_ref().is_some_and(|r| r.is_added());
        let timeline_inputs_touched = revealed
            || path_ref.as_ref().is_some_and(|r| r.is_changed())
            || path_source_ref.as_ref().is_some_and(|r| r.is_changed())
            || fill_level_ref.as_ref().is_some_and(|r| r.is_changed())
            || fill_ref.as_ref().is_some_and(|r| r.is_changed())
            || stroke_ref.as_ref().is_some_and(|r| r.is_changed())
            || fill_progress_ref.as_ref().is_some_and(|r| r.is_changed())
            || tip_glow_ref.as_ref().is_some_and(|r| r.is_changed());
        let current_inputs = timeline_inputs_touched.then(|| {
            FragmentInputs::capture(
                path_ref.as_deref(),
                path_source_ref.as_deref(),
                fill_level_ref.as_deref(),
                fill_ref.as_deref(),
                stroke_ref.as_deref(),
                fill_progress_ref.as_deref(),
                tip_glow_ref.as_deref(),
            )
        });
        let changed = current_inputs
            .as_ref()
            .is_some_and(|inputs| cache.fragment_inputs.get(&mobj_id.0) != Some(inputs))
            || stroke_view != previous_view
            || raster_image_ref.as_ref().is_some_and(|r| r.is_changed())
            || reactive_readout_ref
                .as_ref()
                .is_some_and(|r| r.is_changed())
            || lottie_ref.as_ref().is_some_and(|r| r.is_changed())
            || shadow_ref.as_ref().is_some_and(|r| r.is_changed())
            || glow_ref.as_ref().is_some_and(|r| r.is_changed())
            || blur_ref.as_ref().is_some_and(|r| r.is_changed())
            || clip_ref.as_ref().is_some_and(|r| r.is_changed());

        if changed {
            cache.fragment_cache.remove(&mobj_id.0);
        }

        // 2. Perform camera frustum culling and hierarchical culling propagation
        if let Some(bounds) = cam_bounds {
            // Check if any ancestor of this entity is already culled
            let mut is_ancestor_culled = false;
            let mut current = entity;
            while let Ok(child_of) = child_query.get(current) {
                let parent = child_of.parent();
                if local_culled.contains(&parent) {
                    is_ancestor_culled = true;
                    break;
                }
                current = parent;
            }

            if is_ancestor_culled {
                local_culled.insert(entity);
                continue;
            }

            // Check if this entity's own bounds are out of camera bounds
            if let Some(w_bounds) = world_bounds_opt
                && !bounds.intersects(&w_bounds.0)
            {
                local_culled.insert(entity);
                continue;
            }
        }

        // Only process visible Vello2D elements
        if *render_layer != RenderLayer::Vello2D {
            continue;
        }
        if opacity_is_empty(&global_opacity) {
            continue;
        }

        // Groups do not draw visual geometry directly, they only act as spatial nodes.
        if is_group_opt.is_some() {
            continue;
        }

        // Read the current fill progress. If the component is absent
        // (the common case for non-Writing entities) the fill is
        // rendered at full opacity, preserving legacy behavior.
        let fill_alpha = fill_progress_ref
            .as_ref()
            .map(|f| f.0.clamp(0.0, 1.0))
            .unwrap_or(1.0);
        let completion_alpha = tip_glow_ref.as_ref().map(|t| t.completion).unwrap_or(1.0);
        let anim_wave = if fill_alpha > 0.0 && fill_alpha < 1.0 {
            (fill_alpha as f64 * std::f64::consts::PI).sin()
        } else if completion_alpha > 0.0 && completion_alpha < 1.0 {
            (completion_alpha * std::f64::consts::PI).sin()
        } else {
            0.0
        };

        if !cache.fragment_cache.contains_key(&mobj_id.0) {
            let inputs = current_inputs.unwrap_or_else(|| {
                FragmentInputs::capture(
                    path_ref.as_deref(),
                    path_source_ref.as_deref(),
                    fill_level_ref.as_deref(),
                    fill_ref.as_deref(),
                    stroke_ref.as_deref(),
                    fill_progress_ref.as_deref(),
                    tip_glow_ref.as_deref(),
                )
            });
            cache.fragment_inputs.insert(mobj_id.0, inputs);
        }
        let fragment = cache.fragment_cache.entry(mobj_id.0).or_insert_with(|| {
            let mut scene = vello::Scene::new();

            if let Some(lottie) = lottie_ref.as_deref() {
                scene.append(lottie.scene(), None);
            }

            let empty_bez = kurbo::BezPath::new();
            let elem_path = if path_reveal_is_empty(tip_glow_ref.as_deref()) {
                &empty_bez
            } else {
                path_ref
                    .as_ref()
                    .map(|p| p.0.as_ref())
                    .unwrap_or(&empty_bez)
            };
            let source_path = path_source_ref.as_ref().map(|p| p.0.as_ref());
            let elem_fill = fill_ref.as_ref().and_then(|f| f.0.as_ref());
            let elem_stroke = stroke_ref.as_ref().and_then(|s| s.brush.as_ref());
            let elem_stroke_style = stroke_ref.as_ref().map(|s| &s.style);
            let elem_raster_image = raster_image_ref.as_deref();
            let elem_shadow = shadow_ref.as_deref();
            let elem_glow = glow_ref.as_deref();
            let elem_blur = blur_ref.as_deref();

            // 1. Draw Drop Shadow (rendered under the geometry with custom translation offset)
            if let Some(shadow) = elem_shadow {
                let shadow_transform = kurbo::Affine::translate((shadow.offset.x, shadow.offset.y));
                let shadow_brush = peniko::Brush::Solid(shadow.color);
                if shadow.blur_radius > 0.0 {
                    draw_soft_fill(
                        &mut scene,
                        elem_path,
                        &shadow_brush,
                        shadow.blur_radius,
                        1.0,
                        shadow_transform,
                    );
                } else {
                    scene.fill(
                        peniko::Fill::NonZero,
                        shadow_transform,
                        &shadow_brush,
                        None,
                        elem_path,
                    );
                }
            }

            if let Some(glow) = elem_glow {
                draw_glow(&mut scene, elem_path, glow, stroke_view);
            }

            let is_trimmed_closed = source_path.is_some_and(|src| {
                src != elem_path && src.elements().contains(&kurbo::PathEl::ClosePath)
            });
            let blur_sigma = elem_blur
                .map(|blur| blur.sigma)
                .filter(|sigma| sigma.is_finite() && *sigma > 0.0);
            let blurred_vector = if let Some(sigma) = blur_sigma {
                if let Some(fill_brush) = elem_fill
                    && !is_trimmed_closed
                {
                    draw_soft_fill(
                        &mut scene,
                        elem_path,
                        fill_brush,
                        sigma,
                        fill_alpha,
                        kurbo::Affine::IDENTITY,
                    );
                }
                if let (Some(stroke_brush), Some(style)) = (elem_stroke, elem_stroke_style) {
                    draw_soft_stroke(
                        &mut scene,
                        elem_path,
                        stroke_brush,
                        style,
                        sigma,
                        stroke_view,
                    );
                }
                if is_trimmed_closed {
                    elem_stroke.is_some()
                } else {
                    elem_fill.is_some() || elem_stroke.is_some()
                }
            } else {
                false
            };

            // 2. Draw Fill
            if !blurred_vector
                && let Some(raster_image) = elem_raster_image
                && let Some(image) = raster_image.image.as_ref()
            {
                scene.push_clip_layer(peniko::Fill::NonZero, kurbo::Affine::IDENTITY, elem_path);
                scene.draw_image(image.as_ref(), raster_image.local_transform);
                scene.pop_layer();
            } else if !blurred_vector && fill_alpha < 1.0 {
                if let Some(fill_brush) = elem_fill {
                    if fill_alpha > 0.0 && !is_trimmed_closed {
                        // Push clip layer so ALL fill illumination is STRICTLY CLIPPED inside the character contour!
                        scene.push_clip_layer(
                            peniko::Fill::NonZero,
                            kurbo::Affine::IDENTITY,
                            elem_path,
                        );

                        // Fade the authored paint directly; a temporary white
                        // illumination pass made the fill appear abruptly.
                        let modulated = modulate_brush_alpha(fill_brush, fill_alpha);
                        if let Some(ref brush) = modulated {
                            scene.fill(
                                peniko::Fill::NonZero,
                                kurbo::Affine::IDENTITY,
                                brush,
                                None,
                                elem_path,
                            );
                        }

                        scene.pop_layer();
                    }
                }
            } else if !blurred_vector
                && let Some(fill_brush) = elem_fill
                && !is_trimmed_closed
            {
                scene.fill(
                    peniko::Fill::NonZero,
                    kurbo::Affine::IDENTITY,
                    fill_brush,
                    None,
                    elem_path,
                );
            }

            // 3. Draw Stroke.
            if !blurred_vector
                && let Some(stroke_brush) = elem_stroke
                && let Some(style) = elem_stroke_style
            {
                let (effective_stroke_brush, effective_style) =
                    animated_stroke_paint(stroke_brush, style, anim_wave);

                if let Some(clip_path) = stroke_clip_path(elem_path, source_path) {
                    scene.push_layer(
                        peniko::Fill::NonZero,
                        peniko::BlendMode::default(),
                        1.0,
                        kurbo::Affine::IDENTITY,
                        clip_path,
                    );
                    draw_stroke(
                        &mut scene,
                        &effective_style,
                        kurbo::Affine::IDENTITY,
                        &effective_stroke_brush,
                        stroke_view,
                        elem_path,
                    );
                    scene.pop_layer();
                } else {
                    draw_stroke(
                        &mut scene,
                        &effective_style,
                        kurbo::Affine::IDENTITY,
                        &effective_stroke_brush,
                        stroke_view,
                        elem_path,
                    );
                }
            }

            Arc::new(scene)
        });

        let mut opacity_group = entity;
        while let Ok(child_of) = child_query.get(opacity_group) {
            opacity_group = child_of.parent();
        }
        local_extracted.push(ExtractedElement {
            transform: transform.affine_2d,
            opacity: global_opacity.0,
            opacity_bounds: opacity_layer_bounds(
                world_bounds_opt,
                opacity_fallback,
                shadow_ref.as_deref(),
                glow_ref.as_deref(),
                blur_ref.as_deref(),
            ),
            opacity_group,
            render_order: stacked_render_order(
                *render_order,
                entity,
                |e| child_query.get(e).ok().map(ChildOf::parent),
                |e| order_query.get(e).map_or(0, |order| order.z_index),
            ),
            scene: Arc::clone(fragment),
            clip_mask: clip_ref.as_ref().map(|c| (**c).clone()),
            transition_side: transition_frame
                .as_deref()
                .filter(|frame| !frame.is_empty())
                .map_or_else(Default::default, |frame| {
                    frame.side_of(entity, |e| child_query.get(e).ok().map(ChildOf::parent))
                }),
        });

        // Accumulate AABB from WorldBounds if available, otherwise approximate from transform
        if let Some(bounds) = world_bounds_opt {
            let min = Vec3::new(bounds.0.min.x as f32, bounds.0.min.y as f32, 0.0);
            let max = Vec3::new(bounds.0.max.x as f32, bounds.0.max.y as f32, 0.0);
            scene_aabb_min = scene_aabb_min.min(min);
            scene_aabb_max = scene_aabb_max.max(max);
        } else {
            // Fallback: use the translation component of the affine 2D transform as a point
            // estimate with a generous margin. The 4x4 mat4 field is only available under the
            // `dim3` feature; for the 2D-only path we extract tx/ty from the affine coefficients.
            let coeffs = transform.affine_2d.as_coeffs();
            let center = Vec3::new(coeffs[4] as f32, coeffs[5] as f32, 0.0);
            let margin = Vec3::new(500.0, 500.0, 1.0);
            scene_aabb_min = scene_aabb_min.min(center - margin);
            scene_aabb_max = scene_aabb_max.max(center + margin);
        }
    }

    // Sort elements deterministically by RenderOrder to ensure correct layering
    local_extracted.sort_by(
        |a, b| match a.render_order.z_index.cmp(&b.render_order.z_index) {
            std::cmp::Ordering::Equal => a
                .render_order
                .creation_order
                .cmp(&b.render_order.creation_order),
            other => other,
        },
    );

    // Assemble the global composited Scene in Bevy world coordinates
    let mut main_scene = vello::Scene::new();

    // Draw canvas background as a filled rectangle at the frame bounds,
    // so the canvas area is visually distinct from the window background.
    // Skip when perspective (see compile_scene_from_world comment).
    let is_perspective = gaanim_camera
        .as_ref()
        .is_some_and(|cam| matches!(cam.projection, gaanim_math::Projection::Perspective { .. }));
    let mut shader_request = None;
    if !is_perspective {
        if let Some(ref canvas_bg) = canvas_bg {
            let pixel_size = interactive_background_pixel_size(canvas_bg, gaanim_camera.as_deref());
            let time_seconds = playback_state
                .as_ref()
                .map_or(0.0, |state| state.current_time);
            fill_canvas_background(
                &mut main_scene,
                canvas_bg,
                pixel_size,
                transition_background_time(transition_frame.as_deref(), time_seconds),
                shader_frame.is_some().then_some(&mut shader_request),
            );
            fill_transition_background(
                &mut main_scene,
                canvas_bg,
                pixel_size,
                transition_frame.as_deref(),
            );
        }
    }

    if let Some(frame) = shader_frame.as_mut() {
        frame.0 = shader_request;
    }

    let composition_bounds = local_extracted
        .iter()
        .fold(opacity_fallback, |bounds, elem| {
            bounds.union(elem.opacity_bounds)
        });
    append_extracted_elements(
        &mut main_scene,
        local_extracted.as_slice(),
        composition_bounds,
        transition_frame
            .as_deref()
            .filter(|frame| !frame.is_empty()),
    );
    local_extracted.clear();

    // Compute a sensible AABB for the VelloScene2d entity.
    // If the scene is empty, use a default viewport-sized bounds to avoid zero-size culling.
    let (aabb_min, aabb_max) = if scene_aabb_min.x.is_finite() {
        (scene_aabb_min, scene_aabb_max)
    } else {
        let default = Vec3::new(640.0, 360.0, 0.0);
        (default * -1.0, default)
    };

    // Update the single global VelloScene2d or spawn it on demand
    let mut scene_entity_found = false;
    for (entity, mut scene, mut scene_transform) in &mut query_vello_scene {
        // Hand over the composited encoding instead of copying it again.
        **scene = Box::new(std::mem::take(&mut main_scene));
        scene_transform.scale = Vec3::new(1.0, -1.0, 1.0);
        scene_entity_found = true;

        // Update AABB to match current scene bounds
        commands
            .entity(entity)
            .insert(bevy::camera::primitives::Aabb::from_min_max(
                aabb_min, aabb_max,
            ));
    }

    if !scene_entity_found {
        commands.spawn((
            MainVelloScene,
            VelloScene2d::from(main_scene),
            bevy::camera::primitives::Aabb::from_min_max(aabb_min, aabb_max),
            Transform::from_scale(Vec3::new(1.0, -1.0, 1.0)),
            GlobalTransform::default(),
            Visibility::default(),
            InheritedVisibility::default(),
            ViewVisibility::default(),
        ));
    }
}

/// Returns a new `Brush` whose color alpha has been multiplied by `alpha`.
///
/// Used by the Write animation's cross-fade phase to fade the fill in
/// from `FillDrawProgress = 0.0` (invisible) to `1.0` (fully visible)
/// without changing the underlying brush. Delegates to `peniko::Brush`'s
/// built-in `multiply_alpha`, which handles `Solid`, `Gradient`, and
/// `Image` brush variants uniformly (with overflow saturation).
fn modulate_brush_alpha(brush: &peniko::Brush, alpha: f32) -> Option<peniko::Brush> {
    if alpha >= 1.0 {
        return None; // caller can use the original brush unmodified
    }
    let alpha = alpha.max(0.0);
    Some(brush.clone().multiply_alpha(alpha))
}

fn animated_stroke_paint(
    brush: &peniko::Brush,
    style: &kurbo::Stroke,
    wave: f64,
) -> (peniko::Brush, kurbo::Stroke) {
    let brush = if wave > 0.0 {
        let base_color = match brush {
            peniko::Brush::Solid(color) => *color,
            _ => peniko::Color::WHITE,
        };
        peniko::Brush::Solid(gaanim_core::interpolate_color(
            base_color,
            peniko::Color::WHITE,
            0.25 * wave,
        ))
    } else {
        brush.clone()
    };
    // Lighting is a paint effect. It must not alter a stroke's authored
    // logical geometry as the animation progresses.
    (brush, style.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect_path(x0: f64, y0: f64, x1: f64, y1: f64) -> Arc<kurbo::BezPath> {
        Arc::new(kurbo::Rect::new(x0, y0, x1, y1).to_path(0.1))
    }

    #[test]
    fn group_z_index_lifts_its_whole_subtree() {
        // Regression for #25: group([box, text]).z_index(5) must draw above a
        // root circle with z_index(2); glyphs inside a text with z 6 must stay
        // above a sibling box with z 5.
        let mut world = World::new();
        let order = |z_index, creation_order| RenderOrder {
            z_index,
            creation_order,
        };
        let group = world.spawn(order(5, 1)).id();
        let text = world.spawn((order(1, 3), ChildOf(group))).id();
        let glyph = world.spawn((order(0, 4), ChildOf(text))).id();
        let boxed = world.spawn((order(0, 2), ChildOf(group))).id();

        let stacked = |world: &World, entity: Entity| {
            stacked_render_order(
                *world.get::<RenderOrder>(entity).unwrap(),
                entity,
                |e| world.get::<ChildOf>(e).map(ChildOf::parent),
                |e| world.get::<RenderOrder>(e).map_or(0, |order| order.z_index),
            )
        };
        let circle = order(2, 5);
        assert_eq!(stacked(&world, boxed), order(5, 2));
        assert_eq!(stacked(&world, glyph), order(6, 4));
        assert!(stacked(&world, boxed).z_index > circle.z_index);
        assert!(stacked(&world, glyph).z_index > stacked(&world, boxed).z_index);
    }

    #[test]
    fn coordinate_view_keeps_the_stroke_pen_unscaled_in_headless_output() {
        let mut world = World::new();
        let view_local = gaanim_math::SpatialTransform::default().with_scale_2d(3.0, 1.0);
        let view = world
            .spawn((
                view_local,
                GlobalSpatialTransform::from_local(&view_local),
                gaanim_scene::CoordinateViewRole::View,
            ))
            .id();
        let path = kurbo::Line::new((2.0, 1.0), (2.0, 5.0)).to_path(0.01);
        world.spawn((
            MobjectId(ObjectId::from_raw(91)),
            gaanim_math::SpatialTransform::default(),
            GlobalSpatialTransform::from_local(&view_local),
            GlobalOpacity(1.0),
            RenderOrder::default(),
            RenderLayer::Vello2D,
            Path2D(Arc::new(path)),
            StrokeBrush::new(peniko::Color::BLACK, 0.03),
            Visible,
            ChildOf(view),
        ));
        let scene = compile_scene_from_world(&mut world, None);
        assert_eq!(
            scene.encoding().transforms[0].matrix,
            [1.0, 0.0, 0.0, 1.0],
            "the domain zoom must move the path without widening the stroke pen"
        );
    }

    #[test]
    fn fragments_changed_while_hidden_are_rebuilt_when_shown_again() {
        let mut app = App::new();
        app.init_resource::<GaanimRenderCache>()
            .add_systems(Update, gaanim_render_system);
        let id = ObjectId::from_raw(93);
        let full = kurbo::BezPath::from_svg("M 0 0 L 4 0 L 4 3").unwrap();
        let entity = app
            .world_mut()
            .spawn((
                MobjectId(id),
                GlobalSpatialTransform::default(),
                GlobalOpacity(1.0),
                RenderOrder::default(),
                RenderLayer::Vello2D,
                Path2D(Arc::new(full)),
                StrokeBrush {
                    brush: Some(peniko::Brush::Solid(peniko::Color::BLACK)),
                    style: kurbo::Stroke::new(0.1),
                },
                Visible,
            ))
            .id();
        app.update();
        let drawn = app.world().resource::<GaanimRenderCache>().fragment_cache[&id].clone();

        // A rewind hides the object and resets it to its undrawn state while
        // the render system does not visit it, as before a write/create clip.
        app.world_mut().entity_mut(entity).remove::<Visible>();
        app.update();
        app.world_mut()
            .entity_mut(entity)
            .insert(Path2D(Arc::new(kurbo::BezPath::new())));
        app.update();
        app.update();
        app.world_mut().entity_mut(entity).insert(Visible);
        app.update();

        let shown = &app.world().resource::<GaanimRenderCache>().fragment_cache[&id];
        assert!(!Arc::ptr_eq(&drawn, shown), "the drawn fragment was reused");
        let fresh = compile_scene_from_world(app.world_mut(), None);
        let live = app
            .world_mut()
            .query::<&VelloScene2d>()
            .single(app.world())
            .unwrap();
        assert_eq!(live.encoding().path_data, fresh.encoding().path_data);
    }

    #[test]
    fn coordinate_view_rebuilds_strokes_on_zoom_and_rewind_but_reuses_them_on_pan() {
        use gaanim_math::SpatialTransform;
        let mut app = App::new();
        app.init_resource::<GaanimRenderCache>().add_systems(
            Update,
            (
                gaanim_scene::transform_propagation_system,
                gaanim_render_system,
            )
                .chain(),
        );
        let view = app
            .world_mut()
            .spawn((
                SpatialTransform::default(),
                GlobalSpatialTransform::default(),
                gaanim_scene::CoordinateViewRole::View,
            ))
            .id();
        let id = ObjectId::from_raw(92);
        let path = kurbo::BezPath::from_svg("M 2 1 L 2 5 L 6 5 L 8 7 Q 9 9 6 10").unwrap();
        let style = kurbo::Stroke::new(0.04).with_dashes(0.1, [0.2, 0.1]);
        let brush = peniko::Brush::Solid(peniko::Color::BLACK);
        let entity = app
            .world_mut()
            .spawn((
                MobjectId(id),
                SpatialTransform::default(),
                GlobalSpatialTransform::default(),
                GlobalOpacity(1.0),
                RenderOrder::default(),
                RenderLayer::Vello2D,
                Path2D(Arc::new(path.clone())),
                StrokeBrush {
                    brush: Some(brush.clone()),
                    style: style.clone(),
                },
                Visible,
                ChildOf(view),
            ))
            .id();
        app.update();
        let original = app.world().resource::<GaanimRenderCache>().fragment_cache[&id].clone();
        for (sx, sy) in [(3.0, 1.0), (1.0, 2.0), (2.0, 2.0), (1.0, 1.0), (3.0, 1.0)] {
            app.world_mut()
                .entity_mut(view)
                .insert(SpatialTransform::default().with_scale_2d(sx, sy));
            app.update();
            let fragment = app.world().resource::<GaanimRenderCache>().fragment_cache[&id].clone();
            assert!(!Arc::ptr_eq(&original, &fragment));
            let expected_path = kurbo::Affine::scale_non_uniform(sx, sy) * &path;
            let mut expected = vello::Scene::new();
            expected.stroke(
                &style,
                kurbo::Affine::IDENTITY,
                &brush,
                None,
                &expected_path,
            );
            assert_eq!(
                fragment.encoding().path_data,
                expected.encoding().path_data,
                "dash lengths and curve geometry must be generated after domain zoom"
            );
            let headless = compile_scene_from_world(app.world_mut(), None);
            let live = app
                .world_mut()
                .query::<&VelloScene2d>()
                .single(app.world())
                .unwrap();
            assert_eq!(headless.encoding().path_data, live.encoding().path_data);
            assert_eq!(headless.encoding().transforms, live.encoding().transforms);
            assert_eq!(
                headless.encoding().transforms[0].matrix,
                [1.0, 0.0, 0.0, 1.0]
            );
            app.update();
            assert!(Arc::ptr_eq(
                &fragment,
                &app.world().resource::<GaanimRenderCache>().fragment_cache[&id]
            ));
            app.world_mut()
                .get_mut::<SpatialTransform>(view)
                .unwrap()
                .translation
                .x += 2.0;
            app.update();
            assert!(Arc::ptr_eq(
                &fragment,
                &app.world().resource::<GaanimRenderCache>().fragment_cache[&id]
            ));
        }
        app.world_mut().get_mut::<GlobalOpacity>(entity).unwrap().0 = 0.0;
        app.world_mut()
            .entity_mut(view)
            .insert(SpatialTransform::default());
        app.update();
        assert!(
            !app.world()
                .resource::<GaanimRenderCache>()
                .fragment_cache
                .contains_key(&id)
        );
        app.world_mut().get_mut::<GlobalOpacity>(entity).unwrap().0 = 1.0;
        app.update();
        let restored = &app.world().resource::<GaanimRenderCache>().fragment_cache[&id];
        assert_eq!(original.encoding().path_data, restored.encoding().path_data);
        assert_eq!(
            original.encoding().transforms,
            restored.encoding().transforms
        );
    }

    #[test]
    fn draw_animation_lighting_preserves_logical_stroke_width() {
        let brush = peniko::Brush::Solid(peniko::Color::from_rgb8(40, 100, 240));
        let style = kurbo::Stroke::new(0.03);

        for wave in [0.0, 0.25, 0.5, 1.0] {
            let (_, animated_style) = animated_stroke_paint(&brush, &style, wave);
            assert_eq!(animated_style.width, style.width);
        }
    }

    #[test]
    fn changing_fill_level_rebuilds_its_retained_fragment() {
        let mut app = App::new();
        app.init_resource::<GaanimRenderCache>()
            .add_systems(Update, gaanim_render_system);
        let id = ObjectId::from_raw(47);
        let entity = app
            .world_mut()
            .spawn((
                MobjectId(id),
                GlobalSpatialTransform::default(),
                GlobalOpacity(1.0),
                RenderOrder::default(),
                RenderLayer::Vello2D,
                Path2D(rect_path(0.0, 0.0, 20.0, 20.0)),
                PathSource(rect_path(0.0, 0.0, 20.0, 20.0)),
                FillBrush::color(peniko::Color::from_rgb8(251, 146, 60)),
                StrokeBrush::transparent(),
                FillLevel(0.0),
                Visible,
            ))
            .id();

        app.update();
        let first = app
            .world()
            .resource::<GaanimRenderCache>()
            .fragment_cache
            .get(&id)
            .expect("initial fragment")
            .clone();

        app.world_mut().get_mut::<FillLevel>(entity).unwrap().0 = 0.55;
        app.update();
        let second = app
            .world()
            .resource::<GaanimRenderCache>()
            .fragment_cache
            .get(&id)
            .expect("rebuilt fragment")
            .clone();

        assert!(
            !Arc::ptr_eq(&first, &second),
            "a retained fragment must not survive a fill-level change"
        );
    }

    #[test]
    fn seek_replay_of_identical_values_keeps_the_retained_fragment() {
        let mut app = App::new();
        app.init_resource::<GaanimRenderCache>()
            .add_systems(Update, gaanim_render_system);
        let id = ObjectId::from_raw(49);
        let entity = app
            .world_mut()
            .spawn((
                MobjectId(id),
                GlobalSpatialTransform::default(),
                GlobalOpacity(1.0),
                RenderOrder::default(),
                RenderLayer::Vello2D,
                Path2D(rect_path(0.0, 0.0, 20.0, 20.0)),
                PathSource(rect_path(0.0, 0.0, 20.0, 20.0)),
                FillBrush::color(peniko::Color::WHITE),
                StrokeBrush::transparent(),
                Visible,
            ))
            .id();
        app.update();
        let fragment =
            |app: &App| app.world().resource::<GaanimRenderCache>().fragment_cache[&id].clone();
        let retained = fragment(&app);

        // An exact seek restores the keyframe and replays completed clips,
        // rewriting equal values that Bevy still reports as changed.
        let mut entity_mut = app.world_mut().entity_mut(entity);
        entity_mut.get_mut::<Path2D>().unwrap().0 = rect_path(0.0, 0.0, 20.0, 20.0);
        entity_mut.get_mut::<PathSource>().unwrap().0 = rect_path(0.0, 0.0, 20.0, 20.0);
        entity_mut.insert(FillBrush::color(peniko::Color::WHITE));
        entity_mut.insert(StrokeBrush::transparent());
        app.update();
        assert!(Arc::ptr_eq(&retained, &fragment(&app)));

        app.world_mut().get_mut::<Path2D>(entity).unwrap().0 = rect_path(0.0, 0.0, 10.0, 20.0);
        app.update();
        assert!(!Arc::ptr_eq(&retained, &fragment(&app)));
    }

    #[test]
    fn hidden_geometry_changes_invalidate_fragment_before_fade_in() {
        let mut app = App::new();
        app.init_resource::<GaanimRenderCache>()
            .add_systems(Update, gaanim_render_system);
        let id = ObjectId::from_raw(48);
        let entity = app
            .world_mut()
            .spawn((
                MobjectId(id),
                GlobalSpatialTransform::default(),
                GlobalOpacity(1.0),
                RenderOrder::default(),
                RenderLayer::Vello2D,
                Path2D(rect_path(0.0, 0.0, 20.0, 20.0)),
                FillBrush::color(peniko::Color::WHITE),
                StrokeBrush::transparent(),
                Visible,
            ))
            .id();
        app.update();
        let last_frame = app.world().resource::<GaanimRenderCache>().fragment_cache[&id].clone();
        app.world_mut().get_mut::<GlobalOpacity>(entity).unwrap().0 = 0.0;
        app.world_mut().get_mut::<Path2D>(entity).unwrap().0 = rect_path(30.0, 0.0, 40.0, 10.0);
        app.update();
        assert!(
            !app.world()
                .resource::<GaanimRenderCache>()
                .fragment_cache
                .contains_key(&id)
        );
        // The geometry stays unchanged during the fade; only opacity changes.
        app.world_mut().get_mut::<GlobalOpacity>(entity).unwrap().0 = 0.5;
        app.update();
        let first_frame = &app.world().resource::<GaanimRenderCache>().fragment_cache[&id];
        assert_ne!(
            first_frame.encoding().path_data,
            last_frame.encoding().path_data
        );
    }

    #[test]
    fn live_boolean_system_runs_with_overlapping_path_queries() {
        let mut app = App::new();
        let left = app
            .world_mut()
            .spawn((
                Path2D(rect_path(0.0, 0.0, 10.0, 10.0)),
                GlobalSpatialTransform::default(),
            ))
            .id();
        let right = app
            .world_mut()
            .spawn((
                Path2D(rect_path(5.0, 0.0, 15.0, 10.0)),
                GlobalSpatialTransform::default(),
            ))
            .id();
        let result = app
            .world_mut()
            .spawn((
                BooleanBinding {
                    sources: vec![left, right],
                    op: gaanim_objects::boolean::BooleanOp::Intersection,
                    tolerance: 0.25,
                    rule: gaanim_objects::boolean::BooleanFillRule::NonZero,
                },
                GlobalSpatialTransform::default(),
                Path2D::default(),
                PathSource::default(),
                LocalBounds::default(),
            ))
            .id();
        app.add_systems(Update, resolve_dynamic_boolean_system);

        app.update();

        let bounds = app.world().get::<Path2D>(result).unwrap().0.bounding_box();
        assert_eq!(bounds, kurbo::Rect::new(5.0, 0.0, 10.0, 10.0));
    }

    #[test]
    fn fill_level_system_tracks_level_and_direction() {
        let mut app = App::new();
        let mask = app
            .world_mut()
            .spawn((
                Path2D(rect_path(0.0, 0.0, 100.0, 80.0)),
                GlobalSpatialTransform::default(),
            ))
            .id();
        let fill = app
            .world_mut()
            .spawn((
                FillLevelBinding {
                    sources: vec![mask],
                    direction: FillDirection::Up,
                },
                FillLevel(0.25),
                GlobalSpatialTransform::default(),
                Path2D::default(),
                PathSource::default(),
                LocalBounds::default(),
            ))
            .id();
        app.add_systems(Update, resolve_fill_level_system);

        app.update();
        let quarter = app.world().get::<Path2D>(fill).unwrap().0.bounding_box();
        assert_eq!(quarter, kurbo::Rect::new(0.0, 0.0, 100.0, 20.0));

        app.world_mut().get_mut::<FillLevel>(fill).unwrap().0 = 1.0;
        app.update();
        let full = app.world().get::<Path2D>(fill).unwrap().0.bounding_box();
        assert_eq!(full, kurbo::Rect::new(0.0, 0.0, 100.0, 80.0));
    }

    #[test]
    fn zero_path_reveal_is_rendered_as_empty_geometry() {
        let mut tip = WriteTipGlow::default();
        tip.completion = 0.0;
        assert!(path_reveal_is_empty(Some(&tip)));
        tip.completion = 0.001;
        assert!(!path_reveal_is_empty(Some(&tip)));
        assert!(!path_reveal_is_empty(None));
    }

    #[test]
    fn zero_global_opacity_is_not_extracted() {
        assert!(opacity_is_empty(&GlobalOpacity(0.0)));
        assert!(!opacity_is_empty(&GlobalOpacity(0.001)));
    }

    #[test]
    fn opacity_layers_use_local_finite_bounds_instead_of_a_full_scene_sentinel() {
        let bounds = WorldBounds(gaanim_math::Bounds3D::new_2d(-12.0, -8.0, 18.0, 14.0));
        let fallback = kurbo::Rect::new(-4096.0, -4096.0, 4096.0, 4096.0);
        let rect = opacity_layer_bounds(Some(&bounds), fallback, None, None, None);

        assert!(rect.x0 <= -12.0 && rect.y0 <= -8.0);
        assert!(rect.x1 >= 18.0 && rect.y1 >= 14.0);
        assert!(rect.width() < 64.0);
        assert!(rect.height() < 64.0);
    }

    #[test]
    fn consecutive_glyphs_with_the_same_opacity_share_one_compositor_run() {
        let element = |opacity| ExtractedElement {
            transform: kurbo::Affine::IDENTITY,
            opacity,
            opacity_bounds: kurbo::Rect::new(0.0, 0.0, 10.0, 10.0),
            opacity_group: Entity::PLACEHOLDER,
            render_order: RenderOrder::default(),
            scene: Arc::new(vello::Scene::new()),
            clip_mask: None,
            transition_side: Default::default(),
        };
        let elements = vec![element(0.5), element(0.5), element(0.5), element(0.75)];

        assert_eq!(opacity_run_end(&elements, 0), 3);
        assert_eq!(opacity_run_end(&elements, 3), 4);
    }

    #[test]
    fn elements_clipped_by_the_same_mask_share_one_clip_layer() {
        let mask = |source: u32, invert: bool| ClipMask {
            sources: vec![Entity::from_raw_u32(source).unwrap()],
            invert,
            ..ClipMask::default()
        };
        let element = |clip_mask| ExtractedElement {
            transform: kurbo::Affine::IDENTITY,
            opacity: 1.0,
            opacity_bounds: kurbo::Rect::new(0.0, 0.0, 10.0, 10.0),
            opacity_group: Entity::PLACEHOLDER,
            render_order: RenderOrder::default(),
            scene: Arc::new(vello::Scene::new()),
            clip_mask,
            transition_side: Default::default(),
        };
        let elements = vec![
            element(Some(mask(1, false))),
            element(Some(mask(1, false))),
            element(Some(mask(2, false))),
            element(Some(mask(2, true))),
            element(Some(mask(2, true))),
            element(None),
        ];

        assert_eq!(shared_clip_run_end(&elements, 0), 2);
        assert_eq!(shared_clip_run_end(&elements, 2), 3);
        // Inverted masks depend on each element's bounds and stay separate.
        assert_eq!(shared_clip_run_end(&elements, 3), 4);
        assert_eq!(shared_clip_run_end(&elements, 5), 6);
    }

    #[test]
    fn transition_masks_are_isolated_layers_that_clip_nested_blend_layers() {
        // vello's clip-only layer (`DrawBeginClip::CLIP_BLEND_MODE`) does not
        // clip nested blend layers, such as the stroke layer of a text glyph.
        const CLIP_ONLY: u32 = 0x8003;
        let rect = kurbo::Rect::new(0.0, 0.0, 1.0, 1.0);
        for fade in [
            None,
            Some((kurbo::Point::ZERO, kurbo::Point::new(1.0, 0.0))),
        ] {
            let mask = gaanim_scene::TransitionMask {
                path: rect.to_path(0.1),
                rule: peniko::Fill::NonZero,
                fade,
            };
            let mut scene = vello::Scene::new();
            push_transition_mask(&mut scene, &mask);
            pop_transition_mask(&mut scene, &mask);
            let draw_data = &scene.encoding().draw_data;
            assert_ne!(draw_data.first(), Some(&CLIP_ONLY), "fade: {fade:?}");
            assert!(!draw_data.contains(&CLIP_ONLY));
        }
    }

    #[test]
    fn shader_background_raster_size_tracks_the_interactive_viewport_scale() {
        let background = CanvasBackground {
            paint: BackgroundPaint::solid(peniko::Color::BLACK),
            segment_paints: Vec::new(),
            pixel_size: (1280, 720),
            bounds: gaanim_math::Bounds3D::new_2d(-640.0, -360.0, 640.0, 360.0),
        };
        let camera = gaanim_math::ResolvedCamera::new(
            gaanim_math::Camera::ortho_2d(1280, 720),
            gaanim_math::CameraViewport {
                scale: 0.5,
                offset_y: 0.0,
            },
        );

        assert_eq!(
            interactive_background_pixel_size(&background, Some(&camera)),
            (640, 360)
        );

        let high_zoom_camera = gaanim_math::ResolvedCamera::new(
            gaanim_math::Camera::ortho_2d(1280, 720),
            gaanim_math::CameraViewport {
                scale: 8.0,
                offset_y: 0.0,
            },
        );
        assert_eq!(
            interactive_background_pixel_size(&background, Some(&high_zoom_camera)),
            (8192, 4608)
        );
    }

    #[test]
    fn canvas_background_geometry_matches_the_authored_scene_bounds() {
        let background = CanvasBackground {
            paint: BackgroundPaint::solid(peniko::Color::BLACK),
            segment_paints: Vec::new(),
            pixel_size: (960, 540),
            bounds: gaanim_math::Bounds3D::new_2d(-480.0, -270.0, 480.0, 270.0),
        };
        let (rect, transform) = canvas_background_geometry(&background);

        assert_eq!(rect, kurbo::Rect::new(-480.0, -270.0, 480.0, 270.0));
        assert_eq!(transform, kurbo::Affine::IDENTITY);
    }

    #[test]
    fn segment_backgrounds_override_and_fall_back_at_exact_timeline_positions() {
        let default = peniko::Color::from_rgb8(10, 20, 30);
        let first = peniko::Color::from_rgb8(40, 50, 60);
        let third = peniko::Color::from_rgb8(70, 80, 90);
        let background = CanvasBackground {
            paint: BackgroundPaint::solid(default),
            segment_paints: vec![
                SegmentBackgroundPaint {
                    start_time: 0.0,
                    end_time: 1.0,
                    paint: Some(BackgroundPaint::solid(first)),
                    hold_at_end: true,
                },
                SegmentBackgroundPaint {
                    start_time: 1.0,
                    end_time: 2.0,
                    paint: None,
                    hold_at_end: false,
                },
                SegmentBackgroundPaint {
                    start_time: 2.0,
                    end_time: 3.0,
                    paint: Some(BackgroundPaint::solid(third)),
                    hold_at_end: false,
                },
            ],
            pixel_size: (960, 540),
            bounds: gaanim_math::Bounds3D::new_2d(-480.0, -270.0, 480.0, 270.0),
        };

        assert_eq!(background.paint_at(0.5).fallback_color(), first);
        assert_eq!(background.paint_at(1.0).fallback_color(), first);
        assert_eq!(background.paint_at(1.1).fallback_color(), default);
        assert_eq!(background.paint_at(2.0).fallback_color(), third);
    }

    #[test]
    fn shader_backgrounds_draw_the_render_device_texture_when_available() {
        let shader = crate::background::ShaderBackground::new(
            "fn gaanim_background(uv: vec2<f32>, resolution: vec2<f32>, time: f32) -> vec4<f32> {\n\
             return vec4<f32>(uv, time + resolution.x * 0.0, 1.0);\n}",
            peniko::Color::BLACK,
        )
        .unwrap();
        let solid = peniko::Color::from_rgb8(10, 20, 30);
        let background = CanvasBackground {
            paint: BackgroundPaint::Shader(shader),
            segment_paints: vec![SegmentBackgroundPaint {
                start_time: 2.0,
                end_time: 3.0,
                paint: Some(BackgroundPaint::solid(solid)),
                hold_at_end: false,
            }],
            pixel_size: (960, 540),
            bounds: gaanim_math::Bounds3D::new_2d(-480.0, -270.0, 480.0, 270.0),
        };
        let (rect, _) = canvas_background_geometry(&background);

        let mut request = None;
        let (brush, transform) =
            resolve_canvas_background_brush(&background, rect, (480, 270), 1.0, Some(&mut request));
        let request = request.expect("the shader frame is drawn on the render device");
        let peniko::Brush::Image(image) = brush else {
            panic!("expected the placeholder image brush, got {brush:?}");
        };
        assert_eq!(image.image.data.id(), request.image().data.id());
        assert_eq!((image.image.width, image.image.height), (480, 270));
        let transform = transform.unwrap();
        assert_eq!(
            transform * kurbo::Point::ZERO,
            kurbo::Point::new(rect.x0, rect.y1),
            "the first shader row is the top edge of the y-up world"
        );
        assert_eq!(
            transform * kurbo::Point::new(480.0, 270.0),
            kurbo::Point::new(rect.x1, rect.y0)
        );

        let mut request = None;
        let (brush, _) =
            resolve_canvas_background_brush(&background, rect, (480, 270), 2.5, Some(&mut request));
        assert!(request.is_none(), "solid segments need no shader frame");
        assert!(matches!(brush, peniko::Brush::Solid(color) if color == solid));
    }

    #[test]
    fn shader_background_uv_starts_at_the_top_left_of_the_displayed_canvas() {
        let Some(mut gpu) = crate::background::test_gpu::test_gpu() else {
            eprintln!("skipped: no GPU adapter");
            return;
        };
        let shader = crate::background::ShaderBackground::new(
            "fn gaanim_background(uv: vec2<f32>, resolution: vec2<f32>, time: f32) -> vec4<f32> {\n\
             return vec4<f32>(uv, time * 0.0 + resolution.x * 0.0, 1.0);\n}",
            peniko::Color::BLACK,
        )
        .unwrap();
        let (width, height) = (64, 36);
        let background = CanvasBackground {
            paint: BackgroundPaint::Shader(shader),
            segment_paints: Vec::new(),
            pixel_size: (width, height),
            bounds: gaanim_math::Bounds3D::new_2d(-8.0, -4.5, 8.0, 4.5),
        };
        // The editor and exports both display world +y upwards.
        let world_to_screen = kurbo::Affine::new([4.0, 0.0, 0.0, -4.0, 32.0, 18.0]);
        let target = gpu.target(width, height);
        let mut backgrounds = crate::background::GpuShaderBackgrounds::default();
        for resident in [false, true] {
            let mut request = None;
            let mut world = vello::Scene::new();
            fill_canvas_background(
                &mut world,
                &background,
                (width, height),
                0.0,
                resident.then_some(&mut request),
            );
            if resident {
                backgrounds.prepare(&gpu.device, &gpu.queue, &mut gpu.renderer, request.as_ref());
            }
            let mut screen = vello::Scene::new();
            screen.append(&world, Some(world_to_screen));
            gpu.render(&screen, &target);
            let pixels = gpu.read(&target);
            // Red and green carry uv at each pixel centre.
            let uv = |x: u32, y: u32| {
                let index = ((y * width + x) * 4) as usize;
                (pixels[index], pixels[index + 1])
            };
            let corners = [
                uv(0, 0),
                uv(width - 1, 0),
                uv(0, height - 1),
                uv(width - 1, height - 1),
            ];
            let low = |value: u8| value < 8;
            let high = |value: u8| value > 247;
            assert!(
                low(corners[0].0) && low(corners[0].1),
                "top-left is uv (0, 0): {corners:?}, resident: {resident}"
            );
            assert!(
                high(corners[1].0) && low(corners[1].1),
                "top-right: {corners:?}"
            );
            assert!(
                low(corners[2].0) && high(corners[2].1),
                "bottom-left: {corners:?}"
            );
            assert!(
                high(corners[3].0) && high(corners[3].1),
                "bottom-right: {corners:?}"
            );
        }
    }

    fn window(width: u32, height: u32) -> Window {
        Window {
            resolution: (width, height).into(),
            ..default()
        }
    }

    #[test]
    fn perspective_viewport_keeps_canvas_size_in_taller_window() {
        let cam = gaanim_math::Camera::perspective_3d(1280, 720, 0.7);
        let viewport = gaanim_math::CameraViewport::default();

        let original = fitted_canvas_viewport(&cam, viewport, &window(1280, 720)).unwrap();
        let taller = fitted_canvas_viewport(&cam, viewport, &window(1280, 1000)).unwrap();

        assert_eq!(original.physical_size, UVec2::new(1280, 720));
        assert_eq!(taller.physical_size, original.physical_size);
        assert_eq!(taller.physical_position, UVec2::new(0, 140));
    }

    #[test]
    fn perspective_viewport_scales_with_the_fitted_logical_canvas() {
        let cam = gaanim_math::Camera::perspective_3d(1280, 720, 0.7);
        let viewport = gaanim_math::CameraViewport {
            scale: 0.5,
            offset_y: 0.0,
        };

        let viewport = fitted_canvas_viewport(&cam, viewport, &window(640, 500)).unwrap();

        assert_eq!(viewport.physical_size, UVec2::new(640, 360));
        assert_eq!(viewport.physical_position, UVec2::new(0, 70));
    }

    #[test]
    fn perspective_viewport_offset_is_measured_in_window_pixels() {
        let cam = gaanim_math::Camera::perspective_3d(1280, 720, 0.8);
        let mapping = gaanim_math::CameraViewport {
            scale: 0.5,
            offset_y: -100.0,
        };
        let viewport = fitted_canvas_viewport(&cam, mapping, &window(1280, 720)).unwrap();

        assert_eq!(viewport.physical_size, UVec2::new(640, 360));
        assert_eq!(viewport.physical_position, UVec2::new(320, 80));
    }

    #[test]
    fn presentation_camera_uses_rig_camera_instead_of_editor_override() {
        let mut authored = gaanim_math::Camera::perspective_3d(1280, 720, 0.8);
        authored.position = gaanim_core::glam::DVec3::new(1.0, 2.0, 3.0);
        let mut rig = authored;
        rig.position = gaanim_core::glam::DVec3::new(4.0, 5.0, 6.0);
        let mut resolved = authored.clone();
        resolved.position = gaanim_core::glam::DVec3::new(9.0, 8.0, 7.0);

        let mut app = App::new();
        app.insert_resource(authored)
            .insert_resource(gaanim_math::CameraRigCamera(rig))
            .insert_resource(gaanim_math::ResolvedCamera::new(
                resolved,
                gaanim_math::CameraViewport::default(),
            ))
            .add_systems(Update, sync_gaanim_camera_to_bevy_3d_system);
        let presentation = app
            .world_mut()
            .spawn((Camera3d::default(), gaanim_scene::AuthoritativeCameraView))
            .id();
        let inspection = app.world_mut().spawn(Camera3d::default()).id();

        app.update();

        assert_eq!(
            app.world()
                .entity(presentation)
                .get::<Transform>()
                .unwrap()
                .translation,
            Vec3::new(4.0, 5.0, 6.0)
        );
        assert_eq!(
            app.world()
                .entity(inspection)
                .get::<Transform>()
                .unwrap()
                .translation,
            Vec3::new(9.0, 8.0, 7.0)
        );
    }

    #[test]
    fn pbr_camera_switches_back_to_orthographic_and_releases_perspective_viewport() {
        let camera = gaanim_math::Camera::ortho_2d(960, 540);
        let mut app = App::new();
        app.insert_resource(camera)
            .insert_resource(gaanim_math::ResolvedCamera::new(
                camera,
                gaanim_math::CameraViewport::default(),
            ))
            .add_systems(Update, sync_gaanim_camera_to_bevy_3d_system);
        let entity = app.world_mut().spawn(Camera3d::default()).id();
        app.world_mut()
            .entity_mut(entity)
            .get_mut::<Camera>()
            .unwrap()
            .viewport = Some(bevy::camera::Viewport {
            physical_position: UVec2::new(30, 0),
            physical_size: UVec2::new(900, 540),
            depth: 0.0..1.0,
        });

        app.update();

        assert!(matches!(
            app.world().entity(entity).get::<Projection>(),
            Some(Projection::Orthographic(_))
        ));
        assert!(
            app.world()
                .entity(entity)
                .get::<Camera>()
                .unwrap()
                .viewport
                .is_none(),
            "the fitted perspective viewport must not survive a reset to 2D"
        );
    }
}
